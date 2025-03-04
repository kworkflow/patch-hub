use crate::env::Env;

use super::*;

#[tokio::test]
async fn can_build_with_default_values() {
    let env = Env::mock();

    env.set("HOME", "/fake/home/path").await;
    let (config, _) = ConfigCore::build(env.clone()).await.spawn();

    assert_eq!(30, config.usize(USizeOpt::PageSize).await);
    assert_eq!(
        "/fake/home/path/.cache/patch_hub/patchsets",
        config.string(StringOpt::PatchsetsCacheDir).await
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/bookmarked_patchsets.json",
        config.string(StringOpt::BookmarkedPatchsetsPath).await
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/mailing_lists.json",
        config.string(StringOpt::MailingListsPath).await
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/reviewed_patchsets.json",
        config.string(StringOpt::ReviewedPatchsetsPath).await
    );
    assert_eq!(
        "/fake/home/path/.local/share/patch_hub/logs",
        config.string(StringOpt::LogsPath).await
    );
    assert_eq!(
        "--dry-run --suppress-cc=all",
        config.string(StringOpt::GitSendEmailOptions).await
    );
    assert_eq!(30, config.usize(USizeOpt::MaxLogAge).await);
    assert_eq!(Vec::<String>::new(), config.kernel_trees().await);
    assert!(config.target_kernel_tree().await.is_none());
    assert_eq!("", config.string(StringOpt::GitAmOptions).await);
    assert_eq!(
        "patchset-",
        config.string(StringOpt::GitAmBranchPrefix).await
    );
}

#[tokio::test]
async fn can_build_with_config_file() {
    let env = Env::mock();

    env.set(
        "PATCH_HUB_CONFIG_PATH",
        "src/test_samples/app/config/config.json",
    )
    .await;
    let config = ConfigCore::build(env.clone()).await.spawn().0;
    env.unset("PATCH_HUB_CONFIG_PATH").await;

    assert_eq!(1234, config.usize(USizeOpt::PageSize).await);
    assert_eq!(
        "/cachedir/path",
        config.string(StringOpt::PatchsetsCacheDir).await
    );
    assert_eq!(
        "/bookmarked/patchsets/path",
        config.string(StringOpt::BookmarkedPatchsetsPath).await
    );
    assert_eq!(
        "/mailing/lists/path",
        config.string(StringOpt::MailingListsPath).await
    );
    assert_eq!(
        "/reviewed/patchsets/path",
        config.string(StringOpt::ReviewedPatchsetsPath).await
    );
    assert_eq!("/logs/path", config.string(StringOpt::LogsPath).await);
    assert_eq!(
        "--long-option value -s -h -o -r -t",
        config.string(StringOpt::GitSendEmailOptions).await
    );
    assert_eq!(42, config.usize(USizeOpt::MaxLogAge).await);

    let trees = config.kernel_trees().await;
    assert_eq!(trees.len(), 2);
    assert!(trees.contains(&"linux".to_string()));
    assert!(trees.contains(&"amd-gfx".to_string()));

    assert_eq!(
        KernelTree {
            path: "/home/user/linux".to_string(),
            branch: "master".to_string()
        },
        config.kernel_tree("linux".to_string()).await.unwrap()
    );
    assert!(config.kernel_tree("invalid-id".to_string()).await.is_none());
    assert_eq!("linux", config.target_kernel_tree().await.unwrap());
    assert_eq!(
        "--foo-bar foobar -s -n -o -r -l -a -x",
        config.string(StringOpt::GitAmOptions).await
    );
    assert_eq!(
        "really-creative-prefix-",
        config.string(StringOpt::GitAmBranchPrefix).await
    );
}

#[tokio::test]
async fn can_build_with_env_vars() {
    let env = Env::mock();
    env.set("HOME", "/fake/home/path").await;
    env.set("PATCH_HUB_PAGE_SIZE", "42").await;
    env.set("PATCH_HUB_CACHE_DIR", "/fake/cache/path").await;
    env.set("PATCH_HUB_DATA_DIR", "/fake/data/path").await;
    env.set("PATCH_HUB_GIT_SEND_EMAIL_OPTIONS", "--option1 --option2")
        .await;
    let config = ConfigCore::build(env.clone()).await.spawn().0;
    env.unset("PATCH_HUB_PAGE_SIZE").await;
    env.unset("PATCH_HUB_CACHE_DIR").await;
    env.unset("PATCH_HUB_DATA_DIR").await;
    env.unset("PATCH_HUB_GIT_SEND_EMAIL_OPTIONS").await;

    assert_eq!(42, config.usize(USizeOpt::PageSize).await);
    assert_eq!(
        "/fake/cache/path/patchsets",
        config.string(StringOpt::PatchsetsCacheDir).await
    );
    assert_eq!(
        "/fake/data/path/bookmarked_patchsets.json",
        config.string(StringOpt::BookmarkedPatchsetsPath).await
    );
    assert_eq!(
        "/fake/data/path/mailing_lists.json",
        config.string(StringOpt::MailingListsPath).await
    );
    assert_eq!(
        "/fake/data/path/reviewed_patchsets.json",
        config.string(StringOpt::ReviewedPatchsetsPath).await
    );
    assert_eq!(
        "/fake/data/path/logs",
        config.string(StringOpt::LogsPath).await
    );
    assert_eq!(
        "--option1 --option2",
        config.string(StringOpt::GitSendEmailOptions).await
    );

    env.unset("PATCH_HUB_CACHE_DIR").await;
    env.unset("PATCH_HUB_DATA_DIR").await;
}

#[tokio::test]
async fn test_config_precedence() {
    let env = Env::mock();
    // Default values
    env.set("HOME", "/fake/home/path").await;
    let config = ConfigCore::build(env.clone()).await.spawn().0;
    assert_eq!(30, config.usize(USizeOpt::PageSize).await);

    // Config file should have precedence over default values
    env.set(
        "PATCH_HUB_CONFIG_PATH",
        "src/test_samples/app/config/config.json",
    )
    .await;
    let config = ConfigCore::build(env.clone()).await.spawn().0;
    assert_eq!(1234, config.usize(USizeOpt::PageSize).await);

    // Env vars should have precedence over default values
    env.set("PATCH_HUB_PAGE_SIZE", "42").await;
    let config = ConfigCore::build(env.clone()).await.spawn().0;
    assert_eq!(42, config.usize(USizeOpt::PageSize).await);

    env.unset("PATCH_HUB_CONFIG_PATH").await;
    env.unset("PATCH_HUB_PAGE_SIZE").await;
}
