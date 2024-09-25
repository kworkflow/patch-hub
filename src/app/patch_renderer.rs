use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub enum PatchRenderer {
    #[default]
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "bat")]
    Bat,
    #[serde(rename = "delta")]
    Delta,
    #[serde(rename = "diff-so-fancy")]
    DiffSoFancy,
}

impl From<String> for PatchRenderer {
    fn from(value: String) -> Self {
        match value.as_str() {
            "bat" => PatchRenderer::Bat,
            "delta" => PatchRenderer::Delta,
            "diff-so-fancy" => PatchRenderer::DiffSoFancy,
            _ => PatchRenderer::Default,
        }
    }
}

impl From<&str> for PatchRenderer {
    fn from(value: &str) -> Self {
        match value {
            "bat" => PatchRenderer::Bat,
            "delta" => PatchRenderer::Delta,
            "diff-so-fancy" => PatchRenderer::DiffSoFancy,
            _ => PatchRenderer::Default,
        }
    }
}

impl Display for PatchRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchRenderer::Default => write!(f, "default"),
            PatchRenderer::Bat => write!(f, "bat"),
            PatchRenderer::Delta => write!(f, "delta"),
            PatchRenderer::DiffSoFancy => write!(f, "diff-so-fancy"),
        }
    }
}