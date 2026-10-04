//! Configuration bounded context: state, persistence, bootstrap, and snapshots.

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
pub use repository::{ConfigRepository, JsonConfigRepository};
pub(crate) use service::ConfigService;
pub use state::{ConfigSnapshot, ConfigState, KernelTree};
pub use update::{ConfigUpdateDraft, ValidatedConfigUpdate};

pub const DEFAULT_CONFIG_PATH_SUFFIX: &str = ".config/patch-hub/config.json";

#[cfg(test)]
mod tests;
