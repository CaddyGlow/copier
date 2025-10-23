use std::collections::BTreeSet;
use std::io::Write;

use camino::{Utf8Path, Utf8PathBuf};
use glob::glob;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use tracing::{debug, warn};

use crate::config::{AggregateConfig, AppContext};
use crate::error::{CopierError, Result};
use crate::fs;
use crate::render;
use crate::utils;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub absolute: Utf8PathBuf,
    pub relative: Utf8PathBuf,
    pub contents: String,
    pub language: Option<String>,
}

pub fn run(context: &AppContext, config: AggregateConfig) -> Result<()> {
    config.require_inputs()?;

    let entries = collect_entries(context, &config)?;
    let document = render::render_entries(&entries, &config);

    let document = document?;

    if let Some(output) = &config.output {
        fs::write(output, document.as_bytes())?;
        debug!(path = %output, "wrote aggregated markdown");
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(document.as_bytes())?;
    }

    Ok(())
}

fn collect_entries(context: &AppContext, config: &AggregateConfig) -> Result<Vec<FileEntry>> {
    let excludes = build_exclude_set(&config.excludes)?;
    let mut paths = BTreeSet::new();

    for input in &config.inputs {
        let expanded = expand_input(context, input)?;
        for path in expanded {
            paths.insert(path);
        }
    }

    let mut entries = Vec::new();
    for path in paths {
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            walk_directory(&path, context, config, excludes.as_ref(), &mut entries)?;
        } else if metadata.is_file() {
            maybe_push_entry(&path, context, config, excludes.as_ref(), &mut entries)?;
        } else {
            debug!(path = %path, "skipping non-regular path");
        }
    }

    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(entries)
}

fn walk_directory(
    dir: &Utf8Path,
    context: &AppContext,
    config: &AggregateConfig,
    excludes: Option<&GlobSet>,
    entries: &mut Vec<FileEntry>,
) -> Result<()> {
    let mut builder = WalkBuilder::new(dir);
    builder.follow_links(false);
    builder.sort_by_file_name(|a, b| a.cmp(b));
    builder.standard_filters(true);

    if config.respect_gitignore {
        builder.git_ignore(true);
        builder.git_global(true);
        builder.git_exclude(true);
        builder.require_git(false);
    } else {
        builder.git_ignore(false);
        builder.git_global(false);
        builder.git_exclude(false);
    }

    for ignore_file in &config.ignore_files {
        builder.add_ignore(ignore_file);
    }

    let walker = builder.build();
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

        maybe_push_entry(&path, context, config, excludes, entries)?;
    }

    Ok(())
}

fn maybe_push_entry(
    path: &Utf8Path,
    context: &AppContext,
    config: &AggregateConfig,
    excludes: Option<&GlobSet>,
    entries: &mut Vec<FileEntry>,
) -> Result<()> {
    if excludes.is_some_and(|e| e.is_match(path.as_std_path())) {
        debug!(path = %path, "excluded by pattern");
        return Ok(());
    }

    if !config.respect_gitignore {
        // ensure we do not skip hidden files; already handled by walker
    }

    let bytes = fs::read(path)?;
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

fn expand_input(context: &AppContext, raw: &str) -> Result<Vec<Utf8PathBuf>> {
    if utils::looks_like_glob(raw) {
        expand_glob_input(context, raw)
    } else {
        let path = Utf8Path::new(raw);
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            context.cwd.join(path)
        };
        Ok(vec![path])
    }
}

fn expand_glob_input(context: &AppContext, pattern: &str) -> Result<Vec<Utf8PathBuf>> {
    let pattern = if Utf8Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        context.cwd.join(pattern).to_string()
    };

    let mut paths = Vec::new();
    let walker = glob(&pattern).map_err(|err| CopierError::InvalidArgument(err.to_string()))?;
    for entry in walker {
        match entry {
            Ok(path) => match Utf8PathBuf::from_path_buf(path) {
                Ok(p) => paths.push(p),
                Err(p) => {
                    warn!(path = %p.to_string_lossy(), "skipping non-utf8 glob match");
                }
            },
            Err(err) => {
                warn!(error = %err, "glob expansion error");
            }
        }
    }

    Ok(paths)
}

fn build_exclude_set(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|err| {
            CopierError::InvalidArgument(format!("invalid exclude pattern {pattern}: {err}"))
        })?;
        builder.add(glob);
    }

    builder
        .build()
        .map(Some)
        .map_err(|err| CopierError::InvalidArgument(format!("failed to build glob set: {err}")))
}
