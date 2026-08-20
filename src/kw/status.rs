//! Status snapshot projected out of the kw actor.
//!
//! `AppState` never owns job state; it polls (`GetStatus`) or watches
//! (`WatchStatus`) these snapshots and projects them into the view model.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KwJobKind {
    Build,
    // Constructed once deploy execution lands; kept per the CachePolicy
    // precedent (src/lore/application/cache.rs).
    #[allow(dead_code)]
    Deploy,
    #[allow(dead_code)]
    BuildThenDeploy,
}

/// Running phase of a job. `BuildThenDeploy` jobs pass through both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KwPhase {
    Building,
    // Constructed once deploy execution lands (CachePolicy precedent).
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
