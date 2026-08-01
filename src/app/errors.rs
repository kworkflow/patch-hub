use thiserror::Error;

use crate::{lore::application::errors::LoreError, ui::errors::UiError};

#[allow(dead_code)]
/// Errors surfaced by the application orchestration layer (`App`).
///
/// Used as the typed boundary for `AppActor` messages and startup validation.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid navigation state: {0}")]
    InvalidState(String),

    #[error("lore error: {0}")]
    Lore(#[from] LoreError),

    #[error("render error: {0}")]
    Render(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("ui error: {0}")]
    Ui(#[from] UiError),

    #[error("input error: {0}")]
    Input(String),

    #[error("missing required dependencies: {0}")]
    Dependencies(String),
}
