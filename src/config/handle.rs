use tokio::sync::{mpsc, oneshot};

use crate::config::{
    messages::{ConfigMessage, ConfigResult},
    ConfigError, ConfigSnapshot, ConfigUpdateDraft,
};

#[derive(Clone)]
pub struct ConfigHandle {
    tx: mpsc::Sender<ConfigMessage>,
}

impl ConfigHandle {
    pub fn new(tx: mpsc::Sender<ConfigMessage>) -> Self {
        Self { tx }
    }

    pub async fn get_snapshot(&self) -> ConfigResult<ConfigSnapshot> {
        self.request(|reply| ConfigMessage::GetSnapshot { reply })
            .await
    }

    pub async fn validate_and_apply(
        &self,
        draft: ConfigUpdateDraft,
    ) -> ConfigResult<ConfigSnapshot> {
        self.request_result(|reply| ConfigMessage::ValidateAndApply { draft, reply })
            .await
    }

    /// Signals the actor to stop processing messages and exit its run loop.
    ///
    /// Callers should invoke this after the last request that uses this handle has
    /// completed. Dropping all clones of the handle also stops the actor, but
    /// calling `shutdown` makes the intent explicit and allows ordered teardown.
    pub async fn shutdown(&self) {
        self.tx.send(ConfigMessage::Shutdown).await.ok();
    }

    async fn request_result<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<ConfigResult<T>>) -> ConfigMessage,
    ) -> ConfigResult<T>
    where
        T: Send + 'static,
    {
        self.request(build_message).await?
    }

    async fn request<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<T>) -> ConfigMessage,
    ) -> ConfigResult<T>
    where
        T: Send + 'static,
    {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(build_message(reply))
            .await
            .map_err(|_| ConfigError::ActorUnavailable("request channel closed".to_string()))?;
        rx.await
            .map_err(|_| ConfigError::ActorUnavailable("reply channel closed".to_string()))
    }
}
