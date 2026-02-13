use wgpaper_config::{Background, ScalingMode};

const STRETCH_SHADER: &str = include_str!("shaders/fragment_stretch.wgsl");
const FIT_SHADER: &str = include_str!("shaders/fragment_fit.wgsl");
const FIT_BG_SHADER: &str = include_str!("shaders/fragment_fit_bg_color.wgsl");
const COVER_SHADER: &str = include_str!("shaders/fragment_cover.wgsl");
const CENTER_SHADER: &str = include_str!("shaders/fragment_center.wgsl");
const CENTER_BG_SHADER: &str = include_str!("shaders/fragment_center_bg_color.wgsl");

pub(super) fn create_scaling_fragment_shader(
	device: &wgpu::Device,
	mode: &ScalingMode,
) -> wgpu::ShaderModule {
	let (shader, name_postfix) = match mode {
		ScalingMode::Fit { background } => {
			if matches!(background, Background::AutoColor)
				|| matches!(background, Background::CssColor(_))
			{
				(FIT_BG_SHADER, "fit_bg_color")
			} else {
				(FIT_SHADER, "fit")
			}
		}
		ScalingMode::Center { background } => {
			if matches!(background, Background::AutoColor)
				|| matches!(background, Background::CssColor(_))
			{
				(CENTER_BG_SHADER, "center_bg_color")
			} else {
				(CENTER_SHADER, "center")
			}
		}
		ScalingMode::Stretch => (STRETCH_SHADER, "stretch"),
		ScalingMode::Cover => (COVER_SHADER, "cover"),
	};

	device.create_shader_module(wgpu::ShaderModuleDescriptor {
		label: Some(&format!("scaling_fragment_shader_{}", name_postfix)),
		source: wgpu::ShaderSource::Wgsl(shader.into()),
	})
}
