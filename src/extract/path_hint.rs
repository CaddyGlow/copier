use camino::{Utf8Component, Utf8PathBuf};

use crate::error::{CopierError, Result};

/// Acquires a path hint from trailing text or heading
///
/// Priority order:
/// 1. Heading with backticks (most explicit)
/// 2. Last non-empty line of trailing text
pub fn acquire_path_hint(trailing_text: &mut String, heading: Option<String>) -> Option<String> {
    // Heading takes priority if it was inline code (wrapped in backticks)
    if let Some(heading) = heading {
        trailing_text.clear();
        return Some(heading);
    }

    // Otherwise, look for trailing text hint
    let candidate = trailing_text.trim();
    let hint = candidate.lines().rev().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });

    trailing_text.clear();
    hint
}

/// Extracts a comment-style path hint from the beginning of code block contents
///
/// Supports multiple comment styles: //, #, ;, --
/// If found, removes the comment line from contents and returns the path
pub fn extract_comment_hint(contents: &mut String) -> Option<String> {
    let prefix_candidates = ["//", "#", ";", "--"];
    for prefix in &prefix_candidates {
        let marker = format!("{prefix} ");
        if contents.starts_with(&marker) {
            if let Some(idx) = contents.find('\n') {
                let path = contents[marker.len()..idx].trim().to_string();
                let remainder = contents[idx + 1..].to_string();
                *contents = remainder;
                return Some(path);
            } else {
                let path = contents[marker.len()..].trim().to_string();
                contents.clear();
                return Some(path);
            }
        }
    }
    None
}

/// Sanitizes and validates a relative path
///
/// Ensures:
/// - Path is not empty
/// - Path is relative (not absolute)
/// - Path does not contain parent directory segments (..)
pub fn sanitize_relative(raw: &str) -> Result<Utf8PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CopierError::Markdown("empty file path".into()));
    }

    let candidate = Utf8PathBuf::from(trimmed);
    if candidate.is_absolute() {
        return Err(CopierError::Markdown(format!(
            "absolute paths are not allowed: {trimmed}"
        )));
    }

    if candidate
        .components()
        .any(|c| matches!(c, Utf8Component::ParentDir))
    {
        return Err(CopierError::Markdown(format!(
            "parent directory segments are not allowed: {trimmed}"
        )));
    }

    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_path_hint_heading_priority() {
        let mut trailing = "some text".to_string();
        let heading = Some("path.rs".to_string());
        let result = acquire_path_hint(&mut trailing, heading);
        assert_eq!(result, Some("path.rs".to_string()));
        assert!(trailing.is_empty());
    }

    #[test]
    fn test_acquire_path_hint_trailing_text() {
        let mut trailing = "line1\nline2\npath.rs\n".to_string();
        let result = acquire_path_hint(&mut trailing, None);
        assert_eq!(result, Some("path.rs".to_string()));
        assert!(trailing.is_empty());
    }

    #[test]
    fn test_extract_comment_hint_rust() {
        let mut contents = "// path.rs\nfn main() {}".to_string();
        let result = extract_comment_hint(&mut contents);
        assert_eq!(result, Some("path.rs".to_string()));
        assert_eq!(contents, "fn main() {}");
    }

    #[test]
    fn test_extract_comment_hint_python() {
        let mut contents = "# path.py\ndef hello():".to_string();
        let result = extract_comment_hint(&mut contents);
        assert_eq!(result, Some("path.py".to_string()));
        assert_eq!(contents, "def hello():");
    }

    #[test]
    fn test_sanitize_relative_valid() {
        let result = sanitize_relative("src/main.rs");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "src/main.rs");
    }

    #[test]
    fn test_sanitize_relative_absolute() {
        let result = sanitize_relative("/etc/passwd");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("absolute paths are not allowed"));
    }

    #[test]
    fn test_sanitize_relative_parent_dir() {
        let result = sanitize_relative("../etc/passwd");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("parent directory segments are not allowed"));
    }
}
