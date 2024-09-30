use std::{collections::HashMap, fmt::Display, path::Path};

use derive_getters::Getters;

use super::config::Config;

#[derive(Debug, Getters)]
pub struct EditConfigState {
    #[getter(skip)]
    config_buffer: HashMap<EditableConfig, String>,
    #[getter(rename = "highlighted")]
    highlighted_entry: usize,
    is_editing: bool,
    #[getter(rename = "edit")]
    editing_val: String,
}

impl EditConfigState {
    pub fn new(config: &Config) -> Self {
        let mut config_buffer = HashMap::new();
        config_buffer.insert(EditableConfig::PageSize, config.page_size().to_string());
        config_buffer.insert(EditableConfig::CacheDir, config.cache_dir().to_string());
        config_buffer.insert(EditableConfig::DataDir, config.data_dir().to_string());
        config_buffer.insert(
            EditableConfig::GitSendEmailOpt,
            config.git_send_email_options().to_string(),
        );

        EditConfigState {
            config_buffer,
            highlighted_entry: 0,
            is_editing: false,
            editing_val: String::new(),
        }
    }

    /// Get the number of config entries in the config buffer
    pub fn config_count(&self) -> usize {
        self.config_buffer.len()
    }

    /// Get the config entry at the given index
    pub fn config(&self, i: usize) -> (String, String) {
        let editable_config = EditableConfig::from_integer(i).unwrap();
        let value = self.config_buffer.get(&editable_config).unwrap();
        (editable_config.to_string(), value.clone())
    }

    /// Toggle editing mode
    pub fn toggle_editing(&mut self) {
        if !self.is_editing {
            let editable_config = EditableConfig::from_integer(self.highlighted_entry).unwrap();
            if let Some(value) = self.config_buffer.get(&editable_config) {
                self.editing_val = value.clone();
            }
        }
        self.is_editing = !self.is_editing;
    }

    /// Move the highlight to the previous entry
    pub fn highlight_prev(&mut self) {
        if self.highlighted_entry > 0 {
            self.highlighted_entry -= 1;
        }
    }

    /// Move the highlight to the next entry
    pub fn highlight_next(&mut self) {
        if self.highlighted_entry + 1 < self.config_buffer.len() {
            self.highlighted_entry += 1;
        }
    }

    /// Remove the last char from the current editing value if not empty
    pub fn backspace_edit(&mut self) {
        if !self.editing_val.is_empty() {
            self.editing_val.pop();
        }
    }

    /// Appends a new char to the current editing value
    pub fn append_edit(&mut self, ch: char) {
        self.editing_val.push(ch);
    }

    /// Clear the current editing value
    pub fn clear_edit(&mut self) {
        self.editing_val.clear();
    }

    /// Save the current edit value to the config buffer
    pub fn save_edit(&mut self) {
        let editable_config = EditableConfig::from_integer(self.highlighted_entry).unwrap();
        self.config_buffer
            .insert(editable_config, std::mem::take(&mut self.editing_val));
    }
}

impl EditConfigState {
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
}

#[derive(Debug, Hash, Eq, PartialEq)]
enum EditableConfig {
    PageSize,
    CacheDir,
    DataDir,
    GitSendEmailOpt,
}

impl EditableConfig {
    fn from_integer(i: usize) -> Option<EditableConfig> {
        match i {
            0 => Some(EditableConfig::PageSize),
            1 => Some(EditableConfig::CacheDir),
            2 => Some(EditableConfig::DataDir),
            3 => Some(EditableConfig::GitSendEmailOpt),
            _ => None, // Handle out of bounds
        }
    }
}

impl Display for EditableConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditableConfig::PageSize => write!(f, "Page Size"),
            EditableConfig::CacheDir => write!(f, "Cache Directory"),
            EditableConfig::DataDir => write!(f, "Data Directory"),
            EditableConfig::GitSendEmailOpt => write!(f, "`git send email` option"),
        }
    }
}
