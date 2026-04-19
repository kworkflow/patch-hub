//! Configuration bounded context: state, persistence, bootstrap, and snapshots.
//! Not yet wired into `App` (Phase 5 commit 1 — module only).
#![allow(dead_code)] // exercised in commit 5-B wiring and tests
#![allow(unused_imports)] // pub re-exports unused until wiring (commit 5-B)

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
pub use update::ConfigUpdateDraft;

pub const DEFAULT_CONFIG_PATH_SUFFIX: &str = ".config/patch-hub/config.json";
