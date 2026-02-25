use super::{wgpu_selector, wgpu_texture};
use crate::{
	image_wrapper::ImageWrapper,
	renderer::{
		self, Renderer, RendererOptions,
		wgpu::{
			wgpu_shaders,
			wgpu_texture_scaler::WgpuTextureScaler,
			wgpu_uniforms::PerFrameUniformManager,
			wgpu_utilities::{self, create_surface},
		},
	},
	transition::TransitionProgress,
};
use anyhow::Context;
use pollster::FutureExt;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_client::Connection;
use wgpu::{CommandEncoder, SurfaceError};

pub struct WgpuRenderer {
	device: wgpu::Device,
	queue: wgpu::Queue,
	surface: wgpu::Surface<'static>,
	config: wgpu::SurfaceConfiguration,
	sampler: wgpu::Sampler,

	texture_scaler: WgpuTextureScaler,
	scaled_texture: wgpu_texture::WgpuTexture,

	offscreen_textures: [wgpu_texture::WgpuTexture; 3],
	target_texture: wgpu_texture::WgpuTexture,
	display_texture_idx: usize,
	render_texture_idx: usize,
	animation_texture_bind_group_layout: wgpu::BindGroupLayout,
	animation_texture_bind_group: wgpu::BindGroup,
	animation_pipeline: wgpu::RenderPipeline,

	per_frame_uniform_manager: PerFrameUniformManager,
}

impl WgpuRenderer {
	pub(super) fn render_pass<'tex>(
		&self,
		render_pass: &mut wgpu::RenderPass<'tex>,
		pipeline: &wgpu::RenderPipeline,
		texture_bind_group: &wgpu::BindGroup,
	) {
		wgpu_utilities::render_pass(
			render_pass,
			pipeline,
			texture_bind_group,
			self.per_frame_uniform_manager.bind_group(),
		);
	}

	fn render_animation(&self, encoder: &mut CommandEncoder) {
		let mut animation_render_pass = wgpu_utilities::begin_render_pass(
			encoder,
			wgpu_utilities::create_color_attachment(
				&self.offscreen_textures[self.render_texture_idx].view,
			),
			&"animation_render_pass",
		);
		self.render_pass(
			&mut animation_render_pass,
			&self.animation_pipeline,
			&self.animation_texture_bind_group,
		);
	}
}

impl Renderer for WgpuRenderer {
	fn new(
		conn: &Connection,
		layer_surface: &LayerSurface,
		size: (u32, u32),
		options: &RendererOptions,
	) -> anyhow::Result<Self> {
		let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
			backends: wgpu::Backends::PRIMARY,
			..Default::default()
		});

		let surface = create_surface(&instance, conn, layer_surface)?;

		let gpu_selector = renderer::GpuSelector::from(options.gpu_selector.clone());
		let adapter = pollster::block_on(wgpu_selector::select_gpu(
			&instance,
			wgpu_selector::WgpuSelector::from(gpu_selector),
		))
		.unwrap_or(pollster::block_on(wgpu_selector::select_gpu(
			&instance,
			wgpu_selector::WgpuSelector::default(),
		))?);

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

		let placeholder_rgba8 = vec![0u8; (size.0 * size.1 * 4) as usize]; // Fully black rectangle
		let placeholder_image = ImageWrapper::from_rgba8(placeholder_rgba8, size);
		let initial_image = options.initial_image.unwrap_or(&placeholder_image);

		let initial_texture = wgpu_texture::WgpuTexture::from_image(
			&device,
			&queue,
			initial_image,
			"to_texture",
			surface_format,
		)?;

		let (address_mode, bg_color) =
			wgpu_utilities::get_address_mode_and_bg_color(options.scaling_mode);

		let sampler = wgpu_utilities::create_sampler(&device, address_mode);

		let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some("vertex_shader"),
			source: wgpu::ShaderSource::Wgsl(include_str!("shaders/vertex.wgsl").into()),
		});

		let (mut per_frame_uniform_manager, per_frame_data_bind_group_layout) =
			PerFrameUniformManager::new(
				&device,
				(size.0 as f32, size.1 as f32),
				(initial_image.width() as f32, initial_image.height() as f32),
				bg_color,
			);
		per_frame_uniform_manager.write_data(&queue);
		per_frame_uniform_manager.update_transition_progress(TransitionProgress::finished()); // Mark that there's no ongoing transition

		let scaled_texture = wgpu_texture::WgpuTexture::from_image(
			&device,
			&queue,
			&placeholder_image,
			"scaling_texture",
			surface_format,
		)
		.unwrap();

		let texture_scaler = WgpuTextureScaler::new(
			&device,
			&per_frame_data_bind_group_layout,
			&vertex_shader,
			options.scaling_mode.clone(),
			surface_format,
		);
		texture_scaler.scale(
			&device,
			&queue,
			&sampler,
			&initial_texture.view,
			&scaled_texture.view,
			per_frame_uniform_manager.bind_group(),
		);

		let offscreen_textures: [wgpu_texture::WgpuTexture; 3] = core::array::from_fn(|i| {
			wgpu_texture::WgpuTexture::from_image(
				&device,
				&queue,
				&placeholder_image,
				&format!("offscreen_texture_{}", i),
				surface_format,
			)
			.unwrap()
		});
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

		let animation_shader =
			wgpu_shaders::create_animation_shader(&device, options.shader_source);

		let animation_pipeline_layout =
			device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
				label: Some("animation_pipeline_layout"),
				bind_group_layouts: &[
					&animation_texture_bind_group_layout,
					&per_frame_data_bind_group_layout,
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
			width: size.0,
			height: size.1,
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

			texture_scaler,

			offscreen_textures,
			target_texture: initial_texture,
			display_texture_idx,
			render_texture_idx,
			animation_texture_bind_group_layout,
			animation_texture_bind_group,
			animation_pipeline,

			sampler,
			scaled_texture,
			per_frame_uniform_manager,
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

		let mut encoder = wgpu_utilities::create_command_encoder(&self.device, "encoder");

		self.render_animation(&mut encoder);

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

	fn resize(&mut self, size: (u32, u32)) -> anyhow::Result<()> {
		self.config.width = size.0;
		self.config.height = size.1;
		self.surface.configure(&self.device, &self.config);
		self.per_frame_uniform_manager
			.update_screen_size((size.0 as f32, size.1 as f32));
		Ok(())
	}

	fn get_transition_progress(&self) -> TransitionProgress {
		self.per_frame_uniform_manager.transition_progress()
	}

	fn set_transition_progress(&mut self, progress: TransitionProgress) {
		self.per_frame_uniform_manager
			.update_transition_progress(progress);
		self.per_frame_uniform_manager.write_data(&self.queue);
	}

	fn set_next_image(&mut self, image: &ImageWrapper) {
		self.target_texture = wgpu_texture::WgpuTexture::from_image(
			&self.device,
			&self.queue,
			&image,
			"to_texture",
			self.config.format,
		)
		.unwrap();

		self.per_frame_uniform_manager.update_texture_size((
			self.target_texture.texture.width() as f32,
			self.target_texture.texture.height() as f32,
		));

		self.display_texture_idx = self.render_texture_idx;
		self.render_texture_idx = (self.display_texture_idx + 1) % 3;

		self.texture_scaler.scale(
			&self.device,
			&self.queue,
			&self.sampler,
			&self.target_texture.view,
			&self.scaled_texture.view,
			self.per_frame_uniform_manager.bind_group(),
		);

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
						resource: wgpu::BindingResource::TextureView(&self.scaled_texture.view),
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
