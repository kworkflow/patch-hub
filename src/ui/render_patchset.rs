use std::{
    io::Write,
    process::{Command, Stdio},
};

use ansi_to_tui::IntoText;
use ratatui::text::Text;

use crate::app::{logging::Logger, patch_renderer::PatchRenderer};

pub fn render_patch_preview(
    raw: &str,
    renderer: PatchRenderer,
) -> color_eyre::Result<Text<'static>> {
    let text = match renderer {
        PatchRenderer::Default => Ok(raw.to_string()),
        PatchRenderer::Bat => bat_patch_renderer(raw),
        PatchRenderer::Delta => delta_patch_renderer(raw),
        PatchRenderer::DiffSoFancy => diff_so_fancy_renderer(raw),
    }?
    .into_text()?;

    Ok(text)
}

/// Renders a patch using the `bat` command line tool.
///
/// # Errors
///
/// If bat isn't installed or if the command fails, an error will be returned.
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

/// Renders a patch using the `delta` command line tool.
///
/// # Errors
///
/// If delta isn't installed or if the command fails, an error will be returned.
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

/// Renders a patch using the `diff-so-fancy` command line tool.
///
/// # Errors
///
/// If diff-so-fancy isn't installed or if the command fails, an error will be returned.
fn diff_so_fancy_renderer(patch: &str) -> color_eyre::Result<String> {
    let mut dsf = Command::new("diff-so-fancy")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Logger::error(&format!(
                "Failed to spawn diff-so-fancy for patch preview: {}",
                e
            ));
            e
        })?;

    dsf.stdin.as_mut().unwrap().write_all(patch.as_bytes())?;
    let output = dsf.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "optional-dependency"]
    fn test_bat_patch_renderer() {
        let patch = "\
        diff --git a/file.txt b/file.txt
        index 83db48f..e3b0c44 100644
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -Hello, world!
        +Hello, Rust!
        ";
        let result = bat_patch_renderer(patch);
        assert!(result.is_ok());
        let rendered_patch = result.unwrap();
        assert!(rendered_patch.contains("Hello, Rust!"));
    }

    #[test]
    #[ignore = "optional-dependency"]
    fn test_delta_patch_renderer() {
        let patch = "\
        diff --git a/file.txt b/file.txt
        index 83db48f..e3b0c44 100644
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -Hello, world!
        +Hello, Rust!
        ";
        let result = delta_patch_renderer(patch);
        assert!(result.is_ok());
        let rendered_patch = result.unwrap();
        assert!(rendered_patch.contains("Hello, Rust!"));
    }

    #[test]
    #[ignore = "optional-dependency"]
    fn test_diff_so_fancy_renderer() {
        let patch = "\
        diff --git a/file.txt b/file.txt
        index 83db48f..e3b0c44 100644
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -Hello, world!
        +Hello, Rust!
        ";
        let result = diff_so_fancy_renderer(patch);
        assert!(result.is_ok());
        let rendered_patch = result.unwrap();
        assert!(rendered_patch.contains("Hello, Rust!"));
    }
}
