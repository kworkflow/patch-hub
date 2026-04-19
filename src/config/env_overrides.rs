use crate::config::state::ConfigState;
use crate::infrastructure::env::EnvTrait;

/// Applies `PATCH_HUB_*` overrides (same semantics as legacy `Config::override_with_env_vars`).
pub fn apply_env_overrides(state: &mut ConfigState, env: &dyn EnvTrait) {
    if let Ok(page_size) = env.var("PATCH_HUB_PAGE_SIZE") {
        state.page_size = page_size.parse().unwrap();
    }

    if let Ok(cache_dir) = env.var("PATCH_HUB_CACHE_DIR") {
        state.patchsets_cache_dir = format!("{cache_dir}/patchsets");
        state.cache_dir = cache_dir;
    }

    if let Ok(data_dir) = env.var("PATCH_HUB_DATA_DIR") {
        state.bookmarked_patchsets_path = format!("{data_dir}/bookmarked_patchsets.json");
        state.mailing_lists_path = format!("{data_dir}/mailing_lists.json");
        state.reviewed_patchsets_path = format!("{data_dir}/reviewed_patchsets.json");
        state.logs_path = format!("{data_dir}/logs");
        state.data_dir = data_dir;
    }

    if let Ok(git_send_email_options) = env.var("PATCH_HUB_GIT_SEND_EMAIL_OPTIONS") {
        state.git_send_email_options = git_send_email_options;
    }

    if let Ok(patch_renderer) = env.var("PATCH_HUB_PATCH_RENDERER") {
        state.patch_renderer = patch_renderer.into();
    }
}
