mod app;
mod cli;
mod handler;
mod infrastructure;
mod lore;
mod macros;
mod model;
mod viewmodels;
mod views;

use clap::Parser;
use cli::Cli;
use color_eyre::eyre::bail;
use handler::run_app;
use infrastructure::{
    logging::Logger,
    terminal::{init, restore},
};
use model::{config::Config, Model};
use std::ops::ControlFlow;

fn main() -> color_eyre::Result<()> {
    let args = Cli::parse();

    infrastructure::errors::install_hooks()?;
    let mut terminal = init()?;

    let config = Config::build();
    config.create_dirs();

    match args.resolve(terminal, &config) {
        ControlFlow::Break(b) => return b,
        ControlFlow::Continue(t) => terminal = t,
    }

    let model = Model::new(config)?;
    if !model.check_external_deps() {
        Logger::error("patch-hub cannot be executed because some dependencies are missing");
        bail!("patch-hub cannot be executed because some dependencies are missing, check logs for more information");
    }

    run_app(terminal, model)?;
    restore()?;

    Logger::info("patch-hub finished");
    Logger::flush();

    Ok(())
}
