use std::sync::Mutex;

use actix_web::{
	HttpResponse, Responder,
	web::{self},
};
use lib_wgpaper_daemon::app::communicator::AppCommunicator;

pub async fn start_transition(communicator: web::Data<Mutex<AppCommunicator>>) -> impl Responder {
	communicator
		.lock()
		.unwrap()
		.start_transition()
		.map(|_| HttpResponse::Ok().body("OK"))
		.unwrap_or_else(|_| HttpResponse::ServiceUnavailable().body("SCTK offline"))
}
