//! User-local history of patchset applies and kw builds, stored as JSON
//! under the configured `data_dir`.
//!
//! Apply and build records are user state — not a cache — and are never
//! refreshed from lore.

use mockall::automock;
use serde::{Deserialize, Serialize};
use serde_json::from_reader;

use std::{collections::HashMap, io, path::Path, sync::Arc};

use crate::infrastructure::file_system::{FileSystemError, FileSystemTrait, JsonUtils};

pub const APPLY_HISTORY_FILENAME: &str = "kw_apply_history.json";
pub const BUILD_HISTORY_FILENAME: &str = "kw_build_history.json";

/// One recorded `git am` application of a lore patchset to a kernel tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KwApplyRecord {
    pub message_id: String,
    pub kernel_tree_id: String,
    /// Snapshot of `KernelTree.path` when the record was written.
    pub tree_path: String,
    pub applied_branch: String,
    pub base_branch: String,
    /// RFC3339 timestamp.
    pub applied_at: String,
}

/// One recorded `kw build` attempt on a kernel tree branch, whether it
/// succeeded or not. Failed attempts are stored so deploy-alone can refuse
/// them instead of treating the tree as never built.
///
/// `message_id`, `arch`, `image_path`, and `kernelrelease` are optional: a
/// build can target a branch no patchset was applied to, `arch` is unknown
/// when image discovery had to glob, and a failed build may never have
/// produced an image or a kernelrelease.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KwBuildRecord {
    pub kernel_tree_id: String,
    /// Snapshot of `KernelTree.path` when the record was written.
    pub tree_path: String,
    pub message_id: Option<String>,
    pub branch: String,
    pub arch: Option<String>,
    pub image_path: Option<String>,
    /// Resolved kw-env `O=` dir at build time, if an env was active.
    pub output_dir: Option<String>,
    pub kernelrelease: Option<String>,
    pub log_path: String,
    /// RFC3339 timestamp. Readers compare parsed timestamps; records with
    /// unparseable `built_at` values sort oldest.
    pub built_at: String,
    pub success: bool,
}

/// message id → kernel tree id → record: applying the same patchset to
/// several trees keeps one record per tree.
type ApplyRecords = HashMap<String, HashMap<String, KwApplyRecord>>;

/// kernel tree id → branch → record: building several branches of the same
/// tree keeps one record per branch, so a failed build on one branch does
/// not clobber another branch's successful record.
type BuildRecords = HashMap<String, HashMap<String, KwBuildRecord>>;

#[automock]
pub trait KwHistoryStore: Send + Sync {
    /// Inserts or replaces the apply record for the record's
    /// `(message_id, kernel_tree_id)` pair.
    fn record_apply(&self, record: KwApplyRecord) -> Result<(), FileSystemError>;

    /// Returns the apply record for the `(message_id, kernel_tree_id)` pair,
    /// or `None` if it was never recorded. A missing history file is a normal
    /// state, not an error.
    #[allow(dead_code)]
    fn apply_record(
        &self,
        message_id: &str,
        kernel_tree_id: &str,
    ) -> Result<Option<KwApplyRecord>, FileSystemError>;

    /// Returns the newest apply record for the tree whose applied branch
    /// is `branch` — the link from a build's branch back to the patchset
    /// it came from. A missing history file is a normal state, not an
    /// error. Records with unparseable `applied_at` values sort oldest,
    /// same convention as the build records.
    // Read by KwActor when writing build records (the build step); kept
    // per the CachePolicy precedent (src/lore/application/cache.rs).
    #[allow(dead_code)]
    fn apply_record_for_branch(
        &self,
        kernel_tree_id: &str,
        branch: &str,
    ) -> Result<Option<KwApplyRecord>, FileSystemError>;

    /// Inserts or replaces the build record for the record's
    /// `(kernel_tree_id, branch)` pair.
    #[allow(dead_code)]
    fn record_build(&self, record: KwBuildRecord) -> Result<(), FileSystemError>;

    /// Returns the build record for the `(kernel_tree_id, branch)` pair, or
    /// `None` if it was never recorded. A missing history file is a normal
    /// state, not an error.
    #[allow(dead_code)]
    fn build_record(
        &self,
        kernel_tree_id: &str,
        branch: &str,
    ) -> Result<Option<KwBuildRecord>, FileSystemError>;

    /// Returns the chronologically newest build record for the tree, across
    /// branches, or `None` if none was recorded.
    #[allow(dead_code)]
    fn latest_build_record(
        &self,
        kernel_tree_id: &str,
    ) -> Result<Option<KwBuildRecord>, FileSystemError>;

    /// Returns the record for `(kernel_tree_id, branch)` and the newest
    /// record for the tree across branches from a single load of the
    /// history file — the pair a readiness snapshot is computed from.
    #[allow(dead_code)]
    fn build_records(
        &self,
        kernel_tree_id: &str,
        branch: &str,
    ) -> Result<(Option<KwBuildRecord>, Option<KwBuildRecord>), FileSystemError>;
}

pub struct FileKwHistoryStore {
    fs: Arc<dyn FileSystemTrait>,
    apply_history_path: String,
    build_history_path: String,
}

impl FileKwHistoryStore {
    /// Creates a store keeping both history files ([`APPLY_HISTORY_FILENAME`]
    /// and [`BUILD_HISTORY_FILENAME`]) directly under `data_dir`.
    pub fn new(fs: Arc<dyn FileSystemTrait>, data_dir: String) -> Self {
        FileKwHistoryStore {
            fs,
            apply_history_path: format!("{data_dir}/{APPLY_HISTORY_FILENAME}"),
            build_history_path: format!("{data_dir}/{BUILD_HISTORY_FILENAME}"),
        }
    }

    /// Loads a history file, or its empty default when the file does not
    /// exist. A corrupt file is an error rather than an empty map: history
    /// must never be silently clobbered by the next write.
    fn load_records<T>(&self, path: &str) -> Result<T, FileSystemError>
    where
        T: serde::de::DeserializeOwned + Default,
    {
        let path_ref = Path::new(path);
        if !self.fs.is_file(path_ref) {
            return Ok(T::default());
        }
        let reader = self.fs.open_bufreader(path_ref)?;
        from_reader(reader)
            .map_err(io::Error::from)
            .map_err(FileSystemError::from)
    }

    fn store_apply_record(&self, record: KwApplyRecord) -> Result<(), FileSystemError> {
        let mut records: ApplyRecords = self.load_records(&self.apply_history_path)?;
        records
            .entry(record.message_id.clone())
            .or_default()
            .insert(record.kernel_tree_id.clone(), record);
        JsonUtils::atomic_write_json(&*self.fs, &records, &self.apply_history_path)
    }

    fn store_build_record(&self, record: KwBuildRecord) -> Result<(), FileSystemError> {
        let mut records: BuildRecords = self.load_records(&self.build_history_path)?;
        records
            .entry(record.kernel_tree_id.clone())
            .or_default()
            .insert(record.branch.clone(), record);
        JsonUtils::atomic_write_json(&*self.fs, &records, &self.build_history_path)
    }

    /// Makes store errors self-describing so the apply hook's warning popup
    /// can point the user at the file to inspect or delete.
    fn error_with_path(&self, path: &str, error: FileSystemError) -> FileSystemError {
        FileSystemError::IoError(io::Error::other(format!("{path}: {error}")))
    }
}

impl KwHistoryStore for FileKwHistoryStore {
    fn record_apply(&self, record: KwApplyRecord) -> Result<(), FileSystemError> {
        self.store_apply_record(record)
            .map_err(|e| self.error_with_path(&self.apply_history_path, e))
    }

    fn apply_record(
        &self,
        message_id: &str,
        kernel_tree_id: &str,
    ) -> Result<Option<KwApplyRecord>, FileSystemError> {
        self.load_records(&self.apply_history_path)
            .map(|records: ApplyRecords| {
                records
                    .get(message_id)
                    .and_then(|by_tree| by_tree.get(kernel_tree_id))
                    .cloned()
            })
            .map_err(|e| self.error_with_path(&self.apply_history_path, e))
    }

    fn apply_record_for_branch(
        &self,
        kernel_tree_id: &str,
        branch: &str,
    ) -> Result<Option<KwApplyRecord>, FileSystemError> {
        self.load_records(&self.apply_history_path)
            .map(|records: ApplyRecords| {
                records
                    .values()
                    .filter_map(|by_tree| by_tree.get(kernel_tree_id))
                    .filter(|record| record.applied_branch == branch)
                    .max_by_key(|record| {
                        chrono::DateTime::parse_from_rfc3339(&record.applied_at).ok()
                    })
                    .cloned()
            })
            .map_err(|e| self.error_with_path(&self.apply_history_path, e))
    }

    fn record_build(&self, record: KwBuildRecord) -> Result<(), FileSystemError> {
        self.store_build_record(record)
            .map_err(|e| self.error_with_path(&self.build_history_path, e))
    }

    fn build_record(
        &self,
        kernel_tree_id: &str,
        branch: &str,
    ) -> Result<Option<KwBuildRecord>, FileSystemError> {
        self.build_records(kernel_tree_id, branch)
            .map(|(record, _)| record)
    }

    fn latest_build_record(
        &self,
        kernel_tree_id: &str,
    ) -> Result<Option<KwBuildRecord>, FileSystemError> {
        // The branch half of the pair is unused here.
        self.build_records(kernel_tree_id, "")
            .map(|(_, latest)| latest)
    }

    fn build_records(
        &self,
        kernel_tree_id: &str,
        branch: &str,
    ) -> Result<(Option<KwBuildRecord>, Option<KwBuildRecord>), FileSystemError> {
        self.load_records(&self.build_history_path)
            .map(|records: BuildRecords| {
                let by_branch = records.get(kernel_tree_id);
                let record = by_branch
                    .and_then(|by_branch| by_branch.get(branch))
                    .cloned();
                let latest = by_branch.and_then(|by_branch| {
                    by_branch
                        .values()
                        .max_by_key(|record| {
                            chrono::DateTime::parse_from_rfc3339(&record.built_at).ok()
                        })
                        .cloned()
                });
                (record, latest)
            })
            .map_err(|e| self.error_with_path(&self.build_history_path, e))
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
        FileKwHistoryStore::new(Arc::new(OsFileSystem), dir.to_str().unwrap().to_string())
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

    fn record_at(
        message_id: &str,
        kernel_tree_id: &str,
        branch: &str,
        applied_at: &str,
    ) -> KwApplyRecord {
        KwApplyRecord {
            applied_at: applied_at.to_string(),
            ..record(message_id, kernel_tree_id, branch)
        }
    }

    fn build(kernel_tree_id: &str, branch: &str, built_at: &str) -> KwBuildRecord {
        KwBuildRecord {
            kernel_tree_id: kernel_tree_id.to_string(),
            tree_path: format!("/home/user/{kernel_tree_id}"),
            message_id: None,
            branch: branch.to_string(),
            arch: Some("x86".to_string()),
            image_path: Some(format!("/home/user/{kernel_tree_id}/arch/x86/boot/bzImage")),
            output_dir: None,
            kernelrelease: Some("6.17.0".to_string()),
            log_path: "/home/user/.cache/patch_hub/kw_logs/build-1.log".to_string(),
            built_at: built_at.to_string(),
            success: true,
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
    fn apply_record_for_branch_finds_record_across_message_ids() {
        let dir = tmp_dir("branch-lookup");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .unwrap();
        store
            .record_apply(record("msg-2", "mainline", "patchset-y"))
            .unwrap();

        assert_eq!(
            Some(record("msg-2", "mainline", "patchset-y")),
            store
                .apply_record_for_branch("mainline", "patchset-y")
                .unwrap()
        );
        assert_eq!(
            None,
            store
                .apply_record_for_branch("mainline", "never-applied")
                .unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_record_for_branch_scopes_to_the_tree() {
        let dir = tmp_dir("branch-tree-scope");
        let store = store_at(&dir);

        // The same branch name applied to two trees resolves per tree.
        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .unwrap();
        store
            .record_apply(record("msg-2", "stable", "patchset-x"))
            .unwrap();

        assert_eq!(
            Some(record("msg-2", "stable", "patchset-x")),
            store
                .apply_record_for_branch("stable", "patchset-x")
                .unwrap()
        );
        assert_eq!(
            None,
            store
                .apply_record_for_branch("amd-gfx", "patchset-x")
                .unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_record_for_branch_returns_newest_reapply() {
        let dir = tmp_dir("branch-newest");
        let store = store_at(&dir);

        // Two patchsets applied onto the same branch name: the newest
        // applied_at wins, and unparseable timestamps sort oldest (the
        // build records' convention).
        store
            .record_apply(record_at(
                "msg-old",
                "mainline",
                "patchset-x",
                "2026-08-01T10:00:00Z",
            ))
            .unwrap();
        store
            .record_apply(record_at(
                "msg-new",
                "mainline",
                "patchset-x",
                "2026-08-02T10:00:00Z",
            ))
            .unwrap();
        store
            .record_apply(record_at(
                "msg-broken",
                "mainline",
                "patchset-x",
                "not a timestamp",
            ))
            .unwrap();

        assert_eq!(
            Some(record_at(
                "msg-new",
                "mainline",
                "patchset-x",
                "2026-08-02T10:00:00Z"
            )),
            store
                .apply_record_for_branch("mainline", "patchset-x")
                .unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_record_for_branch_missing_history_reads_as_none() {
        let dir = tmp_dir("branch-missing");
        let store = store_at(&dir);

        assert_eq!(
            None,
            store
                .apply_record_for_branch("mainline", "patchset-x")
                .unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn record_creates_parent_directories() {
        let dir = tmp_dir("parents");
        let store = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            dir.join("nested")
                .join("deeper")
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

    #[test]
    fn build_record_round_trip_including_failures() {
        let dir = tmp_dir("build-round-trip");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .unwrap();
        let mut failed = build("mainline", "patchset-x", "2026-08-02T09:00:00Z");
        failed.success = false;
        failed.image_path = None;
        failed.kernelrelease = None;
        store.record_build(failed.clone()).unwrap();

        // A failed attempt is stored, not dropped: deploy-alone readiness
        // refuses it.
        assert_eq!(
            Some(build("mainline", "for-next", "2026-08-01T18:10:00Z")),
            store.build_record("mainline", "for-next").unwrap()
        );
        assert_eq!(
            Some(failed),
            store.build_record("mainline", "patchset-x").unwrap()
        );
        assert_eq!(None, store.build_record("mainline", "master").unwrap());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_record_with_same_tree_and_branch_overwrites() {
        let dir = tmp_dir("build-overwrite");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .unwrap();
        store
            .record_build(build("mainline", "for-next", "2026-08-02T18:10:00Z"))
            .unwrap();

        assert_eq!(
            Some(build("mainline", "for-next", "2026-08-02T18:10:00Z")),
            store.build_record("mainline", "for-next").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_records_coexist_across_branches_and_trees() {
        let dir = tmp_dir("build-multi");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .unwrap();
        store
            .record_build(build("mainline", "patchset-x", "2026-08-02T18:10:00Z"))
            .unwrap();
        store
            .record_build(build("stable", "for-next", "2026-08-03T18:10:00Z"))
            .unwrap();

        assert_eq!(
            Some(build("mainline", "for-next", "2026-08-01T18:10:00Z")),
            store.build_record("mainline", "for-next").unwrap()
        );
        assert_eq!(
            Some(build("mainline", "patchset-x", "2026-08-02T18:10:00Z")),
            store.build_record("mainline", "patchset-x").unwrap()
        );
        assert_eq!(
            Some(build("stable", "for-next", "2026-08-03T18:10:00Z")),
            store.build_record("stable", "for-next").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn latest_build_record_picks_newest_across_branches() {
        let dir = tmp_dir("build-latest");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .unwrap();
        store
            .record_build(build("mainline", "patchset-x", "2026-08-03T18:10:00Z"))
            .unwrap();
        store
            .record_build(build("mainline", "master", "2026-08-02T18:10:00Z"))
            .unwrap();
        // Unparseable timestamps sort oldest.
        store
            .record_build(build("mainline", "broken-ts", "not a timestamp"))
            .unwrap();

        assert_eq!(
            Some(build("mainline", "patchset-x", "2026-08-03T18:10:00Z")),
            store.latest_build_record("mainline").unwrap()
        );
        assert_eq!(None, store.latest_build_record("amd-gfx").unwrap());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_records_returns_branch_match_and_latest_from_one_load() {
        let dir = tmp_dir("build-pair");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .unwrap();
        store
            .record_build(build("mainline", "patchset-x", "2026-08-03T18:10:00Z"))
            .unwrap();

        assert_eq!(
            (
                Some(build("mainline", "for-next", "2026-08-01T18:10:00Z")),
                Some(build("mainline", "patchset-x", "2026-08-03T18:10:00Z")),
            ),
            store.build_records("mainline", "for-next").unwrap()
        );
        // An unbuilt branch still reports the tree's latest record.
        assert_eq!(
            (
                None,
                Some(build("mainline", "patchset-x", "2026-08-03T18:10:00Z")),
            ),
            store.build_records("mainline", "never-built").unwrap()
        );
        assert_eq!(
            (None, None),
            store.build_records("amd-gfx", "for-next").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_build_history_reads_as_empty() {
        let dir = tmp_dir("build-missing");
        let store = store_at(&dir);

        assert_eq!(None, store.build_record("mainline", "for-next").unwrap());
        assert_eq!(None, store.latest_build_record("mainline").unwrap());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_build_history_errors_instead_of_clobbering() {
        let dir = tmp_dir("build-corrupt");
        let store = store_at(&dir);
        fs::write(dir.join(BUILD_HISTORY_FILENAME), b"not json").unwrap();

        let err = store.build_record("mainline", "for-next").unwrap_err();
        assert!(err.to_string().contains(BUILD_HISTORY_FILENAME));
        assert!(store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .is_err());
        assert_eq!(
            "not json",
            fs::read_to_string(dir.join(BUILD_HISTORY_FILENAME)).unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_atomic_write_leaves_no_tmp_file() {
        let dir = tmp_dir("build-atomic");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .unwrap();

        let tmp_left = fs::read_dir(&dir).unwrap().any(|e| {
            e.ok()
                .is_some_and(|x| x.file_name().to_string_lossy().ends_with(".tmp"))
        });
        assert!(!tmp_left, "atomic write should rename away .tmp");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_and_build_histories_are_independent_files() {
        let dir = tmp_dir("independent");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .unwrap();
        assert!(!dir.join(BUILD_HISTORY_FILENAME).exists());
        assert_eq!(None, store.build_record("mainline", "patchset-x").unwrap());

        store
            .record_build(build("mainline", "patchset-x", "2026-08-01T18:10:00Z"))
            .unwrap();
        assert!(dir.join(APPLY_HISTORY_FILENAME).exists());
        assert!(dir.join(BUILD_HISTORY_FILENAME).exists());
        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-x")),
            store.apply_record("msg-1", "mainline").unwrap()
        );

        fs::remove_dir_all(&dir).unwrap();
    }
}
