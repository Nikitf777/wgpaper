use std::{
	fs, io,
	path::{Path, PathBuf},
};

use image::ImageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImageWrapperError {
	#[error("Failed to read a file from path `{path}`: {error}")]
	FilesystemError { path: PathBuf, error: io::Error },

	#[error("Failed to decode an image: {error}")]
	ImageError { error: ImageError },
}

pub type ImageWrapperResult = Result<ImageWrapper, ImageWrapperError>;

pub struct ImageWrapper {
	rgba8: Vec<u8>,
	size: (u32, u32),
}

impl ImageWrapper {
	pub fn from_path(path: &Path) -> ImageWrapperResult {
		let bytes = fs::read(path).map_err(|err| ImageWrapperError::FilesystemError {
			path: path.to_path_buf(),
			error: err,
		})?;
		let image = image::load_from_memory(&bytes)
			.map_err(|err| ImageWrapperError::ImageError { error: err })?;
		let rgba8 = image.to_rgba8();
		let dimensions = rgba8.dimensions();
		Ok(Self {
			rgba8: rgba8.to_vec(),
			size: dimensions,
		})
	}

	pub fn from_rgba8(rgba8: Vec<u8>, size: (u32, u32)) -> Self {
		Self { rgba8, size }
	}

	pub fn as_slice(&self) -> &[u8] {
		&self.rgba8
	}

	pub fn width(&self) -> u32 {
		self.size.0
	}

	pub fn height(&self) -> u32 {
		self.size.1
	}

	pub fn dimensions(&self) -> (u32, u32) {
		self.size
	}
}
