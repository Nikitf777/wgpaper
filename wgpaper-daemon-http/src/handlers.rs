use std::sync::Mutex;

use actix_web::{
	HttpResponse, Responder,
	web::{self},
};
use lib_wgpaper_daemon::app::manager::SCTKManager;

pub async fn start_transition(app_manager: web::Data<Mutex<SCTKManager>>) -> impl Responder {
	app_manager
		.lock()
		.unwrap()
		.start_transition_all_random()
		.map(|_| HttpResponse::Ok().body("OK"))
		.unwrap_or_else(|_| HttpResponse::ServiceUnavailable().body("SCTK offline"))
}
