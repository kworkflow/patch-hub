use super::*;

use std::sync::Mutex;

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn can_build_with_default_values() {
    let _lock = TEST_LOCK.lock().unwrap();

    env::set_var("HOME", "/fake/home/path");
    let config = Config::build();

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
}

#[test]
fn can_build_with_config_file() {
    let _lock = TEST_LOCK.lock().unwrap();

    env::set_var(
        "PATCH_HUB_CONFIG_PATH",
        "src/test_samples/app/config/config.json",
    );
    let config = Config::build();
    env::remove_var("PATCH_HUB_CONFIG_PATH");

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
}

#[test]
fn can_build_with_env_vars() {
    let _lock = TEST_LOCK.lock().unwrap();

    env::set_var("PATCH_HUB_PAGE_SIZE", "42");
    env::set_var("PATCH_HUB_CACHE_DIR", "/fake/cache/path");
    env::set_var("PATCH_HUB_DATA_DIR", "/fake/data/path");
    env::set_var("PATCH_HUB_GIT_SEND_EMAIL_OPTIONS", "--option1 --option2");
    let config = Config::build();
    env::remove_var("PATCH_HUB_PAGE_SIZE");
    env::remove_var("PATCH_HUB_CACHE_DIR");
    env::remove_var("PATCH_HUB_DATA_DIR");
    env::remove_var("PATCH_HUB_GIT_SEND_EMAIL_OPTIONS");

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

    env::remove_var("PATCH_HUB_CACHE_DIR");
    env::remove_var("PATCH_HUB_DATA_DIR");
}

#[test]
fn test_config_precedence() {
    let _lock = TEST_LOCK.lock().unwrap();

    // Default values
    env::set_var("HOME", "/fake/home/path");
    let config = Config::build();
    assert_eq!(30, config.page_size());

    // Config file should have precedence over default values
    env::set_var(
        "PATCH_HUB_CONFIG_PATH",
        "src/test_samples/app/config/config.json",
    );
    let config = Config::build();
    assert_eq!(1234, config.page_size());

    // Env vars should have precedence over default values
    env::set_var("PATCH_HUB_PAGE_SIZE", "42");
    let config = Config::build();
    assert_eq!(42, config.page_size());

    env::remove_var("PATCH_HUB_CONFIG_PATH");
    env::remove_var("PATCH_HUB_PAGE_SIZE");
}
