use crate::config::errors::ConfigError;
use crate::render_prefs::{CoverRenderer, PatchRenderer};

pub(crate) fn parse_patch_renderer(s: &str) -> Result<PatchRenderer, ConfigError> {
    match s.trim() {
        "" | "default" => Ok(PatchRenderer::Default),
        "bat" => Ok(PatchRenderer::Bat),
        "delta" => Ok(PatchRenderer::Delta),
        "diff-so-fancy" => Ok(PatchRenderer::DiffSoFancy),
        other => Err(ConfigError::InvalidPatchRenderer(other.to_string())),
    }
}

pub(crate) fn parse_cover_renderer(s: &str) -> Result<CoverRenderer, ConfigError> {
    match s.trim() {
        "" | "default" => Ok(CoverRenderer::Default),
        "bat" => Ok(CoverRenderer::Bat),
        other => Err(ConfigError::InvalidCoverRenderer(other.to_string())),
    }
}
