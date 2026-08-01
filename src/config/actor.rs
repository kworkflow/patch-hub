//! Configuration actor: serializes access to mutable config state.
//!
//! All config reads and edits go through [`ConfigHandle`](crate::config::ConfigHandle)
//! as typed request/reply messages. The actor owns [`ConfigState`](crate::config::ConfigState)
//! and persists successful updates through [`JsonConfigRepository`](crate::config::JsonConfigRepository).
use std::ops::ControlFlow;

use tokio::{
    spawn,
    sync::{mpsc, oneshot},
};

use crate::{
    config::{
        handle::ConfigHandle,
        messages::{ConfigMessage, ConfigResult},
        repository::{ConfigRepository, JsonConfigRepository},
        service::{ensure_directories, validate_update},
        state::{normalize_derived_paths, ConfigState},
        ConfigSnapshot, ConfigUpdateDraft,
    },
    infrastructure::file_system::FileSystemTrait,
};

pub const DEFAULT_CONFIG_CHANNEL_SIZE: usize = 16;

pub struct ConfigActor<FS: FileSystemTrait> {
    state: ConfigState,
    repo: JsonConfigRepository<FS>,
    rx: mpsc::Receiver<ConfigMessage>,
}

impl<FS> ConfigActor<FS>
where
    FS: FileSystemTrait + Send + Sync + 'static,
{
    pub fn new(
        state: ConfigState,
        repo: JsonConfigRepository<FS>,
        rx: mpsc::Receiver<ConfigMessage>,
    ) -> Self {
        Self { state, repo, rx }
    }

    pub fn spawn(state: ConfigState, repo: JsonConfigRepository<FS>) -> ConfigHandle {
        let (tx, rx) = mpsc::channel(DEFAULT_CONFIG_CHANNEL_SIZE);
        tracing::debug!(
            channel_size = DEFAULT_CONFIG_CHANNEL_SIZE,
            "spawning config actor"
        );
        spawn(Self::new(state, repo, rx).run());
        ConfigHandle::new(tx)
    }

    pub async fn run(mut self) {
        tracing::info!("config actor started");
        while let Some(message) = self.rx.recv().await {
            if let ControlFlow::Break(()) = self.handle_message(message) {
                break;
            }
        }
        tracing::info!("config actor stopped");
    }

    fn handle_message(&mut self, message: ConfigMessage) -> ControlFlow<()> {
        let message_name = message.name();
        tracing::debug!(message = message_name, "config request received");

        match message {
            ConfigMessage::GetSnapshot { reply } => {
                send_config_snapshot_reply(message_name, reply, self.state.to_snapshot());
                ControlFlow::Continue(())
            }
            ConfigMessage::ValidateAndApply { draft, reply } => {
                let result = self.apply(draft);
                send_config_reply(message_name, reply, result);
                ControlFlow::Continue(())
            }
            ConfigMessage::Shutdown => {
                tracing::debug!("config actor shutting down");
                ControlFlow::Break(())
            }
        }
    }

    fn apply(&mut self, draft: ConfigUpdateDraft) -> ConfigResult<ConfigSnapshot> {
        let update = validate_update(draft, self.repo.fs())?;
        self.state.apply_update(&update);
        normalize_derived_paths(&mut self.state);
        ensure_directories(&self.state, self.repo.fs())?;
        self.repo.save(&self.state)?;
        Ok(self.state.to_snapshot())
    }
}

fn send_config_snapshot_reply(
    message_name: &'static str,
    reply: oneshot::Sender<ConfigSnapshot>,
    snapshot: ConfigSnapshot,
) {
    if reply.send(snapshot).is_err() {
        tracing::warn!(
            message = message_name,
            "config reply receiver dropped before response"
        );
    }
}

fn send_config_reply<T>(
    message_name: &'static str,
    reply: oneshot::Sender<ConfigResult<T>>,
    result: ConfigResult<T>,
) {
    if let Err(error) = &result {
        tracing::warn!(
            message = message_name,
            error = %error,
            "config request failed"
        );
    }

    if reply.send(result).is_err() {
        tracing::warn!(
            message = message_name,
            "config reply receiver dropped before response"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env::VarError,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::{
        config::{
            service::bootstrap_parts, ConfigError, ConfigUpdateDraft, DEFAULT_CONFIG_PATH_SUFFIX,
        },
        infrastructure::{env::MockEnvTrait, file_system::OsFileSystem},
    };

    use super::*;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("patch-hub-{prefix}-{}-{n}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn default_env() -> (MockEnvTrait, PathBuf) {
        let home = unique_test_dir("actor-home");
        let home_s = home.to_string_lossy().into_owned();
        let mut mock = MockEnvTrait::new();
        mock.expect_var()
            .withf(|key| key == "PATCH_HUB_CONFIG_PATH")
            .returning(|_| Err(VarError::NotPresent.into()));
        mock.expect_var()
            .withf(move |key| key == "HOME")
            .returning(move |_| Ok(home_s.clone()));
        mock.expect_var()
            .withf(|key| {
                matches!(
                    key,
                    "PATCH_HUB_PAGE_SIZE"
                        | "PATCH_HUB_CACHE_DIR"
                        | "PATCH_HUB_DATA_DIR"
                        | "PATCH_HUB_GIT_SEND_EMAIL_OPTIONS"
                        | "PATCH_HUB_PATCH_RENDERER"
                )
            })
            .returning(|_| Err(VarError::NotPresent.into()));
        (mock, home)
    }

    fn spawn_test_actor() -> (ConfigHandle, PathBuf) {
        let (env, home) = default_env();
        let (state, repo) = bootstrap_parts(&env, OsFileSystem).unwrap();
        (ConfigActor::spawn(state, repo), home)
    }

    #[tokio::test]
    async fn get_snapshot_returns_actor_state() {
        let (handle, _home) = spawn_test_actor();

        let snapshot = handle.get_snapshot().await.unwrap();

        assert_eq!(30, snapshot.page_size());
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn validate_and_apply_returns_updated_snapshot() {
        let (handle, _home) = spawn_test_actor();

        let snapshot = handle
            .validate_and_apply(ConfigUpdateDraft {
                page_size: Some("77".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(77, snapshot.page_size());
        assert_eq!(77, handle.get_snapshot().await.unwrap().page_size());
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn validate_and_apply_persists_update() {
        let (handle, home) = spawn_test_actor();
        let cfg_path = home.join(DEFAULT_CONFIG_PATH_SUFFIX);

        handle
            .validate_and_apply(ConfigUpdateDraft {
                page_size: Some("88".into()),
                ..Default::default()
            })
            .await
            .unwrap();

        let raw = fs::read_to_string(&cfg_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed["page_size"], 88);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn invalid_update_keeps_existing_state() {
        let (handle, _home) = spawn_test_actor();

        let err = handle
            .validate_and_apply(ConfigUpdateDraft {
                page_size: Some("not-a-number".into()),
                ..Default::default()
            })
            .await
            .unwrap_err();

        assert!(matches!(err, ConfigError::InvalidPageSize(ref s) if s == "not-a-number"));
        assert_eq!(30, handle.get_snapshot().await.unwrap().page_size());
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_stops_actor() {
        let (handle, _home) = spawn_test_actor();

        handle.shutdown().await;
        let err = handle.get_snapshot().await.unwrap_err();

        assert!(matches!(err, ConfigError::ActorUnavailable(_)));
    }
}
