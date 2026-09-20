use color_eyre::{eyre::bail, Report};
use derive_getters::Getters;

use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
    mem,
};

use crate::config::{ConfigSnapshot, ConfigUpdateDraft};

#[derive(Clone, Debug, Getters)]
pub struct EditConfigState {
    #[getter(skip)]
    config_buffer: HashMap<EditableConfig, String>,
    #[getter(skip)]
    tree_options: Vec<String>,
    highlighted: usize,
    is_editing: bool,
    curr_edit: String,
}

impl EditConfigState {
    pub fn new(config: &ConfigSnapshot) -> Self {
        let mut config_buffer = HashMap::new();
        config_buffer.insert(EditableConfig::PageSize, config.page_size().to_string());
        config_buffer.insert(EditableConfig::CacheDir, config.cache_dir().to_string());
        config_buffer.insert(EditableConfig::DataDir, config.data_dir().to_string());
        config_buffer.insert(
            EditableConfig::GitSendEmailOpt,
            config.git_send_email_options().to_string(),
        );
        config_buffer.insert(
            EditableConfig::GitAmOpt,
            config.git_am_options().to_string(),
        );
        config_buffer.insert(
            EditableConfig::PatchRenderer,
            config.patch_renderer().to_string(),
        );
        config_buffer.insert(
            EditableConfig::CoverRenderer,
            config.cover_renderer().to_string(),
        );
        config_buffer.insert(EditableConfig::MaxLogAge, config.max_log_age().to_string());
        config_buffer.insert(
            EditableConfig::StayOnAppliedBranch,
            config.stay_on_applied_branch().to_string(),
        );
        config_buffer.insert(
            EditableConfig::KwRebootAfterDeploy,
            config.kw_reboot_after_deploy().to_string(),
        );
        config_buffer.insert(
            EditableConfig::KwDeployForce,
            config.kw_deploy_force().to_string(),
        );
        config_buffer.insert(
            EditableConfig::TargetKernelTree,
            config.target_kernel_tree().clone().unwrap_or_default(),
        );

        let mut tree_options: Vec<String> = config.kernel_trees().into_iter().cloned().collect();
        tree_options.sort();

        EditConfigState {
            config_buffer,
            tree_options,
            highlighted: 0,
            is_editing: false,
            curr_edit: String::new(),
        }
    }

    /// Get the number of config entries in the config buffer
    pub fn config_count(&self) -> usize {
        self.config_buffer.len()
    }

    /// Get the config entry at the given index
    pub fn config(&self, i: usize) -> Option<(String, String)> {
        EditableConfig::try_from(i)
            .ok()
            .and_then(|editable_config| {
                self.config_buffer.get(&editable_config).map(|value| {
                    let display = if editable_config == EditableConfig::TargetKernelTree
                        && value.is_empty()
                    {
                        "<none>".to_string()
                    } else {
                        value.clone()
                    };
                    (editable_config.to_string(), display)
                })
            })
    }

    pub fn highlighted_is_tree_selector(&self) -> bool {
        matches!(
            EditableConfig::try_from(self.highlighted),
            Ok(EditableConfig::TargetKernelTree)
        )
    }

    /// Cycles the tree selector through `<none>` and the sorted tree keys.
    /// No-op unless the highlighted row is the tree selector and editing.
    pub fn cycle_edit(&mut self, forward: bool) {
        if !self.is_editing || !self.highlighted_is_tree_selector() {
            return;
        }

        let mut options = Vec::with_capacity(self.tree_options.len() + 1);
        options.push(String::new());
        options.extend(self.tree_options.iter().cloned());

        let current = options
            .iter()
            .position(|key| key == &self.curr_edit)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % options.len()
        } else if current == 0 {
            options.len() - 1
        } else {
            current - 1
        };
        self.curr_edit = options[next].clone();
    }

    /// Toggle editing mode
    pub fn toggle_editing(&mut self) {
        if !self.is_editing {
            if let Ok(editable_config) = EditableConfig::try_from(self.highlighted()) {
                if let Some(value) = self.config_buffer.get(&editable_config) {
                    self.curr_edit = value.clone();
                }
            }
        }
        self.is_editing = !self.is_editing;
    }

    /// Move the highlight to the previous entry
    pub fn highlight_prev(&mut self) {
        if self.highlighted > 0 {
            self.highlighted -= 1;
        }
    }

    /// Move the highlight to the next entry
    pub fn highlight_next(&mut self) {
        if self.highlighted + 1 < self.config_buffer.len() {
            self.highlighted += 1;
        }
    }

    /// Remove the last char from the current editing value if not empty
    pub fn backspace_edit(&mut self) {
        if self.highlighted_is_tree_selector() {
            return;
        }
        if !self.curr_edit.is_empty() {
            self.curr_edit.pop();
        }
    }

    /// Appends a new char to the current editing value
    pub fn append_edit(&mut self, ch: char) {
        if self.highlighted_is_tree_selector() {
            return;
        }
        self.curr_edit.push(ch);
    }

    /// Clear the current editing value
    pub fn clear_edit(&mut self) {
        self.curr_edit.clear();
    }

    /// Push the current edit value to the config buffer
    pub fn stage_edit(&mut self) {
        if let Ok(editable_config) = EditableConfig::try_from(self.highlighted) {
            self.config_buffer
                .insert(editable_config, mem::take(&mut self.curr_edit));
        }
    }

    /// Raw form values for config validation.
    pub fn to_update_draft(&self) -> ConfigUpdateDraft {
        ConfigUpdateDraft {
            page_size: self.config_buffer.get(&EditableConfig::PageSize).cloned(),
            cache_dir: self.config_buffer.get(&EditableConfig::CacheDir).cloned(),
            data_dir: self.config_buffer.get(&EditableConfig::DataDir).cloned(),
            git_send_email_option: self
                .config_buffer
                .get(&EditableConfig::GitSendEmailOpt)
                .cloned(),
            git_am_option: self.config_buffer.get(&EditableConfig::GitAmOpt).cloned(),
            patch_renderer: self
                .config_buffer
                .get(&EditableConfig::PatchRenderer)
                .cloned(),
            cover_renderer: self
                .config_buffer
                .get(&EditableConfig::CoverRenderer)
                .cloned(),
            max_log_age: self.config_buffer.get(&EditableConfig::MaxLogAge).cloned(),
            stay_on_applied_branch: self
                .config_buffer
                .get(&EditableConfig::StayOnAppliedBranch)
                .cloned(),
            kw_reboot_after_deploy: self
                .config_buffer
                .get(&EditableConfig::KwRebootAfterDeploy)
                .cloned(),
            kw_deploy_force: self
                .config_buffer
                .get(&EditableConfig::KwDeployForce)
                .cloned(),
            target_kernel_tree: self
                .config_buffer
                .get(&EditableConfig::TargetKernelTree)
                .cloned(),
        }
    }
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
enum EditableConfig {
    PageSize,
    CacheDir,
    DataDir,
    GitSendEmailOpt,
    GitAmOpt,
    PatchRenderer,
    CoverRenderer,
    MaxLogAge,
    StayOnAppliedBranch,
    KwRebootAfterDeploy,
    KwDeployForce,
    TargetKernelTree,
}

impl TryFrom<usize> for EditableConfig {
    type Error = Report;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(EditableConfig::PageSize),
            1 => Ok(EditableConfig::CacheDir),
            2 => Ok(EditableConfig::DataDir),
            3 => Ok(EditableConfig::GitSendEmailOpt),
            4 => Ok(EditableConfig::GitAmOpt),
            5 => Ok(EditableConfig::PatchRenderer),
            6 => Ok(EditableConfig::CoverRenderer),
            7 => Ok(EditableConfig::MaxLogAge),
            8 => Ok(EditableConfig::StayOnAppliedBranch),
            9 => Ok(EditableConfig::KwRebootAfterDeploy),
            10 => Ok(EditableConfig::KwDeployForce),
            11 => Ok(EditableConfig::TargetKernelTree),
            _ => bail!("Invalid index {} for EditableConfig", value), // Handle out of bounds
        }
    }
}

impl Display for EditableConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EditableConfig::PageSize => write!(f, "Page Size"),
            EditableConfig::CacheDir => write!(f, "Cache Directory"),
            EditableConfig::DataDir => write!(f, "Data Directory"),
            EditableConfig::PatchRenderer => {
                write!(f, "Patch Renderer (bat, delta, diff-so-fancy)")
            }
            EditableConfig::CoverRenderer => {
                write!(f, "Cover Renderer (bat)")
            }
            EditableConfig::GitSendEmailOpt => write!(f, "`git send email` option"),
            EditableConfig::MaxLogAge => write!(f, "Max Log Age (0 = forever)"),
            EditableConfig::GitAmOpt => write!(f, "`git am` option"),
            EditableConfig::StayOnAppliedBranch => {
                write!(f, "Stay On Applied Branch (true/false)")
            }
            EditableConfig::KwRebootAfterDeploy => {
                write!(f, "Reboot After kw Deploy (true/false)")
            }
            EditableConfig::KwDeployForce => {
                write!(f, "Force kw Deploy (true/false)")
            }
            EditableConfig::TargetKernelTree => {
                write!(f, "Target Kernel Tree (ENTER, then ←/→ to cycle)")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigState;

    fn snapshot_with_trees(keys: &[&str], target: Option<&str>) -> ConfigSnapshot {
        let mut state = ConfigState::default();
        for key in keys {
            state.kernel_trees.insert(
                (*key).to_string(),
                serde_json::from_value(serde_json::json!({
                    "path": format!("/{key}"),
                    "branch": "master"
                }))
                .unwrap(),
            );
        }
        state.target_kernel_tree = target.map(str::to_string);
        state.to_snapshot()
    }

    fn tree_row(edit: &mut EditConfigState) {
        while edit.highlighted() != 11 {
            edit.highlight_next();
        }
    }

    #[test]
    fn draft_includes_deploy_knobs_with_compiled_in_defaults() {
        let snapshot = ConfigState::default().to_snapshot();
        let edit = EditConfigState::new(&snapshot);
        let draft = edit.to_update_draft();

        assert_eq!(Some("false".to_string()), draft.kw_reboot_after_deploy);
        assert_eq!(Some("true".to_string()), draft.kw_deploy_force);
        assert_eq!(Some(String::new()), draft.target_kernel_tree);
        assert_eq!(12, edit.config_count());
        assert_eq!(
            Some((
                "Reboot After kw Deploy (true/false)".to_string(),
                "false".to_string()
            )),
            edit.config(9)
        );
        assert_eq!(
            Some((
                "Force kw Deploy (true/false)".to_string(),
                "true".to_string()
            )),
            edit.config(10)
        );
        assert_eq!(
            Some((
                "Target Kernel Tree (ENTER, then ←/→ to cycle)".to_string(),
                "<none>".to_string()
            )),
            edit.config(11)
        );
    }

    #[test]
    fn cycle_edit_wraps_across_none_and_sorted_keys() {
        let snapshot = snapshot_with_trees(&["zebra", "linux"], Some("linux"));
        let mut edit = EditConfigState::new(&snapshot);
        tree_row(&mut edit);
        edit.toggle_editing();
        assert_eq!("linux", edit.curr_edit());

        edit.cycle_edit(true);
        assert_eq!("zebra", edit.curr_edit());
        edit.cycle_edit(true);
        assert_eq!("", edit.curr_edit());
        edit.cycle_edit(true);
        assert_eq!("linux", edit.curr_edit());

        edit.cycle_edit(false);
        assert_eq!("", edit.curr_edit());
        edit.cycle_edit(false);
        assert_eq!("zebra", edit.curr_edit());
    }

    #[test]
    fn cycle_edit_is_noop_when_not_editing_or_on_other_rows() {
        let snapshot = snapshot_with_trees(&["linux"], Some("linux"));
        let mut edit = EditConfigState::new(&snapshot);
        tree_row(&mut edit);
        edit.cycle_edit(true);
        assert_eq!("", edit.curr_edit());
        assert_eq!(
            Some((
                "Target Kernel Tree (ENTER, then ←/→ to cycle)".to_string(),
                "linux".to_string()
            )),
            edit.config(11)
        );

        edit.highlighted = 0;
        edit.toggle_editing();
        let page_size = edit.curr_edit().to_string();
        edit.cycle_edit(true);
        assert_eq!(page_size, edit.curr_edit().as_str());
    }

    #[test]
    fn text_input_is_ignored_on_the_tree_selector() {
        let snapshot = snapshot_with_trees(&["linux"], Some("linux"));
        let mut edit = EditConfigState::new(&snapshot);
        tree_row(&mut edit);
        edit.toggle_editing();
        edit.append_edit('x');
        edit.backspace_edit();
        assert_eq!("linux", edit.curr_edit());
    }

    #[test]
    fn staged_tree_selection_reaches_the_draft() {
        let snapshot = snapshot_with_trees(&["linux", "zebra"], Some("linux"));
        let mut edit = EditConfigState::new(&snapshot);
        tree_row(&mut edit);
        edit.toggle_editing();
        edit.cycle_edit(true);
        edit.stage_edit();
        edit.toggle_editing();
        assert_eq!(
            Some("zebra".to_string()),
            edit.to_update_draft().target_kernel_tree
        );

        tree_row(&mut edit);
        edit.toggle_editing();
        edit.cycle_edit(true);
        edit.stage_edit();
        edit.toggle_editing();
        assert_eq!(
            Some(String::new()),
            edit.to_update_draft().target_kernel_tree
        );
        assert_eq!(
            Some((
                "Target Kernel Tree (ENTER, then ←/→ to cycle)".to_string(),
                "<none>".to_string()
            )),
            edit.config(11)
        );
    }
}
