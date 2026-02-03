use crate::{random_file::select_random_file, services::TransitionService};
use actix_web::{App, HttpServer, web};
use calloop::channel::channel;
use lib_wgpaper_daemon::app::{self, Commands, GlobalOptions};
use std::{
	sync::{Arc, Mutex},
	thread,
};

mod handlers;
mod random_file;
mod services;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let config = wgpaper_config::Config::new().unwrap();
	let config_arc = Arc::new(config);

	let (sender, channel) = channel::<Commands>();
	let config_for_thread = config_arc.clone();

	thread::spawn(move || {
		let directories = config_for_thread
			.wallpaper_directories()
			.expect("wallpaper_directories must be configured");

		let path = select_random_file(directories, &[".jpg", ".png"], &[] as &[&str])
			.expect("failed to select random wallpaper");

		let options = GlobalOptions {
			gpu_selector: config_for_thread.gpu().cloned(),
			animation_shader_path: config_for_thread.animation_shader(),
			initial_image_path: Some(&path),
		};

		app::start(channel, options).unwrap();
	});

	let transition_service = Arc::new(Mutex::new(TransitionService::default()));

	HttpServer::new(move || {
		App::new()
			.app_data(web::Data::new(sender.clone()))
			.app_data(web::Data::from(config_arc.clone()))
			.app_data(web::Data::from(transition_service.clone()))
			.route(
				"/transition/start",
				web::post().to(handlers::start_transition),
			)
	})
	.workers(2)
	.bind_uds("/tmp/wgpaper.socket")?
	.run()
	.await
}
