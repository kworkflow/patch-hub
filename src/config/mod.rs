//! Configuration bounded context: state, persistence, bootstrap, and snapshots.
#![allow(unused_imports)] // `pub use` re-exports are the public API of this module

pub mod actor;
mod env_overrides;
mod errors;
pub mod handle;
pub mod messages;
mod parsing;
mod repository;
mod service;
mod state;
mod update;

pub use actor::ConfigActor;
pub use errors::ConfigError;
pub use handle::ConfigHandle;
pub use repository::{resolve_config_path, ConfigRepository, JsonConfigRepository};
pub(crate) use service::bootstrap_parts;
pub use state::{normalize_derived_paths, ConfigSnapshot, ConfigState, KernelTree};
pub use update::{ConfigUpdateDraft, ValidatedConfigUpdate};

pub const DEFAULT_CONFIG_PATH_SUFFIX: &str = ".config/patch-hub/config.json";

#[cfg(test)]
mod tests;
