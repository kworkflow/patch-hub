use std::ops::ControlFlow;

use crate::app::App;
use app::{config::Config, logging::Logger};
use clap::Parser;
use cli::Cli;
use handler::run_app;
use monitoring::{init_monitoring, InitMonitoringProduct};
use tracing::{event, Level};

mod app;
mod cli;
mod handler;
mod monitoring;
mod ui;
mod utils;

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

    utils::install_hooks()?;
    let mut terminal = utils::init()?;

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

    run_app(terminal, app)?;
    utils::restore()?;

    Logger::info("patch-hub finished");
    // event! usage example as an alternative for Logger module
    event!(Level::INFO, "patch-hub finished");

    Logger::flush();

    Ok(())
}
