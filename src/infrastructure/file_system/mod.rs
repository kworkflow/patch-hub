mod json;
mod r#trait;

pub use json::JsonUtils;
pub use r#trait::{FileSystemError, FileSystemTrait};

#[cfg(test)]
pub use r#trait::MockFileSystemTrait;

use std::{
    fs::{self, File},
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
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

    fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError> {
        let mut entries = fs::read_dir(path)?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()?;
        entries.sort();
        Ok(entries)
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

    fn read_tail_to_string(
        &self,
        path: &Path,
        max_bytes: usize,
    ) -> Result<String, FileSystemError> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        let window = max_bytes as u64;
        let start = len.saturating_sub(window);
        file.seek(SeekFrom::Start(start))?;
        let mut buf = vec![0u8; (len - start) as usize];
        file.read_exact(&mut buf)?;
        let bytes = if start > 0 {
            match buf.iter().position(|&b| b == b'\n') {
                Some(index) => &buf[index + 1..],
                None => buf.as_slice(),
            }
        } else {
            buf.as_slice()
        };
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }
}
