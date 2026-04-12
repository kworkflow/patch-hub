use mockall::automock;
use thiserror::Error;

use std::{io, path::Path};

#[derive(Debug, Error)]
pub enum FileSystemError {
    #[error("{0}")]
    IoError(#[from] io::Error),
}

#[allow(dead_code)]
#[automock]
pub trait FileSystemTrait: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Result<String, FileSystemError>;
    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FileSystemError>;
    fn create_dir_all(&self, path: &Path) -> Result<(), FileSystemError>;
    fn exists(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn rename(&self, from: &Path, to: &Path) -> Result<(), FileSystemError>;
    fn create_writer(&self, path: &Path) -> Result<Box<dyn io::Write + Send>, FileSystemError>;
    fn open_bufreader(&self, path: &Path) -> Result<Box<dyn io::BufRead + Send>, FileSystemError>;
    fn metadata(&self, path: &Path) -> Result<std::fs::Metadata, FileSystemError>;
}
