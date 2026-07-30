use anyhow::Context;
use const_format::formatcp;
use serde::{Deserialize, Deserializer};
use shellexpand::tilde;
use std::{
	env, fs,
	hash::{Hash, Hasher},
	path::{Path, PathBuf},
};

#[derive(Clone, PartialEq)]
pub struct Color {
	r: f32,
	g: f32,
	b: f32,
	a: f32,
}

impl Color {
	const fn as_u32(&self) -> u32 {
		let r = (self.r * 255.0).round() as u32;
		let g = (self.g * 255.0).round() as u32;
		let b = (self.b * 255.0).round() as u32;
		let a = (self.a * 255.0).round() as u32;

		(a << 24) | (b << 16) | (g << 8) | r
	}
}

impl<'de> Deserialize<'de> for Color {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		csscolorparser::Color::deserialize(deserializer).map(|c| Color::from(c))
	}
}

impl Default for Color {
	fn default() -> Self {
		csscolorparser::Color::default().into()
	}
}

impl From<csscolorparser::Color> for Color {
	fn from(value: csscolorparser::Color) -> Self {
		unsafe { std::mem::transmute(value) }
	}
}

impl From<[u8; 3]> for Color {
	fn from(value: [u8; 3]) -> Self {
		csscolorparser::Color::from(value).into()
	}
}

impl Eq for Color {}

impl Hash for Color {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.as_u32().hash(state);
	}
}

#[derive(Clone, Deserialize, PartialEq, Hash, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Background {
	AutoColor,
	CssColor(Color),
	Repeat,
	MirrorRepeat,
}

#[derive(Deserialize, Clone, PartialEq, Default, strum_macros::Display, Hash, Eq)]
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

fn deserialize_path_expanded<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error>
where
	D: Deserializer<'de>,
{
	let s: Option<String> = Option::deserialize(deserializer)?;
	s.map(|path_str| Ok(PathBuf::from(get_path_from_string_expanded(path_str))))
		.transpose()
}

fn deserialize_paths_expanded<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
	D: Deserializer<'de>,
{
	let paths: Vec<String> = Vec::deserialize(deserializer)?;
	paths
		.into_iter()
		.map(|path_str| Ok(PathBuf::from(get_path_from_string_expanded(path_str))))
		.collect()
}

fn wallpaper_directories_default() -> Vec<PathBuf> {
	vec![get_path_from_string_expanded(
		"~/Pictures/Wallpapers".to_string(),
	)]
}

fn image_extensions_default() -> Vec<String> {
	vec!["jpg".to_string(), "png".to_string()]
}

#[derive(Deserialize)]
pub struct Config {
	#[serde(default, deserialize_with = "deserialize_path_expanded")]
	shader: Option<PathBuf>,

	#[serde(default, deserialize_with = "deserialize_path_expanded")]
	initial_wallpaper: Option<PathBuf>,

	#[serde(
		default = "wallpaper_directories_default",
		deserialize_with = "deserialize_paths_expanded"
	)]
	wallpaper_directories: Vec<PathBuf>,

	#[serde(default = "image_extensions_default")]
	image_extensions: Vec<String>,

	#[serde(default)]
	scaling_mode: ScalingMode,

	#[serde(default)]
	listen_socket: ListenSocket,
	gpu: Option<GpuSelector>,
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
		let mut config_dir = env::var("XDG_CONFIG_HOME")
			.map(|dir| PathBuf::from(dir))
			.unwrap_or(
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

	/// Returns the animation shader path if configured
	pub fn shader(&self) -> Option<&Path> {
		self.shader.as_deref()
	}

	/// Returns the initial wallpaper path if configured
	pub fn initial_wallpaper(&self) -> Option<&Path> {
		self.initial_wallpaper.as_deref()
	}

	/// Returns wallpaper directories if configured
	pub fn wallpaper_directories(&self) -> &[PathBuf] {
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
	pub fn gpu(&self) -> Option<&GpuSelector> {
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
