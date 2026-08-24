use std::path::Path;

use serde_json::from_str;

use crate::config::env_overrides;
use crate::config::errors::ConfigError;
use crate::config::parsing::{parse_cover_renderer, parse_patch_renderer};
use crate::config::repository::{ConfigRepository, JsonConfigRepository};
use crate::config::state::{normalize_derived_paths, ConfigState};
use crate::config::update::{ConfigUpdateDraft, ValidatedConfigUpdate};
use crate::infrastructure::{env::EnvTrait, file_system::FileSystemTrait};

/// Loads file or defaults, saves, applies env overrides, normalizes paths, and ensures directories.
pub(crate) fn bootstrap_parts<FS: FileSystemTrait>(
    env: &dyn EnvTrait,
    fs: FS,
) -> Result<(ConfigState, JsonConfigRepository<FS>), ConfigError> {
    let repo = JsonConfigRepository::new(env, fs);
    let mut state = load_initial_state(env, &repo);
    if let Err(e) = repo.save(&state) {
        eprintln!("Failed to save default config: {e}");
    }
    env_overrides::apply_env_overrides(&mut state, env)?;
    normalize_derived_paths(&mut state);
    ensure_directories(&state, repo.fs())?;
    Ok((state, repo))
}

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
            validate_dir(fs, t)?;
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
            validate_dir(fs, t)?;
            Some(t.to_string())
        }
    };

    let git_send_email_option = draft.git_send_email_option.clone();
    let git_am_option = draft.git_am_option.clone();

    let patch_renderer = match &draft.patch_renderer {
        None => None,
        Some(s) => Some(parse_patch_renderer(s)?),
    };

    let cover_renderer = match &draft.cover_renderer {
        None => None,
        Some(s) => Some(parse_cover_renderer(s)?),
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
    })
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
