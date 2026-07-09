use tokio::sync::mpsc;

use crate::{
    app::App,
    config::{ConfigHandle, ConfigState},
    infrastructure::{file_system::MockFileSystemTrait, shell::MockShellTrait},
    lore::application::{cache::BootstrapLoreData, handle::LoreApiHandle},
    render::handle::RenderHandle,
};

use super::lore::sample_mailing_list;

pub(crate) struct AppHarness {
    pub(crate) app: App,
}

impl AppHarness {
    pub(crate) fn new() -> Self {
        Self {
            app: minimal_app(),
        }
    }
}

pub(crate) fn minimal_app() -> App {
    App::new(
        ConfigState::default().to_snapshot(),
        dummy_config_handle(),
        bootstrap_data(),
        Box::new(MockFileSystemTrait::new()),
        Box::new(MockShellTrait::new()),
        dummy_lore_handle(),
        dummy_render_handle(),
    )
    .expect("minimal app should build")
}

pub(crate) fn dummy_config_handle() -> ConfigHandle {
    let (tx, _rx) = mpsc::channel(1);
    ConfigHandle::new(tx)
}

pub(crate) fn dummy_lore_handle() -> LoreApiHandle {
    let (tx, _rx) = mpsc::channel(1);
    LoreApiHandle::new(tx)
}

pub(crate) fn dummy_render_handle() -> RenderHandle {
    let (tx, _rx) = mpsc::channel(1);
    RenderHandle::new(tx)
}

fn bootstrap_data() -> BootstrapLoreData {
    BootstrapLoreData {
        mailing_lists: vec![sample_mailing_list()],
        bookmarks: vec![],
        reviewed: Default::default(),
    }
}
