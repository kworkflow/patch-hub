//! Readiness probes for running `kw build` / `kw deploy` on a configured
//! kernel tree.
//!
//! Each probe mirrors the corresponding discovery logic in kw itself
//! (`src/lib/kwlib.sh`, `src/lib/kw_config_loader.sh`, `src/deploy.sh` at
//! kw 0.10) so patch-hub's idea of "ready" matches what kw will actually
//! do, instead of being a parallel interpretation that can silently drift
//! from it. The probes are pure functions over injected infrastructure
//! traits; KwActor composes them into the `GetReadiness` snapshot.

use std::{collections::HashMap, path::Path};

use crate::infrastructure::file_system::FileSystemTrait;

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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::infrastructure::file_system::OsFileSystem;

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
}
