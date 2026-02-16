use actix_web::{App, HttpServer, web};
use lib_wgpaper_daemon::app::manager::AppManager;
use log::{error, info};
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::sync::{Arc, Mutex};

mod handlers;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	env_logger::init();

	let config = wgpaper_config::Config::try_new().unwrap_or_else(|err| {
		error!("Failed to parse config file: {}", err.to_string());
		std::process::exit(1);
	});
	let app_manager = AppManager::try_new(config)
		.map(|manager| Arc::new(Mutex::new(manager)))
		.unwrap_or_else(|err| {
			error!("Failed to initialize the app manager: {}", err.to_string());
			std::process::exit(1);
		});
	let post_server_app_manager = app_manager.clone();

	let server = HttpServer::new(move || {
		App::new()
			.app_data(web::Data::from(app_manager.clone()))
			.route(
				"/transition/start",
				web::post().to(handlers::start_transition),
			)
	})
	.workers(1)
	.bind_uds("/tmp/wgpaper.socket")?
	.run();

	let server_handle = server.handle();

	let mut signals = Signals::new([SIGINT])?;
	tokio::spawn(async move {
		for _ in signals.forever() {
			info!("Ctrl+C received. Stopping HTTP server gracefully");
			server_handle.stop(true).await;
		}
	});

	if let Err(e) = server.await {
		error!("HTTP server error during runtime: {}", e);
	}

	info!("HTTP server stopped. Shutting down SCTK application");

	let mut manager = post_server_app_manager.lock().expect("Poisoned mutex");
	let _ = manager.shutdown();

	info!("Graceful shutdown complete. Exiting.");

	Ok(())
}
