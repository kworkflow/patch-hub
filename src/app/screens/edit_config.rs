use std::{collections::HashMap, fmt::Display, path::Path};

use crate::app::config::Config;
use color_eyre::eyre::bail;
use derive_getters::Getters;

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
    pub fn config(&self, i: usize) -> (String, String) {
        let editable_config = EditableConfig::try_from(i).unwrap();
        let value = self.config_buffer.get(&editable_config).unwrap();
        (editable_config.to_string(), value.clone())
    }

    /// Toggle editing mode
    pub fn toggle_editing(&mut self) {
        if !self.is_editing {
            let editable_config = EditableConfig::try_from(self.highlighted()).unwrap();
            if let Some(value) = self.config_buffer.get(&editable_config) {
                self.curr_edit = value.clone();
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
        let editable_config = EditableConfig::try_from(self.highlighted).unwrap();
        self.config_buffer
            .insert(editable_config, std::mem::take(&mut self.curr_edit));
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
            4 => Ok(EditableConfig::PatchRenderer),
            5 => Ok(EditableConfig::CoverRenderer),
            6 => Ok(EditableConfig::MaxLogAge),
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
        }
    }
}
