#![allow(dead_code)]

use thiserror::Error;

use crate::{
    infrastructure::file_system::FileSystemError,
    lore::infrastructure::http_lore_client::LoreHttpError,
};

#[derive(Debug, Error)]
pub enum LoreError {
    #[error("http error: {0}")]
    Http(#[from] LoreHttpError),

    #[error("persistence error: {0}")]
    Persistence(#[from] FileSystemError),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("patchset not found: {0}")]
    PatchNotFound(String),

    #[error("feed ended")]
    EndOfFeed,
}
