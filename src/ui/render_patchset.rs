use std::{
    io::Write,
    process::{Command, Stdio},
};

use ansi_to_tui::IntoText;
use ratatui::text::Text;

use crate::app::{logging::Logger, patch_renderer::PatchRenderer};

pub fn render_patch_preview(
    raw: &str,
    renderer: &PatchRenderer,
) -> color_eyre::Result<Text<'static>> {
    let text = match renderer {
        PatchRenderer::Default => Ok(raw.to_string()),
        PatchRenderer::Bat => bat_patch_renderer(raw),
        PatchRenderer::Delta => delta_patch_renderer(raw),
    }?
    .into_text()?;

    Ok(text)
}

#[allow(dead_code)]
fn bat_patch_renderer(patch: &str) -> color_eyre::Result<String> {
    let mut bat = Command::new("bat")
        .arg("-pp")
        .arg("-f")
        .arg("-l")
        .arg("patch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Logger::error(&format!("Failed to spawn bat for patch preview: {}", e));
            e
        })?;

    bat.stdin.as_mut().unwrap().write_all(patch.as_bytes())?;
    let output = bat.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}

#[allow(dead_code)]
fn delta_patch_renderer(patch: &str) -> color_eyre::Result<String> {
    let mut delta = Command::new("delta")
        .arg("--pager")
        .arg("less")
        .arg("--paging")
        .arg("never")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Logger::error(&format!("Failed to spawn delta for patch preview: {}", e));
            e
        })?;

    delta.stdin.as_mut().unwrap().write_all(patch.as_bytes())?;
    let output = delta.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}
