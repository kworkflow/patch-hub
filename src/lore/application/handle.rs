use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use tokio::sync::{mpsc, oneshot};

use crate::{
    infrastructure::shell::ShellCommand,
    lore::{
        application::{
            cache::{BootstrapLoreData, CacheMode},
            dto::PatchsetDetails,
            errors::LoreError,
            messages::{LoreApiMessage, LoreApiResult},
        },
        domain::{mailing_list::MailingList, patch::Patch},
    },
};

#[derive(Clone)]
pub struct LoreApiHandle {
    tx: mpsc::Sender<LoreApiMessage>,
}

impl LoreApiHandle {
    pub fn new(tx: mpsc::Sender<LoreApiMessage>) -> Self {
        Self { tx }
    }

    pub async fn get_bootstrap_data(&self) -> LoreApiResult<BootstrapLoreData> {
        self.request_result(|reply| LoreApiMessage::GetBootstrapData { reply })
            .await
    }

    pub async fn fetch_available_lists(
        &self,
        cache_mode: CacheMode,
    ) -> LoreApiResult<Vec<MailingList>> {
        self.request_result(|reply| LoreApiMessage::FetchAvailableLists { cache_mode, reply })
            .await
    }

    pub async fn fetch_feed_page(
        &self,
        target_list: String,
        page_size: usize,
        page_number: usize,
        cache_mode: CacheMode,
    ) -> LoreApiResult<Vec<Patch>> {
        self.request_result(|reply| LoreApiMessage::FetchFeedPage {
            target_list,
            page_size,
            page_number,
            cache_mode,
            reply,
        })
        .await
    }

    pub async fn fetch_patchset_details(
        &self,
        representative_patch: Patch,
        cache_mode: CacheMode,
    ) -> LoreApiResult<PatchsetDetails> {
        self.request_result(|reply| LoreApiMessage::FetchPatchsetDetails {
            representative_patch,
            cache_mode,
            reply,
        })
        .await
    }

    pub async fn save_bookmarks(&self, bookmarks: Vec<Patch>) -> LoreApiResult<()> {
        self.request_result(|reply| LoreApiMessage::SaveBookmarks { bookmarks, reply })
            .await
    }

    pub async fn save_reviewed(
        &self,
        reviewed: HashMap<String, HashSet<usize>>,
    ) -> LoreApiResult<()> {
        self.request_result(|reply| LoreApiMessage::SaveReviewed { reviewed, reply })
            .await
    }

    pub async fn get_git_signature(
        &self,
        git_repo_path: String,
    ) -> Result<(String, String), LoreError> {
        self.request_result(|reply| LoreApiMessage::GetGitSignature {
            git_repo_path,
            reply,
        })
        .await
    }

    pub async fn prepare_reply_commands(
        &self,
        tmp_dir: PathBuf,
        target_list: String,
        patches: Vec<String>,
        patches_to_reply: Vec<bool>,
        git_signature: String,
        git_send_email_options: String,
    ) -> LoreApiResult<Vec<ShellCommand>> {
        self.request_result(|reply| LoreApiMessage::PrepareReplyCommands {
            tmp_dir,
            target_list,
            patches,
            patches_to_reply,
            git_signature,
            git_send_email_options,
            reply,
        })
        .await
    }

    async fn request_result<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<LoreApiResult<T>>) -> LoreApiMessage,
    ) -> LoreApiResult<T>
    where
        T: Send + 'static,
    {
        self.request(build_message).await?
    }

    async fn request<T>(
        &self,
        build_message: impl FnOnce(oneshot::Sender<T>) -> LoreApiMessage,
    ) -> Result<T, LoreError>
    where
        T: Send + 'static,
    {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(build_message(reply))
            .await
            .map_err(|_| LoreError::ActorUnavailable("request channel closed".to_string()))?;
        rx.await
            .map_err(|_| LoreError::ActorUnavailable("reply channel closed".to_string()))
    }
}
