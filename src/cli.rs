use std::ops::ControlFlow;

use clap::Parser;
use color_eyre::eyre::eyre;
use ratatui::{prelude::Backend, Terminal};

use crate::{app::App, logger::Logger, utils};

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[clap(short = 'c', long, action)]
    /// Prints the current configurations to the terminal with the applied overrides
    pub show_configs: bool,
}

impl Cli {
    /// Resolves the command line arguments and applies the necessary changes to the terminal and app
    ///
    /// Some arguments may finish the program early (returning `ControlFlow::Break`)
    pub fn resolve<B: Backend>(
        &self,
        terminal: Terminal<B>,
        app: &mut App,
        logger: Logger,
    ) -> ControlFlow<color_eyre::Result<()>, Terminal<B>> {
        if self.show_configs {
            logger.info("Printing current configurations");
            drop(terminal);
            if let Err(err) = utils::restore() {
                return ControlFlow::Break(Err(eyre!(err)));
            }
            match serde_json::to_string_pretty(&app.config) {
                Err(err) => return ControlFlow::Break(Err(eyre!(err))),
                Ok(config) => println!("patch-hub configurations:\n{}", config),
            }

            return ControlFlow::Break(Ok(()));
        }

        ControlFlow::Continue(terminal)
    }
}
