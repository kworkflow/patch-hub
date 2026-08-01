use thiserror::Error;

/// Errors surfaced by the application orchestration layer (`App`).
///
/// Used as the typed boundary for `AppActor` messages and startup validation.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("missing required dependencies: {0}")]
    Dependencies(String),
}
