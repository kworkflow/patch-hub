use std::path::Path;

use crate::config::env_overrides;
use crate::config::errors::ConfigError;
use crate::config::repository::{ConfigRepository, JsonConfigRepository};
use crate::config::state::ConfigSnapshot;
use crate::config::state::{normalize_derived_paths, ConfigState};
use crate::config::update::ConfigUpdateDraft;
use crate::infrastructure::{env::EnvTrait, file_system::FileSystemTrait};

/// Public surface for configuration (maps to a future `ConfigActor` protocol).
pub trait ConfigServiceApi: Send + Sync {
    fn snapshot(&self) -> ConfigSnapshot;
    fn apply_update(&mut self, draft: ConfigUpdateDraft) -> Result<ConfigSnapshot, ConfigError>;
}

pub struct ConfigService<FS: FileSystemTrait> {
    repo: JsonConfigRepository<FS>,
    state: ConfigState,
}

impl<FS: FileSystemTrait + Send + Sync> ConfigService<FS> {
    /// Bootstrap configuration: load file or defaults, persist, apply env overrides, ensure dirs.
    ///
    /// Same overall behaviour as legacy [`crate::app::config::Config::build`] + [`crate::app::config::Config::create_dirs`].
    pub fn bootstrap(env: &dyn EnvTrait, fs: FS) -> Result<Self, ConfigError> {
        let repo = JsonConfigRepository::new(env, fs);
        let mut state = Self::load_initial_state(env, &repo);
        if let Err(e) = repo.save(&state) {
            eprintln!("Failed to save default config: {e}");
        }
        env_overrides::apply_env_overrides(&mut state, env);
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
}

impl<FS: FileSystemTrait + Send + Sync> ConfigServiceApi for ConfigService<FS> {
    fn snapshot(&self) -> ConfigSnapshot {
        self.state.to_snapshot()
    }

    fn apply_update(&mut self, draft: ConfigUpdateDraft) -> Result<ConfigSnapshot, ConfigError> {
        self.state.apply_update(&draft);
        normalize_derived_paths(&mut self.state);
        self.ensure_directories()?;
        Ok(self.state.to_snapshot())
    }
}
