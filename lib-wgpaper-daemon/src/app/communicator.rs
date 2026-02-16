use crate::{Commands, LaunchOptions, image_wrapper::ImageWrapper, start};
use calloop::channel::{Sender, channel};
use log::{debug, info};
use std::thread::{self, JoinHandle};

pub struct SCTKCommunicator {
	sender: Sender<Commands>,
	sctk_thread: Option<JoinHandle<()>>,
}

impl SCTKCommunicator {
	pub fn new(options: LaunchOptions) -> Self {
		let (sender, channel) = channel::<Commands>();
		let handle = thread::spawn(move || {
			start(channel, options).unwrap();
		});

		Self {
			sender,
			sctk_thread: Some(handle),
		}
	}

	pub fn shutdown(&mut self) -> anyhow::Result<()> {
		let _ = self.sender.send(Commands::Stop);

		if let Some(thread) = self.sctk_thread.take() {
			info!("Waiting for SCTK thread to exit...");
			thread
				.join()
				.map_err(|e| anyhow::anyhow!("SCTK thread panicked: {:?}.", e))?;
			info!("SCTK thread exited cleanly.");
		} else {
			debug!("SCTK thread was already stopped.");
		}
		Ok(())
	}

	pub fn start_transition_all(&self, image: ImageWrapper) -> anyhow::Result<()> {
		let command = Commands::StartTransitionAll { image };
		anyhow::Ok(self.sender.send(command)?)
	}
}

impl Drop for SCTKCommunicator {
	fn drop(&mut self) {
		let _ = self.shutdown();
	}
}
