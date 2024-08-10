use std::env;

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

        let patchsets_cache_dir: String = match env::var("KW_CACHE_DIR") {
            Ok(value) => format!("{value}/patch_hub/patchsets"),
            Err(_) => format!(
                "{}/.cache/kw/patch_hub/patchsets",
                env::var("HOME").unwrap()
            ),
        };

        let bookmarked_patchsets_path: String = match env::var("KW_DATA_DIR") {
            Ok(value) => format!("{value}/patch_hub/bookmarked_patchsets.json"),
            Err(_) => format!(
                "{}/.local/share/kw/patch_hub/bookmarked_patchsets.json",
                env::var("HOME").unwrap()
            ),
        };

        let mailing_lists_path: String = match env::var("KW_DATA_DIR") {
            Ok(value) => format!("{value}/patch_hub/mailing_lists.json"),
            Err(_) => format!(
                "{}/.local/share/kw/patch_hub/mailing_lists.json",
                env::var("HOME").unwrap()
            ),
        };

        let reviewed_patchsets_path: String = match env::var("KW_DATA_DIR") {
            Ok(value) => format!("{value}/patch_hub/reviewed_patchsets.json"),
            Err(_) => format!(
                "{}/.local/share/kw/patch_hub/reviewed_patchsets.json",
                env::var("HOME").unwrap()
            ),
        };

        let logs_path = match env::var("KW_DATA_DIR") {
            Ok(value) => format!("{value}/patch_hub/logs"),
            Err(_) => format!(
                "{}/.local/share/kw/patch_hub/logs",
                env::var("HOME").unwrap()
            ),
        };

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
