/// Errors produced by the UI actor boundary.
#[derive(thiserror::Error, Debug)]
pub enum UiError {
    #[error("failed to build scene: {0}")]
    Build(String),

    #[error("invalid screen state for UI: {0}")]
    InvalidState(String),

    #[error("ui actor unavailable: {0}")]
    ActorUnavailable(String),
}
