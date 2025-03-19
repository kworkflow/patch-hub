use std::ops::ControlFlow;

use crate::app::App;
use app::{config::Config, logging::Logger};
use clap::Parser;
use cli::Cli;
use handler::run_app;

mod app;
mod cli;
mod handler;
mod ui;
mod utils;

fn main() -> color_eyre::Result<()> {
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
    Logger::flush();

    Ok(())
}
