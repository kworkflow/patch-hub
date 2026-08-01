//! Read-only projections for rendering: keeps `ui/` independent of the [`super::App`] struct.

use super::state::AppState;

/// References into [`AppState`] for one Ratatui frame. Built via [`super::App::to_view_model`].
#[derive(Clone, Copy)]
pub struct AppViewModel<'a> {
    pub state: &'a AppState,
}
