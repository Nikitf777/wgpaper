use actix_web::{App, HttpServer, web};
use calloop::channel::channel;
use lib_wgpaper_daemon::app::{self, Commands};
use std::{sync::Arc, thread};

use crate::random_file::select_random_file;

mod handlers;
mod random_file;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let config = wgpaper_config::Config::new().unwrap();
	let config_arc = Arc::new(config);

	let (sender, channel) = channel::<Commands>();
	let config_for_thread = config_arc.clone();

	thread::spawn(move || {
		let shader = config_for_thread
			.animation_shader()
			.expect("animation_shader must be configured");

		let directories = config_for_thread
			.wallpaper_directories()
			.expect("wallpaper_directories must be configured");

		let path = select_random_file(directories, &[".jpg", ".png"], &[] as &[&str])
			.expect("failed to select random wallpaper");

		app::start(channel, shader, &path).unwrap();
	});

	HttpServer::new(move || {
		App::new()
			.app_data(web::Data::new(sender.clone()))
			.app_data(web::Data::from(config_arc.clone()))
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
