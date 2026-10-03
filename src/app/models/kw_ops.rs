use crate::{config::KernelTree, kw::readiness::KwReadiness};

/// Which editable KwOps field is focused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KwOpsFocus {
    #[default]
    Branch,
    ExtraArgs,
}

/// Which deploy start was interrupted by the boot-once confirm popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeployStartKind {
    Deploy,
    BuildThenDeploy,
}

/// Form state on the KwOps screen. Job status lives in [`crate::app::state::KwUiState::status`].
#[derive(Clone, Debug)]
pub struct KwOpsState {
    pub patchset_title: String,
    pub message_id: String,
    pub kernel_tree_id: String,
    pub tree: KernelTree,
    pub branch: String,
    pub extra_args: String,
    pub focus: KwOpsFocus,
    pub editing: bool,
    pub edit_buffer: String,
    pub readiness: KwReadiness,
    /// True when readiness could not name HEAD; Start stays disabled until
    /// the user types a branch (we never guess from `KernelTree.branch`).
    pub head_unreadable: bool,
    /// Bounded tail of the job log, refreshed by AppActor while KwOps is
    /// visible and a job is running.
    pub log_tail: String,
    pub cancel_requested: bool,
    /// Optimistic lock so a second Start before the watch snapshot
    /// arrives is ignored instead of refused with an error popup.
    pub start_requested: bool,
    /// True after the user confirmed boot-into-new-kernel-once for this
    /// KwOps visit. Preserved across `reenter` so a later Start does not
    /// re-prompt in the same session.
    pub boot_once_acknowledged: bool,
    /// Deploy start waiting on the boot-once confirm popup.
    pub pending_deploy: Option<DeployStartKind>,
}
