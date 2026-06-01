mod handlers;

use axum::{routing::{delete, get, post}, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/metrics", get(handlers::metrics))
        .route("/queue", post(handlers::enqueue))
        .route("/queue/{player_id}", delete(handlers::dequeue))
}
