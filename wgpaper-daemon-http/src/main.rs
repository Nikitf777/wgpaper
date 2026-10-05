use crate::server::server;
use lib_wgpaper_daemon::app::manager::SctkManager;
use log::{error, info, warn};
use signal_hook::{
	consts::{SIGINT, SIGTERM},
	iterator::Signals,
};
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

	let mut signals = Signals::new([SIGINT, SIGTERM])?;
	tokio::spawn(async move {
		for sig in signals.forever() {
			let sig_name = match sig {
				SIGINT => "SIGINT",
				SIGTERM => "SIGTERM",
				_ => "UNKNOWN SIGNAL",
			};
			info!("{} received. Stopping HTTP server gracefully.", sig_name);
			server_handle.stop(true).await;
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
		error!(
			"Failed send the Stop command to the SCTK thread: {}.",
			err
		);
		std::process::exit(1);
	});
	info!("SCTK thread stopped.");
}
