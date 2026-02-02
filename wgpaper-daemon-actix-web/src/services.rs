use std::path::{Path, PathBuf};

use actix_web::{HttpResponse, web};
use calloop::channel::Sender;
use lib_wgpaper_daemon::app::Commands;
use wgpaper_config::Config;

use crate::random_file::select_random_file;

#[derive(Default)]
pub struct TransitionService {
	prev_image_path: Option<PathBuf>,
}

impl TransitionService {
	pub fn start_transition(
		&mut self,
		sender: web::Data<Sender<Commands>>,
		config: web::Data<Config>,
	) -> anyhow::Result<HttpResponse> {
		let excluded_files = [self.prev_image_path.as_deref().unwrap_or(Path::new(""))];

		let path = select_random_file(
			config.wallpaper_directories().unwrap_or_default(),
			&[".jpg", ".png"],
			&excluded_files,
		)
		.unwrap();
		self.prev_image_path = Some(path.clone());
		let command = Commands::StartTransition { image_path: path };
		anyhow::Ok(
			sender
				.send(command)
				.map(|_| HttpResponse::Ok().body("OK"))?,
		)
	}
}
