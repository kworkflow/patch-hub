use std::ops::ControlFlow;

use crate::app::App;
use clap::Parser;
use cli::Cli;
use handler::run_app;
use logger::{LogLevel, Logger, LoggerActor};

mod app;
mod cli;
mod handler;
mod logger;
mod ui;
mod utils;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let args = Cli::parse();

    let (logger, _) = Logger::build("/tmp", LogLevel::Info, 0).await?.spawn();
    utils::install_hooks(logger.clone())?;

    let mut terminal = utils::init()?;
    let mut app = App::new(logger.clone());

    match args.resolve(terminal, &mut app, logger.clone()) {
        ControlFlow::Break(b) => return b,
        ControlFlow::Continue(t) => terminal = t,
    }

    run_app(terminal, app, logger.clone())?;
    utils::restore()?;

    logger.info("patch-hub finished");
    let _ = logger.flush().await;

    Ok(())
}
