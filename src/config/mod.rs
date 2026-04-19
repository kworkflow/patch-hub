//! Configuration bounded context: state, persistence, bootstrap, and snapshots.
#![allow(unused_imports)] // `pub use` re-exports are the public API of this module

mod env_overrides;
mod errors;
mod repository;
mod service;
mod state;
mod update;

pub use errors::ConfigError;
pub use repository::{resolve_config_path, ConfigRepository, JsonConfigRepository};
pub use service::{ConfigService, ConfigServiceApi};
pub use state::{normalize_derived_paths, ConfigSnapshot, ConfigState, KernelTree};
pub use update::{ConfigUpdateDraft, ValidatedConfigUpdate};

pub const DEFAULT_CONFIG_PATH_SUFFIX: &str = ".config/patch-hub/config.json";

#[cfg(test)]
mod tests;
