use crate::{
	app::output::OutputStateEntry,
	image_wrapper::ImageWrapper,
	renderer::GpuSelector,
	transition::{ActiveTransition, TransitionProgress},
};
use anyhow::Context;
use calloop::{EventLoop, channel::Channel};
use smithay_client_toolkit::{
	compositor::{CompositorHandler, CompositorState},
	delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
	delegate_shm,
	output::{OutputHandler, OutputState},
	reexports::calloop_wayland_source::WaylandSource,
	registry::{ProvidesRegistryState, RegistryState},
	registry_handlers,
	seat::{Capability, SeatHandler, SeatState},
	shell::{
		WaylandSurface,
		wlr_layer::{
			Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
		},
	},
	shm::{Shm, ShmHandler},
};
use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
};
use wayland_client::protocol::wl_seat;
use wayland_client::{
	Connection, QueueHandle,
	globals::{GlobalList, registry_queue_init},
	protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};
use wgpaper_config::ScalingMode;

pub mod output;

pub struct GlobalOptions<'a> {
	pub gpu_selector: Option<wgpaper_config::GpuSelector>,
	pub animation_shader_path: Option<&'a Path>,
	pub initial_image_path: Option<&'a Path>,
	pub scaling_mode: Option<ScalingMode>,
}

pub struct PerOutputOptions {}

#[derive(serde::Deserialize)]
pub enum Commands {
	StartTransition { image_path: PathBuf },
}

pub fn start(channel: Channel<Commands>, options: GlobalOptions) -> anyhow::Result<()> {
	let conn = Connection::connect_to_env()?;
	let (globals, event_queue) = registry_queue_init(&conn)?;
	let qh = event_queue.handle();

	let mut event_loop = EventLoop::<App>::try_new().unwrap();

	let loop_handle = event_loop.handle();
	loop_handle
		.insert_source(channel, |e, _, app| match e {
			calloop::channel::Event::Msg(command) => match command {
				Commands::StartTransition { image_path } => {
					app.start_transition(&image_path);
				}
			},
			calloop::channel::Event::Closed => todo!(),
		})
		.unwrap();

	WaylandSource::new(conn, event_queue)
		.insert(loop_handle)
		.unwrap();

	let mut app = App::new(globals, qh, options)?;

	event_loop.run(None, &mut app, |_| {}).unwrap();

	Ok(())
}

pub struct App {
	registry_state: RegistryState,
	seat_state: SeatState,
	output_state: OutputState,
	shm: Shm,
	compositor_state: CompositorState,
	layer_shell: LayerShell,
	outputs: HashMap<WlSurface, OutputStateEntry>,
	qh: QueueHandle<App>,
	pub exit: bool,

	gpu_selector: GpuSelector,
	animation_shader: String,
	current_wallpaper: ImageWrapper,
	transition: ActiveTransition,
	scaling_mode: ScalingMode,
}

impl App {
	pub fn new(
		globals: GlobalList,
		qh: QueueHandle<Self>,
		options: GlobalOptions,
	) -> anyhow::Result<Self> {
		let registry_state = RegistryState::new(&globals);
		let seat_state = SeatState::new(&globals, &qh);
		let output_state = OutputState::new(&globals, &qh);
		let compositor_state = CompositorState::bind(&globals, &qh)?;
		let layer_shell = LayerShell::bind(&globals, &qh).context("layer shell not available")?;
		let shm = Shm::bind(&globals, &qh).context("wl_shm not available")?;

		let gpu_selector = GpuSelector::from(options.gpu_selector.unwrap_or_default());
		let animation_shader = fs::read_to_string(
			options
				.animation_shader_path
				.expect("wgpaper can't run without a transition shader."),
		)?;
		let image = ImageWrapper::from_path(
			&options
				.initial_image_path
				.expect("wgpaper can't run without an initial image."),
		)?;

		Ok(Self {
			registry_state,
			seat_state,
			output_state,
			shm,
			compositor_state,
			layer_shell,
			outputs: HashMap::new(),
			qh,
			exit: false,
			gpu_selector,
			animation_shader,
			current_wallpaper: image,
			transition: ActiveTransition::default(),
			scaling_mode: options.scaling_mode.unwrap_or_default(),
		})
	}

	fn queue_render_all(&mut self, qh: &QueueHandle<Self>) {
		for (_, output) in self.outputs.iter_mut() {
			output.frame(qh);
			output.commit();
		}
	}

	pub fn start_transition(&mut self, image_path: &Path) -> anyhow::Result<()> {
		self.current_wallpaper = ImageWrapper::from_path(image_path)?;
		for (surface, output) in self.outputs.iter_mut() {
			output.set_next_image(&self.current_wallpaper);
			output.set_transition_progress(TransitionProgress::reset());
			surface.frame(&self.qh, surface.clone());
			output.commit();
		}
		self.transition.start();
		Ok(())
	}
}

impl CompositorHandler for App {
	fn frame(
		&mut self,
		_conn: &Connection,
		qh: &QueueHandle<Self>,
		surface: &WlSurface,
		_time: u32,
	) {
		if let Some(output) = self.outputs.get_mut(surface) {
			let progress = self.transition.progress();
			if !progress.is_finished() {
				surface.frame(qh, surface.clone());
			}
			if output.is_transitioning() {
				output.set_transition_progress(progress);
			}
			output.render();
			output.commit();
		}
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

impl OutputHandler for App {
	fn output_state(&mut self) -> &mut OutputState {
		&mut self.output_state
	}

	fn new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
		let surface = self.compositor_state.create_surface(qh);
		let layer = self.layer_shell.create_layer_surface(
			qh,
			surface.clone(),
			Layer::Background,
			Some("multi_output_layer"),
			Some(&output),
		);

		layer.set_anchor(Anchor::all());
		layer.set_exclusive_zone(-1);
		layer.set_keyboard_interactivity(KeyboardInteractivity::None);
		layer.set_size(0, 0);
		layer.commit();

		self.outputs
			.insert(surface, OutputStateEntry::new(output, layer));
	}

	fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

	fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
		if let Some((_, _)) = self
			.outputs
			.extract_if(|_, output| output.output() == &_output)
			.next()
		{
			// TODO: log that the output was removed.
		}
	}
}

impl LayerShellHandler for App {
	fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
		self.outputs.retain(|_, output| &output.layer() != &layer);
	}

	fn configure(
		&mut self,
		conn: &Connection,
		qh: &QueueHandle<Self>,
		layer: &LayerSurface,
		configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
		_serial: u32,
	) {
		if let Some(output) = self.outputs.values_mut().find(|e| &e.layer() == &layer) {
			output.resize(configure.new_size);

			if !output.is_initialized() {
				if let Err(e) = output.init_renderer(
					conn,
					self.gpu_selector.clone(),
					&self.animation_shader,
					&self.current_wallpaper,
					&self.scaling_mode,
				) {
					eprintln!("Renderer init failed: {}", e);
				}
			}

			self.queue_render_all(qh);
		}
	}
}

impl SeatHandler for App {
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

impl ShmHandler for App {
	fn shm_state(&mut self) -> &mut Shm {
		&mut self.shm
	}
}

delegate_compositor!(App);
delegate_output!(App);
delegate_seat!(App);
delegate_registry!(App);
delegate_shm!(App);
delegate_layer!(App);

impl ProvidesRegistryState for App {
	fn registry(&mut self) -> &mut RegistryState {
		&mut self.registry_state
	}
	registry_handlers![OutputState, SeatState];
}
