use derive_getters::Getters;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File},
    io,
    path::Path,
};

// store only one Config::default object in memory
lazy_static! {
    static ref DEFAULT_CONFIG: Config = Config::default();
}

pub const DEFAULT_CONFIG_PATH_SUFFIX: &str = ".config/patch-hub/config.json";

use super::{cover_renderer::CoverRenderer, patch_renderer::PatchRenderer};

#[cfg(test)]
mod tests;

#[derive(Serialize, Deserialize, Getters)]
pub struct Config {
    #[getter(skip)]
    #[serde(default = "default_page_size")]
    page_size: usize,
    #[serde(default = "default_patchsets_cache_dir")]
    patchsets_cache_dir: String,
    #[serde(default = "default_bookmarked_patchsets_path")]
    bookmarked_patchsets_path: String,
    #[serde(default = "default_mailing_lists_path")]
    mailing_lists_path: String,
    #[serde(default = "default_reviewed_patchsets_path")]
    reviewed_patchsets_path: String,
    /// Logs directory
    #[serde(default = "default_logs_path")]
    logs_path: String,
    #[serde(default = "default_git_send_email_options")]
    git_send_email_options: String,
    /// Base directory for all patch-hub cache
    #[serde(default = "default_cache_dir")]
    cache_dir: String,
    /// Base directory for all patch-hub cache
    #[serde(default = "default_data_dir")]
    data_dir: String,
    /// Renderer to use for patch previews
    #[serde(default = "default_patch_renderer")]
    patch_renderer: PatchRenderer,
    /// Renderer to use for patchset covers
    #[serde(default = "default_cover_renderer")]
    cover_renderer: CoverRenderer,
    /// Maximum age of a log file in days
    #[serde(default = "default_max_log_age")]
    max_log_age: usize,
    #[getter(skip)]
    /// Map of tracked kernel trees
    #[serde(default = "default_kernel_trees")]
    kernel_trees: HashMap<String, KernelTree>,
    /// Target kernel tree to run actions
    #[serde(default = "default_target_kernel_tree")]
    target_kernel_tree: Option<String>,
    /// Flags to be use with git am command when applying patches
    #[serde(default = "default_git_am_options")]
    git_am_options: String,
    #[serde(default = "default_git_am_branch_help")]
    git_am_branch_prefix: String,
}

#[derive(Debug, Serialize, Deserialize, Getters, Eq, PartialEq, Clone)]
pub struct KernelTree {
    /// Path to kernel tree in the filesystem
    path: String,
    /// Target branch
    branch: String,
}

impl Default for Config {
    fn default() -> Self {
        let home = env::var("HOME").unwrap_or_else(|_| {
            eprintln!("$HOME environment variable not set, using current directory");
            ".".to_string()
        });
        let cache_dir = format!("{}/.cache/patch_hub", home);
        let data_dir = format!("{}/.local/share/patch_hub", home);

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
            kernel_trees: HashMap::new(),
            target_kernel_tree: None,
            git_am_options: String::new(),
            git_am_branch_prefix: String::from("patchset-"),
        }
    }
}

impl Config {
    /// Loads the configuration for patch-hub from the config file.
    ///
    /// Returns the default config if the config file is not found or if it's not a valid JSON.
    fn load_file() -> Config {
        let config_path = Config::get_config_path();

        if Path::new(&config_path).is_file() {
            match fs::read_to_string(&config_path) {
                Ok(file_contents) => match serde_json::from_str(&file_contents) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("Failed to parse config file {}: {}", config_path, e),
                },
                Err(e) => {
                    eprintln!("Failed to read config file {}: {}", config_path, e)
                }
            }
        }

        Config::default()
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
        let mut config = Self::load_file();
        config.save_patch_hub_config().unwrap_or_else(|e| {
            eprintln!("Failed to save default config: {}", e);
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
    pub fn kernel_trees(&self) -> HashSet<&String> {
        self.kernel_trees.keys().collect::<HashSet<&String>>()
    }

    /// Returns a reference to the `KernelTree` mapped by `@kernel_tree_id`, if
    /// it exists.
    #[allow(dead_code)]
    pub fn get_kernel_tree(&self, kernel_tree_id: &str) -> Option<&KernelTree> {
        if let Some(kernel_tree) = self.kernel_trees.get(kernel_tree_id) {
            Some(kernel_tree)
        } else {
            None
        }
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
        let config_path = Config::get_config_path();

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

    /// Returns the current Config path.
    ///
    /// Resolves to env var PATCH_HUB_CONFIG_PATH, if set, and to env var HOME
    /// plus constant DEFAULT_CONFIG_PATH_SUFFIX, otherwise.
    fn get_config_path() -> String {
        env::var("PATCH_HUB_CONFIG_PATH").unwrap_or(format!(
            "{}/{}",
            env::var("HOME").unwrap(),
            DEFAULT_CONFIG_PATH_SUFFIX
        ))
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

fn default_page_size() -> usize {
    DEFAULT_CONFIG.page_size
}

fn default_patchsets_cache_dir() -> String {
    DEFAULT_CONFIG.patchsets_cache_dir.clone()
}

fn default_bookmarked_patchsets_path() -> String {
    DEFAULT_CONFIG.bookmarked_patchsets_path.clone()
}

fn default_mailing_lists_path() -> String {
    DEFAULT_CONFIG.mailing_lists_path.clone()
}

fn default_reviewed_patchsets_path() -> String {
    DEFAULT_CONFIG.reviewed_patchsets_path.clone()
}

fn default_logs_path() -> String {
    DEFAULT_CONFIG.logs_path.clone()
}

fn default_git_send_email_options() -> String {
    DEFAULT_CONFIG.git_send_email_options.clone()
}

fn default_cache_dir() -> String {
    DEFAULT_CONFIG.cache_dir.clone()
}

fn default_data_dir() -> String {
    DEFAULT_CONFIG.data_dir.clone()
}

fn default_patch_renderer() -> PatchRenderer {
    DEFAULT_CONFIG.patch_renderer
}

fn default_cover_renderer() -> CoverRenderer {
    DEFAULT_CONFIG.cover_renderer
}

fn default_max_log_age() -> usize {
    DEFAULT_CONFIG.max_log_age
}

fn default_kernel_trees() -> HashMap<String, KernelTree> {
    DEFAULT_CONFIG.kernel_trees.clone()
}

fn default_target_kernel_tree() -> Option<String> {
    DEFAULT_CONFIG.target_kernel_tree.clone()
}

fn default_git_am_options() -> String {
    DEFAULT_CONFIG.git_am_options.clone()
}

fn default_git_am_branch_help() -> String {
    DEFAULT_CONFIG.git_am_branch_prefix.clone()
}
