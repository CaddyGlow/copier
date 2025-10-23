use crate::analyze::jsonrpc::JsonRpcTransport;
use crate::analyze::lsp_config::get_language_id;
use crate::analyze::project_root::ProjectType;
use crate::error::{CopierError, Result};
use lsp_types::*;
use std::path::Path;
use std::process::{Command, Stdio};

pub struct LspClient {
    transport: JsonRpcTransport,
    root_uri: Url,
    project_type: ProjectType,
    initialized: bool,
}

impl LspClient {
    /// Create a new LSP client by spawning the LSP server process
    pub fn new(
        server_cmd: &str,
        args: &[String],
        root_path: &Path,
        project_type: ProjectType,
    ) -> Result<Self> {
        tracing::info!("Spawning LSP server: {} {:?}", server_cmd, args);

        let mut child = Command::new(server_cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to spawn LSP server '{}': {}", server_cmd, e),
                ))
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to capture stdin",
            ))
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to capture stdout",
            ))
        })?;

        let transport = JsonRpcTransport::new(stdin, stdout);

        let root_uri = Url::from_file_path(root_path).map_err(|_| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid root path",
            ))
        })?;

        Ok(Self {
            transport,
            root_uri,
            project_type,
            initialized: false,
        })
    }

    /// Initialize the LSP server
    pub fn initialize(&mut self) -> Result<InitializeResult> {
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            root_uri: Some(self.root_uri.clone()),
            capabilities: ClientCapabilities {
                text_document: Some(TextDocumentClientCapabilities {
                    hover: Some(HoverClientCapabilities {
                        dynamic_registration: Some(false),
                        content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    }),
                    document_symbol: Some(DocumentSymbolClientCapabilities {
                        dynamic_registration: Some(false),
                        hierarchical_document_symbol_support: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let params_value = serde_json::to_value(params).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize initialize params: {}", e),
            ))
        })?;

        self.transport.send_request("initialize", params_value)?;
        let response = self.transport.read_response()?;

        if let Some(error) = response.error {
            return Err(CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Initialize error: {}", error.message),
            )));
        }

        let result: InitializeResult = serde_json::from_value(response.result.ok_or_else(|| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Missing initialize result",
            ))
        })?)
        .map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse initialize result: {}", e),
            ))
        })?;

        // Send initialized notification
        self.transport
            .send_notification("initialized", serde_json::json!({}))?;

        self.initialized = true;
        tracing::info!("LSP client initialized successfully");

        Ok(result)
    }

    /// Open a document in the LSP server
    pub fn did_open(&mut self, file_path: &Path, content: &str) -> Result<()> {
        if !self.initialized {
            return Err(CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "LSP client not initialized",
            )));
        }

        let uri = Url::from_file_path(file_path).map_err(|_| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid file path",
            ))
        })?;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: get_language_id(self.project_type).to_string(),
                version: 1,
                text: content.to_string(),
            },
        };

        let params_value = serde_json::to_value(params).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize didOpen params: {}", e),
            ))
        })?;

        self.transport
            .send_notification("textDocument/didOpen", params_value)?;

        tracing::debug!("Opened document: {:?}", file_path);

        Ok(())
    }

    /// Get document symbols
    pub fn document_symbols(&mut self, uri: &Url) -> Result<DocumentSymbolResponse> {
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let params_value = serde_json::to_value(params).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize documentSymbol params: {}", e),
            ))
        })?;

        self.transport
            .send_request("textDocument/documentSymbol", params_value)?;
        let response = self.transport.read_response()?;

        if let Some(error) = response.error {
            return Err(CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("documentSymbol error: {}", error.message),
            )));
        }

        let result: DocumentSymbolResponse = serde_json::from_value(
            response.result.ok_or_else(|| {
                CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Missing documentSymbol result",
                ))
            })?,
        )
        .map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse documentSymbol result: {}", e),
            ))
        })?;

        Ok(result)
    }

    /// Get hover information at a position
    pub fn hover(&mut self, uri: &Url, position: Position) -> Result<Option<Hover>> {
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
        };

        let params_value = serde_json::to_value(params).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize hover params: {}", e),
            ))
        })?;

        self.transport
            .send_request("textDocument/hover", params_value)?;
        let response = self.transport.read_response()?;

        if let Some(error) = response.error {
            tracing::warn!("Hover error: {}", error.message);
            return Ok(None);
        }

        if let Some(result) = response.result {
            if result.is_null() {
                return Ok(None);
            }

            let hover: Hover = serde_json::from_value(result).map_err(|e| {
                CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to parse hover result: {}", e),
                ))
            })?;

            Ok(Some(hover))
        } else {
            Ok(None)
        }
    }

    /// Shutdown the LSP server
    pub fn shutdown(&mut self) -> Result<()> {
        if !self.initialized {
            return Ok(());
        }

        self.transport
            .send_request("shutdown", serde_json::json!(null))?;
        let _response = self.transport.read_response()?;

        self.transport
            .send_notification("exit", serde_json::json!(null))?;

        tracing::info!("LSP client shutdown");

        Ok(())
    }
}
