/// Errors produced by the UI actor boundary.
#[derive(thiserror::Error, Debug)]
pub enum UiError {
    #[error("failed to build scene: {0}")]
    Build(String),
}
