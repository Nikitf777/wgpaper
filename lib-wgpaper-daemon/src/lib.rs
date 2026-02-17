use calloop::{EventLoop, channel::Channel};
use log::{info, warn};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use wayland_client::{Connection, globals::registry_queue_init};
use wgpaper_config::ScalingMode;

use crate::{app::SctkState, image_wrapper::ImageWrapper};

pub mod app;
pub mod image_wrapper;
pub mod renderer;
pub mod transition;
pub mod utilities;

pub struct LaunchOptions {
	pub gpu_selector: Option<wgpaper_config::GpuSelector>,
	pub shader_source: Option<String>,
	pub initial_image: Option<ImageWrapper>,
	pub scaling_mode: ScalingMode,
}

pub struct PerOutputLaunchOptions {}

pub enum Commands {
	StartTransitionAll { image: ImageWrapper },
	Stop,
}

pub fn start(channel: Channel<Commands>, options: LaunchOptions) -> anyhow::Result<()> {
	info!("Connecting to a Wayland server...");
	let conn = Connection::connect_to_env()?;
	info!("Connected to the Wayland server.");

	info!("Initializing an event queue...");
	let (globals, event_queue) = registry_queue_init(&conn)?;
	info!("Initialized the event queue.");

	let qh = event_queue.handle();

	info!("Initializing an event loop...");
	let mut event_loop = EventLoop::<SctkState>::try_new()?;
	info!("Initialized the event loop.");

	let loop_signal = event_loop.get_signal();

	let loop_handle = event_loop.handle();
	loop_handle
		.insert_source(channel, move |e, _, app| match e {
			calloop::channel::Event::Msg(command) => match command {
				Commands::StartTransitionAll { image } => {
					app.start_transition_all(image);
				}
				Commands::Stop => {
					info!("Stop command received, terminating event loop.");
					loop_signal.stop();
				}
			},
			calloop::channel::Event::Closed => {
				warn!("Command channel closed unexpectedly.");
				loop_signal.stop();
			}
		})
		.unwrap();

	WaylandSource::new(conn, event_queue)
		.insert(loop_handle)
		.unwrap();

	let mut app = SctkState::try_new(globals, qh, options)?;

	info!("Starting the event loop...");
	event_loop.run(None, &mut app, |_| {})?;
	info!("Event loop stopped.");

	Ok(())
}
