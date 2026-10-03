use tracing::{event, Level};

use color_eyre::{eyre::eyre, Result};

use crate::{
    infrastructure::shell::{ShellCommand, ShellTrait},
    render_prefs::CoverRenderer,
};

pub struct CoverRendererService;

impl CoverRendererService {
    pub fn render_cover(
        shell: &dyn ShellTrait,
        raw: &str,
        renderer: &CoverRenderer,
    ) -> Result<String> {
        let text = match renderer {
            CoverRenderer::Default => Ok(raw.to_string()),
            CoverRenderer::Bat => Self::render_cover_with_bat(shell, raw),
        }?;

        Ok(text)
    }
}

impl CoverRendererService {
    /// Renders a .mbx cover using the `bat` command line tool.
    ///
    /// # Errors
    ///
    /// If bat isn't installed or if the command fails, an error will be returned.
    fn render_cover_with_bat(shell: &dyn ShellTrait, patch: &str) -> Result<String> {
        let cmd = ShellCommand::new("bat").args(["-pp", "-f", "-l", "mbx"]);

        let out = shell
            .execute_with_stdin(&cmd, patch.as_bytes())
            .map_err(|e| {
                event!(Level::ERROR, "Failed to spawn bat for cover preview: {}", e);
                eyre!(e)
            })?;

        Ok(String::from_utf8(out.stdout)?)
    }
}
