use std::ops::ControlFlow;

use crate::app::App;
use app::logging::Logger;
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
    let mut app = App::new();

    match args.resolve(terminal, &mut app) {
        ControlFlow::Break(b) => return b,
        ControlFlow::Continue(t) => terminal = t,
    }

    run_app(terminal, app)?;
    utils::restore()?;

    Logger::info("patch-hub finished");
    Logger::flush();

    Ok(())
}
