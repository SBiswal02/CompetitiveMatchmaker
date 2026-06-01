use chrono::{DateTime, Utc};

use crate::config::MatchmakerConfig;

/// Time-based widening of match constraints (latency vs quality tradeoff).
#[derive(Debug, Clone)]
pub struct RelaxationPolicy {
    config: MatchmakerConfig,
}

impl RelaxationPolicy {
    pub fn new(config: MatchmakerConfig) -> Self {
        Self { config }
    }

    /// Effective skill band for a player given how long they have waited.
    pub fn skill_band_for(&self, enqueued_at: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
        let wait_secs = (now - enqueued_at).num_seconds().max(0) as f64;
        let band = self.config.initial_skill_band + wait_secs * self.config.skill_band_per_second;
        band.min(self.config.max_skill_band)
    }

    /// Minimum acceptable match quality; decreases as the longest-waiting player ages.
    pub fn min_quality_for(&self, oldest_wait_secs: f64) -> f64 {
        // After 60s, allow quality down to 0.5; floor at 0.4.
        let relaxed = self.config.min_match_quality - (oldest_wait_secs / 120.0) * 0.2;
        relaxed.max(0.4)
    }

    pub fn max_team_delta(&self, oldest_wait_secs: f64) -> f64 {
        let extra = (oldest_wait_secs / 30.0) * 10.0;
        (self.config.max_team_skill_delta + extra).min(150.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn skill_band_grows_with_wait() {
        let policy = RelaxationPolicy::new(MatchmakerConfig::default());
        let now = Utc::now();
        let enqueued = now - Duration::seconds(30);
        let band = policy.skill_band_for(enqueued, now);
        assert!(band > 100.0);
        assert!(band <= 500.0);
    }
}
