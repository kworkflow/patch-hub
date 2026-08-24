use crate::config::errors::ConfigError;
use crate::config::state::ConfigState;
use crate::config::DEFAULT_CONFIG_PATH_SUFFIX;
use crate::infrastructure::{
    env::EnvTrait,
    file_system::{FileSystemTrait, JsonUtils},
};

pub trait ConfigRepository: Send + Sync {
    fn save(&self, state: &ConfigState) -> Result<(), ConfigError>;
}

pub fn resolve_config_path(env: &dyn EnvTrait) -> String {
    env.var("PATCH_HUB_CONFIG_PATH").unwrap_or_else(|_| {
        format!(
            "{}/{}",
            env.var("HOME")
                .expect("invariant: HOME environment variable must be set"),
            DEFAULT_CONFIG_PATH_SUFFIX
        )
    })
}

pub struct JsonConfigRepository<FS> {
    config_path: String,
    fs: FS,
}

impl<FS> JsonConfigRepository<FS> {
    pub fn new(env: &dyn EnvTrait, fs: FS) -> Self {
        Self {
            config_path: resolve_config_path(env),
            fs,
        }
    }

    pub fn fs(&self) -> &FS {
        &self.fs
    }

    pub fn config_path(&self) -> &str {
        &self.config_path
    }
}

impl<FS: FileSystemTrait> ConfigRepository for JsonConfigRepository<FS> {
    fn save(&self, state: &ConfigState) -> Result<(), ConfigError> {
        JsonUtils::atomic_write_json(&self.fs, state, &self.config_path)?;
        Ok(())
    }
}
