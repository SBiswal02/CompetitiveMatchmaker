use crate::error::MatchmakerError;

/// Tunable parameters for the matchmaking loop.
#[derive(Debug, Clone)]
pub struct MatchmakerConfig {
    /// Players required per match (5v5 = 10).
    pub lobby_size: usize,
    /// Initial max skill difference between any two players in a lobby.
    pub initial_skill_band: f64,
    /// Skill band added per second of wait (relaxation).
    pub skill_band_per_second: f64,
    /// Hard cap on skill band after relaxation.
    pub max_skill_band: f64,
    /// Max seconds in queue before eviction.
    pub max_queue_time_secs: u64,
    /// Max acceptable |team_a_avg - team_b_avg| after split.
    pub max_team_skill_delta: f64,
    /// Minimum match quality score (0..1) to accept; lowers with wait via relaxation.
    pub min_match_quality: f64,
}

impl Default for MatchmakerConfig {
    fn default() -> Self {
        Self {
            lobby_size: 10,
            initial_skill_band: 100.0,
            skill_band_per_second: 5.0,
            max_skill_band: 500.0,
            max_queue_time_secs: 300,
            max_team_skill_delta: 50.0,
            min_match_quality: 0.7,
        }
    }
}

impl MatchmakerConfig {
    pub fn validate(&self) -> Result<(), MatchmakerError> {
        if self.lobby_size != 10 {
            return Err(MatchmakerError::InvalidConfig(
                "lobby_size must be 10 for 5v5".into(),
            ));
        }
        if self.initial_skill_band <= 0.0 || self.max_skill_band < self.initial_skill_band {
            return Err(MatchmakerError::InvalidConfig(
                "invalid skill band bounds".into(),
            ));
        }
        Ok(())
    }
}
