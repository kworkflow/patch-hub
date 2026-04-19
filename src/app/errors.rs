use thiserror::Error;

use crate::lore::application::errors::LoreError;

/// Errors surfaced by the application orchestration layer (`App`).
///
/// Used as the typed boundary for future App-actor messages; many methods still
/// return `color_eyre::Result` during the refactor.
#[allow(dead_code)]
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
}
