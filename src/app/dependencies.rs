use tracing::{event, Level};

use crate::{
    app::errors::AppError, config::ConfigSnapshot, infrastructure::env::EnvTrait,
    render_prefs::PatchRenderer,
};

/// Verifies required and optional external binaries before the terminal starts.
///
/// A missing `b4` is a hard failure; all other missing binaries only emit
/// warnings. This keeps fatal startup failures out of terminal raw mode.
pub(crate) fn check_external_deps(
    env: &dyn EnvTrait,
    config: &ConfigSnapshot,
) -> Result<(), AppError> {
    if !env.which("b4") {
        event!(
            Level::ERROR,
            "b4 is not installed, patchsets cannot be downloaded"
        );
        return Err(AppError::Dependencies(
            "b4 is not installed; patchsets cannot be downloaded".to_string(),
        ));
    }

    if !env.which("git") {
        event!(Level::WARN, "git is not installed, send-email won't work");
    }

    match config.patch_renderer() {
        PatchRenderer::Bat => {
            if !env.which("bat") {
                event!(
                    Level::WARN,
                    "bat is not installed, patch rendering will fallback to default"
                );
            }
        }
        PatchRenderer::Delta => {
            if !env.which("delta") {
                event!(
                    Level::WARN,
                    "delta is not installed, patch rendering will fallback to default",
                );
            }
        }
        PatchRenderer::DiffSoFancy => {
            if !env.which("diff-so-fancy") {
                event!(
                    Level::WARN,
                    "diff-so-fancy is not installed, patch rendering will fallback to default",
                );
            }
        }
        _ => {}
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{ConfigState, ValidatedConfigUpdate},
        infrastructure::env::MockEnvTrait,
        render_prefs::PatchRenderer,
    };

    use super::*;

    #[test]
    fn missing_b4_returns_dependencies_error() {
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "b4")
            .returning(|_| false);

        let err = check_external_deps(&env, &ConfigState::default().to_snapshot()).unwrap_err();

        assert!(matches!(err, AppError::Dependencies(_)));
    }

    #[test]
    fn missing_git_is_not_fatal() {
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "b4")
            .returning(|_| true);
        env.expect_which()
            .withf(|name| name == "git")
            .returning(|_| false);

        let result = check_external_deps(&env, &ConfigState::default().to_snapshot());

        assert!(result.is_ok());
    }

    #[test]
    fn missing_configured_renderer_is_not_fatal() {
        let mut state = ConfigState::default();
        state.apply_update(&ValidatedConfigUpdate {
            patch_renderer: Some(PatchRenderer::Bat),
            ..Default::default()
        });

        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "b4")
            .returning(|_| true);
        env.expect_which()
            .withf(|name| name == "git")
            .returning(|_| true);
        env.expect_which()
            .withf(|name| name == "bat")
            .returning(|_| false);

        let result = check_external_deps(&env, &state.to_snapshot());

        assert!(result.is_ok());
    }
}
