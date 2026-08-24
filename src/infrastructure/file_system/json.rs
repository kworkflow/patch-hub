//! JSON persistence helpers over [`FileSystemTrait`].

use serde::Serialize;
use serde_json::to_writer_pretty;

use std::{io, path::Path};

use crate::infrastructure::file_system::{FileSystemError, FileSystemTrait};

/// Shared JSON persistence operations. Stateless namespace for methods used
/// by the JSON-backed stores (config repository, lore persistence, kw
/// history).
pub struct JsonUtils;

impl JsonUtils {
    /// Atomically writes `value` as pretty-printed JSON to `path`: the content
    /// goes to `<path>.tmp` first and is then renamed over `path`, so a crash
    /// mid-write cannot leave a truncated file behind. Parent directories are
    /// created as needed.
    pub fn atomic_write_json<T: ?Sized + Serialize>(
        fs: &dyn FileSystemTrait,
        value: &T,
        path: &str,
    ) -> Result<(), FileSystemError> {
        let path = Path::new(path);
        if let Some(parent) = path.parent() {
            fs.create_dir_all(parent)?;
        }

        let tmp_path = format!("{}.tmp", path.display());
        {
            let writer = fs.create_writer(Path::new(&tmp_path))?;
            to_writer_pretty(writer, value).map_err(io::Error::from)?;
        }
        fs.rename(Path::new(&tmp_path), path)?;
        Ok(())
    }
}
