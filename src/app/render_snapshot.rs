use super::{App, AppState, AppViewModel};

/// Owned application state snapshot for terminal actor drawing.
#[derive(Clone)]
pub struct AppRenderSnapshot {
    state: AppState,
}

impl AppRenderSnapshot {
    #[allow(dead_code)] // Wired into the runtime draw path in the next commit.
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub fn to_view_model(&self) -> AppViewModel<'_> {
        AppViewModel { state: &self.state }
    }
}

impl App {
    #[allow(dead_code)] // Wired into the runtime draw path in the next commit.
    pub fn render_snapshot(&self) -> AppRenderSnapshot {
        AppRenderSnapshot::new(self.state.clone())
    }
}
