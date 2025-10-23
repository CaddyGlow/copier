use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// A validated absolute file path
///
/// This newtype wraps PathBuf and ensures the path is absolute.
/// It provides type safety to prevent mixing absolute and relative paths.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FilePath(PathBuf);

impl FilePath {
    /// Create a new FilePath from an absolute path
    ///
    /// # Errors
    /// Returns an error if the path is not absolute
    pub fn new(path: PathBuf) -> Result<Self, String> {
        if path.is_absolute() {
            Ok(Self(path))
        } else {
            Err(format!("Path must be absolute: {}", path.display()))
        }
    }

    /// Create a FilePath from an absolute path without validation
    ///
    /// # Safety
    /// Caller must ensure the path is absolute
    pub fn from_absolute_unchecked(path: PathBuf) -> Self {
        Self(path)
    }

    /// Get the inner PathBuf
    pub fn as_path_buf(&self) -> &PathBuf {
        &self.0
    }

    /// Convert into inner PathBuf
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for FilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl From<FilePath> for PathBuf {
    fn from(file_path: FilePath) -> Self {
        file_path.0
    }
}

/// A validated relative file path
///
/// This newtype wraps PathBuf and ensures the path is relative.
/// It provides type safety to prevent mixing absolute and relative paths.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelativePath(PathBuf);

impl RelativePath {
    /// Create a new RelativePath from a relative path
    ///
    /// # Errors
    /// Returns an error if the path is absolute
    pub fn new(path: PathBuf) -> Result<Self, String> {
        if path.is_relative() {
            Ok(Self(path))
        } else {
            Err(format!("Path must be relative: {}", path.display()))
        }
    }

    /// Create a RelativePath from a String
    pub fn from_string(s: String) -> Self {
        Self(PathBuf::from(s))
    }

    /// Create a RelativePath from a relative path without validation
    ///
    /// # Safety
    /// Caller must ensure the path is relative
    pub fn from_relative_unchecked(path: PathBuf) -> Self {
        Self(path)
    }

    /// Get the inner PathBuf
    pub fn as_path_buf(&self) -> &PathBuf {
        &self.0
    }

    /// Convert into inner PathBuf
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl AsRef<Path> for RelativePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl From<RelativePath> for PathBuf {
    fn from(relative_path: RelativePath) -> Self {
        relative_path.0
    }
}

impl From<RelativePath> for String {
    fn from(relative_path: RelativePath) -> Self {
        relative_path.0.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_path_absolute() {
        let path = PathBuf::from("/absolute/path/to/file.rs");
        let file_path = FilePath::new(path.clone()).unwrap();
        assert_eq!(file_path.as_path_buf(), &path);
    }

    #[test]
    fn test_file_path_rejects_relative() {
        let path = PathBuf::from("relative/path/to/file.rs");
        assert!(FilePath::new(path).is_err());
    }

    #[test]
    fn test_relative_path_relative() {
        let path = PathBuf::from("relative/path/to/file.rs");
        let rel_path = RelativePath::new(path.clone()).unwrap();
        assert_eq!(rel_path.as_path_buf(), &path);
    }

    #[test]
    fn test_relative_path_rejects_absolute() {
        let path = PathBuf::from("/absolute/path/to/file.rs");
        assert!(RelativePath::new(path).is_err());
    }

    #[test]
    fn test_display() {
        let file_path = FilePath::from_absolute_unchecked(PathBuf::from("/test/file.rs"));
        assert_eq!(file_path.to_string(), "/test/file.rs");

        let rel_path = RelativePath::from_relative_unchecked(PathBuf::from("src/lib.rs"));
        assert_eq!(rel_path.to_string(), "src/lib.rs");
    }
}
