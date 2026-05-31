use clap::Parser;
use color_eyre::eyre::eyre;

use std::ops::ControlFlow;

use crate::config::ConfigSnapshot;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[clap(short = 'c', long, action)]
    /// Prints the current configurations to the terminal with the applied overrides
    pub show_configs: bool,
}

impl Cli {
    /// Resolves command line arguments that may finish before the TUI starts.
    ///
    /// Some arguments may finish the program early (returning `ControlFlow::Break`)
    pub fn resolve(&self, config: &ConfigSnapshot) -> ControlFlow<color_eyre::Result<()>, ()> {
        if self.show_configs {
            match serde_json::to_string_pretty(&config) {
                Err(err) => return ControlFlow::Break(Err(eyre!(err))),
                Ok(config) => println!("patch-hub configurations:\n{config}"),
            }

            return ControlFlow::Break(Ok(()));
        }

        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigState;

    #[test]
    fn resolve_continues_when_no_early_cli_action_is_requested() {
        let cli = Cli {
            show_configs: false,
        };
        let config = ConfigState::default().to_snapshot();

        let result = cli.resolve(&config);

        assert!(matches!(result, ControlFlow::Continue(())));
    }

    #[test]
    fn resolve_finishes_after_printing_configs() {
        let cli = Cli { show_configs: true };
        let config = ConfigState::default().to_snapshot();

        let result = cli.resolve(&config);

        assert!(matches!(result, ControlFlow::Break(Ok(()))));
    }
}
