use crate::{
	app::{SctkState, core::WallpaperState},
	image_wrapper::ImageWrapper,
	renderer::{Renderer, RendererOptions, wgpu::wgpu_renderer::WgpuRenderer},
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
		if let Some(renderer) = &mut self.renderer {
			if let Err(e) = renderer.render() {
				warn!("Rendering error: {}", e);
			}
		}
	}

	pub fn init_renderer(
		&mut self,
		conn: &Connection,
		size: (u32, u32),
		options: &RendererOptions,
	) -> anyhow::Result<()> {
		let renderer = WgpuRenderer::new(conn, &self.layer, size, options)?;
		self.renderer = Some(renderer);
		Ok(())
	}

	fn update_virtual_screen_data(
		&mut self,
		offset: (f32, f32),
		scale: (f32, f32),
	) -> anyhow::Result<()> {
		self.renderer
			.as_mut()
			.context("Renderer is not initialized")?
			.update_virtual_screen_data(offset, scale);
		Ok(())
	}

	pub fn resize(&mut self, size: (u32, u32)) {
		if size.0 == 0 || size.1 == 0 {
			return;
		}

		if let Some(renderer) = &mut self.renderer {
			if let Err(e) = renderer.resize(size) {
				warn!("Resize error: {}", e);
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

fn calculate_virtual_screen_data(
	global_bounds: Bounds,
	output_info: OutputInfo,
) -> anyhow::Result<((f32, f32), (f32, f32))> {
	let pos = output_info
		.logical_position
		.context("Failed to get the output's logical position")?;
	let size = output_info
		.logical_size
		.context("Failed to get the output's logical size")?;

	let gpos = global_bounds.position;
	let gsize = global_bounds.size;

	Ok((
		(
			(pos.0 - gpos.0) as f32 / gsize.0 as f32,
			(pos.1 - gpos.1) as f32 / gsize.1 as f32,
		),
		(
			size.0 as f32 / gsize.0 as f32,
			size.1 as f32 / gsize.1 as f32,
		),
	))
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
		for output in self.outputs.values_mut() {
			if &output.layer == layer {
				output
					.init_renderer(
						conn,
						configure.new_size,
						&RendererOptions {
							gpu_selector: &wallpaper_state.gpu_selector,
							shader_source: wallpaper_state.shader_source.as_deref(),
							initial_image: wallpaper_state.current_image.as_ref(),
							scaling_mode: &wallpaper_state.scaling_mode,
						},
					)
					.unwrap_or_else(|err| {
						error!("Renderer init failed: {}", err);
						std::process::exit(1);
					});
			}

			let global_bounds = calculate_global_bounds(&self.output_state).unwrap();
			let (offset, scale) = calculate_virtual_screen_data(
				global_bounds,
				output.get_info(&self.output_state).unwrap(),
			)
			.unwrap();

			let _ = output.update_virtual_screen_data(offset, scale);

			output.frame(qh);
			output.commit();
		}
	}
}
