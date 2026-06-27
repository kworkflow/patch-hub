use tokio::task::JoinHandle;

/// Cloneable-free handle to the `AppActor` task.
///
/// The only operation available at this stage is waiting for the actor to
/// finish. A `Sender` and richer async methods are wired in a later commit.
pub struct AppHandle {
    join: JoinHandle<color_eyre::Result<()>>,
}

impl AppHandle {
    pub fn new(join: JoinHandle<color_eyre::Result<()>>) -> Self {
        Self { join }
    }

    /// Blocks the caller until the actor task completes, propagating any error
    /// returned by the actor's run loop.
    pub async fn run_until_done(self) -> color_eyre::Result<()> {
        self.join.await?
    }
}
