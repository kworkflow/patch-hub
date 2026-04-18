use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MessageID {
    #[serde(rename = "@href")]
    pub href: String,
}
