use crate::render_prefs::{CoverRenderer, PatchRenderer};

/// Raw strings from the edit-config form (one field per option; `None` means omit from form).
#[derive(Debug, Default, Clone)]
pub struct ConfigUpdateDraft {
    pub page_size: Option<String>,
    pub cache_dir: Option<String>,
    pub data_dir: Option<String>,
    pub git_send_email_option: Option<String>,
    pub git_am_option: Option<String>,
    pub patch_renderer: Option<String>,
    pub cover_renderer: Option<String>,
    pub max_log_age: Option<String>,
    pub stay_on_applied_branch: Option<String>,
}

/// Parsed and validated update ready to merge into [`crate::config::ConfigState`](super::state::ConfigState).
#[derive(Debug, Default, Clone)]
pub struct ValidatedConfigUpdate {
    pub page_size: Option<usize>,
    pub cache_dir: Option<String>,
    pub data_dir: Option<String>,
    pub git_send_email_option: Option<String>,
    pub git_am_option: Option<String>,
    pub patch_renderer: Option<PatchRenderer>,
    pub cover_renderer: Option<CoverRenderer>,
    pub max_log_age: Option<usize>,
    pub stay_on_applied_branch: Option<bool>,
}
