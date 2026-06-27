#[allow(dead_code)]
/// Signals the outcome of a single user-input or system-driven state update.
pub enum AppTransition {
    /// No state was mutated; the current frame can be reused.
    Noop,
    /// App state changed and a new frame should be rendered.
    StateChanged,
    /// The user requested the application to exit.
    ExitRequested,
}
