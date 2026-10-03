use std::path::Path;

use serde_json::from_str;

use crate::config::env_overrides;
use crate::config::errors::ConfigError;
use crate::config::parsing::ConfigParsingService;
use crate::config::{
    ConfigRepository, ConfigState, ConfigUpdateDraft, JsonConfigRepository, ValidatedConfigUpdate,
};
use crate::infrastructure::{env::EnvTrait, file_system::FileSystemTrait};

pub(crate) struct ConfigService;

impl ConfigService {
    /// Loads file or defaults, saves, applies env overrides, normalizes paths, and ensures directories.
    pub(crate) fn bootstrap_parts<FS: FileSystemTrait>(
        env: &dyn EnvTrait,
        fs: FS,
    ) -> Result<(ConfigState, JsonConfigRepository<FS>), ConfigError> {
        let repo = JsonConfigRepository::new(env, fs);
        let mut state = Self::load_initial_state(env, &repo);
        if let Err(e) = repo.save(&state) {
            eprintln!("Failed to save default config: {e}");
        }
        env_overrides::EnvOverrideService::apply_env_overrides(&mut state, env)?;
        state.normalize_derived_paths();
        Self::ensure_directories(&state, repo.fs())?;
        Ok((state, repo))
    }

    pub(crate) fn ensure_directories(
        state: &ConfigState,
        fs: &dyn FileSystemTrait,
    ) -> Result<(), ConfigError> {
        let paths = [
            state.cache_dir.as_str(),
            state.data_dir.as_str(),
            state.patchsets_cache_dir.as_str(),
            state.logs_path.as_str(),
        ];
        for path in paths {
            if fs.metadata(Path::new(path)).is_err() {
                fs.create_dir_all(Path::new(path))?;
            }
        }
        Ok(())
    }

    pub(crate) fn validate_update(
        draft: ConfigUpdateDraft,
        fs: &dyn FileSystemTrait,
        current: &ConfigState,
    ) -> Result<ValidatedConfigUpdate, ConfigError> {
        let page_size = match &draft.page_size {
            None => None,
            Some(s) if s.trim().is_empty() => {
                return Err(ConfigError::InvalidPageSize(s.clone()));
            }
            Some(s) => Some(
                s.trim()
                    .parse::<usize>()
                    .map_err(|_| ConfigError::InvalidPageSize(s.clone()))?,
            ),
        };

        let cache_dir = match &draft.cache_dir {
            None => None,
            Some(s) if s.trim().is_empty() => {
                return Err(ConfigError::InvalidDirectory(
                    "cache directory is empty".into(),
                ));
            }
            Some(s) => {
                let t = s.trim();
                Self::validate_dir(fs, t)?;
                Some(t.to_string())
            }
        };

        let data_dir = match &draft.data_dir {
            None => None,
            Some(s) if s.trim().is_empty() => {
                return Err(ConfigError::InvalidDirectory(
                    "data directory is empty".into(),
                ));
            }
            Some(s) => {
                let t = s.trim();
                Self::validate_dir(fs, t)?;
                Some(t.to_string())
            }
        };

        let git_send_email_option = draft.git_send_email_option.clone();
        let git_am_option = draft.git_am_option.clone();

        let patch_renderer = match &draft.patch_renderer {
            None => None,
            Some(s) => Some(ConfigParsingService::parse_patch_renderer(s)?),
        };

        let cover_renderer = match &draft.cover_renderer {
            None => None,
            Some(s) => Some(ConfigParsingService::parse_cover_renderer(s)?),
        };

        let max_log_age = match &draft.max_log_age {
            None => None,
            Some(s) if s.trim().is_empty() => {
                return Err(ConfigError::InvalidMaxLogAge(s.clone()));
            }
            Some(s) => Some(
                s.trim()
                    .parse::<usize>()
                    .map_err(|_| ConfigError::InvalidMaxLogAge(s.clone()))?,
            ),
        };

        let stay_on_applied_branch = match &draft.stay_on_applied_branch {
            None => None,
            Some(s) if s.trim().is_empty() => {
                return Err(ConfigError::InvalidStayOnAppliedBranch(s.clone()));
            }
            Some(s) => Some(
                s.trim()
                    .parse::<bool>()
                    .map_err(|_| ConfigError::InvalidStayOnAppliedBranch(s.clone()))?,
            ),
        };

        let kw_reboot_after_deploy = match &draft.kw_reboot_after_deploy {
            None => None,
            Some(s) if s.trim().is_empty() => {
                return Err(ConfigError::InvalidKwRebootAfterDeploy(s.clone()));
            }
            Some(s) => Some(
                s.trim()
                    .parse::<bool>()
                    .map_err(|_| ConfigError::InvalidKwRebootAfterDeploy(s.clone()))?,
            ),
        };

        let kw_deploy_force = match &draft.kw_deploy_force {
            None => None,
            Some(s) if s.trim().is_empty() => {
                return Err(ConfigError::InvalidKwDeployForce(s.clone()));
            }
            Some(s) => Some(
                s.trim()
                    .parse::<bool>()
                    .map_err(|_| ConfigError::InvalidKwDeployForce(s.clone()))?,
            ),
        };

        let target_kernel_tree = match &draft.target_kernel_tree {
            None => None,
            Some(s) if s.trim().is_empty() => Some(None),
            Some(s) => {
                let key = s.trim();
                if current.kernel_trees.contains_key(key) {
                    Some(Some(key.to_string()))
                } else {
                    return Err(Self::reject_unknown_kernel_tree(s, current));
                }
            }
        };

        Ok(ValidatedConfigUpdate {
            page_size,
            cache_dir,
            data_dir,
            git_send_email_option,
            git_am_option,
            patch_renderer,
            cover_renderer,
            max_log_age,
            stay_on_applied_branch,
            kw_reboot_after_deploy,
            kw_deploy_force,
            target_kernel_tree,
        })
    }
}

impl ConfigService {
    fn load_initial_state<FS: FileSystemTrait>(
        env: &dyn EnvTrait,
        repo: &JsonConfigRepository<FS>,
    ) -> ConfigState {
        let path = repo.config_path();
        let fs = repo.fs();
        if fs.is_file(Path::new(path)) {
            match fs.read_to_string(Path::new(path)) {
                Ok(file_contents) => match from_str(&file_contents) {
                    Ok(config) => return config,
                    Err(e) => eprintln!("Failed to parse config file {path}: {e}"),
                },
                Err(e) => {
                    eprintln!("Failed to read config file {path}: {e}");
                }
            }
        }
        ConfigState::new_with_defaults(env)
    }

    fn reject_unknown_kernel_tree(raw: &str, current: &ConfigState) -> ConfigError {
        let mut keys: Vec<String> = current.kernel_trees.keys().cloned().collect();
        keys.sort();
        let hint = if keys.is_empty() {
            "no kernel trees are configured; unset the target or add trees in the config file"
                .to_string()
        } else {
            format!(
                "known keys: {}; unset the target or pick one of these",
                keys.join(", ")
            )
        };
        ConfigError::InvalidTargetKernelTree {
            key: raw.to_string(),
            hint,
        }
    }

    fn validate_dir(fs: &dyn FileSystemTrait, dir_path: &str) -> Result<(), ConfigError> {
        let path = Path::new(dir_path);
        if fs.exists(path) && fs.is_dir(path) {
            return Ok(());
        }
        fs.create_dir_all(path)
            .map_err(|_| ConfigError::InvalidDirectory(dir_path.to_string()))?;
        Ok(())
    }
}
