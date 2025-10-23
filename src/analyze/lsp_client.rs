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
        Self::new_with_paths(server_cmd, args, root_path, project_type, &[])
    }

    /// Create a new LSP client with custom PATH extensions
    pub fn new_with_paths(
        server_cmd: &str,
        args: &[String],
        root_path: &Path,
        project_type: ProjectType,
        bin_paths: &[String],
    ) -> Result<Self> {
        tracing::info!("Spawning LSP server: {} {:?}", server_cmd, args);

        let mut command = Command::new(server_cmd);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Extend PATH if bin_paths are provided
        if !bin_paths.is_empty() {
            let current_path = std::env::var("PATH").unwrap_or_default();
            let expanded_paths: Vec<String> = bin_paths
                .iter()
                .map(|p| shellexpand::tilde(p).to_string())
                .collect();

            let new_path = if current_path.is_empty() {
                expanded_paths.join(":")
            } else {
                format!("{}:{}", expanded_paths.join(":"), current_path)
            };

            tracing::debug!("Extended PATH with: {:?}", expanded_paths);
            command.env("PATH", new_path);
        }

        let mut child = command.spawn().map_err(|e| {
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
                window: Some(WindowClientCapabilities {
                    work_done_progress: Some(true),
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

        let result: InitializeResult =
            serde_json::from_value(response.result.ok_or_else(|| {
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

    /// Get document symbols with retry logic
    pub fn document_symbols(&mut self, uri: &Url) -> Result<DocumentSymbolResponse> {
        // Retry several times with delays to give LSP time to process the document
        // LSP servers like rust-analyzer may need time to build the crate graph
        let max_retries = 6;
        let retry_delay = std::time::Duration::from_millis(1000);

        for attempt in 0..max_retries {
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

            tracing::debug!(
                "documentSymbol response (attempt {}): has_error={}, has_result={}, result_is_null={}",
                attempt + 1,
                response.error.is_some(),
                response.result.is_some(),
                response
                    .result
                    .as_ref()
                    .map(|r| r.is_null())
                    .unwrap_or(false)
            );

            if let Some(error) = response.error {
                return Err(CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("documentSymbol error: {}", error.message),
                )));
            }

            // Check if we have a result
            if let Some(result_value) = response.result {
                // Check if it's null
                if result_value.is_null() {
                    if attempt < max_retries - 1 {
                        tracing::debug!(
                            "documentSymbol returned null, retrying in {:?}...",
                            retry_delay
                        );
                        std::thread::sleep(retry_delay);
                        continue;
                    } else {
                        // Return empty on final retry
                        tracing::warn!(
                            "documentSymbol returned null after {} retries",
                            max_retries
                        );
                        return Ok(DocumentSymbolResponse::Nested(vec![]));
                    }
                }

                // Try to parse the result
                let result: DocumentSymbolResponse =
                    serde_json::from_value(result_value).map_err(|e| {
                        CopierError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Failed to parse documentSymbol result: {}", e),
                        ))
                    })?;

                return Ok(result);
            } else {
                // No result field
                if attempt < max_retries - 1 {
                    tracing::debug!(
                        "documentSymbol missing result, retrying in {:?}...",
                        retry_delay
                    );
                    std::thread::sleep(retry_delay);
                    continue;
                } else {
                    return Err(CopierError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "Missing documentSymbol result after {} retries",
                            max_retries
                        ),
                    )));
                }
            }
        }

        // This should never be reached due to the loop logic, but just in case
        Ok(DocumentSymbolResponse::Nested(vec![]))
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

    /// Search for symbols across the workspace by name
    pub fn workspace_symbol(&mut self, query: &str) -> Result<Vec<SymbolInformation>> {
        if !self.initialized {
            return Err(CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "LSP client not initialized",
            )));
        }

        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let params_value = serde_json::to_value(params).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize workspace/symbol params: {}", e),
            ))
        })?;

        self.transport
            .send_request("workspace/symbol", params_value)?;
        let response = self.transport.read_response()?;

        if let Some(error) = response.error {
            tracing::warn!("workspace/symbol error: {}", error.message);
            return Ok(Vec::new());
        }

        if let Some(result) = response.result {
            if result.is_null() {
                return Ok(Vec::new());
            }

            // The response can be either Vec<SymbolInformation> or Vec<WorkspaceSymbol>
            // Try to parse as Vec<SymbolInformation> first
            match serde_json::from_value::<Vec<SymbolInformation>>(result.clone()) {
                Ok(symbols) => Ok(symbols),
                Err(_) => {
                    // Try parsing as WorkspaceSymbolResponse
                    match serde_json::from_value::<Option<Vec<SymbolInformation>>>(result) {
                        Ok(Some(symbols)) => Ok(symbols),
                        Ok(None) => Ok(Vec::new()),
                        Err(e) => {
                            tracing::warn!("Failed to parse workspace/symbol result: {}", e);
                            Ok(Vec::new())
                        }
                    }
                }
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Get type definition at a position
    pub fn type_definition(
        &mut self,
        uri: &Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        if !self.initialized {
            return Err(CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "LSP client not initialized",
            )));
        }

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let params_value = serde_json::to_value(params).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize typeDefinition params: {}", e),
            ))
        })?;

        self.transport
            .send_request("textDocument/typeDefinition", params_value)?;
        let response = self.transport.read_response()?;

        if let Some(error) = response.error {
            tracing::debug!("typeDefinition error at {:?}: {}", position, error.message);
            return Ok(None);
        }

        if let Some(result) = response.result {
            if result.is_null() {
                return Ok(None);
            }

            // The response can be Location | Location[] | LocationLink[]
            let goto_response: GotoDefinitionResponse =
                serde_json::from_value(result).map_err(|e| {
                    CopierError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to parse typeDefinition result: {}", e),
                    ))
                })?;

            Ok(Some(goto_response))
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

    /// Collect diagnostics from the LSP server
    /// Waits for the specified timeout to allow diagnostics to arrive
    pub fn collect_diagnostics(
        &mut self,
        timeout_ms: u64,
    ) -> Result<std::collections::HashMap<Url, Vec<lsp_types::Diagnostic>>> {
        tracing::info!("Waiting {}ms for diagnostics to arrive...", timeout_ms);

        // Sleep to allow LSP server to send diagnostics notifications
        std::thread::sleep(std::time::Duration::from_millis(timeout_ms));

        // Take all diagnostics that arrived
        let diagnostics_by_uri = self.transport.take_diagnostics();

        tracing::info!(
            "Collected diagnostics for {} file(s)",
            diagnostics_by_uri.len()
        );

        // Convert String URIs to Url type
        let mut result = std::collections::HashMap::new();
        for (uri_str, diagnostics) in diagnostics_by_uri {
            if let Ok(url) = Url::parse(&uri_str) {
                result.insert(url, diagnostics);
            } else {
                tracing::warn!("Failed to parse URI: {}", uri_str);
            }
        }

        Ok(result)
    }

    /// Wait for LSP server to complete initial indexing
    /// This polls for progress notifications and waits until they're all complete
    pub fn wait_for_indexing(&mut self, timeout_secs: u64) -> Result<()> {
        if !self.initialized {
            return Err(CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "LSP client not initialized",
            )));
        }

        tracing::info!(
            "Waiting for LSP server to complete indexing (timeout: {}s)...",
            timeout_secs
        );

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let poll_interval = std::time::Duration::from_millis(200);

        let mut last_progress_report = std::time::Instant::now();
        let report_interval = std::time::Duration::from_secs(1);

        loop {
            // Check if we've exceeded the timeout
            if start.elapsed() >= timeout {
                tracing::warn!("LSP indexing wait timeout exceeded ({}s)", timeout_secs);
                break;
            }

            // Sleep a bit to let notifications arrive
            std::thread::sleep(poll_interval);

            // Check current progress state
            let has_active = self.transport.has_active_progress();

            // Log progress updates periodically
            if last_progress_report.elapsed() >= report_interval {
                let progress_states = self.transport.get_progress();
                if !progress_states.is_empty() {
                    for (_token, state) in progress_states.iter() {
                        match state {
                            crate::analyze::jsonrpc::ProgressState::Begin { title, message } => {
                                tracing::info!(
                                    "  Progress: {} - {}",
                                    title,
                                    message.as_deref().unwrap_or("")
                                );
                            }
                            crate::analyze::jsonrpc::ProgressState::Report {
                                message,
                                percentage,
                            } => {
                                if let Some(pct) = percentage {
                                    tracing::info!(
                                        "  Progress: {}%{}",
                                        pct,
                                        message
                                            .as_ref()
                                            .map(|m| format!(" - {}", m))
                                            .unwrap_or_default()
                                    );
                                }
                            }
                            crate::analyze::jsonrpc::ProgressState::End { message } => {
                                tracing::info!("  Completed: {}", message.as_deref().unwrap_or(""));
                            }
                        }
                    }
                }
                last_progress_report = std::time::Instant::now();
            }

            // If no active progress and we've waited a reasonable amount of time, we're done
            if !has_active && start.elapsed() > std::time::Duration::from_millis(500) {
                tracing::info!("LSP indexing appears complete");
                break;
            }
        }

        Ok(())
    }
}
