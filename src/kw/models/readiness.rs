use thiserror::Error;

use std::{fmt, path::PathBuf};

use crate::{
    infrastructure::{env::EnvError, file_system::FileSystemError},
    kw::models::remote::{KwRemote, RemoteRefusal},
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
    /// `arch` (see [`ReadinessService::find_newest_kernel_image`]).
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
    /// A kw env is active but the source tree still holds an in-tree
    /// `.config` or `include/config/`: kbuild refuses an `O=` build of an
    /// unclean source tree, so every build fails within seconds.
    InTreeBuildArtifacts,
}

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

impl fmt::Display for TreeReadiness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            TreeReadiness::InTreeBuildArtifacts => write!(
                f,
                "in-tree .config / include/config block kw env (O=) builds; run \
                 `make mrproper` in the tree (the env's O= is untouched)"
            ),
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
        "the last build was on branch '{recorded}', but the deploy target is '{current}'; \
         rebuild on the target branch before deploying"
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

/// Whether `.kw/deploy.config` (then the user-level copy) sets
/// `boot_into_new_kernel_once=no`.
///
/// kw only treats the literal value `no` as off (`src/deploy.sh`); any other
/// value, including a missing key, leaves the option on. [`Unknown`] is
/// therefore a confirm-to-proceed gate, same as [`On`]: patch-hub cannot
/// pass a CLI off-switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootOnceState {
    Off,
    On,
    Unknown,
}

/// Snapshot of tree, kw binary, and history probes used to decide whether
/// a job can start, and why not.
#[derive(Debug, Clone)]
pub struct KwReadiness {
    pub kw_binary: KwBinaryProbe,
    pub tree: TreeReadiness,
    /// Active kw env's `O=` dir, if any.
    pub output_dir: Option<PathBuf>,
    /// `Ok(())` is a self-sufficient verdict: tree readiness is already
    /// conjoined in, so a caller cannot forget to check `tree` as well.
    pub deploy_alone: Result<(), DeployAloneRefusal>,
    /// Currently checked-out branch. `None` when HEAD is detached or
    /// `git branch --show-current` could not be read.
    pub current_branch: Option<String>,
    /// Resolved `--remote` target, or why none could be chosen.
    pub deploy_remote: Result<KwRemote, RemoteRefusal>,
    pub boot_once: BootOnceState,
}
