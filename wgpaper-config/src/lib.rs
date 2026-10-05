use anyhow::Context;
use const_format::formatcp;
use serde::{Deserialize, Deserializer};
use shellexpand::tilde;
use std::{
	collections::HashMap,
	env, fs,
	path::{Path, PathBuf},
};

/// A `PathBuf` whose `~` is expanded to the home directory during
/// deserialization.  All path-typed config fields use this wrapper so the
/// expansion logic lives in a single place.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConfigPath(PathBuf);

impl<'de> Deserialize<'de> for ConfigPath {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		Ok(ConfigPath::from(s))
	}
}

impl From<String> for ConfigPath {
	fn from(s: String) -> Self {
		ConfigPath(get_path_from_string_expanded(s))
	}
}

impl AsRef<Path> for ConfigPath {
	fn as_ref(&self) -> &Path {
		&self.0
	}
}

impl std::ops::Deref for ConfigPath {
	type Target = Path;

	fn deref(&self) -> &Path {
		&self.0
	}
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderConfig {
	Global(ConfigPath),
	PerMonitor(HashMap<String, ConfigPath>),
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Background {
	AutoColor,
	CssColor(csscolorparser::Color),
	Repeat,
	MirrorRepeat,
}

#[derive(Deserialize, Clone, PartialEq, Default, strum_macros::Display)]
#[serde(rename_all = "snake_case")]
pub enum ScalingMode {
	Stretch,
	Fit {
		#[serde(default)]
		background: Background,
	},
	#[default]
	Cover,
	Center {
		#[serde(default)]
		background: Background,
	},
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ListenSocket {
	IP { address: String },
	UDS { path: PathBuf },
}

impl Default for ListenSocket {
	fn default() -> Self {
		Self::UDS {
			path: "/tmp/wgpaper.socket".into(),
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuConfig {
	Global(GpuSelector),
	PerMonitor(HashMap<String, GpuSelector>),
}

impl Default for GpuConfig {
	fn default() -> Self {
		GpuConfig::PerMonitor(HashMap::default())
	}
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
	Other,
	#[default]
	IntegratedGpu,
	DiscreteGpu,
	VirtualGpu,
	Cpu,
}

impl Default for Background {
	fn default() -> Self {
		Self::CssColor(csscolorparser::NAMED_COLORS["black".into()].into())
	}
}

fn get_path_from_string_expanded(path: String) -> PathBuf {
	PathBuf::from(tilde(&path).into_owned())
}

fn wallpaper_directories_default() -> Vec<ConfigPath> {
	vec![ConfigPath::from("~/Pictures/Wallpapers".to_string())]
}

fn image_extensions_default() -> Vec<String> {
	vec!["jpg".to_string(), "png".to_string()]
}

#[derive(Deserialize)]
pub struct Config {
	#[serde(default)]
	shader: Option<ShaderConfig>,

	#[serde(default)]
	initial_wallpaper: Option<ConfigPath>,

	#[serde(default = "wallpaper_directories_default")]
	wallpaper_directories: Vec<ConfigPath>,

	#[serde(default = "image_extensions_default")]
	image_extensions: Vec<String>,

	#[serde(default)]
	scaling_mode: ScalingMode,

	#[serde(default)]
	listen_socket: ListenSocket,
	gpu: Option<GpuConfig>,
}

impl Config {
	pub const APP_NAME: &str = "wgpaper";
	pub const CONFIG_FILE_NAME: &str = "config.json";
	pub const GLOBAL_CONFIG_FILE_PATH: &str =
		formatcp!("/etc/{}/{}", Config::APP_NAME, Config::CONFIG_FILE_NAME);

	pub fn try_new() -> anyhow::Result<Self> {
		let local_config_path = Self::get_local_config_path()?;
		let config_file = if Path::new(&local_config_path).exists() {
			fs::read(local_config_path)?
		} else {
			fs::read(Config::GLOBAL_CONFIG_FILE_PATH)?
		};
		Ok(serde_json::from_slice(&config_file)?)
	}

	fn get_local_config_path() -> anyhow::Result<PathBuf> {
		let mut config_dir = env::var("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or(
			std::env::home_dir()
				.context("Failed to get home directory.")
				.map(|mut home_dir| {
					home_dir.push(".config");
					home_dir
				})?,
		);

		config_dir.push(Config::APP_NAME);
		config_dir.push(Config::CONFIG_FILE_NAME);

		Ok(config_dir)
	}

	/// Returns the animation shader config if configured
	pub fn shader(&self) -> Option<&ShaderConfig> {
		self.shader.as_ref()
	}

	/// Returns the initial wallpaper path if configured
	pub fn initial_wallpaper(&self) -> Option<&Path> {
		self.initial_wallpaper.as_deref()
	}

	/// Returns wallpaper directories if configured
	pub fn wallpaper_directories(&self) -> &[ConfigPath] {
		self.wallpaper_directories.as_ref()
	}

	/// Returns allowed image extensions if configured
	pub fn image_extensions(&self) -> &[String] {
		self.image_extensions.as_ref()
	}

	/// Returns the scaling strategy if configured
	pub fn scaling_mode(&self) -> &ScalingMode {
		&self.scaling_mode
	}

	/// Returns the listen socket configuration if set
	pub fn listen_socket(&self) -> &ListenSocket {
		&self.listen_socket
	}

	/// Returns the GPU configuration if set
	pub fn gpu(&self) -> Option<&GpuConfig> {
		self.gpu.as_ref()
	}
}

impl Default for Config {
	fn default() -> Self {
		Self {
			shader: None,
			initial_wallpaper: None,
			wallpaper_directories: wallpaper_directories_default(),
			image_extensions: image_extensions_default(),
			scaling_mode: ScalingMode::default(),
			listen_socket: ListenSocket::default(),
			gpu: None,
		}
	}
}
