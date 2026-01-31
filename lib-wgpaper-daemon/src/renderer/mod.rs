use crate::transition::TransitionProgress;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_client::Connection;

pub mod texture;
pub mod wgpu_renderer;

pub trait Renderer {
	fn new(
		conn: &Connection,
		layer_surface: &LayerSurface,
		width: u32,
		height: u32,
		animation_shader: &str,
		initial_image: &[u8],
	) -> anyhow::Result<Self>
	where
		Self: Sized;

	fn render(&mut self) -> anyhow::Result<()>;

	fn resize(&mut self, width: u32, height: u32) -> anyhow::Result<()>;

	fn get_transition_progress(&self) -> TransitionProgress;

	fn set_transition_progress(&mut self, progress: TransitionProgress);

	fn set_next_image(&mut self, rgba8: &[u8], dimensions: (u32, u32));
}
