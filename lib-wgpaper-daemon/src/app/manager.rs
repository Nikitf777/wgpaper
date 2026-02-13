use std::sync::Arc;

use wgpaper_config::Config;

use crate::{
	app::communicator::AppCommunicator, image_wrapper::ImageWrapper,
	utilities::random_file::RandomFileSelector,
};

pub struct AppManager {
	communicator: AppCommunicator,
	image_selector: RandomFileSelector,
	next_image: Option<ImageWrapper>,
}

impl AppManager {
	pub fn try_new(config: Arc<Config>) -> anyhow::Result<Self> {
		let mut selector = RandomFileSelector::new(
			config.wallpaper_directories().to_vec(),
			config.image_extensions().to_vec(),
		);
		selector.refresh_matching_files()?;
		let next_image = ImageWrapper::from_path(&selector.pick_next()?)?;

		Ok(Self {
			communicator: AppCommunicator::new(config.clone()),
			image_selector: selector,
			next_image: Some(next_image),
		})
	}

	pub fn start_transition_all_random(&mut self) -> anyhow::Result<()> {
		self.communicator.start_transition_all(
			self.next_image
				.take()
				.expect("next_image should always be populated"),
		)?;

		self.next_image = Some(ImageWrapper::from_path(&self.image_selector.pick_next()?)?);
		Ok(())
	}
}
