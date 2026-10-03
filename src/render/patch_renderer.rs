use color_eyre::{eyre::eyre, Result};
use tracing::{event, Level};

use crate::{
    infrastructure::shell::{ShellCommand, ShellTrait},
    render_prefs::PatchRenderer,
};

pub struct PatchRendererService;

impl PatchRendererService {
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

    pub fn render_patch_preview(
        shell: &dyn ShellTrait,
        raw: &str,
        renderer: &PatchRenderer,
    ) -> Result<String> {
        let text = match renderer {
            PatchRenderer::Default => Ok(raw.to_string()),
            PatchRenderer::Bat => Self::render_patch_with_bat(shell, raw),
            PatchRenderer::Delta => Self::render_patch_with_delta(shell, raw),
            PatchRenderer::DiffSoFancy => Self::render_patch_with_diff_so_fancy(shell, raw),
        }?;

        Ok(text)
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
    fn render_patch_with_bat(shell: &dyn ShellTrait, patch: &str) -> Result<String> {
        let cleaned_patch = Self::clean_patch_for_preview(patch);

        let cmd = ShellCommand::new("bat").args(["-pp", "-f", "-l", "patch"]);

        let out = shell
            .execute_with_stdin(&cmd, cleaned_patch.as_bytes())
            .map_err(|e| {
                event!(Level::ERROR, "Failed to spawn bat for patch preview: {}", e);
                eyre!(e)
            })?;

        Ok(String::from_utf8(out.stdout)?)
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
    fn render_patch_with_delta(shell: &dyn ShellTrait, patch: &str) -> Result<String> {
        let cleaned_patch = Self::clean_patch_for_preview(patch);

        let cmd = ShellCommand::new("delta").args([
            "--pager",
            "less",
            "--no-gitconfig",
            "--paging",
            "never",
            "-w",
            "130",
        ]);

        let out = shell
            .execute_with_stdin(&cmd, cleaned_patch.as_bytes())
            .map_err(|e| {
                event!(
                    Level::ERROR,
                    "Failed to spawn delta for patch preview: {}",
                    e
                );
                eyre!(e)
            })?;

        Ok(String::from_utf8(out.stdout)?)
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
    fn render_patch_with_diff_so_fancy(shell: &dyn ShellTrait, patch: &str) -> Result<String> {
        let cleaned_patch = Self::clean_patch_for_preview(patch);

        let cmd = ShellCommand::new("diff-so-fancy");

        let out = shell
            .execute_with_stdin(&cmd, cleaned_patch.as_bytes())
            .map_err(|e| {
                event!(
                    Level::ERROR,
                    "Failed to spawn diff-so-fancy for patch preview: {}",
                    e
                );
                eyre!(e)
            })?;

        Ok(String::from_utf8(out.stdout)?)
    }
}
