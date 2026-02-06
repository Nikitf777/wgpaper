use std::num::NonZeroU32;

use smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface};
use wayland_client::{Connection, QueueHandle, protocol::wl_output::WlOutput};
use wgpaper_config::ScalingMode;

use crate::{
	app::App,
	image_wrapper::ImageWrapper,
	renderer::{Renderer, wgpu_renderer::WgpuRenderer},
	transition::TransitionProgress,
};

pub struct OutputStateEntry {
	output: WlOutput,
	layer: LayerSurface,
	renderer: Option<Box<dyn Renderer>>,
	size: (u32, u32),
}

impl OutputStateEntry {
	pub fn new(output: WlOutput, layer: LayerSurface) -> Self {
		Self {
			output,
			layer,
			renderer: None,
			size: (0, 0),
		}
	}

	pub fn output(&self) -> &WlOutput {
		&self.output
	}

	pub fn layer(&self) -> &LayerSurface {
		&self.layer
	}

	pub fn size(&self) -> (u32, u32) {
		self.size
	}

	pub fn width(&self) -> u32 {
		self.size.0
	}

	pub fn height(&self) -> u32 {
		self.size.1
	}

	pub fn is_initialized(&self) -> bool {
		self.renderer.is_some()
	}

	pub fn set_size(&mut self, size: (u32, u32)) {
		self.size = (
			NonZeroU32::new(size.0).map_or(256, NonZeroU32::get),
			NonZeroU32::new(size.1).map_or(256, NonZeroU32::get),
		);
	}

	pub fn commit(&self) {
		self.layer.wl_surface().commit();
	}

	pub fn frame(&self, qh: &QueueHandle<App>) {
		self.layer
			.wl_surface()
			.frame(qh, self.layer.wl_surface().clone());
	}

	pub fn render(&mut self) {
		if self.size.0 == 0 || self.size.1 == 0 {
			return;
		}

		if let Some(renderer) = &mut self.renderer {
			if let Err(e) = renderer.render() {
				eprintln!("Rendering error: {}", e);
			}
		}
	}

	pub fn init_renderer(
		&mut self,
		conn: &Connection,
		gpu_selector: crate::renderer::GpuSelector,
		animation_shader: &str,
		initial_image: &ImageWrapper,
		scaling_mode: &ScalingMode,
	) -> anyhow::Result<()> {
		let renderer = WgpuRenderer::new(
			conn,
			&self.layer,
			self.size,
			gpu_selector,
			animation_shader,
			initial_image,
			scaling_mode,
		)?;
		self.renderer = Some(Box::new(renderer));
		Ok(())
	}

	pub fn resize(&mut self, size: (u32, u32)) {
		self.set_size(size);

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
