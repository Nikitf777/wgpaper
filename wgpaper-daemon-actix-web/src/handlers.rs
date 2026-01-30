use actix_web::{HttpResponse, Responder, web};
use calloop::channel::Sender;
use lib_wgpaper_daemon::app::Commands;
use wgpaper_config::Config;

use crate::random_file::select_random_file;

pub async fn start_transition(
	sender: web::Data<Sender<Commands>>,
	config: web::Data<Config>,
) -> impl Responder {
	let path = select_random_file(
		config.wallpaper_directories().unwrap_or_default(),
		&[".jpg", ".png"],
		&[] as &[&str],
	)
	.unwrap();
	let command = Commands::StartTransition {
		image_path: path.to_str().unwrap().to_string(),
	};
	sender
		.send(command)
		.map(|_| HttpResponse::Ok().body("OK"))
		.unwrap_or_else(|_| HttpResponse::ServiceUnavailable().body("SCTK offline"))
}
