mod app;
mod cli;
mod config;
mod handler;
mod infrastructure;
mod lore;
mod macros;
mod render_prefs;
mod ui;

use app::{config::Config, patch_renderer::PatchRenderer, App};
use clap::Parser;
use cli::Cli;
use color_eyre::eyre::bail;
use handler::run_app;
use infrastructure::{
    env::{EnvTrait, OsEnv},
    file_system::OsFileSystem,
    monitoring::{init_monitoring, InitMonitoringProduct},
    net::UreqNetClient,
    render::{RenderServiceApi, ShellRenderService},
    shell::OsShell,
    terminal::{init, restore},
};
use lore::{
    application::{api::LoreServiceApi, cache::CacheTtl, service::LoreService},
    infrastructure::{
        http_lore_client::HttpLoreGateway,
        patchset_fetcher::B4PatchsetFetcher,
        patchset_parser::MboxPatchsetParser,
        persistence::{FileLorePersistence, MailingListsCacheStore, UserLoreStateStore},
    },
};
use std::{ops::ControlFlow, sync::Arc};
use tracing::{event, Level};

/// Verifies required and optional external binaries before the TUI runs.
///
/// Soft dependencies only emit warnings; a missing `b4` makes the app refuse to start.
fn check_external_deps(env: &dyn EnvTrait, config: &Config) -> bool {
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

fn main() -> color_eyre::Result<()> {
    // file writer guards should be propagated to main() so the logging thread lives enough
    let InitMonitoringProduct {
        logging_guards_by_file_name,
        mut multi_log_file_writer,
        logging_reload_handle,
        ..
    } = init_monitoring();

    let args = Cli::parse();

    infrastructure::errors::install_hooks()?;
    let mut terminal = init()?;

    let env = OsEnv;
    let config = Config::build(&env, &OsFileSystem);
    config.create_dirs(&OsFileSystem);

    // with the config we can update log directory
    let _guards = multi_log_file_writer.update_log_writer_with_config(
        &config,
        logging_guards_by_file_name,
        logging_reload_handle,
    );

    match args.resolve(terminal, &config) {
        ControlFlow::Break(b) => return b,
        ControlFlow::Continue(t) => terminal = t,
    }

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

    let render: Box<dyn RenderServiceApi> = Box::new(ShellRenderService::new(shell_arc.clone()));

    let lore_service: Box<dyn LoreServiceApi> = Box::new(LoreService::new(
        gateway.clone(),
        gateway.clone(),
        gateway.clone(),
        persistence.clone() as Arc<dyn MailingListsCacheStore>,
        persistence.clone() as Arc<dyn UserLoreStateStore>,
        fetcher,
        parser,
        fs_arc,
        shell_arc,
        CacheTtl::default(),
    ));

    let app = App::new(
        config,
        Box::new(OsFileSystem),
        Box::new(OsShell),
        Box::new(env),
        lore_service,
        render,
    )?;
    if !check_external_deps(&*app.services.env, &app.state.config) {
        event!(
            Level::WARN,
            "patch-hub cannot be executed because some dependencies are missing"
        );
        bail!("patch-hub cannot be executed because some dependencies are missing, check logs for more information");
    }

    run_app(terminal, app)?;
    restore()?;

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
