use super::project_root::ProjectType;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LspServerConfig {
    pub command: String,
    pub args: Vec<String>,
}

impl LspServerConfig {
    pub fn new(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            command: command.into(),
            args,
        }
    }

    /// Parse a command string that may include arguments
    /// e.g., "rust-analyzer --verbose" → ("rust-analyzer", ["--verbose"])
    pub fn from_command_string(cmd_str: &str) -> Self {
        let parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() {
            return Self::new("", vec![]);
        }

        let command = parts[0].to_string();
        let args = parts[1..].iter().map(|s| s.to_string()).collect();

        Self { command, args }
    }
}

/// Get the default LSP server configuration for a given project type
pub fn get_lsp_server(project_type: ProjectType) -> LspServerConfig {
    get_lsp_server_with_config(project_type, None)
}

/// Get LSP server configuration with optional custom config
pub fn get_lsp_server_with_config(
    project_type: ProjectType,
    custom_config: Option<&HashMap<String, String>>,
) -> LspServerConfig {
    // Check custom config first
    if let Some(config) = custom_config {
        let key = project_type_to_key(project_type);
        if let Some(cmd_str) = config.get(key) {
            return LspServerConfig::from_command_string(cmd_str);
        }
    }

    // Fall back to defaults
    match project_type {
        ProjectType::Rust => LspServerConfig::new("rust-analyzer", vec![]),

        ProjectType::Python => {
            // Prefer pylsp, but pyright is also common
            LspServerConfig::new("pylsp", vec![])
        }

        ProjectType::TypeScript | ProjectType::JavaScript => {
            LspServerConfig::new("typescript-language-server", vec!["--stdio".to_string()])
        }

        ProjectType::Go => LspServerConfig::new("gopls", vec![]),

        ProjectType::Unknown => {
            // Default to a common generic LSP server or return an error
            // For now, we'll default to rust-analyzer as a fallback
            LspServerConfig::new("rust-analyzer", vec![])
        }
    }
}

/// Convert project type to config key
fn project_type_to_key(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Rust => "rust",
        ProjectType::Python => "python",
        ProjectType::TypeScript => "typescript",
        ProjectType::JavaScript => "javascript",
        ProjectType::Go => "go",
        ProjectType::Unknown => "unknown",
    }
}

/// Get language ID for LSP textDocument/didOpen notification
pub fn get_language_id(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Rust => "rust",
        ProjectType::Python => "python",
        ProjectType::TypeScript => "typescript",
        ProjectType::JavaScript => "javascript",
        ProjectType::Go => "go",
        ProjectType::Unknown => "plaintext",
    }
}

/// Map file extension to ProjectType
/// Returns None if the extension doesn't have LSP support
pub fn extension_to_project_type(extension: &str) -> Option<ProjectType> {
    match extension.to_lowercase().as_str() {
        "rs" => Some(ProjectType::Rust),
        "py" | "pyi" => Some(ProjectType::Python),
        "js" | "jsx" | "mjs" | "cjs" => Some(ProjectType::JavaScript),
        "ts" | "tsx" | "mts" | "cts" => Some(ProjectType::TypeScript),
        "go" => Some(ProjectType::Go),
        _ => None,
    }
}

/// Check if a file has LSP support based on its extension
pub fn has_lsp_support(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(extension_to_project_type)
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_config() {
        let config = get_lsp_server(ProjectType::Rust);
        assert_eq!(config.command, "rust-analyzer");
        assert!(config.args.is_empty());
    }

    #[test]
    fn test_typescript_config() {
        let config = get_lsp_server(ProjectType::TypeScript);
        assert_eq!(config.command, "typescript-language-server");
        assert_eq!(config.args, vec!["--stdio"]);
    }

    #[test]
    fn test_language_ids() {
        assert_eq!(get_language_id(ProjectType::Rust), "rust");
        assert_eq!(get_language_id(ProjectType::Python), "python");
        assert_eq!(get_language_id(ProjectType::TypeScript), "typescript");
        assert_eq!(get_language_id(ProjectType::JavaScript), "javascript");
        assert_eq!(get_language_id(ProjectType::Go), "go");
    }
}
