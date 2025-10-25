use std::collections::BTreeSet;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobSet, GlobSetBuilder};
use tracing::{debug, warn};

use crate::config::{AppContext, CopyConfig};
use crate::error::{QuickctxError, Result};
use crate::utils;

use super::FileEntry;
use super::glob_expansion;
use super::walker_config::WalkerConfigBuilder;

/// Collects file entries based on the provided configuration.
pub fn collect_entries(context: &AppContext, config: &CopyConfig) -> Result<Vec<FileEntry>> {
    let excludes = build_exclude_set(&config.excludes)?;
    let paths = expand_all_inputs(context, config)?;
    let mut entries = process_paths(paths, context, config, excludes.as_ref())?;

    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(entries)
}

/// Expands all input paths/globs and deduplicates them.
fn expand_all_inputs(context: &AppContext, config: &CopyConfig) -> Result<BTreeSet<Utf8PathBuf>> {
    let mut paths = BTreeSet::new();

    for input in &config.inputs {
        let expanded = glob_expansion::expand_input(context, input)?;
        for path in expanded {
            paths.insert(path);
        }
    }

    Ok(paths)
}

/// Processes a collection of paths, walking directories and collecting file entries.
fn process_paths(
    paths: BTreeSet<Utf8PathBuf>,
    context: &AppContext,
    config: &CopyConfig,
    excludes: Option<&GlobSet>,
) -> Result<Vec<FileEntry>> {
    let mut entries = Vec::new();

    for path in paths {
        let metadata = fs::metadata(path.as_std_path())?;
        if metadata.is_dir() {
            collect_from_directory(&path, context, config, excludes, &mut entries)?;
        } else if metadata.is_file() {
            try_add_file_entry(&path, context, config, excludes, &mut entries)?;
        } else {
            debug!(path = %path, "skipping non-regular path");
        }
    }

    Ok(entries)
}

/// Walks a directory and collects all file entries within it.
fn collect_from_directory(
    dir: &Utf8Path,
    context: &AppContext,
    config: &CopyConfig,
    excludes: Option<&GlobSet>,
    entries: &mut Vec<FileEntry>,
) -> Result<()> {
    let walker = WalkerConfigBuilder::from_config(dir, config)
        .build()
        .build();

    for result in walker {
        let dir_entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                warn!(error = %err, "failed to read entry, skipping");
                continue;
            }
        };

        let file_type = match dir_entry.file_type() {
            Some(kind) => kind,
            None => continue,
        };

        if !file_type.is_file() {
            continue;
        }

        let path = match Utf8PathBuf::from_path_buf(dir_entry.into_path()) {
            Ok(p) => p,
            Err(p) => {
                warn!(path = %p.to_string_lossy(), "skipping non-utf8 path");
                continue;
            }
        };

        try_add_file_entry(&path, context, config, excludes, entries)?;
    }

    Ok(())
}

/// Attempts to add a file entry, applying exclusion rules and binary file detection.
fn try_add_file_entry(
    path: &Utf8Path,
    context: &AppContext,
    _config: &CopyConfig,
    excludes: Option<&GlobSet>,
    entries: &mut Vec<FileEntry>,
) -> Result<()> {
    if excludes.is_some_and(|e| e.is_match(path.as_std_path())) {
        debug!(path = %path, "excluded by pattern");
        return Ok(());
    }

    let bytes = fs::read(path.as_std_path())?;
    if utils::is_probably_binary(&bytes) {
        warn!(path = %path, "skipping binary file");
        return Ok(());
    }

    let contents = String::from_utf8_lossy(&bytes).into_owned();
    let relative = utils::relative_to(path, &context.cwd);
    let language = utils::language_for_path(path).map(ToString::to_string);

    entries.push(FileEntry {
        absolute: path.to_owned(),
        relative,
        contents,
        language,
    });

    Ok(())
}

/// Builds a GlobSet from exclude patterns.
fn build_exclude_set(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|err| {
            QuickctxError::InvalidArgument(format!("invalid exclude pattern {pattern}: {err}"))
        })?;
        builder.add(glob);
    }

    builder
        .build()
        .map(Some)
        .map_err(|err| QuickctxError::InvalidArgument(format!("failed to build glob set: {err}")))
}
