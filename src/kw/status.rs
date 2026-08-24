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
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KwStatusSnapshot {
    pub job: KwJobStatus,
}

impl KwStatusSnapshot {
    pub fn idle() -> Self {
        Self {
            job: KwJobStatus::Idle,
        }
    }
}
