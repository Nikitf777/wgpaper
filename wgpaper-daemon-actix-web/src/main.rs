use actix_web::{App, HttpResponse, HttpServer, Responder, web};
use calloop::channel::{Sender, channel};
use lib_wgpaper_daemon::app::{self, Commands};
use std::thread;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	let (web_tx, sctk_rx) = channel::<Commands>();
	thread::spawn(move || {
		app::start(sctk_rx).unwrap();
	});

	HttpServer::new(move || {
		App::new()
			.app_data(web::Data::new(web_tx.clone()))
			.route("/transition/start", web::post().to(web_handler))
	})
	.bind_uds("/tmp/wgpaper.socket")?
	.run()
	.await
}

async fn web_handler(cmd: web::Json<Commands>, tx: web::Data<Sender<Commands>>) -> impl Responder {
	tx.send(cmd.into_inner())
		.map(|_| HttpResponse::Ok().body("OK"))
		.unwrap_or_else(|_| HttpResponse::ServiceUnavailable().body("SCTK offline"))
}
