mod app;
mod cli;
mod config;
mod infrastructure;
mod input;
mod kw;
mod lore;
mod macros;
mod render;
mod render_prefs;
mod terminal;
mod ui;

use app::{actor::AppActor, dependencies::check_external_deps, App};
use clap::Parser;
use cli::Cli;
use color_eyre::{eyre::eyre, Result};
use config::{bootstrap_parts, ConfigActor};
use infrastructure::{
    env::OsEnv,
    file_system::{FileSystemTrait, OsFileSystem},
    monitoring::{init_monitoring, InitMonitoringProduct},
    net::UreqNetClient,
    shell::{OsShell, ShellTrait},
    terminal::init,
};
use input::{actor::InputActor, event::InputEvent};
use lore::{
    application::{actor::LoreApiActor, cache::CacheTtl, service::LoreService},
    infrastructure::{
        http_lore_client::HttpLoreGateway,
        patchset_fetcher::B4PatchsetFetcher,
        patchset_parser::MboxPatchsetParser,
        persistence::{FileLorePersistence, MailingListsCacheStore, UserLoreStateStore},
    },
};
use render::{actor::RenderActor, ShellRenderService};
use std::{ops::ControlFlow, sync::Arc};
use terminal::{actor::TerminalActor, session::CrosstermTerminalSession};
use tokio::sync::mpsc;
use tracing::{event, Level};
use ui::actor::UiActor;

#[tokio::main]
async fn main() -> Result<()> {
    // file writer guards should be propagated to main() so the logging thread lives enough
    let InitMonitoringProduct {
        logging_guards_by_file_name,
        mut multi_log_file_writer,
        logging_reload_handle,
        ..
    } = init_monitoring();

    let args = Cli::parse();

    infrastructure::errors::install_hooks()?;

    let env = OsEnv;
    let (config_state, config_repo) = bootstrap_parts(&env, OsFileSystem).map_err(|e| eyre!(e))?;
    let config = config_state.to_snapshot();

    // with the config we can update log directory
    let _guards = multi_log_file_writer.update_log_writer_with_config(
        &config,
        logging_guards_by_file_name,
        logging_reload_handle,
    );

    match args.resolve(&config) {
        ControlFlow::Break(b) => return b,
        ControlFlow::Continue(()) => {}
    }

    check_external_deps(&env, &config)?;

    let config_handle = ConfigActor::spawn(config_state, config_repo);
    let terminal_handle = TerminalActor::spawn(Box::new(CrosstermTerminalSession::new(init()?)));
    let ui_handle = UiActor::spawn();

    // Build shared infrastructure dependencies for LoreService
    let net = Arc::new(UreqNetClient::new());
    let fs_arc: Arc<dyn FileSystemTrait> = Arc::new(OsFileSystem);
    let shell_arc: Arc<dyn ShellTrait> = Arc::new(OsShell);

    let gateway = Arc::new(HttpLoreGateway::new(net));
    let persistence = Arc::new(FileLorePersistence::new(
        fs_arc.clone(),
        config.mailing_lists_path().to_string(),
        config.bookmarked_patchsets_path().to_string(),
        config.reviewed_patchsets_path().to_string(),
    ));
    let fetcher = Arc::new(B4PatchsetFetcher::new(
        shell_arc.clone(),
        fs_arc.clone(),
        config.patchsets_cache_dir().to_string(),
    ));
    let parser = Arc::new(MboxPatchsetParser::new(fs_arc.clone()));

    let render = RenderActor::spawn(Box::new(ShellRenderService::new(shell_arc.clone())));

    let lore_api = LoreApiActor::spawn(LoreService::new(
        gateway.clone(),
        gateway.clone(),
        gateway.clone(),
        persistence.clone() as Arc<dyn MailingListsCacheStore>,
        persistence.clone() as Arc<dyn UserLoreStateStore>,
        fetcher.clone(),
        parser.clone(),
        fs_arc.clone(),
        shell_arc.clone(),
        CacheTtl::default(),
    ));
    let bootstrap = lore_api
        .get_bootstrap_data()
        .await
        .map_err(|error| eyre!("failed to bootstrap Lore data: {error}"))?;

    let app = App::new(
        config_handle
            .get_snapshot()
            .await
            .map_err(|error| eyre!("{error}"))?,
        config_handle.clone(),
        bootstrap,
        Box::new(OsFileSystem),
        Box::new(OsShell),
        lore_api.clone(),
        render.clone(),
    )?;
    let (app_input_tx, app_input_rx) = mpsc::channel::<InputEvent>(64);
    let input_handle = InputActor::spawn(terminal_handle.clone(), app.input_context());
    input_handle
        .subscribe_app(app_input_tx)
        .await
        .map_err(|e| eyre!("{e}"))?;
    let input_shutdown_handle = input_handle.clone();

    // Shutdown ordering:
    //  1. AppActor — exits when the user quits (input channel closes)
    //  2. InputActor — no further terminal input is needed once App is gone
    //  3. ConfigActor — no further configuration requests once App is gone
    //  4. LoreApiActor — no further requests once App is gone
    //  5. RenderActor  — no further requests once App is gone
    //  6. UiActor      — no further scene builds once App is gone
    //  7. TerminalActor — restores the terminal last so the screen stays usable
    //                     during the steps above
    AppActor::spawn(
        app,
        terminal_handle.clone(),
        ui_handle.clone(),
        input_handle,
        app_input_rx,
    )
    .run_until_done()
    .await?;
    input_shutdown_handle
        .shutdown()
        .await
        .map_err(|e| eyre!("{e}"))?;
    config_handle.shutdown().await;
    lore_api.shutdown().await;
    render.shutdown().await;
    ui_handle.shutdown().await;
    terminal_handle
        .shutdown()
        .await
        .map_err(|error| eyre!("{error}"))?;

    event!(Level::INFO, "patch-hub finished");

    Ok(())
}

/// This setup must be done so no test can install the default hooks (when reaching a bail!, for example)
/// before we setup our custom hooks.
#[cfg(test)]
mod test_setup {
    use once_cell::sync::Lazy;

    use crate::infrastructure::errors::install_hooks;

    static INIT_HOOKS: Lazy<()> = Lazy::new(|| {
        install_hooks().expect("Failed to install hooks");
    });

    // This will run before any other test and assure the hooks are installed once
    #[ctor::ctor]
    fn init() {
        Lazy::force(&INIT_HOOKS);
    }
}
