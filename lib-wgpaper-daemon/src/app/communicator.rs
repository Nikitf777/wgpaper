use crate::{Commands, LaunchOptions, image_wrapper::ImageWrapper, start};
use calloop::channel::{Sender, channel};
use log::{debug, error, info};
use std::thread::{self, JoinHandle};

pub struct SctkCommunicator {
	sender: Sender<Commands>,
	sctk_thread: Option<JoinHandle<()>>,
}

impl SctkCommunicator {
	pub fn new(options: LaunchOptions) -> Self {
		let (sender, channel) = channel::<Commands>();
		let handle = thread::spawn(move || {
			// The event loop returns when the compositor goes away, which must
			// not be a panic: the daemon is expected to notice and shut down.
			if let Err(err) = start(channel, options) {
				error!("The SCTK event loop stopped: {:?}", err);
			}
		});

		Self {
			sender,
			sctk_thread: Some(handle),
		}
	}

	/// Whether the SCTK thread is gone, i.e. the compositor is no longer there.
	pub fn is_finished(&self) -> bool {
		self.sctk_thread
			.as_ref()
			.map(thread::JoinHandle::is_finished)
			.unwrap_or(true)
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

impl Drop for SctkCommunicator {
	fn drop(&mut self) {
		let _ = self.shutdown();
	}
}
