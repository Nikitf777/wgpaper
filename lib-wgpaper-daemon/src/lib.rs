use std::path::{Path, PathBuf};

use calloop::{EventLoop, channel::Channel};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use wayland_client::{Connection, globals::registry_queue_init};
use wgpaper_config::ScalingMode;

use crate::app::App;

pub mod app;
pub mod image_wrapper;
pub mod renderer;
pub mod transition;
pub mod utilities;

pub struct GlobalOptions<'a> {
	pub gpu_selector: Option<wgpaper_config::GpuSelector>,
	pub shader_path: Option<&'a Path>,
	pub initial_image_path: Option<&'a Path>,
	pub scaling_mode: Option<ScalingMode>,
}

pub struct PerOutputOptions {}

#[derive(serde::Deserialize)]
pub enum Commands {
	StartTransitionAll { image_path: PathBuf },
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
				Commands::StartTransitionAll { image_path } => {
					app.start_transition_all(&image_path);
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
