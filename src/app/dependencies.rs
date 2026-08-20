use tracing::{event, Level};

use crate::{
    app::errors::AppError,
    config::ConfigSnapshot,
    infrastructure::{env::EnvTrait, shell::ShellTrait},
    kw::readiness::{probe_kw_binary, KwVersionCheck},
    render_prefs::PatchRenderer,
};

/// Verifies required and optional external binaries before the terminal starts.
///
/// A missing `b4` is a hard failure; all other missing binaries only emit
/// warnings — including `kw`, and including an unverifiable kw version,
/// since kw's own VERSION file is stale upstream (it reports `beta-0.9`
/// even at the 0.10 tag). This keeps fatal startup failures out of
/// terminal raw mode.
pub(crate) fn check_external_deps(
    env: &dyn EnvTrait,
    shell: &dyn ShellTrait,
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

    let kw = probe_kw_binary(env, shell);
    if !kw.available {
        event!(
            Level::WARN,
            "kw is not installed, kernel build/deploy won't work"
        );
    } else if !matches!(kw.check, KwVersionCheck::Meets) {
        event!(
            Level::WARN,
            version = kw.version_line.as_deref().unwrap_or("unknown"),
            "could not confirm kw >= 0.10; the build/deploy integration is \
             verified against kw 0.10 (kw's own VERSION file may be stale)"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{ConfigState, ValidatedConfigUpdate},
        infrastructure::{
            env::MockEnvTrait,
            shell::{MockShellTrait, ShellOutput},
        },
        render_prefs::PatchRenderer,
    };

    use super::*;

    /// An env where every binary is present and kw reports a current
    /// version, so individual tests only need to override their own case.
    fn happy_env() -> (MockEnvTrait, MockShellTrait) {
        let mut env = MockEnvTrait::new();
        env.expect_which().returning(|_| true);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().returning(|_| {
            Ok(ShellOutput {
                stdout: b"0.10.0\n".to_vec(),
                stderr: Vec::new(),
                success: true,
            })
        });
        (env, shell)
    }

    #[test]
    fn missing_b4_returns_dependencies_error() {
        let mut env = MockEnvTrait::new();
        env.expect_which()
            .withf(|name| name == "b4")
            .returning(|_| false);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().times(0);

        let err =
            check_external_deps(&env, &shell, &ConfigState::default().to_snapshot()).unwrap_err();

        assert!(matches!(err, AppError::Dependencies(_)));
    }

    #[test]
    fn missing_git_is_not_fatal() {
        let (mut env, shell) = happy_env();
        env.expect_which()
            .withf(|name| name == "git")
            .returning(|_| false);

        let result = check_external_deps(&env, &shell, &ConfigState::default().to_snapshot());

        assert!(result.is_ok());
    }

    #[test]
    fn missing_configured_renderer_is_not_fatal() {
        let mut state = ConfigState::default();
        state.apply_update(&ValidatedConfigUpdate {
            patch_renderer: Some(PatchRenderer::Bat),
            ..Default::default()
        });

        let (mut env, shell) = happy_env();
        env.expect_which()
            .withf(|name| name == "bat")
            .returning(|_| false);

        let result = check_external_deps(&env, &shell, &state.to_snapshot());

        assert!(result.is_ok());
    }

    #[test]
    fn missing_kw_is_not_fatal() {
        let (mut env, mut shell) = happy_env();
        env.expect_which()
            .withf(|name| name == "kw")
            .returning(|_| false);
        // No kw on PATH: the version probe must not spawn anything.
        shell.expect_execute().times(0);

        let result = check_external_deps(&env, &shell, &ConfigState::default().to_snapshot());

        assert!(result.is_ok());
    }

    #[test]
    fn unverifiable_kw_version_is_not_fatal() {
        let (env, mut shell) = happy_env();
        // Real 0.10 installs can still report the stale beta-0.9: the floor
        // check stays a warning regardless of what kw answers.
        shell.expect_execute().returning(|_| {
            Ok(ShellOutput {
                stdout: b"beta-0.9\nBranch: master\nCommit: 3575d38\n".to_vec(),
                stderr: Vec::new(),
                success: true,
            })
        });

        let result = check_external_deps(&env, &shell, &ConfigState::default().to_snapshot());

        assert!(result.is_ok());
    }
}
