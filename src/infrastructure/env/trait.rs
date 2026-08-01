use mockall::automock;
use thiserror::Error;

use std::env::VarError;

#[derive(Debug, Error)]
pub enum EnvError {
    #[error("{0}")]
    VarError(#[from] VarError),
}

#[automock]
pub trait EnvTrait: Send + Sync {
    /// Returns the value of the environment variable `key`.
    fn var(&self, key: &str) -> Result<String, EnvError>;

    /// Returns `true` if the binary `name` is found somewhere on `PATH`.
    fn which(&self, name: &str) -> bool;
}
