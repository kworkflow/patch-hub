use tokio::sync::{mpsc, oneshot, watch};

use crate::{
    config::KernelTree,
    kw::{
        errors::{KwError, KwStartError},
        history::KwApplyRecord,
        messages::{KwMessage, StartRequest},
        readiness::KwReadiness,
        status::KwStatusSnapshot,
    },
};

#[derive(Clone)]
pub struct KwHandle {
    tx: mpsc::Sender<KwMessage>,
}

// Only record_apply/shutdown have a production caller until the KwOps
// screen lands; kept per the CachePolicy precedent
// (src/lore/application/cache.rs).
#[allow(dead_code)]
impl KwHandle {
    pub fn new(tx: mpsc::Sender<KwMessage>) -> Self {
        Self { tx }
    }

    pub async fn record_apply(&self, record: KwApplyRecord) -> Result<(), KwError> {
        self.request_result(|reply| KwMessage::RecordApply { record, reply })
            .await
    }

    /// Resolves as soon as the actor accepts or refuses the job — never
    /// when the job finishes.
    pub async fn start_build(&self, request: StartRequest) -> Result<(), KwStartError> {
        self.start(|reply| KwMessage::StartBuild { request, reply })
            .await
    }

    pub async fn start_deploy(&self, request: StartRequest) -> Result<(), KwStartError> {
        self.start(|reply| KwMessage::StartDeploy { request, reply })
            .await
    }

    pub async fn start_build_then_deploy(&self, request: StartRequest) -> Result<(), KwStartError> {
        self.start(|reply| KwMessage::StartBuildThenDeploy { request, reply })
            .await
    }

    pub async fn cancel(&self) -> Result<(), KwError> {
        self.request_result(|reply| KwMessage::Cancel { reply })
            .await
    }

    pub async fn get_status(&self) -> Result<KwStatusSnapshot, KwError> {
        self.request(|reply| KwMessage::GetStatus { reply }).await
    }

    pub async fn watch_status(&self) -> Result<watch::Receiver<KwStatusSnapshot>, KwError> {
        self.request(|reply| KwMessage::WatchStatus { reply }).await
    }

    pub async fn get_readiness(
        &self,
        kernel_tree_id: &str,
        tree: &KernelTree,
    ) -> Result<KwReadiness, KwError> {
        self.request_result(|reply| KwMessage::GetReadiness {
            kernel_tree_id: kernel_tree_id.to_string(),
            tree: tree.clone(),
            reply,
        })
        .await
    }

    pub async fn restore_previous_branch(&self) -> Result<(), KwError> {
        self.request_result(|reply| KwMessage::RestorePreviousBranch { reply })
            .await
    }

    /// Signals the actor to stop processing messages and exit its run loop.
    /// A running job is killed before the actor stops.
    pub async fn shutdown(&self) {
        self.tx.send(KwMessage::Shutdown).await.ok();
    }

    async fn request_result<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<Result<T, KwError>>) -> KwMessage,
    ) -> Result<T, KwError>
    where
        T: Send + 'static,
    {
        self.request(build_message).await?
    }

    async fn request<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<T>) -> KwMessage,
    ) -> Result<T, KwError>
    where
        T: Send + 'static,
    {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(build_message(reply))
            .await
            .map_err(|_| KwError::ActorUnavailable("request channel closed".to_string()))?;
        rx.await
            .map_err(|_| KwError::ActorUnavailable("reply channel closed".to_string()))
    }

    async fn start(
        &self,
        build_message: impl FnOnce(oneshot::Sender<Result<(), KwStartError>>) -> KwMessage,
    ) -> Result<(), KwStartError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(build_message(reply))
            .await
            .map_err(|_| KwStartError::ActorUnavailable("request channel closed".to_string()))?;
        rx.await
            .map_err(|_| KwStartError::ActorUnavailable("reply channel closed".to_string()))?
    }
}
