//! Readiness probes for `kw build` / `kw deploy` on a configured kernel tree.
//!
//! Probes follow kw's own discovery so "ready" matches what kw will do.
//! Divergences — the `arch`-unset glob and the non-recursive boot-dir scan —
//! are documented on `ReadinessService::find_newest_kernel_image`.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    config::KernelTree,
    infrastructure::{
        env::EnvTrait,
        file_system::{FileSystemError, FileSystemTrait},
        shell::{ShellCommand, ShellTrait},
    },
    kw::{
        history::KwHistoryStore,
        models::{
            history::KwBuildRecord,
            readiness::{
                BootOnceState, DeployAloneRefusal, KwBinaryProbe, KwReadiness, KwReadinessError,
                KwVersionCheck, TreeReadiness,
            },
        },
        remote,
    },
};

pub struct ReadinessService;

/// Minimum kw version this integration is verified against.
pub const KW_MIN_VERSION: (u32, u32) = (0, 10);

impl ReadinessService {
    /// Probes for the kw binary (`which kw`) and, when present, its version
    /// (`kw --version`, whose first line is the version string; repo-mode and
    /// installed kw both print `Branch:`/`Commit:` lines after it).
    pub fn probe_kw_binary(env: &dyn EnvTrait, shell: &dyn ShellTrait) -> KwBinaryProbe {
        if !env.which("kw") {
            return KwBinaryProbe {
                available: false,
                version_line: None,
                check: KwVersionCheck::Unknown,
            };
        }
        let version_line = shell
            .execute(&ShellCommand::new("kw").arg("--version"))
            .ok()
            .filter(|out| out.success)
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|stdout| stdout.lines().next().map(str::to_string))
            .filter(|line| !line.is_empty());
        let check = match &version_line {
            Some(line) => Self::check_kw_version(line),
            None => KwVersionCheck::Unknown,
        };
        KwBinaryProbe {
            available: true,
            version_line,
            check,
        }
    }

    /// Parses kw's `key=value` config, like kw's `parse_configuration`: blank
    /// lines and lines starting with `#` are skipped, text from the last `#`
    /// is a trailing comment, the key loses all whitespace, and the value is
    /// trimmed. Lines without `=` are ignored. A final line without a
    /// trailing newline still counts.
    pub fn parse_kw_config(content: &str) -> HashMap<String, String> {
        let mut entries = HashMap::new();
        for line in content.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let uncommented = match line.rfind('#') {
                Some(index) => &line[..index],
                None => line,
            };
            let Some((key, value)) = uncommented.split_once('=') else {
                continue;
            };
            let key = key.chars().filter(|c| !c.is_whitespace()).collect();
            entries.insert(key, value.trim().to_string());
        }
        entries
    }

    /// Mirrors kw's `is_kernel_root`: the same files and directories kw checks
    /// (also the set `get_maintainer.pl` relies on). `MAINTAINERS` is checked
    /// with `exists` because kw uses `-e` on it and `-f` on the other files.
    pub fn is_kernel_root(fs: &dyn FileSystemTrait, path: &Path) -> bool {
        const FILES: [&str; 5] = ["COPYING", "CREDITS", "Kbuild", "Makefile", "README"];
        const DIRS: [&str; 10] = [
            "Documentation",
            "arch",
            "include",
            "drivers",
            "fs",
            "init",
            "ipc",
            "kernel",
            "lib",
            "scripts",
        ];

        FILES.iter().all(|file| fs.is_file(&path.join(file)))
            && fs.exists(&path.join("MAINTAINERS"))
            && DIRS.iter().all(|dir| fs.is_dir(&path.join(dir)))
    }

    /// Probes whether `tree_path` is a kernel tree ready for kw operations.
    /// `output_dir` is the resolved kw-env `O=` path when an env is active: kw
    /// refuses to activate an env while an in-tree `.config` exists
    /// (`kw_env.sh::validate_env_before_switch`), so with an env active the
    /// `.config` lives only at the env's `O=` dir.
    pub fn probe_tree(
        fs: &dyn FileSystemTrait,
        tree_path: &Path,
        output_dir: Option<&Path>,
    ) -> TreeReadiness {
        if !fs.is_dir(tree_path) {
            return TreeReadiness::Missing;
        }
        if !Self::is_kernel_root(fs, tree_path) {
            return TreeReadiness::NotAKernelRoot;
        }
        if !fs.is_dir(&tree_path.join(".kw")) {
            return TreeReadiness::MissingKwDir;
        }
        if output_dir.is_some()
            && (fs.is_file(&tree_path.join(".config"))
                || fs.is_dir(&tree_path.join("include").join("config")))
        {
            return TreeReadiness::InTreeBuildArtifacts;
        }
        let build_root = output_dir.unwrap_or(tree_path);
        if !fs.is_file(&build_root.join(".config")) {
            return TreeReadiness::MissingKernelConfig;
        }
        TreeReadiness::Ready {
            arch: Self::read_build_arch(fs, tree_path),
        }
    }

    /// The literal `arch=` value from `<tree>/.kw/build.config`, the same
    /// value kw globs under `arch/<arch>/boot/`. `None` when the file or key
    /// is absent or empty — kw treats empty as unset — so the caller globs
    /// `arch/*/boot/` instead of kw's merged-config fallback (see
    /// `ReadinessService::find_newest_kernel_image`).
    pub fn read_build_arch(fs: &dyn FileSystemTrait, tree_path: &Path) -> Option<String> {
        let content = fs
            .read_to_string(&tree_path.join(".kw").join("build.config"))
            .ok()?;
        let arch = Self::parse_kw_config(&content).remove("arch")?;
        if arch.is_empty() {
            None
        } else {
            Some(arch)
        }
    }

    /// Resolves kw's `O=` dir. Env: `<tree>/.kw/env.current`, trailing newlines stripped.
    /// Dir: `{XDG_CACHE_HOME|~/.cache}/kw/envs/<base64(tree)>/<env>`, padded base64,
    /// no wrap, trailing slashes trimmed (kw 0.10 `get_encoded_pwd`; `/` nests dirs).
    /// Ignores `KW_CACHE_DIR` and `KWORKFLOW`. `Ok(None)` if no env is active.
    /// Unreadable `env.current` or no cache base is an error. The dir need not exist.
    pub fn resolve_output_dir(
        fs: &dyn FileSystemTrait,
        env: &dyn EnvTrait,
        tree_path: &Path,
    ) -> Result<Option<PathBuf>, KwReadinessError> {
        let env_file = tree_path.join(".kw").join("env.current");
        if !fs.is_file(&env_file) {
            return Ok(None);
        }
        let env_name = fs
            .read_to_string(&env_file)?
            .trim_end_matches('\n')
            .to_string();
        // An empty env.current names no env; kw's $(<) read yields the same
        // empty string and kw then behaves as if no env were active.
        if env_name.is_empty() {
            return Ok(None);
        }

        // bash's `:-` (and the XDG spec) treat a set-but-empty value as
        // unset; env::var would happily return it as Ok("").
        let cache_base = if let Ok(xdg) = env.var("XDG_CACHE_HOME") {
            if xdg.is_empty() {
                format!("{}/.cache", env.var("HOME")?)
            } else {
                xdg
            }
        } else {
            format!("{}/.cache", env.var("HOME")?)
        };
        let trimmed = tree_path.to_string_lossy();
        let normalized = match trimmed.trim_end_matches('/') {
            "" => "/",
            path => path,
        };
        let encoded = BASE64.encode(normalized);
        Ok(Some(
            Path::new(&cache_base)
                .join("kw")
                .join("envs")
                .join(encoded)
                .join(env_name),
        ))
    }

    /// Newest `*Image` (case-sensitive basename suffix) under
    /// `<build_root>/arch/`; newest mtime wins, path-descending ties.
    /// `arch` set: only `arch/<arch>/boot/`, as kw does. `arch` unset: glob
    /// every `arch/*/boot/` top level — kw would use merged-config `x86_64`
    /// (absent in trees) and fail — so a hit here is not a deploy guarantee.
    pub fn find_newest_kernel_image(
        fs: &dyn FileSystemTrait,
        build_root: &Path,
        arch: Option<&str>,
    ) -> Option<PathBuf> {
        match arch {
            Some(arch) => {
                Self::find_newest_image_in(fs, &build_root.join("arch").join(arch).join("boot"))
            }
            None => fs
                .read_dir(&build_root.join("arch"))
                .ok()?
                .into_iter()
                .filter(|entry| fs.is_dir(entry))
                .filter_map(|arch_dir| {
                    Self::find_newest_image_in(fs, &arch_dir.join("boot"))
                        .map(|image| (Self::read_image_mtime(fs, &image), image))
                })
                .max_by_key(|(mtime, _)| *mtime)
                .map(|(_, image)| image),
        }
    }

    /// Reads the built kernel's release string from
    /// `<build_root>/include/config/kernel.release`, the file a kernel build
    /// generates — cheaper than re-running `make kernelrelease`, and `None`
    /// when the build never produced one (or produced an empty one).
    pub fn read_kernelrelease(fs: &dyn FileSystemTrait, build_root: &Path) -> Option<String> {
        let release = fs
            .read_to_string(
                &build_root
                    .join("include")
                    .join("config")
                    .join("kernel.release"),
            )
            .ok()?;
        let release = release.trim();
        if release.is_empty() {
            None
        } else {
            Some(release.to_string())
        }
    }

    /// Reads `boot_into_new_kernel_once` from `<tree>/.kw/deploy.config`, then
    /// `${XDG_CONFIG_HOME:-$HOME/.config}/kw/deploy.config`. A present but
    /// unreadable tree file is [`BootOnceState::Unknown`] rather than a guess
    /// at the home copy. A readable tree file that simply omits the key still
    /// falls through, matching kw's merged-config lookup.
    pub fn probe_boot_once(
        fs: &dyn FileSystemTrait,
        env: &dyn EnvTrait,
        tree_path: &Path,
    ) -> BootOnceState {
        let local = tree_path.join(".kw").join("deploy.config");
        if fs.is_file(&local) {
            match Self::read_boot_once_from_file(fs, &local) {
                Err(()) => return BootOnceState::Unknown,
                Ok(Some(state)) => return state,
                Ok(None) => {}
            }
        }
        let Some(global) = Self::resolve_xdg_kw_config_file(env, "deploy.config") else {
            return BootOnceState::Unknown;
        };
        if !fs.is_file(&global) {
            return BootOnceState::Unknown;
        }
        match Self::read_boot_once_from_file(fs, &global) {
            Ok(Some(state)) => state,
            Ok(None) | Err(()) => BootOnceState::Unknown,
        }
    }

    /// Deploy-alone record gate: allowed only when a successful build record
    /// exists for the tree and lookup branch, written against the same tree
    /// path and kw env, and an image is still discoverable. A record on
    /// another branch is `HeadMismatch`, not "no build recorded". Tree
    /// readiness is conjoined later by `evaluate_readiness`.
    pub fn check_deploy_alone(
        record: Option<&KwBuildRecord>,
        latest: Option<&KwBuildRecord>,
        tree: &KernelTree,
        head_branch: &str,
        output_dir: Option<&Path>,
        image: Option<&Path>,
    ) -> Result<(), DeployAloneRefusal> {
        let record = match record {
            Some(record) => record,
            None => {
                if let Some(latest) = latest {
                    if latest.branch != head_branch {
                        return Err(DeployAloneRefusal::HeadMismatch {
                            recorded: latest.branch.clone(),
                            current: head_branch.to_string(),
                        });
                    }
                }
                return Err(DeployAloneRefusal::NoBuildRecord);
            }
        };
        if !record.success {
            return Err(DeployAloneRefusal::LastBuildFailed);
        }
        if record.branch != head_branch {
            return Err(DeployAloneRefusal::HeadMismatch {
                recorded: record.branch.clone(),
                current: head_branch.to_string(),
            });
        }
        // Trailing slashes are normalized away: a config edit that only adds
        // or drops one does not move the tree.
        if record.tree_path.trim_end_matches('/') != tree.path().trim_end_matches('/') {
            return Err(DeployAloneRefusal::TreePathDrift {
                recorded: record.tree_path.clone(),
                current: tree.path().to_string(),
            });
        }
        let current_output_dir = output_dir.map(|p| p.to_string_lossy().into_owned());
        if record.output_dir != current_output_dir {
            return Err(DeployAloneRefusal::OutputDirMismatch);
        }
        if image.is_none() {
            return Err(DeployAloneRefusal::ImageMissing);
        }
        Ok(())
    }

    /// All readiness probes for `tree`, composed into a `KwReadiness`.
    /// `head_branch` is the current branch; the caller resolves it via git.
    /// `for_branch`, when set, is the branch deploy-alone is judged against.
    /// `current_branch` still reports the real HEAD so the UI can show both.
    #[expect(clippy::too_many_arguments)]
    pub fn evaluate_readiness(
        fs: &dyn FileSystemTrait,
        env: &dyn EnvTrait,
        shell: &dyn ShellTrait,
        history: &dyn KwHistoryStore,
        kernel_tree_id: &str,
        tree: &KernelTree,
        head_branch: &str,
        for_branch: Option<&str>,
    ) -> Result<KwReadiness, KwReadinessError> {
        let tree_path = Path::new(tree.path());
        let kw_binary = Self::probe_kw_binary(env, shell);
        let output_dir = Self::resolve_output_dir(fs, env, tree_path)?;
        let tree_status = Self::probe_tree(fs, tree_path, output_dir.as_deref());
        let arch = match &tree_status {
            TreeReadiness::Ready { arch } => arch.clone(),
            _ => None,
        };
        let kernel_image = Self::find_newest_kernel_image(
            fs,
            output_dir.as_deref().unwrap_or(tree_path),
            arch.as_deref(),
        );
        let lookup_branch = match for_branch
            .map(str::trim)
            .filter(|branch| !branch.is_empty())
        {
            Some(branch) => branch,
            None => head_branch,
        };
        let (build_record, latest_build) = history.build_records(kernel_tree_id, lookup_branch)?;
        // The tree's current state is part of the verdict: a stale image and a
        // matching record must not green-light a deploy on a tree that has
        // since lost its .config, .kw/, or kernel-root files.
        let deploy_alone = match &tree_status {
            TreeReadiness::Ready { .. } => Self::check_deploy_alone(
                build_record.as_ref(),
                latest_build.as_ref(),
                tree,
                lookup_branch,
                output_dir.as_deref(),
                kernel_image.as_deref(),
            ),
            other => Err(DeployAloneRefusal::TreeNotReady(other.clone())),
        };
        Ok(KwReadiness {
            kw_binary,
            tree: tree_status,
            output_dir,
            deploy_alone,
            current_branch: {
                let trimmed = head_branch.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            },
            deploy_remote: remote::RemoteConfigService::resolve_deploy_remote(fs, env, tree_path),
            boot_once: Self::probe_boot_once(fs, env, tree_path),
        })
    }
}

impl ReadinessService {
    fn check_kw_version(version_line: &str) -> KwVersionCheck {
        match Self::parse_kw_version(version_line) {
            Some(version) => {
                if version >= KW_MIN_VERSION {
                    KwVersionCheck::Meets
                } else {
                    KwVersionCheck::Below(version_line.to_string())
                }
            }
            None => KwVersionCheck::Unknown,
        }
    }

    /// Extracts the first `X.Y[.Z]` pair from a version line: `0.10.0` and the
    /// stale `beta-0.9` kw currently ships both parse.
    fn parse_kw_version(line: &str) -> Option<(u32, u32)> {
        let start = line.find(|c: char| c.is_ascii_digit())?;
        let mut parts = line[start..].splitn(3, '.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts
            .next()?
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>();
        Some((major, minor.parse().ok()?))
    }

    /// Newest `*Image` file directly inside `boot_dir`, if any.
    fn find_newest_image_in(fs: &dyn FileSystemTrait, boot_dir: &Path) -> Option<PathBuf> {
        fs.read_dir(boot_dir)
            .ok()?
            .into_iter()
            .filter(|entry| {
                entry
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with("Image"))
                    && fs.is_file(entry)
            })
            .map(|entry| (Self::read_image_mtime(fs, &entry), entry))
            .max_by_key(|(mtime, _)| *mtime)
            .map(|(_, entry)| entry)
    }

    fn read_image_mtime(fs: &dyn FileSystemTrait, path: &Path) -> SystemTime {
        fs.metadata(path)
            .and_then(|meta| meta.modified().map_err(FileSystemError::from))
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }

    fn read_boot_once_from_file(
        fs: &dyn FileSystemTrait,
        path: &Path,
    ) -> Result<Option<BootOnceState>, ()> {
        let content = fs.read_to_string(path).map_err(|_| ())?;
        Ok(Self::parse_kw_config(&content)
            .remove("boot_into_new_kernel_once")
            .map(|value| match value.as_str() {
                "no" => BootOnceState::Off,
                "yes" => BootOnceState::On,
                _ => BootOnceState::Unknown,
            }))
    }

    /// `${XDG_CONFIG_HOME:-$HOME/.config}/kw/<filename>`. A set-but-empty
    /// `XDG_CONFIG_HOME` is treated as unset, matching bash `:-` and the XDG
    /// spec.
    fn resolve_xdg_kw_config_file(env: &dyn EnvTrait, filename: &str) -> Option<PathBuf> {
        let config_home = if let Ok(xdg) = env.var("XDG_CONFIG_HOME") {
            if xdg.is_empty() {
                format!("{}/.config", env.var("HOME").ok()?)
            } else {
                xdg
            }
        } else {
            format!("{}/.config", env.var("HOME").ok()?)
        };
        Some(Path::new(&config_home).join("kw").join(filename))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, SystemTime},
    };

    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

    use crate::config::KernelTree;
    use crate::infrastructure::{
        env::MockEnvTrait,
        file_system::{FileSystemError, MockFileSystemTrait, OsFileSystem},
        shell::{MockShellTrait, ShellOutput},
    };
    use crate::kw::models::readiness::{BootOnceState, DeployAloneRefusal, TreeReadiness};
    use crate::kw::models::remote::RemoteRefusal;
    use crate::kw::{
        history::{FileKwHistoryStore, KwHistoryStore},
        models::history::KwBuildRecord,
    };

    use super::*;

    use std::env;
    use std::io;
    use std::process;
    use std::sync::Arc;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(test_name: &str) -> Self {
            let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
            let dir = env::temp_dir().join(format!(
                "patch-hub-kw-readiness-{}-{}-{}",
                test_name,
                process::id(),
                n
            ));
            // A leftover from a failed previous run must not poison this one.
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("dir creates");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const KERNEL_ROOT_FILES: [&str; 6] = [
        "COPYING",
        "CREDITS",
        "Kbuild",
        "Makefile",
        "README",
        "MAINTAINERS",
    ];
    const KERNEL_ROOT_DIRS: [&str; 10] = [
        "Documentation",
        "arch",
        "include",
        "drivers",
        "fs",
        "init",
        "ipc",
        "kernel",
        "lib",
        "scripts",
    ];

    /// Creates the exact file/dir set kw's `is_kernel_root` expects.
    fn make_kernel_root(dir: &Path) {
        for file in KERNEL_ROOT_FILES {
            fs::write(dir.join(file), "").expect("file writes");
        }
        for sub in KERNEL_ROOT_DIRS {
            fs::create_dir(dir.join(sub)).expect("dir creates");
        }
    }

    /// A kernel root with `.kw/` and an in-tree `.config`: the fully ready
    /// fixture most probes start from.
    fn make_ready_tree(test_name: &str) -> TempDir {
        let dir = TempDir::new(test_name);
        make_kernel_root(dir.path());
        fs::create_dir(dir.path().join(".kw")).expect("dir creates");
        fs::write(dir.path().join(".config"), "").expect("file writes");
        dir
    }

    #[test]
    fn parse_kw_config_mirrors_kw_semantics() {
        // Deliberately no trailing newline: kw's read loop handles a final
        // unterminated line, and so must we.
        let content = "\
# a comment line
arch=x86_64

  cpu_scaling_factor  =  100
kernel_img_name=bzImage # trailing comment
dtb_copy_pattern={broadcom,rockchip}#kept
no_equals_line
cross_compile=
last_line_without_newline=yes";
        let parsed = ReadinessService::parse_kw_config(content);

        assert_eq!(parsed.get("arch"), Some(&"x86_64".to_string()));
        assert_eq!(parsed.get("cpu_scaling_factor"), Some(&"100".to_string()));
        assert_eq!(parsed.get("kernel_img_name"), Some(&"bzImage".to_string()));
        // kw strips from the last '#', so an earlier one stays in the value.
        assert_eq!(
            parsed.get("dtb_copy_pattern"),
            Some(&"{broadcom,rockchip}".to_string())
        );
        assert_eq!(parsed.get("cross_compile"), Some(&String::new()));
        assert_eq!(
            parsed.get("last_line_without_newline"),
            Some(&"yes".to_string())
        );
        assert!(!parsed.contains_key("no_equals_line"));
        assert_eq!(6, parsed.len());
    }

    #[test]
    fn is_kernel_root_accepts_complete_fixture() {
        let dir = TempDir::new("kernel-root-complete");
        make_kernel_root(dir.path());

        assert!(ReadinessService::is_kernel_root(&OsFileSystem, dir.path()));
    }

    #[test]
    fn is_kernel_root_rejects_any_missing_member() {
        for member in KERNEL_ROOT_FILES.into_iter().chain(KERNEL_ROOT_DIRS) {
            let dir = TempDir::new("kernel-root-incomplete");
            make_kernel_root(dir.path());
            fs::remove_dir_all(dir.path().join(member))
                .or_else(|_| fs::remove_file(dir.path().join(member)))
                .expect("kernel member removes");

            assert!(
                !ReadinessService::is_kernel_root(&OsFileSystem, dir.path()),
                "missing {member} should fail the check"
            );
        }
    }

    #[test]
    fn probe_tree_reports_missing_path() {
        let dir = TempDir::new("probe-missing");

        assert_eq!(
            TreeReadiness::Missing,
            ReadinessService::probe_tree(&OsFileSystem, &dir.path().join("nope"), None)
        );
    }

    #[test]
    fn probe_tree_reports_non_kernel_root() {
        let dir = TempDir::new("probe-not-root");

        assert_eq!(
            TreeReadiness::NotAKernelRoot,
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_reports_missing_kw_dir() {
        let dir = TempDir::new("probe-no-kw");
        make_kernel_root(dir.path());

        assert_eq!(
            TreeReadiness::MissingKwDir,
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_reports_missing_kernel_config() {
        let dir = TempDir::new("probe-no-config");
        make_kernel_root(dir.path());
        fs::create_dir(dir.path().join(".kw")).expect("dir creates");

        assert_eq!(
            TreeReadiness::MissingKernelConfig,
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_ready_without_build_config_means_glob_fallback() {
        let dir = make_ready_tree("probe-ready");

        assert_eq!(
            TreeReadiness::Ready { arch: None },
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_ready_reads_arch_from_build_config() {
        let dir = make_ready_tree("probe-ready-arch");
        fs::write(dir.path().join(".kw/build.config"), "arch=arm64\n").expect("file writes");

        assert_eq!(
            TreeReadiness::Ready {
                arch: Some("arm64".to_string())
            },
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_with_active_env_checks_config_in_output_dir() {
        let dir = make_ready_tree("probe-env");
        let out = TempDir::new("probe-env-output");
        // With a kw env active, kw moves the .config into the env's O= dir.
        fs::remove_file(dir.path().join(".config")).expect("file removes");

        assert_eq!(
            TreeReadiness::MissingKernelConfig,
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), Some(out.path()))
        );

        fs::write(out.path().join(".config"), "").expect("file writes");
        assert!(matches!(
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), Some(out.path())),
            TreeReadiness::Ready { .. }
        ));
    }

    #[test]
    fn probe_tree_with_active_env_refuses_in_tree_config() {
        let dir = make_ready_tree("probe-env-in-tree-config");
        let out = TempDir::new("probe-env-in-tree-config-output");
        fs::write(out.path().join(".config"), "").expect("file writes");

        assert_eq!(
            TreeReadiness::InTreeBuildArtifacts,
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), Some(out.path()))
        );
    }

    #[test]
    fn probe_tree_with_active_env_refuses_in_tree_include_config() {
        let dir = make_ready_tree("probe-env-in-tree-include-config");
        fs::remove_file(dir.path().join(".config")).expect("file removes");
        fs::create_dir(dir.path().join("include/config")).expect("dir creates");
        let out = TempDir::new("probe-env-in-tree-include-config-output");
        fs::write(out.path().join(".config"), "").expect("file writes");

        assert_eq!(
            TreeReadiness::InTreeBuildArtifacts,
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), Some(out.path()))
        );
    }

    #[test]
    fn probe_tree_without_env_accepts_in_tree_build_artifacts() {
        let dir = make_ready_tree("probe-no-env-in-tree-artifacts");
        fs::create_dir(dir.path().join("include/config")).expect("dir creates");

        assert_eq!(
            TreeReadiness::Ready { arch: None },
            ReadinessService::probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn read_build_arch_reads_literal_value() {
        let dir = make_ready_tree("arch-literal");
        fs::write(dir.path().join(".kw/build.config"), "arch=x86\n").expect("file writes");

        assert_eq!(
            Some("x86".to_string()),
            ReadinessService::read_build_arch(&OsFileSystem, dir.path())
        );
    }

    #[test]
    fn read_build_arch_unset_means_glob_fallback() {
        // Missing file, missing key, commented key, and empty value all map
        // to kw's "unset" semantics.
        let no_file = make_ready_tree("arch-no-file");
        assert_eq!(
            None,
            ReadinessService::read_build_arch(&OsFileSystem, no_file.path())
        );

        let no_key = make_ready_tree("arch-no-key");
        fs::write(
            no_key.path().join(".kw/build.config"),
            "cpu_scaling_factor=100\n",
        )
        .expect("file writes");
        assert_eq!(
            None,
            ReadinessService::read_build_arch(&OsFileSystem, no_key.path())
        );

        let commented = make_ready_tree("arch-commented");
        fs::write(commented.path().join(".kw/build.config"), "#arch=riscv\n").expect("file writes");
        assert_eq!(
            None,
            ReadinessService::read_build_arch(&OsFileSystem, commented.path())
        );

        let empty = make_ready_tree("arch-empty");
        fs::write(empty.path().join(".kw/build.config"), "arch=\n").expect("file writes");
        assert_eq!(
            None,
            ReadinessService::read_build_arch(&OsFileSystem, empty.path())
        );
    }

    /// Creates a file whose mtime is `mtime_secs` seconds after the epoch,
    /// so newest-wins ordering is fully deterministic.
    fn write_file_with_mtime(path: &Path, mtime_secs: u64) {
        let file = fs::File::create(path).expect("file creates");
        file.set_times(
            fs::FileTimes::new()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(mtime_secs)),
        )
        .expect("mtime sets");
    }

    #[test]
    fn resolve_output_dir_without_env_file_is_inactive() {
        let dir = make_ready_tree("env-inactive");
        let env = MockEnvTrait::new();

        assert_eq!(
            None,
            ReadinessService::resolve_output_dir(&OsFileSystem, &env, dir.path())
                .expect("output dir resolves")
        );
    }

    #[test]
    fn resolve_output_dir_encodes_tree_path_like_kw() {
        // Known vector: `printf '%s' /home/user/linux | base64 --wrap=0`.
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| path == std::path::Path::new("/home/user/linux/.kw/env.current"))
            .times(1)
            .returning(|p| p == Path::new("/home/user/linux/.kw/env.current"));
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/home/user/linux/.kw/env.current"))
            .times(1)
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CACHE_HOME")
            .times(1)
            .returning(|_| Ok("/xdg".to_string()));

        let resolved =
            ReadinessService::resolve_output_dir(&fs, &env, Path::new("/home/user/linux"))
                .expect("output dir resolves");

        assert_eq!(
            Some(PathBuf::from("/xdg/kw/envs/L2hvbWUvdXNlci9saW51eA==/minix")),
            resolved
        );
    }

    #[test]
    fn resolve_output_dir_falls_back_to_home_cache() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CACHE_HOME")
            .times(1)
            .returning(|_| Err(env::VarError::NotPresent.into()));
        env.expect_var()
            .withf(|key| key == "HOME")
            .times(1)
            .returning(|_| Ok("/home/user".to_string()));

        let resolved = ReadinessService::resolve_output_dir(&fs, &env, Path::new("/kernel"))
            .expect("output dir resolves");

        assert_eq!(
            Some(
                Path::new("/home/user/.cache/kw/envs")
                    .join(BASE64.encode("/kernel"))
                    .join("minix")
            ),
            resolved
        );
    }

    #[test]
    fn resolve_output_dir_treats_empty_xdg_cache_home_as_unset() {
        // bash's `:-` (and the XDG spec) treat set-but-empty as unset;
        // otherwise the resolved path would be relative to cwd.
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CACHE_HOME")
            .times(1)
            .returning(|_| Ok(String::new()));
        env.expect_var()
            .withf(|key| key == "HOME")
            .times(1)
            .returning(|_| Ok("/home/user".to_string()));

        let resolved = ReadinessService::resolve_output_dir(&fs, &env, Path::new("/kernel"))
            .expect("output dir resolves");

        assert_eq!(
            Some(
                Path::new("/home/user/.cache/kw/envs")
                    .join(BASE64.encode("/kernel"))
                    .join("minix")
            ),
            resolved
        );
    }

    #[test]
    fn resolve_output_dir_trims_trailing_slashes_before_encoding() {
        // kw encodes $PWD after cd-ing into the tree, where the path no
        // longer carries a trailing slash.
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| path == std::path::Path::new("/home/user/linux/.kw/env.current"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/home/user/linux/.kw/env.current"))
            .times(1)
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CACHE_HOME")
            .times(1)
            .returning(|_| Ok("/xdg".to_string()));

        let resolved =
            ReadinessService::resolve_output_dir(&fs, &env, Path::new("/home/user/linux/"))
                .expect("output dir resolves");

        assert_eq!(
            Some(PathBuf::from("/xdg/kw/envs/L2hvbWUvdXNlci9saW51eA==/minix")),
            resolved
        );
    }

    #[test]
    fn resolve_output_dir_empty_env_file_is_inactive() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| Ok("\n".to_string()));
        let env = MockEnvTrait::new();

        assert_eq!(
            None,
            ReadinessService::resolve_output_dir(&fs, &env, Path::new("/kernel"))
                .expect("output dir resolves")
        );
    }

    #[test]
    fn resolve_output_dir_unreadable_env_file_errors() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied").into()));
        let env = MockEnvTrait::new();

        assert!(ReadinessService::resolve_output_dir(&fs, &env, Path::new("/kernel")).is_err());
    }

    #[test]
    fn resolve_output_dir_errors_without_any_cache_base() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| true);
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/env.current"))
            .times(1)
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| matches!(key, "HOME" | "XDG_CACHE_HOME"))
            .times(2)
            .returning(|_| Err(env::VarError::NotPresent.into()));

        assert!(ReadinessService::resolve_output_dir(&fs, &env, Path::new("/kernel")).is_err());
    }

    #[test]
    fn find_image_with_arch_picks_newest_image_only() {
        let dir = make_ready_tree("image-arch");
        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).expect("dir creates");
        write_file_with_mtime(&boot.join("bzImage"), 100);
        write_file_with_mtime(&boot.join("Image"), 200);
        // Neither name matches find's case-sensitive `*Image`.
        write_file_with_mtime(&boot.join("vmlinux"), 300);
        write_file_with_mtime(&boot.join("Image.gz"), 400);

        assert_eq!(
            Some(boot.join("Image")),
            ReadinessService::find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );
    }

    #[test]
    fn find_image_without_arch_globs_every_boot_dir() {
        // The no-false-negative regression: with no arch= hint, an image
        // under any arch/*/boot/ must still be found.
        let dir = make_ready_tree("image-glob");
        let x86_boot = dir.path().join("arch/x86/boot");
        let arm64_boot = dir.path().join("arch/arm64/boot");
        fs::create_dir_all(&x86_boot).expect("dir creates");
        fs::create_dir_all(&arm64_boot).expect("dir creates");
        write_file_with_mtime(&x86_boot.join("bzImage"), 100);
        write_file_with_mtime(&arm64_boot.join("Image"), 200);

        assert_eq!(
            Some(arm64_boot.join("Image")),
            ReadinessService::find_newest_kernel_image(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn find_image_tie_breaks_by_path_descending_like_kw() {
        // Same mtime: kw's `sort -r | head -1` on `%T+ %p` picks the
        // lexicographically larger path.
        let dir = make_ready_tree("image-tie");
        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).expect("dir creates");
        write_file_with_mtime(&boot.join("bzImage"), 100);
        write_file_with_mtime(&boot.join("zImage"), 100);

        assert_eq!(
            Some(boot.join("zImage")),
            ReadinessService::find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );
    }

    #[test]
    fn find_image_missing_or_empty_dirs_return_none() {
        let dir = make_ready_tree("image-none");
        // No image anywhere yet: the fixture has an empty arch/ dir.
        assert_eq!(
            None,
            ReadinessService::find_newest_kernel_image(&OsFileSystem, dir.path(), None)
        );
        assert_eq!(
            None,
            ReadinessService::find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );

        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).expect("dir creates");
        write_file_with_mtime(&boot.join("image"), 100); // lowercase: no match
        write_file_with_mtime(&boot.join("Image.gz"), 200); // suffix: no match

        assert_eq!(
            None,
            ReadinessService::find_newest_kernel_image(&OsFileSystem, dir.path(), None)
        );
        assert_eq!(
            None,
            ReadinessService::find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );
    }

    #[test]
    fn kernelrelease_reads_and_trims_the_release_file() {
        let dir = make_ready_tree("kernelrelease");
        let config_dir = dir.path().join("include").join("config");
        fs::create_dir_all(&config_dir).expect("dir creates");
        fs::write(config_dir.join("kernel.release"), "6.17.0-rc1\n").expect("file writes");

        assert_eq!(
            Some("6.17.0-rc1".to_string()),
            ReadinessService::read_kernelrelease(&OsFileSystem, dir.path())
        );
    }

    #[test]
    fn kernelrelease_is_none_without_a_release_file() {
        let dir = make_ready_tree("kernelrelease-missing");

        assert_eq!(
            None,
            ReadinessService::read_kernelrelease(&OsFileSystem, dir.path())
        );

        let config_dir = dir.path().join("include").join("config");
        fs::create_dir_all(&config_dir).expect("dir creates");
        fs::write(config_dir.join("kernel.release"), "\n").expect("file writes");
        assert_eq!(
            None,
            ReadinessService::read_kernelrelease(&OsFileSystem, dir.path())
        );
    }

    fn shell_output(stdout: &str, success: bool) -> ShellOutput {
        ShellOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            success,
        }
    }

    fn kernel_tree(path: &Path) -> KernelTree {
        serde_json::from_value(serde_json::json!({
            "path": path.to_str().expect("path is utf-8"),
            "branch": "master"
        }))
        .expect("json parses")
    }

    fn built_record(tree: &Path, branch: &str) -> KwBuildRecord {
        KwBuildRecord {
            kernel_tree_id: "mainline".to_string(),
            tree_path: tree.to_str().expect("path is utf-8").to_string(),
            message_id: None,
            branch: branch.to_string(),
            arch: Some("x86".to_string()),
            image_path: None,
            output_dir: None,
            kernelrelease: None,
            log_path: String::new(),
            built_at: "2026-08-01T18:10:00Z".to_string(),
            success: true,
        }
    }

    #[test]
    fn kw_probe_missing_binary_never_spawns() {
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(1)
            .returning(|_| false);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().withf(|_| true).times(0);

        let probe = ReadinessService::probe_kw_binary(&env, &shell);

        assert!(!probe.available);
        assert_eq!(None, probe.version_line);
        assert_eq!(KwVersionCheck::Unknown, probe.check);
    }

    #[test]
    fn kw_probe_compares_version_against_floor() {
        for (stdout, expected) in [
            ("0.10.0\n", KwVersionCheck::Meets),
            ("0.10\n", KwVersionCheck::Meets),
            ("1.0\n", KwVersionCheck::Meets),
            ("0.9.9\n", KwVersionCheck::Below("0.9.9".to_string())),
            // What a real 0.10 install prints: kw's shipped VERSION is stale.
            (
                "beta-0.9\nBranch: master\nCommit: 3575d38\n",
                KwVersionCheck::Below("beta-0.9".to_string()),
            ),
            ("not a version\n", KwVersionCheck::Unknown),
        ] {
            let mut env = MockEnvTrait::new();
            env.expect_which()
                .withf(|name| name == "kw")
                .times(1)
                .returning(|_| true);
            let mut shell = MockShellTrait::new();
            let stdout_bytes = stdout.as_bytes().to_vec();
            shell
                .expect_execute()
                .withf(|cmd| cmd.program == "kw" && cmd.args == ["--version"])
                .times(1)
                .returning(move |_| {
                    Ok(shell_output(
                        str::from_utf8(&stdout_bytes).expect("bytes are utf-8"),
                        true,
                    ))
                });

            let probe = ReadinessService::probe_kw_binary(&env, &shell);

            assert!(probe.available, "for version output {stdout:?}");
            assert_eq!(expected, probe.check, "for version output {stdout:?}");
        }
    }

    #[test]
    fn kw_probe_keeps_the_raw_first_line_verbatim() {
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
                Ok(shell_output(
                    "beta-0.9\nBranch: master\nCommit: 3575d38\n",
                    true,
                ))
            });

        let probe = ReadinessService::probe_kw_binary(&env, &shell);

        assert_eq!(Some("beta-0.9".to_string()), probe.version_line);
    }

    #[test]
    fn kw_probe_unknown_when_version_is_unreadable() {
        // kw is on PATH but its --version output cannot be trusted.
        let mut spawn_fails = MockShellTrait::new();
        spawn_fails
            .expect_execute()
            .withf(|cmd| cmd.program == "kw" && cmd.args == ["--version"])
            .times(1)
            .returning(|_| Err(io::Error::other("spawn failed").into()));

        let mut empty_stdout = MockShellTrait::new();
        empty_stdout
            .expect_execute()
            .withf(|cmd| cmd.program == "kw" && cmd.args == ["--version"])
            .times(1)
            .returning(|_| Ok(shell_output("", true)));

        let mut kw_fails = MockShellTrait::new();
        kw_fails
            .expect_execute()
            .withf(|cmd| cmd.program == "kw" && cmd.args == ["--version"])
            .times(1)
            .returning(|_| Ok(shell_output("0.10.0\n", false)));

        for shell in [spawn_fails, empty_stdout, kw_fails] {
            let mut env = MockEnvTrait::new();
            env.expect_which()
                .withf(|name| name == "kw")
                .times(1)
                .returning(|_| true);

            let probe = ReadinessService::probe_kw_binary(&env, &shell);

            assert!(probe.available);
            assert_eq!(None, probe.version_line);
            assert_eq!(KwVersionCheck::Unknown, probe.check);
        }
    }

    #[test]
    fn deploy_alone_requires_a_successful_build_record() {
        let dir = make_ready_tree("deploy-gate");
        let tree = kernel_tree(dir.path());
        let image = dir.path().join("arch/x86/boot/bzImage");

        assert_eq!(
            Err(DeployAloneRefusal::NoBuildRecord),
            ReadinessService::check_deploy_alone(
                None,
                None,
                &tree,
                "patchset-x",
                None,
                Some(&image)
            )
        );

        let mut failed = built_record(dir.path(), "patchset-x");
        failed.success = false;
        assert_eq!(
            Err(DeployAloneRefusal::LastBuildFailed),
            ReadinessService::check_deploy_alone(
                Some(&failed),
                None,
                &tree,
                "patchset-x",
                None,
                Some(&image)
            )
        );
    }

    #[test]
    fn deploy_alone_refuses_frankenstein_combinations() {
        let dir = make_ready_tree("deploy-frankenstein");
        let tree = kernel_tree(dir.path());
        let image = dir.path().join("arch/x86/boot/bzImage");
        let record = built_record(dir.path(), "patchset-x");

        // Keyed-record sanity: the store returned a row whose branch
        // field does not match the lookup key.
        assert_eq!(
            Err(DeployAloneRefusal::HeadMismatch {
                recorded: "patchset-x".to_string(),
                current: "master".to_string(),
            }),
            ReadinessService::check_deploy_alone(
                Some(&record),
                None,
                &tree,
                "master",
                None,
                Some(&image)
            )
        );

        // No keyed row for the target, but the tree has a latest build
        // on another branch.
        assert_eq!(
            Err(DeployAloneRefusal::HeadMismatch {
                recorded: "patchset-x".to_string(),
                current: "master".to_string(),
            }),
            ReadinessService::check_deploy_alone(
                None,
                Some(&record),
                &tree,
                "master",
                None,
                Some(&image)
            )
        );

        // The config repointed the same tree id at another path.
        let moved_tree = kernel_tree(Path::new("/elsewhere/linux"));
        assert_eq!(
            Err(DeployAloneRefusal::TreePathDrift {
                recorded: dir.path().to_str().expect("path is utf-8").to_string(),
                current: "/elsewhere/linux".to_string(),
            }),
            ReadinessService::check_deploy_alone(
                Some(&record),
                None,
                &moved_tree,
                "patchset-x",
                None,
                Some(&image)
            )
        );

        // The active kw env changed since the build.
        assert_eq!(
            Err(DeployAloneRefusal::OutputDirMismatch),
            ReadinessService::check_deploy_alone(
                Some(&record),
                None,
                &tree,
                "patchset-x",
                Some(Path::new("/cache/kw/envs/xyz/minix")),
                Some(&image),
            )
        );

        // The image the build produced is gone.
        assert_eq!(
            Err(DeployAloneRefusal::ImageMissing),
            ReadinessService::check_deploy_alone(
                Some(&record),
                None,
                &tree,
                "patchset-x",
                None,
                None
            )
        );

        assert_eq!(
            Ok(()),
            ReadinessService::check_deploy_alone(
                Some(&record),
                None,
                &tree,
                "patchset-x",
                None,
                Some(&image)
            )
        );

        // A trailing-slash-only difference is the same tree, not drift.
        let mut slashed = built_record(dir.path(), "patchset-x");
        slashed.tree_path = format!("{}/", dir.path().to_str().expect("path is utf-8"));
        assert_eq!(
            Ok(()),
            ReadinessService::check_deploy_alone(
                Some(&slashed),
                None,
                &tree,
                "patchset-x",
                None,
                Some(&image)
            )
        );
    }

    #[test]
    fn deploy_alone_checks_failure_before_branch_mismatch() {
        let dir = make_ready_tree("deploy-order");
        let tree = kernel_tree(dir.path());
        let mut failed = built_record(dir.path(), "patchset-x");
        failed.success = false;

        assert_eq!(
            Err(DeployAloneRefusal::LastBuildFailed),
            ReadinessService::check_deploy_alone(Some(&failed), None, &tree, "master", None, None)
        );
    }

    #[test]
    fn evaluate_readiness_composes_all_probes() {
        let dir = make_ready_tree("evaluate");
        fs::write(dir.path().join(".kw/build.config"), "arch=x86\n").expect("file writes");
        fs::write(
            dir.path().join(".kw/deploy.config"),
            "boot_into_new_kernel_once=no\n",
        )
        .expect("file writes");
        fs::write(
            dir.path().join(".kw/remote.config"),
            "#kw-default=dut\nHost dut\n  Hostname box\n  Port 22\n  User root\n",
        )
        .expect("file writes");
        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).expect("dir creates");
        write_file_with_mtime(&boot.join("bzImage"), 100);

        let data = TempDir::new("evaluate-data");
        let history = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            data.path().to_str().expect("path is utf-8").to_string(),
        );
        history
            .record_build(built_record(dir.path(), "patchset-x"))
            .expect("build records");

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
            .returning(|_| Ok(shell_output("0.10.0\n", true)));

        let tree = kernel_tree(dir.path());
        let readiness = ReadinessService::evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "patchset-x",
            None,
        )
        .expect("readiness evaluates");

        assert_eq!(
            TreeReadiness::Ready {
                arch: Some("x86".to_string())
            },
            readiness.tree
        );
        // A matching successful record plus the image under arch/x86/boot
        // is what makes deploy-alone succeed.
        assert_eq!(Ok(()), readiness.deploy_alone);
        assert_eq!(None, readiness.output_dir);
        assert!(readiness.kw_binary.available);
        assert_eq!(KwVersionCheck::Meets, readiness.kw_binary.check);
        assert_eq!(Some("patchset-x".to_string()), readiness.current_branch);
        assert_eq!(
            "root@box:22",
            readiness
                .deploy_remote
                .expect("deploy remote is set")
                .endpoint()
        );
        assert_eq!(BootOnceState::Off, readiness.boot_once);
    }

    #[test]
    fn evaluate_readiness_missing_tree_short_circuits() {
        let dir = TempDir::new("evaluate-missing");
        let missing = dir.path().join("nope");
        let data = TempDir::new("evaluate-missing-data");
        let history = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            data.path().to_str().expect("path is utf-8").to_string(),
        );

        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(1)
            .returning(|_| false);
        env.expect_var()
            .withf(|key| matches!(key, "HOME" | "XDG_CONFIG_HOME"))
            .times(4)
            .returning(|_| Err(env::VarError::NotPresent.into()));
        let mut shell = MockShellTrait::new();
        shell.expect_execute().withf(|_| true).times(0);

        let tree = kernel_tree(&missing);
        let readiness = ReadinessService::evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "patchset-x",
            None,
        )
        .expect("readiness evaluates");

        assert_eq!(TreeReadiness::Missing, readiness.tree);
        // Tree readiness is conjoined into the deploy-alone verdict, so
        // Ok(()) can never describe a tree that is not build-ready.
        assert_eq!(
            Err(DeployAloneRefusal::TreeNotReady(TreeReadiness::Missing)),
            readiness.deploy_alone
        );
        assert_eq!(
            Err(RemoteRefusal::NoRemotesConfigured),
            readiness.deploy_remote
        );
        assert_eq!(BootOnceState::Unknown, readiness.boot_once);
    }

    #[test]
    fn evaluate_readiness_without_build_refuses_deploy_alone() {
        let dir = make_ready_tree("evaluate-nobuild");
        let data = TempDir::new("evaluate-nobuild-data");
        let history = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            data.path().to_str().expect("path is utf-8").to_string(),
        );

        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(1)
            .returning(|_| false);
        env.expect_var()
            .withf(|key| matches!(key, "HOME" | "XDG_CONFIG_HOME"))
            .times(4)
            .returning(|_| Err(env::VarError::NotPresent.into()));
        let mut shell = MockShellTrait::new();
        shell.expect_execute().withf(|_| true).times(0);

        let tree = kernel_tree(dir.path());
        let readiness = ReadinessService::evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "patchset-x",
            None,
        )
        .expect("readiness evaluates");

        // No build.config: arch stays None (glob fallback); no images exist.
        assert_eq!(TreeReadiness::Ready { arch: None }, readiness.tree);
        assert_eq!(
            Err(DeployAloneRefusal::NoBuildRecord),
            readiness.deploy_alone
        );
        assert!(!readiness.kw_binary.available);
        assert_eq!(Some("patchset-x".to_string()), readiness.current_branch);
        assert_eq!(BootOnceState::Unknown, readiness.boot_once);
    }

    #[test]
    fn evaluate_readiness_empty_head_has_no_current_branch() {
        let dir = make_ready_tree("evaluate-detached");
        let data = TempDir::new("evaluate-detached-data");
        let history = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            data.path().to_str().expect("path is utf-8").to_string(),
        );
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(1)
            .returning(|_| false);
        env.expect_var()
            .withf(|key| matches!(key, "HOME" | "XDG_CONFIG_HOME"))
            .times(4)
            .returning(|_| Err(env::VarError::NotPresent.into()));
        let mut shell = MockShellTrait::new();
        shell.expect_execute().withf(|_| true).times(0);
        let tree = kernel_tree(dir.path());
        let readiness = ReadinessService::evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "",
            None,
        )
        .expect("readiness evaluates");
        assert_eq!(None, readiness.current_branch);
    }

    #[test]
    fn evaluate_readiness_for_branch_looks_up_that_branch_not_head() {
        let dir = make_ready_tree("evaluate-for-branch");
        let data = TempDir::new("evaluate-for-branch-data");
        let history = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            data.path().to_str().expect("path is utf-8").to_string(),
        );
        history
            .record_build(built_record(dir.path(), "patchset-x"))
            .expect("build records");
        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).expect("dir creates");
        write_file_with_mtime(&boot.join("bzImage"), 100);
        fs::write(dir.path().join(".kw/build.config"), "arch=x86\n").expect("file writes");

        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(2)
            .returning(|_| false);
        env.expect_var()
            .withf(|key| matches!(key, "HOME" | "XDG_CONFIG_HOME"))
            .times(8)
            .returning(|_| Err(env::VarError::NotPresent.into()));
        let mut shell = MockShellTrait::new();
        shell.expect_execute().withf(|_| true).times(0);
        let tree = kernel_tree(dir.path());

        let on_head = ReadinessService::evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "master",
            None,
        )
        .expect("readiness evaluates");
        assert_eq!(Some("master".to_string()), on_head.current_branch);
        assert_eq!(
            Err(DeployAloneRefusal::HeadMismatch {
                recorded: "patchset-x".to_string(),
                current: "master".to_string(),
            }),
            on_head.deploy_alone
        );

        let for_typed = ReadinessService::evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "master",
            Some("patchset-x"),
        )
        .expect("readiness evaluates");
        // HEAD is still master; deploy-alone is judged against the typed branch.
        assert_eq!(Some("master".to_string()), for_typed.current_branch);
        assert_eq!(Ok(()), for_typed.deploy_alone);
    }

    #[test]
    fn probe_boot_once_reads_literal_no_and_yes() {
        let off = make_ready_tree("boot-once-off");
        fs::write(
            off.path().join(".kw/deploy.config"),
            "boot_into_new_kernel_once=no\n",
        )
        .expect("file writes");
        let env = MockEnvTrait::new();
        assert_eq!(
            BootOnceState::Off,
            ReadinessService::probe_boot_once(&OsFileSystem, &env, off.path())
        );

        let on = make_ready_tree("boot-once-on");
        fs::write(
            on.path().join(".kw/deploy.config"),
            "boot_into_new_kernel_once=yes\n",
        )
        .expect("file writes");
        assert_eq!(
            BootOnceState::On,
            ReadinessService::probe_boot_once(&OsFileSystem, &env, on.path())
        );
    }

    #[test]
    fn probe_boot_once_unknown_values_and_missing_files_gate() {
        let empty = make_ready_tree("boot-once-missing");
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| matches!(key, "HOME" | "XDG_CONFIG_HOME"))
            .times(2)
            .returning(|_| Err(env::VarError::NotPresent.into()));
        assert_eq!(
            BootOnceState::Unknown,
            ReadinessService::probe_boot_once(&OsFileSystem, &env, empty.path())
        );

        let weird = make_ready_tree("boot-once-weird");
        fs::write(
            weird.path().join(".kw/deploy.config"),
            "boot_into_new_kernel_once=true\n",
        )
        .expect("file writes");
        assert_eq!(
            BootOnceState::Unknown,
            ReadinessService::probe_boot_once(&OsFileSystem, &MockEnvTrait::new(), weird.path())
        );
    }

    #[test]
    fn probe_boot_once_falls_through_to_xdg_when_the_tree_omits_the_key() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| {
                path == std::path::Path::new("/kernel/.kw/deploy.config")
                    || path == std::path::Path::new("/xdg/kw/deploy.config")
            })
            .times(2)
            .returning(|path| {
                path.ends_with(".kw/deploy.config") || path == Path::new("/xdg/kw/deploy.config")
            });
        fs.expect_read_to_string()
            .withf(|path| {
                path == std::path::Path::new("/kernel/.kw/deploy.config")
                    || path == std::path::Path::new("/xdg/kw/deploy.config")
            })
            .times(2)
            .returning(|path| {
                if path.ends_with(".kw/deploy.config") {
                    Ok("reboot=no\n".to_string())
                } else {
                    Ok("boot_into_new_kernel_once=no\n".to_string())
                }
            });
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CONFIG_HOME")
            .times(1)
            .returning(|_| Ok("/xdg".to_string()));

        assert_eq!(
            BootOnceState::Off,
            ReadinessService::probe_boot_once(&fs, &env, Path::new("/kernel"))
        );
    }

    #[test]
    fn probe_boot_once_unreadable_tree_file_does_not_fall_through() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/deploy.config"))
            .times(1)
            .returning(|path| path.ends_with(".kw/deploy.config"));
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/kernel/.kw/deploy.config"))
            .times(1)
            .returning(|_| {
                Err(FileSystemError::IoError(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "denied",
                )))
            });
        let env = MockEnvTrait::new();

        assert_eq!(
            BootOnceState::Unknown,
            ReadinessService::probe_boot_once(&fs, &env, Path::new("/kernel"))
        );
    }

    #[test]
    fn probe_boot_once_treats_empty_xdg_config_home_as_unset() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .withf(|path| {
                path == std::path::Path::new("/home/user/.config/kw/deploy.config")
                    || path == std::path::Path::new("/kernel/.kw/deploy.config")
            })
            .times(2)
            .returning(|path| path == Path::new("/home/user/.config/kw/deploy.config"));
        fs.expect_read_to_string()
            .withf(|path| path == std::path::Path::new("/home/user/.config/kw/deploy.config"))
            .times(1)
            .returning(|_| Ok("boot_into_new_kernel_once=no\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CONFIG_HOME")
            .times(1)
            .returning(|_| Ok(String::new()));
        env.expect_var()
            .withf(|key| key == "HOME")
            .times(1)
            .returning(|_| Ok("/home/user".to_string()));

        assert_eq!(
            BootOnceState::Off,
            ReadinessService::probe_boot_once(&fs, &env, Path::new("/kernel"))
        );
    }
}
