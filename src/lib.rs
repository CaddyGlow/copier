pub mod analyze;
pub mod cli;
pub mod config;
pub mod copy;
pub mod error;
pub mod paste;
pub mod render;
pub mod telemetry;
pub mod utils;

use config::ModeConfig;
pub use error::Result;

use crate::cli::Cli;

pub fn run(cli: Cli) -> Result<()> {
    let runtime = config::load(&cli)?;
    telemetry::init(runtime.context.verbosity)?;

    match runtime.mode {
        ModeConfig::Copy(cfg) => copy::run(&runtime.context, cfg),
        ModeConfig::Paste(cfg) => paste::run(&runtime.context, cfg),
    }
}
