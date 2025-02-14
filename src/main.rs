use std::ops::ControlFlow;

use crate::app::App;
use actix::Actor;
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

#[actix::main]
async fn main() -> color_eyre::Result<()> {
    let args = Cli::parse();

    let logger = Logger::new("/tmp", LogLevel::Info)?.start();
    logger.collect_garbage(30).await;

    utils::install_hooks(logger.clone())?;
    let mut terminal = utils::init()?;
    let mut app = App::new(logger.clone());

    match args.resolve(logger.clone(), terminal, &mut app).await {
        ControlFlow::Break(b) => return b,
        ControlFlow::Continue(t) => terminal = t,
    }

    run_app(logger.clone(), terminal, app).await?;
    utils::restore()?;

    logger.info("patch-hub stopped").await;
    logger.flush().await;

    Ok(())
}
