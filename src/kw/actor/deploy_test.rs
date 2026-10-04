use std::fs;

use tokio::time;

use super::{actor_test::*, *};
use crate::kw::{
    history::MockKwHistoryStore,
    models::{readiness::DeployAloneRefusal, remote::RemoteRefusal},
};

#[tokio::test]
async fn start_deploy_replies_immediately_and_runs_in_background() {
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-immediate",
        deploy_history(Some(matching_build_record())),
        deploy_ready_fs(),
    );

    let result = time::timeout(
        Duration::from_secs(1),
        handle.start_deploy(deploy_request()),
    )
    .await
    .expect("start_deploy must reply immediately");
    result.expect("deploy starts");

    let spawned = process.spawned();
    assert_eq!(1, spawned.len());
    assert_eq!("kw", spawned[0].program);
    assert_eq!(
        [
            "deploy",
            "--remote",
            "root@box:22",
            "--no-reboot",
            "--force"
        ]
        .as_slice(),
        spawned[0].args.as_slice()
    );
    assert_eq!(Path::new("/home/user/linux"), spawned[0].cwd);
    assert!(spawned[0].log_path.starts_with(&log_dir));
    assert!(spawned[0]
        .log_path
        .file_name()
        .expect("path has a file name")
        .to_string_lossy()
        .starts_with("deploy-"));

    let snapshot = handle.get_status().await.expect("status loads");
    assert!(
        matches!(
            snapshot.job,
            KwJobStatus::Running {
                kind: KwJobKind::Deploy,
                phase: KwPhase::Deploying,
                ..
            }
        ),
        "unexpected status: {:?}",
        snapshot.job
    );

    process.last_child().finish(0);
    let mut watch = handle.watch_status().await.expect("status watch opens");
    let status = wait_for_terminal_status(&mut watch).await;
    assert!(
        matches!(
            status,
            KwJobStatus::Succeeded {
                kind: KwJobKind::Deploy,
                ..
            }
        ),
        "unexpected status: {status:?}"
    );

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn deploy_exit_zero_with_initramfs_failure_succeeds_with_warnings() {
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-warnings",
        deploy_history(Some(matching_build_record())),
        deploy_ready_fs(),
    );
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_deploy(deploy_request())
        .await
        .expect("deploy starts");
    process.last_child().write_log(
        b"update-initramfs: Generating /boot/initrd.img-6.17.0\n\
          E: gzip compression (CONFIG_RD_GZIP) not supported by kernel\n\
          update-initramfs: failed for /boot/initrd.img-6.17.0 with 1.\n\
          Generating grub configuration file ...\n\
          Found linux image: /boot/vmlinuz-6.8.0-generic\n\
          done\n",
    );
    process.last_child().finish(0);

    match wait_for_terminal_status(&mut watch).await {
        KwJobStatus::Succeeded { kind, warnings, .. } => {
            assert_eq!(KwJobKind::Deploy, kind);
            assert_eq!(
                vec![
                    "update-initramfs: failed for /boot/initrd.img-6.17.0 with 1.".to_string(),
                    "GRUB did not list kernel 6.17.0".to_string(),
                ],
                warnings
            );
        }
        other => panic!("expected Succeeded, got {other:?}"),
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn build_then_deploy_checks_grub_for_the_release_it_just_built() {
    let (handle, process, log_dir) =
        spawn_deploy_actor("chain-warnings", quiet_history(), deploy_ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build_then_deploy(deploy_request())
        .await
        .expect("build then deploy starts");
    process.last_child().finish(0);
    wait_for_running_phase(&mut watch, KwPhase::Deploying).await;
    process.last_child().write_log(
        b"Generating grub configuration file ...\n\
          Found linux image: /boot/vmlinuz-6.8.0-generic\n\
          Found linux image: /boot/Image-6.17.0-rc1\n\
          done\n",
    );
    process.last_child().finish(0);

    match wait_for_terminal_status(&mut watch).await {
        KwJobStatus::Succeeded { warnings, .. } => {
            assert_eq!(
                vec!["GRUB did not list kernel 6.17.0".to_string()],
                warnings
            );
        }
        other => panic!("expected Succeeded, got {other:?}"),
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_merges_extras_and_follows_reboot_force_options() {
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-argv",
        deploy_history(Some(matching_build_record())),
        deploy_ready_fs(),
    );

    let mut request = deploy_request();
    request.deploy = Some(DeployOptions {
        reboot: true,
        force: false,
        boot_once_acknowledged: false,
    });
    request.extra_args = [
        "--verbose",
        "--local",
        "--alert=n",
        "--force",
        "--remote",
        "other:22",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    handle.start_deploy(request).await.expect("deploy starts");

    let spawned = process.spawned();
    assert_eq!(
        ["deploy", "--remote", "root@box:22", "--reboot", "--verbose"].as_slice(),
        spawned[0].args.as_slice()
    );

    process.last_child().finish(0);
    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_refused_without_a_build_record() {
    let (handle, process, log_dir) =
        spawn_deploy_actor("deploy-no-record", deploy_history(None), deploy_ready_fs());

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("deploy refused without a build");

    assert!(matches!(
        err,
        KwStartError::DeployAloneRefused(DeployAloneRefusal::NoBuildRecord)
    ));
    assert_eq!(
        KwJobStatus::Idle,
        handle.get_status().await.expect("status loads").job
    );
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_refused_when_last_build_failed() {
    let mut record = matching_build_record();
    record.success = false;
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-failed-build",
        deploy_history(Some(record)),
        deploy_ready_fs(),
    );

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("failed build refuses deploy");

    assert!(matches!(
        err,
        KwStartError::DeployAloneRefused(DeployAloneRefusal::LastBuildFailed)
    ));
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_refused_on_tree_path_drift() {
    let mut record = matching_build_record();
    record.tree_path = "/other/linux".to_string();
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-drift",
        deploy_history(Some(record)),
        deploy_ready_fs(),
    );

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("tree drift refuses deploy");

    assert!(matches!(
        err,
        KwStartError::DeployAloneRefused(DeployAloneRefusal::TreePathDrift { .. })
    ));
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_refused_on_output_dir_mismatch() {
    let mut record = matching_build_record();
    record.output_dir = Some("/cache/kw/envs/testenv".to_string());
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-env",
        deploy_history(Some(record)),
        deploy_ready_fs(),
    );

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("output dir mismatch refuses deploy");

    assert!(matches!(
        err,
        KwStartError::DeployAloneRefused(DeployAloneRefusal::OutputDirMismatch)
    ));
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_refused_when_image_is_missing() {
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-no-image",
        deploy_history(Some(matching_build_record())),
        deploy_fs(DEPLOY_REMOTE_CONFIG, DEPLOY_BOOT_ONCE_OFF, false),
    );

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("missing image refuses deploy");

    assert!(matches!(
        err,
        KwStartError::DeployAloneRefused(DeployAloneRefusal::ImageMissing)
    ));
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_refused_when_remote_is_unresolved() {
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-no-remote",
        MockKwHistoryStore::new(),
        deploy_fs("", DEPLOY_BOOT_ONCE_OFF, true),
    );

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("unresolved remote refuses deploy");

    assert!(matches!(
        err,
        KwStartError::RemoteUnresolved(RemoteRefusal::NoRemotesConfigured)
    ));
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_refused_when_boot_once_is_on_and_unacked() {
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-boot-once",
        deploy_history(Some(matching_build_record())),
        deploy_fs(DEPLOY_REMOTE_CONFIG, DEPLOY_BOOT_ONCE_ON, true),
    );

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("unacked boot-once refuses deploy");

    assert!(matches!(err, KwStartError::BootOnceNotAcknowledged));
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_refused_when_latest_build_is_on_another_branch() {
    let mut latest = matching_build_record();
    latest.branch = "other".to_string();
    let mut history = MockKwHistoryStore::new();
    history
        .expect_build_records()
        .withf(|tree, branch| tree == "mainline" && branch == "patchset-2026-08-01-17-30-00")
        .times(1)
        .returning(move |_, _| Ok((None, Some(latest.clone()))));
    history.expect_record_build().withf(|_| true).times(0);
    let (handle, process, log_dir) =
        spawn_deploy_actor("deploy-head-mismatch", history, deploy_ready_fs());

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("other branch refuses deploy");

    assert!(matches!(
        err,
        KwStartError::DeployAloneRefused(DeployAloneRefusal::HeadMismatch { .. })
    ));
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_proceeds_when_boot_once_is_on_and_acked() {
    let (handle, process, log_dir) = spawn_deploy_actor(
        "deploy-boot-once-acked",
        deploy_history(Some(matching_build_record())),
        deploy_fs(DEPLOY_REMOTE_CONFIG, DEPLOY_BOOT_ONCE_ON, true),
    );

    let mut request = deploy_request();
    request.deploy = Some(deploy_options(true));
    handle.start_deploy(request).await.expect("deploy starts");
    assert_eq!(1, process.spawned().len());

    process.last_child().finish(0);
    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_deploy_post_switch_refusal_rolls_the_switch_back() {
    let git = GitStub::on_branch("master");
    let (handle, process, log_dir) = spawn_full_actor(
        "deploy-rollback",
        deploy_history(None),
        git.shell(),
        deploy_ready_fs(),
        env_with_kw(),
    );

    let err = handle
        .start_deploy(deploy_request())
        .await
        .expect_err("post-switch refusal rolls back");

    assert!(matches!(
        err,
        KwStartError::DeployAloneRefused(DeployAloneRefusal::NoBuildRecord)
    ));
    assert!(process.spawned().is_empty());
    assert_eq!(git.head(), "master");
    assert!(matches!(
        handle.restore_previous_branch().await,
        Err(KwError::NoRecordedBranch)
    ));

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_then_deploy_chains_deploy_after_a_successful_build() {
    let (history, builds) = recording_history(None);
    let (handle, process, log_dir) =
        spawn_deploy_actor("chain-success", history, deploy_ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    let mut request = deploy_request();
    request.extra_args = ["--verbose", "--ccache", "--alert=n"]
        .into_iter()
        .map(String::from)
        .collect();
    time::timeout(
        Duration::from_secs(1),
        handle.start_build_then_deploy(request),
    )
    .await
    .expect("start_build_then_deploy must reply immediately")
    .expect("build then deploy starts");

    assert_eq!(1, process.spawned().len());
    assert_eq!(
        ["build", "--verbose", "--ccache"].as_slice(),
        process.spawned()[0].args.as_slice()
    );
    assert!(
        matches!(
            handle.get_status().await.expect("status loads").job,
            KwJobStatus::Running {
                kind: KwJobKind::BuildThenDeploy,
                phase: KwPhase::Building,
                ..
            }
        ),
        "unexpected status: {:?}",
        handle.get_status().await.expect("status loads").job
    );

    let build = process.last_child();
    build.finish(0);
    let deploying = wait_for_running_phase(&mut watch, KwPhase::Deploying).await;
    match deploying {
        KwJobStatus::Running {
            kind,
            phase,
            log_path,
            ..
        } => {
            assert_eq!(KwJobKind::BuildThenDeploy, kind);
            assert_eq!(KwPhase::Deploying, phase);
            assert!(log_path
                .file_name()
                .expect("path has a file name")
                .to_string_lossy()
                .starts_with("deploy-"));
        }
        other => panic!("expected Running Deploying, got {other:?}"),
    }

    let spawned = process.spawned();
    assert_eq!(2, spawned.len());
    assert_eq!(
        [
            "deploy",
            "--remote",
            "root@box:22",
            "--no-reboot",
            "--force",
            "--verbose",
        ]
        .as_slice(),
        spawned[1].args.as_slice()
    );
    assert_eq!(Path::new("/home/user/linux"), spawned[1].cwd);
    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        assert!(builds[0].success);
    }

    process.last_child().finish(0);
    let status = wait_for_terminal_status(&mut watch).await;
    assert!(
        matches!(
            status,
            KwJobStatus::Succeeded {
                kind: KwJobKind::BuildThenDeploy,
                ..
            }
        ),
        "unexpected status: {status:?}"
    );
    assert_eq!(1, builds.lock().expect("builds locks").len());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_then_deploy_skips_deploy_when_the_build_fails() {
    let (history, builds) = recording_history(None);
    let (handle, process, log_dir) =
        spawn_deploy_actor("chain-build-fail", history, deploy_ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build_then_deploy(deploy_request())
        .await
        .expect("build then deploy starts");
    process.last_child().finish(2);

    let status = wait_for_terminal_status(&mut watch).await;
    match status {
        KwJobStatus::Failed {
            kind,
            phase,
            exit_code,
            ..
        } => {
            assert_eq!(KwJobKind::BuildThenDeploy, kind);
            assert_eq!(KwPhase::Building, phase);
            assert_eq!(Some(2), exit_code);
        }
        other => panic!("expected Failed Building, got {other:?}"),
    }
    assert_eq!(1, process.spawned().len());
    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        assert!(!builds[0].success);
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_then_deploy_cancel_in_building_skips_deploy_and_record() {
    let (history, builds) = recording_history(None);
    let (handle, process, log_dir) =
        spawn_deploy_actor("chain-cancel-build", history, deploy_ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build_then_deploy(deploy_request())
        .await
        .expect("build then deploy starts");
    handle.cancel().await.expect("job cancels");

    let status = wait_for_terminal_status(&mut watch).await;
    match status {
        KwJobStatus::Cancelled { kind, phase, .. } => {
            assert_eq!(KwJobKind::BuildThenDeploy, kind);
            assert_eq!(KwPhase::Building, phase);
        }
        other => panic!("expected Cancelled Building, got {other:?}"),
    }
    assert_eq!(1, process.spawned().len());
    assert!(process.last_child().was_killed());
    assert!(builds.lock().expect("builds locks").is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_then_deploy_cancel_then_exit_zero_skips_deploy() {
    let (history, builds) = recording_history(None);
    let (handle, process, log_dir) =
        spawn_deploy_actor("chain-cancel-exit-zero", history, deploy_ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    process.ignore_sigterm(true);
    handle
        .start_build_then_deploy(deploy_request())
        .await
        .expect("build then deploy starts");
    handle.cancel().await.expect("job cancels");
    process.last_child().finish(0);

    let status = wait_for_terminal_status(&mut watch).await;
    match status {
        KwJobStatus::Cancelled { kind, phase, .. } => {
            assert_eq!(KwJobKind::BuildThenDeploy, kind);
            assert_eq!(KwPhase::Building, phase);
        }
        other => panic!("expected Cancelled Building, got {other:?}"),
    }
    assert_eq!(1, process.spawned().len());
    assert!(builds.lock().expect("builds locks").is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_then_deploy_cancel_in_deploying_keeps_the_build_record() {
    let (history, builds) = recording_history(None);
    let (handle, process, log_dir) =
        spawn_deploy_actor("chain-cancel-deploy", history, deploy_ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build_then_deploy(deploy_request())
        .await
        .expect("build then deploy starts");
    process.last_child().finish(0);
    let _ = wait_for_running_phase(&mut watch, KwPhase::Deploying).await;
    assert_eq!(1, builds.lock().expect("builds locks").len());

    handle.cancel().await.expect("job cancels");
    let status = wait_for_terminal_status(&mut watch).await;
    match status {
        KwJobStatus::Cancelled { kind, phase, .. } => {
            assert_eq!(KwJobKind::BuildThenDeploy, kind);
            assert_eq!(KwPhase::Deploying, phase);
        }
        other => panic!("expected Cancelled Deploying, got {other:?}"),
    }
    assert_eq!(2, process.spawned().len());
    assert!(process.last_child().was_killed());
    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        assert!(builds[0].success);
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_then_deploy_spawn_failure_at_boundary_keeps_the_build_record() {
    let (history, builds) = recording_history(None);
    let (handle, process, log_dir) =
        spawn_deploy_actor("chain-spawn-fail", history, deploy_ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build_then_deploy(deploy_request())
        .await
        .expect("build then deploy starts");
    let build = process.last_child();
    process.refuse_spawns(true);
    build.finish(0);

    let status = wait_for_terminal_status(&mut watch).await;
    match status {
        KwJobStatus::Failed {
            kind,
            phase,
            exit_code,
            ..
        } => {
            assert_eq!(KwJobKind::BuildThenDeploy, kind);
            assert_eq!(KwPhase::Deploying, phase);
            assert_eq!(None, exit_code);
        }
        other => panic!("expected Failed Deploying with no exit, got {other:?}"),
    }
    assert_eq!(1, process.spawned().len());
    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        assert!(builds[0].success);
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_then_deploy_refused_when_remote_is_unresolved() {
    let (handle, process, log_dir) = spawn_deploy_actor(
        "chain-no-remote",
        MockKwHistoryStore::new(),
        deploy_fs("", DEPLOY_BOOT_ONCE_OFF, true),
    );

    let err = handle
        .start_build_then_deploy(deploy_request())
        .await
        .expect_err("unresolved remote refuses chain");

    assert!(matches!(
        err,
        KwStartError::RemoteUnresolved(RemoteRefusal::NoRemotesConfigured)
    ));
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}
