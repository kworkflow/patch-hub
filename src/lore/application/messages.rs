#![allow(dead_code)] // Phase 6 wires this protocol in follow-up commits.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use tokio::sync::oneshot;

use crate::{
    infrastructure::shell::ShellCommand,
    lore::{
        application::{
            cache::{BootstrapLoreData, CacheMode},
            dto::PatchsetDetails,
            errors::LoreError,
        },
        domain::{mailing_list::MailingList, patch::Patch},
    },
};

pub type LoreApiResult<T> = Result<T, LoreError>;

pub enum LoreApiMessage {
    GetBootstrapData {
        reply: oneshot::Sender<LoreApiResult<BootstrapLoreData>>,
    },
    FetchAvailableLists {
        cache_mode: CacheMode,
        reply: oneshot::Sender<LoreApiResult<Vec<MailingList>>>,
    },
    FetchFeedPage {
        target_list: String,
        page_size: usize,
        page_number: usize,
        cache_mode: CacheMode,
        reply: oneshot::Sender<LoreApiResult<Vec<Patch>>>,
    },
    FetchPatchsetDetails {
        representative_patch: Patch,
        cache_mode: CacheMode,
        reply: oneshot::Sender<LoreApiResult<PatchsetDetails>>,
    },
    LoadBookmarks {
        reply: oneshot::Sender<LoreApiResult<Vec<Patch>>>,
    },
    SaveBookmarks {
        bookmarks: Vec<Patch>,
        reply: oneshot::Sender<LoreApiResult<()>>,
    },
    LoadReviewed {
        reply: oneshot::Sender<LoreApiResult<HashMap<String, HashSet<usize>>>>,
    },
    SaveReviewed {
        reviewed: HashMap<String, HashSet<usize>>,
        reply: oneshot::Sender<LoreApiResult<()>>,
    },
    GetGitSignature {
        git_repo_path: String,
        reply: oneshot::Sender<(String, String)>,
    },
    PrepareReplyCommands {
        tmp_dir: PathBuf,
        target_list: String,
        patches: Vec<String>,
        patches_to_reply: Vec<bool>,
        git_signature: String,
        git_send_email_options: String,
        reply: oneshot::Sender<LoreApiResult<Vec<ShellCommand>>>,
    },
}
