use std::path::Path;

use crate::config::env_overrides;
use crate::config::errors::ConfigError;
use crate::config::parsing::{parse_cover_renderer, parse_patch_renderer};
use crate::config::repository::{ConfigRepository, JsonConfigRepository};
use crate::config::state::ConfigSnapshot;
use crate::config::state::{normalize_derived_paths, ConfigState};
use crate::config::update::{ConfigUpdateDraft, ValidatedConfigUpdate};
use crate::infrastructure::{env::EnvTrait, file_system::FileSystemTrait};

/// Public surface for configuration (maps to a future `ConfigActor` protocol).
pub trait ConfigServiceApi: Send + Sync {
    fn snapshot(&self) -> ConfigSnapshot;
    fn validate_update(
        &self,
        draft: ConfigUpdateDraft,
    ) -> Result<ValidatedConfigUpdate, ConfigError>;
    fn apply_update(
        &mut self,
        update: ValidatedConfigUpdate,
    ) -> Result<ConfigSnapshot, ConfigError>;
}

pub struct ConfigService<FS: FileSystemTrait> {
    repo: JsonConfigRepository<FS>,
    state: ConfigState,
}

impl<FS: FileSystemTrait + Send + Sync> ConfigService<FS> {
    /// Bootstrap configuration: load file or defaults, persist, apply env overrides, ensure dirs.
    ///
    /// Loads file or defaults, saves, applies env overrides, ensures directories exist.
    pub fn bootstrap(env: &dyn EnvTrait, fs: FS) -> Result<Self, ConfigError> {
        let repo = JsonConfigRepository::new(env, fs);
        let mut state = Self::load_initial_state(env, &repo);
        if let Err(e) = repo.save(&state) {
            eprintln!("Failed to save default config: {e}");
        }
        env_overrides::apply_env_overrides(&mut state, env)?;
        normalize_derived_paths(&mut state);
        let service = Self { repo, state };
        service.ensure_directories()?;
        Ok(service)
    }

    fn load_initial_state(env: &dyn EnvTrait, repo: &JsonConfigRepository<FS>) -> ConfigState {
        let path = repo.config_path();
        let fs = repo.fs();
        if fs.is_file(Path::new(path)) {
            match fs.read_to_string(Path::new(path)) {
                Ok(file_contents) => match serde_json::from_str(&file_contents) {
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

    fn ensure_directories(&self) -> Result<(), ConfigError> {
        let fs = self.repo.fs();
        let paths = [
            self.state.cache_dir.as_str(),
            self.state.data_dir.as_str(),
            self.state.patchsets_cache_dir.as_str(),
            self.state.logs_path.as_str(),
        ];
        for path in paths {
            if fs.metadata(Path::new(path)).is_err() {
                fs.create_dir_all(Path::new(path))?;
            }
        }
        Ok(())
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

impl<FS: FileSystemTrait + Send + Sync> ConfigServiceApi for ConfigService<FS> {
    fn snapshot(&self) -> ConfigSnapshot {
        self.state.to_snapshot()
    }

    fn validate_update(
        &self,
        draft: ConfigUpdateDraft,
    ) -> Result<ValidatedConfigUpdate, ConfigError> {
        let fs = self.repo.fs();

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

        Ok(ValidatedConfigUpdate {
            page_size,
            cache_dir,
            data_dir,
            git_send_email_option,
            git_am_option,
            patch_renderer,
            cover_renderer,
            max_log_age,
        })
    }

    fn apply_update(
        &mut self,
        update: ValidatedConfigUpdate,
    ) -> Result<ConfigSnapshot, ConfigError> {
        self.state.apply_update(&update);
        normalize_derived_paths(&mut self.state);
        self.ensure_directories()?;
        self.repo.save(&self.state)?;
        Ok(self.state.to_snapshot())
    }
}
