use wgpaper_config::{GpuSelector, ScalingMode};

use crate::{image_wrapper::ImageWrapper, transition::ActiveTransition};

pub struct WallpaperState {
	pub gpu_selector: GpuSelector,
	pub shader_source: String,
	pub current_image: ImageWrapper,
	pub transition: ActiveTransition,
	pub scaling_mode: ScalingMode,
}

impl WallpaperState {}
