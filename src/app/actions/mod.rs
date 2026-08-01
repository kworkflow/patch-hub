pub(crate) mod apply;
pub(crate) mod reviewed_reply;

use color_eyre::Result;

use crate::{
    config::ConfigSnapshot,
    infrastructure::{file_system::FileSystemTrait, shell::ShellTrait},
    lore::application::handle::LoreApiHandle,
};

use apply::ApplyPatchsetRequest;
use reviewed_reply::{ReviewedReplyRequest, ReviewedReplyResult};

pub(crate) struct PatchsetActionService<'a> {
    fs: &'a dyn FileSystemTrait,
    shell: &'a dyn ShellTrait,
    lore_api: &'a LoreApiHandle,
}

impl<'a> PatchsetActionService<'a> {
    pub(crate) fn new(
        fs: &'a dyn FileSystemTrait,
        shell: &'a dyn ShellTrait,
        lore_api: &'a LoreApiHandle,
    ) -> Self {
        Self {
            fs,
            shell,
            lore_api,
        }
    }

    pub(crate) fn apply_patchset(
        &self,
        request: &ApplyPatchsetRequest,
        config: &ConfigSnapshot,
    ) -> Result<String, String> {
        apply::apply_patchset(request, self.fs, self.shell, config)
    }

    pub(crate) async fn execute_reviewed_reply(
        &self,
        request: ReviewedReplyRequest,
    ) -> Result<ReviewedReplyResult> {
        reviewed_reply::execute_reviewed_reply(request, self.lore_api, self.shell).await
    }
}
