use super::*;

use std::sync::Mutex;

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn can_build() {
    let _lock = TEST_LOCK.lock().unwrap();

    env::set_var("HOME", "/fake/home/path");
    let config = Config::build();

    assert_eq!(30, config.get_page_size());
    assert_eq!(
        "/fake/home/path/.cache/patch_hub/patchsets",
        config.get_patchsets_cache_dir()
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/bookmarked_patchsets.json",
        config.get_bookmarked_patchsets_path()
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/mailing_lists.json",
        config.get_mailing_lists_path()
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/reviewed_patchsets.json",
        config.get_reviewed_patchsets_path()
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/logs",
        config.get_logs_path()
    );
}

#[test]
fn can_build_with_custom_values() {
    let _lock = TEST_LOCK.lock().unwrap();

    env::set_var("PATCH_HUB_CACHE_DIR", "/fake/cache/path");
    env::set_var("PATCH_HUB_DATA_DIR", "/fake/data/path");
    let config = Config::build();

    assert_eq!(30, config.get_page_size());
    assert_eq!(
        "/fake/cache/path/patchsets",
        config.get_patchsets_cache_dir()
    );
    assert_eq!(
        "/fake/data/path/bookmarked_patchsets.json",
        config.get_bookmarked_patchsets_path()
    );
    assert_eq!(
        "/fake/data/path/mailing_lists.json",
        config.get_mailing_lists_path()
    );
    assert_eq!(
        "/fake/data/path/reviewed_patchsets.json",
        config.get_reviewed_patchsets_path()
    );
    assert_eq!("/fake/data/path/logs", config.get_logs_path());

    env::remove_var("PATCH_HUB_CACHE_DIR");
    env::remove_var("PATCH_HUB_DATA_DIR");
}
