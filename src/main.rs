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
    // We use an unused var because we only parse for `-h|--help` and `-V|--version`
    let _args = Cli::parse();

    utils::install_hooks()?;
    let mut terminal = utils::init()?;
    let mut app = App::new();
    run_app(&mut terminal, &mut app)?;
    utils::restore()?;

    Logger::info("patch-hub finished");
    Logger::flush();

    Ok(())
}
