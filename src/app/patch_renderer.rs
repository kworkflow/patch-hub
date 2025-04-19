use std::{
    fmt::Display,
    io::Write,
    process::{Command, Stdio},
};

use color_eyre::eyre::eyre;
use serde::{Deserialize, Serialize};
use tracing::{event, Level};

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

pub fn render_patch_preview(raw: &str, renderer: &PatchRenderer) -> color_eyre::Result<String> {
    let text = match renderer {
        PatchRenderer::Default => Ok(raw.to_string()),
        PatchRenderer::Bat => bat_patch_renderer(raw),
        PatchRenderer::Delta => delta_patch_renderer(raw),
        PatchRenderer::DiffSoFancy => diff_so_fancy_renderer(raw),
    }?;

    Ok(text)
}

/// Cleans patch contents before rendering for preview. Currently, it only trims
/// the trailing signature delimiter (the `--` at the end of the patch) if it
/// exists, as it is incorrectly rendered as a deletion by diff renderers.
fn clean_patch_for_preview(patch: &str) -> String {
    let lines: Vec<&str> = patch.lines().collect();

    if let Some(sig_pos) = lines.iter().position(|&line| line.trim() == "--") {
        lines[..sig_pos].join("\n")
    } else {
        patch.to_string()
    }
}

/// Renders a patch using the `bat` command line tool.
///
/// # Errors
///
/// If bat isn't installed or if the command fails, an error will be returned.
fn bat_patch_renderer(patch: &str) -> color_eyre::Result<String> {
    let cleaned_patch = clean_patch_for_preview(patch);

    let mut bat = Command::new("bat")
        .arg("-pp")
        .arg("-f")
        .arg("-l")
        .arg("patch")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            event!(Level::ERROR, "Failed to spawn bat for patch preview: {}", e);
            e
        })?;

    bat.stdin
        .as_mut()
        .ok_or_else(|| eyre!("Failed to get stdin handle"))?
        .write_all(cleaned_patch.as_bytes())?;
    let output = bat.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}

/// Renders a patch using the `delta` command line tool.
///
/// # Errors
///
/// If delta isn't installed or if the command fails, an error will be returned.
fn delta_patch_renderer(patch: &str) -> color_eyre::Result<String> {
    let cleaned_patch = clean_patch_for_preview(patch);

    let mut delta = Command::new("delta")
        .arg("--pager")
        .arg("less")
        .arg("--no-gitconfig")
        .arg("--paging")
        .arg("never")
        .arg("-w")
        .arg("130")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            event!(
                Level::ERROR,
                "Failed to spawn delta for patch preview: {}",
                e
            );
            e
        })?;

    delta
        .stdin
        .as_mut()
        .ok_or_else(|| eyre!("Failed to get stdin handle"))?
        .write_all(cleaned_patch.as_bytes())?;
    let output = delta.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}

/// Renders a patch using the `diff-so-fancy` command line tool.
///
/// # Errors
///
/// If diff-so-fancy isn't installed or if the command fails, an error will be returned.
fn diff_so_fancy_renderer(patch: &str) -> color_eyre::Result<String> {
    let cleaned_patch = clean_patch_for_preview(patch);

    let mut dsf = Command::new("diff-so-fancy")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            event!(
                Level::ERROR,
                "Failed to spawn diff-so-fancy for patch preview: {}",
                e
            );
            e
        })?;

    dsf.stdin
        .as_mut()
        .ok_or_else(|| eyre!("Failed to get stdin handle"))?
        .write_all(cleaned_patch.as_bytes())?;
    let output = dsf.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const PATCH_SAMPLE: &str = "diff --git a/file.txt b/file.txt
index 83db48f..e3b0c44 100644
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-Hello, world!
+Hello, Rust!
";

    #[test]
    #[ignore = "optional-dependency"]
    fn test_bat_patch_renderer() {
        let result = bat_patch_renderer(PATCH_SAMPLE);
        assert!(result.is_ok());
        let rendered_patch = result.unwrap();
        assert_eq!(
            fs::read_to_string("src/test_samples/ui/render_patchset/expected_bat.diff").unwrap(),
            rendered_patch,
            "Wrong rendering of bat"
        );
    }

    #[test]
    #[ignore = "optional-dependency"]
    fn test_delta_patch_renderer() {
        let result = delta_patch_renderer(PATCH_SAMPLE);
        assert!(result.is_ok());
        let rendered_patch = result.unwrap();
        assert_eq!(
            fs::read_to_string("src/test_samples/ui/render_patchset/expected_delta.diff").unwrap(),
            rendered_patch,
            "Wrong rendering of delta"
        );
    }

    #[test]
    #[ignore = "optional-dependency"]
    fn test_diff_so_fancy_renderer() {
        let result = diff_so_fancy_renderer(PATCH_SAMPLE);
        assert!(result.is_ok());
        let rendered_patch = result.unwrap();
        assert_eq!(
            fs::read_to_string("src/test_samples/ui/render_patchset/expected_diff-so-fancy.diff")
                .unwrap(),
            rendered_patch,
            "Wrong rendering of diff-so-fancy"
        );
    }
}
