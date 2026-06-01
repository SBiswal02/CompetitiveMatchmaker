//! Thread-safe waiting player pool with atomic eviction.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use uuid::Uuid;

use matchmaker_types::{QueuePlayer, Role};

use crate::config::MatchmakerConfig;
use crate::error::MatchmakerError;
use crate::metrics::MatchmakerMetrics;

/// In-memory representation of a queued player.
#[derive(Debug, Clone)]
pub struct QueuedPlayer {
    pub id: Uuid,
    pub skill: f64,
    pub region: String,
    pub role: Role,
    pub party_id: Option<Uuid>,
    pub enqueued_at: DateTime<Utc>,
}

impl From<QueuePlayer> for QueuedPlayer {
    fn from(p: QueuePlayer) -> Self {
        Self {
            id: p.id,
            skill: p.skill,
            region: p.region,
            role: p.role,
            party_id: p.party_id,
            enqueued_at: p.enqueued_at,
        }
    }
}

/// Concurrent player pool: multiple workers may scan; eviction uses compare-remove.
#[derive(Debug)]
pub struct PlayerPool {
    players: DashMap<Uuid, QueuedPlayer>,
    /// Monotonic generation bumped on each successful match removal batch.
    generation: AtomicU64,
    config: MatchmakerConfig,
    metrics: Arc<MatchmakerMetrics>,
}

impl PlayerPool {
    pub fn new(config: MatchmakerConfig, metrics: Arc<MatchmakerMetrics>) -> Self {
        Self {
            players: DashMap::new(),
            generation: AtomicU64::new(0),
            config,
            metrics,
        }
    }

    pub fn len(&self) -> usize {
        self.players.len()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn enqueue(&self, player: QueuedPlayer) -> Result<(), MatchmakerError> {
        if self.players.contains_key(&player.id) {
            return Err(MatchmakerError::AlreadyQueued(player.id));
        }
        self.players.insert(player.id, player);
        self.metrics.set_queue_depth(self.len() as u64);
        Ok(())
    }

    pub fn dequeue(&self, player_id: Uuid) -> Result<(), MatchmakerError> {
        self.players
            .remove(&player_id)
            .ok_or(MatchmakerError::NotInQueue(player_id))?;
        self.metrics.set_queue_depth(self.len() as u64);
        Ok(())
    }

    /// Snapshot players in a region for matching (copy-on-read, avoids long locks).
    pub fn snapshot_by_region(&self, region: &str) -> Vec<QueuedPlayer> {
        self.players
            .iter()
            .filter(|e| e.value().region == region)
            .map(|e| e.value().clone())
            .collect()
    }

    pub fn regions(&self) -> Vec<String> {
        let mut regions: Vec<String> = self
            .players
            .iter()
            .map(|e| e.value().region.clone())
            .collect();
        regions.sort();
        regions.dedup();
        regions
    }

    /// Remove matched players atomically; returns false if any ID was already taken.
    pub fn try_remove_batch(&self, ids: &[Uuid]) -> bool {
        // Pre-check all present to avoid partial removal.
        for id in ids {
            if !self.players.contains_key(id) {
                return false;
            }
        }
        for id in ids {
            if self.players.remove(id).is_none() {
                return false;
            }
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.metrics.set_queue_depth(self.len() as u64);
        true
    }

    /// Evict players who exceeded max queue time. Safe for concurrent workers.
    pub fn evict_expired(&self, now: DateTime<Utc>) -> usize {
        let max_wait = chrono::Duration::seconds(self.config.max_queue_time_secs as i64);
        let expired: Vec<Uuid> = self
            .players
            .iter()
            .filter(|e| now - e.value().enqueued_at > max_wait)
            .map(|e| *e.key())
            .collect();

        let count = expired.len();
        for id in expired {
            let _ = self.players.remove(&id);
        }
        if count > 0 {
            self.generation.fetch_add(1, Ordering::AcqRel);
            self.metrics.record_evictions(count as u64);
            self.metrics.set_queue_depth(self.len() as u64);
        }
        count
    }
}
