use tokio::sync::oneshot;

use crate::{
    app::view_model::AppViewModel,
    ui::{errors::UiError, scene::UiScene},
};

pub type UiResult<T> = Result<T, UiError>;

pub enum UiMessage {
    BuildScene {
        app_view: Box<AppViewModel>,
        reply_to: oneshot::Sender<UiResult<UiScene>>,
    },
    Shutdown,
}

impl UiMessage {
    pub fn name(&self) -> &'static str {
        match self {
            UiMessage::BuildScene { .. } => "BuildScene",
            UiMessage::Shutdown => "Shutdown",
        }
    }
}
