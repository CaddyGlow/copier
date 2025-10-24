pub mod extractor;
pub mod formatter;
pub mod jsonrpc;
pub mod lsp_client;
pub mod lsp_config;
pub mod path_types;
pub mod progress;
pub mod project_root;
pub mod symbol_index;
pub mod type_extractor;
pub mod type_resolver;

pub use extractor::{SymbolInfo, extract_symbols};
pub use formatter::{
    FileDiagnostics, FileTypeDependencies, Formatter, JsonFormatter, MarkdownFormatter,
    OutputFormat, ProjectDiagnostics, ProjectTypeDependencies, get_formatter,
};
pub use lsp_client::LspClient;
pub use lsp_config::{
    LspServerConfig, extension_to_project_type, get_lsp_server, get_lsp_server_with_config,
    has_lsp_support,
};
pub use path_types::{FilePath, RelativePath};
pub use project_root::{ProjectType, detect_project_root, extract_project_name};
pub use symbol_index::{SymbolIndex, SymbolLocation};
pub use type_extractor::{TypeContext, TypeExtractor, TypeReference};
pub use type_resolver::{ResolvedType, TypeResolution, TypeResolver};

// Re-export for convenience
pub use crate::config::AnalyzeSection;
