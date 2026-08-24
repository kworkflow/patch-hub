// The actor that reads these fields is unix-only.
#![cfg_attr(not(unix), allow(dead_code))]

use tokio::sync::{oneshot, watch};

use crate::{
    config::KernelTree,
    kw::{
        errors::{KwError, KwStartError},
        history::KwApplyRecord,
        readiness::KwReadiness,
        status::KwStatusSnapshot,
    },
};

/// Everything the actor needs to start a job. The tree context is resolved
/// by the caller from its config snapshot, keeping KwActor decoupled from
/// ConfigActor.
#[derive(Debug, Clone)]
pub struct StartRequest {
    pub kernel_tree_id: String,
    pub tree: KernelTree,
    /// Branch the job must run on; the actor switches the tree onto it
    /// before spawning and leaves HEAD there after the job.
    pub branch: String,
    /// Extra kw CLI tokens, already whitespace-split by the caller.
    /// Reserved options (`--alert`, `--save-log-to`) are stripped —
    /// patch-hub's own argv wins.
    pub extra_args: Vec<String>,
}

pub enum KwMessage {
    RecordApply {
        record: KwApplyRecord,
        reply: oneshot::Sender<Result<(), KwError>>,
    },
    StartBuild {
        request: StartRequest,
        reply: oneshot::Sender<Result<(), KwStartError>>,
    },
    StartDeploy {
        #[allow(dead_code)]
        request: StartRequest,
        reply: oneshot::Sender<Result<(), KwStartError>>,
    },
    StartBuildThenDeploy {
        #[allow(dead_code)]
        request: StartRequest,
        reply: oneshot::Sender<Result<(), KwStartError>>,
    },
    /// Acknowledges that kill was requested; the actual process death is
    /// observed via the status snapshot, not this reply.
    Cancel {
        reply: oneshot::Sender<Result<(), KwError>>,
    },
    GetStatus {
        reply: oneshot::Sender<KwStatusSnapshot>,
    },
    /// Called once by the AppActor when it attaches, not per frame: the
    /// returned receiver is its wake source for status changes.
    WatchStatus {
        reply: oneshot::Sender<watch::Receiver<KwStatusSnapshot>>,
    },
    GetReadiness {
        kernel_tree_id: String,
        tree: KernelTree,
        reply: oneshot::Sender<Result<KwReadiness, KwError>>,
    },
    RestorePreviousBranch {
        reply: oneshot::Sender<Result<(), KwError>>,
    },
    /// Unlike the other actors' bare `Shutdown`, this one replies — the
    /// `TerminalMessage::Shutdown` convention. Teardown must know the
    /// running job's process group was actually killed before the runtime
    /// is dropped; a fire-and-forget message leaves that to scheduler luck.
    Shutdown { reply: oneshot::Sender<()> },
}

impl KwMessage {
    pub fn name(&self) -> &'static str {
        match self {
            KwMessage::RecordApply { .. } => "RecordApply",
            KwMessage::StartBuild { .. } => "StartBuild",
            KwMessage::StartDeploy { .. } => "StartDeploy",
            KwMessage::StartBuildThenDeploy { .. } => "StartBuildThenDeploy",
            KwMessage::Cancel { .. } => "Cancel",
            KwMessage::GetStatus { .. } => "GetStatus",
            KwMessage::WatchStatus { .. } => "WatchStatus",
            KwMessage::GetReadiness { .. } => "GetReadiness",
            KwMessage::RestorePreviousBranch { .. } => "RestorePreviousBranch",
            KwMessage::Shutdown { .. } => "Shutdown",
        }
    }
}
