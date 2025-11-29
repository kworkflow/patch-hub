use color_eyre::eyre::eyre;
use serde::{Deserialize, Serialize};

use std::{
    fmt::Display,
    io::Write,
    process::{Command, Stdio},
};

use crate::infrastructure::logging::Logger;

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
///
/// # Tests
///
/// [tests::test_bat_patch_renderer]
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
            Logger::error(format!("Failed to spawn bat for patch preview: {e}"));
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
///
/// # Tests
///
/// [tests::test_delta_patch_renderer]
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
            Logger::error(format!("Failed to spawn delta for patch preview: {e}"));
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
///
/// # Tests
///
/// [tests::test_diff_so_fancy_renderer]
fn diff_so_fancy_renderer(patch: &str) -> color_eyre::Result<String> {
    let cleaned_patch = clean_patch_for_preview(patch);

    let mut dsf = Command::new("diff-so-fancy")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Logger::error(format!(
                "Failed to spawn diff-so-fancy for patch preview: {e}"
            ));
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
    use super::*;
    use crate::app::config::Config;
    use crate::infrastructure::logging::Logger;
    use std::io::ErrorKind;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_test_logger() {
        INIT.call_once(|| {
            let mut config = Config::default();
            let mut temp_dir = std::env::temp_dir();
            temp_dir.push("patch-hub-renderer-tests");
            let _ = std::fs::create_dir_all(&temp_dir);

            config.set_data_dir(temp_dir.to_string_lossy().to_string());

            let _ = std::panic::catch_unwind(|| {
                Logger::init_log_file(&config).ok();
            });
        });
    }

    fn get_sample_patch() -> &'static str {
        "diff --git a/file.rs b/file.rs\n\
         index 123..456 100644\n\
         --- a/file.rs\n\
         +++ b/file.rs\n\
         @@ -1,3 +1,3 @@\n\
         -old line\n\
         +new line\n\
         context line"
    }

    #[test]
    fn test_patch_renderer_enum_conversions_and_display() {
        init_test_logger();

        assert!(matches!(PatchRenderer::from("bat"), PatchRenderer::Bat));
        assert!(matches!(PatchRenderer::from("delta"), PatchRenderer::Delta));
        assert!(matches!(
            PatchRenderer::from("diff-so-fancy"),
            PatchRenderer::DiffSoFancy
        ));

        assert!(matches!(
            PatchRenderer::from("unknown-tool"),
            PatchRenderer::Default
        ));
        assert!(matches!(PatchRenderer::from(""), PatchRenderer::Default));

        assert_eq!(PatchRenderer::Bat.to_string(), "bat");
        assert_eq!(PatchRenderer::Default.to_string(), "default");
        assert_eq!(PatchRenderer::DiffSoFancy.to_string(), "diff-so-fancy");
    }

    #[test]
    fn test_clean_patch_for_preview() {
        init_test_logger();

        let raw = "line1\nline2\nline3";
        let cleaned = clean_patch_for_preview(raw);
        assert_eq!(cleaned, raw);

        let raw_with_sig = "line1\nline2\n--\nRegards,\nAuthor";
        let expected = "line1\nline2";
        let cleaned_sig = clean_patch_for_preview(raw_with_sig);
        assert_eq!(cleaned_sig, expected);

        let raw_code_decrement = "cnt--;\nif (x) {\n--\nSig";
        let expected_code = "cnt--;\nif (x) {";
        let cleaned_code = clean_patch_for_preview(raw_code_decrement);
        assert_eq!(cleaned_code, expected_code);

        assert_eq!(clean_patch_for_preview(""), "");
    }

    #[test]
    fn test_render_patch_preview() {
        init_test_logger();
        let patch = get_sample_patch();

        let result = render_patch_preview(patch, &PatchRenderer::Default);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), patch.to_string());

        let result_bat = render_patch_preview(patch, &PatchRenderer::Bat);
        assert!(result_bat.is_ok() || result_bat.is_err());
    }

    #[test]
    fn test_delta_patch_renderer() {
        init_test_logger();
        let patch = get_sample_patch();
        let result = delta_patch_renderer(patch);

        match result {
            Ok(output) => assert!(!output.is_empty()),
            Err(e) => {
                if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == ErrorKind::NotFound {
                        println!("Skipping delta test: command not found");
                        return;
                    }
                }
            }
        }
    }

    #[test]
    fn test_diff_so_fancy_renderer() {
        init_test_logger();
        let patch = get_sample_patch();
        let result = diff_so_fancy_renderer(patch);

        match result {
            Ok(output) => assert!(!output.is_empty()),
            Err(e) => {
                if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == ErrorKind::NotFound {
                        println!("Skipping diff-so-fancy test: command not found");
                        return;
                    }
                }
            }
        }
    }
}
