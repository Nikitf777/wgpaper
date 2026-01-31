use crate::{
	renderer::{Renderer, wgpu_renderer::WgpuRenderer},
	transition::{Transition, TransitionProgress},
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
use std::{collections::HashMap, fs, num::NonZeroU32, path::Path, time::Instant};
use wayland_client::protocol::wl_seat;
use wayland_client::{
	Connection, QueueHandle,
	globals::{GlobalList, registry_queue_init},
	protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};

#[derive(serde::Deserialize)]
pub enum Commands {
	StartTransition { image_path: String },
}

pub fn start(
	channel: Channel<Commands>,
	animation_shader: &Path,
	initial_image_path: &Path,
) -> anyhow::Result<()> {
	let conn = Connection::connect_to_env()?;
	let (globals, event_queue) = registry_queue_init(&conn)?;
	let qh = event_queue.handle();

	let mut event_loop = EventLoop::<App>::try_new().unwrap();

	let loop_handle = event_loop.handle();
	loop_handle
		.insert_source(channel, |e, _, app| match e {
			calloop::channel::Event::Msg(command) => match command {
				Commands::StartTransition { image_path } => {
					app.start_transition(image_path);
				}
			},
			calloop::channel::Event::Closed => todo!(),
		})
		.unwrap();

	WaylandSource::new(conn, event_queue)
		.insert(loop_handle)
		.unwrap();

	let mut app = App::new(globals, qh, animation_shader, initial_image_path)?;

	event_loop.run(None, &mut app, |_| {}).unwrap();

	Ok(())
}

struct OutputStateEntry {
	output: WlOutput,
	layer: LayerSurface,
	renderer: Option<Box<dyn Renderer>>,
	width: u32,
	height: u32,
}

impl OutputStateEntry {
	fn render(&mut self) {
		if self.width == 0 || self.height == 0 {
			return;
		}

		if let Some(renderer) = &mut self.renderer {
			if let Err(e) = renderer.render() {
				eprintln!("Rendering error: {}", e);
			}
		}

		self.layer.wl_surface().commit();
	}

	fn init_renderer(
		&mut self,
		conn: &Connection,
		animation_shader: &str,
		initial_image: &[u8],
	) -> anyhow::Result<()> {
		let renderer = WgpuRenderer::new(
			conn,
			&self.layer,
			self.width,
			self.height,
			animation_shader,
			initial_image,
		)?;
		self.renderer = Some(Box::new(renderer));
		Ok(())
	}

	fn resize(&mut self, width: u32, height: u32) {
		if width == 0 || height == 0 {
			return;
		}

		self.width = width;
		self.height = height;

		if let Some(renderer) = &mut self.renderer {
			if let Err(e) = renderer.resize(width, height) {
				eprintln!("Resize error: {}", e);
			}
		}
	}

	fn is_transitioning(&self) -> bool {
		if let Some(renderer) = &self.renderer {
			!renderer.get_transition_progress().is_finished()
		} else {
			false
		}
	}

	fn set_transition_progress(&mut self, progress: TransitionProgress) {
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.set_transition_progress(progress);
		}
	}

	fn set_next_image(&mut self, rgba8: &[u8], dimensions: (u32, u32)) {
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.set_next_image(rgba8, dimensions);
		}
	}
}

pub struct App {
	animation_shader: String,
	initial_imgae: Vec<u8>,
	registry_state: RegistryState,
	seat_state: SeatState,
	output_state: OutputState,
	shm: Shm,
	compositor_state: CompositorState,
	layer_shell: LayerShell,
	outputs: HashMap<WlSurface, OutputStateEntry>,
	pub exit: bool,
	qh: QueueHandle<App>,
	transition_begin: Instant,
	transition: Transition,
}

impl App {
	pub fn new(
		globals: GlobalList,
		qh: QueueHandle<Self>,
		animation_shader: &Path,
		initial_image_path: &Path,
	) -> anyhow::Result<Self> {
		let animation_shader = fs::read_to_string(animation_shader)?;
		let image = fs::read(initial_image_path)?;
		let registry_state = RegistryState::new(&globals);
		let seat_state = SeatState::new(&globals, &qh);
		let output_state = OutputState::new(&globals, &qh);
		let compositor_state = CompositorState::bind(&globals, &qh)?;
		let layer_shell = LayerShell::bind(&globals, &qh).context("layer shell not available")?;
		let shm = Shm::bind(&globals, &qh).context("wl_shm not available")?;

		Ok(Self {
			animation_shader,
			initial_imgae: image,
			registry_state,
			seat_state,
			output_state,
			shm,
			compositor_state,
			layer_shell,
			outputs: HashMap::new(),
			exit: false,
			qh,
			transition_begin: Instant::now(),
			transition: Transition::new(1.0, (0.54, 0.0, 0.34, 0.99)),
		})
	}

	fn render_all(&mut self, qh: &QueueHandle<Self>) {
		for (_, entry) in self.outputs.iter_mut() {
			entry
				.layer
				.wl_surface()
				.frame(qh, entry.layer.wl_surface().clone());
			entry.render();
		}
	}

	pub fn start_transition(&mut self, image_path: String) {
		let bytes = fs::read(image_path).unwrap();
		let img = image::load_from_memory(&bytes).unwrap();
		let rgba = img.into_rgba8();
		let dimansions = rgba.dimensions();
		self.transition_begin = Instant::now();
		for (surface, entry) in self.outputs.iter_mut() {
			entry.set_next_image(&rgba, dimansions);
			entry.set_transition_progress(TransitionProgress::reset());
			surface.frame(&self.qh, surface.clone());
			entry.render();
		}
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
			if !output.is_transitioning() {
				return;
			}
			let progress = self
				.transition
				.advance_to(Instant::now().duration_since(self.transition_begin));
			if !progress.is_finished() {
				surface.frame(qh, surface.clone());
			}
			output.set_transition_progress(progress);
			output.render();
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

		self.outputs.insert(
			surface,
			OutputStateEntry {
				output,
				layer,
				renderer: None,
				width: 0,
				height: 0,
			},
		);
	}

	fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

	fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
		if let Some((_, _)) = self
			.outputs
			.extract_if(|_, entry| entry.output == _output)
			.next()
		{
			// TODO: log that the output was removed.
		}
	}
}

impl LayerShellHandler for App {
	fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
		self.outputs.retain(|_, entry| &entry.layer != layer);
	}

	fn configure(
		&mut self,
		conn: &Connection,
		qh: &QueueHandle<Self>,
		layer: &LayerSurface,
		configure: smithay_client_toolkit::shell::wlr_layer::LayerSurfaceConfigure,
		_serial: u32,
	) {
		if let Some(entry) = self.outputs.values_mut().find(|e| &e.layer == layer) {
			let (new_width, new_height) = configure.new_size;
			entry.width = NonZeroU32::new(new_width).map_or(256, NonZeroU32::get);
			entry.height = NonZeroU32::new(new_height).map_or(256, NonZeroU32::get);

			entry.resize(entry.width, entry.height);

			if entry.renderer.is_none() {
				if let Err(e) =
					entry.init_renderer(conn, &self.animation_shader, &self.initial_imgae)
				{
					eprintln!("Renderer init failed: {}", e);
				}
			}

			self.render_all(qh);
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
