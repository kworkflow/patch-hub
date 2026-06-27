use std::ops::ControlFlow;

use tokio::sync::oneshot;

use crate::{
    app::{errors::AppError, view_model::AppViewModel},
    input::{context::InputContext, event::InputEvent},
};

#[allow(dead_code)]
/// Typed message protocol for the `AppActor`.
///
/// External callers communicate with the actor exclusively through these
/// variants rather than touching `App` state directly.
pub enum AppMessage {
    /// Request the actor to perform startup validation.
    Initialize {
        reply_to: oneshot::Sender<Result<(), AppError>>,
    },

    /// Inject a synthetic input event for processing.
    Input {
        event: InputEvent,
        reply_to: oneshot::Sender<color_eyre::Result<ControlFlow<()>>>,
    },

    /// Request a snapshot of the current presentation model.
    GetViewModel {
        reply_to: oneshot::Sender<AppViewModel>,
    },

    /// Request the current input context for the input mapper.
    GetInputContext {
        reply_to: oneshot::Sender<InputContext>,
    },

    /// Request the actor to stop its run loop and exit cleanly.
    Shutdown {
        reply_to: oneshot::Sender<()>,
    },
}
