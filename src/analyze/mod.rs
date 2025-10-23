pub mod extractor;
pub mod formatter;
pub mod jsonrpc;
pub mod lsp_client;
pub mod lsp_config;
pub mod project_root;

pub use extractor::{extract_symbols, SymbolInfo};
pub use formatter::{get_formatter, Formatter, JsonFormatter, MarkdownFormatter, OutputFormat};
pub use lsp_client::LspClient;
pub use lsp_config::{get_lsp_server, get_lsp_server_with_config, LspServerConfig};
pub use project_root::{detect_project_root, extract_project_name, ProjectType};

// Re-export for convenience
pub use crate::config::AnalyzeSection;
