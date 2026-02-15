use crate::{image_wrapper::ImageWrapper, transition::TransitionProgress};
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_client::Connection;
use wgpaper_config::ScalingMode;

pub mod wgpu;

pub trait Renderer {
	fn new(
		conn: &Connection,
		layer_surface: &LayerSurface,
		size: (u32, u32),
		gpu_selector: wgpaper_config::GpuSelector,
		shader_source: Option<&str>,
		initial_image: Option<&ImageWrapper>,
		scaling_mode: &ScalingMode,
	) -> anyhow::Result<Self>
	where
		Self: Sized;

	fn render(&mut self) -> anyhow::Result<()>;

	fn resize(&mut self, size: (u32, u32)) -> anyhow::Result<()>;

	fn get_transition_progress(&self) -> TransitionProgress;

	fn set_transition_progress(&mut self, progress: TransitionProgress);

	fn set_next_image(&mut self, image: &ImageWrapper);
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
