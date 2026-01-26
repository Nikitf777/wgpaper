use super::Renderer;
use crate::texture::Texture;
use anyhow::Context;
use pollster::FutureExt;
use raw_window_handle::{
	RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use std::ptr::NonNull;
use wayland_client::{Connection, Proxy};
use wgpu::{Buffer, SurfaceError};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TransitionProgressUniforms {
	transition_progress: f32,
	transition_progress_clamped: f32,
	_padding: [u8; 248],
}

impl TransitionProgressUniforms {
	fn new(v1: f32, v2: f32) -> Self {
		Self {
			transition_progress: v1,
			transition_progress_clamped: v2,
			_padding: [0u8; 248],
		}
	}
}

pub struct WgpuRenderer {
	device: wgpu::Device,
	queue: wgpu::Queue,
	surface: wgpu::Surface<'static>,
	config: wgpu::SurfaceConfiguration,
	render_pipeline: wgpu::RenderPipeline,
	diffuse_bind_group: wgpu::BindGroup,
	_diffuse_texture: Texture,
	transition_progress_bind_group: wgpu::BindGroup,
	transition_progress_uniform_buffer: Buffer,
}

impl Renderer for WgpuRenderer {
	fn new(
		conn: &Connection,
		layer_surface: &LayerSurface,
		width: u32,
		height: u32,
	) -> anyhow::Result<Self> {
		let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
			backends: wgpu::Backends::PRIMARY,
			..Default::default()
		});

		let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
			NonNull::new(conn.backend().display_ptr() as *mut _).unwrap(),
		));
		let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
			NonNull::new(layer_surface.wl_surface().id().as_ptr() as *mut _).unwrap(),
		));

		let surface = unsafe {
			instance
				.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
					raw_display_handle,
					raw_window_handle,
				})
				.context("Failed to create surface")?
		};

		let adapter = instance
			.request_adapter(&wgpu::RequestAdapterOptions {
				compatible_surface: Some(&surface),
				..Default::default()
			})
			.block_on()
			.context("Failed to request adapter")?;

		let (device, queue) = adapter
			.request_device(&wgpu::DeviceDescriptor::default())
			.block_on()
			.context("Failed to request device")?;

		let surface_caps = surface.get_capabilities(&adapter);
		let surface_format = surface_caps
			.formats
			.iter()
			.copied()
			.find(|f| f.is_srgb())
			.unwrap_or(surface_caps.formats[0]);

		let diffuse_bytes = include_bytes!("happy-tree.png");
		let diffuse_texture =
			Texture::from_bytes(&device, &queue, diffuse_bytes, "happy-tree.png")?;

		let texture_bind_group_layout =
			device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
				entries: &[
					wgpu::BindGroupLayoutEntry {
						binding: 0,
						visibility: wgpu::ShaderStages::FRAGMENT,
						ty: wgpu::BindingType::Texture {
							multisampled: false,
							sample_type: wgpu::TextureSampleType::Float { filterable: true },
							view_dimension: wgpu::TextureViewDimension::D2,
						},
						count: None,
					},
					wgpu::BindGroupLayoutEntry {
						binding: 1,
						visibility: wgpu::ShaderStages::FRAGMENT,
						ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
						count: None,
					},
				],
				label: Some("texture_bind_group_layout"),
			});

		let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &texture_bind_group_layout,
			entries: &[
				wgpu::BindGroupEntry {
					binding: 0,
					resource: wgpu::BindingResource::TextureView(&diffuse_texture.view),
				},
				wgpu::BindGroupEntry {
					binding: 1,
					resource: wgpu::BindingResource::Sampler(&diffuse_texture.sampler),
				},
			],
			label: Some("diffuse_bind_group"),
		});

		let transition_progress_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
			label: Some("Frame Uniforms"),
			size: std::mem::size_of::<TransitionProgressUniforms>() as wgpu::BufferAddress,
			usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let transition_progress_bind_group_layout =
			device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
				entries: &[wgpu::BindGroupLayoutEntry {
					binding: 0,
					visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
					ty: wgpu::BindingType::Buffer {
						ty: wgpu::BufferBindingType::Uniform,
						has_dynamic_offset: false,
						min_binding_size: wgpu::BufferSize::new(256),
					},
					count: None,
				}],
				label: None,
			});

		let transition_progress_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &transition_progress_bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding: 0,
				resource: transition_progress_uniform_buffer.as_entire_binding(),
			}],
			label: None,
		});

		let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some("Shader"),
			source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
		});

		let render_pipeline_layout =
			device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("Render Pipeline Layout"),
				bind_group_layouts: &[&texture_bind_group_layout],
				immediate_size: 0,
			});

		let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: Some("Render Pipeline"),
			layout: Some(&render_pipeline_layout),
			vertex: wgpu::VertexState {
				module: &shader,
				entry_point: Some("vs_main"),
				buffers: &[],
				compilation_options: Default::default(),
			},
			fragment: Some(wgpu::FragmentState {
				module: &shader,
				entry_point: Some("fs_main"),
				targets: &[Some(wgpu::ColorTargetState {
					format: surface_format,
					blend: Some(wgpu::BlendState::REPLACE),
					write_mask: wgpu::ColorWrites::ALL,
				})],
				compilation_options: Default::default(),
			}),
			primitive: wgpu::PrimitiveState::default(),
			depth_stencil: None,
			multisample: wgpu::MultisampleState::default(),
			multiview_mask: None,
			cache: None,
		});

		let config = wgpu::SurfaceConfiguration {
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
			format: surface_format,
			width,
			height,
			present_mode: surface_caps.present_modes[0],
			alpha_mode: surface_caps.alpha_modes[0],
			view_formats: vec![],
			desired_maximum_frame_latency: 2,
		};
		surface.configure(&device, &config);

		Ok(Self {
			device,
			queue,
			surface,
			config,
			render_pipeline,
			diffuse_bind_group,
			_diffuse_texture: diffuse_texture,
			transition_progress_bind_group,
			transition_progress_uniform_buffer,
		})
	}

	fn render(
		&mut self,
		transition_progress: f32,
		transition_progress_clamped: f32,
	) -> anyhow::Result<()> {
		let frame = match self.surface.get_current_texture() {
			Ok(frame) => frame,
			Err(SurfaceError::Outdated | SurfaceError::Lost) => {
				self.surface.configure(&self.device, &self.config);
				return Ok(());
			}
			Err(e) => return Err(anyhow::anyhow!("Failed to acquire next texture: {}", e)),
		};

		let view = frame
			.texture
			.create_view(&wgpu::TextureViewDescriptor::default());

		let mut encoder = self
			.device
			.create_command_encoder(&wgpu::CommandEncoderDescriptor {
				label: Some("Render Encoder"),
			});

		let uniforms =
			TransitionProgressUniforms::new(transition_progress, transition_progress_clamped);
		self.queue.write_buffer(
			&self.transition_progress_uniform_buffer,
			0,
			bytemuck::bytes_of(&uniforms),
		);

		{
			let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
				label: Some("Render Pass"),
				color_attachments: &[Some(wgpu::RenderPassColorAttachment {
					view: &view,
					resolve_target: None,
					ops: wgpu::Operations {
						load: wgpu::LoadOp::Clear(wgpu::Color {
							r: 0.1,
							g: 0.2,
							b: 0.3,
							a: 1.0,
						}),
						store: wgpu::StoreOp::Store,
					},
					depth_slice: None,
				})],
				depth_stencil_attachment: None,
				timestamp_writes: None,
				occlusion_query_set: None,
				multiview_mask: None,
			});

			render_pass.set_pipeline(&self.render_pipeline);
			render_pass.set_bind_group(0, &self.diffuse_bind_group, &[]);
			render_pass.set_bind_group(1, &self.transition_progress_bind_group, &[]);
			render_pass.draw(0..3, 0..1);
		}

		self.queue.submit(Some(encoder.finish()));
		frame.present();
		Ok(())
	}

	fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
		self.config.width = width;
		self.config.height = height;
		self.surface.configure(&self.device, &self.config);
		Ok(())
	}
}
