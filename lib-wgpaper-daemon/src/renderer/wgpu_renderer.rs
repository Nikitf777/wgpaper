use super::{Renderer, texture::Texture};
use crate::{
	image_wrapper::ImageWrapper,
	renderer::{
		GpuSelector,
		wgpu_selector::{WgpuSelector, select_gpu},
	},
	transition::TransitionProgress,
};
use anyhow::Context;
use pollster::FutureExt;
use raw_window_handle::{
	RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use std::ptr::NonNull;
use wayland_client::{Connection, Proxy};
use wgpaper_config::ScalingStrategy;
use wgpu::{Buffer, SurfaceError};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TransitionProgressUniforms {
	progress: f32,
	progress_clamped: f32,
	_padding: [u8; 248],
}

impl TransitionProgressUniforms {
	fn new(v1: f32, v2: f32) -> Self {
		Self {
			progress: v1,
			progress_clamped: v2,
			_padding: [0u8; 248],
		}
	}

	fn to(&self) -> TransitionProgress {
		TransitionProgress {
			progress: self.progress,
			progress_clamped: self.progress_clamped,
		}
	}
}

impl From<TransitionProgress> for TransitionProgressUniforms {
	fn from(progress: TransitionProgress) -> Self {
		Self::new(progress.progress, progress.progress_clamped)
	}
}

pub struct WgpuRenderer {
	device: wgpu::Device,
	queue: wgpu::Queue,
	surface: wgpu::Surface<'static>,
	config: wgpu::SurfaceConfiguration,
	sampler: wgpu::Sampler,

	scaled_texture: Texture,
	scaling_bind_group: wgpu::BindGroup,
	scaling_pipeline: wgpu::RenderPipeline,

	offscreen_textures: Vec<Texture>,
	to_texture: Texture,
	display_texture_idx: usize,
	render_texture_idx: usize,
	animation_texture_bind_group_layout: wgpu::BindGroupLayout,
	animation_texture_bind_group: wgpu::BindGroup,
	animation_pipeline: wgpu::RenderPipeline,

	transition_progress: TransitionProgressUniforms,
	transition_progress_bind_group: wgpu::BindGroup,
	transition_progress_uniform_buffer: Buffer,
}

impl WgpuRenderer {
	fn create_scaling_fragment_shader(
		device: &wgpu::Device,
		strategy: ScalingStrategy,
	) -> wgpu::ShaderModule {
		device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some(&format!("scaling_fragment_shader_{}", strategy)),
			source: wgpu::ShaderSource::Wgsl(include_str!("fragment.wgsl").into()),
		})
	}
}

impl Renderer for WgpuRenderer {
	fn new(
		conn: &Connection,
		layer_surface: &LayerSurface,
		width: u32,
		height: u32,
		gpu_selector: GpuSelector,
		animation_shader: &str,
		initial_image: &ImageWrapper,
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

		let adapter =
			pollster::block_on(select_gpu(&instance, WgpuSelector::from(gpu_selector))).unwrap_or(
				pollster::block_on(select_gpu(&instance, WgpuSelector::default()))?,
			);

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

		let initial_texture = Texture::from_rgba8_with_format(
			&device,
			&queue,
			initial_image.width(),
			initial_image.height(),
			initial_image.as_slice(),
			"to_texture",
			surface_format,
		)?;

		let data = vec![0u8; (width * height * 4) as usize]; // Data for initial placeholder textures (black rectangle)

		// Scaling Pipeline
		let scaled_texture = Texture::from_rgba8_with_format(
			&device,
			&queue,
			width,
			height,
			&data,
			"scaling_texture",
			surface_format,
		)
		.unwrap();

		let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
			label: Some("animation_sampler"),
			address_mode_u: wgpu::AddressMode::ClampToEdge,
			address_mode_v: wgpu::AddressMode::ClampToEdge,
			address_mode_w: wgpu::AddressMode::ClampToEdge,
			mag_filter: wgpu::FilterMode::Linear,
			min_filter: wgpu::FilterMode::Linear,
			mipmap_filter: wgpu::MipmapFilterMode::Nearest,
			..Default::default()
		});

		let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some("vertex_shader"),
			source: wgpu::ShaderSource::Wgsl(include_str!("vertex.wgsl").into()),
		});

		let scaling_bind_group_layout =
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
				label: Some("scaling_bind_group_layout"),
			});

		let scaling_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &scaling_bind_group_layout,
			entries: &[
				wgpu::BindGroupEntry {
					binding: 0,
					resource: wgpu::BindingResource::TextureView(&initial_texture.view),
				},
				wgpu::BindGroupEntry {
					binding: 1,
					resource: wgpu::BindingResource::Sampler(&sampler),
				},
			],
			label: Some("scaling_bind_group"),
		});
		let scaling_fragment_shader =
			WgpuRenderer::create_scaling_fragment_shader(&device, ScalingStrategy::default());

		let scaling_pipeline_layout =
			device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("scaling_pipeline_layout"),
				bind_group_layouts: &[&scaling_bind_group_layout],
				immediate_size: 0,
			});

		let scaling_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: Some("scaling_pipeline"),
			layout: Some(&scaling_pipeline_layout),
			vertex: wgpu::VertexState {
				module: &vertex_shader,
				entry_point: Some("vs_main"),
				buffers: &[],
				compilation_options: Default::default(),
			},
			fragment: Some(wgpu::FragmentState {
				module: &scaling_fragment_shader,
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

		// Animation Pipeline
		let offscreen_textures = (0..3)
			.map(|i| {
				Texture::from_rgba8_with_format(
					&device,
					&queue,
					width,
					height,
					&data,
					&format!("offscreen_texture_{}", i),
					surface_format,
				)
				.unwrap()
			})
			.collect::<Vec<_>>();
		let display_texture_idx: usize = 0;
		let render_texture_idx: usize = 1;

		let animation_texture_bind_group_layout =
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
						ty: wgpu::BindingType::Texture {
							multisampled: false,
							sample_type: wgpu::TextureSampleType::Float { filterable: true },
							view_dimension: wgpu::TextureViewDimension::D2,
						},
						count: None,
					},
					wgpu::BindGroupLayoutEntry {
						binding: 2,
						visibility: wgpu::ShaderStages::FRAGMENT,
						ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
						count: None,
					},
				],
				label: Some("animation_texture_bind_group_layout"),
			});

		let animation_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &animation_texture_bind_group_layout,
			entries: &[
				wgpu::BindGroupEntry {
					binding: 0,
					resource: wgpu::BindingResource::TextureView(&scaled_texture.view),
				},
				wgpu::BindGroupEntry {
					binding: 1,
					resource: wgpu::BindingResource::TextureView(
						&offscreen_textures[display_texture_idx].view,
					),
				},
				wgpu::BindGroupEntry {
					binding: 2,
					resource: wgpu::BindingResource::Sampler(&sampler),
				},
			],
			label: Some("animation_texture_bind_group"),
		});

		let transition_progress_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
			label: Some("transition_progress_uniform_buffer"),
			size: std::mem::size_of::<TransitionProgressUniforms>() as wgpu::BufferAddress,
			usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let transition_progress_bind_group_layout =
			device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
				entries: &[wgpu::BindGroupLayoutEntry {
					binding: 0,
					visibility: wgpu::ShaderStages::FRAGMENT,
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

		let animation_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some("animation_shader"),
			source: wgpu::ShaderSource::Wgsl(animation_shader.into()),
		});

		let animation_pipeline_layout =
			device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("animation_pipeline_layout"),
				bind_group_layouts: &[
					&animation_texture_bind_group_layout,
					&transition_progress_bind_group_layout,
				],
				immediate_size: 0,
			});

		let animation_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: Some("animation_pipeline"),
			layout: Some(&animation_pipeline_layout),
			vertex: wgpu::VertexState {
				module: &vertex_shader,
				entry_point: Some("vs_main"),
				buffers: &[],
				compilation_options: Default::default(),
			},
			fragment: Some(wgpu::FragmentState {
				module: &animation_shader,
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
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
			format: surface_format,
			width,
			height,
			present_mode: surface_caps.present_modes[0],
			alpha_mode: surface_caps.alpha_modes[0],
			view_formats: vec![],
			desired_maximum_frame_latency: 2,
		};
		surface.configure(&device, &config);

		let transition_progress = TransitionProgressUniforms::from(TransitionProgress::finished());

		Ok(Self {
			device,
			queue,
			surface,
			config,

			sampler,
			scaled_texture,
			scaling_bind_group,
			scaling_pipeline,

			offscreen_textures,
			to_texture: initial_texture,
			display_texture_idx,
			render_texture_idx,
			animation_texture_bind_group_layout,
			animation_texture_bind_group,
			animation_pipeline,

			transition_progress,
			transition_progress_bind_group,
			transition_progress_uniform_buffer,
		})
	}

	fn render(&mut self) -> anyhow::Result<()> {
		let surface_texture = match self.surface.get_current_texture() {
			Ok(frame) => frame,
			Err(SurfaceError::Outdated | SurfaceError::Lost) => {
				self.surface.configure(&self.device, &self.config);
				return Ok(());
			}
			Err(e) => return Err(anyhow::anyhow!("Failed to acquire next texture: {}", e)),
		};

		let mut encoder = self
			.device
			.create_command_encoder(&wgpu::CommandEncoderDescriptor {
				label: Some("encoder"),
			});

		{
			let mut scaling_render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
				label: Some("scaling_render_pass"),
				color_attachments: &[Some(wgpu::RenderPassColorAttachment {
					view: &self.scaled_texture.view,
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

			scaling_render_pass.set_pipeline(&self.scaling_pipeline);
			scaling_render_pass.set_bind_group(0, &self.scaling_bind_group, &[]);
			scaling_render_pass.draw(0..3, 0..1);
		}

		{
			let mut animation_render_pass =
				encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
					label: Some("animation_render_pass"),
					color_attachments: &[Some(wgpu::RenderPassColorAttachment {
						view: &self.offscreen_textures[self.render_texture_idx].view,
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

			animation_render_pass.set_pipeline(&self.animation_pipeline);
			animation_render_pass.set_bind_group(0, &self.animation_texture_bind_group, &[]);
			animation_render_pass.set_bind_group(1, &self.transition_progress_bind_group, &[]);
			animation_render_pass.draw(0..3, 0..1);
		}

		encoder.copy_texture_to_texture(
			wgpu::TexelCopyTextureInfo {
				texture: &self.offscreen_textures[self.render_texture_idx].texture,
				mip_level: 0,
				origin: wgpu::Origin3d::ZERO,
				aspect: wgpu::TextureAspect::All,
			},
			wgpu::TexelCopyTextureInfo {
				texture: &surface_texture.texture,
				mip_level: 0,
				origin: wgpu::Origin3d::ZERO,
				aspect: wgpu::TextureAspect::All,
			},
			self.offscreen_textures[self.render_texture_idx]
				.texture
				.size(),
		);

		self.queue.submit(Some(encoder.finish()));

		surface_texture.present();
		Ok(())
	}

	fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
		self.config.width = width;
		self.config.height = height;
		self.surface.configure(&self.device, &self.config);
		Ok(())
	}

	fn get_transition_progress(&self) -> TransitionProgress {
		self.transition_progress.to()
	}

	fn set_transition_progress(&mut self, progress: TransitionProgress) {
		let progress = TransitionProgressUniforms::from(progress);
		self.transition_progress = progress;
		self.queue.write_buffer(
			&self.transition_progress_uniform_buffer,
			0,
			bytemuck::bytes_of(&progress),
		);
	}

	fn set_next_image(&mut self, rgba8: &[u8], dimensions: (u32, u32)) {
		self.to_texture = Texture::from_rgba8_with_format(
			&self.device,
			&self.queue,
			dimensions.0,
			dimensions.1,
			rgba8,
			"to_texture",
			self.config.format,
		)
		.unwrap();

		self.display_texture_idx = self.render_texture_idx;
		self.render_texture_idx = (self.display_texture_idx + 1) % 3;

		self.animation_texture_bind_group =
			self.device.create_bind_group(&wgpu::BindGroupDescriptor {
				layout: &self.animation_texture_bind_group_layout,
				entries: &[
					wgpu::BindGroupEntry {
						binding: 0,
						resource: wgpu::BindingResource::TextureView(
							&self.offscreen_textures[self.display_texture_idx].view,
						),
					},
					wgpu::BindGroupEntry {
						binding: 1,
						resource: wgpu::BindingResource::TextureView(&self.to_texture.view),
					},
					wgpu::BindGroupEntry {
						binding: 2,
						resource: wgpu::BindingResource::Sampler(&self.sampler),
					},
				],
				label: Some("texture_bind_group"),
			});
	}
}
