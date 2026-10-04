use crate::{config::KernelTree, kw::models::readiness::KwReadiness};

use crate::app::models::kw_ops::{KwOpsFocus, KwOpsState};

impl KwOpsState {
    pub fn new(
        patchset_title: String,
        message_id: String,
        kernel_tree_id: String,
        tree: KernelTree,
        readiness: KwReadiness,
    ) -> Self {
        let head_unreadable = readiness.current_branch.is_none();
        let branch = readiness.current_branch.clone().unwrap_or_default();
        Self {
            patchset_title,
            message_id,
            kernel_tree_id,
            tree,
            branch,
            extra_args: String::new(),
            focus: KwOpsFocus::Branch,
            editing: false,
            edit_buffer: String::new(),
            readiness,
            head_unreadable,
            log_tail: String::new(),
            cancel_requested: false,
            start_requested: false,
            boot_once_acknowledged: false,
            pending_deploy: None,
        }
    }

    /// Re-open KwOps for the same patchset and tree without dropping extras,
    /// an in-flight cancel or start, or a boot-once acknowledgement.
    ///
    /// Branch and `head_unreadable` stay as last edited. A HEAD change while
    /// away is not applied, so a stale detached-HEAD flag can survive.
    pub fn reenter(&mut self, patchset_title: String, tree: KernelTree, readiness: KwReadiness) {
        self.patchset_title = patchset_title;
        self.tree = tree;
        self.readiness = readiness;
        self.editing = false;
        self.edit_buffer.clear();
    }

    pub fn extra_arg_tokens(&self) -> Vec<String> {
        Self::split_extra_args(&self.extra_args)
    }

    /// Extra args shown in the command preview, including in-progress
    /// edits so the Command line tracks the Extra args field.
    pub fn extra_arg_tokens_for_preview(&self) -> Vec<String> {
        let raw = if self.editing && self.focus == KwOpsFocus::ExtraArgs {
            self.edit_buffer.as_str()
        } else {
            self.extra_args.as_str()
        };
        Self::split_extra_args(raw)
    }

    pub fn highlight_prev(&mut self) {
        self.focus = KwOpsFocus::Branch;
    }

    pub fn highlight_next(&mut self) {
        self.focus = KwOpsFocus::ExtraArgs;
    }

    pub fn begin_edit(&mut self) {
        self.edit_buffer = match self.focus {
            KwOpsFocus::Branch => self.branch.clone(),
            KwOpsFocus::ExtraArgs => self.extra_args.clone(),
        };
        self.editing = true;
    }

    pub fn commit_edit(&mut self) {
        match self.focus {
            KwOpsFocus::Branch => self.branch = self.edit_buffer.trim().to_string(),
            KwOpsFocus::ExtraArgs => self.extra_args = self.edit_buffer.clone(),
        }
        self.editing = false;
        self.edit_buffer.clear();
    }

    pub fn cancel_edit(&mut self) {
        self.editing = false;
        self.edit_buffer.clear();
    }

    pub fn backspace_edit(&mut self) {
        self.edit_buffer.pop();
    }

    pub fn append_edit(&mut self, ch: char) {
        self.edit_buffer.push(ch);
    }
}

impl KwOpsState {
    fn split_extra_args(raw: &str) -> Vec<String> {
        raw.split_whitespace().map(str::to_string).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::kw_ops::DeployStartKind;
    use crate::kw::models::readiness::{
        BootOnceState, DeployAloneRefusal, KwBinaryProbe, KwReadiness, KwVersionCheck,
        TreeReadiness,
    };
    use crate::kw::models::remote::RemoteRefusal;

    fn sample_tree() -> KernelTree {
        serde_json::from_value(serde_json::json!({
            "path": "/kernel",
            "branch": "main"
        }))
        .expect("kernel tree should deserialize")
    }

    fn readiness(branch: Option<&str>) -> KwReadiness {
        KwReadiness {
            kw_binary: KwBinaryProbe {
                available: true,
                version_line: Some("kw, version 0.10.0".to_string()),
                check: KwVersionCheck::Meets,
            },
            tree: TreeReadiness::Ready {
                arch: Some("x86_64".to_string()),
            },
            output_dir: None,
            deploy_alone: Err(DeployAloneRefusal::NoBuildRecord),
            current_branch: branch.map(str::to_string),
            deploy_remote: Err(RemoteRefusal::NoRemotesConfigured),
            boot_once: BootOnceState::Unknown,
        }
    }

    #[test]
    fn prefills_branch_from_readiness_not_from_tree_config() {
        let ops = KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            sample_tree(),
            readiness(Some("feature")),
        );
        assert_eq!("feature", ops.branch);
        assert!(!ops.head_unreadable);
        assert_eq!("main", ops.tree.branch());
    }

    #[test]
    fn detached_head_leaves_branch_blank() {
        let ops = KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            sample_tree(),
            readiness(None),
        );
        assert!(ops.branch.is_empty());
        assert!(ops.head_unreadable);
    }

    #[test]
    fn focus_and_edit_commit_the_active_field() {
        let mut ops = KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            sample_tree(),
            readiness(Some("main")),
        );
        ops.highlight_next();
        ops.begin_edit();
        ops.append_edit('-');
        ops.append_edit('j');
        ops.append_edit('8');
        ops.commit_edit();
        assert_eq!("-j8", ops.extra_args);
        assert_eq!(vec!["-j8"], ops.extra_arg_tokens());
        assert!(!ops.editing);

        ops.highlight_prev();
        ops.begin_edit();
        ops.append_edit('x');
        ops.cancel_edit();
        assert_eq!("main", ops.branch);
        assert!(!ops.editing);
    }

    #[test]
    fn reenter_keeps_extras_and_cancel_requested() {
        let mut ops = KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            sample_tree(),
            readiness(Some("main")),
        );
        ops.extra_args = "--verbose".to_string();
        ops.cancel_requested = true;
        ops.reenter(
            "new title".to_string(),
            sample_tree(),
            readiness(Some("feature")),
        );
        assert_eq!("--verbose", ops.extra_args);
        assert!(ops.cancel_requested);
        assert_eq!("new title", ops.patchset_title);
        assert_eq!("main", ops.branch);
    }

    #[test]
    fn reenter_keeps_boot_once_ack_and_pending_deploy() {
        let mut ops = KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            sample_tree(),
            readiness(Some("main")),
        );
        ops.boot_once_acknowledged = true;
        ops.pending_deploy = Some(DeployStartKind::BuildThenDeploy);
        ops.reenter(
            "new title".to_string(),
            sample_tree(),
            readiness(Some("feature")),
        );
        assert!(ops.boot_once_acknowledged);
        assert_eq!(Some(DeployStartKind::BuildThenDeploy), ops.pending_deploy);
    }

    #[test]
    fn extra_arg_preview_tokens_follow_the_edit_buffer() {
        let mut ops = KwOpsState::new(
            "title".to_string(),
            "mid".to_string(),
            "linux".to_string(),
            sample_tree(),
            readiness(Some("main")),
        );
        ops.extra_args = "--verbose".to_string();
        ops.highlight_next();
        ops.begin_edit();
        ops.append_edit(' ');
        ops.append_edit('-');
        ops.append_edit('j');
        ops.append_edit('8');
        assert_eq!(vec!["--verbose", "-j8"], ops.extra_arg_tokens_for_preview());
        assert_eq!(vec!["--verbose"], ops.extra_arg_tokens());
    }
}
