use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct MailingList {
    name: String,
    description: String,
}

impl MailingList {
    pub fn new(name: &str, description: &str) -> Self {
        MailingList {
            name: format!("{name}"),
            description: format!("{description}"),
        }
    }

    pub fn get_name(self: &Self) -> &str {
        &self.name
    }

    pub fn get_description(self: &Self) -> &str {
        &self.description
    }
}

impl Ord for MailingList {
    fn cmp(self: &Self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for MailingList {
    fn partial_cmp(self: &Self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
