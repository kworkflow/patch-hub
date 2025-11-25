use serde::{Deserialize, Serialize};

use std::{
    fmt::Display,
    io::Write,
    process::{Command, Stdio},
};

use crate::infrastructure::logging::Logger;

/// Choices for showing the cover letter.
///
/// You can show it as plain text or use tools like `bat` to add colors.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub enum CoverRenderer {
    #[default]
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "bat")]
    Bat,
}

/// Creates a renderer choice from a String.
impl From<String> for CoverRenderer {
    fn from(value: String) -> Self {
        match value.as_str() {
            "bat" => CoverRenderer::Bat,
            _ => CoverRenderer::Default,
        }
    }
}

/// Creates a renderer choice from a string slice.
impl From<&str> for CoverRenderer {
    fn from(value: &str) -> Self {
        match value {
            "bat" => CoverRenderer::Bat,
            _ => CoverRenderer::Default,
        }
    }
}

/// Turns the choice back into a String.
impl Display for CoverRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverRenderer::Default => write!(f, "default"),
            CoverRenderer::Bat => write!(f, "bat"),
        }
    }
}

/// Decides how to show the cover letter.
///
/// It looks at the settings and calls the right function to format the text.
pub fn render_cover(raw: &str, renderer: &CoverRenderer) -> color_eyre::Result<String> {
    let text = match renderer {
        CoverRenderer::Default => Ok(raw.to_string()),
        CoverRenderer::Bat => bat_cover_renderer(raw),
    }?;

    Ok(text)
}

/// Formats the cover letter using the `bat` command.
///
/// # Errors
///
/// Fails if `bat` is not installed or has an error.
fn bat_cover_renderer(patch: &str) -> color_eyre::Result<String> {
    let mut bat = Command::new("bat")
        .arg("-pp") // Plain mode: no paging or extra decorations
        .arg("-f") // Force: keep colors even if piping
        .arg("-l") // Language: set the language type
        .arg("mbx") // Mailbox format
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Logger::error(format!("Failed to spawn bat for cover preview: {e}"));
            e
        })?;

    // Sends the text to bat
    bat.stdin.as_mut().unwrap().write_all(patch.as_bytes())?;

    // Waits for bat to finish and gets the result
    let output = bat.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}
