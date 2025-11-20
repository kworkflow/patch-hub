use serde::{Deserialize, Serialize};

use std::{
    fmt::Display,
    io::Write,
    process::{Command, Stdio},
};

use crate::infrastructure::logging::Logger;

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

pub fn render_cover(raw: &str, renderer: &CoverRenderer) -> color_eyre::Result<String> {
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
        .map_err(|e| {
            Logger::error(format!("Failed to spawn bat for cover preview: {e}"));
            e
        })?;

    bat.stdin.as_mut().unwrap().write_all(patch.as_bytes())?;
    let output = bat.wait_with_output()?;
    Ok(String::from_utf8(output.stdout)?)
}

#[cfg(test)]
mod tests {
    use crate::app::config::Config;

    use super::*;

    #[test]
    fn test_cover_renderer_from_string() {
        assert!(matches!(CoverRenderer::from("bat".to_string()), CoverRenderer::Bat));
        assert!(matches!(CoverRenderer::from("default".to_string()), CoverRenderer::Default));
        assert!(matches!(CoverRenderer::from("qualquer".to_string()), CoverRenderer::Default));
    }

    #[test]
    fn test_cover_renderer_from_str() {
        assert!(matches!(CoverRenderer::from("bat"), CoverRenderer::Bat));
        assert!(matches!(CoverRenderer::from("default"), CoverRenderer::Default));
        assert!(matches!(CoverRenderer::from("zzz"), CoverRenderer::Default));
    }

    #[test]
    fn test_cover_renderer_display() {
        assert_eq!(CoverRenderer::Default.to_string(), "default");
        assert_eq!(CoverRenderer::Bat.to_string(), "bat");
    }

    #[test]
    fn test_render_cover_default() {
        let text = "abc";
        let result = render_cover(text, &CoverRenderer::Default).unwrap();
        assert_eq!(result, "abc");
    }

    #[test]
    fn test_bat_renderer_fails_when_bat_missing() {
        let cfg = Config::default();
        Logger::init_log_file(&cfg).ok();

        let backup = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", "");

        let result = bat_cover_renderer("x");

        std::env::set_var("PATH", backup);

        assert!(result.is_err());
    }
}

