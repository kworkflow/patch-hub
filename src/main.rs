mod app;
mod cli;
mod handler;
mod infrastructure;
mod lore;
mod macros;
mod ui;

use app::{config::Config, App};
use clap::Parser;
use cli::Cli;
use color_eyre::eyre::bail;
use handler::run_app;
use infrastructure::{
    logging::Logger,
    monitoring::{init_monitoring, InitMonitoringProduct},
    terminal::{init, restore},
};
use std::ops::ControlFlow;
use tracing::{event, Level};

fn main() -> color_eyre::Result<()> {
    event!(
        Level::INFO,
        "log before logging initialization (should not appear anywhere)"
    );

    // file writer guards should be propagated to main() so the logging thread lives enough
    let InitMonitoringProduct {
        logging_guards_by_file_name,
        mut multi_log_file_writer,
        logging_reload_handle,
        ..
    } = init_monitoring();

    event!(Level::INFO, "log before config initialization");

    let args = Cli::parse();

    infrastructure::errors::install_hooks()?;
    let mut terminal = init()?;

    let config = Config::build();
    config.create_dirs();

    event!(
        Level::INFO,
        "log after config initialization but before logging layer reload"
    );

    // with the config we can update log directory
    let _guards = multi_log_file_writer.update_log_writer_with_config(
        &config,
        logging_guards_by_file_name,
        logging_reload_handle,
    );

    event!(Level::INFO, "log after logging layer reload");

    match args.resolve(terminal, &config) {
        ControlFlow::Break(b) => return b,
        ControlFlow::Continue(t) => terminal = t,
    }

    let app = App::new(config)?;
    if !app.check_external_deps() {
        Logger::error("patch-hub cannot be executed because some dependencies are missing");
        bail!("patch-hub cannot be executed because some dependencies are missing, check logs for more information");
    }

    run_app(terminal, app)?;
    restore()?;

    Logger::info("patch-hub finished");
    // event! usage example as an alternative for Logger module
    event!(Level::INFO, "patch-hub finished");

    Logger::flush();

    Ok(())
}
