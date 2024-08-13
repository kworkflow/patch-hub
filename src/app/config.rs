use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File},
    io,
    path::Path,
};

#[cfg(test)]
mod tests;

#[derive(Serialize, Deserialize)]
pub struct Config {
    page_size: usize,
    patchsets_cache_dir: String,
    bookmarked_patchsets_path: String,
    mailing_lists_path: String,
    reviewed_patchsets_path: String,
    /// Logs directory
    logs_path: String,
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
        }
    }

    fn detect_patch_hub_config_file() -> Option<Config> {
        if let Ok(config_path) = env::var("PATCH_HUB_CONFIG_PATH") {
            if Path::new(&config_path).is_file() {
                let file_contents = fs::read_to_string(&config_path).unwrap_or(String::new());
                if let Ok(config) = serde_json::from_str(&file_contents) {
                    return Some(config);
                }
            }
        }

        let config_path = format!(
            "{}/.local/share/patch_hub/config.json",
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
            self.patchsets_cache_dir = format!("{cache_dir}/patchsets");
        };

        if let Ok(data_dir) = env::var("PATCH_HUB_DATA_DIR") {
            self.bookmarked_patchsets_path = format!("{data_dir}/bookmarked_patchsets.json");
            self.mailing_lists_path = format!("{data_dir}/mailing_lists.json");
            self.reviewed_patchsets_path = format!("{data_dir}/reviewed_patchsets.json");
            self.logs_path = format!("{data_dir}/logs");
        };
    }

    pub fn build() -> Self {
        let mut config = Self::default();

        if let Some(config_from_file) = Self::detect_patch_hub_config_file() {
            config = config_from_file;
        }

        config.override_with_env_vars();

        config
    }

    pub fn get_page_size(&self) -> usize {
        self.page_size
    }

    pub fn get_patchsets_cache_dir(&self) -> &str {
        &self.patchsets_cache_dir
    }

    pub fn get_bookmarked_patchsets_path(&self) -> &str {
        &self.bookmarked_patchsets_path
    }

    pub fn get_mailing_lists_path(&self) -> &str {
        &self.mailing_lists_path
    }

    pub fn get_reviewed_patchsets_path(&self) -> &str {
        &self.reviewed_patchsets_path
    }

    pub fn get_logs_path(&self) -> &str {
        &self.logs_path
    }

    #[allow(dead_code)]
    pub fn save_patch_hub_config(&self) -> io::Result<()> {
        let config_path = if let Ok(path) = env::var("PATCH_HUB_CONFIG_PATH") {
            path
        } else {
            format!(
                "{}/.local/share/patch_hub/config.json",
                env::var("HOME").unwrap()
            )
        };

        let tmp_filename = format!("{}.tmp", config_path);
        {
            let tmp_file = File::create(&tmp_filename)?;
            serde_json::to_writer_pretty(tmp_file, self)?;
        }
        fs::rename(tmp_filename, config_path)?;
        Ok(())
    }
}
