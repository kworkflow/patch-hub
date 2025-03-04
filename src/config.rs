use std::{collections::HashMap, io, path::Path};

use crate::env::Env;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
    sync::mpsc::Sender,
    task::JoinHandle,
};

#[cfg(test)]
mod tests;

use crate::app::{cover_renderer::CoverRenderer, patch_renderer::PatchRenderer};

/// Stores the whole configuration options set in patch-hub.
/// Manages configuration loading, saving an applying overrides.
///
/// You're not supposed to use this directly, but use the [`ConfigTx`] instead
/// by calling [`spawn`] as soon as this struct is built.
///
/// The expected flow is:
///  - Instantiate the configurations with [`build`]
///  - Spawn the actor with [`spawn`]
///  - Get and set options, call save and do other operations with the [`ConfigTx`] struct
///
///  [`spawn`]: Config::spawn
///  [`build`]: Config::build
#[derive(Debug)]
pub struct ConfigCore {
    data: Data,
    env: Env,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Data {
    page_size: usize,
    patchsets_cache_dir: String,
    bookmarked_patchsets_path: String,
    mailing_lists_path: String,
    reviewed_patchsets_path: String,
    /// Logs directory
    logs_path: String,
    git_send_email_options: String,
    /// Base directory for all patch-hub cache
    cache_dir: String,
    /// Base directory for all patch-hub cache
    data_dir: String,
    /// Renderer to use for patch previews
    patch_renderer: PatchRenderer,
    /// Renderer to use for patchset covers
    cover_renderer: CoverRenderer,
    /// Maximum age of a log file in days
    max_log_age: usize,
    /// Map of tracked kernel trees
    kernel_trees: HashMap<String, KernelTree>,
    /// Target kernel tree to run actions
    target_kernel_tree: Option<String>,
    /// Flags to be use with `git am` command when applying patches
    git_am_options: String,
    git_am_branch_prefix: String,
}

/// Describes a kernel tree with the path to the local repository
/// and the target branch
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Getters)]
pub struct KernelTree {
    /// Path to kernel tree in the filesystem
    path: String,
    /// Target branch
    branch: String,
}

impl ConfigCore {
    async fn load(env: Env) -> Option<ConfigCore> {
        if let Ok(config_path) = env.get("PATCH_HUB_CONFIG_PATH").await {
            if Path::new(&config_path).is_file() {
                let file_contents = fs::read_to_string(&config_path)
                    .await
                    .unwrap_or_else(|_| String::new());
                if let Ok(data) = serde_json::from_str::<Data>(&file_contents) {
                    return Some(ConfigCore { data, env });
                }
            }
        }

        let config_path = format!(
            "{}/.config/patch-hub/config.json",
            env.get("HOME").await.expect("$HOME is not set")
        );
        if Path::new(&config_path).is_file() {
            let file_contents = fs::read_to_string(&config_path)
                .await
                .unwrap_or_else(|_| String::new());
            if let Ok(data) = serde_json::from_str::<Data>(&file_contents) {
                return Some(ConfigCore { data, env });
            }
        }

        None
    }

    async fn save(&self) -> io::Result<()> {
        let config_path = if let Ok(path) = self.env.get("PATCH_HUB_CONFIG_PATH").await {
            path
        } else {
            format!(
                "{}/.config/patch-hub/config.json",
                self.env.get("HOME").await.expect("$HOME is not set")
            )
        };

        let config_path = Path::new(&config_path);
        // We need to assure that the parent dir of `config_path` exists
        if let Some(parent_dir) = Path::parent(config_path) {
            fs::create_dir_all(parent_dir).await?;
        }

        let tmp_filename = format!("{}.tmp", config_path.display());
        {
            let mut tmp_file = File::create(&tmp_filename).await?;
            let content = serde_json::to_string_pretty(&self.data)?;
            tmp_file.write_all(content.as_bytes()).await?;
        }
        fs::rename(tmp_filename, config_path).await?;
        Ok(())
    }

    async fn apply_env_overrides(&mut self) {
        if let Ok(page_size) = self.env.get("PATCH_HUB_PAGE_SIZE").await {
            self.data.page_size = page_size.parse().unwrap();
        };

        if let Ok(cache_dir) = self.env.get("PATCH_HUB_CACHE_DIR").await {
            self.data.patchsets_cache_dir = format!("{}/patchsets", &cache_dir);
            self.data.cache_dir = cache_dir;
        };

        if let Ok(data_dir) = self.env.get("PATCH_HUB_DATA_DIR").await {
            self.data.bookmarked_patchsets_path =
                format!("{}/bookmarked_patchsets.json", &data_dir);
            self.data.mailing_lists_path = format!("{}/mailing_lists.json", &data_dir);
            self.data.reviewed_patchsets_path = format!("{}/reviewed_patchsets.json", &data_dir);
            self.data.logs_path = format!("{}/logs", &data_dir);
            self.data.data_dir = data_dir;
        };

        if let Ok(git_send_email_options) = self.env.get("PATCH_HUB_GIT_SEND_EMAIL_OPTIONS").await {
            self.data.git_send_email_options = git_send_email_options;
        };

        if let Ok(patch_renderer) = self.env.get("PATCH_HUB_PATCH_RENDERER").await {
            self.data.patch_renderer = patch_renderer.into();
        };
    }

    /// Creates a new config instance either by loading it from the config file, or by
    /// Defining a brand new [`default`] one.
    ///
    /// You're supposed to call [`spawn`] immediately after this method to use the actor
    ///
    /// [`default`]: Config::default
    /// [`spawn`]: Config::spawn
    pub async fn build(env: Env) -> Self {
        let mut config = if let Some(config) = Self::load(env.clone()).await {
            config
        } else {
            let cache_dir = format!(
                "{}/.cache/patch_hub",
                env.get("HOME").await.expect("HOME env var not set")
            );
            let data_dir = format!(
                "{}/.local/share/patch_hub",
                env.get("HOME").await.expect("HOME env var not set")
            );

            let data = Data::new(cache_dir, data_dir);

            let config = Self { data, env };
            // TODO: Better handle this error
            let _ = config.save().await;
            config
        };

        config.apply_env_overrides().await;

        config
    }

    /// Transforms the config instance into an actor. This method returns a [`ConfigTx`] and a
    /// [`JoinHandle`] that can be used to send commands to the logger or await for it to finish
    ///
    /// The handling of the commandds received is done sequentially, so a command is only processed
    /// once the previous one is finished.
    pub fn spawn(mut self) -> (Config, JoinHandle<()>) {
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let handle = tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                use Command::*;

                match command {
                    Save(tx) => {
                        let _ = self.save().await;
                        let _ = tx.send(());
                    }
                    Serialize(tx) => {
                        let ser = serde_json::to_string_pretty(&self.data);
                        let _ = tx.send(ser);
                    }
                    Deserialize(content, tx) => {
                        let de = serde_json::from_str::<Data>(&content);

                        match de {
                            Ok(_data) => {
                                // self.data = data;
                                let _ = tx.send(Ok(()));
                            }
                            Err(err) => {
                                let _ = tx.send(Err(err));
                            }
                        }
                    }
                    GetString(opt, tx) => {
                        let response = self.data.string_opt(opt).to_string();
                        let _ = tx.send(response); // If failed, the receiver has been dropped
                    }
                    SetString(opt, val) => *self.data.string_opt_mut(opt) = val,
                    GetUSize(opt, tx) => {
                        let response = self.data.usize_opt(opt);
                        let _ = tx.send(response); // If failed, the receiver has been dropped
                    }
                    SetUSize(opt, val) => *self.data.usize_opt_mut(opt) = val,
                    GetPatchRenderer(tx) => {
                        let response = self.data.patch_renderer;
                        let _ = tx.send(response); // If failed, the receiver has been dropped
                    }
                    SetPatchRenderer(val) => self.data.patch_renderer = val,
                    GetCoverRenderer(tx) => {
                        let response = self.data.cover_renderer;
                        let _ = tx.send(response); // If failed, the receiver has been dropped
                    }
                    SetCoverRenderer(val) => self.data.cover_renderer = val,
                    GetKernelTrees(tx) => {
                        let response = self.data.kernel_trees.keys().cloned().collect();
                        let _ = tx.send(response); // If failed, the receiver has been dropped
                    }
                    GetKernelTree(name, tx) => {
                        let response = self.data.kernel_trees.get(&name).cloned();
                        let _ = tx.send(response); // If failed, the receiver has been dropped
                    }
                    SetKernelTree(name, val) => {
                        self.data.set_kernel_tree(name, val);
                    }
                    GetTargetKernelTree(tx) => {
                        let response = self.data.target_kernel_tree.clone();
                        let _ = tx.send(response); // If failed, the receiver has been dropped
                    }
                    SetTargetKernelTree(val, tx) => {
                        let _ = tx.send(self.data.set_target_kernel_tree(val));
                    }
                }
            }
        });

        let config = Config {
            sender: tx,
            mode: Mode::Default,
        };

        (config, handle)
    }
}

impl Data {
    fn new(cache_dir: String, data_dir: String) -> Self {
        Data {
            page_size: 30,
            patchsets_cache_dir: format!("{cache_dir}/patchsets"),
            bookmarked_patchsets_path: format!("{data_dir}/bookmarked_patchsets.json"),
            mailing_lists_path: format!("{data_dir}/mailing_lists.json"),
            reviewed_patchsets_path: format!("{data_dir}/reviewed_patchsets.json"),
            logs_path: format!("{data_dir}/logs"),
            git_send_email_options: "--dry-run --suppress-cc=all".to_string(),
            patch_renderer: Default::default(),
            cover_renderer: Default::default(),
            cache_dir,
            data_dir,
            max_log_age: 30,
            kernel_trees: HashMap::new(),
            target_kernel_tree: None,
            git_am_options: String::new(),
            git_am_branch_prefix: String::from("patchset-"),
        }
    }

    fn string_opt(&self, opt: StringOpt) -> &str {
        match opt {
            StringOpt::PatchsetsCacheDir => &self.patchsets_cache_dir,
            StringOpt::BookmarkedPatchsetsPath => &self.bookmarked_patchsets_path,
            StringOpt::MailingListsPath => &self.mailing_lists_path,
            StringOpt::ReviewedPatchsetsPath => &self.reviewed_patchsets_path,
            StringOpt::LogsPath => &self.logs_path,
            StringOpt::GitSendEmailOptions => &self.git_send_email_options,
            StringOpt::CacheDir => &self.cache_dir,
            StringOpt::DataDir => &self.data_dir,
            StringOpt::GitAmOptions => &self.git_am_options,
            StringOpt::GitAmBranchPrefix => &self.git_am_branch_prefix,
        }
    }

    fn string_opt_mut(&mut self, opt: StringOpt) -> &mut String {
        match opt {
            StringOpt::PatchsetsCacheDir => &mut self.patchsets_cache_dir,
            StringOpt::BookmarkedPatchsetsPath => &mut self.bookmarked_patchsets_path,
            StringOpt::MailingListsPath => &mut self.mailing_lists_path,
            StringOpt::ReviewedPatchsetsPath => &mut self.reviewed_patchsets_path,
            StringOpt::LogsPath => &mut self.logs_path,
            StringOpt::GitSendEmailOptions => &mut self.git_send_email_options,
            StringOpt::CacheDir => &mut self.cache_dir,
            StringOpt::DataDir => &mut self.data_dir,
            StringOpt::GitAmOptions => &mut self.git_am_options,
            StringOpt::GitAmBranchPrefix => &mut self.git_am_branch_prefix,
        }
    }

    fn usize_opt(&self, opt: USizeOpt) -> usize {
        match opt {
            USizeOpt::PageSize => self.page_size,
            USizeOpt::MaxLogAge => self.max_log_age,
        }
    }

    fn usize_opt_mut(&mut self, opt: USizeOpt) -> &mut usize {
        match opt {
            USizeOpt::PageSize => &mut self.page_size,
            USizeOpt::MaxLogAge => &mut self.max_log_age,
        }
    }

    fn set_kernel_tree(&mut self, name: String, val: Option<KernelTree>) {
        if let Some(tree) = val {
            self.kernel_trees.insert(name, tree);
        } else {
            self.kernel_trees.remove(&name);
        }
    }

    fn set_target_kernel_tree(&mut self, val: Option<String>) -> bool {
        if let Some(tree) = val {
            if self.kernel_trees.contains_key(&tree) {
                self.target_kernel_tree = Some(tree);
                true
            } else {
                false
            }
        } else {
            self.target_kernel_tree = None;
            true
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub enum StringOpt {
    /// The directory where the cached patchsets are stored
    PatchsetsCacheDir,
    /// The path to the file where bookmarked patchsets data is stored
    BookmarkedPatchsetsPath,
    /// The path to the file where mailing lists data is stored
    MailingListsPath,
    /// The path to the file where reviewed patchsets data is stored
    ReviewedPatchsetsPath,
    /// Path where to store log files
    LogsPath,
    /// Options to be used with `git send-email`
    GitSendEmailOptions,
    CacheDir,
    DataDir,
    /// Options to be used with `git am` when applying a patchset
    GitAmOptions,
    /// The prefix to use with branches created during a patchset apply
    GitAmBranchPrefix,
}

#[allow(dead_code)]
#[derive(Debug, Eq, PartialEq)]
pub enum USizeOpt {
    /// The count of patchsets to be shown in a page
    PageSize,
    /// The max age in days that a log file can exist before getting auto deleted
    /// 0: forever
    MaxLogAge,
}

#[allow(dead_code)]
enum Command {
    Save(tokio::sync::oneshot::Sender<()>),
    Serialize(tokio::sync::oneshot::Sender<serde_json::Result<String>>),
    Deserialize(String, tokio::sync::oneshot::Sender<serde_json::Result<()>>),
    GetString(StringOpt, tokio::sync::oneshot::Sender<String>),
    SetString(StringOpt, String),
    GetUSize(USizeOpt, tokio::sync::oneshot::Sender<usize>),
    SetUSize(USizeOpt, usize),
    GetPatchRenderer(tokio::sync::oneshot::Sender<PatchRenderer>),
    SetPatchRenderer(PatchRenderer),
    GetCoverRenderer(tokio::sync::oneshot::Sender<CoverRenderer>),
    SetCoverRenderer(CoverRenderer),
    GetKernelTrees(tokio::sync::oneshot::Sender<Vec<String>>),
    GetKernelTree(String, tokio::sync::oneshot::Sender<Option<KernelTree>>),
    SetKernelTree(String, Option<KernelTree>),
    GetTargetKernelTree(tokio::sync::oneshot::Sender<Option<String>>),
    SetTargetKernelTree(Option<String>, tokio::sync::oneshot::Sender<bool>),
}

/// The transmitter that sends messages down to a config actor. This is what you're supossed to use
/// accross the code to manage configuration options, instead of [`Config`]. Cloning it is cheap so
/// do not feel afraid to pass it around.
///
/// The transmitter is obtained by calling [`spawn`] on a [`Config`] instance, consuming it and
/// creating a dedicated task for it. Use the methods of this struct to interact with the logger.
///
/// The intended usage is:
/// - Instantiate the config with [`Config::build`]
/// - Spawn the config actor with [`Config::spawn`]
/// - Use the methods of this struct to manage configurations
///
/// [`spawn`]: Config::spawn
#[derive(Debug, Clone)]
pub struct Config {
    sender: Sender<Command>,
    mode: Mode,
}

/// The operation mode for [`Config`], the only difference is that [`Mode::Mock`]
/// won't do file operations (save and load config)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Default,
    Mock,
}

impl From<ConfigCore> for Config {
    fn from(value: ConfigCore) -> Self {
        value.spawn().0
    }
}

impl Config {
    #[allow(dead_code)]
    pub async fn mock(env: Env, cache_dir: String, data_dir: String) -> Self {
        let mut core = ConfigCore {
            data: Data::new(cache_dir, data_dir),
            env,
        };

        core.apply_env_overrides().await;

        let config = core.spawn().0;
        Config {
            mode: Mode::Mock,
            ..config
        }
    }

    /// Serializes the config data to a [`String`] using [`serde`]
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn serialize(&self) -> serde_json::Result<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.sender
            .send(Command::Serialize(tx))
            .await
            .expect("Config actor died");

        rx.await.expect("Config actor died")
    }

    /// Deserializes a string into the config data
    ///
    /// # Panics
    /// If somehow the config task is finished early
    #[allow(dead_code)]
    async fn deserialize(
        &self,
        content: impl ToString + Sync + Send + 'static,
    ) -> serde_json::Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.sender
            .send(Command::Deserialize(content.to_string(), tx))
            .await
            .expect("Config actor died");

        rx.await.expect("Config actor died")
    }

    /// Save the current configuration to the config file
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn save(&self) {
        if let Mode::Mock = self.mode {
            return;
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::Save(tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died");
    }

    /// Get a [`String`] config option
    /// Available options are defined by [`StringOpt`]
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn string(&self, opt: StringOpt) -> String {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::GetString(opt, tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died")
    }

    /// Set a [`String`] config option
    /// Available options are defined by [`StringOpt`]
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn set_string(&self, opt: StringOpt, val: String) {
        self.sender
            .send(Command::SetString(opt, val))
            .await
            .expect("Config actor died");
    }

    /// Get a [`usize`] config option
    /// Available options are defined by [`USizeOpt`]
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn usize(&self, opt: USizeOpt) -> usize {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::GetUSize(opt, tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died")
    }

    /// Set a [`usize`] config option
    /// Available options are defined by [`USizeOpt`]
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn set_usize(&self, opt: USizeOpt, val: usize) {
        self.sender
            .send(Command::SetUSize(opt, val))
            .await
            .expect("Config actor died");
    }

    /// Get the current [`PatchRenderer`]
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn patch_renderer(&self) -> PatchRenderer {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::GetPatchRenderer(tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died")
    }

    /// Set a [`PatchRenderer`] to be used
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn set_patch_renderer(&self, val: PatchRenderer) {
        self.sender
            .send(Command::SetPatchRenderer(val))
            .await
            .expect("Config actor died");
    }

    /// Get the current [`CoverRenderer`]
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn cover_renderer(&self) -> CoverRenderer {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::GetCoverRenderer(tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died")
    }

    /// Set a [`CoverRenderer`] to be used
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn set_cover_renderer(&self, val: CoverRenderer) {
        self.sender
            .send(Command::SetCoverRenderer(val))
            .await
            .expect("Config actor died");
    }

    /// Get the list of the names of defined kernel trees
    ///
    /// # Panics
    /// If somehow the config task is finished early
    #[allow(dead_code)]
    pub async fn kernel_trees(&self) -> Vec<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::GetKernelTrees(tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died")
    }

    /// Get a single kernel tree by its name. Will return [`None`]
    /// if the tree is not defined
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn kernel_tree(&self, name: String) -> Option<KernelTree> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::GetKernelTree(name, tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died")
    }

    /// Either defines a new or updates the data of a kernel tree.
    /// Using [`None`] as the value will remove it. While using [`Some()`]
    /// will set its value
    ///
    /// # Panics
    /// If somehow the config task is finished early
    #[allow(dead_code)]
    pub async fn set_kernel_tree(&self, name: String, val: Option<KernelTree>) {
        self.sender
            .send(Command::SetKernelTree(name, val))
            .await
            .expect("Config actor died");
    }

    /// Get the current target kernel tree to be used in patch application operations
    ///
    /// If no kernel tree is being targeted, returns [`None`]
    ///
    /// # Panics
    /// If somehow the config task is finished early
    pub async fn target_kernel_tree(&self) -> Option<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::GetTargetKernelTree(tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died")
    }

    /// Set a kernel tree as the target. If the given name do not match the name of a
    /// defined kernel tree, will return [`false`].
    ///
    /// Use [`None`] as the value to unset the target kernel tree
    ///
    /// # Panics
    /// If somehow the config task is finished early
    #[allow(dead_code)]
    pub async fn set_target_kernel_tree(&self, val: Option<String>) -> bool {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.sender
            .send(Command::SetTargetKernelTree(val, tx))
            .await
            .expect("Config actor died");
        rx.await.expect("Config actor died")
    }
}
