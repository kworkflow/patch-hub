use color_eyre::eyre::bail;
use derive_getters::Getters;

use std::{collections::HashMap, fmt::Display, path::Path};

use crate::app::config::Config;

#[derive(Debug, Getters)]
pub struct EditConfig {
    #[getter(skip)]
    config_buffer: HashMap<EditableConfig, String>,
    highlighted: usize,
    is_editing: bool,
    curr_edit: String,
}

impl EditConfig {
    pub fn new(config: &Config) -> Self {
        let mut config_buffer = HashMap::new();
        config_buffer.insert(EditableConfig::PageSize, config.page_size().to_string());
        config_buffer.insert(EditableConfig::CacheDir, config.cache_dir().to_string());
        config_buffer.insert(EditableConfig::DataDir, config.data_dir().to_string());
        config_buffer.insert(
            EditableConfig::GitSendEmailOpt,
            config.git_send_email_options().to_string(),
        );
        config_buffer.insert(
            EditableConfig::GitAmOpt,
            config.git_am_options().to_string(),
        );
        config_buffer.insert(
            EditableConfig::PatchRenderer,
            config.patch_renderer().to_string(),
        );
        config_buffer.insert(
            EditableConfig::CoverRenderer,
            config.cover_renderer().to_string(),
        );
        config_buffer.insert(EditableConfig::MaxLogAge, config.max_log_age().to_string());

        EditConfig {
            config_buffer,
            highlighted: 0,
            is_editing: false,
            curr_edit: String::new(),
        }
    }

    /// Get the number of config entries in the config buffer
    pub fn config_count(&self) -> usize {
        self.config_buffer.len()
    }

    /// Get the config entry at the given index
    pub fn config(&self, i: usize) -> Option<(String, String)> {
        EditableConfig::try_from(i)
            .ok()
            .and_then(|editable_config| {
                self.config_buffer
                    .get(&editable_config)
                    .map(|value| (editable_config.to_string(), value.clone()))
            })
    }

    /// Toggle editing mode
    pub fn toggle_editing(&mut self) {
        if !self.is_editing {
            if let Ok(editable_config) = EditableConfig::try_from(self.highlighted()) {
                if let Some(value) = self.config_buffer.get(&editable_config) {
                    self.curr_edit = value.clone();
                }
            }
        }
        self.is_editing = !self.is_editing;
    }

    /// Move the highlight to the previous entry
    pub fn highlight_prev(&mut self) {
        if self.highlighted > 0 {
            self.highlighted -= 1;
        }
    }

    /// Move the highlight to the next entry
    pub fn highlight_next(&mut self) {
        if self.highlighted + 1 < self.config_buffer.len() {
            self.highlighted += 1;
        }
    }

    /// Remove the last char from the current editing value if not empty
    pub fn backspace_edit(&mut self) {
        if !self.curr_edit.is_empty() {
            self.curr_edit.pop();
        }
    }

    /// Appends a new char to the current editing value
    pub fn append_edit(&mut self, ch: char) {
        self.curr_edit.push(ch);
    }

    /// Clear the current editing value
    pub fn clear_edit(&mut self) {
        self.curr_edit.clear();
    }

    /// Push the current edit value to the config buffer
    pub fn stage_edit(&mut self) {
        if let Ok(editable_config) = EditableConfig::try_from(self.highlighted) {
            self.config_buffer
                .insert(editable_config, std::mem::take(&mut self.curr_edit));
        }
    }
}

impl EditConfig {
    fn extract_config_buffer_val(&mut self, editable_config: &EditableConfig) -> String {
        let mut ret_value = String::new();
        if let Some(config_value) = self.config_buffer.get_mut(editable_config) {
            std::mem::swap(&mut ret_value, config_value);
        }
        ret_value
    }

    /// Extracts the page size from the config
    ///
    /// # Errors
    ///
    /// Returns an error if the page size inserted string is not a valid integer
    pub fn page_size(&mut self) -> Result<usize, ()> {
        match self
            .extract_config_buffer_val(&EditableConfig::PageSize)
            .parse::<usize>()
        {
            Ok(value) => Ok(value),
            Err(_) => Err(()),
        }
    }

    fn is_valid_dir(dir_path: &str) -> bool {
        let path_to_check = Path::new(dir_path);

        if path_to_check.exists() && path_to_check.is_dir() {
            true
        } else {
            std::fs::create_dir_all(path_to_check).is_ok()
        }
    }

    /// Extracts the cache directory from the config
    ///
    /// # Errors
    ///
    /// Returns an error if the cache directory is not a valid directory
    pub fn cache_dir(&mut self) -> Result<String, ()> {
        let cache_dir = self.extract_config_buffer_val(&EditableConfig::CacheDir);
        match Self::is_valid_dir(&cache_dir) {
            true => Ok(cache_dir),
            false => Err(()),
        }
    }

    /// Extracts the data directory from the config
    ///
    /// # Errors
    ///
    /// Returns an error if the data directory is not a valid directory
    pub fn data_dir(&mut self) -> Result<String, ()> {
        let data_dir = self.extract_config_buffer_val(&EditableConfig::DataDir);
        match Self::is_valid_dir(&data_dir) {
            true => Ok(data_dir),
            false => Err(()),
        }
    }

    /// Extracts the `git send email` option from the config
    pub fn git_send_email_option(&mut self) -> Result<String, ()> {
        let git_send_emial_option =
            self.extract_config_buffer_val(&EditableConfig::GitSendEmailOpt);
        // TODO: Check if the option is valid
        Ok(git_send_emial_option)
    }

    /// Extracts the `git am` option from the config
    pub fn git_am_option(&mut self) -> Result<String, ()> {
        let git_am_option = self.extract_config_buffer_val(&EditableConfig::GitAmOpt);
        Ok(git_am_option)
    }

    pub fn extract_patch_renderer(&mut self) -> Result<String, ()> {
        let patch_renderer = self.extract_config_buffer_val(&EditableConfig::PatchRenderer);
        Ok(patch_renderer)
    }

    pub fn extract_cover_renderer(&mut self) -> Result<String, ()> {
        let cover_renderer = self.extract_config_buffer_val(&EditableConfig::CoverRenderer);
        Ok(cover_renderer)
    }

    /// Extracts the max log age from the config
    ///
    /// # Errors
    ///
    /// Returns an error if the max log age inserted string is not a valid integer
    pub fn max_log_age(&mut self) -> Result<usize, ()> {
        match self
            .extract_config_buffer_val(&EditableConfig::MaxLogAge)
            .parse::<usize>()
        {
            Ok(value) => Ok(value),
            Err(_) => Err(()),
        }
    }
}

#[derive(Debug, Hash, Eq, PartialEq)]
enum EditableConfig {
    PageSize,
    CacheDir,
    DataDir,
    GitSendEmailOpt,
    GitAmOpt,
    PatchRenderer,
    CoverRenderer,
    MaxLogAge,
}

impl TryFrom<usize> for EditableConfig {
    type Error = color_eyre::Report;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(EditableConfig::PageSize),
            1 => Ok(EditableConfig::CacheDir),
            2 => Ok(EditableConfig::DataDir),
            3 => Ok(EditableConfig::GitSendEmailOpt),
            4 => Ok(EditableConfig::GitAmOpt),
            5 => Ok(EditableConfig::PatchRenderer),
            6 => Ok(EditableConfig::CoverRenderer),
            7 => Ok(EditableConfig::MaxLogAge),
            _ => bail!("Invalid index {} for EditableConfig", value), // Handle out of bounds
        }
    }
}

impl Display for EditableConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditableConfig::PageSize => write!(f, "Page Size"),
            EditableConfig::CacheDir => write!(f, "Cache Directory"),
            EditableConfig::DataDir => write!(f, "Data Directory"),
            EditableConfig::PatchRenderer => {
                write!(f, "Patch Renderer (bat, delta, diff-so-fancy)")
            }
            EditableConfig::CoverRenderer => {
                write!(f, "Cover Renderer (bat)")
            }
            EditableConfig::GitSendEmailOpt => write!(f, "`git send email` option"),
            EditableConfig::MaxLogAge => write!(f, "Max Log Age (0 = forever)"),
            EditableConfig::GitAmOpt => write!(f, "`git am` option"),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap};

    fn config_buffer_instance() -> HashMap<EditableConfig, String> {
        let mut config_buffer = HashMap::new();
        config_buffer.insert(EditableConfig::PageSize, "20".into());
        config_buffer.insert(
            EditableConfig::CacheDir,
            std::env::temp_dir().to_string_lossy().into(),
        );
        config_buffer.insert(
            EditableConfig::DataDir,
            std::env::temp_dir().to_string_lossy().into(),
        );
        config_buffer.insert(EditableConfig::GitSendEmailOpt, "--x".into());
        config_buffer.insert(EditableConfig::GitAmOpt, "--y".into());
        config_buffer.insert(EditableConfig::PatchRenderer, "bat".into());
        config_buffer.insert(EditableConfig::CoverRenderer, "bat".into());
        config_buffer.insert(EditableConfig::MaxLogAge, "7".into());
        config_buffer
    }

    fn instance_from_config_buffer(config_buffer: HashMap<EditableConfig, String>) -> EditConfig {
        EditConfig {
            config_buffer,
            highlighted: 0,
            is_editing: false,
            curr_edit: String::new(),
        }
    }

    #[test]
    fn test_initial_values_loaded_correctly() {
        let config = config_buffer_instance();
        let config_len = config.len();
        let edit = instance_from_config_buffer(config);

        assert_eq!(edit.config_count(), config_len);

        let (name, value) = edit
            .config(0)
            .expect("Deve haver uma configuração no índice 0");

        assert_eq!(name, "Page Size");
        assert_eq!(value, "20");
    }

        #[test]
    fn test_highlight_next_within_bounds() {
        let config = config_buffer_instance();
        let mut edit = instance_from_config_buffer(config);

        assert_eq!(edit.highlighted(), 0);

        edit.highlight_next();
        assert_eq!(edit.highlighted(), 1);

        edit.highlight_next();
        assert_eq!(edit.highlighted(), 2);
    }

    #[test]
    fn test_highlight_next_stops_at_last() {
        let config = config_buffer_instance();
        let mut edit = instance_from_config_buffer(config);

        let last_index = edit.config_count() - 1;

        edit.highlighted = last_index;

        edit.highlight_next();

        assert_eq!(edit.highlighted(), last_index);
    }

    #[test]
    fn test_highlight_prev_within_bounds() {
        let config = config_buffer_instance();
        let mut edit = instance_from_config_buffer(config);

        edit.highlight_next();
        edit.highlight_next();

        assert_eq!(edit.highlighted(), 2);

        edit.highlight_prev();
        assert_eq!(edit.highlighted(), 1);

        edit.highlight_prev();
        assert_eq!(edit.highlighted(), 0);
    }

    #[test]
    fn test_highlight_prev_stops_at_zero() {
        let config = config_buffer_instance();
        let mut edit = instance_from_config_buffer(config);

        edit.highlighted = 0;

        assert_eq!(edit.highlighted(), 0);

        edit.highlight_prev();

        assert_eq!(edit.highlighted(), 0);
    }

    #[test]
    fn test_append_edit() {
        let config = config_buffer_instance();
        let mut edit = instance_from_config_buffer(config);

        edit.is_editing = true;
        edit.curr_edit.clear();

        edit.append_edit('a');
        edit.append_edit('b');
        edit.append_edit('c');
        assert_eq!(edit.curr_edit, "abc");
        edit.append_edit('ç');
        assert_eq!(edit.curr_edit, "abcç");
    }

    #[test]
    fn test_backspace_edit() {
        let config = config_buffer_instance();
        let mut edit = instance_from_config_buffer(config);

        edit.is_editing = true;

        edit.curr_edit.clear();

        edit.curr_edit = "abc".into();
        edit.backspace_edit();
        assert_eq!(edit.curr_edit, "ab");
        edit.backspace_edit();
        assert_eq!(edit.curr_edit, "a");
        edit.backspace_edit();
        assert_eq!(edit.curr_edit, "");
        edit.backspace_edit();
        assert_eq!(edit.curr_edit, "");
    }

    #[test]
    fn test_clear_edit() {
        let mut edit = instance_from_config_buffer(config_buffer_instance());

        edit.is_editing = true;

        assert!(edit.curr_edit().is_empty());
        edit.clear_edit();
        assert!(edit.curr_edit().is_empty());

        edit.curr_edit = "abc".to_string();
        assert_eq!(edit.curr_edit(), "abc");

        edit.clear_edit();
        assert!(edit.curr_edit().is_empty());
    }

    #[test]
    fn test_toggle_editing_twice() {
        let config = config_buffer_instance();
        let mut edit = instance_from_config_buffer(config);

        assert!(!edit.is_editing);

        let (_, initial_val) = edit.config(edit.highlighted()).unwrap();

        edit.toggle_editing();
        assert!(edit.is_editing);
        assert_eq!(*edit.curr_edit(), initial_val);

        edit.toggle_editing();
        assert!(!edit.is_editing);
        assert_eq!(*edit.curr_edit(), initial_val);
    }

    #[test]
    fn test_stage_edit_updates_value() {
        let mut edit = instance_from_config_buffer(config_buffer_instance());

        edit.is_editing = true;

        edit.clear_edit();
        edit.append_edit('4');
        edit.append_edit('2');
        assert_eq!(edit.curr_edit(), "42");

        edit.stage_edit();

        let (_, value) = edit.config(0).unwrap();
        assert_eq!(value, "42");
    }

    #[test]
    fn test_page_size_parsing() {
        let mut edit = instance_from_config_buffer(config_buffer_instance());

        edit.config_buffer.insert(EditableConfig::PageSize, "50".to_string());

        assert_eq!(edit.page_size().unwrap(), 50);
    }

    #[test]
    fn test_page_size_invalid() {
        let mut edit = instance_from_config_buffer(config_buffer_instance());

        edit.config_buffer.insert(EditableConfig::PageSize, "x".to_string());

        assert!(edit.page_size().is_err());
    }

    #[test]
    fn test_cache_dir_valid() {
        let mut edit = instance_from_config_buffer(config_buffer_instance());

        edit.config_buffer.insert(EditableConfig::CacheDir, "/tmp/cache".into());

        assert!(edit.cache_dir().is_ok());
    }

    #[test]
    fn test_data_dir_valid() {
        let mut edit = instance_from_config_buffer(config_buffer_instance());

        edit.config_buffer.insert(EditableConfig::DataDir, "/tmp/data".into());

        assert!(edit.data_dir().is_ok());
    }

    #[test]
    fn test_max_log_age_parsing() {
        let mut edit = instance_from_config_buffer(config_buffer_instance());

        edit.config_buffer.insert(EditableConfig::MaxLogAge, "99".to_string());

        assert_eq!(edit.max_log_age().unwrap(), 99);
    }


    #[test]
    fn test_try_from_valid_index() {
        for i in 0..=7 {
            assert!(EditableConfig::try_from(i).is_ok());
        }

        assert!(EditableConfig::try_from(8).is_err());
    }
}
