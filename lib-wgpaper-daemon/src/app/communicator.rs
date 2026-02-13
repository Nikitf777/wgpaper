use crate::{
	Commands, LaunchOptions, image_wrapper::ImageWrapper, start,
	utilities::random_file::select_random_file,
};
use calloop::channel::{Sender, channel};
use std::{
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
			let path = select_random_file(
				&config.wallpaper_directories(),
				&config.image_extensions(),
				&[] as &[&str],
			)
			.expect("failed to select random wallpaper");

			let options = LaunchOptions {
				gpu_selector: config_for_thread.gpu().cloned(),
				shader_path: config_for_thread.shader(),
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

	pub fn start_transition_all(&mut self, image: ImageWrapper) -> anyhow::Result<()> {
		let command = Commands::StartTransitionAll { image };
		anyhow::Ok(self.sender.send(command)?)
	}
}
