//! Status snapshot projected out of the kw actor.
//!
//! `AppState` never owns job state; it polls (`GetStatus`) or watches
//! (`WatchStatus`) these snapshots and projects them into the view model.

// The actor that constructs/reads these is unix-only.
#![cfg_attr(not(unix), allow(dead_code))]

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KwJobKind {
    Build,
    Deploy,
    BuildThenDeploy,
}

/// Running phase of a job. `BuildThenDeploy` jobs pass through both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KwPhase {
    Building,
    Deploying,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KwJobStatus {
    Idle,
    Running {
        kind: KwJobKind,
        phase: KwPhase,
        kernel_tree_id: String,
        branch: String,
        log_path: PathBuf,
    },
    Succeeded {
        kind: KwJobKind,
        kernel_tree_id: String,
        branch: String,
        log_path: PathBuf,
    },
    Failed {
        kind: KwJobKind,
        phase: KwPhase,
        exit_code: Option<i32>,
        log_path: PathBuf,
    },
    Cancelled {
        kind: KwJobKind,
        phase: KwPhase,
        log_path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KwStatusSnapshot {
    pub job: KwJobStatus,
    /// Pre-job branch the last accepted job can restore to, if any.
    /// Session-only; `None` after a successful restore or when nothing
    /// was recorded.
    pub restore_branch: Option<String>,
}

impl KwStatusSnapshot {
    pub fn idle() -> Self {
        Self {
            job: KwJobStatus::Idle,
            restore_branch: None,
        }
    }

    /// Compact nav-bar copy for a job that is still running. Terminal
    /// success/failure is not shown globally — it belongs on KwOps.
    pub fn running_indicator(&self) -> Option<String> {
        match &self.job {
            KwJobStatus::Running { phase, branch, .. } => {
                let phase = match phase {
                    KwPhase::Building => "building",
                    KwPhase::Deploying => "deploying",
                };
                Some(format!("kw: {phase} {branch}"))
            }
            _ => None,
        }
    }
}

impl KwJobStatus {
    /// Log file for the current or last job, if the actor has opened one.
    pub fn log_path(&self) -> Option<&std::path::Path> {
        match self {
            Self::Idle => None,
            Self::Running { log_path, .. }
            | Self::Succeeded { log_path, .. }
            | Self::Failed { log_path, .. }
            | Self::Cancelled { log_path, .. } => Some(log_path),
        }
    }
}

/// Human-readable hint for a known `kw deploy` exit code.
/// Unknown codes return `None` so the UI can still show the raw number.
/// 68 can still surface with `--force`: force skips the prompt, not the
/// initramfs errors.
#[cfg_attr(not(unix), allow(dead_code))]
pub fn deploy_exit_hint(code: i32) -> Option<&'static str> {
    Some(match code {
        2 => "kernel image not found",
        22 => "invalid option or kernel name",
        68 => "initramfs generation reported errors",
        95 => "unsupported bootloader",
        101 => "SSH unreachable after setup",
        103 => "passwordless root SSH setup failed",
        124 | 125 => "deploy cancelled, no valid kernel image, or not a kernel root",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn running(branch: &str) -> KwStatusSnapshot {
        KwStatusSnapshot {
            job: KwJobStatus::Running {
                kind: KwJobKind::Build,
                phase: KwPhase::Building,
                kernel_tree_id: "mainline".to_string(),
                branch: branch.to_string(),
                log_path: PathBuf::from("/tmp/build.log"),
            },
            restore_branch: Some("master".to_string()),
        }
    }

    #[test]
    fn running_indicator_names_the_phase_and_branch() {
        assert_eq!(
            Some("kw: building patchset-x".to_string()),
            running("patchset-x").running_indicator()
        );
    }

    #[test]
    fn log_path_is_present_for_jobs_with_a_log() {
        assert_eq!(
            Some(PathBuf::from("/tmp/build.log").as_path()),
            running("patchset-x").job.log_path()
        );
        assert_eq!(None, KwJobStatus::Idle.log_path());
    }

    #[test]
    fn terminal_and_idle_status_have_no_global_indicator() {
        assert_eq!(None, KwStatusSnapshot::idle().running_indicator());
        assert_eq!(
            None,
            KwStatusSnapshot {
                job: KwJobStatus::Succeeded {
                    kind: KwJobKind::Build,
                    kernel_tree_id: "mainline".to_string(),
                    branch: "patchset-x".to_string(),
                    log_path: PathBuf::from("/tmp/build.log"),
                },
                restore_branch: Some("master".to_string()),
            }
            .running_indicator()
        );
        assert_eq!(
            None,
            KwStatusSnapshot {
                job: KwJobStatus::Cancelled {
                    kind: KwJobKind::Build,
                    phase: KwPhase::Building,
                    log_path: PathBuf::from("/tmp/build.log"),
                },
                restore_branch: Some("master".to_string()),
            }
            .running_indicator()
        );
    }

    #[test]
    fn deploy_exit_hint_names_the_known_deploy_codes() {
        let cases = [
            (2, "kernel image not found"),
            (22, "invalid option or kernel name"),
            (68, "initramfs generation reported errors"),
            (95, "unsupported bootloader"),
            (101, "SSH unreachable after setup"),
            (103, "passwordless root SSH setup failed"),
            (
                124,
                "deploy cancelled, no valid kernel image, or not a kernel root",
            ),
            (
                125,
                "deploy cancelled, no valid kernel image, or not a kernel root",
            ),
        ];
        for (code, hint) in cases {
            assert_eq!(Some(hint), deploy_exit_hint(code), "code {code}");
        }
    }

    #[test]
    fn deploy_exit_hint_leaves_unknown_codes_unnamed() {
        // Unknown codes have no hint.
        assert_eq!(None, deploy_exit_hint(30));
        assert_eq!(None, deploy_exit_hint(1));
        assert_eq!(None, deploy_exit_hint(0));
    }
}
