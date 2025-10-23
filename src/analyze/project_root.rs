use crate::error::{CopierError, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Unknown,
}

/// Detect the project root by walking up the directory tree looking for marker files
pub fn detect_project_root(file_path: &Path) -> Result<(PathBuf, ProjectType)> {
    // Canonicalize the path to get absolute path
    let canonical_path = file_path.canonicalize().map_err(CopierError::Io)?;

    let start_dir = if canonical_path.is_file() {
        canonical_path
            .parent()
            .ok_or_else(|| CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Cannot determine parent directory",
            )))?
    } else {
        &canonical_path
    };

    let mut current = start_dir.to_path_buf();

    loop {
        // Check for project markers in priority order
        if let Some(proj_type) = check_project_markers(&current) {
            return Ok((current, proj_type));
        }

        // Move up one directory
        if !current.pop() {
            // Reached filesystem root, use start directory as fallback
            return Ok((start_dir.to_path_buf(), ProjectType::Unknown));
        }
    }
}

fn check_project_markers(dir: &Path) -> Option<ProjectType> {
    // Rust
    if dir.join("Cargo.toml").exists() {
        return Some(ProjectType::Rust);
    }

    // Python
    if dir.join("pyproject.toml").exists() || dir.join("setup.py").exists() {
        return Some(ProjectType::Python);
    }

    // JavaScript/TypeScript
    if dir.join("package.json").exists() {
        // Try to distinguish between JS and TS
        if dir.join("tsconfig.json").exists() {
            return Some(ProjectType::TypeScript);
        }
        return Some(ProjectType::JavaScript);
    }

    // Go
    if dir.join("go.mod").exists() {
        return Some(ProjectType::Go);
    }

    // Git root as fallback
    if dir.join(".git").is_dir() {
        return Some(ProjectType::Unknown);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_detect_rust_project() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let src = root.join("src");
        fs::create_dir(&src).unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();
        fs::write(src.join("main.rs"), "").unwrap();

        let (detected_root, proj_type) = detect_project_root(&src.join("main.rs")).unwrap();
        assert_eq!(detected_root, root);
        assert_eq!(proj_type, ProjectType::Rust);
    }

    #[test]
    fn test_detect_python_project() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::write(root.join("pyproject.toml"), "").unwrap();
        fs::write(root.join("main.py"), "").unwrap();

        let (detected_root, proj_type) = detect_project_root(&root.join("main.py")).unwrap();
        assert_eq!(detected_root, root);
        assert_eq!(proj_type, ProjectType::Python);
    }
}
