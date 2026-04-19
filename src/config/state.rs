use derive_getters::Getters;
use patch_hub_proc_macros::serde_individual_default;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use crate::config::update::ValidatedConfigUpdate;
use crate::infrastructure::env::EnvTrait;
use crate::render_prefs::{CoverRenderer, PatchRenderer};

#[derive(Debug, Serialize, serde::Deserialize, Getters, Eq, PartialEq, Clone)]
pub struct KernelTree {
    path: String,
    branch: String,
}

/// Canonical persisted configuration (on-disk JSON for patch-hub).
#[derive(Serialize, Getters)]
#[serde_individual_default]
pub struct ConfigState {
    #[getter(skip)]
    pub(crate) page_size: usize,
    pub(crate) patchsets_cache_dir: String,
    pub(crate) bookmarked_patchsets_path: String,
    pub(crate) mailing_lists_path: String,
    pub(crate) reviewed_patchsets_path: String,
    pub(crate) logs_path: String,
    pub(crate) git_send_email_options: String,
    pub(crate) cache_dir: String,
    pub(crate) data_dir: String,
    pub(crate) patch_renderer: PatchRenderer,
    pub(crate) cover_renderer: CoverRenderer,
    pub(crate) max_log_age: usize,
    #[getter(skip)]
    pub(crate) kernel_trees: HashMap<String, KernelTree>,
    pub(crate) target_kernel_tree: Option<String>,
    pub(crate) git_am_options: String,
    pub(crate) git_am_branch_prefix: String,
}

impl Default for ConfigState {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| {
            eprintln!("$HOME environment variable not set, using current directory");
            ".".to_string()
        });
        Self::defaults_from_home(&home)
    }
}

impl ConfigState {
    pub fn new_with_defaults(env: &dyn EnvTrait) -> Self {
        let home = env.var("HOME").unwrap_or_else(|_| {
            eprintln!("$HOME environment variable not set, using current directory");
            ".".to_string()
        });
        Self::defaults_from_home(&home)
    }

    fn defaults_from_home(home: &str) -> Self {
        let cache_dir = format!("{home}/.cache/patch_hub");
        let data_dir = format!("{home}/.local/share/patch_hub");
        ConfigState {
            page_size: 30,
            patchsets_cache_dir: format!("{cache_dir}/patchsets"),
            bookmarked_patchsets_path: format!("{data_dir}/bookmarked_patchsets.json"),
            mailing_lists_path: format!("{data_dir}/mailing_lists.json"),
            reviewed_patchsets_path: format!("{data_dir}/reviewed_patchsets.json"),
            logs_path: format!("{data_dir}/logs"),
            git_send_email_options: "--dry-run --suppress-cc=all".to_string(),
            patch_renderer: Default::default(),
            cover_renderer: Default::default(),
            cache_dir,
            data_dir,
            max_log_age: 30,
            kernel_trees: HashMap::new(),
            target_kernel_tree: None,
            git_am_options: String::new(),
            git_am_branch_prefix: String::from("patchset-"),
        }
    }

    #[allow(dead_code)]
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    fn set_page_size(&mut self, page_size: usize) {
        self.page_size = page_size;
    }

    fn set_cache_dir(&mut self, cache_dir: String) {
        self.patchsets_cache_dir = format!("{cache_dir}/patchsets");
        self.cache_dir = cache_dir;
    }

    fn set_data_dir(&mut self, data_dir: String) {
        self.bookmarked_patchsets_path = format!("{data_dir}/bookmarked_patchsets.json");
        self.mailing_lists_path = format!("{data_dir}/mailing_lists.json");
        self.reviewed_patchsets_path = format!("{data_dir}/reviewed_patchsets.json");
        self.logs_path = format!("{data_dir}/logs");
        self.data_dir = data_dir;
    }

    fn set_git_send_email_option(&mut self, git_send_email_options: String) {
        self.git_send_email_options = git_send_email_options;
    }

    fn set_git_am_option(&mut self, git_am_options: String) {
        self.git_am_options = git_am_options;
    }

    fn set_patch_renderer(&mut self, patch_renderer: PatchRenderer) {
        self.patch_renderer = patch_renderer;
    }

    fn set_cover_renderer(&mut self, cover_renderer: CoverRenderer) {
        self.cover_renderer = cover_renderer;
    }

    fn set_max_log_age(&mut self, max_log_age: usize) {
        self.max_log_age = max_log_age;
    }

    /// Merges validated field updates from the edit-config flow.
    pub fn apply_update(&mut self, u: &ValidatedConfigUpdate) {
        if let Some(page_size) = u.page_size {
            self.set_page_size(page_size);
        }
        if let Some(ref cache_dir) = u.cache_dir {
            self.set_cache_dir(cache_dir.clone());
        }
        if let Some(ref data_dir) = u.data_dir {
            self.set_data_dir(data_dir.clone());
        }
        if let Some(ref git_send_email_options) = u.git_send_email_option {
            self.set_git_send_email_option(git_send_email_options.clone());
        }
        if let Some(ref git_am_options) = u.git_am_option {
            self.set_git_am_option(git_am_options.clone());
        }
        if let Some(patch_renderer) = u.patch_renderer {
            self.set_patch_renderer(patch_renderer);
        }
        if let Some(cover_renderer) = u.cover_renderer {
            self.set_cover_renderer(cover_renderer);
        }
        if let Some(max_log_age) = u.max_log_age {
            self.set_max_log_age(max_log_age);
        }
    }

    pub fn to_snapshot(&self) -> ConfigSnapshot {
        ConfigSnapshot::from_state(self)
    }
}

/// Recomputes derived path fields from `cache_dir` and `data_dir`.
pub fn normalize_derived_paths(state: &mut ConfigState) {
    let cache_dir = state.cache_dir.clone();
    state.patchsets_cache_dir = format!("{cache_dir}/patchsets");
    let data_dir = state.data_dir.clone();
    state.bookmarked_patchsets_path = format!("{data_dir}/bookmarked_patchsets.json");
    state.mailing_lists_path = format!("{data_dir}/mailing_lists.json");
    state.reviewed_patchsets_path = format!("{data_dir}/reviewed_patchsets.json");
    state.logs_path = format!("{data_dir}/logs");
}

/// Immutable view of configuration for the rest of the application (read-only).
#[derive(Debug, Serialize, Clone, Getters)]
pub struct ConfigSnapshot {
    #[getter(skip)]
    page_size: usize,
    patchsets_cache_dir: String,
    bookmarked_patchsets_path: String,
    mailing_lists_path: String,
    reviewed_patchsets_path: String,
    logs_path: String,
    git_send_email_options: String,
    cache_dir: String,
    data_dir: String,
    patch_renderer: PatchRenderer,
    cover_renderer: CoverRenderer,
    max_log_age: usize,
    #[getter(skip)]
    kernel_trees: HashMap<String, KernelTree>,
    target_kernel_tree: Option<String>,
    git_am_options: String,
    git_am_branch_prefix: String,
}

impl ConfigSnapshot {
    pub(crate) fn from_state(s: &ConfigState) -> Self {
        Self {
            page_size: s.page_size,
            patchsets_cache_dir: s.patchsets_cache_dir.clone(),
            bookmarked_patchsets_path: s.bookmarked_patchsets_path.clone(),
            mailing_lists_path: s.mailing_lists_path.clone(),
            reviewed_patchsets_path: s.reviewed_patchsets_path.clone(),
            logs_path: s.logs_path.clone(),
            git_send_email_options: s.git_send_email_options.clone(),
            cache_dir: s.cache_dir.clone(),
            data_dir: s.data_dir.clone(),
            patch_renderer: s.patch_renderer,
            cover_renderer: s.cover_renderer,
            max_log_age: s.max_log_age,
            kernel_trees: s.kernel_trees.clone(),
            target_kernel_tree: s.target_kernel_tree.clone(),
            git_am_options: s.git_am_options.clone(),
            git_am_branch_prefix: s.git_am_branch_prefix.clone(),
        }
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    #[allow(dead_code)]
    pub fn kernel_trees(&self) -> HashSet<&String> {
        self.kernel_trees.keys().collect::<HashSet<&String>>()
    }

    #[allow(dead_code)]
    pub fn get_kernel_tree(&self, kernel_tree_id: &str) -> Option<&KernelTree> {
        self.kernel_trees.get(kernel_tree_id)
    }
}
