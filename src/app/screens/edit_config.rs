use std::{collections::HashMap, fmt::Display, path::Path};

use crate::app::config::Config;

#[derive(Debug)]
pub struct EditConfigState {
    config_buffer: HashMap<EditableConfig, String>,
    highlighted_entry: usize,
    is_editing: bool,
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
        config_buffer.insert(
            EditableConfig::PatchRenderer,
            config.patch_renderer().to_string(),
        );

        EditConfigState {
            config_buffer,
            highlighted_entry: 0,
            is_editing: false,
            editing_val: String::new(),
        }
    }

    pub fn is_editing(&self) -> bool {
        self.is_editing
    }

    pub fn get_number_of_configs(&self) -> usize {
        self.config_buffer.len()
    }

    pub fn get_config_by_index(&self, i: usize) -> (String, String) {
        let editable_config = EditableConfig::from_integer(i).unwrap();
        let value = self.config_buffer.get(&editable_config).unwrap();
        (editable_config.to_string(), value.clone())
    }

    pub fn get_highlighted_entry(&self) -> usize {
        self.highlighted_entry
    }

    pub fn get_editing_val(&self) -> &str {
        &self.editing_val
    }

    pub fn toggle_editing(&mut self) {
        if !self.is_editing {
            let editable_config = EditableConfig::from_integer(self.highlighted_entry).unwrap();
            if let Some(value) = self.config_buffer.get(&editable_config) {
                self.editing_val = value.clone();
            }
        }
        self.is_editing = !self.is_editing;
    }

    pub fn highlight_above_entry(&mut self) {
        if self.highlighted_entry > 0 {
            self.highlighted_entry -= 1;
        }
    }

    pub fn highlight_below_entry(&mut self) {
        if self.highlighted_entry + 1 < self.config_buffer.len() {
            self.highlighted_entry += 1;
        }
    }

    pub fn remove_char_from_editing_val(&mut self) {
        if !self.editing_val.is_empty() {
            self.editing_val.pop();
        }
    }

    pub fn add_char_to_editing_val(&mut self, ch: char) {
        self.editing_val.push(ch);
    }

    pub fn clear_editing_val(&mut self) {
        self.editing_val.clear();
    }

    pub fn push_editing_val_to_buffer(&mut self) {
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

    pub fn extract_page_size(&mut self) -> Result<usize, ()> {
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

    pub fn extract_cache_dir(&mut self) -> Result<String, ()> {
        let cache_dir = self.extract_config_buffer_val(&EditableConfig::CacheDir);
        match Self::is_valid_dir(&cache_dir) {
            true => Ok(cache_dir),
            false => Err(()),
        }
    }

    pub fn extract_data_dir(&mut self) -> Result<String, ()> {
        let data_dir = self.extract_config_buffer_val(&EditableConfig::DataDir);
        match Self::is_valid_dir(&data_dir) {
            true => Ok(data_dir),
            false => Err(()),
        }
    }

    pub fn extract_git_send_email_option(&mut self) -> Result<String, ()> {
        let git_send_emial_option =
            self.extract_config_buffer_val(&EditableConfig::GitSendEmailOpt);
        // TODO: Check if the option is valid
        Ok(git_send_emial_option)
    }

    pub fn extract_patch_renderer(&mut self) -> Result<String, ()> {
        let patch_renderer = self.extract_config_buffer_val(&EditableConfig::PatchRenderer);
        Ok(patch_renderer)
    }
}

#[derive(Debug, Hash, Eq, PartialEq)]
enum EditableConfig {
    PageSize,
    CacheDir,
    DataDir,
    GitSendEmailOpt,
    PatchRenderer,
}

impl EditableConfig {
    fn from_integer(i: usize) -> Option<EditableConfig> {
        match i {
            0 => Some(EditableConfig::PageSize),
            1 => Some(EditableConfig::CacheDir),
            2 => Some(EditableConfig::DataDir),
            3 => Some(EditableConfig::GitSendEmailOpt),
            4 => Some(EditableConfig::PatchRenderer),
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
            EditableConfig::PatchRenderer => {
                write!(f, "Patch Renderer (bat, delta, diff-so-fancy)")
            }
            EditableConfig::GitSendEmailOpt => write!(f, "`git send email` option"),
        }
    }
}
