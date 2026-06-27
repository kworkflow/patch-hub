#![allow(dead_code)] // Wired to runtime in Commit 3 (phase 10).

use thiserror::Error;

/// Failures at the Input actor boundary.
#[derive(Debug, Error)]
pub enum InputError {
    #[error("subscriber already registered")]
    SubscriberAlreadyRegistered,
    #[error("failed to update input context: {0}")]
    ContextUpdate(String),
    #[error("input actor closed")]
    Closed,
}
