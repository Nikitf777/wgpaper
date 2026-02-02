use std::{fs, path::Path};

pub struct ImageWrapper {
	rgba8: Vec<u8>,
	width: u32,
	height: u32,
}

impl ImageWrapper {
	pub fn from_path(path: &Path) -> anyhow::Result<Self> {
		let bytes = fs::read(path)?;
		let image = image::load_from_memory(&bytes)?;
		let rgba8 = image.to_rgba8();
		let dimensions = rgba8.dimensions();
		Ok(Self {
			rgba8: rgba8.to_vec(),
			width: dimensions.0,
			height: dimensions.1,
		})
	}

	pub fn as_slice(&self) -> &[u8] {
		&self.rgba8
	}

	pub fn width(&self) -> u32 {
		self.width
	}

	pub fn height(&self) -> u32 {
		self.height
	}
}
