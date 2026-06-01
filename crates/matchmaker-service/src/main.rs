//! HTTP API and background matching workers for the 5v5 engine.

mod api;
mod state;
mod workers;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use matchmaker_core::{MatchmakerConfig, MatchmakerEngine, MatchmakerMetrics, PlayerPool};
use tokio::sync::broadcast;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = MatchmakerConfig::default();
    let metrics = Arc::new(MatchmakerMetrics::default());
    let pool = Arc::new(PlayerPool::new(config.clone(), metrics.clone()));
    let engine = Arc::new(MatchmakerEngine::new(pool, config.clone(), metrics.clone()));

    let (match_tx, _) = broadcast::channel(1024);
    let state = AppState {
        engine: engine.clone(),
        config,
        match_tx,
    };

    let worker_count: usize = std::env::var("MATCHMAKER_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);

    let scan_interval_ms: u64 = std::env::var("MATCHMAKER_SCAN_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    workers::spawn_workers(engine, worker_count, scan_interval_ms);

    let app = Router::new()
        .merge(api::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = std::env::var("MATCHMAKER_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .expect("invalid MATCHMAKER_BIND");

    tracing::info!(%addr, worker_count, scan_interval_ms, "matchmaker listening");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
