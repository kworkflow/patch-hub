mod app;
mod cli;
mod handler;
mod infrastructure;
mod macros;
mod ui;

use std::ops::ControlFlow;

use app::{config::Config, App};
use clap::Parser;
use cli::Cli;
use handler::run_app;
use infrastructure::{
    logging::Logger,
    terminal::{init, restore},
};

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

    let app = App::new(config)?;

    run_app(terminal, app)?;
    restore()?;

    Logger::info("patch-hub finished");
    Logger::flush();

    Ok(())
}
