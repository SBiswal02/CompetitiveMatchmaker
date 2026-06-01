//! Core matching algorithm: select 10 compatible players and form balanced teams.

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use uuid::Uuid;

use matchmaker_types::Match;

use crate::balance::balance_teams;
use crate::config::MatchmakerConfig;
use crate::metrics::MatchmakerMetrics;
use crate::pool::{PlayerPool, QueuedPlayer};
use crate::relaxation::RelaxationPolicy;

/// A candidate lobby ready to commit.
#[derive(Debug, Clone)]
pub struct MatchCandidate {
    pub players: Vec<QueuedPlayer>,
    pub team_a: Vec<Uuid>,
    pub team_b: Vec<Uuid>,
    pub quality: f64,
    pub region: String,
}

/// Matchmaking engine orchestrating pool scans and team formation.
#[derive(Debug)]
pub struct MatchmakerEngine {
    pool: Arc<PlayerPool>,
    config: MatchmakerConfig,
    relaxation: RelaxationPolicy,
    metrics: Arc<MatchmakerMetrics>,
}

impl MatchmakerEngine {
    pub fn new(
        pool: Arc<PlayerPool>,
        config: MatchmakerConfig,
        metrics: Arc<MatchmakerMetrics>,
    ) -> Self {
        config.validate().expect("invalid matchmaker config");
        let relaxation = RelaxationPolicy::new(config.clone());
        Self {
            pool,
            config,
            relaxation,
            metrics,
        }
    }

    pub fn pool(&self) -> &Arc<PlayerPool> {
        &self.pool
    }

    pub fn metrics(&self) -> &Arc<MatchmakerMetrics> {
        &self.metrics
    }

    /// Single scan pass: evict expired, then try to form matches per region.
    pub fn scan_once(&self) -> Vec<Match> {
        let start = Instant::now();
        let now = Utc::now();

        self.pool.evict_expired(now);

        let mut matches = Vec::new();
        let regions = self.pool.regions();

        for region in regions {
            while let Some(candidate) = self.find_best_lobby(&region, now) {
                let ids: Vec<Uuid> = candidate.players.iter().map(|p| p.id).collect();
                if !self.pool.try_remove_batch(&ids) {
                    break;
                }
                let quality = candidate.quality;
                let m = self.candidate_to_match(candidate);
                self.metrics.record_match(quality);
                matches.push(m);
            }
        }

        let elapsed = start.elapsed().as_nanos() as u64;
        self.metrics.record_scan(elapsed);
        matches
    }

    fn find_best_lobby(&self, region: &str, now: chrono::DateTime<Utc>) -> Option<MatchCandidate> {
        let mut players = self.pool.snapshot_by_region(region);
        if players.len() < self.config.lobby_size {
            return None;
        }

        // Anchor on longest-waiting player to apply relaxation fairly.
        players.sort_by_key(|p| p.enqueued_at);
        let anchor = &players[0];
        let oldest_wait = (now - anchor.enqueued_at).num_seconds().max(0) as f64;
        let skill_band = self.relaxation.skill_band_for(anchor.enqueued_at, now);
        let min_quality = self.relaxation.min_quality_for(oldest_wait);
        let max_team_delta = self.relaxation.max_team_delta(oldest_wait);

        let anchor_skill = anchor.skill;
        let mut candidates: Vec<QueuedPlayer> = players
            .iter()
            .filter(|p| (p.skill - anchor_skill).abs() <= skill_band)
            .cloned()
            .collect();

        if candidates.len() < self.config.lobby_size {
            return None;
        }

        // Greedy: pick 10 closest to anchor skill (tight lobby), then balance teams.
        candidates.sort_by(|a, b| {
            let da = (a.skill - anchor_skill).abs();
            let db = (b.skill - anchor_skill).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(self.config.lobby_size);

        let lobby_skill: f64 = candidates.iter().map(|p| p.skill).sum::<f64>() / 10.0;
        let spread = candidates
            .iter()
            .map(|p| (p.skill - lobby_skill).abs())
            .fold(0.0_f64, f64::max);

        let quality = self.compute_quality(spread, skill_band);
        if quality < min_quality {
            return None;
        }

        let balance = balance_teams(&candidates)?;
        if balance.skill_delta > max_team_delta {
            return None;
        }

        Some(MatchCandidate {
            team_a: balance.team_a,
            team_b: balance.team_b,
            players: candidates,
            quality,
            region: region.to_string(),
        })
    }

    fn compute_quality(&self, spread: f64, band: f64) -> f64 {
        if band <= 0.0 {
            return 0.0;
        }
        (1.0 - (spread / band)).clamp(0.0, 1.0)
    }

    fn candidate_to_match(&self, c: MatchCandidate) -> Match {
        let avg: f64 = c.players.iter().map(|p| p.skill).sum::<f64>() / c.players.len() as f64;
        let spread = c
            .players
            .iter()
            .map(|p| (p.skill - avg).abs())
            .fold(0.0_f64, f64::max);

        Match {
            match_id: Uuid::new_v4(),
            region: c.region,
            team_a: c.team_a,
            team_b: c.team_b,
            average_skill: avg,
            skill_spread: spread,
            formed_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use matchmaker_types::Role;

    fn queued(skill: f64, wait_secs: i64) -> QueuedPlayer {
        QueuedPlayer {
            id: Uuid::new_v4(),
            skill,
            region: "us-east".into(),
            role: Role::Flex,
            party_id: None,
            enqueued_at: Utc::now() - chrono::Duration::seconds(wait_secs),
        }
    }

    #[test]
    fn forms_match_for_similar_skills() {
        let metrics = Arc::new(MatchmakerMetrics::default());
        let config = MatchmakerConfig::default();
        let pool = Arc::new(PlayerPool::new(config.clone(), metrics.clone()));
        let engine = MatchmakerEngine::new(pool.clone(), config, metrics);

        for i in 0..10 {
            pool.enqueue(queued(1500.0 + i as f64, 5)).unwrap();
        }

        let matches = engine.scan_once();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].team_a.len(), 5);
        assert_eq!(matches[0].team_b.len(), 5);
    }
}
