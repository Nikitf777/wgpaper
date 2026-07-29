use anyhow::Context;
use pollster::FutureExt;
use wgpaper_config::ScalingMode;
use wgpu::{
	Adapter, BindGroupLayout, Device, Instance, Queue, Sampler, ShaderModule,
};

use crate::renderer::{
	self,
	wgpu::{
		wgpu_selector::{self, WgpuSelector},
		wgpu_uniforms::per_frame_bind_group_layout,
		wgpu_utilities,
	},
};

/// Wraps a single GPU adapter + device with all the resources that are
/// shared across all surfaces using this GPU.
///
/// Shared resources include:
/// - The vertex shader (used by all pipelines)
/// - Two samplers (repeat / mirror-repeat) for texture wrapping
/// - The per-frame uniform bind-group layout (needed when building pipelines)
pub struct GpuDevice {
	pub adapter: Adapter,
	pub device: Device,
	pub queue: Queue,
	pub vertex_shader: ShaderModule,
	pub repeat_sampler: Sampler,
	pub mirror_repeat_sampler: Sampler,
	pub per_frame_bind_group_layout: BindGroupLayout,
}

impl GpuDevice {
	/// Select a GPU matching `gpu_selector`, request a device, and create
	/// all shared resources.
	pub fn new(instance: &Instance, gpu_selector: &renderer::GpuSelector) -> anyhow::Result<Self> {
		let wgpu_selector = WgpuSelector::from(gpu_selector.clone());
		let adapter = pollster::block_on(wgpu_selector::select_gpu(instance, wgpu_selector))
			.unwrap_or(pollster::block_on(wgpu_selector::select_gpu(
				instance,
				WgpuSelector::default(),
			))?);

		let (device, queue) = adapter
			.request_device(&wgpu::DeviceDescriptor::default())
			.block_on()
			.context("Failed to request device")?;

		let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
			label: Some("vertex_shader"),
			source: wgpu::ShaderSource::Wgsl(include_str!("shaders/vertex.wgsl").into()),
		});

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
}
