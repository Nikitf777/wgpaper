use crate::{Commands, LaunchOptions, image_wrapper::ImageWrapper, start};
use calloop::channel::{Sender, channel};
use std::thread::{self, JoinHandle};

pub struct AppCommunicator {
	sender: Sender<Commands>,
	app_thread: JoinHandle<()>,
}

impl AppCommunicator {
	pub fn new(options: LaunchOptions) -> Self {
		let (sender, channel) = channel::<Commands>();
		let handle = thread::spawn(move || {
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
