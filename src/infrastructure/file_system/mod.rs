mod r#trait;

pub use r#trait::{FileSystemError, FileSystemTrait};

#[cfg(test)]
pub use r#trait::MockFileSystemTrait;

use std::{
    fs::{self, File},
    io::{self, BufReader},
    path::Path,
};

#[cfg(test)]
mod tests;

pub struct OsFileSystem;

impl FileSystemTrait for OsFileSystem {
    fn read_to_string(&self, path: &Path) -> Result<String, FileSystemError> {
        Ok(fs::read_to_string(path)?)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FileSystemError> {
        Ok(fs::write(path, contents)?)
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), FileSystemError> {
        Ok(fs::create_dir_all(path)?)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), FileSystemError> {
        Ok(fs::rename(from, to)?)
    }

    fn create_writer(&self, path: &Path) -> Result<Box<dyn io::Write + Send>, FileSystemError> {
        let file = File::create(path)?;
        Ok(Box::new(file))
    }

    fn open_bufreader(&self, path: &Path) -> Result<Box<dyn io::BufRead + Send>, FileSystemError> {
        let file = File::open(path)?;
        Ok(Box::new(BufReader::new(file)))
    }

    fn metadata(&self, path: &Path) -> Result<fs::Metadata, FileSystemError> {
        Ok(fs::metadata(path)?)
    }
}
