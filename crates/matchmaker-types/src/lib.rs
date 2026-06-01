//! Shared contracts between API, core engine, and game servers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Skill rating used for matching (e.g. Elo, TrueSkill mu).
pub type SkillRating = f64;

/// Region / datacenter bucket for latency-aware matching.
pub type RegionId = String;

/// Role preference for team composition (optional constraint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Tank,
    Damage,
    Support,
    Flex,
}

/// Player identity and queue metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuePlayer {
    pub id: Uuid,
    pub skill: SkillRating,
    pub region: RegionId,
    pub role: Role,
    pub party_id: Option<Uuid>,
    pub enqueued_at: DateTime<Utc>,
}

/// Request to join the competitive queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueRequest {
    pub player_id: Uuid,
    pub skill: SkillRating,
    pub region: RegionId,
    #[serde(default = "default_role")]
    pub role: Role,
    pub party_id: Option<Uuid>,
}

fn default_role() -> Role {
    Role::Flex
}

/// Response after joining the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueResponse {
    pub ticket_id: Uuid,
    pub position_hint: Option<u32>,
    pub estimated_wait_secs: Option<u32>,
}

/// A formed 5v5 match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Match {
    pub match_id: Uuid,
    pub region: RegionId,
    pub team_a: Vec<Uuid>,
    pub team_b: Vec<Uuid>,
    pub average_skill: SkillRating,
    pub skill_spread: SkillRating,
    pub formed_at: DateTime<Utc>,
}

/// Health snapshot for operators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub queue_depth: u64,
    pub matches_formed_total: u64,
    pub scans_total: u64,
    pub evictions_total: u64,
    pub avg_match_quality: f64,
    pub p99_scan_micros: u64,
}
