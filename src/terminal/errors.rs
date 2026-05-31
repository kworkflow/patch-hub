use thiserror::Error;

/// Failures at the terminal actor/session boundary.
#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("terminal session error: {0}")]
    Session(String),
    #[error("terminal actor unavailable: {0}")]
    ActorUnavailable(String),
}
