use super::{App, AppState, AppViewModel};

/// Owned application state snapshot for terminal actor drawing.
#[derive(Clone)]
pub struct AppRenderSnapshot {
    state: AppState,
}

impl AppRenderSnapshot {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub fn to_view_model(&self) -> AppViewModel<'_> {
        AppViewModel { state: &self.state }
    }
}

impl App {
    pub fn render_snapshot(&self) -> AppRenderSnapshot {
        AppRenderSnapshot::new(self.state.clone())
    }
}
