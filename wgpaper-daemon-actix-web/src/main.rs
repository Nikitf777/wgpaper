use actix_web::{App, HttpServer, web};
use lib_wgpaper_daemon::app::communicator::AppCommunicator;
use std::sync::{Arc, Mutex};

mod handlers;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let config = Arc::new(wgpaper_config::Config::new().unwrap());
	let communicator = Arc::new(Mutex::new(AppCommunicator::new(config)));

	HttpServer::new(move || {
		App::new()
			.app_data(web::Data::from(communicator.clone()))
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
