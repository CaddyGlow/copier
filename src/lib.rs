pub mod aggregate;
pub mod cli;
pub mod config;
pub mod error;
pub mod extract;
pub mod fs;
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
        ModeConfig::Aggregate(cfg) => aggregate::run(&runtime.context, cfg),
        ModeConfig::Extract(cfg) => extract::run(&runtime.context, cfg),
    }
}
