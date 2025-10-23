mod language;

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::error::Result;

pub use language::language_for_path;

pub fn looks_like_glob(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

pub fn relative_to(path: &Utf8Path, base: &Utf8Path) -> Utf8PathBuf {
    path.strip_prefix(base)
        .map(Utf8PathBuf::from)
        .unwrap_or_else(|_| path.to_owned())
}

pub fn is_probably_binary(data: &[u8]) -> bool {
    const SAMPLE_LIMIT: usize = 1024;
    let sample = if data.len() > SAMPLE_LIMIT {
        &data[..SAMPLE_LIMIT]
    } else {
        data
    };

    if sample.contains(&0) {
        return true;
    }

    let control_count = sample
        .iter()
        .filter(|b| {
            let byte = **b;
            (byte < 0x09 || (byte > 0x0D && byte < 0x20))
                && byte != b'\n'
                && byte != b'\r'
                && byte != b'\t'
        })
        .count();

    control_count > sample.len() / 10
}

/// Ensure parent directories exist for the given path
pub fn ensure_parent(path: &Utf8Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent.as_std_path())?;
    }
    Ok(())
}

/// Write data to a file, creating parent directories if needed
pub fn write_with_parent(path: &Utf8Path, data: &[u8]) -> Result<()> {
    ensure_parent(path)?;
    fs::write(path.as_std_path(), data)?;
    Ok(())
}
