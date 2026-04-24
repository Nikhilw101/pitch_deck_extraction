use axum::{extract::Request, middleware::Next, response::Response};

pub async fn authorize(req: Request, next: Next) -> Response {
    // Dummy implementation
    next.run(req).await
}
