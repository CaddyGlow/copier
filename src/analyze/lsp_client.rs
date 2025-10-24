use crate::analyze::jsonrpc::JsonRpcTransport;
use crate::analyze::lsp_config::get_language_id;
use crate::analyze::project_root::ProjectType;
use crate::error::{QuickctxError, Result};
use lsp_types::*;
use std::path::Path;
use std::process::{Child, Command, Stdio};

pub struct LspClient {
    transport: JsonRpcTransport,
    child_process: Option<Child>,
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
            QuickctxError::Io(std::io::Error::other(format!(
                "Failed to spawn LSP server '{}': {}",
                server_cmd, e
            )))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| QuickctxError::Io(std::io::Error::other("Failed to capture stdin")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| QuickctxError::Io(std::io::Error::other("Failed to capture stdout")))?;

        let transport = JsonRpcTransport::new(stdin, stdout);

        let root_uri = Url::from_file_path(root_path).map_err(|_| {
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid root path",
            ))
        })?;

        Ok(Self {
            transport,
            child_process: Some(child),
            root_uri,
            project_type,
            initialized: false,
        })
    }

    /// Initialize the LSP server
    pub fn initialize(&mut self) -> Result<InitializeResult> {
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: self.root_uri.clone(),
                name: "root".to_string(),
            }]),
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
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize initialize params: {}", e),
            ))
        })?;

        let id = self.transport.send_request("initialize", params_value)?;
        let response = self.transport.read_response(id)?;

        if let Some(error) = response.error {
            return Err(QuickctxError::Io(std::io::Error::other(format!(
                "Initialize error: {}",
                error.message
            ))));
        }

        let result: InitializeResult =
            serde_json::from_value(response.result.ok_or_else(|| {
                QuickctxError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Missing initialize result",
                ))
            })?)
            .map_err(|e| {
                QuickctxError::Io(std::io::Error::new(
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
            return Err(QuickctxError::Io(std::io::Error::other(
                "LSP client not initialized",
            )));
        }

        let uri = Url::from_file_path(file_path).map_err(|_| {
            QuickctxError::Io(std::io::Error::new(
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
            QuickctxError::Io(std::io::Error::new(
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
                QuickctxError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to serialize documentSymbol params: {}", e),
                ))
            })?;

            let id = self
                .transport
                .send_request("textDocument/documentSymbol", params_value)?;
            let response = self.transport.read_response(id)?;

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
                return Err(QuickctxError::Io(std::io::Error::other(format!(
                    "documentSymbol error: {}",
                    error.message
                ))));
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
                        QuickctxError::Io(std::io::Error::new(
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
                    return Err(QuickctxError::Io(std::io::Error::new(
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
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize hover params: {}", e),
            ))
        })?;

        let id = self
            .transport
            .send_request("textDocument/hover", params_value)?;
        let response = self.transport.read_response(id)?;

        if let Some(error) = response.error {
            tracing::warn!("Hover error: {}", error.message);
            return Ok(None);
        }

        if let Some(result) = response.result {
            if result.is_null() {
                return Ok(None);
            }

            let hover: Hover = serde_json::from_value(result).map_err(|e| {
                QuickctxError::Io(std::io::Error::new(
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
            return Err(QuickctxError::Io(std::io::Error::other(
                "LSP client not initialized",
            )));
        }

        let params = WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let params_value = serde_json::to_value(params).map_err(|e| {
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize workspace/symbol params: {}", e),
            ))
        })?;

        let id = self
            .transport
            .send_request("workspace/symbol", params_value)?;
        let response = self.transport.read_response(id)?;

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
            return Err(QuickctxError::Io(std::io::Error::other(
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
            QuickctxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize typeDefinition params: {}", e),
            ))
        })?;

        let id = self
            .transport
            .send_request("textDocument/typeDefinition", params_value)?;
        let response = self.transport.read_response(id)?;

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
                    QuickctxError::Io(std::io::Error::new(
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

        // Send shutdown request
        let id = self
            .transport
            .send_request("shutdown", serde_json::json!(null))?;
        let _response = self.transport.read_response(id)?;

        // Send exit notification
        self.transport
            .send_notification("exit", serde_json::json!(null))?;

        // Give the server a brief moment to process the exit notification
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Explicitly kill the child process to ensure clean shutdown
        // This causes stdout to close, which allows the reader thread to exit
        if let Some(mut child) = self.child_process.take() {
            match child.kill() {
                Ok(_) => {
                    tracing::debug!("LSP server process killed");
                    let _ = child.wait(); // Reap the zombie process
                }
                Err(e) => {
                    tracing::warn!("Failed to kill LSP server process: {}", e);
                }
            }
        }

        tracing::info!("LSP client shutdown");

        Ok(())
    }

    /// Collect diagnostics from the LSP server
    /// Waits for the specified timeout to allow diagnostics to arrive
    ///
    /// With the background reader thread, diagnostics notifications are processed
    /// automatically as they arrive. We just need to poll and wait.
    ///
    /// If `expected_file_count` is provided, will only exit early when diagnostics
    /// have been received for all expected files. Otherwise, exits early when
    /// diagnostics stabilize.
    pub fn collect_diagnostics(
        &mut self,
        timeout_ms: u64,
        expected_file_count: Option<usize>,
        progress_display: Option<&crate::analyze::progress::ProgressDisplay>,
    ) -> Result<std::collections::HashMap<Url, Vec<lsp_types::Diagnostic>>> {
        if let Some(count) = expected_file_count {
            tracing::info!(
                "Waiting {}ms for diagnostics to arrive (expecting {} file(s))...",
                timeout_ms,
                count
            );
        } else {
            tracing::info!("Waiting {}ms for diagnostics to arrive...", timeout_ms);
        }

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);
        let poll_interval = std::time::Duration::from_millis(100); // Check every 100ms

        // Create progress bar if we have expected count and progress display
        let progress_bar =
            if let (Some(count), Some(display)) = (expected_file_count, progress_display) {
                let pb = display.progress_bar(count as u64, "[3/3]");
                pb.set_message("Collecting diagnostics");
                Some(pb)
            } else {
                None
            };

        // Keep track of whether we've seen any diagnostics
        let mut last_diag_count = 0;
        let mut stable_count = 0;
        let stable_threshold = 3; // Number of polls with same count before considering stable

        while start.elapsed() < timeout {
            // Sleep briefly to let notifications arrive
            std::thread::sleep(poll_interval);

            // Check if any diagnostics have arrived
            let current_diag_count = self.transport.diagnostics_count();

            // Update progress bar
            if let Some(ref pb) = progress_bar {
                pb.set_position(current_diag_count as u64);
            }

            tracing::debug!("Current diagnostic file count: {}", current_diag_count);

            // Determine if we can exit early
            let can_exit_early = if let Some(expected_count) = expected_file_count {
                // If we expect a specific count, only exit when we have all of them
                if current_diag_count >= expected_count {
                    tracing::info!(
                        "Received diagnostics for all {} expected file(s), exiting early",
                        expected_count
                    );
                    true
                } else {
                    false
                }
            } else {
                // Otherwise, exit when diagnostics stabilize
                if current_diag_count > 0 && current_diag_count == last_diag_count {
                    stable_count += 1;
                    if stable_count >= stable_threshold {
                        tracing::info!(
                            "Diagnostics appear stable at {} file(s), exiting early",
                            current_diag_count
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    stable_count = 0;
                    false
                }
            };

            if can_exit_early {
                break;
            }

            last_diag_count = current_diag_count;
        }

        // Finish progress bar
        if let Some(pb) = progress_bar {
            pb.finish_and_clear();
            eprintln!("[3/3] ✓ Collecting diagnostics");
        }

        // Take all diagnostics that arrived
        let diagnostics_by_uri = self.transport.take_diagnostics();

        if let Some(expected_count) = expected_file_count {
            tracing::info!(
                "Collected diagnostics for {} file(s) (expected {})",
                diagnostics_by_uri.len(),
                expected_count
            );
        } else {
            tracing::info!(
                "Collected diagnostics for {} file(s)",
                diagnostics_by_uri.len()
            );
        }

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
    pub fn wait_for_indexing(
        &mut self,
        timeout_secs: u64,
        progress_mgr: Option<&crate::analyze::progress::LspProgressManager>,
    ) -> Result<()> {
        if !self.initialized {
            return Err(QuickctxError::Io(std::io::Error::other(
                "LSP client not initialized",
            )));
        }

        tracing::info!(
            "Waiting for LSP server to complete indexing (timeout: {}s)...",
            timeout_secs
        );

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let poll_interval = std::time::Duration::from_millis(50); // Faster polling to catch quick progress items

        // Do an initial update before entering the loop to catch progress items
        // that start and finish quickly
        if let Some(mgr) = progress_mgr {
            let progress_states = self.transport.get_progress();
            mgr.update(&progress_states);
        }

        loop {
            // Check if we've exceeded the timeout
            if start.elapsed() >= timeout {
                tracing::warn!("LSP indexing wait timeout exceeded ({}s)", timeout_secs);
                if let Some(mgr) = progress_mgr {
                    mgr.clear();
                }
                break;
            }

            // Sleep a bit to let notifications arrive
            std::thread::sleep(poll_interval);

            // Check current progress state
            let has_active = self.transport.has_active_progress();

            // Update progress display with current states
            if let Some(mgr) = progress_mgr {
                let progress_states = self.transport.get_progress();
                mgr.update(&progress_states);
            }

            // If no active progress and we've waited a reasonable amount of time, we're done
            if !has_active && start.elapsed() > std::time::Duration::from_millis(500) {
                tracing::info!("LSP indexing appears complete");
                if let Some(mgr) = progress_mgr {
                    mgr.clear();
                }
                break;
            }
        }

        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // Ensure the child process is killed when the client is dropped
        if let Some(mut child) = self.child_process.take() {
            tracing::debug!("Cleaning up LSP server process in Drop");
            let _ = child.kill();
            let _ = child.wait(); // Reap zombie
        }
    }
}
