use std::time::Duration;

/// Fixed input timing values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBindings {
    pub chord_timeout: Duration,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            chord_timeout: Duration::from_millis(500),
        }
    }
}
