use serde_json::json;
use std::{env::VarError, fs, process::Command};

use super::*;

use crate::infrastructure::{env::MockEnvTrait, file_system::OsFileSystem};

fn os_fs() -> OsFileSystem {
    OsFileSystem
}

/// Returns a `MockEnvTrait` that responds to all PATCH_HUB_* and HOME/config
/// path vars with `NotPresent`, and returns `home` for `HOME`.
fn default_env(home: &str) -> MockEnvTrait {
    let home = home.to_string();
    let mut mock = MockEnvTrait::new();
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(|_| Err(VarError::NotPresent.into()));
    mock.expect_var()
        .withf(move |key| key == "HOME")
        .returning(move |_| Ok(home.clone()));
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
    mock
}

#[test]
/// Tests [`Config::build`]
fn can_build_with_default_values() {
    let env = default_env("/fake/home/path");
    let config = Config::build(&env, &os_fs());

    assert_eq!(30, config.page_size());
    assert_eq!(
        "/fake/home/path/.cache/patch_hub/patchsets",
        config.patchsets_cache_dir()
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/bookmarked_patchsets.json",
        config.bookmarked_patchsets_path()
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/mailing_lists.json",
        config.mailing_lists_path()
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/reviewed_patchsets.json",
        config.reviewed_patchsets_path()
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/logs",
        config.logs_path()
    );
    assert_eq!(
        "--dry-run --suppress-cc=all",
        config.git_send_email_options()
    );
    assert_eq!(30, config.max_log_age());
    assert_eq!(HashSet::<&String>::new(), config.kernel_trees());
    assert!(config.target_kernel_tree().is_none());
    assert_eq!("", config.git_am_options());
    assert_eq!("patchset-", config.git_am_branch_prefix());
}

#[test]
/// Tests [`Config::build`]
fn can_build_with_config_file() {
    let tmp_path = String::from_utf8(
        Command::new("mktemp")
            .output()
            .expect("Failed to create temporary file!")
            .stdout,
    )
    .expect("Couldn't convert `mktemp` output to String!")
    .trim()
    .to_string();

    fs::copy("test_samples/app/config/config.json", &tmp_path)
        .expect("Couldn't copy config sample file!");

    let tmp_path_clone = tmp_path.clone();
    let mut mock = MockEnvTrait::new();
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(move |_| Ok(tmp_path_clone.clone()));
    mock.expect_var()
        .withf(|key| {
            matches!(
                key,
                "HOME"
                    | "PATCH_HUB_PAGE_SIZE"
                    | "PATCH_HUB_CACHE_DIR"
                    | "PATCH_HUB_DATA_DIR"
                    | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                    | "PATCH_HUB_PATCH_RENDERER"
            )
        })
        .returning(|_| Err(VarError::NotPresent.into()));

    let config = Config::build(&mock, &os_fs());

    let _ = fs::remove_file(&tmp_path);

    assert_eq!(1234, config.page_size());
    assert_eq!("/cachedir/path", config.patchsets_cache_dir());
    assert_eq!(
        "/bookmarked/patchsets/path",
        config.bookmarked_patchsets_path()
    );
    assert_eq!("/mailing/lists/path", config.mailing_lists_path());
    assert_eq!("/reviewed/patchsets/path", config.reviewed_patchsets_path());
    assert_eq!("/logs/path", config.logs_path());
    assert_eq!(
        "--long-option value -s -h -o -r -t",
        config.git_send_email_options()
    );
    assert_eq!(42, config.max_log_age());
    assert_eq!(
        HashSet::from([&"linux".to_string(), &"amd-gfx".to_string()]),
        config.kernel_trees()
    );
    assert_eq!(
        &KernelTree {
            path: "/home/user/linux".to_string(),
            branch: "master".to_string()
        },
        config.get_kernel_tree("linux").unwrap()
    );
    assert!(config.get_kernel_tree("invalid-id").is_none());
    assert_eq!("linux", config.target_kernel_tree().as_ref().unwrap());
    assert_eq!(
        "--foo-bar foobar -s -n -o -r -l -a -x",
        config.git_am_options()
    );
    assert_eq!("really-creative-prefix-", config.git_am_branch_prefix());
}

#[test]
/// Tests [`Config::build`]
fn can_build_with_env_vars() {
    let mut mock = MockEnvTrait::new();
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(|_| Err(VarError::NotPresent.into()));
    mock.expect_var()
        .withf(|key| key == "HOME")
        .returning(|_| Ok("/fake/home/path".to_string()));
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_PAGE_SIZE")
        .returning(|_| Ok("42".to_string()));
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_CACHE_DIR")
        .returning(|_| Ok("/fake/cache/path".to_string()));
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_DATA_DIR")
        .returning(|_| Ok("/fake/data/path".to_string()));
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS")
        .returning(|_| Ok("--option1 --option2".to_string()));
    mock.expect_var()
        .withf(|key| key == "PATCH_HUB_PATCH_RENDERER")
        .returning(|_| Err(VarError::NotPresent.into()));

    let config = Config::build(&mock, &os_fs());

    assert_eq!(42, config.page_size());
    assert_eq!("/fake/cache/path/patchsets", config.patchsets_cache_dir());
    assert_eq!(
        "/fake/data/path/bookmarked_patchsets.json",
        config.bookmarked_patchsets_path()
    );
    assert_eq!(
        "/fake/data/path/mailing_lists.json",
        config.mailing_lists_path()
    );
    assert_eq!(
        "/fake/data/path/reviewed_patchsets.json",
        config.reviewed_patchsets_path()
    );
    assert_eq!("/fake/data/path/logs", config.logs_path());
    assert_eq!("--option1 --option2", config.git_send_email_options());
}

#[test]
/// Tests [`Config::build`]
fn test_config_precedence() {
    // Default values
    let env = default_env("/fake/home/path");
    let config = Config::build(&env, &os_fs());
    assert_eq!(30, config.page_size());

    // Config file should have precedence over default values
    let tmp_path = String::from_utf8(
        Command::new("mktemp")
            .output()
            .expect("Failed to create temporary file!")
            .stdout,
    )
    .expect("Couldn't convert `mktemp` output to String!")
    .trim()
    .to_string();

    fs::copy("test_samples/app/config/config.json", &tmp_path)
        .expect("Couldn't copy config sample file!");

    let tmp_path_clone = tmp_path.clone();
    let mut env_with_file = MockEnvTrait::new();
    env_with_file
        .expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(move |_| Ok(tmp_path_clone.clone()));
    env_with_file
        .expect_var()
        .withf(|key| {
            matches!(
                key,
                "HOME"
                    | "PATCH_HUB_PAGE_SIZE"
                    | "PATCH_HUB_CACHE_DIR"
                    | "PATCH_HUB_DATA_DIR"
                    | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                    | "PATCH_HUB_PATCH_RENDERER"
            )
        })
        .returning(|_| Err(VarError::NotPresent.into()));

    let config = Config::build(&env_with_file, &os_fs());
    assert_eq!(1234, config.page_size());

    // Env vars should have precedence over config file values
    let tmp_path_clone2 = tmp_path.clone();
    let mut env_with_file_and_var = MockEnvTrait::new();
    env_with_file_and_var
        .expect_var()
        .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
        .returning(move |_| Ok(tmp_path_clone2.clone()));
    env_with_file_and_var
        .expect_var()
        .withf(|key| key == "PATCH_HUB_PAGE_SIZE")
        .returning(|_| Ok("42".to_string()));
    env_with_file_and_var
        .expect_var()
        .withf(|key| {
            matches!(
                key,
                "HOME"
                    | "PATCH_HUB_CACHE_DIR"
                    | "PATCH_HUB_DATA_DIR"
                    | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                    | "PATCH_HUB_PATCH_RENDERER"
            )
        })
        .returning(|_| Err(VarError::NotPresent.into()));

    let config = Config::build(&env_with_file_and_var, &os_fs());
    assert_eq!(42, config.page_size());

    let _ = fs::remove_file(&tmp_path);
}

#[test]
fn test_deserialize_config_with_missing_field() {
    // Example JSON string that doesn't contain `page_size` but has `max_log_age` set to 500.
    let json_data = json!({
        "max_log_age": 500
    });

    let config: Config = serde_json::from_value(json_data).unwrap();

    // Assert that `page_size` is set to the default value (25)
    assert_eq!(config.page_size, 30);

    // Assert that `max_log_age` is set to the custom value
    assert_eq!(config.max_log_age, 500);
}
