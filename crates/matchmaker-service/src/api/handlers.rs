use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use matchmaker_core::{MatchmakerError, QueuedPlayer};
use matchmaker_types::{
    EnqueueRequest, EnqueueResponse, HealthSnapshot, QueuePlayer,
};
use uuid::Uuid;

use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<HealthSnapshot> {
    Json(state.engine.metrics().snapshot())
}

pub async fn metrics(State(state): State<AppState>) -> Json<HealthSnapshot> {
    Json(state.engine.metrics().snapshot())
}

pub async fn enqueue(
    State(state): State<AppState>,
    Json(req): Json<EnqueueRequest>,
) -> Result<Json<EnqueueResponse>, (StatusCode, String)> {
    let player = QueuePlayer {
        id: req.player_id,
        skill: req.skill,
        region: req.region,
        role: req.role,
        party_id: req.party_id,
        enqueued_at: Utc::now(),
    };

    let queued: QueuedPlayer = player.into();
    state
        .engine
        .pool()
        .enqueue(queued)
        .map_err(map_error)?;

    let depth = state.engine.pool().len();
    Ok(Json(EnqueueResponse {
        ticket_id: req.player_id,
        position_hint: Some(depth as u32),
        estimated_wait_secs: Some(estimate_wait(depth)),
    }))
}

pub async fn dequeue(
    State(state): State<AppState>,
    Path(player_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .engine
        .pool()
        .dequeue(player_id)
        .map_err(map_error)?;
    Ok(StatusCode::NO_CONTENT)
}

fn map_error(err: MatchmakerError) -> (StatusCode, String) {
    match err {
        MatchmakerError::AlreadyQueued(id) => {
            (StatusCode::CONFLICT, format!("player {id} already queued"))
        }
        MatchmakerError::NotInQueue(id) => {
            (StatusCode::NOT_FOUND, format!("player {id} not in queue"))
        }
        MatchmakerError::InvalidConfig(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
    }
}

fn estimate_wait(queue_depth: usize) -> u32 {
    // Rough heuristic: need 10 players, assume ~2s per slot at moderate load.
    if queue_depth < 10 {
        ((10 - queue_depth) as u32) * 2 + 5
    } else {
        10
    }
}
