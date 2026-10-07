use crate::server::server;
use lib_wgpaper_daemon::app::manager::SctkManager;
use log::{error, info, warn};
use std::sync::{Arc, Mutex};
use wgpaper_config::Config;

mod handlers;
mod server;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	env_logger::init();

	let config = Config::try_new()
		.inspect_err(|err| {
			warn!(
				"Failed to parse config file: {}. Falling back to defaults.",
				err
			);
		})
		.unwrap_or_default();

	let sctk_manager = SctkManager::try_new(config)
		.map(|manager| Arc::new(Mutex::new(manager)))
		.unwrap_or_else(|err| {
			error!("Failed to initialize the app manager: {}.", err);
			std::process::exit(1);
		});
	let post_server_sctk_manager = sctk_manager.clone();

	let server = server(sctk_manager.clone()).unwrap_or_else(|err| {
		error!(
			"Failed to start the HTTP server: {}. Trying to stop the SCTK thread...",
			err
		);
		shutdown_sctk_manager(&post_server_sctk_manager);
		std::process::exit(1);
	});
	let server_handle = server.handle();

	// `signal_hook`'s iterator is blocking, which would starve every other task on
// the single-threaded actix runtime, hence tokio's async signal API.
	let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
	let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
	tokio::spawn({
		let handle = server_handle.clone();
		async move {
			sigint.recv().await;
			info!("SIGINT received. Stopping HTTP server gracefully.");
			handle.stop(true).await;
		}
	});
	tokio::spawn({
		let handle = server_handle.clone();
		async move {
			sigterm.recv().await;
			info!("SIGTERM received. Stopping HTTP server gracefully.");
			handle.stop(true).await;
		}
	});

	// The SCTK thread dies together with its compositor. Without this the daemon
	// would linger forever, serving requests it can no longer fulfil.
	let watched_manager = post_server_sctk_manager.clone();
	tokio::spawn(async move {
		let mut ticker = tokio::time::interval(std::time::Duration::from_millis(500));
		loop {
			ticker.tick().await;
			// `try_lock` keeps the handlers from being blocked by this check.
			let is_finished = watched_manager
				.try_lock()
				.map(|manager| manager.is_finished())
				.unwrap_or(false);

			if is_finished {
				warn!("The compositor is gone. Stopping HTTP server gracefully.");
				server_handle.stop(true).await;
				return;
			}
		}
	});

	if let Err(e) = server.await {
		error!("HTTP server error during runtime: {}.", e);
	}

	info!("HTTP server stopped. Shutting down SCTK manager...");

	shutdown_sctk_manager(&post_server_sctk_manager);
	info!("Graceful shutdown complete. Exiting.");

	Ok(())
}

fn shutdown_sctk_manager(sctk_manager: &Arc<Mutex<SctkManager>>) {
	let mut manager = sctk_manager.lock().unwrap_or_else(|err| {
		error!("Failed to sync SCTK manager: {}.", err);
		std::process::exit(1);
	});
	manager.shutdown().unwrap_or_else(|err| {
		error!("Failed send the Stop command to the SCTK thread: {}.", err);
		std::process::exit(1);
	});
	info!("SCTK thread stopped.");
}
