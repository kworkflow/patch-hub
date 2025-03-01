use std::{
    fmt::Display,
    io::Write,
    process::{Command, Stdio},
};

use color_eyre::eyre::Context;
use serde::{Deserialize, Serialize};

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

pub fn render_cover(raw: &str, renderer: CoverRenderer) -> color_eyre::Result<String> {
    let text = match renderer {
        CoverRenderer::Default => Ok(raw.to_string()),
        CoverRenderer::Bat => bat_cover_renderer(raw),
    }?;

    Ok(text)
}

/// Renders a .mbx cover using the `bat` command line tool.
///
/// # Errors
///
/// If bat isn't installed or if the command fails, an error will be returned.
fn bat_cover_renderer(patch: &str) -> color_eyre::Result<String> {
    let mut bat = Command::new("bat")
        .arg("-pp")
        .arg("-f")
        .arg("-l")
        .arg("mbx")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to spawn bat for cover preview")?;

    bat.stdin.as_mut().unwrap().write_all(patch.as_bytes())?;
    let output = bat.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}
