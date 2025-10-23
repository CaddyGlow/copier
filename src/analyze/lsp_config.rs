use super::project_root::ProjectType;

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
}

/// Get the default LSP server configuration for a given project type
pub fn get_lsp_server(project_type: ProjectType) -> LspServerConfig {
    match project_type {
        ProjectType::Rust => LspServerConfig::new("rust-analyzer", vec![]),

        ProjectType::Python => {
            // Prefer pylsp, but pyright is also common
            LspServerConfig::new("pylsp", vec![])
        }

        ProjectType::TypeScript | ProjectType::JavaScript => {
            LspServerConfig::new(
                "typescript-language-server",
                vec!["--stdio".to_string()],
            )
        }

        ProjectType::Go => LspServerConfig::new("gopls", vec![]),

        ProjectType::Unknown => {
            // Default to a common generic LSP server or return an error
            // For now, we'll default to rust-analyzer as a fallback
            LspServerConfig::new("rust-analyzer", vec![])
        }
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
