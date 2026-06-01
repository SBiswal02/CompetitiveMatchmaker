use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum MatchmakerError {
    #[error("player {0} already in queue")]
    AlreadyQueued(Uuid),

    #[error("player {0} not in queue")]
    NotInQueue(Uuid),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}
