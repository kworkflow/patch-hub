use serde_json::json;
use std::{
    collections::HashSet,
    env::VarError,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::config::state::ConfigState;
use crate::config::{ConfigService, ConfigServiceApi};
use crate::infrastructure::{env::MockEnvTrait, file_system::OsFileSystem};

static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

fn os_fs() -> OsFileSystem {
    OsFileSystem
}

fn unique_test_dir(prefix: &str) -> PathBuf {
    let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("patch-hub-{prefix}-{}-{}", std::process::id(), n));
    fs::create_dir_all(&p).unwrap();
    p
}

/// Writable `HOME` and mock env: no `PATCH_HUB_CONFIG_PATH` (uses `HOME/.config/...`).
fn default_env() -> (MockEnvTrait, PathBuf) {
    let home = unique_test_dir("home");
    let home_s = home.to_string_lossy().into_owned();
    let mut mock = MockEnvTrait::new();
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(|_| Err(VarError::NotPresent.into()));
    mock.expect_var()
        .withf(move |key| key == "HOME")
        .returning(move |_| Ok(home_s.clone()));
    mock.expect_var()
        .withf(|key| {
            matches!(
                key,
                "PATCH_HUB_PAGE_SIZE"
                    | "PATCH_HUB_CACHE_DIR"
                    | "PATCH_HUB_DATA_DIR"
                    | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                    | "PATCH_HUB_PATCH_RENDERER"
            )
        })
        .returning(|_| Err(VarError::NotPresent.into()));
    (mock, home)
}

/// Same logical content as `test_samples/app/config/config.json`, but paths under `root` so
/// `ensure_directories` stays inside a writable temp tree. After bootstrap, `normalize_derived_paths`
/// overwrites patchset/data paths from `cache_dir` and `data_dir` only (explicit per-field paths
/// in JSON are not preserved).
fn config_fixture_json(root: &Path) -> String {
    let patchsets_cache_dir = root.join("cachedir").join("path");
    let bookmarked = root.join("bookmarked").join("patchsets.json");
    let mailing = root.join("mailing").join("lists.json");
    let reviewed = root.join("reviewed").join("patchsets.json");
    let logs = root.join("logs");
    let cache_dir = root.join("cache_dir");
    let data_dir = root.join("data_dir");

    let v = json!({
      "page_size": 1234,
      "patchsets_cache_dir": patchsets_cache_dir.to_str(),
      "bookmarked_patchsets_path": bookmarked.to_str(),
      "mailing_lists_path": mailing.to_str(),
      "reviewed_patchsets_path": reviewed.to_str(),
      "logs_path": logs.to_str(),
      "git_send_email_options": "--long-option value -s -h -o -r -t",
      "cache_dir": cache_dir.to_str(),
      "data_dir": data_dir.to_str(),
      "patch_renderer": "default",
      "cover_renderer": "default",
      "max_log_age": 42,
      "kernel_trees": {
        "linux": {
          "path": "/home/user/linux",
          "branch": "master"
        },
        "amd-gfx": {
          "path": "/home/user/amd-gfx",
          "branch": "amd-staging-drm-next"
        }
      },
      "target_kernel_tree": "linux",
      "git_am_options": "--foo-bar foobar -s -n -o -r -l -a -x",
      "git_am_branch_prefix": "really-creative-prefix-"
    });
    serde_json::to_string_pretty(&v).unwrap()
}

#[test]
fn bootstrap_with_default_values() {
    let (env, home) = default_env();
    let h = home.to_string_lossy();
    let service = ConfigService::bootstrap(&env, os_fs()).unwrap();
    let config = service.snapshot();

    assert_eq!(30, config.page_size());
    assert_eq!(
        format!("{h}/.cache/patch_hub/patchsets"),
        config.patchsets_cache_dir().as_str()
    );
    assert_eq!(
        format!("{h}/.local/share/patch_hub/bookmarked_patchsets.json"),
        config.bookmarked_patchsets_path().as_str()
    );
    assert_eq!(
        format!("{h}/.local/share/patch_hub/mailing_lists.json"),
        config.mailing_lists_path().as_str()
    );
    assert_eq!(
        format!("{h}/.local/share/patch_hub/reviewed_patchsets.json"),
        config.reviewed_patchsets_path().as_str()
    );
    assert_eq!(
        format!("{h}/.local/share/patch_hub/logs"),
        config.logs_path().as_str()
    );
    assert_eq!(
        "--dry-run --suppress-cc=all",
        config.git_send_email_options().as_str()
    );
    assert_eq!(30, config.max_log_age());
    assert_eq!(HashSet::<&String>::new(), config.kernel_trees());
    assert!(config.target_kernel_tree().is_none());
    assert_eq!("", config.git_am_options().as_str());
    assert_eq!("patchset-", config.git_am_branch_prefix().as_str());
}

#[test]
fn bootstrap_with_config_file() {
    let fixture_root = unique_test_dir("fixture");
    let tmp_path = fixture_root.join("config.json");
    fs::write(&tmp_path, config_fixture_json(&fixture_root)).unwrap();
    let tmp_path_s = tmp_path.to_string_lossy().into_owned();

    let home = unique_test_dir("home-cfg");
    let home_s = home.to_string_lossy().into_owned();

    let mut mock = MockEnvTrait::new();
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(move |_| Ok(tmp_path_s.clone()));
    mock.expect_var()
        .withf(|key| key == "HOME")
        .returning(move |_| Ok(home_s.clone()));
    mock.expect_var()
        .withf(|key| {
            matches!(
                key,
                "PATCH_HUB_PAGE_SIZE"
                    | "PATCH_HUB_CACHE_DIR"
                    | "PATCH_HUB_DATA_DIR"
                    | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                    | "PATCH_HUB_PATCH_RENDERER"
            )
        })
        .returning(|_| Err(VarError::NotPresent.into()));

    let service = ConfigService::bootstrap(&mock, os_fs()).unwrap();
    let config = service.snapshot();

    // `normalize_derived_paths` recomputes these from `cache_dir` / `data_dir` after load.
    let patchsets_cache_dir = fixture_root.join("cache_dir").join("patchsets");
    let data_dir = fixture_root.join("data_dir");
    let bookmarked = data_dir.join("bookmarked_patchsets.json");
    let mailing = data_dir.join("mailing_lists.json");
    let reviewed = data_dir.join("reviewed_patchsets.json");
    let logs = data_dir.join("logs");

    assert_eq!(1234, config.page_size());
    assert_eq!(
        patchsets_cache_dir.to_string_lossy().as_ref(),
        config.patchsets_cache_dir().as_str()
    );
    assert_eq!(
        bookmarked.to_string_lossy().as_ref(),
        config.bookmarked_patchsets_path().as_str()
    );
    assert_eq!(
        mailing.to_string_lossy().as_ref(),
        config.mailing_lists_path().as_str()
    );
    assert_eq!(
        reviewed.to_string_lossy().as_ref(),
        config.reviewed_patchsets_path().as_str()
    );
    assert_eq!(logs.to_string_lossy().as_ref(), config.logs_path().as_str());
    assert_eq!(
        "--long-option value -s -h -o -r -t",
        config.git_send_email_options().as_str()
    );
    assert_eq!(42, config.max_log_age());
    assert_eq!(
        HashSet::from([&"linux".to_string(), &"amd-gfx".to_string()]),
        config.kernel_trees()
    );
    let linux = config.get_kernel_tree("linux").unwrap();
    assert_eq!(linux.path().as_str(), "/home/user/linux");
    assert_eq!(linux.branch().as_str(), "master");
    assert!(config.get_kernel_tree("invalid-id").is_none());
    assert_eq!(
        "linux",
        config.target_kernel_tree().as_ref().unwrap().as_str()
    );
    assert_eq!(
        "--foo-bar foobar -s -n -o -r -l -a -x",
        config.git_am_options().as_str()
    );
    assert_eq!(
        "really-creative-prefix-",
        config.git_am_branch_prefix().as_str()
    );
}

#[test]
fn bootstrap_with_env_vars() {
    let home = unique_test_dir("home-env");
    let home_s = home.to_string_lossy().into_owned();
    let cache_dir = home.join("fake-cache");
    let data_dir = home.join("fake-data");
    let cache_s = cache_dir.to_string_lossy().into_owned();
    let data_s = data_dir.to_string_lossy().into_owned();

    let mut mock = MockEnvTrait::new();
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(|_| Err(VarError::NotPresent.into()));
    mock.expect_var()
        .withf(move |key| key == "HOME")
        .returning(move |_| Ok(home_s.clone()));
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_PAGE_SIZE")
        .returning(|_| Ok("42".to_string()));
    mock.expect_var()
        .withf(move |key| key == "PATCH_HUB_CACHE_DIR")
        .returning(move |_| Ok(cache_s.clone()));
    mock.expect_var()
        .withf(move |key| key == "PATCH_HUB_DATA_DIR")
        .returning(move |_| Ok(data_s.clone()));
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS")
        .returning(|_| Ok("--option1 --option2".to_string()));
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_PATCH_RENDERER")
        .returning(|_| Err(VarError::NotPresent.into()));

    let service = ConfigService::bootstrap(&mock, os_fs()).unwrap();
    let config = service.snapshot();

    assert_eq!(42, config.page_size());
    assert_eq!(
        format!("{}/patchsets", cache_dir.to_string_lossy()),
        config.patchsets_cache_dir().as_str()
    );
    assert_eq!(
        format!("{}/bookmarked_patchsets.json", data_dir.to_string_lossy()),
        config.bookmarked_patchsets_path().as_str()
    );
    assert_eq!(
        format!("{}/mailing_lists.json", data_dir.to_string_lossy()),
        config.mailing_lists_path().as_str()
    );
    assert_eq!(
        format!("{}/reviewed_patchsets.json", data_dir.to_string_lossy()),
        config.reviewed_patchsets_path().as_str()
    );
    assert_eq!(
        format!("{}/logs", data_dir.to_string_lossy()),
        config.logs_path().as_str()
    );
    assert_eq!(
        "--option1 --option2",
        config.git_send_email_options().as_str()
    );
}

#[test]
fn bootstrap_config_precedence() {
    let (env, home) = default_env();
    let service = ConfigService::bootstrap(&env, os_fs()).unwrap();
    assert_eq!(30, service.snapshot().page_size());

    let fixture_root = unique_test_dir("prec");
    let tmp_path = fixture_root.join("config.json");
    fs::write(&tmp_path, config_fixture_json(&fixture_root)).unwrap();
    let tmp_path_s = tmp_path.to_string_lossy().into_owned();

    let home_s = home.to_string_lossy().into_owned();
    let mut env_with_file = MockEnvTrait::new();
    env_with_file
        .expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(move |_| Ok(tmp_path_s.clone()));
    env_with_file
        .expect_var()
        .withf(move |key| key == "HOME")
        .returning(move |_| Ok(home_s.clone()));
    env_with_file
        .expect_var()
        .withf(|key| {
            matches!(
                key,
                "PATCH_HUB_PAGE_SIZE"
                    | "PATCH_HUB_CACHE_DIR"
                    | "PATCH_HUB_DATA_DIR"
                    | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                    | "PATCH_HUB_PATCH_RENDERER"
            )
        })
        .returning(|_| Err(VarError::NotPresent.into()));

    let service = ConfigService::bootstrap(&env_with_file, os_fs()).unwrap();
    assert_eq!(1234, service.snapshot().page_size());

    let tmp_path_s2 = tmp_path.to_string_lossy().into_owned();
    let home_s2 = home.to_string_lossy().into_owned();
    let mut env_with_file_and_var = MockEnvTrait::new();
    env_with_file_and_var
        .expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(move |_| Ok(tmp_path_s2.clone()));
    env_with_file_and_var
        .expect_var()
        .withf(|key| key == "PATCH_HUB_PAGE_SIZE")
        .returning(|_| Ok("42".to_string()));
    env_with_file_and_var
        .expect_var()
        .withf(move |key| key == "HOME")
        .returning(move |_| Ok(home_s2.clone()));
    env_with_file_and_var
        .expect_var()
        .withf(|key| {
            matches!(
                key,
                "PATCH_HUB_CACHE_DIR"
                    | "PATCH_HUB_DATA_DIR"
                    | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                    | "PATCH_HUB_PATCH_RENDERER"
            )
        })
        .returning(|_| Err(VarError::NotPresent.into()));

    let service = ConfigService::bootstrap(&env_with_file_and_var, os_fs()).unwrap();
    assert_eq!(42, service.snapshot().page_size());

    let _ = fs::remove_file(&tmp_path);
}

#[test]
fn deserialize_config_state_with_missing_field() {
    let json_data = json!({
        "max_log_age": 500
    });

    let state: ConfigState = serde_json::from_value(json_data).unwrap();

    assert_eq!(state.page_size(), 30);
    assert_eq!(state.max_log_age(), 500);
}
