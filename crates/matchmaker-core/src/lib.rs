//! High-performance 5v5 matchmaking engine.
//!
//! Design goals:
//! - **Latency vs quality**: configurable relaxation widens skill bands over wait time.
//! - **Thread-safe pool**: concurrent workers scan via [`PlayerPool`]; eviction is atomic.
//! - **Team balance**: 10-player lobbies split to minimize skill delta between teams.
//! - **Cheap metrics**: atomics only on the hot path; histograms updated outside the scan.

mod balance;
mod config;
mod error;
mod matcher;
mod metrics;
mod pool;
mod relaxation;

pub use balance::{balance_teams, TeamBalance};
pub use config::MatchmakerConfig;
pub use error::MatchmakerError;
pub use matcher::{MatchCandidate, MatchmakerEngine};
pub use metrics::MatchmakerMetrics;
pub use pool::{PlayerPool, QueuedPlayer};
pub use relaxation::RelaxationPolicy;
