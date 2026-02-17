use actix_web::{App, HttpServer, dev::Server, web};
use lib_wgpaper_daemon::app::manager::SctkManager;
use std::sync::{Arc, Mutex};

use crate::handlers;

pub fn server(sctk_manager: Arc<Mutex<SctkManager>>) -> std::io::Result<Server> {
	Ok(HttpServer::new(move || {
		App::new()
			.app_data(web::Data::from(sctk_manager.clone()))
			.service(
				web::scope("/transition")
					.route("/start", web::post().to(handlers::start_transition)),
			)
	})
	.workers(1)
	.bind_uds("/tmp/wgpaper.socket")?
	.run())
}
