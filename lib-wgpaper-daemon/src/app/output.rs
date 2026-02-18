use std::collections::HashMap;

use anyhow::{Context, Ok};
use smithay_client_toolkit::{
	compositor::CompositorState,
	output::OutputState,
	shell::{
		WaylandSurface,
		wlr_layer::{
			Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface, LayerSurfaceConfigure,
		},
	},
};
use wayland_client::{
	Connection, QueueHandle,
	protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};
use wgpaper_config::{GpuSelector, ScalingMode};

use crate::{
	app::{SctkState, core::WallpaperState},
	image_wrapper::ImageWrapper,
	renderer::{
		Renderer,
		wgpu::wgpu_renderer::{self, WgpuRenderer},
	},
	transition::TransitionProgress,
};

pub struct OutputStateEntry {
	output: WlOutput,
	layer: LayerSurface,
	renderer: Option<WgpuRenderer>,
}

impl OutputStateEntry {
	pub fn new(output: WlOutput, layer: LayerSurface) -> Self {
		Self {
			output,
			layer,
			renderer: None,
		}
	}

	pub fn is_initialized(&self) -> bool {
		self.renderer.is_some()
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
		if let Some(renderer) = &mut self.renderer {
			if let Err(e) = renderer.render() {
				eprintln!("Rendering error: {}", e);
			}
		}
	}

	pub fn init_renderer(
		&mut self,
		conn: &Connection,
		size: (u32, u32),
		gpu_selector: GpuSelector,
		shader_source: Option<&str>,
		initial_image: Option<&ImageWrapper>,
		scaling_mode: &ScalingMode,
	) -> anyhow::Result<()> {
		let renderer = wgpu_renderer::WgpuRenderer::new(
			conn,
			&self.layer,
			size,
			gpu_selector,
			shader_source,
			initial_image,
			scaling_mode,
		)?;
		self.renderer = Some(renderer);
		Ok(())
	}

	pub fn resize(&mut self, size: (u32, u32)) {
		if size.0 == 0 || size.1 == 0 {
			return;
		}

		if let Some(renderer) = &mut self.renderer {
			if let Err(e) = renderer.resize(size) {
				eprintln!("Resize error: {}", e);
			}
		}
	}

	pub fn is_transitioning(&self) -> bool {
		if let Some(renderer) = &self.renderer {
			!renderer.get_transition_progress().is_finished()
		} else {
			false
		}
	}

	pub fn set_transition_progress(&mut self, progress: TransitionProgress) {
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.set_transition_progress(progress);
		}
	}

	pub fn set_next_image(&mut self, image: &ImageWrapper) {
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.set_next_image(image);
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

pub struct OutputManager {
	outputs: HashMap<WlSurface, OutputStateEntry>,
	output_state: OutputState,
}

impl OutputManager {
	pub fn new(output_state: OutputState) -> Self {
		Self {
			outputs: HashMap::default(),
			output_state,
		}
	}

	pub fn output_state(&mut self) -> &mut OutputState {
		&mut self.output_state
	}

	pub fn queue_render_all(&mut self, qh: &QueueHandle<SctkState>) {
		for (_, output) in self.outputs.iter_mut() {
			output.frame(qh);
			output.commit();
		}
	}

	pub fn start_transition(
		&mut self,
		qh: &QueueHandle<SctkState>,
		image: &ImageWrapper,
	) -> anyhow::Result<()> {
		for (surface, output) in self.outputs.iter_mut() {
			output.set_next_image(image);
			output.set_transition_progress(TransitionProgress::reset());
			surface.frame(qh, surface.clone());
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
			if !progress.is_finished() {
				surface.frame(qh, surface.clone());
			}
			if output.is_transitioning() {
				output.set_transition_progress(progress);
			}
			output.render();
			output.commit();
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
			Some(&output),
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
		conn: &Connection,
		qh: &QueueHandle<SctkState>,
		layer: &LayerSurface,
		configure: &LayerSurfaceConfigure,
		wallpaper_state: &WallpaperState,
	) {
		if let Some(output) = self.outputs.values_mut().find(|e| &e.layer == layer) {
			output.resize(configure.new_size);

			if !output.is_initialized() {
				if let Err(e) = output.init_renderer(
					conn,
					configure.new_size,
					wallpaper_state.gpu_selector.clone(),
					wallpaper_state.shader_source.as_deref(),
					wallpaper_state.current_image.as_ref(),
					&wallpaper_state.scaling_mode,
				) {
					eprintln!("Renderer init failed: {}", e);
				}
			}

			self.queue_render_all(qh);
		}
	}
}
