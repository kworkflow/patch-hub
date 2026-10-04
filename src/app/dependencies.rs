use tracing::{event, Level};

use crate::{
    app::errors::AppError,
    config::ConfigSnapshot,
    infrastructure::{env::EnvTrait, shell::ShellTrait},
    kw::{models::readiness::KwVersionCheck, readiness::ReadinessService},
    render_prefs::PatchRenderer,
};

pub(crate) struct DependencyService;

impl DependencyService {
    /// Verifies external binaries before the terminal starts.
    ///
    /// A missing `b4` is fatal. Other misses, including `kw` and an
    /// unverifiable version, only warn: kw's VERSION file reports `beta-0.9`
    /// even at the 0.10 tag, and fatal checks must stay out of raw mode.
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

        let kw = ReadinessService::probe_kw_binary(env, shell);
        let kw_version_unconfirmed = match &kw.check {
            KwVersionCheck::Meets => false,
            KwVersionCheck::Below(_) | KwVersionCheck::Unknown => true,
        };
        if !kw.available {
            event!(
                Level::WARN,
                "kw is not installed, kernel build/deploy won't work"
            );
        } else if kw_version_unconfirmed {
            event!(
                Level::WARN,
                version = kw.version_line.as_deref().unwrap_or("unknown"),
                "could not confirm kw >= 0.10; the build/deploy integration is \
             verified against kw 0.10 (kw's own VERSION file may be stale)"
            );
        }

        Ok(())
    }
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
        env.expect_which()
            .withf(|name| matches!(name, "b4" | "bat" | "git" | "kw"))
            .times(3..=4)
            .returning(|_| true);
        let mut shell = MockShellTrait::new();
        shell
            .expect_execute()
            .withf(|cmd| cmd.program == "kw" && cmd.args == ["--version"])
            .times(1)
            .returning(|_| {
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
            .times(1)
            .returning(|_| false);
        let mut shell = MockShellTrait::new();
        shell.expect_execute().withf(|_| true).times(0);

        let err = DependencyService::check_external_deps(
            &env,
            &shell,
            &ConfigSnapshot::from(&ConfigState::default()),
        )
        .unwrap_err();

        assert!(matches!(err, AppError::Dependencies(_)));
    }

    #[test]
    fn missing_git_is_not_fatal() {
        let (mut env, shell) = happy_env();
        env.expect_which()
            .withf(|name| name == "git")
            .times(0)
            .returning(|_| false);

        let result = DependencyService::check_external_deps(
            &env,
            &shell,
            &ConfigSnapshot::from(&ConfigState::default()),
        );

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
            .times(0)
            .returning(|_| false);

        let result =
            DependencyService::check_external_deps(&env, &shell, &ConfigSnapshot::from(&state));

        assert!(result.is_ok());
    }

    #[test]
    fn missing_kw_is_not_fatal() {
        let (mut env, mut shell) = happy_env();
        env.expect_which()
            .withf(|name| name == "kw")
            .times(0)
            .returning(|_| false);
        // No kw on PATH: the version probe must not spawn anything.
        shell.expect_execute().withf(|_| true).times(0);

        let result = DependencyService::check_external_deps(
            &env,
            &shell,
            &ConfigSnapshot::from(&ConfigState::default()),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn unverifiable_kw_version_is_not_fatal() {
        let (env, mut shell) = happy_env();
        // Real 0.10 installs can still report the stale beta-0.9: the floor
        // check stays a warning regardless of what kw answers.
        shell
            .expect_execute()
            .withf(|_| true)
            .times(0)
            .returning(|_| {
                Ok(ShellOutput {
                    stdout: b"beta-0.9\nBranch: master\nCommit: 3575d38\n".to_vec(),
                    stderr: Vec::new(),
                    success: true,
                })
            });

        let result = DependencyService::check_external_deps(
            &env,
            &shell,
            &ConfigSnapshot::from(&ConfigState::default()),
        );

        assert!(result.is_ok());
    }
}
