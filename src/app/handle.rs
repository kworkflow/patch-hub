use tokio::task::JoinHandle;

/// Handle to the `AppActor` task.
///
/// Returned by [`AppActor::spawn`] and consumed by [`AppHandle::run_until_done`].
/// The actor runs until the input event channel closes (user exits) or an
/// unrecoverable error propagates from the run loop.
pub struct AppHandle {
    join: JoinHandle<color_eyre::Result<()>>,
}

impl AppHandle {
    pub(crate) fn new(join: JoinHandle<color_eyre::Result<()>>) -> Self {
        Self { join }
    }

    /// Blocks the caller until the actor task completes, propagating any error
    /// returned by the actor's run loop.
    pub async fn run_until_done(self) -> color_eyre::Result<()> {
        self.join.await?
    }
}
