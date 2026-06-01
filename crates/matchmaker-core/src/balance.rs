//! Optimal 5v5 team split from 10 players (minimize skill imbalance).

use uuid::Uuid;

use matchmaker_types::SkillRating;

use crate::pool::QueuedPlayer;

/// Result of balancing two teams of five.
#[derive(Debug, Clone)]
pub struct TeamBalance {
    pub team_a: Vec<Uuid>,
    pub team_b: Vec<Uuid>,
    pub team_a_avg: SkillRating,
    pub team_b_avg: SkillRating,
    pub skill_delta: SkillRating,
}

/// Partition 10 players into two teams of 5 with minimal average skill gap.
///
/// Uses meet-in-the-middle over 5-combinations (C(10,5)=252), fast enough for hot path.
pub fn balance_teams(players: &[QueuedPlayer]) -> Option<TeamBalance> {
    if players.len() != 10 {
        return None;
    }

    const N: usize = 10;
    const HALF: usize = 5;
    let n = N;
    let half = HALF;
    let skills: Vec<f64> = players.iter().map(|p| p.skill).collect();
    let ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();

    let mut best_delta = f64::MAX;
    let mut best_mask: u16 = 0;

    // Enumerate all 5-subsets via bitmask (only low 10 bits used).
    for mask in 0u16..(1 << N) {
        if mask.count_ones() as usize != HALF {
            continue;
        }
        let mut sum_a = 0.0;
        for i in 0..n {
            if mask & (1 << i) != 0 {
                sum_a += skills[i];
            }
        }
        let sum_all: f64 = skills.iter().sum();
        let sum_b = sum_all - sum_a;
        let delta = (sum_a / half as f64 - sum_b / half as f64).abs();
        if delta < best_delta {
            best_delta = delta;
            best_mask = mask;
        }
    }

    let mut team_a = Vec::with_capacity(half);
    let mut team_b = Vec::with_capacity(half);
    for i in 0..n {
        if best_mask & (1 << i) != 0 {
            team_a.push(ids[i]);
        } else {
            team_b.push(ids[i]);
        }
    }

    let team_a_avg = team_a
        .iter()
        .map(|id| players.iter().find(|p| p.id == *id).unwrap().skill)
        .sum::<f64>()
        / half as f64;
    let team_b_avg = team_b
        .iter()
        .map(|id| players.iter().find(|p| p.id == *id).unwrap().skill)
        .sum::<f64>()
        / half as f64;

    Some(TeamBalance {
        team_a,
        team_b,
        team_a_avg,
        team_b_avg,
        skill_delta: best_delta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use matchmaker_types::Role;
    use uuid::Uuid;

    fn player(skill: f64) -> QueuedPlayer {
        QueuedPlayer {
            id: Uuid::new_v4(),
            skill,
            region: "us-east".into(),
            role: Role::Flex,
            party_id: None,
            enqueued_at: Utc::now(),
        }
    }

    #[test]
    fn splits_evenly_by_skill() {
        let players: Vec<_> = (0..10).map(|_| player(1500.0)).collect();
        let balance = balance_teams(&players).unwrap();
        assert!(balance.skill_delta < 1.0);
        assert_eq!(balance.team_a.len(), 5);
        assert_eq!(balance.team_b.len(), 5);
    }

    #[test]
    fn minimizes_delta_for_mixed_skills() {
        let players: Vec<_> = (0..10)
            .map(|i| player(if i % 2 == 0 { 1000.0 } else { 2000.0 }))
            .collect();
        let balance = balance_teams(&players).unwrap();
        // Best possible split: 3 low + 2 high vs 2 low + 3 high → delta 200.
        assert!(balance.skill_delta <= 200.0 + f64::EPSILON);
    }
}
