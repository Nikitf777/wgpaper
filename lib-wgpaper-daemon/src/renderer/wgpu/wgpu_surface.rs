use std::rc::Rc;

use wgpaper_config::ScalingMode;
use wgpu::{
	Origin3d, Sampler, Surface, SurfaceConfiguration, SurfaceError, TexelCopyTextureInfo,
	TextureAspect, TextureFormat,
};

use crate::{
	image_wrapper::ImageWrapper,
	renderer::{
		RendererOptions,
		wgpu::{
			wgpu_device::GpuDevice,
			wgpu_shaders,
			wgpu_texture::WgpuTexture,
			wgpu_transition_renderer::WgpuTransitionRenderer,
			wgpu_uniforms::PerFrameUniformManager,
			wgpu_utilities,
		},
	},
	transition::TransitionProgress,
};

/// Per-surface rendering state.
///
/// Each monitor (output) gets one `SurfaceRenderer`. It owns the wgpu
/// surface, its configuration, the scaled wallpaper texture, a triple-buffer
/// of off-screen textures for transitions, and the transition renderer.
///
/// Texture scalers are shared across all surfaces on the same device via
/// [`GpuDevice::scale_texture`].
pub struct SurfaceRenderer {
	device: Rc<GpuDevice>,
	surface: Surface<'static>,
	config: SurfaceConfiguration,
	sampler: Sampler,
	surface_format: TextureFormat,
	scaling_mode: ScalingMode,

	scaled_texture: WgpuTexture,
	offscreen_textures: [WgpuTexture; 3],
	display_texture_idx: usize,
	render_texture_idx: usize,

	transition_renderer: WgpuTransitionRenderer,
	per_frame_uniform_manager: PerFrameUniformManager,
}

impl SurfaceRenderer {
	/// Build a `SurfaceRenderer` from a pre-created wgpu `Surface`.
	///
	/// `device` is the `GpuDevice` that will back this surface.
	/// The wgpu `Surface` must already have been created (by the
	/// `RenderManager` which owns the `Instance`).
	pub fn new(
		device: Rc<GpuDevice>,
		surface: Surface<'static>,
		size: (u32, u32),
		options: &RendererOptions,
	) -> anyhow::Result<Self> {
		let scaling_mode = options.scaling_mode.clone();

		// ── surface capabilities / format / config ──────────────────
		let surface_caps = surface.get_capabilities(&device.adapter);
		let surface_format = surface_caps
			.formats
			.iter()
			.copied()
			.find(|f| f.is_srgb())
			.unwrap_or(surface_caps.formats[0]);

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
		surface.configure(&device.device, &config);

		// ── placeholder / initial image ──────────────────────────────
		let placeholder_rgba8 = vec![0u8; (size.0 * size.1 * 4) as usize];
		let placeholder_image = ImageWrapper::from_rgba8(placeholder_rgba8, size);
		let initial_image = options.initial_image.unwrap_or(&placeholder_image);

		let initial_texture = WgpuTexture::from_image(
			&device.device,
			&device.queue,
			initial_image,
			"initial_texture",
			surface_format,
		)?;

		// ── sampler ──────────────────────────────────────────────────
		let (address_mode, bg_color) =
			wgpu_utilities::get_address_mode_and_bg_color(&scaling_mode);
		let sampler = wgpu_utilities::create_sampler(&device.device, address_mode);

		// ── per-frame uniforms ───────────────────────────────────────
		let mut per_frame_uniform_manager = PerFrameUniformManager::with_layout(
			&device.device,
			&device.per_frame_bind_group_layout,
			(size.0 as f32, size.1 as f32),
			(initial_image.width() as f32, initial_image.height() as f32),
			bg_color,
		);
		per_frame_uniform_manager.write_data(&device.queue);
		per_frame_uniform_manager
			.update_transition_progress(TransitionProgress::finished());

		// ── scaled texture ───────────────────────────────────────────
		let scaled_texture = WgpuTexture::from_image(
			&device.device,
			&device.queue,
			&placeholder_image,
			"scaled_texture",
			surface_format,
		)?;

		// Scale the initial image into the scaled texture.
		device.scale_texture(
			&scaling_mode,
			surface_format,
			&device.queue,
			&sampler,
			&initial_texture.view,
			&scaled_texture.view,
			per_frame_uniform_manager.bind_group(),
		);

		// ── off-screen textures (triple buffer) ──────────────────────
		let offscreen_textures: [WgpuTexture; 3] = core::array::from_fn(|i| {
			WgpuTexture::from_image(
				&device.device,
				&device.queue,
				&placeholder_image,
				&format!("offscreen_texture_{}", i),
				surface_format,
			)
			.unwrap()
		});
		let display_texture_idx: usize = 0;
		let render_texture_idx: usize = 1;

		// ── transition renderer ──────────────────────────────────────
		let transition_shader =
			wgpu_shaders::create_animation_shader(&device.device, options.shader_source);

		let transition_renderer = WgpuTransitionRenderer::new(
			&device.device,
			&sampler,
			&scaled_texture.view,                          // initial = prev
			&offscreen_textures[display_texture_idx].view, // next = first frame
			&device.per_frame_bind_group_layout,
			&device.vertex_shader,
			&transition_shader,
			surface_format,
		);

		Ok(Self {
			device,
			surface,
			config,
			sampler,
			surface_format,
			scaling_mode,
			scaled_texture,
			offscreen_textures,
			display_texture_idx,
			render_texture_idx,
			transition_renderer,
			per_frame_uniform_manager,
		})
	}

	// ── internal helpers ──────────────────────────────────────────

	fn increment_idx(&mut self) {
		self.display_texture_idx = self.render_texture_idx;
		self.render_texture_idx = (self.display_texture_idx + 1) % 3;
	}

	// ── public API ────────────────────────────────────────────────

	/// Draw one frame.
	pub fn render(&mut self) -> anyhow::Result<()> {
		let surface_texture = match self.surface.get_current_texture() {
			Ok(frame) => frame,
			Err(SurfaceError::Outdated | SurfaceError::Lost) => {
				self.surface
					.configure(&self.device.device, &self.config);
				return Ok(());
			}
			Err(e) => return Err(anyhow::anyhow!("Failed to acquire next texture: {e}")),
		};

		let mut encoder = wgpu_utilities::create_command_encoder(
			&self.device.device,
			"transition_command_encoder",
		);

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

		self.device.queue.submit(Some(encoder.finish()));
		surface_texture.present();

		Ok(())
	}

	/// Update the surface size (called when the output is resized).
	pub fn resize(&mut self, size: (u32, u32)) -> anyhow::Result<()> {
		if size.0 == 0 || size.1 == 0 {
			return Ok(());
		}

		self.config.width = size.0;
		self.config.height = size.1;
		self.surface
			.configure(&self.device.device, &self.config);
		self.per_frame_uniform_manager
			.update_screen_size((size.0 as f32, size.1 as f32));
		Ok(())
	}

	/// Schedule a new wallpaper image for the next transition.
	///
	/// Loads the image into a GPU texture, scales it, and updates the
	/// transition renderer to blend between the current display texture
	/// and the newly scaled wallpaper.
	pub fn set_next_image(&mut self, image: &ImageWrapper) {
		let next_texture = WgpuTexture::from_image(
			&self.device.device,
			&self.device.queue,
			image,
			"texture_to_scale",
			self.surface_format,
		)
		.unwrap();

		self.per_frame_uniform_manager.update_texture_size((
			next_texture.texture.width() as f32,
			next_texture.texture.height() as f32,
		));

		self.increment_idx();

		self.device.scale_texture(
			&self.scaling_mode,
			self.surface_format,
			&self.device.queue,
			&self.sampler,
			&next_texture.view,
			&self.scaled_texture.view,
			self.per_frame_uniform_manager.bind_group(),
		);

		self.transition_renderer.update_textures(
			&self.device.device,
			&self.offscreen_textures[self.display_texture_idx].view,
			&self.scaled_texture.view,
			&self.sampler,
		);
	}

	/// Return the current transition progress.
	pub fn get_transition_progress(&self) -> TransitionProgress {
		self.per_frame_uniform_manager.transition_progress()
	}

	/// Update the transition progress and write it to the GPU buffer.
	pub fn set_transition_progress(&mut self, progress: TransitionProgress) {
		self.per_frame_uniform_manager
			.update_transition_progress(progress);
		self.per_frame_uniform_manager
			.write_data(&self.device.queue);
	}
}
