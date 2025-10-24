use crate::error::{QuickctxError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    let canonical_path = file_path.canonicalize().map_err(QuickctxError::Io)?;

    let start_dir = if canonical_path.is_file() {
        canonical_path.parent().ok_or_else(|| {
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Cannot determine parent directory",
            ))
        })?
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
            // Reached filesystem root, try to detect from file extension
            let project_type = if canonical_path.is_file() {
                detect_type_from_extension(&canonical_path).unwrap_or(ProjectType::Unknown)
            } else {
                ProjectType::Unknown
            };
            return Ok((start_dir.to_path_buf(), project_type));
        }
    }
}

/// Detect project type from file extension
fn detect_type_from_extension(file_path: &Path) -> Option<ProjectType> {
    file_path.extension().and_then(|ext| match ext.to_str()? {
        "rs" => Some(ProjectType::Rust),
        "py" => Some(ProjectType::Python),
        "ts" | "tsx" => Some(ProjectType::TypeScript),
        "js" | "jsx" => Some(ProjectType::JavaScript),
        "go" => Some(ProjectType::Go),
        _ => None,
    })
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

/// Extract the project name from project files
/// Falls back to directory basename if extraction fails
pub fn extract_project_name(root_path: &Path, project_type: ProjectType) -> String {
    match project_type {
        ProjectType::Rust => extract_from_cargo_toml(root_path),
        ProjectType::Python => extract_from_pyproject_toml(root_path),
        ProjectType::TypeScript | ProjectType::JavaScript => extract_from_package_json(root_path),
        ProjectType::Go => extract_from_go_mod(root_path),
        ProjectType::Unknown => Some(get_directory_name(root_path)),
    }
    .unwrap_or_else(|| get_directory_name(root_path))
}

fn extract_from_cargo_toml(root_path: &Path) -> Option<String> {
    let cargo_toml_path = root_path.join("Cargo.toml");
    let content = std::fs::read_to_string(cargo_toml_path).ok()?;

    let parsed: toml::Value = toml::from_str(&content).ok()?;
    let name = parsed.get("package")?.get("name")?.as_str()?;

    Some(name.to_string())
}

fn extract_from_pyproject_toml(root_path: &Path) -> Option<String> {
    let pyproject_path = root_path.join("pyproject.toml");
    let content = std::fs::read_to_string(pyproject_path).ok()?;

    let parsed: toml::Value = toml::from_str(&content).ok()?;

    // Try [project].name first (PEP 621 standard)
    if let Some(name) = parsed
        .get("project")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
    {
        return Some(name.to_string());
    }

    // Try [tool.poetry].name (Poetry)
    if let Some(name) = parsed
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
    {
        return Some(name.to_string());
    }

    None
}

fn extract_from_package_json(root_path: &Path) -> Option<String> {
    let package_json_path = root_path.join("package.json");
    let content = std::fs::read_to_string(package_json_path).ok()?;

    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let name = parsed.get("name")?.as_str()?;

    Some(name.to_string())
}

fn extract_from_go_mod(root_path: &Path) -> Option<String> {
    let go_mod_path = root_path.join("go.mod");
    let content = std::fs::read_to_string(go_mod_path).ok()?;

    // First line should be: module github.com/user/project
    let first_line = content.lines().next()?;
    let module_path = first_line.strip_prefix("module ")?.trim();

    // Extract last segment of module path
    let name = module_path.rsplit('/').next()?;

    Some(name.to_string())
}

fn get_directory_name(root_path: &Path) -> String {
    root_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
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
