use crate::image_wrapper::ImageWrapper;
use wgpaper_config::ScalingMode;

pub mod wgpu;

pub struct RendererOptions<'a> {
	pub gpu_selector: &'a wgpaper_config::GpuSelector,
	pub shader_source: Option<&'a str>,
	pub initial_image: Option<&'a ImageWrapper>,
	pub scaling_mode: &'a ScalingMode,
}

#[derive(Debug, Clone)]
pub struct GpuSelector {
	pub index: Option<usize>,
	pub name_substring: Option<String>,
	pub device_type: Option<DeviceType>,
}

#[derive(Debug, Clone)]
pub enum DeviceType {
	Other,
	IntegratedGpu,
	DiscreteGpu,
	VirtualGpu,
	Cpu,
}

impl From<wgpaper_config::DeviceType> for DeviceType {
	#[inline]
	fn from(src: wgpaper_config::DeviceType) -> Self {
		match src {
			wgpaper_config::DeviceType::Other => DeviceType::Other,
			wgpaper_config::DeviceType::IntegratedGpu => DeviceType::IntegratedGpu,
			wgpaper_config::DeviceType::DiscreteGpu => DeviceType::DiscreteGpu,
			wgpaper_config::DeviceType::VirtualGpu => DeviceType::VirtualGpu,
			wgpaper_config::DeviceType::Cpu => DeviceType::Cpu,
		}
	}
}

impl From<wgpaper_config::GpuSelector> for GpuSelector {
	fn from(selector: wgpaper_config::GpuSelector) -> Self {
		Self {
			index: selector.index,
			name_substring: selector.name_substring,
			device_type: selector
				.device_type
				.map(|device_type| DeviceType::from(device_type)),
		}
	}
}

impl Default for GpuSelector {
	fn default() -> Self {
		Self::from(wgpaper_config::GpuSelector::default())
	}
}
