use thiserror::Error;

/// Failures at the Input actor boundary.
#[derive(Debug, Error)]
pub enum InputError {
    /// The Input actor's receive channel is closed; the actor has stopped.
    #[error("input actor closed")]
    Closed,
}
