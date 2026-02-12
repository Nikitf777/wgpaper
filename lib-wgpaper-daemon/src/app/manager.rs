use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use wgpaper_config::Config;

use crate::{app::communicator::AppCommunicator, utilities::random_file::select_random_file};

pub struct AppManager {
	communicator: AppCommunicator,
	wallpaper_directories: Vec<PathBuf>,
	prev_image_path: Option<PathBuf>,
}

impl AppManager {
	pub fn new(config: Arc<Config>) -> Self {
		let config_for_communicator = config.clone();
		Self {
			communicator: AppCommunicator::new(config_for_communicator),
			wallpaper_directories: config.wallpaper_directories().unwrap_or_default().to_vec(),
			prev_image_path: None,
		}
	}

	pub fn start_transition_all_random(&mut self) -> anyhow::Result<()> {
		let excluded_files = [self.prev_image_path.as_deref().unwrap_or(Path::new(""))];
		let path = select_random_file(
			&self.wallpaper_directories,
			&[".jpg", ".png"],
			&excluded_files,
		)
		.unwrap();

		self.prev_image_path = Some(path.clone());
		self.communicator.start_transition_all(path)
	}
}
