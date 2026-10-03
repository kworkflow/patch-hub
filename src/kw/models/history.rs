use serde::{Deserialize, Serialize};

/// One recorded `git am` application of a lore patchset to a kernel tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KwApplyRecord {
    pub message_id: String,
    pub kernel_tree_id: String,
    /// Snapshot of `KernelTree.path` when the record was written.
    pub tree_path: String,
    pub applied_branch: String,
    pub base_branch: String,
    /// RFC3339 timestamp.
    pub applied_at: String,
}

/// One recorded `kw build` attempt on a kernel tree branch, whether it
/// succeeded or not. Failed attempts are stored so deploy-alone can refuse
/// them instead of treating the tree as never built.
///
/// `message_id`, `arch`, `image_path`, and `kernelrelease` are optional: a
/// build can target a branch no patchset was applied to, `arch` is unknown
/// when image discovery had to glob, and a failed build may never have
/// produced an image or a kernelrelease.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KwBuildRecord {
    pub kernel_tree_id: String,
    /// Snapshot of `KernelTree.path` when the record was written.
    pub tree_path: String,
    pub message_id: Option<String>,
    pub branch: String,
    pub arch: Option<String>,
    pub image_path: Option<String>,
    /// Resolved kw-env `O=` dir at build time, if an env was active.
    pub output_dir: Option<String>,
    pub kernelrelease: Option<String>,
    pub log_path: String,
    /// RFC3339 timestamp. Readers compare parsed timestamps; records with
    /// unparseable `built_at` values sort oldest.
    pub built_at: String,
    pub success: bool,
}
