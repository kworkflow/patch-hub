use std::{
    env, fs, io,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

use chrono::DateTime;
use tokio::time;

use crate::{
    infrastructure::{
        env::MockEnvTrait,
        file_system::{FileSystemError, MockFileSystemTrait},
        process::FakeProcess,
        shell::{MockShellTrait, ShellOutput},
    },
    kw::{
        errors::KwStartError,
        history::MockKwHistoryStore,
        models::{
            readiness::{BootOnceState, DeployAloneRefusal, TreeReadiness},
            remote::RemoteRefusal,
        },
        status::{KwJobKind, KwJobStatus, KwPhase},
    },
};

use super::*;

mod helpers {
    use super::super::*;
    use crate::{
        infrastructure::{
            env::MockEnvTrait,
            file_system::{FileSystemError, MockFileSystemTrait},
            process::FakeProcess,
            shell::{MockShellTrait, ShellCommand, ShellOutput},
        },
        kw::{
            history::MockKwHistoryStore,
            messages::{DeployOptions, StartRequest},
            models::history::{KwApplyRecord, KwBuildRecord},
            status::{KwJobStatus, KwPhase},
        },
    };

    use std::{
        env, fs, io,
        path::Path,
        process,
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Mutex,
        },
        time::Duration,
    };
    use tokio::time;

    pub static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    /// A real directory: FakeProcess creates the log file on spawn, so the
    /// parent must exist even though the fs trait is mocked.
    pub fn tmp_log_dir(test_name: &str) -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = env::temp_dir().join(format!(
            "patch-hub-kw-actor-{}-{test_name}-{n}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir creates");
        dir
    }

    pub fn kernel_tree(path: &Path) -> KernelTree {
        serde_json::from_value(serde_json::json!({
            "path": path.to_str().expect("path is utf-8"),
            "branch": "master"
        }))
        .expect("json parses")
    }

    pub fn start_request() -> StartRequest {
        StartRequest {
            kernel_tree_id: "mainline".to_string(),
            tree: kernel_tree(Path::new("/home/user/linux")),
            branch: "patchset-2026-08-01-17-30-00".to_string(),
            extra_args: Vec::new(),
            deploy: None,
        }
    }

    /// Spawns the actor with the already-configured mocks (mockall
    /// expectations need `&mut`, so they are set before the mocks move
    /// behind `Arc`s). The log dir is a real unique temp dir even though
    /// these tests never start a job, so a future test that accidentally
    /// does cannot share a fixed path with the job tests.
    pub fn spawn_test_actor(
        test_name: &str,
        history: MockKwHistoryStore,
        shell: MockShellTrait,
        fs: MockFileSystemTrait,
        env: MockEnvTrait,
    ) -> KwHandle {
        KwActor::spawn(
            Arc::new(history),
            Arc::new(FakeProcess::new()),
            Arc::new(shell),
            Arc::new(fs),
            Arc::new(env),
            tmp_log_dir(test_name),
        )
    }

    pub const KW_VERSION_OK: &[u8] = b"kw, version 0.10.0\n";
    /// A clean `git status --porcelain` answer.
    pub const CLEAN_STATUS: (&[u8], &[u8], bool) = (b"", b"", true);
    /// A successful `git switch` answer.
    pub const SWITCH_OK: (&[u8], bool) = (b"", true);

    pub fn command_parts(cmd: &ShellCommand) -> Vec<String> {
        let mut parts = vec![cmd.program.clone()];
        parts.extend(cmd.args.clone());
        parts
    }

    pub fn command(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    /// A shell mock that logs every command's argv parts and answers by
    /// content: kw's version probe with `kw_version`; `git status
    /// --porcelain` with `status` (stdout, stderr, success); `git switch`
    /// with `switch` (stderr, success); any other git call — the HEAD
    /// branch probe — with `master`.
    pub fn recording_shell(
        kw_version: &'static [u8],
        status: (&'static [u8], &'static [u8], bool),
        switch: (&'static [u8], bool),
    ) -> (MockShellTrait, Arc<Mutex<Vec<Vec<String>>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_in_shell = Arc::clone(&calls);
        let mut shell = MockShellTrait::new();
        shell
            .expect_execute()
            .withf(|cmd| {
                cmd.program == "kw" && cmd.args == ["--version"]
                    || cmd.program == "git"
                        && cmd.args
                            == [
                                "-C",
                                "/home/user/linux",
                                "status",
                                "--porcelain",
                                "--untracked-files=no",
                            ]
                    || cmd.program == "git"
                        && cmd.args == ["-C", "/home/user/linux", "branch", "--show-current"]
                    || cmd.program == "git"
                        && cmd.args
                            == [
                                "-C",
                                "/home/user/linux",
                                "switch",
                                "--",
                                "patchset-2026-08-01-17-30-00",
                            ]
                    || cmd.program == "git"
                        && cmd.args == ["-C", "/home/user/linux", "switch", "--", "master"]
            })
            .times(2..=9)
            .returning(move |cmd| {
                calls_in_shell
                    .lock()
                    .expect("calls in shell locks")
                    .push(command_parts(cmd));
                let output = |stdout: &[u8], stderr: &[u8], success: bool| ShellOutput {
                    stdout: stdout.to_vec(),
                    stderr: stderr.to_vec(),
                    success,
                };
                if cmd.program == "kw" {
                    return Ok(output(kw_version, b"", true));
                }
                if cmd.args.iter().any(|arg| arg == "status") {
                    return Ok(output(status.0, status.1, status.2));
                }
                if cmd.args.iter().any(|arg| arg == "switch") {
                    return Ok(output(b"", switch.0, switch.1));
                }
                Ok(output(b"master\n", b"", true))
            });
        (shell, calls)
    }

    /// Shell double for checkout/restore tests: kw version probe answers
    /// 0.10.0, `git status --porcelain` reflects the dirty flag, the HEAD
    /// probe reports `head` (including git's trailing newline), and a
    /// successful `git switch` updates `head`. Switches to `fail_switch_to`
    /// fail, so going forward can succeed while coming back fails.
    pub struct GitStub {
        head: Arc<Mutex<String>>,
        dirty: Arc<AtomicBool>,
        fail_switch_to: Arc<Mutex<Option<String>>>,
    }

    impl GitStub {
        pub fn on_branch(branch: &str) -> Self {
            Self {
                head: Arc::new(Mutex::new(branch.to_string())),
                dirty: Arc::new(AtomicBool::new(false)),
                fail_switch_to: Arc::new(Mutex::new(None)),
            }
        }

        pub fn head(&self) -> String {
            self.head.lock().expect("head locks").clone()
        }

        pub fn shell(&self) -> MockShellTrait {
            let head = Arc::clone(&self.head);
            let dirty = Arc::clone(&self.dirty);
            let fail_switch_to = Arc::clone(&self.fail_switch_to);
            let mut shell = MockShellTrait::new();
            shell
                .expect_execute()
                .withf(|cmd| {
                    cmd.program == "kw" && cmd.args == ["--version"]
                        || cmd.program == "git"
                            && cmd.args
                                == [
                                    "-C",
                                    "/home/user/linux",
                                    "status",
                                    "--porcelain",
                                    "--untracked-files=no",
                                ]
                        || cmd.program == "git"
                            && cmd.args == ["-C", "/home/user/linux", "branch", "--show-current"]
                        || cmd.program == "git"
                            && cmd.args
                                == [
                                    "-C",
                                    "/home/user/linux",
                                    "switch",
                                    "--",
                                    "patchset-2026-08-01-17-30-00",
                                ]
                        || cmd.program == "git"
                            && cmd.args == ["-C", "/home/user/linux", "switch", "--", "master"]
                        || cmd.program == "git"
                            && cmd.args
                                == ["-C", "/home/user/linux", "switch", "--", "patchset-two"]
                })
                .times(5..=11)
                .returning(move |cmd| {
                    let output = |stdout: &[u8]| ShellOutput {
                        stdout: stdout.to_vec(),
                        stderr: Vec::new(),
                        success: true,
                    };
                    if cmd.program == "kw" {
                        return Ok(output(KW_VERSION_OK));
                    }
                    if cmd.args.iter().any(|arg| arg == "status") {
                        let stdout: &[u8] = if dirty.load(Ordering::Relaxed) {
                            b" M src/main.c\n"
                        } else {
                            b""
                        };
                        return Ok(output(stdout));
                    }
                    if cmd.args.iter().any(|arg| arg == "switch") {
                        let branch = cmd.args.last().expect("iterator yields last").clone();
                        if fail_switch_to
                            .lock()
                            .expect("fail switch to locks")
                            .as_deref()
                            == Some(branch.as_str())
                        {
                            return Ok(ShellOutput {
                                stdout: Vec::new(),
                                stderr: b"error: you need to resolve your current index first\n"
                                    .to_vec(),
                                success: false,
                            });
                        }
                        *head.lock().expect("head locks") = branch;
                        return Ok(output(b""));
                    }
                    let current = format!("{}\n", head.lock().expect("head locks"));
                    Ok(output(current.as_bytes()))
                });
            shell
        }
    }

    impl GitStub {
        pub(super) fn set_dirty(&self, dirty: bool) {
            self.dirty.store(dirty, Ordering::Relaxed);
        }

        pub(super) fn fail_switches_to(&self, branch: Option<&str>) {
            *self.fail_switch_to.lock().expect("fail switch to locks") = branch.map(str::to_string);
        }
    }

    /// fs answers for a ready kernel tree with no active kw env: the
    /// kernel-root probes pass, `.config` exists, `.kw/env.current` is
    /// absent, `.kw/build.config` is unreadable (arch probes as None),
    /// and there is no arch/ dir to glob images from.
    pub fn expect_ready_tree(fs: &mut MockFileSystemTrait) {
        fs.expect_is_dir()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux")
                    || path == std::path::Path::new("/home/user/linux/.kw")
                    || path == std::path::Path::new("/home/user/linux/Documentation")
                    || path == std::path::Path::new("/home/user/linux/arch")
                    || path == std::path::Path::new("/home/user/linux/drivers")
                    || path == std::path::Path::new("/home/user/linux/fs")
                    || path == std::path::Path::new("/home/user/linux/include")
                    || path == std::path::Path::new("/home/user/linux/init")
                    || path == std::path::Path::new("/home/user/linux/ipc")
                    || path == std::path::Path::new("/home/user/linux/kernel")
                    || path == std::path::Path::new("/home/user/linux/lib")
                    || path == std::path::Path::new("/home/user/linux/scripts")
            })
            .times(12..=24)
            .returning(|_| true);
        fs.expect_is_file()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux/.config")
                    || path == std::path::Path::new("/home/user/linux/.kw/env.current")
                    || path == std::path::Path::new("/home/user/linux/COPYING")
                    || path == std::path::Path::new("/home/user/linux/CREDITS")
                    || path == std::path::Path::new("/home/user/linux/Kbuild")
                    || path == std::path::Path::new("/home/user/linux/Makefile")
                    || path == std::path::Path::new("/home/user/linux/README")
            })
            .times(7..=14)
            .returning(|path| !path.ends_with(".kw/env.current"));
        fs.expect_exists()
            .withf(|path| path == std::path::Path::new("/home/user/linux/MAINTAINERS"))
            .times(1..=2)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux/.kw/build.config")
                    || path
                        == std::path::Path::new("/home/user/linux/include/config/kernel.release")
                    || path.extension() == Some("log".as_ref())
            })
            .times(1..=3)
            .returning(|path| {
                read_real_job_log(path).unwrap_or_else(|| {
                    Err(FileSystemError::IoError(io::Error::new(
                        io::ErrorKind::NotFound,
                        "missing",
                    )))
                })
            });
        fs.expect_read_dir()
            .withf(|path| path == std::path::Path::new("/home/user/linux/arch"))
            .times(0..=1)
            .returning(|_| {
                Err(FileSystemError::IoError(io::Error::new(
                    io::ErrorKind::NotFound,
                    "missing",
                )))
            });
    }

    /// Job logs are real files ([`FakeProcess`] creates them and
    /// `write_log` appends to them), so fs doubles read `*.log` paths from
    /// disk.
    pub fn read_real_job_log(path: &Path) -> Option<Result<String, FileSystemError>> {
        (path.extension() == Some("log".as_ref()))
            .then(|| fs::read_to_string(path).map_err(FileSystemError::from))
    }

    /// A ready kernel tree whose log dir can be created.
    pub fn ready_fs() -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        expect_ready_tree(&mut fs);
        fs.expect_create_dir_all()
            .withf(|path| path.starts_with(std::env::temp_dir()))
            .times(1..=2)
            .returning(|_| Ok(()));
        fs
    }

    /// A ready kernel tree whose build produced an image and a
    /// kernelrelease: build.config sets `arch=x86`, `arch/x86/boot/`
    /// holds a bzImage, and `include/config/kernel.release` exists. The
    /// image's metadata is unreadable, so its mtime falls back to the
    /// epoch — still the only, hence newest, candidate.
    pub fn built_tree_fs() -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux")
                    || path == std::path::Path::new("/home/user/linux/.kw")
                    || path == std::path::Path::new("/home/user/linux/Documentation")
                    || path == std::path::Path::new("/home/user/linux/arch")
                    || path == std::path::Path::new("/home/user/linux/drivers")
                    || path == std::path::Path::new("/home/user/linux/fs")
                    || path == std::path::Path::new("/home/user/linux/include")
                    || path == std::path::Path::new("/home/user/linux/init")
                    || path == std::path::Path::new("/home/user/linux/ipc")
                    || path == std::path::Path::new("/home/user/linux/kernel")
                    || path == std::path::Path::new("/home/user/linux/lib")
                    || path == std::path::Path::new("/home/user/linux/scripts")
            })
            .times(12)
            .returning(|_| true);
        fs.expect_is_file()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux/.config")
                    || path == std::path::Path::new("/home/user/linux/.kw/env.current")
                    || path == std::path::Path::new("/home/user/linux/COPYING")
                    || path == std::path::Path::new("/home/user/linux/CREDITS")
                    || path == std::path::Path::new("/home/user/linux/Kbuild")
                    || path == std::path::Path::new("/home/user/linux/Makefile")
                    || path == std::path::Path::new("/home/user/linux/README")
                    || path == std::path::Path::new("/home/user/linux/arch/x86/boot/bzImage")
            })
            .times(7..=8)
            .returning(|path| !path.ends_with(".kw/env.current"));
        fs.expect_exists()
            .withf(|path| path == std::path::Path::new("/home/user/linux/MAINTAINERS"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux/.kw/build.config")
                    || path
                        == std::path::Path::new("/home/user/linux/include/config/kernel.release")
                    || path.extension() == Some("log".as_ref())
            })
            .times(2)
            .returning(|path| {
                if path.ends_with("build.config") {
                    Ok("arch=x86\n".to_string())
                } else if path.ends_with("kernel.release") {
                    Ok("6.17.0\n".to_string())
                } else {
                    Err(FileSystemError::IoError(io::Error::new(
                        io::ErrorKind::NotFound,
                        "missing",
                    )))
                }
            });
        fs.expect_read_dir()
            .withf(|path| path == std::path::Path::new("/home/user/linux/arch/x86/boot"))
            .times(0..=1)
            .returning(|path| {
                if path.ends_with("arch/x86/boot") {
                    Ok(vec![PathBuf::from(
                        "/home/user/linux/arch/x86/boot/bzImage",
                    )])
                } else {
                    Err(FileSystemError::IoError(io::Error::new(
                        io::ErrorKind::NotFound,
                        "missing",
                    )))
                }
            });
        fs.expect_metadata()
            .withf(|path| path == std::path::Path::new("/home/user/linux/arch/x86/boot/bzImage"))
            .times(0..=1)
            .returning(|_| Err(FileSystemError::IoError(io::Error::other("no metadata"))));
        fs.expect_create_dir_all()
            .withf(|path| path.starts_with(std::env::temp_dir()))
            .times(1)
            .returning(|_| Ok(()));
        fs
    }

    pub const DEPLOY_REMOTE_CONFIG: &str =
        "#kw-default=dut\nHost dut\n  Hostname box\n  Port 22\n  User root\n";
    pub const DEPLOY_BOOT_ONCE_OFF: &str = "boot_into_new_kernel_once=no\n";
    pub const DEPLOY_BOOT_ONCE_ON: &str = "boot_into_new_kernel_once=yes\n";

    pub fn deploy_ready_fs() -> MockFileSystemTrait {
        deploy_fs(DEPLOY_REMOTE_CONFIG, DEPLOY_BOOT_ONCE_OFF, true)
    }

    pub fn deploy_fs(
        remote_config: &'static str,
        deploy_config: &'static str,
        has_image: bool,
    ) -> MockFileSystemTrait {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_dir()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux")
                    || path == std::path::Path::new("/home/user/linux/.kw")
                    || path == std::path::Path::new("/home/user/linux/Documentation")
                    || path == std::path::Path::new("/home/user/linux/arch")
                    || path == std::path::Path::new("/home/user/linux/drivers")
                    || path == std::path::Path::new("/home/user/linux/fs")
                    || path == std::path::Path::new("/home/user/linux/include")
                    || path == std::path::Path::new("/home/user/linux/init")
                    || path == std::path::Path::new("/home/user/linux/ipc")
                    || path == std::path::Path::new("/home/user/linux/kernel")
                    || path == std::path::Path::new("/home/user/linux/lib")
                    || path == std::path::Path::new("/home/user/linux/scripts")
            })
            .times(12)
            .returning(|_| true);
        fs.expect_is_file()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux/.config")
                    || path == std::path::Path::new("/home/user/linux/.kw/deploy.config")
                    || path == std::path::Path::new("/home/user/linux/.kw/env.current")
                    || path == std::path::Path::new("/home/user/linux/.kw/remote.config")
                    || path == std::path::Path::new("/home/user/linux/COPYING")
                    || path == std::path::Path::new("/home/user/linux/CREDITS")
                    || path == std::path::Path::new("/home/user/linux/Kbuild")
                    || path == std::path::Path::new("/home/user/linux/Makefile")
                    || path == std::path::Path::new("/home/user/linux/README")
                    || path == std::path::Path::new("/home/user/linux/arch/x86/boot/bzImage")
            })
            .times(8..=10)
            .returning(|path| !path.ends_with(".kw/env.current"));
        fs.expect_exists()
            .withf(|path| path == std::path::Path::new("/home/user/linux/MAINTAINERS"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| {
                path == std::path::Path::new("/home/user/linux/.kw/build.config")
                    || path == std::path::Path::new("/home/user/linux/.kw/deploy.config")
                    || path == std::path::Path::new("/home/user/linux/.kw/remote.config")
                    || path
                        == std::path::Path::new("/home/user/linux/include/config/kernel.release")
                    || path.extension() == Some("log".as_ref())
            })
            .times(2..=5)
            .returning(move |path| {
                if let Some(log) = read_real_job_log(path) {
                    log
                } else if path.ends_with("build.config") {
                    Ok("arch=x86\n".to_string())
                } else if path.ends_with("kernel.release") {
                    Ok("6.17.0\n".to_string())
                } else if path.ends_with("remote.config") {
                    Ok(remote_config.to_string())
                } else if path.ends_with("deploy.config") {
                    Ok(deploy_config.to_string())
                } else {
                    Err(FileSystemError::IoError(io::Error::new(
                        io::ErrorKind::NotFound,
                        "missing",
                    )))
                }
            });
        fs.expect_read_dir()
            .withf(|path| path == std::path::Path::new("/home/user/linux/arch/x86/boot"))
            .times(0..=1)
            .returning(move |path| {
                if has_image && path.ends_with("arch/x86/boot") {
                    Ok(vec![PathBuf::from(
                        "/home/user/linux/arch/x86/boot/bzImage",
                    )])
                } else {
                    Err(FileSystemError::IoError(io::Error::new(
                        io::ErrorKind::NotFound,
                        "missing",
                    )))
                }
            });
        fs.expect_metadata()
            .withf(|path| path == std::path::Path::new("/home/user/linux/arch/x86/boot/bzImage"))
            .times(0..=1)
            .returning(|_| Err(FileSystemError::IoError(io::Error::other("no metadata"))));
        fs.expect_create_dir_all()
            .withf(|path| path.starts_with(std::env::temp_dir()))
            .times(0..=2)
            .returning(|_| Ok(()));
        fs
    }

    pub fn matching_build_record() -> KwBuildRecord {
        KwBuildRecord {
            kernel_tree_id: "mainline".to_string(),
            tree_path: "/home/user/linux".to_string(),
            message_id: None,
            branch: "patchset-2026-08-01-17-30-00".to_string(),
            arch: Some("x86".to_string()),
            image_path: Some("/home/user/linux/arch/x86/boot/bzImage".to_string()),
            output_dir: None,
            kernelrelease: Some("6.17.0".to_string()),
            log_path: String::new(),
            built_at: "2026-08-01T18:10:00Z".to_string(),
            success: true,
        }
    }

    pub fn deploy_options(acknowledged: bool) -> DeployOptions {
        DeployOptions {
            reboot: false,
            force: true,
            boot_once_acknowledged: acknowledged,
        }
    }

    pub fn deploy_request() -> StartRequest {
        let mut request = start_request();
        request.deploy = Some(deploy_options(false));
        request
    }

    /// History for a deploy-alone start: answers the record lookup for the
    /// requested branch and panics if a deploy writes a build record.
    pub fn deploy_history(record: Option<KwBuildRecord>) -> MockKwHistoryStore {
        let mut history = MockKwHistoryStore::new();
        history
            .expect_build_records()
            .withf(|kernel_tree_id, branch| {
                kernel_tree_id == "mainline" && branch == "patchset-2026-08-01-17-30-00"
            })
            .times(1)
            .returning(move |_, _| Ok((record.clone(), record.clone())));
        history.expect_record_build().withf(|_| true).times(0);
        history
    }

    pub fn env_with_kw() -> MockEnvTrait {
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(1)
            .returning(|_| true);
        env
    }

    pub fn spawn_deploy_actor(
        test_name: &str,
        history: MockKwHistoryStore,
        fs: MockFileSystemTrait,
    ) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        spawn_full_actor(test_name, history, shell, fs, env_with_kw())
    }

    /// History answers for an actor whose jobs complete: no patchset
    /// link, build-record writes accepted and dropped.
    pub fn quiet_history() -> MockKwHistoryStore {
        let mut history = MockKwHistoryStore::new();
        history
            .expect_apply_record_for_branch()
            .withf(|tree, branch| tree == "mainline" && branch == "patchset-2026-08-01-17-30-00")
            .times(1)
            .returning(|_, _| Ok(None));
        history
            .expect_record_build()
            .withf(|record| record.branch == "patchset-2026-08-01-17-30-00")
            .times(1)
            .returning(|_| Ok(()));
        history
    }

    /// A history double that captures written build records and answers
    /// the patchset-link lookup with `apply_record`.
    pub fn recording_history(
        apply_record: Option<KwApplyRecord>,
    ) -> (MockKwHistoryStore, Arc<Mutex<Vec<KwBuildRecord>>>) {
        let builds = Arc::new(Mutex::new(Vec::new()));
        let builds_in_store = Arc::clone(&builds);
        let mut history = MockKwHistoryStore::new();
        history
            .expect_apply_record_for_branch()
            .withf(|tree, branch| tree == "mainline" && branch == "patchset-2026-08-01-17-30-00")
            .times(1)
            .returning(move |_, _| Ok(apply_record.clone()));
        history
            .expect_record_build()
            .withf(|record| record.branch == "patchset-2026-08-01-17-30-00")
            .times(1)
            .returning(move |record| {
                builds_in_store
                    .lock()
                    .expect("builds in store locks")
                    .push(record);
                Ok(())
            });
        (history, builds)
    }

    /// Spawns the actor with every dependency explicit, a real temp log
    /// dir, and the [`FakeProcess`] exposed so tests drive the "running"
    /// process.
    pub fn spawn_full_actor(
        test_name: &str,
        history: MockKwHistoryStore,
        shell: MockShellTrait,
        fs: MockFileSystemTrait,
        env: MockEnvTrait,
    ) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let process = Arc::new(FakeProcess::new());
        let log_dir = tmp_log_dir(test_name);
        let handle = KwActor::spawn(
            Arc::new(history),
            process.clone(),
            Arc::new(shell),
            Arc::new(fs),
            Arc::new(env),
            log_dir.clone(),
        );
        (handle, process, log_dir)
    }

    /// Spawns the actor with a real temp log dir and exposes the
    /// [`FakeProcess`] so tests drive the "running" process. The env mock
    /// has kw on PATH.
    pub fn spawn_job_actor_with_mocks(
        test_name: &str,
        shell: MockShellTrait,
        fs: MockFileSystemTrait,
    ) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(1..=2)
            .returning(|_| true);
        spawn_full_actor(test_name, quiet_history(), shell, fs, env)
    }

    pub fn spawn_job_actor(test_name: &str) -> (KwHandle, Arc<FakeProcess>, PathBuf) {
        let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
        spawn_job_actor_with_mocks(test_name, shell, ready_fs())
    }

    pub fn apply_record() -> KwApplyRecord {
        KwApplyRecord {
            message_id: "msg-1".to_string(),
            kernel_tree_id: "mainline".to_string(),
            tree_path: "/home/user/linux".to_string(),
            applied_branch: "patchset-2026-08-01-17-30-00".to_string(),
            base_branch: "master".to_string(),
            applied_at: "2026-08-01T17:30:00Z".to_string(),
        }
    }

    /// Waits until the status leaves `Idle`/`Running` and returns the
    /// terminal status. The receiver may have observed the `Running`
    /// transition first, so a single `changed()` is not enough. The timeout
    /// backstops against a wedged actor; tests that exercise the grace
    /// periods run with paused time instead of waiting them out.
    pub async fn wait_for_terminal_status(
        watch: &mut watch::Receiver<KwStatusSnapshot>,
    ) -> KwJobStatus {
        time::timeout(Duration::from_secs(10), async {
            loop {
                let status = watch.borrow().job.clone();
                if !matches!(status, KwJobStatus::Idle | KwJobStatus::Running { .. }) {
                    return status;
                }
                watch.changed().await.expect("watch notifies");
            }
        })
        .await
        .expect("status must reach a terminal state")
    }

    pub async fn wait_for_running_phase(
        watch: &mut watch::Receiver<KwStatusSnapshot>,
        phase: KwPhase,
    ) -> KwJobStatus {
        time::timeout(Duration::from_secs(10), async {
            loop {
                let status = watch.borrow().job.clone();
                if matches!(
                    status,
                    KwJobStatus::Running { phase: running, .. } if running == phase
                ) {
                    return status;
                }
                watch.changed().await.expect("watch notifies");
            }
        })
        .await
        .expect("status must reach the expected running phase")
    }
}
pub use helpers::*;

#[tokio::test]
async fn record_apply_writes_through_history_store() {
    let expected = apply_record();
    let mut history = MockKwHistoryStore::new();
    history
        .expect_record_apply()
        .withf(move |record| *record == expected)
        .times(1)
        .returning(|_| Ok(()));
    let handle = spawn_test_actor(
        "record-apply",
        history,
        MockShellTrait::new(),
        MockFileSystemTrait::new(),
        MockEnvTrait::new(),
    );

    handle
        .record_apply(apply_record())
        .await
        .expect("apply records");
    handle.shutdown().await;
}

#[tokio::test]
async fn record_apply_surfaces_store_errors() {
    let mut history = MockKwHistoryStore::new();
    history
        .expect_record_apply()
        .withf(|record| {
            record.message_id == "msg-1"
                && record.kernel_tree_id == "mainline"
                && record.tree_path == "/home/user/linux"
                && record.applied_branch == "patchset-2026-08-01-17-30-00"
                && record.base_branch == "master"
                && record.applied_at == "2026-08-01T17:30:00Z"
        })
        .times(1)
        .returning(|_| Err(FileSystemError::IoError(io::Error::other("disk full"))));
    let handle = spawn_test_actor(
        "record-apply-error",
        history,
        MockShellTrait::new(),
        MockFileSystemTrait::new(),
        MockEnvTrait::new(),
    );

    let err = handle
        .record_apply(apply_record())
        .await
        .expect_err("apply record fails");

    assert!(matches!(err, KwError::History(_)));
    handle.shutdown().await;
}

#[tokio::test]
async fn get_status_reports_idle_before_any_job() {
    let handle = spawn_test_actor(
        "idle-status",
        MockKwHistoryStore::new(),
        MockShellTrait::new(),
        MockFileSystemTrait::new(),
        MockEnvTrait::new(),
    );

    let snapshot = handle.get_status().await.expect("status loads");

    assert_eq!(KwJobStatus::Idle, snapshot.job);
    assert_eq!(None, snapshot.restore_branch);
    handle.shutdown().await;
}

#[tokio::test]
async fn watch_status_receiver_sees_current_snapshot() {
    let handle = spawn_test_actor(
        "watch-idle",
        MockKwHistoryStore::new(),
        MockShellTrait::new(),
        MockFileSystemTrait::new(),
        MockEnvTrait::new(),
    );

    let receiver = handle.watch_status().await.expect("status watch opens");

    assert_eq!(KwJobStatus::Idle, receiver.borrow().job);
    handle.shutdown().await;
}

#[tokio::test]
async fn get_readiness_composes_probes_and_head_branch() {
    let tree = kernel_tree(Path::new("/home/user/linux"));

    let mut env = MockEnvTrait::new();
    env.expect_which()
        .withf(|name| name == "kw")
        .times(1)
        .returning(|_| false);
    env.expect_var()
        .withf(|key| matches!(key, "HOME" | "XDG_CONFIG_HOME"))
        .times(4)
        .returning(|_| Err(env::VarError::NotPresent.into()));
    let mut fs = MockFileSystemTrait::new();
    fs.expect_is_file()
        .withf(|path| {
            path == std::path::Path::new("/home/user/linux/.kw/deploy.config")
                || path == std::path::Path::new("/home/user/linux/.kw/env.current")
                || path == std::path::Path::new("/home/user/linux/.kw/remote.config")
        })
        .times(3)
        .returning(|_| false);
    fs.expect_is_dir()
        .withf(|path| path == std::path::Path::new("/home/user/linux"))
        .times(1)
        .returning(|_| false);
    fs.expect_read_dir()
        .withf(|path| path == std::path::Path::new("/home/user/linux/arch"))
        .times(1)
        .returning(|_| {
            Err(FileSystemError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                "missing",
            )))
        });
    let mut history = MockKwHistoryStore::new();
    history
        .expect_build_records()
        .withf(|kernel_tree_id, branch| kernel_tree_id == "mainline" && branch == "for-next")
        .times(1)
        .returning(|_, _| Ok((None, None)));
    let mut shell = MockShellTrait::new();
    shell
        .expect_execute()
        .withf(|cmd| {
            cmd.program == "git"
                && cmd.args == ["-C", "/home/user/linux", "branch", "--show-current"]
        })
        .times(1)
        .returning(|_| {
            Ok(ShellOutput {
                stdout: b"for-next\n".to_vec(),
                stderr: Vec::new(),
                success: true,
            })
        });
    let handle = spawn_test_actor("readiness", history, shell, fs, env);

    let readiness = handle
        .get_readiness("mainline", &tree, None)
        .await
        .expect("readiness loads");

    assert!(!readiness.kw_binary.available);
    assert_eq!(TreeReadiness::Missing, readiness.tree);
    assert_eq!(
        Err(DeployAloneRefusal::TreeNotReady(TreeReadiness::Missing)),
        readiness.deploy_alone
    );
    assert_eq!(Some("for-next".to_string()), readiness.current_branch);
    assert_eq!(
        Err(RemoteRefusal::NoRemotesConfigured),
        readiness.deploy_remote
    );
    assert_eq!(BootOnceState::Unknown, readiness.boot_once);
    handle.shutdown().await;
}

#[tokio::test]
async fn get_readiness_for_branch_looks_up_that_branch_not_head() {
    let tree = kernel_tree(Path::new("/home/user/linux"));

    let mut env = MockEnvTrait::new();
    env.expect_which()
        .withf(|name| name == "kw")
        .times(1)
        .returning(|_| false);
    env.expect_var()
        .withf(|key| matches!(key, "HOME" | "XDG_CONFIG_HOME"))
        .times(4)
        .returning(|_| Err(env::VarError::NotPresent.into()));
    let mut fs = MockFileSystemTrait::new();
    fs.expect_is_file()
        .withf(|path| {
            path == std::path::Path::new("/home/user/linux/.kw/deploy.config")
                || path == std::path::Path::new("/home/user/linux/.kw/env.current")
                || path == std::path::Path::new("/home/user/linux/.kw/remote.config")
        })
        .times(3)
        .returning(|_| false);
    fs.expect_is_dir()
        .withf(|path| path == std::path::Path::new("/home/user/linux"))
        .times(1)
        .returning(|_| false);
    fs.expect_read_dir()
        .withf(|path| path == std::path::Path::new("/home/user/linux/arch"))
        .times(1)
        .returning(|_| {
            Err(FileSystemError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                "missing",
            )))
        });
    let mut history = MockKwHistoryStore::new();
    history
        .expect_build_records()
        .withf(|kernel_tree_id, branch| kernel_tree_id == "mainline" && branch == "patchset-x")
        .times(1)
        .returning(|_, _| Ok((None, None)));
    let mut shell = MockShellTrait::new();
    shell
        .expect_execute()
        .withf(|cmd| {
            cmd.program == "git"
                && cmd.args == ["-C", "/home/user/linux", "branch", "--show-current"]
        })
        .times(1)
        .returning(|_| {
            Ok(ShellOutput {
                stdout: b"master\n".to_vec(),
                stderr: Vec::new(),
                success: true,
            })
        });
    let handle = spawn_test_actor("readiness-for-branch", history, shell, fs, env);

    let readiness = handle
        .get_readiness("mainline", &tree, Some("patchset-x"))
        .await
        .expect("readiness loads");

    assert_eq!(Some("master".to_string()), readiness.current_branch);
    handle.shutdown().await;
}

#[tokio::test]
async fn start_build_replies_immediately_and_runs_in_background() {
    let (handle, process, log_dir) = spawn_job_actor("start-immediate");

    // start_build resolves while the spawned process is still running
    // (no finish() was ever signaled).
    let result = time::timeout(Duration::from_secs(1), handle.start_build(start_request()))
        .await
        .expect("start_build must reply immediately");
    result.expect("build starts");

    let spawned = process.spawned();
    assert_eq!(1, spawned.len());
    assert_eq!("kw", spawned[0].program);
    assert_eq!(["build"], spawned[0].args.as_slice());
    assert_eq!(Path::new("/home/user/linux"), spawned[0].cwd);
    assert!(spawned[0].log_path.starts_with(&log_dir));

    let snapshot = handle.get_status().await.expect("status loads");
    assert!(
        matches!(
            snapshot.job,
            KwJobStatus::Running {
                kind: KwJobKind::Build,
                phase: KwPhase::Building,
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
                kind: KwJobKind::Build,
                ..
            }
        ),
        "unexpected status: {status:?}"
    );

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_merges_extra_args_into_the_spawned_argv() {
    let (handle, process, log_dir) = spawn_job_actor("extra-args");

    let mut request = start_request();
    request.extra_args = [
        "--verbose",
        "--alert=vv",
        "--save-log-to",
        "/tmp/x.log",
        "--ccache",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    handle.start_build(request).await.expect("build starts");

    let spawned = process.spawned();
    // Reserved options are stripped: the user's --alert and --save-log-to
    // do not reach kw; the rest passes through in order.
    assert_eq!(
        ["build", "--verbose", "--ccache"].as_slice(),
        spawned[0].args.as_slice()
    );

    process.last_child().finish(0);
    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn second_start_while_running_is_refused() {
    let (handle, process, log_dir) = spawn_job_actor("busy");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    let second = handle.start_build(start_request()).await;

    assert!(matches!(second, Err(KwStartError::JobAlreadyRunning)));

    process.last_child().finish(0);
    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn failed_build_reports_exit_code_and_log_path() {
    let (handle, process, log_dir) = spawn_job_actor("failed");
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().write_log(
        b"  CC      init/main.o\n\
          init/main.c:1691:2: error: #error broken\n\
          make: *** [Makefile:248: __sub-make] Error 2\n",
    );
    process.last_child().finish(2);

    let status = wait_for_terminal_status(&mut watch).await;

    match status {
        KwJobStatus::Failed {
            kind,
            phase,
            exit_code,
            log_path,
            first_error,
        } => {
            assert_eq!(KwJobKind::Build, kind);
            assert_eq!(KwPhase::Building, phase);
            assert_eq!(Some(2), exit_code);
            assert!(log_path.starts_with(&log_dir));
            assert_eq!(
                Some("init/main.c:1691:2: error: #error broken"),
                first_error.as_deref()
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn cancel_kills_process_group_and_reports_cancelled() {
    let (handle, process, log_dir) = spawn_job_actor("cancel");
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    // The ack is immediate: process death is observed via the status,
    // not the reply.
    time::timeout(Duration::from_secs(1), handle.cancel())
        .await
        .expect("cancel must ack immediately")
        .expect("job cancels");

    assert!(process.last_child().was_killed());
    let status = wait_for_terminal_status(&mut watch).await;
    match status {
        KwJobStatus::Cancelled {
            kind,
            phase,
            log_path,
        } => {
            assert_eq!(KwJobKind::Build, kind);
            assert_eq!(KwPhase::Building, phase);
            assert!(log_path.starts_with(&log_dir));
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn shutdown_kills_running_job() {
    let (handle, process, log_dir) = spawn_job_actor("shutdown-kill");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    // shutdown() returns only after the kill escalation has completed.
    handle.shutdown().await;

    assert!(process.last_child().was_killed());
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn spawn_failure_refuses_start_and_stays_idle() {
    let (handle, process, log_dir) = spawn_job_actor("spawn-fail");
    process.refuse_spawns(true);

    let err = handle
        .start_build(start_request())
        .await
        .expect_err("spawn fails");

    assert!(matches!(err, KwStartError::Spawn(_)));
    assert_eq!(
        KwJobStatus::Idle,
        handle.get_status().await.expect("status loads").job
    );
    // A refused start must leave the actor able to accept a later one.
    process.refuse_spawns(false);
    handle
        .start_build(start_request())
        .await
        .expect("build starts");

    process.last_child().finish(0);
    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_refused_when_kw_binary_missing() {
    let process = Arc::new(FakeProcess::new());
    let mut env = MockEnvTrait::new();
    env.expect_which()
        .withf(|name| name == "kw")
        .times(1)
        .returning(|_| false);
    let log_dir = tmp_log_dir("kw-missing");
    let handle = KwActor::spawn(
        Arc::new(MockKwHistoryStore::new()),
        process.clone(),
        // No shell or fs calls are expected: the missing binary
        // short-circuits the start before any other probe or spawn.
        Arc::new(MockShellTrait::new()),
        Arc::new(MockFileSystemTrait::new()),
        Arc::new(env),
        log_dir.clone(),
    );

    let err = handle
        .start_build(start_request())
        .await
        .expect_err("missing kw refuses build");

    assert!(matches!(err, KwStartError::KwBinaryMissing));
    assert_eq!(
        KwJobStatus::Idle,
        handle.get_status().await.expect("status loads").job
    );
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_refused_when_tree_not_ready() {
    let process = Arc::new(FakeProcess::new());
    let mut env = MockEnvTrait::new();
    env.expect_which()
        .withf(|name| name == "kw")
        .times(1)
        .returning(|_| true);
    let mut shell = MockShellTrait::new();
    shell
        .expect_execute()
        .withf(|cmd| cmd.program == "kw" && cmd.args == ["--version"])
        .times(1)
        .returning(|_| {
            Ok(ShellOutput {
                stdout: b"kw, version 0.10.0\n".to_vec(),
                stderr: Vec::new(),
                success: true,
            })
        });
    // The kernel-root probes pass, but there is no .kw directory: kw
    // init was never run in this tree.
    let mut fs = MockFileSystemTrait::new();
    fs.expect_is_dir()
        .withf(|path| {
            path == std::path::Path::new("/home/user/linux")
                || path == std::path::Path::new("/home/user/linux/.kw")
                || path == std::path::Path::new("/home/user/linux/Documentation")
                || path == std::path::Path::new("/home/user/linux/arch")
                || path == std::path::Path::new("/home/user/linux/drivers")
                || path == std::path::Path::new("/home/user/linux/fs")
                || path == std::path::Path::new("/home/user/linux/include")
                || path == std::path::Path::new("/home/user/linux/init")
                || path == std::path::Path::new("/home/user/linux/ipc")
                || path == std::path::Path::new("/home/user/linux/kernel")
                || path == std::path::Path::new("/home/user/linux/lib")
                || path == std::path::Path::new("/home/user/linux/scripts")
        })
        .times(12)
        .returning(|path| path.file_name().is_none_or(|name| name != ".kw"));
    fs.expect_is_file()
        .withf(|path| {
            path == std::path::Path::new("/home/user/linux/.kw/env.current")
                || path == std::path::Path::new("/home/user/linux/COPYING")
                || path == std::path::Path::new("/home/user/linux/CREDITS")
                || path == std::path::Path::new("/home/user/linux/Kbuild")
                || path == std::path::Path::new("/home/user/linux/Makefile")
                || path == std::path::Path::new("/home/user/linux/README")
        })
        .times(6)
        .returning(|path| !path.ends_with(".kw/env.current"));
    fs.expect_exists()
        .withf(|path| path == std::path::Path::new("/home/user/linux/MAINTAINERS"))
        .times(1)
        .returning(|_| true);
    let log_dir = tmp_log_dir("tree-not-ready");
    let handle = KwActor::spawn(
        Arc::new(MockKwHistoryStore::new()),
        process.clone(),
        Arc::new(shell),
        Arc::new(fs),
        Arc::new(env),
        log_dir.clone(),
    );

    let err = handle
        .start_build(start_request())
        .await
        .expect_err("unreadiness refuses build");

    assert!(matches!(
        err,
        KwStartError::TreeNotReady(TreeReadiness::MissingKwDir)
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
async fn start_build_allowed_when_kw_version_below_floor() {
    // kw's shipped VERSION file is stale (`beta-0.9` even at the 0.10
    // tag): a below-floor report warns but never gates the start.
    let (shell, _calls) = recording_shell(b"kw, version beta-0.9\n", CLEAN_STATUS, SWITCH_OK);
    let (handle, process, log_dir) = spawn_job_actor_with_mocks("version-below", shell, ready_fs());

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    assert_eq!(1, process.spawned().len());

    process.last_child().finish(0);
    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_switches_to_requested_branch_before_spawning() {
    let (shell, calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
    let (handle, process, log_dir) =
        spawn_job_actor_with_mocks("checkout-order", shell, ready_fs());

    handle
        .start_build(start_request())
        .await
        .expect("build starts");

    {
        let calls = calls.lock().expect("calls locks");
        let git_position = |subcommand: &str| {
            calls
                .iter()
                .position(|call| {
                    call.first().map(String::as_str) == Some("git")
                        && call.iter().any(|part| part == subcommand)
                })
                .expect("expected git call missing")
        };
        let status = git_position("status");
        let head = git_position("--show-current");
        let switch = git_position("switch");
        assert!(
            status < head && head < switch,
            "checkout policy must probe dirty state, then HEAD, then switch: {calls:?}"
        );
        // Untracked scratch files don't block a build; only tracked
        // changes do.
        assert_eq!(
            &calls[status],
            &command(&[
                "git",
                "-C",
                "/home/user/linux",
                "status",
                "--porcelain",
                "--untracked-files=no"
            ])
        );
        // The `--` keeps a branch named like a flag from being parsed
        // as one.
        assert_eq!(
            &calls[switch],
            &command(&[
                "git",
                "-C",
                "/home/user/linux",
                "switch",
                "--",
                "patchset-2026-08-01-17-30-00"
            ])
        );
    }
    assert_eq!(1, process.spawned().len());

    process.last_child().finish(0);
    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_refused_when_worktree_is_dirty() {
    let (shell, calls) = recording_shell(KW_VERSION_OK, (b" M src/main.c\n", b"", true), SWITCH_OK);
    let mut fs = MockFileSystemTrait::new();
    expect_ready_tree(&mut fs);
    let (handle, process, log_dir) = spawn_job_actor_with_mocks("dirty", shell, fs);

    let err = handle
        .start_build(start_request())
        .await
        .expect_err("dirty worktree refuses build");

    assert!(matches!(err, KwStartError::DirtyWorktree));
    assert_eq!(
        KwJobStatus::Idle,
        handle.get_status().await.expect("status loads").job
    );
    assert!(process.spawned().is_empty());
    // The refusal happens before any branch mutation.
    assert!(!calls
        .lock()
        .expect("calls locks")
        .iter()
        .any(|call| call.iter().any(|part| part == "switch")));

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_refused_when_git_state_is_unverifiable() {
    let (shell, _calls) = recording_shell(
        KW_VERSION_OK,
        (b"", b"fatal: not a git repository\n", false),
        SWITCH_OK,
    );
    let mut fs = MockFileSystemTrait::new();
    expect_ready_tree(&mut fs);
    let (handle, process, log_dir) = spawn_job_actor_with_mocks("git-probe-fail", shell, fs);

    let err = handle
        .start_build(start_request())
        .await
        .expect_err("unverifiable git refuses build");

    assert!(matches!(err, KwStartError::GitStateProbe(_)));
    assert!(err.to_string().contains("not a git repository"));
    assert_eq!(
        KwJobStatus::Idle,
        handle.get_status().await.expect("status loads").job
    );
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn start_build_refused_when_branch_switch_fails() {
    let (shell, _calls) = recording_shell(
        KW_VERSION_OK,
        CLEAN_STATUS,
        (
            b"error: pathspec 'no-such-branch' did not match any file(s) known to git\n",
            false,
        ),
    );
    let (handle, process, log_dir) = spawn_job_actor_with_mocks("switch-fail", shell, ready_fs());

    let err = handle
        .start_build(start_request())
        .await
        .expect_err("branch switch refuses build");

    assert!(matches!(err, KwStartError::CheckoutFailed(_)));
    assert!(err.to_string().contains("did not match"));
    assert_eq!(
        KwJobStatus::Idle,
        handle.get_status().await.expect("status loads").job
    );
    assert!(process.spawned().is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn cancel_and_restore_without_job_are_immediate_errors() {
    let handle = spawn_test_actor(
        "no-job",
        MockKwHistoryStore::new(),
        MockShellTrait::new(),
        MockFileSystemTrait::new(),
        MockEnvTrait::new(),
    );

    let cancel = time::timeout(Duration::from_secs(1), handle.cancel())
        .await
        .expect("cancel must reply immediately");
    let restore = time::timeout(Duration::from_secs(1), handle.restore_previous_branch())
        .await
        .expect("restore must reply immediately");

    assert!(matches!(cancel, Err(KwError::NoJobRunning)));
    assert!(matches!(restore, Err(KwError::NoRecordedBranch)));
    handle.shutdown().await;
}

#[tokio::test]
async fn restore_switches_back_to_pre_job_branch_and_is_consumed() {
    let git = GitStub::on_branch("master");
    let (handle, process, log_dir) = spawn_job_actor_with_mocks("restore", git.shell(), ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    // The checkout policy left HEAD on the build branch.
    assert_eq!(git.head(), "patchset-2026-08-01-17-30-00");
    assert_eq!(
        Some("master"),
        handle
            .get_status()
            .await
            .expect("status loads")
            .restore_branch
            .as_deref()
    );
    process.last_child().finish(0);
    let status = wait_for_terminal_status(&mut watch).await;
    assert!(matches!(status, KwJobStatus::Succeeded { .. }));
    assert_eq!(
        Some("master"),
        handle
            .get_status()
            .await
            .expect("status loads")
            .restore_branch
            .as_deref()
    );

    handle
        .restore_previous_branch()
        .await
        .expect("previous branch restores");
    assert_eq!(git.head(), "master");
    assert_eq!(
        None,
        handle
            .get_status()
            .await
            .expect("status loads")
            .restore_branch
    );

    // A successful restore consumes the context: a second restore has
    // nothing to do.
    let err = handle
        .restore_previous_branch()
        .await
        .expect_err("second restore has no branch");
    assert!(matches!(err, KwError::NoRecordedBranch));

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn restore_refused_while_job_is_running() {
    let git = GitStub::on_branch("master");
    let (handle, process, log_dir) =
        spawn_job_actor_with_mocks("restore-running", git.shell(), ready_fs());

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    let err = handle
        .restore_previous_branch()
        .await
        .expect_err("restore refused while running");
    assert!(matches!(err, KwError::JobRunning));

    // The context survives the refusal: restore works once the job
    // ends.
    process.last_child().finish(0);
    let mut watch = handle.watch_status().await.expect("status watch opens");
    let _ = wait_for_terminal_status(&mut watch).await;
    handle
        .restore_previous_branch()
        .await
        .expect("previous branch restores");
    assert_eq!(git.head(), "master");

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn restore_refused_when_worktree_is_dirty() {
    let git = GitStub::on_branch("master");
    let (handle, process, log_dir) =
        spawn_job_actor_with_mocks("restore-dirty", git.shell(), ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    let _ = wait_for_terminal_status(&mut watch).await;

    git.set_dirty(true);
    let err = handle
        .restore_previous_branch()
        .await
        .expect_err("dirty worktree refuses restore");
    assert!(matches!(err, KwError::DirtyWorktree));
    // The refused restore did not touch the tree.
    assert_eq!(git.head(), "patchset-2026-08-01-17-30-00");
    assert_eq!(
        Some("master"),
        handle
            .get_status()
            .await
            .expect("status loads")
            .restore_branch
            .as_deref()
    );

    // The context survives: clean the tree and retry.
    git.set_dirty(false);
    handle
        .restore_previous_branch()
        .await
        .expect("previous branch restores");
    assert_eq!(git.head(), "master");

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn restore_failure_keeps_the_context_for_a_retry() {
    let git = GitStub::on_branch("master");
    let (handle, process, log_dir) =
        spawn_job_actor_with_mocks("restore-fail", git.shell(), ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    let _ = wait_for_terminal_status(&mut watch).await;

    git.fail_switches_to(Some("master"));
    let err = handle
        .restore_previous_branch()
        .await
        .expect_err("restore fails");
    assert!(matches!(err, KwError::CheckoutFailed(_)));
    assert!(err.to_string().contains("resolve your current index"));

    git.fail_switches_to(None);
    handle
        .restore_previous_branch()
        .await
        .expect("previous branch restores");
    assert_eq!(git.head(), "master");

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn refused_start_does_not_clobber_the_restore_context() {
    let git = GitStub::on_branch("master");
    let (handle, process, log_dir) =
        spawn_job_actor_with_mocks("restore-clobber", git.shell(), ready_fs());
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    let _ = wait_for_terminal_status(&mut watch).await;
    assert_eq!(git.head(), "patchset-2026-08-01-17-30-00");

    // This start is refused at spawn — after its HEAD probe and
    // switch — and the switch is rolled back: the tree returns to the
    // first job's branch and the recorded restore target is intact.
    process.refuse_spawns(true);
    let mut second = start_request();
    second.branch = "patchset-two".to_string();
    let err = handle
        .start_build(second)
        .await
        .expect_err("refused start stays refused");
    assert!(matches!(err, KwStartError::Spawn(_)));
    assert_eq!(git.head(), "patchset-2026-08-01-17-30-00");

    handle
        .restore_previous_branch()
        .await
        .expect("previous branch restores");
    assert_eq!(git.head(), "master");

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn shutdown_stops_actor() {
    let handle = spawn_test_actor(
        "shutdown",
        MockKwHistoryStore::new(),
        MockShellTrait::new(),
        MockFileSystemTrait::new(),
        MockEnvTrait::new(),
    );

    handle.shutdown().await;
    let err = handle.get_status().await.expect_err("actor has stopped");

    assert!(matches!(err, KwError::ActorUnavailable(_)));
}

#[tokio::test]
async fn log_dir_creation_failure_refuses_start_and_stays_idle() {
    let git = GitStub::on_branch("master");
    let mut fs = MockFileSystemTrait::new();
    expect_ready_tree(&mut fs);
    fs.expect_create_dir_all()
        .withf(|path| path.starts_with(std::env::temp_dir()))
        .times(1)
        .returning(|_| {
            Err(FileSystemError::IoError(io::Error::other(
                "read-only filesystem",
            )))
        });
    let (handle, process, log_dir) = spawn_job_actor_with_mocks("log-dir-fail", git.shell(), fs);

    let err = handle
        .start_build(start_request())
        .await
        .expect_err("log dir failure refuses start");

    assert!(matches!(err, KwStartError::Fs(_)));
    assert_eq!(
        KwJobStatus::Idle,
        handle.get_status().await.expect("status loads").job
    );
    assert!(process.spawned().is_empty());
    // The switch happened and was rolled back: the tree is back on the
    // user's branch, and no restore target was recorded.
    assert_eq!(git.head(), "master");
    assert!(matches!(
        handle.restore_previous_branch().await,
        Err(KwError::NoRecordedBranch)
    ));

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn rollback_failure_keeps_the_spawn_refusal() {
    let git = GitStub::on_branch("master");
    // Going forward works; coming back fails.
    git.fail_switches_to(Some("master"));
    let (handle, process, log_dir) =
        spawn_job_actor_with_mocks("rollback-fails", git.shell(), ready_fs());
    process.refuse_spawns(true);

    let err = handle
        .start_build(start_request())
        .await
        .expect_err("spawn stays refused");

    assert!(matches!(err, KwStartError::Spawn(_)));
    // The rollback failure is logged, not reported: the caller keeps
    // the actionable refusal, and the tree honestly shows where HEAD
    // is. No restore target was recorded.
    assert_eq!(git.head(), "patchset-2026-08-01-17-30-00");
    assert!(matches!(
        handle.restore_previous_branch().await,
        Err(KwError::NoRecordedBranch)
    ));

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn cancel_racing_a_successful_exit_reports_success() {
    let (handle, process, log_dir) = spawn_job_actor("cancel-race");
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    // Whether the actor processes the exit or the cancel first is
    // timing-dependent, but the terminal status must be Succeeded
    // either way: the process exited before any signal landed.
    let _ = handle.cancel().await;

    let status = wait_for_terminal_status(&mut watch).await;
    assert!(
        matches!(status, KwJobStatus::Succeeded { .. }),
        "expected Succeeded, got {status:?}"
    );
    // Killing an already-finished process is a no-op, not a kill.
    assert!(!process.last_child().was_killed());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn cancel_racing_a_failed_exit_reports_failure() {
    let (handle, process, log_dir) = spawn_job_actor("cancel-race-fail");
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(2);
    // Whether the actor processes the exit or the cancel first is
    // timing-dependent, but a plain exit (not signal-terminated) means
    // the process failed on its own before the signal landed: the
    // terminal status must be Failed either way, so the build history
    // records a failure rather than a cancel.
    let _ = handle.cancel().await;

    let status = wait_for_terminal_status(&mut watch).await;
    assert!(
        matches!(
            status,
            KwJobStatus::Failed {
                exit_code: Some(2),
                ..
            }
        ),
        "expected Failed(2), got {status:?}"
    );

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

// Paused time: the runtime auto-advances through the grace-period
// timers instead of burning wall-clock seconds on them.
#[tokio::test(start_paused = true)]
async fn cancel_escalates_to_sigkill_when_sigterm_is_ignored() {
    let (handle, process, log_dir) = spawn_job_actor("sigkill-escalation");
    process.ignore_sigterm(true);
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    handle.cancel().await.expect("job cancels");

    let status = wait_for_terminal_status(&mut watch).await;
    assert!(
        matches!(status, KwJobStatus::Cancelled { .. }),
        "expected Cancelled, got {status:?}"
    );
    let child = process.last_child();
    assert!(child.was_killed());
    assert!(child.was_force_killed());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn successful_build_writes_a_full_build_record() {
    let (history, builds) = recording_history(Some(apply_record()));
    let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
    let (handle, process, log_dir) =
        spawn_full_actor("build-record", history, shell, built_tree_fs(), {
            let mut env = MockEnvTrait::new();
            env.expect_which()
                .withf(|name| name == "kw")
                .times(1)
                .returning(|_| true);
            env
        });
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    let status = wait_for_terminal_status(&mut watch).await;
    assert!(matches!(status, KwJobStatus::Succeeded { .. }));

    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        let record = &builds[0];
        assert_eq!("mainline", record.kernel_tree_id);
        assert_eq!("/home/user/linux", record.tree_path);
        assert_eq!(Some("msg-1"), record.message_id.as_deref());
        assert_eq!("patchset-2026-08-01-17-30-00", record.branch);
        assert_eq!(Some("x86"), record.arch.as_deref());
        assert_eq!(
            Some("/home/user/linux/arch/x86/boot/bzImage"),
            record.image_path.as_deref()
        );
        assert_eq!(None, record.output_dir);
        assert_eq!(Some("6.17.0"), record.kernelrelease.as_deref());
        assert!(record
            .log_path
            .starts_with(log_dir.to_str().expect("path is utf-8")));
        assert!(record.success);
        // The readiness latest-lookup parses built_at as RFC3339.
        assert!(DateTime::parse_from_rfc3339(&record.built_at).is_ok());
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn failed_build_writes_a_failure_record() {
    let (history, builds) = recording_history(Some(apply_record()));
    let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
    let (handle, process, log_dir) =
        spawn_full_actor("failed-record", history, shell, built_tree_fs(), {
            let mut env = MockEnvTrait::new();
            env.expect_which()
                .withf(|name| name == "kw")
                .times(1)
                .returning(|_| true);
            env
        });
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(2);
    let status = wait_for_terminal_status(&mut watch).await;
    assert!(matches!(
        status,
        KwJobStatus::Failed {
            exit_code: Some(2),
            ..
        }
    ));

    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        let record = &builds[0];
        assert!(!record.success);
        // Config/env facts are still recorded; what the build never
        // produced is not — a stale image from an earlier build must
        // not leak into a failure record.
        assert_eq!(Some("x86"), record.arch.as_deref());
        assert_eq!(None, record.image_path);
        assert_eq!(None, record.kernelrelease);
        assert_eq!(Some("msg-1"), record.message_id.as_deref());
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn cancelled_build_writes_no_record() {
    let (history, builds) = recording_history(None);
    let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
    let (handle, _process, log_dir) =
        spawn_full_actor("cancel-record", history, shell, ready_fs(), {
            let mut env = MockEnvTrait::new();
            env.expect_which()
                .withf(|name| name == "kw")
                .times(1)
                .returning(|_| true);
            env
        });
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    handle.cancel().await.expect("job cancels");
    let status = wait_for_terminal_status(&mut watch).await;

    assert!(matches!(status, KwJobStatus::Cancelled { .. }));
    assert!(builds.lock().expect("builds locks").is_empty());

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn lost_exit_status_records_a_failed_build() {
    let (history, builds) = recording_history(None);
    let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
    let (handle, process, log_dir) =
        spawn_full_actor("wait-failure", history, shell, ready_fs(), {
            let mut env = MockEnvTrait::new();
            env.expect_which()
                .withf(|name| name == "kw")
                .times(1)
                .returning(|_| true);
            env
        });
    let mut watch = handle.watch_status().await.expect("status watch opens");
    process.fail_waits(true);

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    let status = wait_for_terminal_status(&mut watch).await;

    // A lost exit status is an honest failure: the record must not
    // become deploy-alone evidence.
    assert!(matches!(
        status,
        KwJobStatus::Failed {
            exit_code: None,
            ..
        }
    ));
    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        assert!(!builds[0].success);
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn build_record_write_failure_keeps_the_terminal_status() {
    let mut history = MockKwHistoryStore::new();
    history
        .expect_apply_record_for_branch()
        .withf(|tree, branch| tree == "mainline" && branch == "patchset-2026-08-01-17-30-00")
        .times(1)
        .returning(|_, _| Ok(None));
    history
        .expect_record_build()
        .withf(|record| record.branch == "patchset-2026-08-01-17-30-00")
        .times(1)
        .returning(|_| Err(FileSystemError::IoError(io::Error::other("disk full"))));
    let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
    let (handle, process, log_dir) =
        spawn_full_actor("record-write-fails", history, shell, ready_fs(), {
            let mut env = MockEnvTrait::new();
            env.expect_which()
                .withf(|name| name == "kw")
                .times(1)
                .returning(|_| true);
            env
        });
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    let status = wait_for_terminal_status(&mut watch).await;

    // The build's real outcome reached the user; a history-write
    // failure must not turn it into a reported failure.
    assert!(matches!(status, KwJobStatus::Succeeded { .. }));

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn build_record_keeps_no_patchset_link_when_apply_lookup_fails() {
    let builds = Arc::new(Mutex::new(Vec::new()));
    let builds_in_store = Arc::clone(&builds);
    let mut history = MockKwHistoryStore::new();
    history
        .expect_apply_record_for_branch()
        .withf(|tree, branch| tree == "mainline" && branch == "patchset-2026-08-01-17-30-00")
        .times(1)
        .returning(|_, _| {
            Err(FileSystemError::IoError(io::Error::other(
                "corrupt history",
            )))
        });
    history
        .expect_record_build()
        .withf(|record| record.branch == "patchset-2026-08-01-17-30-00")
        .times(1)
        .returning(move |record| {
            builds_in_store
                .lock()
                .expect("builds in store locks")
                .push(record);
            Ok(())
        });
    let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
    let (handle, process, log_dir) =
        spawn_full_actor("link-lookup-fails", history, shell, ready_fs(), {
            let mut env = MockEnvTrait::new();
            env.expect_which()
                .withf(|name| name == "kw")
                .times(1)
                .returning(|_| true);
            env
        });
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    let _ = wait_for_terminal_status(&mut watch).await;

    // The lookup error must not drop the record, only the link.
    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        assert_eq!(None, builds[0].message_id);
        assert!(builds[0].success);
    }

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}

#[tokio::test]
async fn successful_build_with_active_env_records_the_output_dir() {
    // env.current is read once, at accept time. The completion record
    // must describe the env the build ran under — the snapshot — not
    // the tree's env state at whatever time the job ends.
    let env_current_reads = Arc::new(AtomicU64::new(0));
    let env_current_reads_in_fs = Arc::clone(&env_current_reads);
    let mut fs = MockFileSystemTrait::new();
    // With an env active the build artifacts live only under O=; the
    // source tree is clean.
    fs.expect_is_dir()
        .withf(|path| {
            path == std::path::Path::new("/home/user/linux")
                || path == std::path::Path::new("/home/user/linux/.kw")
                || path == std::path::Path::new("/home/user/linux/Documentation")
                || path == std::path::Path::new("/home/user/linux/arch")
                || path == std::path::Path::new("/home/user/linux/drivers")
                || path == std::path::Path::new("/home/user/linux/fs")
                || path == std::path::Path::new("/home/user/linux/include")
                || path == std::path::Path::new("/home/user/linux/include/config")
                || path == std::path::Path::new("/home/user/linux/init")
                || path == std::path::Path::new("/home/user/linux/ipc")
                || path == std::path::Path::new("/home/user/linux/kernel")
                || path == std::path::Path::new("/home/user/linux/lib")
                || path == std::path::Path::new("/home/user/linux/scripts")
        })
        .times(13)
        .returning(|path| path != Path::new("/home/user/linux/include/config"));
    fs.expect_is_file()
        .withf(|path| {
            path == std::path::Path::new(
                "/home/user/.cache/kw/envs/L2hvbWUvdXNlci9saW51eA==/testenv/.config",
            ) || path == std::path::Path::new(
                "/home/user/.cache/kw/envs/L2hvbWUvdXNlci9saW51eA==/testenv/arch/x86/boot/bzImage",
            ) || path == std::path::Path::new("/home/user/linux/.config")
                || path == std::path::Path::new("/home/user/linux/.kw/env.current")
                || path == std::path::Path::new("/home/user/linux/COPYING")
                || path == std::path::Path::new("/home/user/linux/CREDITS")
                || path == std::path::Path::new("/home/user/linux/Kbuild")
                || path == std::path::Path::new("/home/user/linux/Makefile")
                || path == std::path::Path::new("/home/user/linux/README")
        })
        .times(9)
        .returning(|path| path != Path::new("/home/user/linux/.config"));
    fs.expect_exists()
        .withf(|path| path == std::path::Path::new("/home/user/linux/MAINTAINERS"))
        .times(1)
        .returning(|_| true);
    fs.expect_read_to_string().withf(|path| path == std::path::Path::new("/home/user/.cache/kw/envs/L2hvbWUvdXNlci9saW51eA==/testenv/include/config/kernel.release")
            || path == std::path::Path::new("/home/user/linux/.kw/build.config")
            || path == std::path::Path::new("/home/user/linux/.kw/env.current")).times(3).returning(move |path| {
        if path.ends_with("env.current") {
            env_current_reads_in_fs.fetch_add(1, Ordering::SeqCst);
            Ok("testenv\n".to_string())
        } else if path.ends_with("build.config") {
            Ok("arch=x86\n".to_string())
        } else if path.ends_with("kernel.release") {
            Ok("6.17.0\n".to_string())
        } else {
            Err(FileSystemError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                "missing",
            )))
        }
    });
    // Every boot-dir probe answers with an image inside the probed
    // dir: the record's image path shows which build root was used.
    fs.expect_read_dir()
        .withf(|path| {
            path == std::path::Path::new(
                "/home/user/.cache/kw/envs/L2hvbWUvdXNlci9saW51eA==/testenv/arch/x86/boot",
            )
        })
        .times(1)
        .returning(|path| {
            if path.ends_with("arch/x86/boot") {
                Ok(vec![path.join("bzImage")])
            } else {
                Err(FileSystemError::IoError(io::Error::new(
                    io::ErrorKind::NotFound,
                    "missing",
                )))
            }
        });
    fs.expect_metadata()
        .withf(|path| {
            path == std::path::Path::new(
                "/home/user/.cache/kw/envs/L2hvbWUvdXNlci9saW51eA==/testenv/arch/x86/boot/bzImage",
            )
        })
        .times(1)
        .returning(|_| Err(FileSystemError::IoError(io::Error::other("no metadata"))));
    fs.expect_create_dir_all()
        .withf(|path| path.starts_with(std::env::temp_dir()))
        .times(1)
        .returning(|_| Ok(()));
    let mut env = MockEnvTrait::new();
    env.expect_which()
        .withf(|name| name == "kw")
        .times(1)
        .returning(|_| true);
    env.expect_var()
        .withf(|key| key == "XDG_CACHE_HOME")
        .times(1)
        .returning(|_| Ok("/home/user/.cache".to_string()));
    let (history, builds) = recording_history(None);
    let (shell, _calls) = recording_shell(KW_VERSION_OK, CLEAN_STATUS, SWITCH_OK);
    let (handle, process, log_dir) = spawn_full_actor("env-build", history, shell, fs, env);
    let mut watch = handle.watch_status().await.expect("status watch opens");

    handle
        .start_build(start_request())
        .await
        .expect("build starts");
    process.last_child().finish(0);
    let status = wait_for_terminal_status(&mut watch).await;
    assert!(matches!(status, KwJobStatus::Succeeded { .. }));

    {
        let builds = builds.lock().expect("builds locks");
        assert_eq!(1, builds.len());
        let record = &builds[0];
        let output_dir = record
            .output_dir
            .as_deref()
            .expect("an env build records its O= dir");
        assert!(
            output_dir.contains("/kw/envs/") && output_dir.ends_with("/testenv"),
            "unexpected output dir: {output_dir}"
        );
        let image = record.image_path.as_deref().expect("image recorded");
        assert!(
            image.starts_with(output_dir) && image.ends_with("bzImage"),
            "image {image} must be probed under the env's output dir {output_dir}"
        );
        assert_eq!(Some("x86"), record.arch.as_deref());
        assert_eq!(Some("6.17.0"), record.kernelrelease.as_deref());
        assert!(record.success);
    }
    // Read at accept time only: the completion record uses the
    // snapshot, never a re-resolve.
    assert_eq!(1, env_current_reads.load(Ordering::SeqCst));

    handle.shutdown().await;
    fs::remove_dir_all(&log_dir).expect("temp dir removes");
}
