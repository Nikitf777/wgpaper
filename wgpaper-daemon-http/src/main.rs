use actix_web::{App, HttpServer, web};
use lib_wgpaper_daemon::app::manager::AppManager;
use std::sync::{Arc, Mutex};

mod handlers;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	env_logger::init();

	let config = Arc::new(wgpaper_config::Config::new().unwrap());
	let app_manager = Arc::new(Mutex::new(AppManager::try_new(config).unwrap()));

	HttpServer::new(move || {
		App::new()
			.app_data(web::Data::from(app_manager.clone()))
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
