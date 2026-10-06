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
			wgpu_device::GpuDevice, wgpu_shaders, wgpu_texture::WgpuTexture,
			wgpu_transition_renderer::WgpuTransitionRenderer,
			wgpu_uniforms::PerFrameUniformManager, wgpu_utilities,
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

		// ── initial image ────────────────────────────────────────────
		let initial_image = options.initial_image;

		// ── sampler ──────────────────────────────────────────────────
		// Reuse the samplers cached on the device instead of creating one
		// per surface: the address mode is fully determined by the
		// scaling mode.
		let sampler = device.choose_sampler(&scaling_mode).clone();

		// ── per-frame uniforms ───────────────────────────────────────
		let mut per_frame_uniform_manager = PerFrameUniformManager::with_layout(
			&device.device,
			&device.per_frame_bind_group_layout,
			(size.0 as f32, size.1 as f32),
			initial_image
				.map(|image| (image.width() as f32, image.height() as f32))
				.unwrap_or((size.0 as f32, size.1 as f32)),
			wgpu_utilities::bg_color_for(&scaling_mode),
		);
		// `write_data` must come after every field update: it is the only
		// thing that pushes the uniform to the GPU, so a later in-memory
		// mutation is invisible to the shader until the next write.
		per_frame_uniform_manager.update_transition_progress(TransitionProgress::finished());
		per_frame_uniform_manager.write_data(&device.queue);

		// ── scaled texture ───────────────────────────────────────────
		let scaled_texture = WgpuTexture::cleared(
			&device.device,
			&device.queue,
			size,
			"scaled_texture",
			surface_format,
		);

		// Scale the initial image into the scaled texture.
		if let Some(initial_image) = initial_image {
			let initial_texture = WgpuTexture::from_image(
				&device.device,
				&device.queue,
				initial_image,
				"initial_texture",
				surface_format,
			)?;

			device.scale_texture(
				&scaling_mode,
				surface_format,
				&device.queue,
				&sampler,
				&initial_texture.view,
				&scaled_texture.view,
				per_frame_uniform_manager.bind_group(),
			);
		}

		// ── off-screen textures (triple buffer) ──────────────────────
		let offscreen_textures: [WgpuTexture; 3] = core::array::from_fn(|i| {
			WgpuTexture::cleared(
				&device.device,
				&device.queue,
				size,
				&format!("offscreen_texture_{}", i),
				surface_format,
			)
		});
		let display_texture_idx: usize = 0;
		let render_texture_idx: usize = 1;

		// ── transition renderer ──────────────────────────────────────
		let transition_shader =
			wgpu_shaders::create_animation_shader(&device.device, options.shader_source);

		// `prev` is the blank frame that was just rendered, `target` is the
		// wallpaper.  With progress pinned to `finished()` the shader
		// resolves to `target`, so the initial configure shows the wallpaper
		// rather than the empty first frame.
		let transition_renderer = WgpuTransitionRenderer::new(
			&device.device,
			&sampler,
			&offscreen_textures[display_texture_idx].view, // prev = blank first frame
			&scaled_texture.view,                          // next = wallpaper
			&device.per_frame_bind_group_layout,
			&device.vertex_shader,
			&transition_shader.module,
			transition_shader.entry_point,
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
				self.surface.configure(&self.device.device, &self.config);
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
	///
	/// Only the screen-sized resources are re-created: the wgpu surface,
	/// sampler, uniform buffer and pipelines are size-independent and are
	/// kept alive, so a resize does not recompile any shader.
	pub fn resize(&mut self, size: (u32, u32)) -> anyhow::Result<()> {
		if size.0 == 0 || size.1 == 0 || size == (self.config.width, self.config.height) {
			return Ok(());
		}

		// The source image is not kept in memory, so the wallpaper that is
		// currently on screen is resampled into the new geometry instead of
		// being decoded and uploaded again.  `Stretch` is a plain 1:1
		// resample of the already-fitted wallpaper, which is what we want
		// here regardless of the configured scaling mode.
		let new_scaled = WgpuTexture::cleared(
			&self.device.device,
			&self.device.queue,
			size,
			"scaled_texture",
			self.surface_format,
		);
		let prev_scaled = std::mem::replace(&mut self.scaled_texture, new_scaled);
		self.device.scale_texture(
			&ScalingMode::Stretch,
			self.surface_format,
			&self.device.queue,
			&self.sampler,
			&prev_scaled.view,
			&self.scaled_texture.view,
			self.per_frame_uniform_manager.bind_group(),
		);
		drop(prev_scaled);

		for (index, texture) in self.offscreen_textures.iter_mut().enumerate() {
			*texture = WgpuTexture::cleared(
				&self.device.device,
				&self.device.queue,
				size,
				&format!("offscreen_texture_{}", index),
				self.surface_format,
			);
		}

		self.config.width = size.0;
		self.config.height = size.1;
		self.surface.configure(&self.device.device, &self.config);
		self.per_frame_uniform_manager
			.update_screen_size((size.0 as f32, size.1 as f32));

		// The bind group still references the texture views that were just
		// dropped, so it has to be rebuilt against the new ones.
		self.transition_renderer.update_textures(
			&self.device.device,
			&self.offscreen_textures[self.display_texture_idx].view,
			&self.scaled_texture.view,
			&self.sampler,
		);

		// A resize destroys the "previous frame" the transition blends from,
		// so an in-flight transition is completed instead of fading in from
		// an empty texture.
		self.set_transition_progress(TransitionProgress::finished());
		self.write_data();

		Ok(())
	}

	/// Schedule a new wallpaper image for the next transition.
	///
	/// Loads the image into a GPU texture, scales it, and updates the
	/// transition renderer to blend between the current display texture
	/// and the newly scaled wallpaper.
	pub fn set_next_image_size(&mut self, image: &ImageWrapper) {
		self.per_frame_uniform_manager
			.update_texture_size((image.width() as f32, image.height() as f32));
	}

	/// Return the current transition progress.
	pub fn get_transition_progress(&self) -> TransitionProgress {
		self.per_frame_uniform_manager.transition_progress()
	}

	/// Update the transition progress and write it to the GPU buffer.
	pub fn set_transition_progress(&mut self, progress: TransitionProgress) {
		self.per_frame_uniform_manager
			.update_transition_progress(progress);
	}

	pub fn write_data(&mut self) {
		self.per_frame_uniform_manager
			.write_data(&self.device.queue);
	}

	pub fn set_next_image(&mut self, image: &ImageWrapper) {
		let next_texture = WgpuTexture::from_image(
			&self.device.device,
			&self.device.queue,
			image,
			"texture_to_scale",
			self.surface_format,
		)
		.unwrap();

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
}
