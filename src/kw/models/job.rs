use std::{path::PathBuf, process::ExitStatus};

use tokio::sync::oneshot;

use crate::{
    infrastructure::process::ProcessError,
    kw::{
        messages::DeployOptions,
        models::remote::KwRemote,
        status::{KwJobKind, KwPhase},
    },
};

/// How a job's process ended, as observed by the detached task that owns
/// the process handle.
pub(crate) enum JobOutcome {
    Exited(ExitStatus),
    WaitFailed(ProcessError),
    Cancelled,
}

/// Internal report from a job task back to the actor loop. Kept off the
/// public [`KwMessage`] protocol: no caller can fake a completion.
pub(crate) enum JobEvent {
    Finished(JobOutcome),
}

/// Where RestorePreviousBranch switches back to: the branch HEAD was on
/// when the last job was accepted, and the tree that branch lives in.
/// Session-only, deliberately not persisted.
pub(crate) struct RestoreContext {
    pub(crate) tree_path: String,
    pub(crate) branch: String,
}

/// What the actor remembers about the running job while the detached task
/// owns the process itself (see [`run_job`]).
pub(crate) struct JobState {
    pub(crate) kind: KwJobKind,
    pub(crate) phase: KwPhase,
    pub(crate) kernel_tree_id: String,
    pub(crate) branch: String,
    /// Tree path, kw-env output dir, and build arch as probed at accept
    /// time. The build runs under these, so the completion record
    /// describes this snapshot — not whatever the tree's configuration
    /// says by the time the job ends.
    pub(crate) tree_path: String,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) arch: Option<String>,
    pub(crate) log_path: PathBuf,
    /// Present for deploy kinds so a BuildThenDeploy job can spawn the
    /// deploy process at the build/deploy boundary without the original
    /// StartRequest.
    pub(crate) deploy: Option<JobDeploy>,
    /// `None` once a cancel has been requested; a second `Cancel` is an
    /// idempotent ack.
    pub(crate) cancel_tx: Option<oneshot::Sender<()>>,
}

/// Deploy argv inputs snapshotted at accept. The remote and options are
/// resolved before the job starts so a chain cannot silently retarget
/// mid-build.
#[derive(Clone)]
pub(crate) struct JobDeploy {
    pub(crate) remote: KwRemote,
    pub(crate) options: DeployOptions,
    pub(crate) extra_args: Vec<String>,
    /// Release of the kernel being deployed, from the build record, so
    /// the deploy log can be checked for a GRUB menu that never listed
    /// it. `None` until a BuildThenDeploy build has been recorded.
    pub(crate) kernelrelease: Option<String>,
}
