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
	let directories = config.wallpaper_directories().unwrap();
	let path = select_random_file(directories, &[".jpg", ".png"], &[] as &[&str]).unwrap();

	let (sender, channel) = channel::<Commands>();
	thread::spawn(move || {
		app::start(channel, &path).unwrap();
	});

	let config_arc = Arc::new(config);

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
