mod r#trait;

#[allow(unused_imports)]
pub use r#trait::{FileSystemError, FileSystemTrait};

use std::{io, path::Path};

#[allow(dead_code)]
pub struct OsFileSystem;

impl FileSystemTrait for OsFileSystem {
    fn read_to_string(&self, _path: &Path) -> Result<String, FileSystemError> {
        todo!()
    }

    fn write(&self, _path: &Path, _contents: &[u8]) -> Result<(), FileSystemError> {
        todo!()
    }

    fn create_dir_all(&self, _path: &Path) -> Result<(), FileSystemError> {
        todo!()
    }

    fn exists(&self, _path: &Path) -> bool {
        todo!()
    }

    fn is_file(&self, _path: &Path) -> bool {
        todo!()
    }

    fn is_dir(&self, _path: &Path) -> bool {
        todo!()
    }

    fn rename(&self, _from: &Path, _to: &Path) -> Result<(), FileSystemError> {
        todo!()
    }

    fn create_writer(&self, _path: &Path) -> Result<Box<dyn io::Write + Send>, FileSystemError> {
        todo!()
    }

    fn open_bufreader(&self, _path: &Path) -> Result<Box<dyn io::BufRead + Send>, FileSystemError> {
        todo!()
    }

    fn metadata(&self, _path: &Path) -> Result<std::fs::Metadata, FileSystemError> {
        todo!()
    }
}
