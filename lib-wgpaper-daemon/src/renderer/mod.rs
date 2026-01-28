use crate::transition::TransitionProgress;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_client::Connection;
pub mod wgpu_renderer;

pub trait Renderer {
	fn new(
		conn: &Connection,
		layer_surface: &LayerSurface,
		width: u32,
		height: u32,
	) -> anyhow::Result<Self>
	where
		Self: Sized;

	fn render(&mut self) -> anyhow::Result<()>;

	fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()>;

	fn get_transition_progress(&self) -> TransitionProgress;

	fn set_transition_progress(&mut self, progress: TransitionProgress);
}
