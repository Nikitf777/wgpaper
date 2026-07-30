use std::rc::Rc;

use log::warn;
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use wayland_client::Connection;

use wgpaper_config::GpuSelector;

use crate::{
	image_wrapper::ImageWrapper,
	renderer::{
		RendererOptions,
		wgpu::{
			wgpu_device::GpuDevice, wgpu_selector::WgpuSelector, wgpu_surface::SurfaceRenderer,
			wgpu_utilities::create_surface,
		},
	},
};

/// Top-level manager for all GPU devices and their surfaces.
///
/// Owns the wgpu `Instance` (needed to create surfaces) and a collection
/// of [`GpuDevice`]s.  When a new output appears, [`RenderManager::create_surface`]
/// selects (or creates) the appropriate GPU device, builds a wgpu `Surface`,
/// and returns a [`SurfaceRenderer`] that the output can use for rendering.
pub struct RenderManager {
	instance: wgpu::Instance,
	devices: Vec<Rc<GpuDevice>>,
}

impl RenderManager {
	/// Create a new manager with a default wgpu instance (PRIMARY backends).
	pub fn new() -> Self {
		let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
			backends: wgpu::Backends::PRIMARY,
			..Default::default()
		});
		Self {
			instance,
			devices: Vec::new(),
		}
	}

	/// Convenience constructor for tests / non-wgpu contexts.
	pub fn from_instance(instance: wgpu::Instance) -> Self {
		Self {
			instance,
			devices: Vec::new(),
		}
	}

	// ── device lookup / creation ────────────────────────────────

	/// Find an existing device that matches `selector`, or create a new one.
	fn get_or_create_device(
		&mut self,
		gpu_selector: &GpuSelector,
	) -> anyhow::Result<Rc<GpuDevice>> {
		let renderer_sel = crate::renderer::GpuSelector::from(gpu_selector.clone());
		let wgpu_sel = WgpuSelector::from(renderer_sel.clone());

		// Reuse an existing matching device.
		for dev in &self.devices {
			if dev.matches(&wgpu_sel) {
				return Ok(Rc::clone(dev));
			}
		}

		// Create a brand-new device.
		let device = GpuDevice::new(&self.instance, &renderer_sel)?;
		let device = Rc::new(device);
		self.devices.push(Rc::clone(&device));
		Ok(device)
	}

	// ── surface creation ────────────────────────────────────────

	/// Create a [`SurfaceRenderer`] for one output.
	///
	/// Internally this finds (or creates) a [`GpuDevice`] matching
	/// `options.gpu_selector`, builds a wgpu `Surface`, and wraps
	/// everything in a [`SurfaceRenderer`] that the output can use to
	/// render frames.
	pub fn create_surface(
		&mut self,
		conn: &Connection,
		layer_surface: &LayerSurface,
		size: (u32, u32),
		options: &RendererOptions,
	) -> anyhow::Result<SurfaceRenderer> {
		let device = self.get_or_create_device(options.gpu_selector)?;
		let surface = create_surface(&self.instance, conn, layer_surface)?;
		SurfaceRenderer::new(device, surface, size, options)
	}

	// ── broadcast helpers ───────────────────────────────────────

	/// Tell every surface on every device to switch to a new wallpaper image.
	///
	/// This is called when a "set-wallpaper" command is received.
	/// Currently a no-op placeholder – the caller should iterate over
	/// surfaces and call [`SurfaceRenderer::set_next_image`] on each.
	pub fn set_next_image_for_all(&self, _image: &ImageWrapper) {
		// TODO: store a list of SurfaceRenderers (or have the output manager
		//       iterate them) so we can call set_next_image on each.
		warn!("RenderManager::set_next_image_for_all is not yet wired up");
	}
}
