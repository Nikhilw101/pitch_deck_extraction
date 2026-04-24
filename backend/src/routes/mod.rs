pub mod deck_routes;

use axum::{extract::DefaultBodyLimit, Router};

pub fn create_router() -> Router {
    Router::new()
        .merge(deck_routes::router())
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
}
