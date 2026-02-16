use crate::{
	LaunchOptions,
	app::{core::WallpaperState, output::OutputManager},
	image_wrapper::ImageWrapper,
	transition::ActiveTransition,
};
use anyhow::Context;
use smithay_client_toolkit::{
	compositor::{CompositorHandler, CompositorState},
	delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
	delegate_shm,
	output::{OutputHandler, OutputState},
	registry::{ProvidesRegistryState, RegistryState},
	registry_handlers,
	seat::{Capability, SeatHandler, SeatState},
	shell::wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
	shm::{Shm, ShmHandler},
};
use wayland_client::protocol::wl_seat;
use wayland_client::{
	Connection, QueueHandle,
	globals::GlobalList,
	protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};

pub mod communicator;
pub mod core;
pub mod manager;
pub mod output;

pub struct SCTKState {
	registry_state: RegistryState,
	seat_state: SeatState,
	output_state: OutputState,
	shm: Shm,
	compositor_state: CompositorState,
	layer_shell: LayerShell,
	output_manager: OutputManager,
	qh: QueueHandle<SCTKState>,
	pub exit: bool,

	wallpaper_state: WallpaperState,
}

impl SCTKState {
	pub fn try_new(
		globals: GlobalList,
		qh: QueueHandle<Self>,
		options: LaunchOptions,
	) -> anyhow::Result<Self> {
		let registry_state = RegistryState::new(&globals);
		let seat_state = SeatState::new(&globals, &qh);
		let output_state = OutputState::new(&globals, &qh);
		let compositor_state = CompositorState::bind(&globals, &qh)?;
		let layer_shell = LayerShell::bind(&globals, &qh).context("layer shell not available")?;
		let shm = Shm::bind(&globals, &qh).context("wl_shm not available")?;

		let gpu_selector = options.gpu_selector.unwrap_or_default();

		Ok(Self {
			registry_state,
			seat_state,
			output_state,
			shm,
			compositor_state,
			layer_shell,
			output_manager: OutputManager::default(),
			qh,
			exit: false,

			wallpaper_state: WallpaperState {
				gpu_selector,
				shader_source: options.shader_source,
				current_image: options.initial_image,
				transition: ActiveTransition::default(),
				scaling_mode: options.scaling_mode,
			},
		})
	}

	pub fn start_transition_all(&mut self, image: ImageWrapper) -> anyhow::Result<()> {
		self.output_manager.start_transition(&self.qh, &image)?;
		self.wallpaper_state.transition.start();
		self.wallpaper_state.current_image = Some(image);
		Ok(())
	}
}

impl CompositorHandler for SCTKState {
	fn frame(
		&mut self,
		_conn: &Connection,
		qh: &QueueHandle<Self>,
		surface: &WlSurface,
		_time: u32,
	) {
		let progress = self.wallpaper_state.transition.progress();
		self.output_manager.frame(qh, surface, progress);
	}

	fn scale_factor_changed(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_surface: &WlSurface,
		_new_factor: i32,
	) {
	}

	fn transform_changed(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_surface: &WlSurface,
		_new_transform: wayland_client::protocol::wl_output::Transform,
	) {
	}

	fn surface_enter(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_surface: &WlSurface,
		_output: &wayland_client::protocol::wl_output::WlOutput,
	) {
	}

	fn surface_leave(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_surface: &WlSurface,
		_output: &wayland_client::protocol::wl_output::WlOutput,
	) {
	}
}

impl OutputHandler for SCTKState {
	fn output_state(&mut self) -> &mut OutputState {
		&mut self.output_state
	}

	fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
		self.output_manager.handle_new_output(
			&qh,
			&self.compositor_state,
			&self.layer_shell,
			&output,
		);
	}

	fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

	fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
		self.output_manager.handle_output_destroyed(output);
	}
}

impl LayerShellHandler for SCTKState {
	fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
		self.output_manager.handle_layer_surface_closed(layer);
	}

	fn configure(
		&mut self,
		conn: &Connection,
		qh: &QueueHandle<Self>,
		layer: &LayerSurface,
		configure: LayerSurfaceConfigure,
		_serial: u32,
	) {
		self.output_manager
			.handle_configure(conn, qh, layer, &configure, &self.wallpaper_state);
	}
}

impl SeatHandler for SCTKState {
	fn seat_state(&mut self) -> &mut SeatState {
		&mut self.seat_state
	}

	fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

	fn new_capability(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_seat: wl_seat::WlSeat,
		_capability: Capability,
	) {
	}

	fn remove_capability(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_seat: wl_seat::WlSeat,
		_capability: Capability,
	) {
	}

	fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {
	}
}

impl ShmHandler for SCTKState {
	fn shm_state(&mut self) -> &mut Shm {
		&mut self.shm
	}
}

delegate_compositor!(SCTKState);
delegate_output!(SCTKState);
delegate_seat!(SCTKState);
delegate_registry!(SCTKState);
delegate_shm!(SCTKState);
delegate_layer!(SCTKState);

impl ProvidesRegistryState for SCTKState {
	fn registry(&mut self) -> &mut RegistryState {
		&mut self.registry_state
	}
	registry_handlers![OutputState, SeatState];
}
