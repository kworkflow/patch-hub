use mockall::automock;
use thiserror::Error;

pub enum HttpMethod {
    Get,
}

#[derive(Debug, Error)]
pub enum NetError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("HTTP {code}: {message}")]
    HttpStatus { code: u16, message: String },
    #[error("{0}")]
    Other(String),
}

impl From<ureq::Error> for NetError {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::StatusCode(code) => NetError::HttpStatus {
                code,
                message: format!("HTTP {code}"),
            },
            ureq::Error::Timeout(_) => NetError::Timeout(e.to_string()),
            _ => NetError::ConnectionError(e.to_string()),
        }
    }
}

#[automock]
pub trait NetClientTrait: Send + Sync {
    fn request(&self, method: HttpMethod, url: &str) -> Result<String, NetError>;
}
