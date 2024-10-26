use derive_getters::Getters;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Getters, Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct MailingList {
    name: String,
    description: String,
}

impl MailingList {
    pub fn new(name: &str, description: &str) -> Self {
        MailingList {
            name: name.to_string(),
            description: description.to_string(),
        }
    }
}

impl Ord for MailingList {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for MailingList {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
