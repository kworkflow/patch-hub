use tokio::{
    sync::{mpsc, oneshot},
    task,
};

use crate::lore::application::{
    errors::LoreError, handle::LoreApiHandle, messages::LoreApiMessage, service::LoreService,
};

pub const DEFAULT_LORE_API_CHANNEL_SIZE: usize = 32;

pub struct LoreApiActor {
    core: Option<LoreService>,
    rx: mpsc::Receiver<LoreApiMessage>,
}

impl LoreApiActor {
    pub fn new(core: LoreService, rx: mpsc::Receiver<LoreApiMessage>) -> Self {
        Self {
            core: Some(core),
            rx,
        }
    }

    pub fn spawn(core: LoreService) -> LoreApiHandle {
        let (tx, rx) = mpsc::channel(DEFAULT_LORE_API_CHANNEL_SIZE);
        tracing::debug!(
            channel_size = DEFAULT_LORE_API_CHANNEL_SIZE,
            "spawning lore api actor"
        );
        tokio::spawn(Self::new(core, rx).run());
        LoreApiHandle::new(tx)
    }

    pub async fn run(mut self) {
        tracing::info!("lore api actor started");
        while let Some(message) = self.rx.recv().await {
            self.handle_message(message).await;
        }
        tracing::info!("lore api actor stopped");
    }

    async fn handle_message(&mut self, message: LoreApiMessage) {
        let message_name = message.name();
        tracing::debug!(message = message_name, "lore api request received");

        match message {
            LoreApiMessage::GetBootstrapData { reply } => {
                tracing::debug!("loading lore bootstrap data");
                let result = self
                    .with_core(|core| core.warm_bootstrap_cache())
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::FetchAvailableLists { cache_mode, reply } => {
                tracing::debug!(?cache_mode, "fetching available mailing lists");
                let result = self
                    .with_core(move |core| core.fetch_available_lists(cache_mode))
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::FetchFeedPage {
                target_list,
                page_size,
                page_number,
                cache_mode,
                reply,
            } => {
                tracing::debug!(
                    list = %target_list,
                    page_size,
                    page_number,
                    ?cache_mode,
                    "fetching lore feed page"
                );
                let result = self
                    .with_core(move |core| {
                        core.fetch_next_patch_page(&target_list, page_size, page_number, cache_mode)
                    })
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::FetchPatchsetDetails {
                representative_patch,
                cache_mode,
                reply,
            } => {
                tracing::debug!(
                    message_id = %representative_patch.message_id().href,
                    ?cache_mode,
                    "fetching patchset details"
                );
                let result = self
                    .with_core(move |core| {
                        core.fetch_patchset_details(&representative_patch, cache_mode)
                    })
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::LoadBookmarks { reply } => {
                tracing::debug!("loading bookmarked patchsets");
                let result = self
                    .with_core(|core| core.load_bookmarked_patchsets())
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::SaveBookmarks { bookmarks, reply } => {
                tracing::debug!(count = bookmarks.len(), "saving bookmarked patchsets");
                let result = self
                    .with_core(move |core| core.save_bookmarked_patchsets(&bookmarks))
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::LoadReviewed { reply } => {
                tracing::debug!("loading reviewed patchsets");
                let result = self
                    .with_core(|core| core.load_reviewed_patchsets())
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::SaveReviewed { reviewed, reply } => {
                tracing::debug!(patchsets = reviewed.len(), "saving reviewed patchsets");
                let result = self
                    .with_core(move |core| core.save_reviewed_patchsets(&reviewed))
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::GetGitSignature {
                git_repo_path,
                reply,
            } => {
                tracing::debug!(repo = %git_repo_path, "loading git signature");
                let result = self
                    .with_core(move |core| core.get_git_signature(&git_repo_path))
                    .await;
                send_lore_reply(message_name, reply, result);
            }
            LoreApiMessage::PrepareReplyCommands {
                tmp_dir,
                target_list,
                patches,
                patches_to_reply,
                git_signature,
                git_send_email_options,
                reply,
            } => {
                tracing::debug!(
                    target_list = %target_list,
                    patch_count = patches.len(),
                    selected_count = patches_to_reply.iter().filter(|selected| **selected).count(),
                    "preparing reply commands"
                );
                let result = self
                    .with_core(move |core| {
                        core.prepare_reply_commands(
                            &tmp_dir,
                            &target_list,
                            &patches,
                            &patches_to_reply,
                            &git_signature,
                            &git_send_email_options,
                        )
                    })
                    .await
                    .and_then(|result| result);
                send_lore_reply(message_name, reply, result);
            }
        }
    }

    async fn with_core<T, F>(&mut self, operation: F) -> Result<T, LoreError>
    where
        T: Send + 'static,
        F: FnOnce(&mut LoreService) -> T + Send + 'static,
    {
        let mut core = self
            .core
            .take()
            .ok_or_else(|| LoreError::ActorUnavailable("core unavailable".to_string()))?;
        let (core, result) = task::spawn_blocking(move || {
            let result = operation(&mut core);
            (core, result)
        })
        .await
        .map_err(|e| LoreError::ActorUnavailable(e.to_string()))?;
        self.core = Some(core);
        Ok(result)
    }
}

fn send_lore_reply<T>(
    message_name: &'static str,
    reply: oneshot::Sender<Result<T, LoreError>>,
    result: Result<T, LoreError>,
) {
    if let Err(error) = &result {
        tracing::warn!(
            message = message_name,
            error = %error,
            "lore api request failed"
        );
    }

    if reply.send(result).is_err() {
        tracing::warn!(
            message = message_name,
            "lore api reply receiver dropped before response"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use crate::{
        infrastructure::{file_system::MockFileSystemTrait, shell::MockShellTrait},
        lore::{
            application::{
                cache::{CacheMode, CacheTtl},
                handle::LoreApiHandle,
            },
            domain::mailing_list::MailingList,
            infrastructure::{
                http_lore_client::{MockFeedGateway, MockListsGateway, MockPatchHtmlGateway},
                patchset_fetcher::MockPatchsetFetcher,
                patchset_parser::MockPatchsetParser,
                persistence::{MockMailingListsCacheStore, MockUserLoreStateStore},
            },
        },
    };

    use super::*;

    fn make_service(
        lists_store: MockMailingListsCacheStore,
        user_state: MockUserLoreStateStore,
    ) -> LoreService {
        LoreService::new(
            Arc::new(MockListsGateway::new()),
            Arc::new(MockFeedGateway::new()),
            Arc::new(MockPatchHtmlGateway::new()),
            Arc::new(lists_store),
            Arc::new(user_state),
            Arc::new(MockPatchsetFetcher::new()),
            Arc::new(MockPatchsetParser::new()),
            Arc::new(MockFileSystemTrait::new()),
            Arc::new(MockShellTrait::new()),
            CacheTtl::default(),
        )
    }

    fn spawn_test_actor(core: LoreService) -> LoreApiHandle {
        LoreApiActor::spawn(core)
    }

    #[tokio::test]
    async fn handle_returns_bootstrap_data_from_actor() {
        let mut lists_store = MockMailingListsCacheStore::new();
        lists_store
            .expect_load_available_lists()
            .times(1)
            .returning(|| Ok(vec![MailingList::new("linux-mm", "")]));

        let mut user_state = MockUserLoreStateStore::new();
        user_state
            .expect_load_bookmarked_patchsets()
            .times(1)
            .returning(|| Ok(vec![]));
        user_state
            .expect_load_reviewed_patchsets()
            .times(1)
            .returning(|| Ok(HashMap::new()));

        let handle = spawn_test_actor(make_service(lists_store, user_state));

        let data = handle.get_bootstrap_data().await.unwrap();

        assert_eq!(1, data.mailing_lists.len());
        assert_eq!("linux-mm", data.mailing_lists[0].name());
        assert!(data.bookmarks.is_empty());
        assert!(data.reviewed.is_empty());
    }

    #[tokio::test]
    async fn actor_preserves_cache_across_messages() {
        let mut lists_store = MockMailingListsCacheStore::new();
        lists_store
            .expect_load_available_lists()
            .times(1)
            .returning(|| Ok(vec![MailingList::new("cached-list", "")]));

        let handle = spawn_test_actor(make_service(lists_store, MockUserLoreStateStore::new()));

        let first = handle
            .fetch_available_lists(CacheMode::UseCache)
            .await
            .unwrap();
        let second = handle
            .fetch_available_lists(CacheMode::UseCache)
            .await
            .unwrap();

        assert_eq!("cached-list", first[0].name());
        assert_eq!("cached-list", second[0].name());
    }
}
