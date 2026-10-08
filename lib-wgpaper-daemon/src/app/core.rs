use crate::{RuntimeShaderConfig, image_wrapper::ImageWrapper};
use wgpaper_config::ScalingMode;

/// The wallpaper state that is shared by every output.
///
/// Deliberately holds no transition clock: each output owns its own
/// [`ActiveTransition`](crate::transition::ActiveTransition), so a transition
/// can be started on some outputs without disturbing the rest.
pub struct WallpaperState {
	pub shader: RuntimeShaderConfig,
	pub current_image: Option<ImageWrapper>,
	pub scaling_mode: ScalingMode,
}
