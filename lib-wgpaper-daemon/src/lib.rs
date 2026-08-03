use std::{collections::HashMap, fs};

use calloop::{EventLoop, channel::Channel};
use log::{info, warn};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use wayland_client::{Connection, globals::registry_queue_init};
use wgpaper_config::{ScalingMode, ShaderConfig};

use crate::{app::SctkState, image_wrapper::ImageWrapper};

pub mod app;
pub mod image_wrapper;
pub mod renderer;
pub mod transition;
pub mod utilities;

/// Animation shader(s) resolved at startup time by reading the configured files.
///
/// Mirrors `ShaderConfig` but holds the file *contents* rather than paths.
#[derive(Debug, Clone)]
pub enum RuntimeShaderConfig {
	Global(Option<String>),
	PerMonitor(HashMap<String, String>),
}

impl Default for RuntimeShaderConfig {
	fn default() -> Self {
		RuntimeShaderConfig::Global(None)
	}
}

impl RuntimeShaderConfig {
	/// Read the configured shader file(s) into memory.
	pub fn from_config(config: Option<&ShaderConfig>) -> Self {
		match config {
			None => RuntimeShaderConfig::Global(None),
			Some(ShaderConfig::Global(path)) => Self::Global(fs::read_to_string(path).ok()),
			Some(ShaderConfig::PerMonitor(paths)) => {
				let sources = paths
					.iter()
					.filter_map(|(name, path)| {
						fs::read_to_string(path).ok().map(|src| (name.clone(), src))
					})
					.collect();
				RuntimeShaderConfig::PerMonitor(sources)
			}
		}
	}

	/// Resolve the shader source for a given output name.
	///
	/// For `Global`, the configured source is returned regardless of the
	/// output name.  For `PerMonitor`, the source is returned only if a
	/// shader is configured for that output; otherwise `None` (fall back to
	/// the built-in transition shader).
	pub fn resolve_for_output(&self, output_name: Option<&str>) -> Option<&str> {
		match self {
			RuntimeShaderConfig::Global(source) => source.as_deref(),
			RuntimeShaderConfig::PerMonitor(map) => {
				output_name.and_then(|name| map.get(name)).map(|s| s.as_str())
			}
		}
	}
}

pub struct LaunchOptions {
	pub gpu: Option<wgpaper_config::GpuConfig>,
	pub shader: RuntimeShaderConfig,
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
