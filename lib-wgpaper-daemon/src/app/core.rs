use crate::{RuntimeShaderConfig, image_wrapper::ImageWrapper, transition::ActiveTransition};
use wgpaper_config::ScalingMode;

pub struct WallpaperState {
	pub shader: RuntimeShaderConfig,
	pub current_image: Option<ImageWrapper>,
	pub transition: ActiveTransition,
	pub scaling_mode: ScalingMode,
}

impl WallpaperState {}
