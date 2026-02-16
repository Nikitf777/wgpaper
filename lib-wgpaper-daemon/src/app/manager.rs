use crate::{
	LaunchOptions,
	app::communicator::AppCommunicator,
	image_wrapper::{ImageWrapper, ImageWrapperError},
	utilities::random_file::{RandomFileError, RandomFileSelector},
};
use anyhow::Context;
use log::warn;
use std::fs;
use thiserror::Error;
use wgpaper_config::Config;

#[derive(Debug, Error)]
pub enum AppManagerError {
	#[error("Failed to pick the next file.")]
	RandomFileError { error: RandomFileError },

	#[error("Failed to decode an image: {error}")]
	ImageError { error: ImageWrapperError },
}

pub type AppManagerResult = Result<(), AppManagerError>;

fn pick_next_image(
	file_selector: &mut RandomFileSelector,
) -> Result<ImageWrapper, AppManagerError> {
	match file_selector.pick_next() {
		Ok(path) => {
			ImageWrapper::from_path(&path).map_err(|err| AppManagerError::ImageError { error: err })
		}
		Err(err) => Err(AppManagerError::RandomFileError { error: err }),
	}
}

pub struct AppManager {
	communicator: AppCommunicator,
	image_selector: RandomFileSelector,
	next_image: Option<ImageWrapper>,
}

impl AppManager {
	pub fn try_new(config: Config) -> anyhow::Result<Self> {
		let mut selector = RandomFileSelector::new(
			config.wallpaper_directories().to_vec(),
			config.image_extensions().to_vec(),
		);
		selector.refresh_matching_files()?;

		let initial_image = pick_next_image(&mut selector)
			.inspect_err(|err| {
				warn!("Failed to pick the initial image: {}", err.to_string());
			})
			.ok();
		let next_image = pick_next_image(&mut selector)
			.inspect_err(|err| {
				warn!("Failed to pick the next image: {}", err.to_string());
			})
			.ok();

		let shader_source = if let Some(shader_path) = config.shader() {
			fs::read_to_string(shader_path).ok()
		} else {
			None
		};

		let options = LaunchOptions {
			gpu_selector: config.gpu().cloned(),
			shader_source,
			initial_image: initial_image,
			scaling_mode: config.scaling_mode().clone(),
		};

		Ok(Self {
			communicator: AppCommunicator::new(options),
			image_selector: selector,
			next_image,
		})
	}

	pub fn shutdown(&mut self) -> anyhow::Result<()> {
		self.communicator.shutdown()
	}

	pub fn start_transition_all_random(&mut self) -> anyhow::Result<()> {
		match self
			.next_image
			.take()
			.context("The target image is not specified.")
		{
			Ok(image) => self.communicator.start_transition_all(image)?,
			Err(err) => {
				warn!("{}. Skipping starting the transition...", err.to_string())
			}
		}

		self.next_image = if let Ok(path) = &self.image_selector.pick_next() {
			match ImageWrapper::from_path(path) {
				Ok(image) => Some(image),
				Err(err) => {
					warn!("Failed to pick the next image: {}", err.to_string());
					None
				}
			}
		} else {
			None
		};

		Ok(())
	}
}
