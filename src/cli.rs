use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[clap(long, action)]
    /// Prints the current configurations to the terminal with the applied overrides
    pub show_configs: bool,
}
