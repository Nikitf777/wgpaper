use crate::{Commands, GlobalOptions, start, utilities::random_file::select_random_file};
use calloop::channel::{Sender, channel};
use std::{
	path::PathBuf,
	sync::Arc,
	thread::{self, JoinHandle},
};
use wgpaper_config::Config;

pub struct AppCommunicator {
	sender: Sender<Commands>,
	app_thread: JoinHandle<()>,
}

impl AppCommunicator {
	pub fn new(config: Arc<Config>) -> Self {
		let (sender, channel) = channel::<Commands>();
		let config_for_thread = config.clone();
		let handle = thread::spawn(move || {
			let directories = config_for_thread
				.wallpaper_directories()
				.expect("wallpaper_directories must be configured");

			let path = select_random_file(directories, &[".jpg", ".png"], &[] as &[&str])
				.expect("failed to select random wallpaper");

			let options = GlobalOptions {
				gpu_selector: config_for_thread.gpu().cloned(),
				animation_shader_path: config_for_thread.animation_shader(),
				initial_image_path: Some(&path),
				scaling_mode: config_for_thread.scaling_mode().cloned(),
			};

			start(channel, options).unwrap();
		});

		Self {
			sender,
			app_thread: handle,
		}
	}

	pub fn start_transition_all(&mut self, image_path: PathBuf) -> anyhow::Result<()> {
		let command = Commands::StartTransitionAll { image_path };
		anyhow::Ok(self.sender.send(command)?)
	}
}
