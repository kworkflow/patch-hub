use serde::{Deserialize, Serialize};
use tracing::{event, Level};

use std::fmt::Display;

use color_eyre::eyre::eyre;

use crate::infrastructure::shell::{ShellCommand, ShellTrait};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub enum CoverRenderer {
    #[default]
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "bat")]
    Bat,
}

impl From<String> for CoverRenderer {
    fn from(value: String) -> Self {
        match value.as_str() {
            "bat" => CoverRenderer::Bat,
            _ => CoverRenderer::Default,
        }
    }
}

impl From<&str> for CoverRenderer {
    fn from(value: &str) -> Self {
        match value {
            "bat" => CoverRenderer::Bat,
            _ => CoverRenderer::Default,
        }
    }
}

impl Display for CoverRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverRenderer::Default => write!(f, "default"),
            CoverRenderer::Bat => write!(f, "bat"),
        }
    }
}

pub fn render_cover(
    shell: &dyn ShellTrait,
    raw: &str,
    renderer: &CoverRenderer,
) -> color_eyre::Result<String> {
    let text = match renderer {
        CoverRenderer::Default => Ok(raw.to_string()),
        CoverRenderer::Bat => bat_cover_renderer(shell, raw),
    }?;

    Ok(text)
}

/// Renders a .mbx cover using the `bat` command line tool.
///
/// # Errors
///
/// If bat isn't installed or if the command fails, an error will be returned.
fn bat_cover_renderer(shell: &dyn ShellTrait, patch: &str) -> color_eyre::Result<String> {
    let cmd = ShellCommand::new("bat").args(["-pp", "-f", "-l", "mbx"]);

    let out = shell
        .execute_with_stdin(&cmd, patch.as_bytes())
        .map_err(|e| {
            event!(Level::ERROR, "Failed to spawn bat for cover preview: {}", e);
            eyre!(e)
        })?;

    Ok(String::from_utf8(out.stdout)?)
}
