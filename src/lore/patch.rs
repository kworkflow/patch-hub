use std::fmt::Display;

use derive_getters::Getters;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Getters, Serialize, Deserialize, Debug, Clone)]
pub struct PatchFeed {
    #[serde(rename = "entry")]
    patches: Vec<Patch>,
}

#[derive(Getters, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Patch {
    r#title: String,
    #[serde(default = "default_version")]
    #[getter(skip)]
    version: usize,
    #[serde(default = "default_number_in_series")]
    #[getter(skip)]
    number_in_series: usize,
    #[serde(default = "default_total_in_series")]
    #[getter(skip)]
    total_in_series: usize,
    author: Author,
    #[serde(rename = "link")]
    message_id: MessageID,
    #[serde(rename = "in-reply-to")]
    in_reply_to: Option<MessageID>,
    updated: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash, Eq, PartialEq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

impl Display for Author {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} <{}>", self.name, self.email)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MessageID {
    pub href: String,
}

fn default_version() -> usize {
    1
}
fn default_number_in_series() -> usize {
    1
}
fn default_total_in_series() -> usize {
    1
}

impl Patch {
    pub fn version(&self) -> usize {
        self.version
    }

    pub fn number_in_series(&self) -> usize {
        self.number_in_series
    }

    pub fn total_in_series(&self) -> usize {
        self.total_in_series
    }

    pub fn update_patch_metadata(&mut self, patch_regex: &PatchRegex) {
        let patch_tag: String = match self.get_patch_tag(&patch_regex.re_patch_tag) {
            Some(value) => value.to_string(),
            None => return,
        };

        self.remove_patch_tag_from_title(&patch_tag);
        self.set_version(&patch_tag, &patch_regex.re_patch_version);
        self.set_number_in_series(&patch_tag, &patch_regex.re_patch_series);
        self.set_total_in_series(&patch_tag, &patch_regex.re_patch_series);
    }

    fn get_patch_tag(&self, re_patch_tag: &Regex) -> Option<&str> {
        match re_patch_tag.find(&self.title) {
            Some(patch_tag) => Some(patch_tag.as_str()),
            None => None,
        }
    }

    fn remove_patch_tag_from_title(&mut self, patch_tag: &str) {
        self.title = self.title.replace(patch_tag, "").trim().to_string();
    }

    fn set_version(&mut self, patch_tag: &str, re_patch_version: &Regex) {
        if let Some(capture) = re_patch_version.captures(patch_tag) {
            if let Some(version) = capture.get(1) {
                self.version = version.as_str().parse().unwrap();
            }
        }
    }

    fn set_number_in_series(&mut self, patch_tag: &str, re_patch_series: &Regex) {
        if let Some(capture) = re_patch_series.captures(patch_tag) {
            if let Some(number_in_series) = capture.get(1) {
                self.number_in_series = number_in_series.as_str().parse().unwrap();
            }
        }
    }

    fn set_total_in_series(&mut self, patch_tag: &str, re_patch_series: &Regex) {
        if let Some(capture) = re_patch_series.captures(patch_tag) {
            if let Some(total_in_series) = capture.get(2) {
                self.total_in_series = total_in_series.as_str().parse().unwrap();
            }
        }
    }
}

pub struct PatchRegex {
    pub re_patch_tag: Regex,
    pub re_patch_version: Regex,
    pub re_patch_series: Regex,
}

impl Default for PatchRegex {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchRegex {
    pub fn new() -> PatchRegex {
        let re_patch_tag = Regex::new(r"(?i)\[[^\]]*(PATCH|RFC)[^\[]*\]").unwrap();
        let re_patch_version = Regex::new(r"[v|V] *(\d+)").unwrap();
        let re_patch_series = Regex::new(r"(\d+) */ *(\d+)").unwrap();

        PatchRegex {
            re_patch_tag,
            re_patch_version,
            re_patch_series,
        }
    }
}
