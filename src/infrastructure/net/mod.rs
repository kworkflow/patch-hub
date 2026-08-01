mod r#trait;

pub use r#trait::{HttpMethod, NetClientTrait, NetError};

#[cfg(test)]
pub use r#trait::MockNetClientTrait;

#[cfg(test)]
mod tests;

use std::time::Duration;

use ureq::tls::TlsConfig;
use ureq::Agent;

pub struct UreqNetClient {
    agent: Agent,
}

impl UreqNetClient {
    pub fn new() -> Self {
        let kw_agent = format!("kworkflow/patch-hub/{}", env!("CARGO_PKG_VERSION"));
        let agent: Agent = Agent::config_builder()
            .user_agent(ureq::config::AutoHeaderValue::from(kw_agent))
            .timeout_per_call(Some(Duration::from_secs(120)))
            .tls_config(TlsConfig::builder().build())
            .build()
            .into();
        Self { agent }
    }
}

impl NetClientTrait for UreqNetClient {
    fn request(&self, method: HttpMethod, url: &str) -> Result<String, NetError> {
        let mut response = match method {
            HttpMethod::Get => self
                .agent
                .get(url)
                .header("Accept", "text/html,application/xhtml+xml,application/xml")
                .call()?,
        };
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| NetError::Other(e.to_string()))
    }
}
