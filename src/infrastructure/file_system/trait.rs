use mockall::automock;
use thiserror::Error;

use std::{
    fs::Metadata,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Error)]
pub enum FileSystemError {
    #[error("{0}")]
    IoError(#[from] io::Error),
}

#[automock]
pub trait FileSystemTrait: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Result<String, FileSystemError>;
    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FileSystemError>;
    fn create_dir_all(&self, path: &Path) -> Result<(), FileSystemError>;
    fn exists(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    /// Returns the immediate children of directory `path` as full paths,
    /// sorted for determinism. Entry kind and metadata are queried
    /// separately via `is_dir`/`is_file`/`metadata`.
    // No production caller until the kw readiness probes land; kept per the
    // CachePolicy precedent (src/lore/application/cache.rs).
    #[allow(dead_code)]
    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError>;
    fn rename(&self, from: &Path, to: &Path) -> Result<(), FileSystemError>;
    fn create_writer(&self, path: &Path) -> Result<Box<dyn io::Write + Send>, FileSystemError>;
    fn open_bufreader(&self, path: &Path) -> Result<Box<dyn io::BufRead + Send>, FileSystemError>;
    fn metadata(&self, path: &Path) -> Result<Metadata, FileSystemError>;
}
