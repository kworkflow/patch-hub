mod r#trait;

pub use r#trait::{EnvError, EnvTrait};

#[cfg(test)]
pub use r#trait::MockEnvTrait;

use std::env;

#[cfg(test)]
mod tests;

pub struct OsEnv;

impl EnvTrait for OsEnv {
    fn var(&self, key: &str) -> Result<String, EnvError> {
        Ok(env::var(key)?)
    }

    fn which(&self, name: &str) -> bool {
        which::which(name).is_ok()
    }
}
