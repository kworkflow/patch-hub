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
    #[allow(dead_code)]
    Deploy,
    #[allow(dead_code)]
    BuildThenDeploy,
}

/// Running phase of a job. `BuildThenDeploy` jobs pass through both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KwPhase {
    Building,
    #[allow(dead_code)]
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
}
