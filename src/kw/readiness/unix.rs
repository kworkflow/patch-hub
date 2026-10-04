use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::infrastructure::{
    env::EnvTrait,
    file_system::{FileSystemError, FileSystemTrait},
    shell::ShellTrait,
};
use crate::{
    config::KernelTree,
    kw::{
        history::{KwBuildRecord, KwHistoryStore},
        remote,
    },
};

use super::{
    BootOnceState, DeployAloneRefusal, KwReadiness, KwReadinessError, ReadinessService,
    TreeReadiness,
};

impl ReadinessService {
    /// Parses kw's `key=value` config format (`.kw/build.config`,
    /// `.kw/deploy.config`, ...), mirroring kw's own `parse_configuration`:
    /// blank lines and lines starting with `#` are skipped, everything from the
    /// last `#` on is stripped as a trailing comment, the key has all
    /// whitespace removed, and the value is trimmed. Lines without `=` are
    /// ignored, and a final line without a trailing newline still counts.
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
            let key: String = key.chars().filter(|c| !c.is_whitespace()).collect();
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

    /// Reads the literal `arch=` value from `<tree>/.kw/build.config`, the same
    /// value kw's image discovery globs under `arch/<arch>/boot/`. Returns
    /// `None` when the file or the key is absent or empty — kw's
    /// `${build_config[arch]:-...}` expansion treats empty as unset — meaning
    /// the caller falls back to globbing `arch/*/boot/`, a deliberate
    /// divergence from kw's merged-config fallback (see
    /// [`ReadinessService::find_newest_kernel_image`]).
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

    /// Resolves kw's active build output dir (`O=`) for `tree_path`, mirroring
    /// kw's env handling: the env name is the content of
    /// `<tree>/.kw/env.current` (trailing newlines stripped, like bash's
    /// `$(< ...)`), and the output dir is
    /// `{XDG_CACHE_HOME | ~/.cache}/kw/envs/<base64(tree path)>/<env name>`.
    /// The tree path is encoded like kw 0.10's `get_encoded_pwd` — standard
    /// base64 with padding, no wrapping — after trimming trailing slashes,
    /// since kw encodes `$PWD` after changing into the tree. The encoded path
    /// may contain `/` (standard alphabet), producing nested directories; kw
    /// has the same behavior.
    ///
    /// kw's launcher recomputes the cache dir unconditionally, so a
    /// user-exported `KW_CACHE_DIR` is intentionally ignored here too. The
    /// `KWORKFLOW` rename knob is not honored: it exists for kw development.
    ///
    /// Returns `Ok(None)` when no env is active. The resolved dir is not
    /// required to exist: kw/make create it on first build. An unreadable
    /// `env.current` or an unresolvable cache base (neither `XDG_CACHE_HOME`
    /// nor `HOME` set) is an error, since the env state is then unknown —
    /// kw's "active but unresolvable" case.
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

        let cache_base = match env.var("XDG_CACHE_HOME") {
            // bash's `:-` (and the XDG spec) treat a set-but-empty value as
            // unset; env::var would happily return it as Ok("").
            Ok(xdg) if !xdg.is_empty() => xdg,
            _ => format!("{}/.cache", env.var("HOME")?),
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

    /// Finds the newest kernel image under `<build_root>/arch/`. Candidate
    /// basenames must end with `Image` (the `-name '*Image'` in kw's
    /// `get_kernel_binary_name` is case-sensitive, so `Image.gz` and `image`
    /// are excluded) and the most recently modified one wins, with ties broken
    /// by descending path (kw's `sort -r | head -1`).
    ///
    /// With `arch`, only `arch/<arch>/boot/` is probed — exactly kw's behavior.
    /// Without `arch` this is a **deliberate divergence**, not a mirror: kw
    /// falls back to the merged kw-config `arch` (packaged default `x86_64`, a
    /// directory that does not exist in kernel trees, so `kw deploy` then fails
    /// with exit 125), and patch-hub does not read kw's global config layers.
    /// Globbing every `arch/*/boot/` gives a more useful readiness signal than
    /// probing a directory that is never there — at the cost of possibly
    /// reporting an image kw would not find. A green image probe with `arch=`
    /// unset is therefore not a guarantee kw deploy will locate one; setting
    /// `arch=` in `.kw/build.config` makes the two agree.
    ///
    /// Second deliberate deviation: kw's `find` recurses into boot/
    /// subdirectories, while this scans only the top level. Kernel images for
    /// every arch kw supports are produced directly in boot/ (subdirs like
    /// compressed/ or dts/ never hold `*Image` files), and find does not
    /// descend into symlinked dirs either, so the behaviors agree on real
    /// trees.
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

    /// Deploy-alone readiness gate: a deploy without a preceding build is only
    /// allowed when a successful build record exists for the tree and the
    /// lookup branch, written against the same tree path and kw env, and a
    /// kernel image is still discoverable.
    ///
    /// `record` is the lookup keyed by the deploy target branch. `latest` is
    /// the newest record for the tree across branches. When the target has
    /// no keyed record but another branch does, that is
    /// [`DeployAloneRefusal::HeadMismatch`], not "no build recorded".
    ///
    /// This is only the record-matching half of the gate — it says nothing
    /// about the tree's *current* state. [`ReadinessService::evaluate_readiness`] conjoins
    /// [`TreeReadiness`] into its `deploy_alone` verdict; prefer it over
    /// calling this directly.
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

    /// Runs all readiness probes for `tree` and composes them into a
    /// [`KwReadiness`] snapshot. `head_branch` is the tree's current branch —
    /// resolving it (via git) is the caller's job, keeping these probes pure.
    ///
    /// `for_branch`, when set, is the branch deploy-alone should be judged
    /// against (the branch typed on KwOps). `current_branch` still reports
    /// the real HEAD so the UI can show both.
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
        let config_home = match env.var("XDG_CONFIG_HOME") {
            Ok(xdg) if !xdg.is_empty() => xdg,
            _ => format!("{}/.config", env.var("HOME").ok()?),
        };
        Some(Path::new(&config_home).join("kw").join(filename))
    }
}
