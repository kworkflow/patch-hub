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
    // file writer guards should be propagated to main() so the logging thread lives enough
    let InitMonitoringProduct {
        logging_guards_by_file_name,
        mut multi_log_file_writer,
        logging_reload_handle,
        ..
    } = init_monitoring();

    let args = Cli::parse();

    infrastructure::errors::install_hooks()?;
    let mut terminal = init()?;

    let config = Config::build();
    config.create_dirs();

    // with the config we can update log directory
    let _guards = multi_log_file_writer.update_log_writer_with_config(
        &config,
        logging_guards_by_file_name,
        logging_reload_handle,
    );

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

/// This setup must be done so no test can install the default hooks (when reaching a bail!, for example)
/// before we setup our custom hooks.
#[cfg(test)]
mod test_setup {
    use once_cell::sync::Lazy;

    use crate::infrastructure::errors::install_hooks;

    static INIT_HOOKS: Lazy<()> = Lazy::new(|| {
        install_hooks().expect("Failed to install hooks");
    });

    // This will run before any other test and assure the hooks are installed once
    #[ctor::ctor]
    fn init() {
        Lazy::force(&INIT_HOOKS);
    }
}
