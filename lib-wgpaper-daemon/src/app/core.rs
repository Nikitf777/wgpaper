use crate::{image_wrapper::ImageWrapper, transition::ActiveTransition};
use wgpaper_config::{GpuSelector, ScalingMode};

pub struct WallpaperState {
	pub shader_source: Option<String>,
	pub current_image: Option<ImageWrapper>,
	pub transition: ActiveTransition,
	pub scaling_mode: ScalingMode,
}

impl WallpaperState {}
