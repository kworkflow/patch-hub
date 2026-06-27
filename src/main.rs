mod app;
mod cli;
mod config;
mod infrastructure;
mod input;
mod lore;
mod macros;
mod render;
mod render_prefs;
mod terminal;
mod ui;

use app::{actor::AppActor, App};
use clap::Parser;
use cli::Cli;
use color_eyre::eyre::{bail, eyre};
use config::{ConfigService, ConfigServiceApi, ConfigSnapshot};
use infrastructure::{
    env::{EnvTrait, OsEnv},
    file_system::OsFileSystem,
    monitoring::{init_monitoring, InitMonitoringProduct},
    net::UreqNetClient,
    shell::OsShell,
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
use render_prefs::PatchRenderer;
use std::{ops::ControlFlow, sync::Arc};
use terminal::{actor::TerminalActor, session::CrosstermTerminalSession};
use tokio::sync::mpsc;
use tracing::{event, Level};
use ui::actor::UiActor;

/// Verifies required and optional external binaries before the TUI runs.
///
/// Soft dependencies only emit warnings; a missing `b4` makes the app refuse to start.
fn check_external_deps(env: &dyn EnvTrait, config: &ConfigSnapshot) -> bool {
    let mut app_can_run = true;

    if !env.which("b4") {
        event!(
            Level::ERROR,
            "b4 is not installed, patchsets cannot be downloaded"
        );
        app_can_run = false;
    }

    if !env.which("git") {
        event!(Level::WARN, "git is not installed, send-email won't work");
    }

    match config.patch_renderer() {
        PatchRenderer::Bat => {
            if !env.which("bat") {
                event!(
                    Level::WARN,
                    "bat is not installed, patch rendering will fallback to default"
                );
            }
        }
        PatchRenderer::Delta => {
            if !env.which("delta") {
                event!(
                    Level::WARN,
                    "delta is not installed, patch rendering will fallback to default",
                );
            }
        }
        PatchRenderer::DiffSoFancy => {
            if !env.which("diff-so-fancy") {
                event!(
                    Level::WARN,
                    "diff-so-fancy is not installed, patch rendering will fallback to default",
                );
            }
        }
        _ => {}
    }

    app_can_run
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
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
    let config_service: Box<dyn ConfigServiceApi> =
        Box::new(ConfigService::bootstrap(&env, OsFileSystem).map_err(|e| eyre!(e))?);
    let config = config_service.snapshot();

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

    let terminal_handle = TerminalActor::spawn(Box::new(CrosstermTerminalSession::new(init()?)));
    let ui_handle = UiActor::spawn();

    // Build shared infrastructure dependencies for LoreService
    let net = Arc::new(UreqNetClient::new());
    let fs_arc: Arc<dyn infrastructure::file_system::FileSystemTrait> = Arc::new(OsFileSystem);
    let shell_arc: Arc<dyn infrastructure::shell::ShellTrait> = Arc::new(OsShell);

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
    let bootstrap = lore_api.get_bootstrap_data().await.unwrap_or_default();

    let app = App::new(
        config_service,
        bootstrap,
        Box::new(OsFileSystem),
        Box::new(OsShell),
        Box::new(env),
        lore_api,
        render,
    )?;
    if !check_external_deps(&*app.services.env, &app.state.config) {
        event!(
            Level::WARN,
            "patch-hub cannot be executed because some dependencies are missing"
        );
        bail!("patch-hub cannot be executed because some dependencies are missing, check logs for more information");
    }

    let (app_input_tx, app_input_rx) = mpsc::channel::<InputEvent>(64);
    let input_handle = InputActor::spawn(terminal_handle.clone(), app.input_context());
    input_handle
        .subscribe_app(app_input_tx)
        .await
        .map_err(|e| eyre!("{e}"))?;

    AppActor::spawn(
        app,
        terminal_handle.clone(),
        ui_handle.clone(),
        input_handle,
        app_input_rx,
    )
    .run_until_done()
    .await?;
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
