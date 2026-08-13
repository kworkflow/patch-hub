//! User-local history of patchset applies (and, in later steps, kw builds),
//! stored as JSON under the configured `data_dir`.
//!
//! Apply records feed kw build/deploy readiness and the KwOps branch prefill,
//! so they are user state — not a cache — and are never refreshed from lore.

use mockall::automock;
use serde::{Deserialize, Serialize};
use serde_json::{from_reader, to_writer_pretty};

use std::{collections::HashMap, io, path::Path, sync::Arc};

use crate::infrastructure::file_system::{FileSystemError, FileSystemTrait};

pub const APPLY_HISTORY_FILENAME: &str = "kw_apply_history.json";

/// One recorded `git am` application of a lore patchset to a kernel tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KwApplyRecord {
    pub message_id: String,
    pub kernel_tree_id: String,
    /// Snapshot of `KernelTree.path` when the record was written, so later
    /// readiness checks can detect the tree being repointed or moved.
    pub tree_path: String,
    pub applied_branch: String,
    pub base_branch: String,
    /// RFC3339 timestamp.
    pub applied_at: String,
}

#[automock]
pub trait KwHistoryStore: Send + Sync {
    /// Inserts or replaces the apply record keyed by `record.message_id`.
    fn record_apply(&self, record: KwApplyRecord) -> Result<(), FileSystemError>;

    /// Returns the apply record for `message_id`, or `None` if it was never
    /// recorded. A missing history file is a normal state, not an error.
    // Read by the kw readiness checks in a later step; kept per the
    // CachePolicy precedent (src/lore/application/cache.rs).
    #[allow(dead_code)]
    fn apply_record(&self, message_id: &str) -> Result<Option<KwApplyRecord>, FileSystemError>;
}

pub struct FileKwHistoryStore {
    fs: Arc<dyn FileSystemTrait>,
    apply_history_path: String,
}

impl FileKwHistoryStore {
    pub fn new(fs: Arc<dyn FileSystemTrait>, apply_history_path: String) -> Self {
        FileKwHistoryStore {
            fs,
            apply_history_path,
        }
    }

    fn load_apply_records(&self) -> Result<HashMap<String, KwApplyRecord>, FileSystemError> {
        let path = Path::new(&self.apply_history_path);
        if !self.fs.is_file(path) {
            return Ok(HashMap::new());
        }
        let reader = self.fs.open_bufreader(path)?;
        // A corrupt file is an error rather than an empty map: history must
        // never be silently clobbered by the next write.
        from_reader(reader)
            .map_err(io::Error::from)
            .map_err(FileSystemError::from)
    }

    /// Mirrors `FileLorePersistence::atomic_write_json`
    /// (src/lore/infrastructure/persistence.rs).
    fn atomic_write_json<T: Serialize + ?Sized>(
        &self,
        value: &T,
        path: &str,
    ) -> Result<(), FileSystemError> {
        if let Some(parent) = Path::new(path).parent() {
            self.fs.create_dir_all(parent)?;
        }

        let tmp_path = format!("{path}.tmp");
        {
            let writer = self.fs.create_writer(Path::new(&tmp_path))?;
            to_writer_pretty(writer, value).map_err(io::Error::from)?;
        }
        self.fs.rename(Path::new(&tmp_path), Path::new(path))?;
        Ok(())
    }
}

impl KwHistoryStore for FileKwHistoryStore {
    fn record_apply(&self, record: KwApplyRecord) -> Result<(), FileSystemError> {
        let mut records = self.load_apply_records()?;
        records.insert(record.message_id.clone(), record);
        self.atomic_write_json(&records, &self.apply_history_path)
    }

    fn apply_record(&self, message_id: &str) -> Result<Option<KwApplyRecord>, FileSystemError> {
        Ok(self.load_apply_records()?.remove(message_id))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::infrastructure::file_system::OsFileSystem;

    use super::*;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    fn tmp_dir(test_name: &str) -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "patch-hub-kw-history-{}-{}-{}",
            test_name,
            std::process::id(),
            n
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn store_at(dir: &Path) -> FileKwHistoryStore {
        FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            dir.join(APPLY_HISTORY_FILENAME)
                .to_str()
                .unwrap()
                .to_string(),
        )
    }

    fn record(message_id: &str, branch: &str) -> KwApplyRecord {
        KwApplyRecord {
            message_id: message_id.to_string(),
            kernel_tree_id: "mainline".to_string(),
            tree_path: "/home/user/linux".to_string(),
            applied_branch: branch.to_string(),
            base_branch: "master".to_string(),
            applied_at: "2026-08-01T17:30:00Z".to_string(),
        }
    }

    #[test]
    fn record_and_read_round_trip() {
        let dir = tmp_dir("round-trip");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "patchset-2026-08-01-17-30-00"))
            .unwrap();
        store
            .record_apply(record("msg-2", "patchset-2026-08-02-10-00-00"))
            .unwrap();

        assert_eq!(
            Some(record("msg-1", "patchset-2026-08-01-17-30-00")),
            store.apply_record("msg-1").unwrap()
        );
        assert_eq!(
            Some(record("msg-2", "patchset-2026-08-02-10-00-00")),
            store.apply_record("msg-2").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn record_with_same_message_id_overwrites() {
        let dir = tmp_dir("overwrite");
        let store = store_at(&dir);

        store.record_apply(record("msg-1", "patchset-old")).unwrap();
        store.record_apply(record("msg-1", "patchset-new")).unwrap();

        assert_eq!(
            Some(record("msg-1", "patchset-new")),
            store.apply_record("msg-1").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_history_file_reads_as_empty() {
        let dir = tmp_dir("missing");
        let store = store_at(&dir);

        assert_eq!(None, store.apply_record("msg-1").unwrap());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn record_creates_parent_directories() {
        let dir = tmp_dir("parents");
        let store = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            dir.join("nested")
                .join("deeper")
                .join(APPLY_HISTORY_FILENAME)
                .to_str()
                .unwrap()
                .to_string(),
        );

        store.record_apply(record("msg-1", "patchset-x")).unwrap();

        assert_eq!(
            Some(record("msg-1", "patchset-x")),
            store.apply_record("msg-1").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_history_file_errors_instead_of_clobbering() {
        let dir = tmp_dir("corrupt");
        let store = store_at(&dir);
        fs::write(dir.join(APPLY_HISTORY_FILENAME), b"not json").unwrap();

        assert!(store.apply_record("msg-1").is_err());
        assert!(store.record_apply(record("msg-1", "patchset-x")).is_err());
        // The corrupt file is left untouched for the user to inspect.
        assert_eq!(
            "not json",
            fs::read_to_string(dir.join(APPLY_HISTORY_FILENAME)).unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file() {
        let dir = tmp_dir("atomic");
        let store = store_at(&dir);

        store.record_apply(record("msg-1", "patchset-x")).unwrap();

        let tmp_left = fs::read_dir(&dir).unwrap().any(|e| {
            e.ok()
                .is_some_and(|x| x.file_name().to_string_lossy().ends_with(".tmp"))
        });
        assert!(!tmp_left, "atomic write should rename away .tmp");

        fs::remove_dir_all(&dir).unwrap();
    }
}
