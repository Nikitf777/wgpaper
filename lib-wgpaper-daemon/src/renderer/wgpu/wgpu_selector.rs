use std::fmt;
use wgpu::{Adapter, AdapterInfo, Backends, DeviceType, Instance};

use crate::renderer;

/// Selection criteria for GPU adapters
#[derive(Debug, Clone)]
pub struct WgpuSelector {
	/// Select by positional index in adapter list (0 = first)
	pub index: Option<usize>,
	/// Case-insensitive substring match against adapter name
	pub name_substring: Option<String>,
	/// Filter by physical device type (Integrated/Discrete/Virtual/CPU)
	pub device_type: Option<DeviceType>,
	/// Optional backend restriction (Vulkan, Metal, DX12, etc.)
	pub backends: Option<Backends>,
}

impl From<crate::renderer::DeviceType> for DeviceType {
	#[inline]
	fn from(src: crate::renderer::DeviceType) -> Self {
		match src {
			crate::renderer::DeviceType::Other => DeviceType::Other,
			crate::renderer::DeviceType::IntegratedGpu => DeviceType::IntegratedGpu,
			crate::renderer::DeviceType::DiscreteGpu => DeviceType::DiscreteGpu,
			crate::renderer::DeviceType::VirtualGpu => DeviceType::VirtualGpu,
			crate::renderer::DeviceType::Cpu => DeviceType::Cpu,
		}
	}
}

impl From<renderer::GpuSelector> for WgpuSelector {
	fn from(selector: renderer::GpuSelector) -> Self {
		Self {
			index: selector.index,
			name_substring: selector.name_substring,
			device_type: selector
				.device_type
				.map(|device_type| DeviceType::from(device_type)),
			backends: None,
		}
	}
}

impl Default for WgpuSelector {
	fn default() -> Self {
		Self::from(renderer::GpuSelector::default())
	}
}

impl WgpuSelector {
	/// Create a new selector with default settings (all backends, no filters)
	pub fn new() -> Self {
		Self::default()
	}

	/// Select adapter at specific index (0-based)
	pub fn with_index(mut self, index: usize) -> Self {
		self.index = Some(index);
		self
	}

	/// Filter by case-insensitive name substring (e.g., "nvidia", "intel")
	pub fn with_name(mut self, name: &str) -> Self {
		self.name_substring = Some(name.to_lowercase());
		self
	}

	/// Filter by physical device type
	pub fn with_device_type(mut self, device_type: DeviceType) -> Self {
		self.device_type = Some(device_type);
		self
	}

	/// Restrict to specific graphics backends
	pub fn with_backends(mut self, backends: Backends) -> Self {
		self.backends = Some(backends);
		self
	}

	/// Check if adapter matches all specified criteria
	fn matches(&self, adapter: &Adapter) -> bool {
		let info = adapter.get_info();

		// Name filter (case-insensitive substring)
		if let Some(ref substr) = self.name_substring {
			if !info.name.to_lowercase().contains(substr) {
				return false;
			}
		}

		// Device type filter
		if let Some(device_type) = self.device_type {
			if info.device_type != device_type {
				return false;
			}
		}

		true
	}
}

/// Select GPU adapter based on provided criteria
///
/// # Arguments
/// * `instance` - wgpu instance to enumerate adapters from
/// * `selector` - Selection criteria (index/name/type/backends)
///
/// # Returns
/// * `Ok(adapter)` - Matching adapter
/// * `Err(SelectionError)` - Failure reason (no adapters, index out of bounds, no matches)
pub async fn select_gpu(
	instance: &Instance,
	selector: WgpuSelector,
) -> Result<Adapter, SelectionError> {
	let backends = selector.backends.unwrap_or(Backends::all());
	let adapters: Vec<Adapter> = instance.enumerate_adapters(backends).await;

	if adapters.is_empty() {
		return Err(SelectionError::NoAdaptersAvailable {
			requested_backends: backends,
		});
	}

	// Handle index-based selection (positional)
	if let Some(index) = selector.index {
		return match adapters.get(index) {
			Some(adapter) if selector.matches(adapter) => Ok(adapter.clone()),
			Some(_) => Err(SelectionError::IndexMismatch {
				index,
				name: adapters[index].get_info().name.clone(),
			}),
			None => Err(SelectionError::IndexOutOfBounds {
				index,
				available: adapters.len(),
			}),
		};
	}

	// Filter by name/type criteria
	for adapter in &adapters {
		if selector.matches(adapter) {
			return Ok(adapter.clone());
		}
	}

	// No matches found - provide diagnostic information
	Err(SelectionError::NoMatchingAdapter {
		criteria: selector,
		available_adapters: adapters.into_iter().map(|a| a.get_info()).collect(),
	})
}

/// Detailed error reporting for GPU selection failures
#[derive(Debug)]
pub enum SelectionError {
	NoAdaptersAvailable {
		requested_backends: Backends,
	},
	IndexOutOfBounds {
		index: usize,
		available: usize,
	},
	IndexMismatch {
		index: usize,
		name: String,
	},
	NoMatchingAdapter {
		criteria: WgpuSelector,
		available_adapters: Vec<AdapterInfo>,
	},
}

impl std::error::Error for SelectionError {}

impl fmt::Display for SelectionError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			SelectionError::NoAdaptersAvailable { requested_backends } => {
				write!(
					f,
					"No GPU adapters available for requested backends: {:?}",
					requested_backends
				)
			}
			SelectionError::IndexOutOfBounds { index, available } => {
				write!(
					f,
					"Adapter index {} out of bounds (only {} adapters available)",
					index, available
				)
			}
			SelectionError::IndexMismatch { index, name } => {
				write!(
					f,
					"Adapter at index {} ('{}') doesn't match selection criteria",
					index, name
				)
			}
			SelectionError::NoMatchingAdapter {
				criteria,
				available_adapters,
			} => {
				writeln!(f, "No adapter matched selection criteria: {:?}", criteria)?;
				writeln!(f, "\nAvailable adapters:")?;
				for (i, info) in available_adapters.iter().enumerate() {
					writeln!(
						f,
						"  [{i}] {} ({:?}, backend: {:?})",
						info.name, info.device_type, info.backend
					)?;
				}
				Ok(())
			}
		}
	}
}

// Convenience methods for common selection patterns
impl WgpuSelector {
	/// Prefer discrete GPU (common for performance-critical applications)
	pub fn discrete_gpu() -> Self {
		Self::new().with_device_type(DeviceType::DiscreteGpu)
	}

	/// Prefer integrated GPU (common for power efficiency)
	pub fn integrated_gpu() -> Self {
		Self::new().with_device_type(DeviceType::IntegratedGpu)
	}

	/// Select first available adapter (fallback strategy)
	pub fn first_available() -> Self {
		Self::new().with_index(0)
	}
}
