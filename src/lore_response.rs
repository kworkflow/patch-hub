use crate::patch::Patch;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct LoreResponse {
    #[serde(rename = "entry")]
    patches: Vec<Patch>,
}

impl LoreResponse {
    pub fn get_patches(self: Self) -> Vec<Patch> {
        self.patches
    }
}
