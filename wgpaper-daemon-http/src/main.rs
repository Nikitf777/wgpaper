use crate::server::server;
use lib_wgpaper_daemon::app::manager::SCTKManager;
use log::{error, info};
use signal_hook::{
	consts::{SIGINT, SIGTERM},
	iterator::Signals,
};
use std::sync::{Arc, Mutex};

mod handlers;
mod server;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	env_logger::init();

	let config = wgpaper_config::Config::try_new().unwrap_or_else(|err| {
		error!("Failed to parse config file: {}.", err.to_string());
		std::process::exit(1);
	});
	let sctk_manager = SCTKManager::try_new(config)
		.map(|manager| Arc::new(Mutex::new(manager)))
		.unwrap_or_else(|err| {
			error!("Failed to initialize the app manager: {}.", err.to_string());
			std::process::exit(1);
		});
	let post_server_sctk_manager = sctk_manager.clone();

	let server = server(sctk_manager.clone())?;
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

	info!("HTTP server stopped. Shutting down SCTK application...");

	let mut manager = post_server_sctk_manager.lock().unwrap_or_else(|err| {
		error!("Failed to sync SCTK manager: {}.", err.to_string());
		std::process::exit(1);
	});
	manager.shutdown().unwrap_or_else(|err| {
		error!(
			"Failed send the Stop command to the SCTK thread: {}.",
			err.to_string()
		);
		std::process::exit(1);
	});

	info!("Graceful shutdown complete. Exiting.");

	Ok(())
}
