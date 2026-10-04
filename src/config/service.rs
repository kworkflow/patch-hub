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
        let page_size = Self::parse_non_empty(
            &draft.page_size,
            |raw| Err(ConfigError::InvalidPageSize(raw.to_string())),
            |raw| {
                raw.trim()
                    .parse::<usize>()
                    .map_err(|_| ConfigError::InvalidPageSize(raw.to_string()))
            },
        )?;

        let cache_dir = Self::parse_non_empty(
            &draft.cache_dir,
            |_| {
                Err(ConfigError::InvalidDirectory(
                    "cache directory is empty".into(),
                ))
            },
            |raw| {
                let trimmed = raw.trim();
                Self::validate_dir(fs, trimmed)?;
                Ok(trimmed.to_string())
            },
        )?;

        let data_dir = Self::parse_non_empty(
            &draft.data_dir,
            |_| {
                Err(ConfigError::InvalidDirectory(
                    "data directory is empty".into(),
                ))
            },
            |raw| {
                let trimmed = raw.trim();
                Self::validate_dir(fs, trimmed)?;
                Ok(trimmed.to_string())
            },
        )?;

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

        let max_log_age = Self::parse_non_empty(
            &draft.max_log_age,
            |raw| Err(ConfigError::InvalidMaxLogAge(raw.to_string())),
            |raw| {
                raw.trim()
                    .parse::<usize>()
                    .map_err(|_| ConfigError::InvalidMaxLogAge(raw.to_string()))
            },
        )?;

        let stay_on_applied_branch = Self::parse_non_empty(
            &draft.stay_on_applied_branch,
            |raw| Err(ConfigError::InvalidStayOnAppliedBranch(raw.to_string())),
            |raw| {
                raw.trim()
                    .parse::<bool>()
                    .map_err(|_| ConfigError::InvalidStayOnAppliedBranch(raw.to_string()))
            },
        )?;

        let kw_reboot_after_deploy = Self::parse_non_empty(
            &draft.kw_reboot_after_deploy,
            |raw| Err(ConfigError::InvalidKwRebootAfterDeploy(raw.to_string())),
            |raw| {
                raw.trim()
                    .parse::<bool>()
                    .map_err(|_| ConfigError::InvalidKwRebootAfterDeploy(raw.to_string()))
            },
        )?;

        let kw_deploy_force = Self::parse_non_empty(
            &draft.kw_deploy_force,
            |raw| Err(ConfigError::InvalidKwDeployForce(raw.to_string())),
            |raw| {
                raw.trim()
                    .parse::<bool>()
                    .map_err(|_| ConfigError::InvalidKwDeployForce(raw.to_string()))
            },
        )?;

        let target_kernel_tree = Self::parse_non_empty(
            &draft.target_kernel_tree,
            |_| Ok(Some(None)),
            |raw| {
                let key = raw.trim();
                if current.kernel_trees.contains_key(key) {
                    Ok(Some(key.to_string()))
                } else {
                    Err(Self::reject_unknown_kernel_tree(raw, current))
                }
            },
        )?;

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

    fn parse_non_empty<T>(
        value: &Option<String>,
        on_blank: impl FnOnce(&str) -> Result<Option<T>, ConfigError>,
        on_value: impl FnOnce(&str) -> Result<T, ConfigError>,
    ) -> Result<Option<T>, ConfigError> {
        if let Some(raw) = value {
            if raw.trim().is_empty() {
                on_blank(raw)
            } else {
                on_value(raw).map(Some)
            }
        } else {
            Ok(None)
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
