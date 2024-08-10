use std::env;

#[cfg(test)]
mod tests;

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
    pub fn build() -> Self {
        let page_size: usize = match env::var("PATCH_HUB_PAGE_SIZE") {
            Ok(value) => value.parse().unwrap(),
            Err(_) => 30,
        };

        let cache_dir = match env::var("PATCH_HUB_CACHE_DIR") {
            Ok(value) => value,
            Err(_) => format!("{}/.cache/patch_hub", env::var("HOME").unwrap()),
        };

        let patchsets_cache_dir = format!("{cache_dir}/patchsets");

        let data_dir = match env::var("PATCH_HUB_DATA_DIR") {
            Ok(value) => value,
            Err(_) => format!("{}/.local/share/patch_hub", env::var("HOME").unwrap()),
        };

        let bookmarked_patchsets_path = format!("{data_dir}/bookmarked_patchsets.json");
        let mailing_lists_path = format!("{data_dir}/mailing_lists.json");
        let reviewed_patchsets_path = format!("{data_dir}/reviewed_patchsets.json");
        let logs_path = format!("{data_dir}/logs");

        Config {
            page_size,
            patchsets_cache_dir,
            bookmarked_patchsets_path,
            mailing_lists_path,
            reviewed_patchsets_path,
            logs_path,
        }
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
}
