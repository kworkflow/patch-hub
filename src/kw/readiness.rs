//! Readiness probes for running `kw build` / `kw deploy` on a configured
//! kernel tree.
//!
//! Each probe mirrors the corresponding discovery logic in kw itself
//! (`src/lib/kwlib.sh`, `src/lib/kw_config_loader.sh`, `src/deploy.sh` at
//! kw 0.10) so patch-hub's idea of "ready" matches what kw will actually
//! do, instead of being a parallel interpretation that can silently drift
//! from it. The probes are pure functions over injected infrastructure
//! traits; KwActor composes them into the `GetReadiness` snapshot.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use thiserror::Error;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::infrastructure::{
    env::{EnvError, EnvTrait},
    file_system::{FileSystemError, FileSystemTrait},
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
    /// `arch=` value from `.kw/build.config`; `None` means image discovery
    /// must glob `arch/*/boot/`, the fallback kw's own `arch=` resolution
    /// effectively produces when the key is unset.
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
/// then keeps the `.config` there instead of in the tree root.
// No production caller until the readiness aggregation lands; kept per the
// CachePolicy precedent (src/lore/application/cache.rs).
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
/// the caller should fall back to globbing `arch/*/boot/`.
pub fn read_build_arch(fs: &dyn FileSystemTrait, tree_path: &Path) -> Option<String> {
    let content = fs
        .read_to_string(&tree_path.join(".kw").join("build.config"))
        .ok()?;
    let arch = parse_kw_config(&content).remove("arch")?;
    if arch.is_empty() { None } else { Some(arch) }
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
// No production caller until the readiness aggregation lands; kept per the
// CachePolicy precedent (src/lore/application/cache.rs).
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
        Ok(xdg) => xdg,
        Err(_) => format!("{}/.cache", env.var("HOME")?),
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

/// Finds the newest kernel image under `<build_root>/arch/`, mirroring kw's
/// `get_kernel_binary_name`: a candidate's basename must end with `Image`
/// (find's `-name '*Image'` is case-sensitive, so `Image.gz` and `image`
/// are excluded) and the most recently modified one wins, with ties broken
/// by descending path (kw's `sort -r | head -1`). With `arch`, only
/// `arch/<arch>/boot/` is probed; without, every `arch/*/boot/` is globbed.
///
/// Deliberate deviation: kw's `find` recurses into boot/ subdirectories,
/// while this scans only the top level. Kernel images for every arch kw
/// supports are produced directly in boot/ (subdirs like compressed/ or
/// dts/ never hold `*Image` files), and find does not descend into symlinked
/// dirs either, so the behaviors agree on real trees.
// No production caller until the readiness aggregation lands; kept per the
// CachePolicy precedent (src/lore/application/cache.rs).
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
    };

    use super::*;

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
        fs::write(
            commented.path().join(".kw/build.config"),
            "#arch=riscv\n",
        )
        .unwrap();
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

        let resolved =
            resolve_output_dir(&fs, &env, Path::new("/home/user/linux")).unwrap();

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

        let resolved =
            resolve_output_dir(&fs, &env, Path::new("/home/user/linux/")).unwrap();

        assert_eq!(
            Some(PathBuf::from("/xdg/kw/envs/L2hvbWUvdXNlci9saW51eA==/minix")),
            resolved
        );
    }

    #[test]
    fn resolve_output_dir_empty_env_file_is_inactive() {
        let mut fs = MockFileSystemTrait::new();
        fs.expect_is_file().returning(|_| true);
        fs.expect_read_to_string().returning(|_| Ok("\n".to_string()));
        let env = MockEnvTrait::new();

        assert_eq!(None, resolve_output_dir(&fs, &env, Path::new("/kernel")).unwrap());
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
        assert_eq!(None, find_newest_kernel_image(&OsFileSystem, dir.path(), None));
        assert_eq!(
            None,
            find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );

        let boot = dir.path().join("arch/x86/boot");
        fs::create_dir_all(&boot).unwrap();
        write_file_with_mtime(&boot.join("image"), 100); // lowercase: no match
        write_file_with_mtime(&boot.join("Image.gz"), 200); // suffix: no match

        assert_eq!(None, find_newest_kernel_image(&OsFileSystem, dir.path(), None));
        assert_eq!(
            None,
            find_newest_kernel_image(&OsFileSystem, dir.path(), Some("x86"))
        );
    }
}
