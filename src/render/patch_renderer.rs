use color_eyre::{eyre::eyre, Result};
use tracing::{event, Level};

use crate::{
    infrastructure::shell::{ShellCommand, ShellTrait},
    render_prefs::PatchRenderer,
};

pub struct PatchRendererService;

impl PatchRendererService {
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
}

impl PatchRendererService {
    /// Cleans patch contents before rendering for preview. Currently, it only trims
    /// the trailing signature delimiter (the `--` at the end of the patch) if it
    /// exists, as it is incorrectly rendered as a deletion by diff renderers.
    fn clean_patch_for_preview(patch: &str) -> String {
        let lines = patch.lines().collect::<Vec<&str>>();

        if let Some(sig_pos) = lines.iter().position(|&line| line.trim() == "--") {
            lines[..sig_pos].join("\n")
        } else {
            patch.to_string()
        }
    }

    /// Renders a patch with `bat`. Returns an error when bat is missing or
    /// the command fails.
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

    /// Renders a patch with `delta`. Returns an error when delta is missing
    /// or the command fails.
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

    /// Renders a patch with `diff-so-fancy`. Returns an error when the tool
    /// is missing or the command fails.
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
