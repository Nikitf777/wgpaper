use std::path::Path;

use calloop::{EventLoop, channel::Channel};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use wayland_client::{Connection, globals::registry_queue_init};
use wgpaper_config::ScalingMode;

use crate::{app::App, image_wrapper::ImageWrapper};

pub mod app;
pub mod image_wrapper;
pub mod renderer;
pub mod transition;
pub mod utilities;

pub struct LaunchOptions<'a> {
	pub gpu_selector: Option<wgpaper_config::GpuSelector>,
	pub shader_path: Option<&'a Path>,
	pub initial_image_path: Option<&'a Path>,
	pub scaling_mode: Option<ScalingMode>,
}

pub struct PerOutputLaunchOptions {}

pub enum Commands {
	StartTransitionAll { image: ImageWrapper },
}

pub fn start(channel: Channel<Commands>, options: LaunchOptions) -> anyhow::Result<()> {
	let conn = Connection::connect_to_env()?;
	let (globals, event_queue) = registry_queue_init(&conn)?;
	let qh = event_queue.handle();

	let mut event_loop = EventLoop::<App>::try_new().unwrap();

	let loop_handle = event_loop.handle();
	loop_handle
		.insert_source(channel, |e, _, app| match e {
			calloop::channel::Event::Msg(command) => match command {
				Commands::StartTransitionAll { image } => {
					app.start_transition_all(image);
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
