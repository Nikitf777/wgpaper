use std::sync::Mutex;

use actix_web::{
	HttpResponse, Responder,
	web::{self},
};
use calloop::channel::Sender;
use lib_wgpaper_daemon::Commands;
use wgpaper_config::Config;

use crate::services::TransitionService;

pub async fn start_transition(
	sender: web::Data<Sender<Commands>>,
	config: web::Data<Config>,
	transition_service: web::Data<Mutex<TransitionService>>,
) -> impl Responder {
	let mut service = transition_service.lock().unwrap();
	service
		.start_transition(sender, config)
		.unwrap_or_else(|_| HttpResponse::ServiceUnavailable().body("SCTK offline"))
}
