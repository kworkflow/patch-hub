use crate::{config::KernelTree, kw::readiness::KwReadiness};

/// Which editable KwOps field is focused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KwOpsFocus {
    #[default]
    Branch,
    ExtraArgs,
}

/// Build-path form on the KwOps screen. Job status lives in [`crate::app::state::KwUiState::status`].
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
}

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
        }
    }

    pub fn extra_arg_tokens(&self) -> Vec<String> {
        self.extra_args
            .split_whitespace()
            .map(str::to_string)
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kw::readiness::{
        DeployAloneRefusal, KwBinaryProbe, KwReadiness, KwVersionCheck, TreeReadiness,
    };

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
            kernel_image: None,
            build_record: None,
            latest_build: None,
            deploy_alone: Err(DeployAloneRefusal::NoBuildRecord),
            current_branch: branch.map(str::to_string),
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
}
