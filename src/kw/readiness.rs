//! Readiness probes for running `kw build` / `kw deploy` on a configured
//! kernel tree.
//!
//! Most probes mirror the corresponding discovery logic in kw itself
//! (`src/lib/kwlib.sh`, `src/lib/kw_config_loader.sh`, `src/deploy.sh` at
//! kw 0.10) so patch-hub's idea of "ready" matches what kw will actually
//! do, instead of being a parallel interpretation that can silently drift
//! from it. The deliberate divergences — the `arch`-unset glob fallback
//! and the non-recursive boot-dir scan — are documented on
//! [`find_newest_kernel_image`]. The probes are pure functions over
//! injected infrastructure traits; KwActor composes them into the
//! `GetReadiness` snapshot.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use thiserror::Error;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::infrastructure::{
    env::{EnvError, EnvTrait},
    file_system::{FileSystemError, FileSystemTrait},
    shell::{ShellCommand, ShellTrait},
};
use crate::{
    config::KernelTree,
    kw::history::{KwBuildRecord, KwHistoryStore},
};

/// Errors from readiness probes for states where "absent" is not a normal
/// situation (unlike a missing `.kw` dir, which is a readiness verdict).
#[derive(Debug, Error)]
pub enum KwReadinessError {
    #[error("filesystem error: {0}")]
    Fs(#[from] FileSystemError),
    #[error("cannot resolve the kw cache dir: {0}")]
    Env(#[from] EnvError),
}

/// Readiness of a configured kernel tree for kw operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeReadiness {
    /// Kernel root, kw-initialized, with a `.config`. `arch` is the literal
    /// `arch=` value from `.kw/build.config`; `None` means the key is unset
    /// and image discovery will glob `arch/*/boot/` instead — a deliberate
    /// divergence from kw, whose own fallback is the merged kw-config
    /// `arch` (see [`find_newest_kernel_image`]).
    Ready { arch: Option<String> },
    /// The configured path is not a directory.
    Missing,
    /// Directory exists but lacks the files and directories kw's
    /// `is_kernel_root` expects at a kernel tree root.
    NotAKernelRoot,
    /// No `.kw/` directory: `kw init` was never run in this tree.
    MissingKwDir,
    /// No `.config` at the build root (the tree itself, or the kw env's
    /// output dir when one is active).
    MissingKernelConfig,
}

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
#[allow(dead_code)]
pub fn probe_tree(
    fs: &dyn FileSystemTrait,
    tree_path: &Path,
    output_dir: Option<&Path>,
) -> TreeReadiness {
    if !fs.is_dir(tree_path) {
        return TreeReadiness::Missing;
    }
    if !is_kernel_root(fs, tree_path) {
        return TreeReadiness::NotAKernelRoot;
    }
    if !fs.is_dir(&tree_path.join(".kw")) {
        return TreeReadiness::MissingKwDir;
    }
    let build_root = output_dir.unwrap_or(tree_path);
    if !fs.is_file(&build_root.join(".config")) {
        return TreeReadiness::MissingKernelConfig;
    }
    TreeReadiness::Ready {
        arch: read_build_arch(fs, tree_path),
    }
}

/// Reads the literal `arch=` value from `<tree>/.kw/build.config`, the same
/// value kw's image discovery globs under `arch/<arch>/boot/`. Returns
/// `None` when the file or the key is absent or empty — kw's
/// `${build_config[arch]:-...}` expansion treats empty as unset — meaning
/// the caller falls back to globbing `arch/*/boot/`, a deliberate
/// divergence from kw's merged-config fallback (see
/// [`find_newest_kernel_image`]).
pub fn read_build_arch(fs: &dyn FileSystemTrait, tree_path: &Path) -> Option<String> {
    let content = fs
        .read_to_string(&tree_path.join(".kw").join("build.config"))
        .ok()?;
    let arch = parse_kw_config(&content).remove("arch")?;
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
/// The tree path is encoded exactly like kw's `get_encoded_pwd` — standard
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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub fn find_newest_kernel_image(
    fs: &dyn FileSystemTrait,
    build_root: &Path,
    arch: Option<&str>,
) -> Option<PathBuf> {
    match arch {
        Some(arch) => newest_image_in(fs, &build_root.join("arch").join(arch).join("boot")),
        None => fs
            .read_dir(&build_root.join("arch"))
            .ok()?
            .into_iter()
            .filter(|entry| fs.is_dir(entry))
            .filter_map(|arch_dir| {
                newest_image_in(fs, &arch_dir.join("boot"))
                    .map(|image| (image_mtime(fs, &image), image))
            })
            .max_by_key(|(mtime, _)| *mtime)
            .map(|(_, image)| image),
    }
}

/// Newest `*Image` file directly inside `boot_dir`, if any.
fn newest_image_in(fs: &dyn FileSystemTrait, boot_dir: &Path) -> Option<PathBuf> {
    fs.read_dir(boot_dir)
        .ok()?
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with("Image"))
                && fs.is_file(entry)
        })
        .map(|entry| (image_mtime(fs, &entry), entry))
        .max_by_key(|(mtime, _)| *mtime)
        .map(|(_, entry)| entry)
}

fn image_mtime(fs: &dyn FileSystemTrait, path: &Path) -> SystemTime {
    fs.metadata(path)
        .and_then(|meta| meta.modified().map_err(FileSystemError::from))
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Reads the built kernel's release string from
/// `<build_root>/include/config/kernel.release`, the file a kernel build
/// generates — cheaper than re-running `make kernelrelease`, and `None`
/// when the build never produced one (or produced an empty one).
// Read by KwActor when writing build records (the build step); kept per
// the CachePolicy precedent (src/lore/application/cache.rs).
#[allow(dead_code)]
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

/// Minimum kw version this integration is verified against.
pub const KW_MIN_VERSION: (u32, u32) = (0, 10);

/// Result of comparing the version kw reports against [`KW_MIN_VERSION`].
///
/// Advisory only: kw's shipped VERSION file is stale (it reads `beta-0.9`
/// even at the 0.10 tag), so `Below` can fire on a genuinely recent kw and
/// must never gate functionality — the raw line is carried verbatim so the
/// UI can show exactly what kw reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KwVersionCheck {
    Meets,
    Below(String),
    /// The version output could not be obtained or parsed.
    Unknown,
}

/// Probe of the kw binary on `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KwBinaryProbe {
    pub available: bool,
    /// First line of `kw --version` output, verbatim.
    pub version_line: Option<String>,
    pub check: KwVersionCheck,
}

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
        Some(line) => check_kw_version(line),
        None => KwVersionCheck::Unknown,
    };
    KwBinaryProbe {
        available: true,
        version_line,
        check,
    }
}

fn check_kw_version(version_line: &str) -> KwVersionCheck {
    match parse_kw_version(version_line) {
        Some(version) if version >= KW_MIN_VERSION => KwVersionCheck::Meets,
        Some(_) => KwVersionCheck::Below(version_line.to_string()),
        None => KwVersionCheck::Unknown,
    }
}

/// Extracts the first `X.Y[.Z]` pair from a version line: `0.10.0` and the
/// stale `beta-0.9` kw currently ships both parse.
fn parse_kw_version(line: &str) -> Option<(u32, u32)> {
    let start = line.find(|c: char| c.is_ascii_digit())?;
    let mut parts = line[start..].splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor: String = parts
        .next()?
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    Some((major, minor.parse().ok()?))
}

impl std::fmt::Display for TreeReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeReadiness::Ready { .. } => write!(f, "ready"),
            TreeReadiness::Missing => write!(f, "the configured path is not a directory"),
            TreeReadiness::NotAKernelRoot => write!(
                f,
                "the directory is not a kernel tree root (missing files like \
                 Makefile or dirs like arch/)"
            ),
            TreeReadiness::MissingKwDir => {
                write!(f, "kw init was never run in this tree (no .kw/ directory)")
            }
            TreeReadiness::MissingKernelConfig => {
                write!(
                    f,
                    "no .config at the build root (the tree, or the kw env's O=)"
                )
            }
        }
    }
}

/// Why a deploy-without-build was refused. Each variant's message is the
/// actionable explanation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DeployAloneRefusal {
    #[error("the kernel tree is not ready: {0}")]
    TreeNotReady(TreeReadiness),
    #[error("no build recorded for this tree and branch; run a build first")]
    NoBuildRecord,
    #[error("the last build of this branch failed; rebuild before deploying")]
    LastBuildFailed,
    #[error(
        "the last build was on branch '{recorded}', but HEAD is '{current}'; \
         rebuild on the current branch before deploying"
    )]
    HeadMismatch { recorded: String, current: String },
    #[error(
        "the kernel tree moved from '{recorded}' to '{current}' since the last build; \
         rebuild before deploying"
    )]
    TreePathDrift { recorded: String, current: String },
    #[error(
        "the last build ran with a different kw env (O=) than the active one; \
         rebuild in the active env before deploying"
    )]
    OutputDirMismatch,
    #[error(
        "no kernel image (*Image) found under arch/*/boot; rebuild before deploying \
         (with no arch= in .kw/build.config, kw probes arch/x86_64/boot/, which \
         does not exist in kernel trees — set arch= explicitly)"
    )]
    ImageMissing,
}

/// Deploy-alone readiness gate: a deploy without a preceding build is only
/// allowed when a successful build record exists for the tree and current
/// HEAD, written against the same tree path and kw env, and a kernel image
/// is still discoverable.
///
/// This is only the record-matching half of the gate — it says nothing
/// about the tree's *current* state. [`evaluate_readiness`] conjoins
/// [`TreeReadiness`] into its `deploy_alone` verdict; prefer it over
/// calling this directly.
#[allow(dead_code)]
pub fn check_deploy_alone(
    record: Option<&KwBuildRecord>,
    tree: &KernelTree,
    head_branch: &str,
    output_dir: Option<&Path>,
    image: Option<&Path>,
) -> Result<(), DeployAloneRefusal> {
    let record = record.ok_or(DeployAloneRefusal::NoBuildRecord)?;
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

/// Snapshot of tree, kw binary, and history probes used to decide whether
/// a job can start, and why not.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct KwReadiness {
    pub kw_binary: KwBinaryProbe,
    pub tree: TreeReadiness,
    /// Active kw env's `O=` dir, if any.
    pub output_dir: Option<PathBuf>,
    /// Newest discoverable kernel image under the build root, if any.
    pub kernel_image: Option<PathBuf>,
    /// Build record for `(kernel_tree_id, head_branch)`, if any.
    pub build_record: Option<KwBuildRecord>,
    /// Newest build record for the tree across branches, even when HEAD
    /// has none.
    pub latest_build: Option<KwBuildRecord>,
    /// `Ok(())` is a self-sufficient verdict: tree readiness is already
    /// conjoined in, so a caller cannot forget to check `tree` as well.
    pub deploy_alone: Result<(), DeployAloneRefusal>,
}

/// Runs all readiness probes for `tree` and composes them into a
/// [`KwReadiness`] snapshot. `head_branch` is the tree's current branch —
/// resolving it (via git) is the caller's job, keeping these probes pure.
#[allow(dead_code)]
pub fn evaluate_readiness(
    fs: &dyn FileSystemTrait,
    env: &dyn EnvTrait,
    shell: &dyn ShellTrait,
    history: &dyn KwHistoryStore,
    kernel_tree_id: &str,
    tree: &KernelTree,
    head_branch: &str,
) -> Result<KwReadiness, KwReadinessError> {
    let tree_path = Path::new(tree.path());
    let kw_binary = probe_kw_binary(env, shell);
    let output_dir = resolve_output_dir(fs, env, tree_path)?;
    let tree_status = probe_tree(fs, tree_path, output_dir.as_deref());
    let arch = match &tree_status {
        TreeReadiness::Ready { arch } => arch.clone(),
        _ => None,
    };
    let kernel_image = find_newest_kernel_image(
        fs,
        output_dir.as_deref().unwrap_or(tree_path),
        arch.as_deref(),
    );
    let (build_record, latest_build) = history.build_records(kernel_tree_id, head_branch)?;
    // The tree's current state is part of the verdict: a stale image and a
    // matching record must not green-light a deploy on a tree that has
    // since lost its .config, .kw/, or kernel-root files.
    let deploy_alone = match &tree_status {
        TreeReadiness::Ready { .. } => check_deploy_alone(
            build_record.as_ref(),
            tree,
            head_branch,
            output_dir.as_deref(),
            kernel_image.as_deref(),
        ),
        other => Err(DeployAloneRefusal::TreeNotReady(other.clone())),
    };
    Ok(KwReadiness {
        kw_binary,
        tree: tree_status,
        output_dir,
        kernel_image,
        build_record,
        latest_build,
        deploy_alone,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, SystemTime},
    };

    use crate::infrastructure::{
        env::MockEnvTrait,
        file_system::{MockFileSystemTrait, OsFileSystem},
        shell::{MockShellTrait, ShellOutput},
    };
    use crate::kw::history::FileKwHistoryStore;

    use super::*;

    use std::sync::Arc;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(test_name: &str) -> Self {
            let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "patch-hub-kw-readiness-{}-{}-{}",
                test_name,
                std::process::id(),
                n
            ));
            // A leftover from a failed previous run must not poison this one.
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
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
            fs::write(dir.join(file), "").unwrap();
        }
        for sub in KERNEL_ROOT_DIRS {
            fs::create_dir(dir.join(sub)).unwrap();
        }
    }

    /// A kernel root with `.kw/` and an in-tree `.config`: the fully ready
    /// fixture most probes start from.
    fn make_ready_tree(test_name: &str) -> TempDir {
        let dir = TempDir::new(test_name);
        make_kernel_root(dir.path());
        fs::create_dir(dir.path().join(".kw")).unwrap();
        fs::write(dir.path().join(".config"), "").unwrap();
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
        let parsed = parse_kw_config(content);

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

        assert!(is_kernel_root(&OsFileSystem, dir.path()));
    }

    #[test]
    fn is_kernel_root_rejects_any_missing_member() {
        for member in KERNEL_ROOT_FILES.into_iter().chain(KERNEL_ROOT_DIRS) {
            let dir = TempDir::new("kernel-root-incomplete");
            make_kernel_root(dir.path());
            fs::remove_dir_all(dir.path().join(member))
                .or_else(|_| fs::remove_file(dir.path().join(member)))
                .unwrap();

            assert!(
                !is_kernel_root(&OsFileSystem, dir.path()),
                "missing {member} should fail the check"
            );
        }
    }

    #[test]
    fn probe_tree_reports_missing_path() {
        let dir = TempDir::new("probe-missing");

        assert_eq!(
            TreeReadiness::Missing,
            probe_tree(&OsFileSystem, &dir.path().join("nope"), None)
        );
    }

    #[test]
    fn probe_tree_reports_non_kernel_root() {
        let dir = TempDir::new("probe-not-root");

        assert_eq!(
            TreeReadiness::NotAKernelRoot,
            probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_reports_missing_kw_dir() {
        let dir = TempDir::new("probe-no-kw");
        make_kernel_root(dir.path());

        assert_eq!(
            TreeReadiness::MissingKwDir,
            probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_reports_missing_kernel_config() {
        let dir = TempDir::new("probe-no-config");
        make_kernel_root(dir.path());
        fs::create_dir(dir.path().join(".kw")).unwrap();

        assert_eq!(
            TreeReadiness::MissingKernelConfig,
            probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_ready_without_build_config_means_glob_fallback() {
        let dir = make_ready_tree("probe-ready");

        assert_eq!(
            TreeReadiness::Ready { arch: None },
            probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_ready_reads_arch_from_build_config() {
        let dir = make_ready_tree("probe-ready-arch");
        fs::write(dir.path().join(".kw/build.config"), "arch=arm64\n").unwrap();

        assert_eq!(
            TreeReadiness::Ready {
                arch: Some("arm64".to_string())
            },
            probe_tree(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn probe_tree_with_active_env_checks_config_in_output_dir() {
        let dir = make_ready_tree("probe-env");
        let out = TempDir::new("probe-env-output");
        // With a kw env active, kw moves the .config into the env's O= dir.
        fs::remove_file(dir.path().join(".config")).unwrap();

        assert_eq!(
            TreeReadiness::MissingKernelConfig,
            probe_tree(&OsFileSystem, dir.path(), Some(out.path()))
        );

        fs::write(out.path().join(".config"), "").unwrap();
        assert!(matches!(
            probe_tree(&OsFileSystem, dir.path(), Some(out.path())),
            TreeReadiness::Ready { .. }
        ));
    }

    #[test]
    fn read_build_arch_reads_literal_value() {
        let dir = make_ready_tree("arch-literal");
        fs::write(dir.path().join(".kw/build.config"), "arch=x86\n").unwrap();

        assert_eq!(
            Some("x86".to_string()),
            read_build_arch(&OsFileSystem, dir.path())
        );
    }

    #[test]
    fn read_build_arch_unset_means_glob_fallback() {
        // Missing file, missing key, commented key, and empty value all map
        // to kw's "unset" semantics.
        let no_file = make_ready_tree("arch-no-file");
        assert_eq!(None, read_build_arch(&OsFileSystem, no_file.path()));

        let no_key = make_ready_tree("arch-no-key");
        fs::write(
            no_key.path().join(".kw/build.config"),
            "cpu_scaling_factor=100\n",
        )
        .unwrap();
        assert_eq!(None, read_build_arch(&OsFileSystem, no_key.path()));

        let commented = make_ready_tree("arch-commented");
        fs::write(commented.path().join(".kw/build.config"), "#arch=riscv\n").unwrap();
        assert_eq!(None, read_build_arch(&OsFileSystem, commented.path()));

        let empty = make_ready_tree("arch-empty");
        fs::write(empty.path().join(".kw/build.config"), "arch=\n").unwrap();
        assert_eq!(None, read_build_arch(&OsFileSystem, empty.path()));
    }

    /// Creates a file whose mtime is `mtime_secs` seconds after the epoch,
    /// so newest-wins ordering is fully deterministic.
    fn write_file_with_mtime(path: &Path, mtime_secs: u64) {
        let file = fs::File::create(path).unwrap();
        file.set_times(
            fs::FileTimes::new()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(mtime_secs)),
        )
        .unwrap();
    }

    #[test]
    fn resolve_output_dir_without_env_file_is_inactive() {
        let dir = make_ready_tree("env-inactive");
        let env = MockEnvTrait::new();

        assert_eq!(
            None,
            resolve_output_dir(&OsFileSystem, &env, dir.path()).unwrap()
        );
    }

    #[test]
    fn resolve_output_dir_encodes_tree_path_like_kw() {
        // Known vector: `printf '%s' /home/user/linux | base64 --wrap=0`.
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file()
            .returning(|p| p == Path::new("/home/user/linux/.kw/env.current"));
        fs.expect_read_to_string()
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CACHE_HOME")
            .returning(|_| Ok("/xdg".to_string()));

        let resolved = resolve_output_dir(&fs, &env, Path::new("/home/user/linux")).unwrap();

        assert_eq!(
            Some(PathBuf::from("/xdg/kw/envs/L2hvbWUvdXNlci9saW51eA==/minix")),
            resolved
        );
    }

    #[test]
    fn resolve_output_dir_falls_back_to_home_cache() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file().returning(|_| true);
        fs.expect_read_to_string()
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CACHE_HOME")
            .returning(|_| Err(std::env::VarError::NotPresent.into()));
        env.expect_var()
            .withf(|key| key == "HOME")
            .returning(|_| Ok("/home/user".to_string()));

        let resolved = resolve_output_dir(&fs, &env, Path::new("/kernel")).unwrap();

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
        fs.expect_is_file().returning(|_| true);
        fs.expect_read_to_string()
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CACHE_HOME")
            .returning(|_| Ok(String::new()));
        env.expect_var()
            .withf(|key| key == "HOME")
            .returning(|_| Ok("/home/user".to_string()));

        let resolved = resolve_output_dir(&fs, &env, Path::new("/kernel")).unwrap();

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
        fs.expect_is_file().returning(|_| true);
        fs.expect_read_to_string()
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .withf(|key| key == "XDG_CACHE_HOME")
            .returning(|_| Ok("/xdg".to_string()));

        let resolved = resolve_output_dir(&fs, &env, Path::new("/home/user/linux/")).unwrap();

        assert_eq!(
            Some(PathBuf::from("/xdg/kw/envs/L2hvbWUvdXNlci9saW51eA==/minix")),
            resolved
        );
    }

    #[test]
    fn resolve_output_dir_empty_env_file_is_inactive() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file().returning(|_| true);
        fs.expect_read_to_string()
            .returning(|_| Ok("\n".to_string()));
        let env = MockEnvTrait::new();

        assert_eq!(
            None,
            resolve_output_dir(&fs, &env, Path::new("/kernel")).unwrap()
        );
    }

    #[test]
    fn resolve_output_dir_unreadable_env_file_errors() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file().returning(|_| true);
        fs.expect_read_to_string().returning(|_| {
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied").into())
        });
        let env = MockEnvTrait::new();

        assert!(resolve_output_dir(&fs, &env, Path::new("/kernel")).is_err());
    }

    #[test]
    fn resolve_output_dir_errors_without_any_cache_base() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file().returning(|_| true);
        fs.expect_read_to_string()
            .returning(|_| Ok("minix\n".to_string()));
        let mut env = MockEnvTrait::new();
        env.expect_var()
            .returning(|_| Err(std::env::VarError::NotPresent.into()));

        assert!(resolve_output_dir(&fs, &env, Path::new("/kernel")).is_err());
    }

    #[test]
    fn find_image_with_arch_picks_newest_image_only() {
        let dir = make_ready_tree("image-arch");
        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).unwrap();
        write_file_with_mtime(&boot.join("bzImage"), 100);
        write_file_with_mtime(&boot.join("Image"), 200);
        // Neither name matches find's case-sensitive `*Image`.
        write_file_with_mtime(&boot.join("vmlinux"), 300);
        write_file_with_mtime(&boot.join("Image.gz"), 400);

        assert_eq!(
            Some(boot.join("Image")),
            find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );
    }

    #[test]
    fn find_image_without_arch_globs_every_boot_dir() {
        // The no-false-negative regression: with no arch= hint, an image
        // under any arch/*/boot/ must still be found.
        let dir = make_ready_tree("image-glob");
        let x86_boot = dir.path().join("arch/x86/boot");
        let arm64_boot = dir.path().join("arch/arm64/boot");
        fs::create_dir_all(&x86_boot).unwrap();
        fs::create_dir_all(&arm64_boot).unwrap();
        write_file_with_mtime(&x86_boot.join("bzImage"), 100);
        write_file_with_mtime(&arm64_boot.join("Image"), 200);

        assert_eq!(
            Some(arm64_boot.join("Image")),
            find_newest_kernel_image(&OsFileSystem, dir.path(), None)
        );
    }

    #[test]
    fn find_image_tie_breaks_by_path_descending_like_kw() {
        // Same mtime: kw's `sort -r | head -1` on `%T+ %p` picks the
        // lexicographically larger path.
        let dir = make_ready_tree("image-tie");
        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).unwrap();
        write_file_with_mtime(&boot.join("bzImage"), 100);
        write_file_with_mtime(&boot.join("zImage"), 100);

        assert_eq!(
            Some(boot.join("zImage")),
            find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );
    }

    #[test]
    fn find_image_missing_or_empty_dirs_return_none() {
        let dir = make_ready_tree("image-none");
        // No image anywhere yet: the fixture has an empty arch/ dir.
        assert_eq!(
            None,
            find_newest_kernel_image(&OsFileSystem, dir.path(), None)
        );
        assert_eq!(
            None,
            find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );

        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).unwrap();
        write_file_with_mtime(&boot.join("image"), 100); // lowercase: no match
        write_file_with_mtime(&boot.join("Image.gz"), 200); // suffix: no match

        assert_eq!(
            None,
            find_newest_kernel_image(&OsFileSystem, dir.path(), None)
        );
        assert_eq!(
            None,
            find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );
    }

    #[test]
    fn kernelrelease_reads_and_trims_the_release_file() {
        let dir = make_ready_tree("kernelrelease");
        let config_dir = dir.path().join("include").join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("kernel.release"), "6.17.0-rc1\n").unwrap();

        assert_eq!(
            Some("6.17.0-rc1".to_string()),
            read_kernelrelease(&OsFileSystem, dir.path())
        );
    }

    #[test]
    fn kernelrelease_is_none_without_a_release_file() {
        let dir = make_ready_tree("kernelrelease-missing");

        assert_eq!(None, read_kernelrelease(&OsFileSystem, dir.path()));

        let config_dir = dir.path().join("include").join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("kernel.release"), "\n").unwrap();
        assert_eq!(None, read_kernelrelease(&OsFileSystem, dir.path()));
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
            "path": path.to_str().unwrap(),
            "branch": "master"
        }))
        .unwrap()
    }

    fn built_record(tree: &Path, branch: &str) -> KwBuildRecord {
        KwBuildRecord {
            kernel_tree_id: "mainline".to_string(),
            tree_path: tree.to_str().unwrap().to_string(),
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
            .returning(|_| false);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().times(0);

        let probe = probe_kw_binary(&env, &shell);

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
            env.expect_which().returning(|_| true);
            let mut shell = MockShellTrait::new();
            let stdout_bytes = stdout.as_bytes().to_vec();
            shell
                .expect_execute()
                .withf(|cmd| cmd.program == "kw" && cmd.args == ["--version"])
                .returning(move |_| Ok(shell_output(str::from_utf8(&stdout_bytes).unwrap(), true)));

            let probe = probe_kw_binary(&env, &shell);

            assert!(probe.available, "for version output {stdout:?}");
            assert_eq!(expected, probe.check, "for version output {stdout:?}");
        }
    }

    #[test]
    fn kw_probe_keeps_the_raw_first_line_verbatim() {
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().returning(|_| {
            Ok(shell_output(
                "beta-0.9\nBranch: master\nCommit: 3575d38\n",
                true,
            ))
        });

        let probe = probe_kw_binary(&env, &shell);

        assert_eq!(Some("beta-0.9".to_string()), probe.version_line);
    }

    #[test]
    fn kw_probe_unknown_when_version_is_unreadable() {
        // kw is on PATH but its --version output cannot be trusted.
        let mut spawn_fails = MockShellTrait::new();
        spawn_fails
            .expect_execute()
            .returning(|_| Err(std::io::Error::other("spawn failed").into()));

        let mut empty_stdout = MockShellTrait::new();
        empty_stdout
            .expect_execute()
            .returning(|_| Ok(shell_output("", true)));

        let mut kw_fails = MockShellTrait::new();
        kw_fails
            .expect_execute()
            .returning(|_| Ok(shell_output("0.10.0\n", false)));

        for shell in [spawn_fails, empty_stdout, kw_fails] {
            let mut env = MockEnvTrait::new();
            env.expect_which().returning(|_| true);

            let probe = probe_kw_binary(&env, &shell);

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
            check_deploy_alone(None, &tree, "patchset-x", None, Some(&image))
        );

        let mut failed = built_record(dir.path(), "patchset-x");
        failed.success = false;
        assert_eq!(
            Err(DeployAloneRefusal::LastBuildFailed),
            check_deploy_alone(Some(&failed), &tree, "patchset-x", None, Some(&image))
        );
    }

    #[test]
    fn deploy_alone_refuses_frankenstein_combinations() {
        let dir = make_ready_tree("deploy-frankenstein");
        let tree = kernel_tree(dir.path());
        let image = dir.path().join("arch/x86/boot/bzImage");
        let record = built_record(dir.path(), "patchset-x");

        // HEAD moved to another branch since the build.
        assert_eq!(
            Err(DeployAloneRefusal::HeadMismatch {
                recorded: "patchset-x".to_string(),
                current: "master".to_string(),
            }),
            check_deploy_alone(Some(&record), &tree, "master", None, Some(&image))
        );

        // The config repointed the same tree id at another path.
        let moved_tree = kernel_tree(Path::new("/elsewhere/linux"));
        assert_eq!(
            Err(DeployAloneRefusal::TreePathDrift {
                recorded: dir.path().to_str().unwrap().to_string(),
                current: "/elsewhere/linux".to_string(),
            }),
            check_deploy_alone(Some(&record), &moved_tree, "patchset-x", None, Some(&image))
        );

        // The active kw env changed since the build.
        assert_eq!(
            Err(DeployAloneRefusal::OutputDirMismatch),
            check_deploy_alone(
                Some(&record),
                &tree,
                "patchset-x",
                Some(Path::new("/cache/kw/envs/xyz/minix")),
                Some(&image),
            )
        );

        // The image the build produced is gone.
        assert_eq!(
            Err(DeployAloneRefusal::ImageMissing),
            check_deploy_alone(Some(&record), &tree, "patchset-x", None, None)
        );

        assert_eq!(
            Ok(()),
            check_deploy_alone(Some(&record), &tree, "patchset-x", None, Some(&image))
        );

        // A trailing-slash-only difference is the same tree, not drift.
        let mut slashed = built_record(dir.path(), "patchset-x");
        slashed.tree_path = format!("{}/", dir.path().to_str().unwrap());
        assert_eq!(
            Ok(()),
            check_deploy_alone(Some(&slashed), &tree, "patchset-x", None, Some(&image))
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
            check_deploy_alone(Some(&failed), &tree, "master", None, None)
        );
    }

    #[test]
    fn evaluate_readiness_composes_all_probes() {
        let dir = make_ready_tree("evaluate");
        fs::write(dir.path().join(".kw/build.config"), "arch=x86\n").unwrap();
        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).unwrap();
        write_file_with_mtime(&boot.join("bzImage"), 100);

        let data = TempDir::new("evaluate-data");
        let history = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            data.path().to_str().unwrap().to_string(),
        );
        let record = built_record(dir.path(), "patchset-x");
        history.record_build(record.clone()).unwrap();

        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        let mut shell = MockShellTrait::new();
        shell
            .expect_execute()
            .returning(|_| Ok(shell_output("0.10.0\n", true)));

        let tree = kernel_tree(dir.path());
        let readiness = evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "patchset-x",
        )
        .unwrap();

        assert_eq!(
            TreeReadiness::Ready {
                arch: Some("x86".to_string())
            },
            readiness.tree
        );
        assert_eq!(Some(boot.join("bzImage")), readiness.kernel_image);
        assert_eq!(Some(record.clone()), readiness.build_record);
        assert_eq!(Some(record), readiness.latest_build);
        assert_eq!(Ok(()), readiness.deploy_alone);
        assert_eq!(None, readiness.output_dir);
        assert!(readiness.kw_binary.available);
        assert_eq!(KwVersionCheck::Meets, readiness.kw_binary.check);
    }

    #[test]
    fn evaluate_readiness_missing_tree_short_circuits() {
        let dir = TempDir::new("evaluate-missing");
        let missing = dir.path().join("nope");
        let data = TempDir::new("evaluate-missing-data");
        let history = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            data.path().to_str().unwrap().to_string(),
        );

        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| false);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().times(0);

        let tree = kernel_tree(&missing);
        let readiness = evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "patchset-x",
        )
        .unwrap();

        assert_eq!(TreeReadiness::Missing, readiness.tree);
        assert_eq!(None, readiness.kernel_image);
        assert_eq!(None, readiness.build_record);
        assert_eq!(None, readiness.latest_build);
        // Tree readiness is conjoined into the deploy-alone verdict, so
        // Ok(()) can never describe a tree that is not build-ready.
        assert_eq!(
            Err(DeployAloneRefusal::TreeNotReady(TreeReadiness::Missing)),
            readiness.deploy_alone
        );
    }

    #[test]
    fn evaluate_readiness_without_build_refuses_deploy_alone() {
        let dir = make_ready_tree("evaluate-nobuild");
        let data = TempDir::new("evaluate-nobuild-data");
        let history = FileKwHistoryStore::new(
            Arc::new(OsFileSystem),
            data.path().to_str().unwrap().to_string(),
        );

        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| false);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().times(0);

        let tree = kernel_tree(dir.path());
        let readiness = evaluate_readiness(
            &OsFileSystem,
            &env,
            &shell,
            &history,
            "mainline",
            &tree,
            "patchset-x",
        )
        .unwrap();

        // No build.config: arch stays None (glob fallback); no images exist.
        assert_eq!(TreeReadiness::Ready { arch: None }, readiness.tree);
        assert_eq!(None, readiness.kernel_image);
        assert_eq!(None, readiness.build_record);
        assert_eq!(None, readiness.latest_build);
        assert_eq!(
            Err(DeployAloneRefusal::NoBuildRecord),
            readiness.deploy_alone
        );
        assert!(!readiness.kw_binary.available);
    }
}
