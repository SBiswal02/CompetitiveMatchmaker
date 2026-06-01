use std::sync::Arc;

use axum::extract::FromRef;
use matchmaker_core::{MatchmakerConfig, MatchmakerEngine};
use matchmaker_types::Match;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<MatchmakerEngine>,
    pub config: MatchmakerConfig,
    pub match_tx: broadcast::Sender<Match>,
}

impl FromRef<AppState> for Arc<MatchmakerEngine> {
    fn from_ref(state: &AppState) -> Self {
        state.engine.clone()
    }
}

impl FromRef<AppState> for MatchmakerConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

impl FromRef<AppState> for broadcast::Sender<Match> {
    fn from_ref(state: &AppState) -> Self {
        state.match_tx.clone()
    }
}
