use camino::{Utf8Path, Utf8PathBuf};
use glob::glob;
use tracing::warn;

use crate::config::AppContext;
use crate::error::{CopierError, Result};
use crate::utils;

/// Expands a single input string (which may be a path or glob pattern) into
/// a vector of concrete paths.
pub fn expand_input(context: &AppContext, raw: &str) -> Result<Vec<Utf8PathBuf>> {
    if utils::looks_like_glob(raw) {
        expand_glob_pattern(context, raw)
    } else {
        expand_simple_path(context, raw)
    }
}

/// Expands a simple path (non-glob) by resolving it relative to the current
/// working directory if it's not absolute.
fn expand_simple_path(context: &AppContext, raw: &str) -> Result<Vec<Utf8PathBuf>> {
    let path = Utf8Path::new(raw);
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        context.cwd.join(path)
    };
    Ok(vec![path])
}

/// Expands a glob pattern into a vector of matching paths.
fn expand_glob_pattern(context: &AppContext, pattern: &str) -> Result<Vec<Utf8PathBuf>> {
    let pattern = normalize_glob_pattern(context, pattern);

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

/// Normalizes a glob pattern by making it absolute if it's relative.
fn normalize_glob_pattern(context: &AppContext, pattern: &str) -> String {
    if Utf8Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        context.cwd.join(pattern).to_string()
    }
}
