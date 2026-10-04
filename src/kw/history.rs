//! User-local history of patchset applies and kw builds, stored as JSON
//! under the configured `data_dir`.
//!
//! Apply and build records are user state — not a cache — and are never
//! refreshed from lore.

use mockall::automock;
use serde_json::from_reader;

use std::{collections::HashMap, io, path::Path, sync::Arc};

use crate::infrastructure::file_system::{FileSystemError, FileSystemTrait, JsonUtils};

use crate::kw::models::history::KwApplyRecord;
use crate::kw::models::history::KwBuildRecord;
use chrono::DateTime;
use serde::de::DeserializeOwned;

pub const APPLY_HISTORY_FILENAME: &str = "kw_apply_history.json";
pub const BUILD_HISTORY_FILENAME: &str = "kw_build_history.json";

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

    /// Returns the newest apply record for the tree whose applied branch
    /// is `branch` — the link from a build's branch back to the patchset
    /// it came from. A missing history file is a normal state, not an
    /// error. Records with unparseable `applied_at` values sort oldest,
    /// same convention as the build records.
    fn apply_record_for_branch(
        &self,
        kernel_tree_id: &str,
        branch: &str,
    ) -> Result<Option<KwApplyRecord>, FileSystemError>;

    /// Inserts or replaces the build record for the record's
    /// `(kernel_tree_id, branch)` pair.
    fn record_build(&self, record: KwBuildRecord) -> Result<(), FileSystemError>;

    /// Returns the record for `(kernel_tree_id, branch)` and the newest
    /// record for the tree across branches from a single load of the
    /// history file — the pair a readiness snapshot is computed from.
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
    /// Creates a store keeping `kw_apply_history.json` and
    /// `kw_build_history.json` under `data_dir`.
    pub fn new(fs: Arc<dyn FileSystemTrait>, data_dir: String) -> Self {
        FileKwHistoryStore {
            fs,
            apply_history_path: format!("{data_dir}/{APPLY_HISTORY_FILENAME}"),
            build_history_path: format!("{data_dir}/{BUILD_HISTORY_FILENAME}"),
        }
    }
}

impl FileKwHistoryStore {
    /// Loads a history file, or its empty default when the file does not
    /// exist. A corrupt file is an error rather than an empty map: history
    /// must never be silently clobbered by the next write.
    fn load_records<T>(&self, path: &str) -> Result<T, FileSystemError>
    where
        T: DeserializeOwned + Default,
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
                    .max_by_key(|record| DateTime::parse_from_rfc3339(&record.applied_at).ok())
                    .cloned()
            })
            .map_err(|e| self.error_with_path(&self.apply_history_path, e))
    }

    fn record_build(&self, record: KwBuildRecord) -> Result<(), FileSystemError> {
        self.store_build_record(record)
            .map_err(|e| self.error_with_path(&self.build_history_path, e))
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
                        .max_by_key(|record| DateTime::parse_from_rfc3339(&record.built_at).ok())
                        .cloned()
                });
                (record, latest)
            })
            .map_err(|e| self.error_with_path(&self.build_history_path, e))
    }
}

#[cfg(test)]
mod tests {

    mod helpers {
        use super::super::*;
        use crate::infrastructure::file_system::OsFileSystem;
        use std::env;
        use std::fs;
        use std::path::PathBuf;
        use std::process;
        use std::sync::atomic::{AtomicU64, Ordering};

        pub(super) static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

        pub(super) fn tmp_dir(test_name: &str) -> PathBuf {
            let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
            let dir = env::temp_dir().join(format!(
                "patch-hub-kw-history-{}-{}-{}",
                test_name,
                process::id(),
                n
            ));
            // A leftover from a failed previous run (pid reuse + counter reset)
            // must not poison this one.
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("dir creates");
            dir
        }

        pub(super) fn store_at(dir: &Path) -> FileKwHistoryStore {
            FileKwHistoryStore::new(
                Arc::new(OsFileSystem),
                dir.to_str().expect("path is utf-8").to_string(),
            )
        }

        pub(super) fn record(
            message_id: &str,
            kernel_tree_id: &str,
            branch: &str,
        ) -> KwApplyRecord {
            KwApplyRecord {
                message_id: message_id.to_string(),
                kernel_tree_id: kernel_tree_id.to_string(),
                tree_path: format!("/home/user/{kernel_tree_id}"),
                applied_branch: branch.to_string(),
                base_branch: "master".to_string(),
                applied_at: "2026-08-01T17:30:00Z".to_string(),
            }
        }

        pub(super) fn record_at(
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

        pub(super) fn build(kernel_tree_id: &str, branch: &str, built_at: &str) -> KwBuildRecord {
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
    }
    use helpers::*;
    use std::fs;

    use crate::infrastructure::file_system::OsFileSystem;

    use super::*;

    #[test]
    fn record_and_read_round_trip() {
        let dir = tmp_dir("round-trip");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-2026-08-01-17-30-00"))
            .expect("apply records");
        store
            .record_apply(record("msg-2", "mainline", "patchset-2026-08-02-10-00-00"))
            .expect("apply records");

        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-2026-08-01-17-30-00")),
            store
                .apply_record_for_branch("mainline", "patchset-2026-08-01-17-30-00")
                .expect("apply record loads")
        );
        assert_eq!(
            Some(record("msg-2", "mainline", "patchset-2026-08-02-10-00-00")),
            store
                .apply_record_for_branch("mainline", "patchset-2026-08-02-10-00-00")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn record_with_same_message_id_and_tree_overwrites() {
        let dir = tmp_dir("overwrite");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-old"))
            .expect("apply records");
        store
            .record_apply(record("msg-1", "mainline", "patchset-new"))
            .expect("apply records");

        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-new")),
            store
                .apply_record_for_branch("mainline", "patchset-new")
                .expect("apply record loads")
        );
        assert_eq!(
            None,
            store
                .apply_record_for_branch("mainline", "patchset-old")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn same_message_id_applied_to_multiple_trees_coexists() {
        let dir = tmp_dir("multi-tree");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-mainline"))
            .expect("apply records");
        store
            .record_apply(record("msg-1", "stable", "patchset-stable"))
            .expect("apply records");

        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-mainline")),
            store
                .apply_record_for_branch("mainline", "patchset-mainline")
                .expect("apply record loads")
        );
        assert_eq!(
            Some(record("msg-1", "stable", "patchset-stable")),
            store
                .apply_record_for_branch("stable", "patchset-stable")
                .expect("apply record loads")
        );
        assert_eq!(
            None,
            store
                .apply_record_for_branch("amd-gfx", "patchset-mainline")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn missing_history_file_reads_as_empty() {
        let dir = tmp_dir("missing");
        let store = store_at(&dir);

        assert_eq!(
            None,
            store
                .apply_record_for_branch("mainline", "patchset-x")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn apply_record_for_branch_finds_record_across_message_ids() {
        let dir = tmp_dir("branch-lookup");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .expect("apply records");
        store
            .record_apply(record("msg-2", "mainline", "patchset-y"))
            .expect("apply records");

        assert_eq!(
            Some(record("msg-2", "mainline", "patchset-y")),
            store
                .apply_record_for_branch("mainline", "patchset-y")
                .expect("apply record loads")
        );
        assert_eq!(
            None,
            store
                .apply_record_for_branch("mainline", "never-applied")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn apply_record_for_branch_scopes_to_the_tree() {
        let dir = tmp_dir("branch-tree-scope");
        let store = store_at(&dir);

        // The same branch name applied to two trees resolves per tree.
        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .expect("apply records");
        store
            .record_apply(record("msg-2", "stable", "patchset-x"))
            .expect("apply records");

        assert_eq!(
            Some(record("msg-2", "stable", "patchset-x")),
            store
                .apply_record_for_branch("stable", "patchset-x")
                .expect("apply record loads")
        );
        assert_eq!(
            None,
            store
                .apply_record_for_branch("amd-gfx", "patchset-x")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
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
            .expect("apply records");
        store
            .record_apply(record_at(
                "msg-new",
                "mainline",
                "patchset-x",
                "2026-08-02T10:00:00Z",
            ))
            .expect("apply records");
        store
            .record_apply(record_at(
                "msg-broken",
                "mainline",
                "patchset-x",
                "not a timestamp",
            ))
            .expect("apply records");

        assert_eq!(
            Some(record_at(
                "msg-new",
                "mainline",
                "patchset-x",
                "2026-08-02T10:00:00Z"
            )),
            store
                .apply_record_for_branch("mainline", "patchset-x")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn apply_record_for_branch_missing_history_reads_as_none() {
        let dir = tmp_dir("branch-missing");
        let store = store_at(&dir);

        assert_eq!(
            None,
            store
                .apply_record_for_branch("mainline", "patchset-x")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn record_creates_parent_directories() {
        let dir = tmp_dir("parents");
        let store = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            dir.join("nested")
                .join("deeper")
                .to_str()
                .expect("path is utf-8")
                .to_string(),
        );

        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .expect("apply records");

        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-x")),
            store
                .apply_record_for_branch("mainline", "patchset-x")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn corrupt_history_file_errors_instead_of_clobbering() {
        let dir = tmp_dir("corrupt");
        let store = store_at(&dir);
        fs::write(dir.join(APPLY_HISTORY_FILENAME), b"not json").expect("file writes");

        let err = store
            .apply_record_for_branch("mainline", "patchset-x")
            .unwrap_err();
        // Errors name the file so the warning popup can point at it.
        assert!(err.to_string().contains(APPLY_HISTORY_FILENAME));
        assert!(store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .is_err());
        // The corrupt file is left untouched for the user to inspect.
        assert_eq!(
            "not json",
            fs::read_to_string(dir.join(APPLY_HISTORY_FILENAME)).expect("file reads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file() {
        let dir = tmp_dir("atomic");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .expect("apply records");

        let tmp_left = fs::read_dir(&dir).expect("dir reads").any(|e| {
            e.ok()
                .is_some_and(|x| x.file_name().to_string_lossy().ends_with(".tmp"))
        });
        assert!(!tmp_left, "atomic write should rename away .tmp");

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn build_record_round_trip_including_failures() {
        let dir = tmp_dir("build-round-trip");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .expect("build records");
        let mut failed = build("mainline", "patchset-x", "2026-08-02T09:00:00Z");
        failed.success = false;
        failed.image_path = None;
        failed.kernelrelease = None;
        store.record_build(failed.clone()).expect("build records");

        // A failed attempt is stored, not dropped: deploy-alone readiness
        // refuses it. The newer failed record is also the tree's latest.
        assert_eq!(
            (
                Some(build("mainline", "for-next", "2026-08-01T18:10:00Z")),
                Some(failed.clone()),
            ),
            store
                .build_records("mainline", "for-next")
                .expect("build history loads")
        );
        assert_eq!(
            (Some(failed.clone()), Some(failed.clone())),
            store
                .build_records("mainline", "patchset-x")
                .expect("build history loads")
        );
        assert_eq!(
            (None, Some(failed)),
            store
                .build_records("mainline", "master")
                .expect("build history loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn build_record_with_same_tree_and_branch_overwrites() {
        let dir = tmp_dir("build-overwrite");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .expect("build records");
        store
            .record_build(build("mainline", "for-next", "2026-08-02T18:10:00Z"))
            .expect("build records");

        let overwritten = build("mainline", "for-next", "2026-08-02T18:10:00Z");
        assert_eq!(
            (Some(overwritten.clone()), Some(overwritten)),
            store
                .build_records("mainline", "for-next")
                .expect("build history loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn build_records_coexist_across_branches_and_trees() {
        let dir = tmp_dir("build-multi");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .expect("build records");
        store
            .record_build(build("mainline", "patchset-x", "2026-08-02T18:10:00Z"))
            .expect("build records");
        store
            .record_build(build("stable", "for-next", "2026-08-03T18:10:00Z"))
            .expect("build records");

        let mainline_latest = build("mainline", "patchset-x", "2026-08-02T18:10:00Z");
        assert_eq!(
            (
                Some(build("mainline", "for-next", "2026-08-01T18:10:00Z")),
                Some(mainline_latest.clone()),
            ),
            store
                .build_records("mainline", "for-next")
                .expect("build history loads")
        );
        assert_eq!(
            (Some(mainline_latest.clone()), Some(mainline_latest)),
            store
                .build_records("mainline", "patchset-x")
                .expect("build history loads")
        );
        let stable = build("stable", "for-next", "2026-08-03T18:10:00Z");
        assert_eq!(
            (Some(stable.clone()), Some(stable)),
            store
                .build_records("stable", "for-next")
                .expect("build history loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn build_records_picks_newest_across_branches() {
        let dir = tmp_dir("build-latest");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .expect("build records");
        store
            .record_build(build("mainline", "patchset-x", "2026-08-03T18:10:00Z"))
            .expect("build records");
        store
            .record_build(build("mainline", "master", "2026-08-02T18:10:00Z"))
            .expect("build records");
        // Unparseable timestamps sort oldest.
        store
            .record_build(build("mainline", "broken-ts", "not a timestamp"))
            .expect("build records");

        let newest = build("mainline", "patchset-x", "2026-08-03T18:10:00Z");
        assert_eq!(
            (
                Some(build("mainline", "for-next", "2026-08-01T18:10:00Z")),
                Some(newest.clone()),
            ),
            store
                .build_records("mainline", "for-next")
                .expect("build history loads")
        );
        assert_eq!(
            (Some(newest.clone()), Some(newest)),
            store
                .build_records("mainline", "patchset-x")
                .expect("build history loads")
        );
        assert_eq!(
            (None, None),
            store
                .build_records("amd-gfx", "patchset-x")
                .expect("build history loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn build_records_returns_branch_match_and_latest_from_one_load() {
        let dir = tmp_dir("build-pair");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .expect("build records");
        store
            .record_build(build("mainline", "patchset-x", "2026-08-03T18:10:00Z"))
            .expect("build records");

        assert_eq!(
            (
                Some(build("mainline", "for-next", "2026-08-01T18:10:00Z")),
                Some(build("mainline", "patchset-x", "2026-08-03T18:10:00Z")),
            ),
            store
                .build_records("mainline", "for-next")
                .expect("build history loads")
        );
        // An unbuilt branch still reports the tree's latest record.
        assert_eq!(
            (
                None,
                Some(build("mainline", "patchset-x", "2026-08-03T18:10:00Z")),
            ),
            store
                .build_records("mainline", "never-built")
                .expect("build history loads")
        );
        assert_eq!(
            (None, None),
            store
                .build_records("amd-gfx", "for-next")
                .expect("build history loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn missing_build_history_reads_as_empty() {
        let dir = tmp_dir("build-missing");
        let store = store_at(&dir);

        assert_eq!(
            (None, None),
            store
                .build_records("mainline", "for-next")
                .expect("build history loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn corrupt_build_history_errors_instead_of_clobbering() {
        let dir = tmp_dir("build-corrupt");
        let store = store_at(&dir);
        fs::write(dir.join(BUILD_HISTORY_FILENAME), b"not json").expect("file writes");

        let err = store.build_records("mainline", "for-next").unwrap_err();
        assert!(err.to_string().contains(BUILD_HISTORY_FILENAME));
        assert!(store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .is_err());
        assert_eq!(
            "not json",
            fs::read_to_string(dir.join(BUILD_HISTORY_FILENAME)).expect("file reads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn build_atomic_write_leaves_no_tmp_file() {
        let dir = tmp_dir("build-atomic");
        let store = store_at(&dir);

        store
            .record_build(build("mainline", "for-next", "2026-08-01T18:10:00Z"))
            .expect("build records");

        let tmp_left = fs::read_dir(&dir).expect("dir reads").any(|e| {
            e.ok()
                .is_some_and(|x| x.file_name().to_string_lossy().ends_with(".tmp"))
        });
        assert!(!tmp_left, "atomic write should rename away .tmp");

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }

    #[test]
    fn apply_and_build_histories_are_independent_files() {
        let dir = tmp_dir("independent");
        let store = store_at(&dir);

        store
            .record_apply(record("msg-1", "mainline", "patchset-x"))
            .expect("apply records");
        assert!(!dir.join(BUILD_HISTORY_FILENAME).exists());
        assert_eq!(
            (None, None),
            store
                .build_records("mainline", "patchset-x")
                .expect("build history loads")
        );

        store
            .record_build(build("mainline", "patchset-x", "2026-08-01T18:10:00Z"))
            .expect("build records");
        assert!(dir.join(APPLY_HISTORY_FILENAME).exists());
        assert!(dir.join(BUILD_HISTORY_FILENAME).exists());
        assert_eq!(
            Some(record("msg-1", "mainline", "patchset-x")),
            store
                .apply_record_for_branch("mainline", "patchset-x")
                .expect("apply record loads")
        );

        fs::remove_dir_all(&dir).expect("temp dir removes");
    }
}
