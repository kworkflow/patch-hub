use tokio::sync::mpsc;

use crate::{
    app::App,
    config::{ConfigHandle, ConfigState},
    infrastructure::{file_system::MockFileSystemTrait, shell::MockShellTrait},
    lore::application::{cache::BootstrapLoreData, handle::LoreApiHandle},
    render::handle::RenderHandle,
    terminal::{handle::TerminalHandle, messages::TerminalMessage},
};

use super::lore::{sample_mailing_list, sample_patch};

pub(crate) struct AppHarness {
    pub(crate) app: App,
}

impl AppHarness {
    pub(crate) fn new() -> Self {
        Self {
            app: minimal_app(),
        }
    }

    pub(crate) fn with_handles(lore_api: LoreApiHandle, render: RenderHandle) -> Self {
        Self {
            app: app_with_bootstrap_and_handles(bootstrap_data(), lore_api, render),
        }
    }

    pub(crate) fn with_bookmark(lore_api: LoreApiHandle, render: RenderHandle) -> Self {
        let mut bootstrap = bootstrap_data();
        bootstrap.bookmarks = vec![sample_patch()];
        Self {
            app: app_with_bootstrap_and_handles(bootstrap, lore_api, render),
        }
    }
}

pub(crate) fn minimal_app() -> App {
    app_with_bootstrap_and_handles(bootstrap_data(), dummy_lore_handle(), dummy_render_handle())
}

pub(crate) fn app_with_bootstrap_and_handles(
    bootstrap: BootstrapLoreData,
    lore_api: LoreApiHandle,
    render: RenderHandle,
) -> App {
    App::new(
        ConfigState::default().to_snapshot(),
        dummy_config_handle(),
        bootstrap,
        Box::new(MockFileSystemTrait::new()),
        Box::new(MockShellTrait::new()),
        lore_api,
        render,
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

pub(crate) fn dummy_terminal_handle() -> TerminalHandle {
    let (tx, _rx) = mpsc::channel::<TerminalMessage>(1);
    TerminalHandle::new(tx)
}

fn bootstrap_data() -> BootstrapLoreData {
    BootstrapLoreData {
        mailing_lists: vec![sample_mailing_list()],
        bookmarks: vec![],
        reviewed: Default::default(),
    }
}
