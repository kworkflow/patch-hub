use derive_getters::Getters;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io,
    path::Path,
};

use super::{cover_renderer::CoverRenderer, patch_renderer::PatchRenderer};

#[cfg(test)]
mod tests;

#[derive(Serialize, Deserialize, Getters)]
pub struct Config {
    #[getter(skip)]
    page_size: usize,
    patchsets_cache_dir: String,
    bookmarked_patchsets_path: String,
    mailing_lists_path: String,
    reviewed_patchsets_path: String,
    /// Logs directory
    logs_path: String,
    git_send_email_options: String,
    /// Base directory for all patch-hub cache
    cache_dir: String,
    /// Base directory for all patch-hub cache
    data_dir: String,
    /// Renderer to use for patch previews
    patch_renderer: PatchRenderer,
    /// Renderer to use for patchset covers
    cover_renderer: CoverRenderer,
    /// Maximum age of a log file in days
    max_log_age: usize,
    /// Flags to be use with `git am` command when applying patches
    git_am_options: String,
    #[getter(skip)]
    /// List of kernel trees
    kernel_trees: HashMap<String, String>,
    /// The current kernel tree being used
    current_tree: Option<String>,
    git_am_branch_prefix: String,
}

impl Config {
    fn default() -> Self {
        let cache_dir = format!("{}/.cache/patch_hub", env::var("HOME").unwrap());
        let data_dir = format!("{}/.local/share/patch_hub", env::var("HOME").unwrap());

        Config {
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
            git_am_options: "".to_string(),
            kernel_trees: HashMap::new(),
            current_tree: None,
            git_am_branch_prefix: "patchset-".to_string(),
        }
    }

    /// Loads the configuration for patch-hub from the config file.
    ///
    /// Returns `None` if the config file is not found or if it's not a valid JSON.
    fn load_file() -> Option<Config> {
        if let Ok(config_path) = env::var("PATCH_HUB_CONFIG_PATH") {
            if Path::new(&config_path).is_file() {
                let file_contents = fs::read_to_string(&config_path).unwrap_or(String::new());
                if let Ok(config) = serde_json::from_str(&file_contents) {
                    return Some(config);
                }
            }
        }

        let config_path = format!(
            "{}/.config/patch-hub/config.json",
            env::var("HOME").unwrap()
        );
        if Path::new(&config_path).is_file() {
            let file_contents = fs::read_to_string(&config_path).unwrap_or(String::new());
            if let Ok(config) = serde_json::from_str(&file_contents) {
                return Some(config);
            }
        }

        None
    }

    fn override_with_env_vars(&mut self) {
        if let Ok(page_size) = env::var("PATCH_HUB_PAGE_SIZE") {
            self.page_size = page_size.parse().unwrap();
        };

        if let Ok(cache_dir) = env::var("PATCH_HUB_CACHE_DIR") {
            self.set_cache_dir(cache_dir);
        };

        if let Ok(data_dir) = env::var("PATCH_HUB_DATA_DIR") {
            self.set_data_dir(data_dir);
        };

        if let Ok(git_send_email_options) = env::var("PATCH_HUB_GIT_SEND_EMAIL_OPTIONS") {
            self.git_send_email_options = git_send_email_options;
        };

        if let Ok(patch_renderer) = env::var("PATCH_HUB_PATCH_RENDERER") {
            self.patch_renderer = patch_renderer.into();
        };
    }

    pub fn build() -> Self {
        let mut config = Self::load_file().unwrap_or_else(|| {
            let config = Self::default();
            // TODO: Better handle this error
            let _ = config.save_patch_hub_config();
            config
        });

        config.override_with_env_vars();

        config
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    pub fn set_page_size(&mut self, page_size: usize) {
        self.page_size = page_size;
    }

    pub fn set_cache_dir(&mut self, cache_dir: String) {
        self.patchsets_cache_dir = format!("{cache_dir}/patchsets");
        self.cache_dir = cache_dir;
    }

    pub fn set_data_dir(&mut self, data_dir: String) {
        self.bookmarked_patchsets_path = format!("{data_dir}/bookmarked_patchsets.json");
        self.mailing_lists_path = format!("{data_dir}/mailing_lists.json");
        self.reviewed_patchsets_path = format!("{data_dir}/reviewed_patchsets.json");
        self.logs_path = format!("{data_dir}/logs");
        self.data_dir = data_dir;
    }

    pub fn set_git_send_email_option(&mut self, git_send_email_options: String) {
        self.git_send_email_options = git_send_email_options;
    }

    pub fn set_git_am_option(&mut self, git_am_options: String) {
        self.git_am_options = git_am_options;
    }

    #[allow(dead_code)]
    /// Returns the list of names of the registered kernel trees
    pub fn kernel_trees(&self) -> Vec<&String> {
        self.kernel_trees.keys().collect::<Vec<&String>>()
    }

    /// Returns the path of the kernel tree with the given name if it exists
    pub fn kernel_tree_path(&self, kernel_tree: &str) -> Option<&String> {
        self.kernel_trees.get(kernel_tree)
    }

    pub fn set_patch_renderer(&mut self, patch_renderer: PatchRenderer) {
        self.patch_renderer = patch_renderer;
    }

    pub fn set_cover_renderer(&mut self, cover_renderer: CoverRenderer) {
        self.cover_renderer = cover_renderer;
    }

    pub fn set_max_log_age(&mut self, max_log_age: usize) {
        self.max_log_age = max_log_age;
    }

    pub fn save_patch_hub_config(&self) -> io::Result<()> {
        let config_path = if let Ok(path) = env::var("PATCH_HUB_CONFIG_PATH") {
            path
        } else {
            format!(
                "{}/.config/patch-hub/config.json",
                env::var("HOME").unwrap()
            )
        };

        let config_path = Path::new(&config_path);
        // We need to assure that the parent dir of `config_path` exists
        if let Some(parent_dir) = Path::parent(config_path) {
            fs::create_dir_all(parent_dir)?;
        }

        let tmp_filename = format!("{}.tmp", config_path.display());
        {
            let tmp_file = File::create(&tmp_filename)?;
            serde_json::to_writer_pretty(tmp_file, self)?;
        }
        fs::rename(tmp_filename, config_path)?;
        Ok(())
    }

    /// Creates the needed directories if they don't exist.
    /// The directories are defined during the Config build.
    ///
    /// This function must be called as soon as the Config is built so no other function attempt to use an inexistent folder.
    pub fn create_dirs(&self) {
        let paths = vec![
            &self.cache_dir,
            &self.data_dir,
            &self.patchsets_cache_dir,
            &self.logs_path,
        ];

        for path in paths {
            if fs::metadata(path).is_err() {
                fs::create_dir_all(path).unwrap();
            }
        }
    }
}
