use std::sync::OnceLock;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt};

use crate::error::QuickctxError;

static TELEMETRY: OnceLock<()> = OnceLock::new();

pub fn init(verbosity: u8) -> Result<(), QuickctxError> {
    // Check if already initialized
    if TELEMETRY.get().is_some() {
        return Ok(());
    }

    let default_level = level_for_verbosity(verbosity);
    let env_filter = EnvFilter::builder()
        .with_default_directive(default_level.into())
        .from_env_lossy();

    fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .try_init()
        .map_err(|err| QuickctxError::TelemetryInit(err.to_string()))?;

    // Set the flag - if another thread beat us to it, that's fine
    let _ = TELEMETRY.set(());
    Ok(())
}

fn level_for_verbosity(verbosity: u8) -> LevelFilter {
    match verbosity {
        0 => LevelFilter::INFO,
        1 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    }
}
