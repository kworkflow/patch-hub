use color_eyre::eyre::bail;
use derive_getters::Getters;

use std::{collections::HashMap, fmt::Display};

use crate::config::{ConfigSnapshot, ConfigUpdateDraft};

#[derive(Debug, Getters)]
pub struct EditConfigState {
    #[getter(skip)]
    config_buffer: HashMap<EditableConfig, String>,
    highlighted: usize,
    is_editing: bool,
    curr_edit: String,
}

impl EditConfigState {
    pub fn new(config: &ConfigSnapshot) -> Self {
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

        EditConfigState {
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

    /// Raw form values for [`crate::config::ConfigServiceApi::validate_update`].
    pub fn to_update_draft(&self) -> ConfigUpdateDraft {
        ConfigUpdateDraft {
            page_size: self.config_buffer.get(&EditableConfig::PageSize).cloned(),
            cache_dir: self.config_buffer.get(&EditableConfig::CacheDir).cloned(),
            data_dir: self.config_buffer.get(&EditableConfig::DataDir).cloned(),
            git_send_email_option: self
                .config_buffer
                .get(&EditableConfig::GitSendEmailOpt)
                .cloned(),
            git_am_option: self.config_buffer.get(&EditableConfig::GitAmOpt).cloned(),
            patch_renderer: self
                .config_buffer
                .get(&EditableConfig::PatchRenderer)
                .cloned(),
            cover_renderer: self
                .config_buffer
                .get(&EditableConfig::CoverRenderer)
                .cloned(),
            max_log_age: self.config_buffer.get(&EditableConfig::MaxLogAge).cloned(),
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
