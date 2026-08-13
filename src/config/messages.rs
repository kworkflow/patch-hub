use tokio::sync::oneshot;

use crate::config::{ConfigError, ConfigSnapshot, ConfigUpdateDraft};

pub type ConfigResult<T> = Result<T, ConfigError>;

pub enum ConfigMessage {
    GetSnapshot {
        reply: oneshot::Sender<ConfigSnapshot>,
    },
    ValidateAndApply {
        // Boxed to keep the enum small (clippy::large_enum_variant): the draft
        // holds one `Option<String>` per editable field.
        draft: Box<ConfigUpdateDraft>,
        reply: oneshot::Sender<ConfigResult<ConfigSnapshot>>,
    },
    Shutdown,
}

impl ConfigMessage {
    pub fn name(&self) -> &'static str {
        match self {
            ConfigMessage::GetSnapshot { .. } => "GetSnapshot",
            ConfigMessage::ValidateAndApply { .. } => "ValidateAndApply",
            ConfigMessage::Shutdown => "Shutdown",
        }
    }
}
