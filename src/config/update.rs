/// Parsed edits from the edit-config screen, ready to merge into config state.
///
/// Each field is independent: only `Some` values are applied by [`ConfigState::apply_update`](crate::config::ConfigState::apply_update).
#[derive(Debug, Default, Clone)]
pub struct ConfigUpdateDraft {
    pub page_size: Option<usize>,
    pub cache_dir: Option<String>,
    pub data_dir: Option<String>,
    pub git_send_email_option: Option<String>,
    pub git_am_option: Option<String>,
    pub patch_renderer: Option<String>,
    pub cover_renderer: Option<String>,
    pub max_log_age: Option<usize>,
}
