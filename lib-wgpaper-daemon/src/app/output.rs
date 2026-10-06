use crate::{
	app::{SctkState, core::WallpaperState},
	image_wrapper::ImageWrapper,
	renderer::{
		RendererOptions,
		wgpu::{wgpu_render_manager::RenderManager, wgpu_surface::SurfaceRenderer},
	},
	transition::TransitionProgress,
};
use anyhow::{Context, Ok};
use log::{error, warn};
use smithay_client_toolkit::{
	compositor::CompositorState,
	output::{OutputInfo, OutputState},
	shell::{
		WaylandSurface,
		wlr_layer::{
			Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface, LayerSurfaceConfigure,
		},
	},
};
use std::collections::HashMap;
use wayland_client::{
	Connection, QueueHandle,
	protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};
use wgpaper_config::{GpuConfig, ShaderConfig};

pub struct OutputStateEntry {
	output: WlOutput,
	layer: LayerSurface,
	renderer: Option<SurfaceRenderer>,
	/// Size the renderer was last built or resized for.  Meaningless while
	/// `renderer` is `None`.
	current_size: (u32, u32),
}

/// What a layer-surface configure means for an output's renderer.
///
/// The renderer owns resources that do not depend on the surface size (the
/// wgpu surface, the sampler, the pipelines), so it is built exactly once —
/// on the first usable configure — and merely resized afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RendererState {
	/// No renderer yet: this configure has to build one.
	Uninitialized,
	/// The renderer already matches the configured size; nothing to do.
	UpToDate,
	/// The renderer exists but was built for a different size.
	NeedsResize,
}

impl OutputStateEntry {
	pub fn new(output: WlOutput, layer: LayerSurface) -> Self {
		Self {
			output,
			layer,
			renderer: None,
			current_size: (0, 0),
		}
	}

	/// Classify a configure against the state of this output's renderer.
	fn renderer_state(&self, size: (u32, u32)) -> RendererState {
		match &self.renderer {
			None => RendererState::Uninitialized,
			Some(_) if self.current_size == size => RendererState::UpToDate,
			Some(_) => RendererState::NeedsResize,
		}
	}

	fn get_info(&self, output_state: &OutputState) -> Option<OutputInfo> {
		output_state.info(&self.output)
	}

	pub fn commit(&self) {
		self.layer.wl_surface().commit();
	}

	pub fn frame(&self, qh: &QueueHandle<SctkState>) {
		self.layer
			.wl_surface()
			.frame(qh, self.layer.wl_surface().clone());
	}

	pub fn render(&mut self) {
		if let Some(renderer) = &mut self.renderer
			&& let Err(e) = renderer.render()
		{
			warn!("Rendering error: {}", e);
		}
	}

	pub fn init_renderer(
		&mut self,
		render_manager: &mut RenderManager,
		conn: &Connection,
		size: (u32, u32),
		options: &RendererOptions,
	) -> anyhow::Result<()> {
		let renderer = render_manager.create_surface(conn, &self.layer, size, options)?;
		self.renderer = Some(renderer);
		self.current_size = size;
		Ok(())
	}

	/// Resize the renderer, keeping the pipelines and the wgpu surface.
	///
	/// A no-op if the renderer has not been created yet, or if it already
	/// uses this size.
	pub fn resize(&mut self, size: (u32, u32)) -> anyhow::Result<()> {
		let Some(renderer) = &mut self.renderer else {
			return Ok(());
		};

		if size.0 == 0 || size.1 == 0 || self.current_size == size {
			return Ok(());
		}

		renderer.resize(size)?;
		self.current_size = size;
		Ok(())
	}

	pub fn is_transitioning(&self) -> bool {
		if let Some(renderer) = &self.renderer {
			!renderer.get_transition_progress().is_finished()
		} else {
			false
		}
	}

	pub fn start_transition(&mut self, image: &ImageWrapper) {
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.set_next_image_size(image);
			renderer.set_transition_progress(TransitionProgress::reset());
			renderer.write_data();
			renderer.set_next_image(image);
		}
	}

	pub fn set_transition_progress(&mut self, progress: TransitionProgress) {
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.set_transition_progress(progress);
			renderer.write_data();
		}
	}
}

struct Bounds {
	position: (i32, i32),
	size: (u32, u32),
}

impl Bounds {
	fn new(top_left: (i32, i32), bottom_right: (i32, i32)) -> Self {
		Self {
			position: top_left,
			size: (
				(top_left.0 - bottom_right.0) as u32,
				(top_left.1 - bottom_right.1) as u32,
			),
		}
	}
}

fn calculate_global_bounds(output_state: &OutputState) -> anyhow::Result<Bounds> {
	let mut max_x = 0;
	let mut max_y = 0;
	let mut min_x = 0;
	let mut min_y = 0;
	for o in output_state.outputs() {
		let info = output_state
			.info(&o)
			.context("Failed to get the output's info")?;
		let position = info
			.logical_position
			.context("Failed to get the output's logical position")?;
		max_x = position.0.max(max_x);
		max_y = position.1.max(max_y);
		min_x = position.0.min(min_x);
		min_y = position.1.min(min_y);
	}

	Ok(Bounds::new((max_x, max_y), (min_x, min_y)))
}

pub struct OutputsConfiguration {
	gpu_config: GpuConfig,
	shader_config: ShaderConfig,
}

pub struct OutputManager {
	gpu_config: GpuConfig,
	output_state: OutputState,
	outputs: HashMap<WlSurface, OutputStateEntry>,
}

impl OutputManager {
	pub fn new(gpu_config: GpuConfig, output_state: OutputState) -> Self {
		Self {
			gpu_config,
			output_state,
			outputs: HashMap::default(),
		}
	}

	pub fn output_state(&mut self) -> &mut OutputState {
		&mut self.output_state
	}

	pub fn queue_render_all(&mut self, qh: &QueueHandle<SctkState>) {
		for output in self.outputs.values_mut() {
			output.frame(qh);
			output.render();
			output.commit();
		}
	}

	pub fn start_transition(
		&mut self,
		qh: &QueueHandle<SctkState>,
		image: &ImageWrapper,
	) -> anyhow::Result<()> {
		for (surface, output) in self.outputs.iter_mut() {
			output.start_transition(image);
			surface.frame(qh, surface.clone());
			output.render();
			output.commit();
		}
		Ok(())
	}

	pub fn frame(
		&mut self,
		qh: &QueueHandle<SctkState>,
		surface: &WlSurface,
		progress: TransitionProgress,
	) {
		if let Some(output) = self.outputs.get_mut(surface) {
			if output.is_transitioning() {
				output.set_transition_progress(progress);
			}
			output.render();
			output.commit();

			// Only keep the frame loop alive while the transition is actually
			// animating. Once `progress` reaches its end we draw that final
			// frame above and stop asking for callbacks, so an idle wallpaper
			// costs nothing. A later `start_transition` requests the next
			// callback and resumes the loop.
			if !progress.is_finished() {
				surface.frame(qh, surface.clone());
			}
		}
	}

	pub fn handle_new_output(
		&mut self,
		qh: &QueueHandle<SctkState>,
		compositor_state: &CompositorState,
		layer_shell: &LayerShell,
		output: &WlOutput,
	) {
		let surface = compositor_state.create_surface(qh);
		let layer = layer_shell.create_layer_surface(
			qh,
			surface.clone(),
			Layer::Background,
			Some("wallpaper_layer"),
			Some(output),
		);

		layer.set_anchor(Anchor::all());
		layer.set_exclusive_zone(-1);
		layer.set_keyboard_interactivity(KeyboardInteractivity::None);
		layer.set_size(0, 0);
		layer.commit();

		self.outputs
			.insert(surface, OutputStateEntry::new(output.clone(), layer));
	}

	pub fn handle_output_destroyed(&mut self, destroyed_output: WlOutput) {
		if let Some((_, _)) = self
			.outputs
			.extract_if(|_, output| output.output == destroyed_output)
			.next()
		{
			// TODO: log that the output was removed.
		}
	}

	pub fn handle_layer_surface_closed(&mut self, layer: &LayerSurface) {
		self.outputs.retain(|_, output| &output.layer != layer);
	}

	pub fn handle_configure(
		&mut self,
		render_manager: &mut RenderManager,
		conn: &Connection,
		qh: &QueueHandle<SctkState>,
		layer: &LayerSurface,
		configure: &LayerSurfaceConfigure,
		wallpaper_state: &WallpaperState,
	) {
		let Some(output) = self.outputs.values_mut().find(|e| &e.layer == layer) else {
			return;
		};

		let size = configure.new_size;

		// A zero-sized configure carries no usable geometry, and the
		// compositor may legitimately send one (the layer surface requested
		// its size with `set_size(0, 0)`).  The surface must still be
		// committed to acknowledge the configure, otherwise it stays stuck
		// in the pending-configure state and never draws.
		if size.0 == 0 || size.1 == 0 {
			warn!(
				"Ignoring a zero-sized configure ({:?}); waiting for a usable size.",
				size
			);
			output.commit();
			return;
		}

		match output.renderer_state(size) {
			// First usable configure: build the renderer.  Every later
			// configure only resizes it, so pipelines are compiled once per
			// output instead of once per configure.
			RendererState::Uninitialized => {
				let gpu_selector_default = wgpaper_config::GpuSelector::default();
				let gpu_selector = match &self.gpu_config {
					GpuConfig::Global(selector) => selector,
					GpuConfig::PerMonitor(map) => output
						.get_info(&self.output_state)
						.map(|info| {
							info.name
								.as_ref()
								.map(|name| map.get(name).unwrap_or(&gpu_selector_default))
								.unwrap_or(&gpu_selector_default)
						})
						.unwrap_or(&gpu_selector_default),
				};
				let shader_source = match output.get_info(&self.output_state) {
					Some(info) => wallpaper_state
						.shader
						.resolve_for_output(info.name.as_deref()),
					None => wallpaper_state.shader.resolve_for_output(None),
				};

				output
					.init_renderer(
						render_manager,
						conn,
						size,
						&RendererOptions {
							gpu_selector,
							shader_source,
							initial_image: wallpaper_state.current_image.as_ref(),
							scaling_mode: &wallpaper_state.scaling_mode,
						},
					)
					.unwrap_or_else(|err| {
						error!("Renderer init failed: {}", err);
						std::process::exit(1);
					});
			}
			// The size changed: re-allocate the screen-sized textures and
			// reconfigure the surface, keeping the pipelines.
			RendererState::NeedsResize => {
				if let Err(err) = output.resize(size) {
					error!("Failed to resize the renderer to {:?}: {}", size, err);
				}
			}
			// Duplicate configure for the size we are already at.
			RendererState::UpToDate => {}
		}

		self.queue_render_all(qh);
	}
}
