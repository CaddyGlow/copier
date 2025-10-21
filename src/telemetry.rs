use anyhow::anyhow;
use once_cell::sync::OnceCell;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt};

use crate::error::CopierError;

static TELEMETRY: OnceCell<()> = OnceCell::new();

pub fn init(verbosity: u8) -> Result<(), CopierError> {
    TELEMETRY.get_or_try_init(|| {
        let default_level = level_for_verbosity(verbosity);
        let env_filter = EnvFilter::builder()
            .with_default_directive(default_level.into())
            .from_env_lossy();

        fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .try_init()
            .map_err(|err| CopierError::Other(anyhow!(err.to_string())))?;

        Ok::<(), CopierError>(())
    })?;

    Ok(())
}

fn level_for_verbosity(verbosity: u8) -> LevelFilter {
    match verbosity {
        0 => LevelFilter::INFO,
        1 => LevelFilter::DEBUG,
        _ => LevelFilter::TRACE,
    }
}
