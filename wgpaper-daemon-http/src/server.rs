use actix_web::{App, HttpServer, dev::Server, web};
use lib_wgpaper_daemon::app::manager::SCTKManager;
use std::sync::{Arc, Mutex};

use crate::handlers;

pub fn server(sctk_manager: Arc<Mutex<SCTKManager>>) -> std::io::Result<Server> {
	Ok(HttpServer::new(move || {
		App::new()
			.app_data(web::Data::from(sctk_manager.clone()))
			.route(
				"/transition/start",
				web::post().to(handlers::start_transition),
			)
	})
	.workers(1)
	.bind_uds("/tmp/wgpaper.socket")?
	.run())
}
