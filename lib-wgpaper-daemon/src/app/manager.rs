use std::sync::Arc;

use wgpaper_config::Config;

use crate::{
	app::communicator::AppCommunicator, image_wrapper::ImageWrapper,
	utilities::random_file::RandomFileSelector,
};

pub struct AppManager {
	communicator: AppCommunicator,
	image_selector: RandomFileSelector,
}

impl AppManager {
	pub fn try_new(config: Arc<Config>) -> anyhow::Result<Self> {
		let mut selector = RandomFileSelector::new(
			config.wallpaper_directories().to_vec(),
			config.image_extensions().to_vec(),
		);
		selector.update_matching_files()?;

		Ok(Self {
			communicator: AppCommunicator::new(config.clone()),
			image_selector: selector,
		})
	}

	pub fn start_transition_all_random(&mut self) -> anyhow::Result<()> {
		let path = self.image_selector.pick_random()?;
		self.communicator
			.start_transition_all(ImageWrapper::from_path(&path)?)
	}
}
