/// Utilities for converting between file paths and LSP Uri types
///
/// The lsp-types 0.97+ uses fluent_uri::Uri which doesn't provide
/// from_file_path/to_file_path methods like the old url::Url did.
use crate::error::{QuickctxError, Result};
use lsp_types::Uri;
use std::path::Path;

/// Convert a file path to an LSP Uri (file:// scheme)
pub fn uri_from_file_path(path: &Path) -> Result<Uri> {
    // Normalize the path to an absolute path
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(QuickctxError::Io)?
            .join(path)
    };

    // Convert to string, ensuring proper path formatting
    let path_str = absolute_path.to_str().ok_or_else(|| {
        QuickctxError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Path contains invalid UTF-8",
        ))
    })?;

    // Build file:// URI string
    // On Unix: file:///path/to/file
    // On Windows: file:///C:/path/to/file
    #[cfg(unix)]
    let uri_string = format!("file://{}", path_str);

    #[cfg(windows)]
    let uri_string = {
        // Windows paths need special handling - replace backslashes
        let normalized = path_str.replace('\\', "/");
        format!("file:///{}", normalized)
    };

    // Parse into Uri using FromStr
    uri_string.parse().map_err(|e| {
        QuickctxError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Failed to parse URI '{}': {}", uri_string, e),
        ))
    })
}

/// Convert an LSP Uri to a file path
pub fn uri_to_file_path(uri: &Uri) -> Result<std::path::PathBuf> {
    let uri_str = uri.as_str();

    // Check that it's a file:// URI
    if !uri_str.starts_with("file://") {
        return Err(QuickctxError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("URI is not a file:// URI: {}", uri_str),
        )));
    }

    // Remove file:// prefix
    let path_part = &uri_str[7..]; // "file://" is 7 chars

    // On Windows, the path starts with /C:/ which needs to become C:/
    // On Unix, the path starts with / which is correct
    #[cfg(unix)]
    let path_str = path_part;

    #[cfg(windows)]
    let path_str = {
        // Remove leading slash if followed by drive letter
        if path_part.len() >= 3
            && path_part.starts_with('/')
            && path_part.chars().nth(2) == Some(':')
        {
            &path_part[1..]
        } else {
            path_part
        }
    };

    // Decode percent-encoding if present
    let decoded = percent_encoding::percent_decode_str(path_str)
        .decode_utf8()
        .map_err(|e| {
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to decode URI path: {}", e),
            ))
        })?;

    Ok(std::path::PathBuf::from(decoded.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_from_file_path() {
        let path = Path::new("/tmp/test.txt");
        let uri = uri_from_file_path(path).unwrap();
        assert!(uri.as_str().starts_with("file:///"));
        assert!(uri.as_str().contains("test.txt"));
    }

    #[test]
    fn test_uri_to_file_path() {
        let uri: Uri = "file:///tmp/test.txt".parse().unwrap();
        let path = uri_to_file_path(&uri).unwrap();
        assert_eq!(path, Path::new("/tmp/test.txt"));
    }

    #[test]
    fn test_roundtrip() {
        let original = Path::new("/tmp/test.txt");
        let uri = uri_from_file_path(original).unwrap();
        let restored = uri_to_file_path(&uri).unwrap();
        assert_eq!(original, restored);
    }
}
