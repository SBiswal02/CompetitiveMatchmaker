//! Concurrent matching workers that scan the pool on a fixed interval.

use std::sync::Arc;
use std::time::Duration;

use matchmaker_core::MatchmakerEngine;
use tracing::debug;

/// Spawn N independent workers; each runs `scan_once` on the shared engine.
pub fn spawn_workers(engine: Arc<MatchmakerEngine>, count: usize, interval_ms: u64) {
    let interval = Duration::from_millis(interval_ms);

    for worker_id in 0..count {
        let engine = engine.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;
                let matches = engine.scan_once();
                if !matches.is_empty() {
                    debug!(worker_id, count = matches.len(), "formed matches");
                }
            }
        });
    }
}
