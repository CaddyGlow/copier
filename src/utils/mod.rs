mod language;

use camino::{Utf8Path, Utf8PathBuf};

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
