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
    // file writer guards should be propagated to main() so the logging thread lives enough
    let InitMonitoringProduct {
        file_writer_guards: _file_writer_guards,
    } = init_monitoring();

    let args = Cli::parse();

    utils::install_hooks()?;
    let mut terminal = utils::init()?;

    let config = Config::build();
    config.create_dirs();

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
