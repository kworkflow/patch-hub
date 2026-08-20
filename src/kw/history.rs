//! User-local history of patchset applies (and, in later steps, kw builds),
//! stored as JSON under the configured `data_dir`.
//!
//! Apply records feed kw build/deploy readiness and the KwOps branch prefill,
//! so they are user state — not a cache — and are never refreshed from lore.

use mockall::automock;
use serde::{Deserialize, Serialize};
use serde_json::from_reader;

use std::{collections::HashMap, io, path::Path, sync::Arc};

use crate::infrastructure::file_system::{FileSystemError, FileSystemTrait, JsonUtils};

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

/// message id → kernel tree id → record: applying the same patchset to
/// several trees keeps one record per tree.
type ApplyRecords = HashMap<String, HashMap<String, KwApplyRecord>>;

#[automock]
pub trait KwHistoryStore: Send + Sync {
    /// Inserts or replaces the apply record for the record's
    /// `(message_id, kernel_tree_id)` pair.
    fn record_apply(&self, record: KwApplyRecord) -> Result<(), FileSystemError>;

    /// Returns the apply record for the `(message_id, kernel_tree_id)` pair,
    /// or `None` if it was never recorded. A missing history file is a normal
    /// state, not an error.
    // Read by the kw readiness checks in a later step; kept per the
    // CachePolicy precedent (src/lore/application/cache.rs).
    #[allow(dead_code)]
    fn apply_record(
        &self,
        message_id: &str,
        kernel_tree_id: &str,
    ) -> Result<Option<KwApplyRecord>, FileSystemError>;
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

    fn load_apply_records(&self) -> Result<ApplyRecords, FileSystemError> {
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

    fn store_apply_record(&self, record: KwApplyRecord) -> Result<(), FileSystemError> {
        let mut records = self.load_apply_records()?;
        records
            .entry(record.message_id.clone())
            .or_default()
            .insert(record.kernel_tree_id.clone(), record);
        JsonUtils::atomic_write_json(&*self.fs, &records, &self.apply_history_path)
    }

    /// Makes store errors self-describing so the apply hook's warning popup
    /// can point the user at the file to inspect or delete.
    fn error_with_path(&self, error: FileSystemError) -> FileSystemError {
        FileSystemError::IoError(io::Error::other(format!(
            "{}: {error}",
            self.apply_history_path
        )))
    }
}

impl KwHistoryStore for FileKwHistoryStore {
    fn record_apply(&self, record: KwApplyRecord) -> Result<(), FileSystemError> {
        self.store_apply_record(record)
            .map_err(|e| self.error_with_path(e))
    }

    fn apply_record(
        &self,
        message_id: &str,
        kernel_tree_id: &str,
    ) -> Result<Option<KwApplyRecord>, FileSystemError> {
        self.load_apply_records()
            .map(|records| {
                records
                    .get(message_id)
                    .and_then(|by_tree| by_tree.get(kernel_tree_id))
                    .cloned()
            })
            .map_err(|e| self.error_with_path(e))
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
        // A leftover from a failed previous run (pid reuse + counter reset)
        // must not poison this one.
        let _ = fs::remove_dir_all(&dir);
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

    fn record(message_id: &str, kernel_tree_id: &str, branch: &str) -> KwApplyRecord {
        KwApplyRecord {
            message_id: message_id.to_string(),
            kernel_tree_id: kernel_tree_id.to_string(),
            tree_path: format!("/home/user/{kernel_tree_id}"),
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
            .record_apply(record("msg-1", "mainline", "patchset-2026-08-01-17-30-00"))
            .unwrap();
        store
            .record_apply(record("msg-2", "mainline", "patchset-2026-08-02-10-00-00"))
            .unwrap();

        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-2026-08-01-17-30-00")),
            store.apply_record("msg-1", "mainline").unwrap()
        );
        assert_eq!(
            Some(record("msg-2", "mainline", "patchset-2026-08-02-10-00-00")),
            store.apply_record("msg-2", "mainline").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn record_with_same_message_id_and_tree_overwrites() {
        let dir = tmp_dir("overwrite");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-old"))
            .unwrap();
        store
            .record_apply(record("msg-1", "mainline", "patchset-new"))
            .unwrap();

        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-new")),
            store.apply_record("msg-1", "mainline").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn same_message_id_applied_to_multiple_trees_coexists() {
        let dir = tmp_dir("multi-tree");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-mainline"))
            .unwrap();
        store
            .record_apply(record("msg-1", "stable", "patchset-stable"))
            .unwrap();

        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-mainline")),
            store.apply_record("msg-1", "mainline").unwrap()
        );
        assert_eq!(
            Some(record("msg-1", "stable", "patchset-stable")),
            store.apply_record("msg-1", "stable").unwrap()
        );
        assert_eq!(None, store.apply_record("msg-1", "amd-gfx").unwrap());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_history_file_reads_as_empty() {
        let dir = tmp_dir("missing");
        let store = store_at(&dir);

        assert_eq!(None, store.apply_record("msg-1", "mainline").unwrap());

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

        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .unwrap();

        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-x")),
            store.apply_record("msg-1", "mainline").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_history_file_errors_instead_of_clobbering() {
        let dir = tmp_dir("corrupt");
        let store = store_at(&dir);
        fs::write(dir.join(APPLY_HISTORY_FILENAME), b"not json").unwrap();

        let err = store.apply_record("msg-1", "mainline").unwrap_err();
        // Errors name the file so the warning popup can point at it.
        assert!(err.to_string().contains(APPLY_HISTORY_FILENAME));
        assert!(store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .is_err());
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

        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .unwrap();

        let tmp_left = fs::read_dir(&dir).unwrap().any(|e| {
            e.ok()
                .is_some_and(|x| x.file_name().to_string_lossy().ends_with(".tmp"))
        });
        assert!(!tmp_left, "atomic write should rename away .tmp");

        fs::remove_dir_all(&dir).unwrap();
    }
}
