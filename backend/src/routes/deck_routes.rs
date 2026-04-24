use crate::controllers::{deck_controller, search_controller};
use crate::middleware::{request_id, request_logger};
use axum::{middleware::from_fn, routing::{get, post}, Router};

pub fn router() -> Router {
    Router::new()
        .route("/api/health", get(crate::controllers::health_controller::health_check))
        .route("/api/decks/upload", post(deck_controller::upload_deck))
        .route("/api/decks/search", post(search_controller::search_decks))
        .route("/api/jobs/status/:job_id", get(crate::controllers::job_controller::get_job_status))
        .layer(from_fn(request_logger::log_request))
        .layer(from_fn(request_id::inject_request_id))
}
