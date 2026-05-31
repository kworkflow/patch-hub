#![allow(dead_code)] // Follow-up Phase 9 commits wire the full terminal protocol.

use std::time::Duration;

use ratatui::crossterm::event::KeyCode;
use tokio::sync::oneshot;

use crate::{
    app::render_snapshot::AppRenderSnapshot, input::event::TerminalEvent, terminal::TerminalError,
};

pub type TerminalResult<T> = Result<T, TerminalError>;

/// Owned payload for a terminal draw request.
#[derive(Clone, Default)]
pub enum TerminalFrame {
    Main(Box<AppRenderSnapshot>),
    Loading(String),
    /// Placeholder frame used while the terminal actor is wired into the app.
    #[default]
    Empty,
}

pub enum TerminalMessage {
    Draw {
        frame: TerminalFrame,
        reply: oneshot::Sender<TerminalResult<()>>,
    },
    ReadEvent {
        reply: oneshot::Sender<TerminalResult<Option<TerminalEvent>>>,
    },
    PollEvent {
        timeout: Duration,
        reply: oneshot::Sender<TerminalResult<Option<TerminalEvent>>>,
    },
    SetupUserIo {
        reply: oneshot::Sender<TerminalResult<()>>,
    },
    TeardownUserIo {
        reply: oneshot::Sender<TerminalResult<()>>,
    },
    WaitForKeyPress {
        key: KeyCode,
        timeout: Duration,
        reply: oneshot::Sender<TerminalResult<bool>>,
    },
    GetSize {
        reply: oneshot::Sender<TerminalResult<(u16, u16)>>,
    },
    Shutdown {
        reply: oneshot::Sender<TerminalResult<()>>,
    },
}

impl TerminalMessage {
    pub fn name(&self) -> &'static str {
        match self {
            TerminalMessage::Draw { .. } => "Draw",
            TerminalMessage::ReadEvent { .. } => "ReadEvent",
            TerminalMessage::PollEvent { .. } => "PollEvent",
            TerminalMessage::SetupUserIo { .. } => "SetupUserIo",
            TerminalMessage::TeardownUserIo { .. } => "TeardownUserIo",
            TerminalMessage::WaitForKeyPress { .. } => "WaitForKeyPress",
            TerminalMessage::GetSize { .. } => "GetSize",
            TerminalMessage::Shutdown { .. } => "Shutdown",
        }
    }
}
