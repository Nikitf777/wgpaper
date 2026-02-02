use const_format::formatcp;
use serde::Deserialize;
use std::{
	env, fs,
	path::{Path, PathBuf},
};

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, strum_macros::Display)]
pub enum ScalingStrategy {
	Stretch,
	#[default]
	Fit,
	Cover,
	Center,
}

#[derive(Deserialize, Debug, Clone)]
pub enum ListenSocket {
	IP { address: String },
	UDS { path: PathBuf },
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpuSelector {
	pub index: Option<usize>,
	pub name_substring: Option<String>,
	pub device_type: Option<DeviceType>,
}

impl Default for GpuSelector {
	fn default() -> Self {
		Self {
			index: Some(0),
			name_substring: None,
			device_type: None,
		}
	}
}

#[derive(Debug, Clone, Default, Deserialize)]
pub enum DeviceType {
	Other,
	#[default]
	IntegratedGpu,
	DiscreteGpu,
	VirtualGpu,
	Cpu,
}

#[derive(Deserialize)]
pub struct Config {
	animation_shader: Option<PathBuf>,
	initial_wallpaper: Option<PathBuf>,
	wallpaper_directories: Option<Vec<PathBuf>>,
	image_extensions: Option<Vec<String>>,
	scaling_strategy: Option<ScalingStrategy>,
	listen_socket: Option<ListenSocket>,
	gpu: Option<GpuSelector>,
}

impl Config {
	pub const APP_NAME: &str = "wgpaper";
	pub const CONFIG_FILE_NAME: &str = "config.json";
	pub const GLOBAL_CONFIG_FILE_PATH: &str =
		formatcp!("/etc/{}/{}", Config::APP_NAME, Config::CONFIG_FILE_NAME);

	pub fn new() -> anyhow::Result<Self> {
		let local_config_path = Self::get_local_config_path()?;
		let config_file = if Path::new(&local_config_path).exists() {
			fs::read(local_config_path)?
		} else {
			fs::read(Config::GLOBAL_CONFIG_FILE_PATH)?
		};
		Ok(serde_json::from_slice(&config_file)?)
	}

	fn get_local_config_path() -> anyhow::Result<String> {
		let config_dir =
			env::var("XDG_CONFIG_HOME").unwrap_or(format!("{}/.config", env::var("HOME")?));

		Ok(format!(
			"{}/{}/{}",
			config_dir,
			Config::APP_NAME,
			Config::CONFIG_FILE_NAME
		))
	}

	/// Returns the animation shader path if configured
	pub fn animation_shader(&self) -> Option<&Path> {
		self.animation_shader.as_deref()
	}

	/// Returns the initial wallpaper path if configured
	pub fn initial_wallpaper(&self) -> Option<&Path> {
		self.initial_wallpaper.as_deref()
	}

	/// Returns wallpaper directories if configured
	pub fn wallpaper_directories(&self) -> Option<&[PathBuf]> {
		self.wallpaper_directories.as_deref()
	}

	/// Returns allowed image extensions if configured
	pub fn image_extensions(&self) -> Option<&[String]> {
		self.image_extensions.as_deref()
	}

	/// Returns the scaling strategy if configured
	pub fn scaling_strategy(&self) -> Option<ScalingStrategy> {
		self.scaling_strategy.clone()
	}

	/// Returns the listen socket configuration if set
	pub fn listen_socket(&self) -> Option<&ListenSocket> {
		self.listen_socket.as_ref()
	}

	/// Returns the GPU configuration if set
	pub fn gpu(&self) -> Option<&GpuSelector> {
		self.gpu.as_ref()
	}
}
