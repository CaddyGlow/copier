mod collector;
mod glob_expansion;
mod walker_config;

use std::io::Write;

use camino::Utf8PathBuf;
use tracing::debug;

use crate::config::{AggregateConfig, AppContext};
use crate::error::Result;
use crate::render;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub absolute: Utf8PathBuf,
    pub relative: Utf8PathBuf,
    pub contents: String,
    pub language: Option<String>,
}

pub fn run(context: &AppContext, config: AggregateConfig) -> Result<()> {
    config.require_inputs()?;

    let entries = collector::collect_entries(context, &config)?;
    let document = render::render_entries(&entries, &config)?;

    write_output(&config, &document)?;

    Ok(())
}

fn write_output(config: &AggregateConfig, document: &str) -> Result<()> {
    if let Some(output) = &config.output {
        crate::utils::write_with_parent(output, document.as_bytes())?;
        debug!(path = %output, "wrote aggregated markdown");
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(document.as_bytes())?;
    }
    Ok(())
}

