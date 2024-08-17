use std::env;

pub struct Config {
    pub page_size: u32,
    pub patchsets_cache_dir: String,
    pub bookmarked_patchsets_path: String,
    pub mailing_lists_path: String,
    pub reviewed_patchsets_path: String,
}

impl Config {
    pub fn build() -> Self {
        let page_size: u32;
        let patchsets_cache_dir: String;
        let bookmarked_patchsets_path: String;
        let mailing_lists_path: String;
        let reviewed_patchsets_path: String;

        page_size = match env::var("PATCH_HUB_PAGE_SIZE") {
            Ok(value) => value.parse().unwrap(),
            Err(_) => 30,
        };

        patchsets_cache_dir = match env::var("KW_CACHE_DIR") {
            Ok(value) => format!("{value}/patch_hub/patchsets"),
            Err(_) => format!(
                "{}/.cache/kw/patch_hub/patchsets",
                env::var("HOME").unwrap()
            ),
        };

        bookmarked_patchsets_path = match env::var("KW_DATA_DIR") {
            Ok(value) => format!("{value}/patch_hub/bookmarked_patchsets.json"),
            Err(_) => format!(
                "{}/.local/share/kw/patch_hub/bookmarked_patchsets.json",
                env::var("HOME").unwrap()
            ),
        };

        mailing_lists_path = match env::var("KW_DATA_DIR") {
            Ok(value) => format!("{value}/patch_hub/mailing_lists.json"),
            Err(_) => format!(
                "{}/.local/share/kw/patch_hub/mailing_lists.json",
                env::var("HOME").unwrap()
            ),
        };

        reviewed_patchsets_path = match env::var("KW_DATA_DIR") {
            Ok(value) => format!("{value}/patch_hub/reviewed_patchsets.json"),
            Err(_) => format!(
                "{}/.local/share/kw/patch_hub/reviewed_patchsets.json",
                env::var("HOME").unwrap()
            ),
        };

        Config {
            page_size,
            patchsets_cache_dir,
            bookmarked_patchsets_path,
            mailing_lists_path,
            reviewed_patchsets_path,
        }
    }
}
