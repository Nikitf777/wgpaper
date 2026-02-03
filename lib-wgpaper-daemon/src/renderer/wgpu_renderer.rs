use super::{Renderer, texture::Texture};
use crate::{
	image_wrapper::ImageWrapper,
	renderer::{
		GpuSelector,
		lerp::Lerp,
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
use wgpu::{Buffer, Queue, SurfaceError};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScalingDataUniforms {
	screen_size: [f32; 2],
	texture_size: [f32; 2],
	screen_aspect: f32,
	texture_aspect: f32,
	_padding: [u8; 232],
}

impl ScalingDataUniforms {
	fn new(screen_size: (f32, f32), texture_size: (f32, f32)) -> Self {
		Self {
			screen_size: unsafe { std::mem::transmute(screen_size) },
			texture_size: unsafe { std::mem::transmute(texture_size) },
			screen_aspect: screen_size.0 / screen_size.1,
			texture_aspect: texture_size.0 / texture_size.1,
			_padding: [0u8; 232],
		}
	}

	fn texture_size(&self) -> (f32, f32) {
		unsafe { std::mem::transmute(self.texture_size) }
	}

	fn update_screen_size(&mut self, new_size: (f32, f32)) {
		self.screen_size = unsafe { std::mem::transmute(new_size) };
		self.screen_aspect = new_size.0 / new_size.1;
	}

	fn update_texture_size(&mut self, new_size: (f32, f32)) {
		self.texture_size = unsafe { std::mem::transmute(new_size) };
		self.texture_aspect = new_size.0 / new_size.1;
	}
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TransitionProgressUniforms {
	progress: f32,
	progress_clamped: f32,
	_padding: [u8; 248],
}

impl TransitionProgressUniforms {
	fn new(progress: f32, progress_clamped: f32) -> Self {
		Self {
			progress,
			progress_clamped,
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

	offscreen_textures: Vec<Texture>,
	to_texture: Texture,
	display_texture_idx: usize,
	render_texture_idx: usize,
	animation_texture_bind_group_layout: wgpu::BindGroupLayout,
	animation_texture_bind_group: wgpu::BindGroup,
	animation_pipeline: wgpu::RenderPipeline,

	texture_before_scaling: Texture,
	scaling_texture_bind_group_layout: wgpu::BindGroupLayout,
	scaling_texture_bind_group: wgpu::BindGroup,
	scaling_data: ScalingDataUniforms,
	scaling_data_uniform_buffer: Buffer,
	scaling_data_bind_group: wgpu::BindGroup,
	prev_image_size: (f32, f32),
	scaling_pipeline: wgpu::RenderPipeline,

	transition_progress: TransitionProgressUniforms,
	transition_progress_bind_group: wgpu::BindGroup,
	transition_progress_uniform_buffer: Buffer,
}

impl WgpuRenderer {
	const STRETCH_SHADER: &str = include_str!("fragment_stretch.wgsl");
	const FIT_SHADER: &str = include_str!("fragment_fit.wgsl");
	const COVER_SHADER: &str = include_str!("fragment_cover.wgsl");
	const CENTER_SHADER: &str = include_str!("fragment_center.wgsl");

	fn write_scaling_data(data: &ScalingDataUniforms, queue: &Queue, buffer: &Buffer) {
		queue.write_buffer(&buffer, 0, bytemuck::bytes_of(data));
	}
}

impl WgpuRenderer {
	fn create_scaling_fragment_shader(
		device: &wgpu::Device,
		strategy: ScalingStrategy,
	) -> wgpu::ShaderModule {
		let shader = match strategy {
			ScalingStrategy::Stretch => Self::STRETCH_SHADER,
			ScalingStrategy::Fit => Self::FIT_SHADER,
			ScalingStrategy::Center => Self::CENTER_SHADER,
			ScalingStrategy::Cover => Self::COVER_SHADER,
		};

		device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some(&format!("scaling_fragment_shader_{}", strategy)),
			source: wgpu::ShaderSource::Wgsl(shader.into()),
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

		let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
			label: Some("sampler"),
			address_mode_u: wgpu::AddressMode::Repeat,
			address_mode_v: wgpu::AddressMode::Repeat,
			mag_filter: wgpu::FilterMode::Linear,
			min_filter: wgpu::FilterMode::Linear,
			mipmap_filter: wgpu::MipmapFilterMode::Nearest,
			..Default::default()
		});

		let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some("vertex_shader"),
			source: wgpu::ShaderSource::Wgsl(include_str!("vertex.wgsl").into()),
		});

		let scaling_data_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
			label: Some("scaling_data_uniform_buffer"),
			size: std::mem::size_of::<ScalingDataUniforms>() as wgpu::BufferAddress,
			usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let scaling_data = ScalingDataUniforms::new(
			(width as f32, height as f32),
			(initial_image.width() as f32, initial_image.height() as f32),
		);
		WgpuRenderer::write_scaling_data(&scaling_data, &queue, &scaling_data_uniform_buffer);

		let scaling_data_bind_group_layout =
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
				label: Some("scaling_data_bind_group_layout"),
			});

		let scaling_data_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &scaling_data_bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding: 0,
				resource: scaling_data_uniform_buffer.as_entire_binding(),
			}],
			label: Some("scaling_data_bind_group"),
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
					resource: wgpu::BindingResource::TextureView(&initial_texture.view),
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
				label: Some("transition_progress_bind_group_layout"),
			});

		let transition_progress_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &transition_progress_bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding: 0,
				resource: transition_progress_uniform_buffer.as_entire_binding(),
			}],
			label: Some("transition_progress_bind_group"),
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
					&scaling_data_bind_group_layout,
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

		let scaling_texture_bind_group_layout =
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
				label: Some("scaling_texture_bind_group_layout"),
			});

		let scaling_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
			layout: &scaling_texture_bind_group_layout,
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
			label: Some("scaling_texture_bind_group"),
		});

		let scaling_fragment_shader =
			WgpuRenderer::create_scaling_fragment_shader(&device, ScalingStrategy::default());

		let scaling_pipeline_layout =
			device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("scaling_pipeline_layout"),
				bind_group_layouts: &[
					&scaling_texture_bind_group_layout,
					&scaling_data_bind_group_layout,
				],
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

		let image_size_f32 = (initial_image.width() as f32, initial_image.height() as f32);

		Ok(Self {
			device,
			queue,
			surface,
			config,

			offscreen_textures,
			to_texture: initial_texture,
			display_texture_idx,
			render_texture_idx,
			animation_texture_bind_group_layout,
			animation_texture_bind_group,
			animation_pipeline,

			sampler,
			texture_before_scaling: scaled_texture,
			scaling_texture_bind_group_layout,
			scaling_texture_bind_group,
			scaling_data,
			scaling_data_uniform_buffer,
			scaling_data_bind_group,
			prev_image_size: image_size_f32,
			scaling_pipeline,

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
		let surface_view = surface_texture
			.texture
			.create_view(&wgpu::TextureViewDescriptor::default());

		let mut encoder = self
			.device
			.create_command_encoder(&wgpu::CommandEncoderDescriptor {
				label: Some("encoder"),
			});

		{
			let mut animation_render_pass =
				encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
					label: Some("animation_render_pass"),
					color_attachments: &[Some(wgpu::RenderPassColorAttachment {
						view: &self.texture_before_scaling.view,
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
			animation_render_pass.set_bind_group(1, &self.scaling_data_bind_group, &[]);
			animation_render_pass.set_bind_group(2, &self.transition_progress_bind_group, &[]);
			animation_render_pass.draw(0..3, 0..1);
		}

		encoder.copy_texture_to_texture(
			wgpu::TexelCopyTextureInfo {
				texture: &self.texture_before_scaling.texture,
				mip_level: 0,
				origin: wgpu::Origin3d::ZERO,
				aspect: wgpu::TextureAspect::All,
			},
			wgpu::TexelCopyTextureInfo {
				texture: &self.offscreen_textures[self.render_texture_idx].texture,
				mip_level: 0,
				origin: wgpu::Origin3d::ZERO,
				aspect: wgpu::TextureAspect::All,
			},
			self.offscreen_textures[self.render_texture_idx]
				.texture
				.size(),
		);

		{
			let mut scaling_render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
				label: Some("scaling_render_pass"),
				color_attachments: &[Some(wgpu::RenderPassColorAttachment {
					view: &surface_view,
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
			scaling_render_pass.set_bind_group(0, &self.scaling_texture_bind_group, &[]);
			scaling_render_pass.set_bind_group(1, &self.scaling_data_bind_group, &[]);
			scaling_render_pass.draw(0..3, 0..1);
		}

		self.queue.submit(Some(encoder.finish()));

		surface_texture.present();
		Ok(())
	}

	fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()> {
		self.config.width = width;
		self.config.height = height;
		self.surface.configure(&self.device, &self.config);
		self.scaling_data
			.update_screen_size((width as f32, height as f32));
		Ok(())
	}

	fn get_transition_progress(&self) -> TransitionProgress {
		self.transition_progress.to()
	}

	fn set_transition_progress(&mut self, progress: TransitionProgress) {
		self.scaling_data
			.update_texture_size(self.prev_image_size.lerp(
				(
					self.to_texture.texture.width() as f32,
					self.to_texture.texture.height() as f32,
				),
				progress.progress,
			));
		WgpuRenderer::write_scaling_data(
			&self.scaling_data,
			&self.queue,
			&self.scaling_data_uniform_buffer,
		);
		let progress = TransitionProgressUniforms::from(progress);
		self.transition_progress = progress;
		self.queue.write_buffer(
			&self.transition_progress_uniform_buffer,
			0,
			bytemuck::bytes_of(&progress),
		);
	}

	fn set_next_image(&mut self, image: &ImageWrapper) {
		self.prev_image_size = self.scaling_data.texture_size();

		self.to_texture = Texture::from_rgba8_with_format(
			&self.device,
			&self.queue,
			image.width(),
			image.height(),
			image.as_slice(),
			"to_texture",
			self.config.format,
		)
		.unwrap();

		self.display_texture_idx = self.render_texture_idx;
		self.render_texture_idx = (self.display_texture_idx + 1) % 3;

		self.scaling_texture_bind_group =
			self.device.create_bind_group(&wgpu::BindGroupDescriptor {
				layout: &self.scaling_texture_bind_group_layout,
				entries: &[
					wgpu::BindGroupEntry {
						binding: 0,
						resource: wgpu::BindingResource::TextureView(
							&self.texture_before_scaling.view,
						),
					},
					wgpu::BindGroupEntry {
						binding: 1,
						resource: wgpu::BindingResource::Sampler(&self.sampler),
					},
				],
				label: Some("scaling_texture_bind_group"),
			});

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
				label: Some("animation_texture_bind_group"),
			});
	}
}
