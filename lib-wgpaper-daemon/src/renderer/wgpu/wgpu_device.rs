use std::{cell::RefCell, collections::HashMap};

use anyhow::Context;
use pollster::FutureExt;
use wgpaper_config::ScalingMode;
use wgpu::{
	Adapter, BindGroup, BindGroupLayout, Device, Instance, Queue, Sampler, ShaderModule,
	TextureFormat, TextureView,
};

use crate::renderer::{
	self,
	wgpu::{
		wgpu_selector::{self, WgpuSelector},
		wgpu_shaders,
		wgpu_texture_scaler::WgpuTextureScaler,
		wgpu_uniforms::per_frame_bind_group_layout,
		wgpu_utilities,
	},
};

/// Flattened scaling mode that discards background variants.
/// There are at most 6 distinct pipelines (stretch, fit, cover, center)
/// regardless of the background-fill variant, so we use this as the cache key.
#[derive(Eq, Hash, PartialEq, Clone, Copy)]
enum ScalingModeFlat {
	Stretch,
	Fit,
	Cover,
	Center,
}

impl From<&ScalingMode> for ScalingModeFlat {
	fn from(value: &ScalingMode) -> Self {
		match value {
			ScalingMode::Stretch => ScalingModeFlat::Stretch,
			ScalingMode::Fit { background: _ } => ScalingModeFlat::Fit,
			ScalingMode::Cover => ScalingModeFlat::Cover,
			ScalingMode::Center { background: _ } => ScalingModeFlat::Center,
		}
	}
}

/// Wraps a single GPU adapter + device with all the resources that are
/// shared across all surfaces using this GPU.
///
/// Shared resources include:
/// - The vertex shader (used by all pipelines)
/// - Two samplers (repeat / mirror-repeat) for texture wrapping
/// - The per-frame uniform bind-group layout (needed when building pipelines)
/// - A lazily-populated cache of [`WgpuTextureScaler`]s (one per geometric
///   scaling mode, so at most 4).
pub struct GpuDevice {
	pub adapter: Adapter,
	pub device: Device,
	pub queue: Queue,
	pub vertex_shader: ShaderModule,
	pub repeat_sampler: Sampler,
	pub mirror_repeat_sampler: Sampler,
	pub per_frame_bind_group_layout: BindGroupLayout,
	texture_scalers: RefCell<HashMap<ScalingModeFlat, WgpuTextureScaler>>,
}

impl GpuDevice {
	/// Select a GPU matching `gpu_selector`, request a device, and create
	/// all shared resources.
	pub fn new(instance: &Instance, gpu_selector: &renderer::GpuSelector) -> anyhow::Result<Self> {
		let wgpu_selector = WgpuSelector::from(gpu_selector.clone());
		let adapter =
			pollster::block_on(wgpu_selector::select_gpu(instance, wgpu_selector)).unwrap_or(
				pollster::block_on(wgpu_selector::select_gpu(instance, WgpuSelector::default()))?,
			);

		let (device, queue) = adapter
			.request_device(&wgpu::DeviceDescriptor::default())
			.block_on()
			.context("Failed to request device")?;

		let vertex = wgpu_shaders::create_spv_module(&device, "vertex_shader", wgpu_shaders::VS_ENTRY);
		let vertex_shader = vertex.module;

		let repeat_sampler = wgpu_utilities::create_sampler(&device, wgpu::AddressMode::Repeat);
		let mirror_repeat_sampler =
			wgpu_utilities::create_sampler(&device, wgpu::AddressMode::MirrorRepeat);

		let per_frame_bind_group_layout = per_frame_bind_group_layout(&device);

		Ok(Self {
			adapter,
			device,
			queue,
			vertex_shader,
			repeat_sampler,
			mirror_repeat_sampler,
			per_frame_bind_group_layout,
			texture_scalers: RefCell::new(HashMap::new()),
		})
	}

	/// Check whether this device matches the given selector.
	pub fn matches(&self, selector: &WgpuSelector) -> bool {
		selector.matches(&self.adapter)
	}

	/// Choose the appropriate sampler based on the scaling mode's background.
	///
	/// * `Stretch` / `Cover` → repeat-sampler (image fills the screen, edge
	///   wrapping is irrelevant).
	/// * `Fit` / `Center`    → the background variant decides:
	///   - `MirrorRepeat`    → mirror-repeat sampler
	///   - everything else   → repeat sampler
	pub fn choose_sampler(&self, scaling_mode: &ScalingMode) -> &Sampler {
		match scaling_mode {
			ScalingMode::Fit { background } | ScalingMode::Center { background } => {
				match background {
					wgpaper_config::Background::MirrorRepeat => &self.mirror_repeat_sampler,
					_ => &self.repeat_sampler,
				}
			}
			_ => &self.repeat_sampler,
		}
	}

	/// Scale `src_view` into `dst_view` using the scaler for `scaling_mode`.
	///
	/// The scaler pipeline is created on first use and then cached, so all
	/// surfaces sharing this device reuse the same pipeline for the same
	/// geometric scaling mode.
	pub fn scale_texture(
		&self,
		scaling_mode: &ScalingMode,
		format: TextureFormat,
		queue: &Queue,
		sampler: &Sampler,
		src_view: &TextureView,
		dst_view: &TextureView,
		per_frame_data_bind_group: &BindGroup,
	) {
		let flat = ScalingModeFlat::from(scaling_mode);
		let mut cache = self.texture_scalers.borrow_mut();
		let scaler = cache.entry(flat).or_insert_with(|| {
			WgpuTextureScaler::new(
				&self.device,
				&self.per_frame_bind_group_layout,
				&self.vertex_shader,
				scaling_mode.clone(),
				format,
			)
		});
		scaler.scale(
			&self.device,
			queue,
			sampler,
			src_view,
			dst_view,
			per_frame_data_bind_group,
		);
	}
}
