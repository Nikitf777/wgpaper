use rand::{RngExt, prelude::IndexedRandom, rng};
use std::{
	fs,
	path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RandomFileError {
	#[error("No directories provided")]
	NoDirectories,

	#[error("No matching files found with extensions: {extensions:?}")]
	NoMatchingFiles { extensions: Vec<String> },

	#[error("Failed to read directory '{path}': {source}")]
	ReadDir {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},

	#[error("Path is not a directory: '{path}'")]
	NotADirectory { path: PathBuf },
}

/// Fills the given vector with the file paths that match the given conditions.
/// Note, that it doesn't clear the vector before filling.
///
/// # Arguments
/// * `directories` - Slice of directory paths to search (non-recursive).
/// * `extensions` - Slice of file extensions to match (e.g., &[".txt", ".rs"]). Extensions should include the leading dot and are matched case-sensitively.
/// * `excluded_files` - Slice of paths to files that won't be returned.
/// * `matching_files` - Vector to fill with matching files.
///
/// # Returns
/// * `Ok(())` - Success.
/// * `Err(RandomFileError)` - If no matching files found or I/O errors occur.
///
/// # Examples
/// ```
fn fill_matching_files<P, E, F>(
	directories: &[P],
	extensions: &[E],
	excluded_files: &[F],
	matching_files: &mut Vec<PathBuf>,
) -> Result<(), RandomFileError>
where
	P: AsRef<Path>,
	E: AsRef<str>,
	F: AsRef<Path>,
{
	if directories.is_empty() {
		return Err(RandomFileError::NoDirectories);
	}

	let extensions: Vec<String> = extensions
		.iter()
		.map(|e| {
			let s = e.as_ref();
			s.strip_prefix('.').unwrap_or(s).to_string()
		})
		.collect();

	if extensions.is_empty() {
		return Err(RandomFileError::NoMatchingFiles { extensions: vec![] });
	}

	for dir in directories {
		let dir_path = dir.as_ref();

		if !dir_path.is_dir() {
			return Err(RandomFileError::NotADirectory {
				path: dir_path.to_path_buf(),
			});
		}

		let entries = fs::read_dir(dir_path).map_err(|source| RandomFileError::ReadDir {
			path: dir_path.to_path_buf(),
			source,
		})?;

		for entry in entries {
			let entry = entry.map_err(|source| RandomFileError::ReadDir {
				path: dir_path.to_path_buf(),
				source,
			})?;

			let path_buf = entry.path();

			if !path_buf.is_file() {
				continue;
			}

			if let Some(ext) = path_buf.extension().and_then(|s| s.to_str()) {
				if extensions.iter().any(|e| e == ext) {
					if excluded_files
						.iter()
						.find(|excl| excl.as_ref() == path_buf)
						.is_none()
					{
						matching_files.push(path_buf);
					}
				}
			}
		}
	}

	Ok(())
}

pub struct RandomFileSelector {
	directories: Vec<PathBuf>,
	extensions: Vec<String>,
	matching_files: Vec<PathBuf>,
	prev_index: Option<usize>,
}

impl RandomFileSelector {
	pub fn new(directories: Vec<PathBuf>, extensions: Vec<String>) -> Self {
		Self {
			directories,
			extensions,
			matching_files: Vec::new(),
			prev_index: None,
		}
	}

	pub fn refresh_matching_files(&mut self) -> Result<(), RandomFileError> {
		self.matching_files.clear();
		fill_matching_files(
			&self.directories,
			&self.extensions,
			&[] as &[&Path],
			&mut self.matching_files,
		)?;
		self.prev_index = None;
		Ok(())
	}

	pub fn pick_next(&mut self) -> Result<PathBuf, RandomFileError> {
		if self.matching_files.is_empty() {
			return Err(RandomFileError::NoMatchingFiles {
				extensions: self.extensions.clone(),
			});
		}

		if self.matching_files.len() == 1 {
			self.prev_index = Some(0);
			return Ok(self.matching_files[0].clone());
		}

		let mut rng = rng();
		let mut new_index;
		loop {
			new_index = rng.random_range(0..self.matching_files.len());
			if Some(new_index) != self.prev_index {
				break;
			}
		}

		self.prev_index = Some(new_index);
		Ok(self.matching_files[new_index].clone())
	}
}

/// Selects a random file from the given directories (non-recursively) matching any of the specified extensions.
///
/// # Arguments
/// * `directories` - Slice of directory paths to search (non-recursive)
/// * `extensions` - Slice of file extensions to match (e.g., &[".txt", ".rs"]). Extensions should include the leading dot and are matched case-sensitively
/// * `excluded_files` - Slice of paths to files that won't be returned
///
/// # Returns
/// * `Ok(PathBuf)` - Path to a randomly selected matching file
/// * `Err(RandomFileError)` - If no matching files found or I/O errors occur
///
/// # Examples
/// ```
/// let file = select_random_file(
///     &["/tmp/dir1", "/tmp/dir2"],
///     &[".txt", ".log"],
///     &["/path/to/excluded_file"]
/// ).unwrap();
/// ```
pub fn select_random_file<P, E, F>(
	directories: &[P],
	extensions: &[E],
	excluded_files: &[F],
) -> Result<PathBuf, RandomFileError>
where
	P: AsRef<Path>,
	E: AsRef<str>,
	F: AsRef<Path>,
{
	if directories.is_empty() {
		return Err(RandomFileError::NoDirectories);
	}

	let extensions: Vec<String> = extensions
		.iter()
		.map(|e| {
			let s = e.as_ref();
			s.strip_prefix('.').unwrap_or(s).to_string()
		})
		.collect();

	if extensions.is_empty() {
		return Err(RandomFileError::NoMatchingFiles { extensions: vec![] });
	}

	let mut matching_files = Vec::new();

	for dir in directories {
		let dir_path = dir.as_ref();

		if !dir_path.is_dir() {
			return Err(RandomFileError::NotADirectory {
				path: dir_path.to_path_buf(),
			});
		}

		let entries = fs::read_dir(dir_path).map_err(|source| RandomFileError::ReadDir {
			path: dir_path.to_path_buf(),
			source,
		})?;

		for entry in entries {
			let entry = entry.map_err(|source| RandomFileError::ReadDir {
				path: dir_path.to_path_buf(),
				source,
			})?;

			let path_buf = entry.path();

			if !path_buf.is_file() {
				continue;
			}

			if let Some(ext) = path_buf.extension().and_then(|s| s.to_str()) {
				if extensions.iter().any(|e| e == ext) {
					if excluded_files
						.iter()
						.find(|excl| excl.as_ref() == path_buf)
						.is_none()
					{
						matching_files.push(path_buf);
					}
				}
			}
		}
	}

	if matching_files.is_empty() {
		return Err(RandomFileError::NoMatchingFiles {
			extensions: extensions.clone(),
		});
	}

	let mut rng = rng();
	matching_files
		.choose(&mut rng)
		.cloned()
		.ok_or_else(|| RandomFileError::NoMatchingFiles {
			extensions: extensions.clone(),
		})
}
