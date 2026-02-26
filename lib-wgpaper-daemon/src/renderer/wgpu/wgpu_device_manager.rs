use crate::{
	image_wrapper::ImageWrapper,
	renderer::{
		self, RendererOptions,
		wgpu::{
			wgpu_selector::{self, WgpuSelector},
			wgpu_texture::{self, WgpuTexture},
			wgpu_texture_scaler::WgpuTextureScaler,
			wgpu_transition_renderer::WgpuTransitionRenderer,
			wgpu_uniforms::PerFrameUniformManager,
			wgpu_utilities::{self, create_surface},
		},
	},
};
use anyhow::Context;
use log::warn;
use pollster::FutureExt;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use std::{collections::HashMap, rc::Rc};
use wayland_client::Connection;
use wgpaper_config::{Background, ScalingMode};
use wgpu::{
	Adapter, BindGroup, CommandEncoder, Device, Instance, Origin3d, Queue, Sampler, ShaderModule,
	Surface, SurfaceCapabilities, SurfaceConfiguration, SurfaceError, TexelCopyTextureInfo,
	TextureAspect, TextureFormat, TextureView,
};

fn choose_sampler<'a>(
	mode: &ScalingMode,
	repeat_sampler: &'a Sampler,
	mirror_repeat_sampler: &'a Sampler,
) -> &'a Sampler {
	match mode {
		ScalingMode::Fit { background } | ScalingMode::Center { background } => match background {
			Background::MirrorRepeat => mirror_repeat_sampler,
			_ => repeat_sampler,
		},
		_ => repeat_sampler,
	}
}

#[derive(Eq, Hash, PartialEq, Clone, Copy)]
enum ScalingModeFlat {
	Stretch,
	Fit,
	Cover,
	Center,
}

impl From<&ScalingMode> for ScalingModeFlat {
	fn from(value: &ScalingMode) -> Self {
		match value {
			ScalingMode::Stretch => ScalingModeFlat::Stretch,
			ScalingMode::Fit { background: _ } => ScalingModeFlat::Fit,
			ScalingMode::Cover => ScalingModeFlat::Cover,
			ScalingMode::Center { background: _ } => ScalingModeFlat::Center,
		}
	}
}

pub struct WgpuDeviceHandler {
	manager: Rc<RenderingManager>,

	adapter: Adapter,
	device: Device,
	queue: Queue,
	surface_caps: SurfaceCapabilities,
	surface_format: TextureFormat,

	vertex_shader: ShaderModule,
	texture_scalers: HashMap<ScalingModeFlat, WgpuTextureScaler>,
	scaled_textures_and_renderers: HashMap<ScalingMode, HashMap<(u32, u32), WgpuTexture>>,
	repeat_sampler: Sampler,
	mirror_repeat_sampler: Sampler,
}

impl WgpuDeviceHandler {
	pub fn new(
		manager: Rc<RenderingManager>,
		conn: &Connection,
		layer_surface: &LayerSurface,
		options: &RendererOptions,
	) -> anyhow::Result<Self> {
		let gpu_selector = renderer::GpuSelector::from(options.gpu_selector.clone());
		let adapter = pollster::block_on(wgpu_selector::select_gpu(
			manager.instance(),
			wgpu_selector::WgpuSelector::from(gpu_selector),
		))
		.unwrap_or(pollster::block_on(wgpu_selector::select_gpu(
			manager.instance(),
			wgpu_selector::WgpuSelector::default(),
		))?);

		let (device, queue) = adapter
			.request_device(&wgpu::DeviceDescriptor::default())
			.block_on()
			.context("Failed to request device")?;

		let surface = create_surface(manager.instance(), conn, layer_surface)?;
		let surface_caps = surface.get_capabilities(&adapter);
		let surface_format = surface_caps
			.formats
			.iter()
			.copied()
			.find(|f| f.is_srgb())
			.unwrap_or(surface_caps.formats[0]);

		let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some("vertex_shader"),
			source: wgpu::ShaderSource::Wgsl(include_str!("shaders/vertex.wgsl").into()),
		});

		let texture_scalers = HashMap::<ScalingModeFlat, WgpuTextureScaler>::new();
		let scaled_textures_and_renderers =
			HashMap::<ScalingMode, HashMap<(u32, u32), WgpuTexture>>::new();

		let repeat_sampler = wgpu_utilities::create_sampler(&device, wgpu::AddressMode::Repeat);
		let mirror_repeat_sampler =
			wgpu_utilities::create_sampler(&device, wgpu::AddressMode::MirrorRepeat);

		Ok(Self {
			manager,

			adapter,
			device,
			queue,
			surface_caps,
			surface_format,

			vertex_shader,
			texture_scalers,
			scaled_textures_and_renderers,
			repeat_sampler,
			mirror_repeat_sampler,
		})
	}

	pub fn surface_format(&self) -> &TextureFormat {
		&self.surface_format
	}

	pub fn create_surface(
		&self,
		conn: &Connection,
		layer_surface: &LayerSurface,
	) -> anyhow::Result<Surface> {
		create_surface(&self.manager.instance(), conn, layer_surface)
	}

	pub fn create_surface_config(&self, size: (u32, u32)) -> SurfaceConfiguration {
		wgpu::SurfaceConfiguration {
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
			format: self.surface_format,
			width: size.0,
			height: size.1,
			present_mode: self.surface_caps.present_modes[0],
			alpha_mode: self.surface_caps.alpha_modes[0],
			view_formats: vec![],
			desired_maximum_frame_latency: 2,
		}
	}

	pub fn configure_surface(&self, surface: &Surface, config: &SurfaceConfiguration) {
		surface.configure(&self.device, config);
	}

	pub fn create_transition_renderer(
		&self,
		sampler: &Sampler,
		initial_view: &TextureView,
		next_view: &TextureView,
		transition_shader: &ShaderModule,
	) -> WgpuTransitionRenderer {
		WgpuTransitionRenderer::new(
			&self.device,
			sampler,
			initial_view,
			next_view,
			&self.per_frame_data_bind_group_layout,
			&self.vertex_shader,
			&transition_shader,
			self.surface_format,
		)
	}

	pub fn match_adapter(&self, selector: &WgpuSelector) -> bool {
		selector.matches(&self.adapter)
	}

	pub fn new_renderer(
		&mut self,
		size: (u32, u32),
		options: RendererOptions,
	) -> &WgpuSurfaceRenderer {
		let new_renderer = WgpuSurfaceRenderer::new(options);
		let placeholder_rgba8 = vec![0u8; (size.0 * size.1 * 4) as usize]; // Fully black rectangle
		let placeholder_image = ImageWrapper::from_rgba8(placeholder_rgba8, size);
		let initial_image = options.initial_image.unwrap_or(&placeholder_image);

		let initial_texture = wgpu_texture::WgpuTexture::from_image(
			&self.device,
			&self.queue,
			initial_image,
			"to_texture",
			self.surface_format,
		)
		.unwrap();

		let renderers = self
			.scaled_textures_and_renderers
			.get(options.scaling_mode)
			.unwrap_or(
				&self
					.scaled_textures_and_renderers
					.insert(
						options.scaling_mode.clone(),
						HashMap::from([(size, initial_texture)]),
					)
					.unwrap(),
			);
	}

	// pub fn set_next_image_for_all(&mut self, image: &ImageWrapper) -> anyhow::Result<()> {
	// 	let next_texture = WgpuTexture::from_image(
	// 		&self.device,
	// 		&self.queue,
	// 		image,
	// 		"texture_to_scale",
	// 		self.texture_format,
	// 	)?;
	// 	for (mode, map) in &mut self.scaled_textures_and_renderers {
	// 		let sampler = choose_sampler(mode, &self.repeat_sampler, &self.mirror_repeat_sampler);

	// 		let mode_flat = ScalingModeFlat::from(mode);
	// 		let scaler = self
	// 			.texture_scalers
	// 			.get(&mode_flat)
	// 			.context("Failed to get a scaler")?;

	// 		for (size, (texture, renderers)) in map {
	// 			self.uniform_manager
	// 				.update_screen_size((size.0 as f32, size.1 as f32));
	// 			scaler.scale(
	// 				&self.device,
	// 				&self.queue,
	// 				sampler,
	// 				&next_texture.view,
	// 				&texture.view,
	// 				self.uniform_manager.bind_group(),
	// 			);

	// 			for renderer in renderers {
	// 				renderer.set_next_texture(&self.device, &texture.view, sampler);
	// 			}
	// 		}
	// 	}

	// 	Ok(())
	// }

	pub fn create_command_encoder(&self, label: &str) -> CommandEncoder {
		wgpu_utilities::create_command_encoder(&self.device, label)
	}

	pub fn submit(&self, encoder: CommandEncoder) {
		self.queue.submit(Some(encoder.finish()));
	}
}

pub struct WgpuSurfaceRenderer {
	device_handler: Rc<WgpuDeviceHandler>,
	surface: Surface<'static>,
	config: SurfaceConfiguration,
	sampler: Sampler,

	scaling_mode: ScalingMode,

	transition_renderer: WgpuTransitionRenderer,
	transition_texture_bind_group: BindGroup,
	offscreen_textures: [wgpu_texture::WgpuTexture; 3],
	display_texture_idx: usize,
	render_texture_idx: usize,

	per_frame_uniform_manager: PerFrameUniformManager,
}

impl WgpuSurfaceRenderer {
	pub fn new(
		device_handler: Rc<WgpuDeviceHandler>,
		conn: &Connection,
		layer_surface: &LayerSurface,
		size: (u32, u32),
		options: RendererOptions,
	) -> anyhome::Result<Self> {
		let surface = device_handler.create_surface(conn, layer_surface)?;
		let surface_format = device_handler.surface_format();
		let surface_config = device_handler.create_surface_config(size);

		device_handler.configure_surface(&surface, &surface_config);

		let transition_shader =
			wgpu_shaders::create_animation_shader(&device, options.shader_source);
	}

	fn increment_idx(&mut self) {
		self.display_texture_idx = self.render_texture_idx;
		self.render_texture_idx = (self.display_texture_idx + 1) % 3;
	}

	pub fn set_next_image(&mut self, image: &ImageWrapper) {
		let next_texture = WgpuTexture::from_image(
			&self.device_handler.device,
			&self.device_handler.queue,
			image,
			"texture_to_scale",
			self.device_handler.surface_format,
		);
	}

	pub fn set_next_texture(
		&mut self,
		device: &Device,
		texture_view: &TextureView,
		sampler: &Sampler,
	) {
		self.increment_idx();
		self.transition_renderer.update_textures(
			device,
			&self.offscreen_textures[self.display_texture_idx].view,
			texture_view,
			sampler,
		);
	}

	pub fn render(&self) -> anyhow::Result<()> {
		let surface_texture = match self.surface.get_current_texture() {
			Ok(frame) => frame,
			Err(SurfaceError::Outdated | SurfaceError::Lost) => {
				self.device_handler
					.configure_surface(&self.surface, &self.config);
				return Ok(());
			}
			Err(e) => return Err(anyhow::anyhow!("Failed to acquire next texture: {}", e)),
		};

		let mut encoder = self
			.device_handler
			.create_command_encoder("transition_command_encoder");

		self.transition_renderer.transition(
			&mut encoder,
			&self.offscreen_textures[self.render_texture_idx].view,
			self.per_frame_uniform_manager.bind_group(),
		);

		encoder.copy_texture_to_texture(
			TexelCopyTextureInfo {
				texture: &self.offscreen_textures[self.render_texture_idx].texture,
				mip_level: 0,
				origin: Origin3d::ZERO,
				aspect: TextureAspect::All,
			},
			TexelCopyTextureInfo {
				texture: &surface_texture.texture,
				mip_level: 0,
				origin: Origin3d::ZERO,
				aspect: TextureAspect::All,
			},
			self.offscreen_textures[self.render_texture_idx]
				.texture
				.size(),
		);

		self.device_handler.submit(encoder);

		Ok(())
	}
}

pub struct RenderingManager {
	instance: Instance,
	devices: Vec<WgpuDeviceHandler>,
}

impl RenderingManager {
	pub fn instance(&self) -> &Instance {
		&self.instance
	}

	pub fn new_renderer(
		&mut self,
		size: (u32, u32),
		options: RendererOptions,
	) -> &WgpuSurfaceRenderer {
		for device in &mut self.devices {
			if device.match_adapter(&wgpu_selector::WgpuSelector::from(
				renderer::GpuSelector::from(options.gpu_selector.clone()),
			)) {
				return device.new_renderer(size, options);
			}
		}

		let mut new_device = WgpuDeviceHandler::new();
		self.devices.push(new_device);
		// let new_renderer = new_device.new_renderer(size, options);
		self.devices.last().unwrap().new_renderer(size, options)

		// new_renderer
	}

	pub fn set_next_image_for_all(&mut self, image: &ImageWrapper) {
		for device in &mut self.devices {
			device.set_next_image_for_all(image).unwrap_or_else(|err| {
				warn!("Error when setting the next image: {}.", err.to_string())
			});
		}
	}
}
