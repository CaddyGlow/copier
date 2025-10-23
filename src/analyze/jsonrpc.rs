use crate::error::{CopierError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Progress tracking state
#[derive(Debug, Clone)]
pub enum ProgressState {
    Begin {
        title: String,
        message: Option<String>,
    },
    Report {
        message: Option<String>,
        percentage: Option<u32>,
    },
    End {
        message: Option<String>,
    },
}

/// JSON-RPC 2.0 transport over stdio
pub struct JsonRpcTransport {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    request_id: AtomicU64,
    diagnostics: Arc<Mutex<HashMap<String, Vec<lsp_types::Diagnostic>>>>,
    progress: Arc<Mutex<HashMap<String, ProgressState>>>,
}

impl JsonRpcTransport {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin,
            stdout: BufReader::new(stdout),
            request_id: AtomicU64::new(1),
            diagnostics: Arc::new(Mutex::new(HashMap::new())),
            progress: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Generate next request ID
    pub fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a request and return the request ID
    pub fn send_request(
        &mut self,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<u64> {
        let id = self.next_id();
        let request = JsonRpcRequest::new(id, method, params);
        self.write_message(&request)?;
        Ok(id)
    }

    /// Send a notification (no response expected)
    pub fn send_notification(
        &mut self,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<()> {
        let notification = JsonRpcNotification::new(method, params);
        self.write_message(&notification)?;
        Ok(())
    }

    /// Read a response from the LSP server
    /// LSP servers can send both responses (with id) and notifications (without id).
    /// This method skips notifications and returns only responses.
    pub fn read_response(&mut self) -> Result<JsonRpcResponse> {
        loop {
            let headers = self.read_headers()?;
            let content_length = headers
                .get("content-length")
                .ok_or_else(|| {
                    CopierError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Missing Content-Length header",
                    ))
                })?
                .parse::<usize>()
                .map_err(|e| {
                    CopierError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Invalid Content-Length: {}", e),
                    ))
                })?;

            let mut content = vec![0u8; content_length];
            self.stdout
                .read_exact(&mut content)
                .map_err(CopierError::Io)?;

            // Parse as generic JSON first to check if it's a notification or response
            let json: serde_json::Value = serde_json::from_slice(&content).map_err(|e| {
                CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to parse JSON: {}", e),
                ))
            })?;

            // Check if it has a 'method' field - if so, it's either a notification or server-to-client request
            if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
                // It's either a notification (no id) or a server request (has id)
                // Either way, we handle it and continue reading

                // Handle publishDiagnostics notifications
                if method == "textDocument/publishDiagnostics" {
                    if let Some(params) = json.get("params")
                        && let Err(e) = self.handle_diagnostics_notification(params) {
                        tracing::warn!("Failed to handle diagnostics: {}", e);
                    }
                    continue;
                }

                // Handle progress notifications
                if method == "$/progress" {
                    if let Some(params) = json.get("params")
                        && let Err(e) = self.handle_progress_notification(params) {
                        tracing::warn!("Failed to handle progress: {}", e);
                    }
                    continue;
                }

                // Handle server-to-client requests (like window/workDoneProgress/create)
                // We need to send back a response
                if let Some(id) = json.get("id").and_then(|i| i.as_u64()) {
                    tracing::debug!(
                        "Received server request '{}' with id {}, sending empty response",
                        method,
                        id
                    );
                    // Send an empty success response
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": null
                    });
                    if let Err(e) = self.write_message(&response) {
                        tracing::warn!("Failed to send response to server request: {}", e);
                    }
                    continue;
                }

                // Other notifications (no id) - just log and skip
                tracing::debug!("Received notification '{}' (skipping)", method);
                continue;
            }

            // It's a response with an id, parse it properly
            tracing::debug!(
                "Raw JSON-RPC response: {}",
                serde_json::to_string(&json).unwrap_or_else(|_| "failed to serialize".to_string())
            );

            let response: JsonRpcResponse = serde_json::from_value(json).map_err(|e| {
                CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to parse JSON-RPC response: {}", e),
                ))
            })?;

            if let Some(error) = &response.error {
                tracing::error!("JSON-RPC error: {:?}", error);
            }

            return Ok(response);
        }
    }

    /// Write a message with LSP headers
    fn write_message<T: Serialize>(&mut self, message: &T) -> Result<()> {
        let json = serde_json::to_string(message).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize message: {}", e),
            ))
        })?;

        let content = format!("Content-Length: {}\r\n\r\n{}", json.len(), json);

        self.stdin
            .write_all(content.as_bytes())
            .map_err(CopierError::Io)?;
        self.stdin.flush().map_err(CopierError::Io)?;

        tracing::debug!("Sent: {}", json);

        Ok(())
    }

    /// Read headers until empty line
    fn read_headers(&mut self) -> Result<std::collections::HashMap<String, String>> {
        let mut headers = std::collections::HashMap::new();
        let mut line = String::new();

        loop {
            line.clear();
            self.stdout.read_line(&mut line).map_err(CopierError::Io)?;

            // Empty line marks end of headers
            if line == "\r\n" || line == "\n" {
                break;
            }

            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }

        Ok(headers)
    }

    /// Handle a publishDiagnostics notification
    fn handle_diagnostics_notification(&self, params: &serde_json::Value) -> Result<()> {
        #[derive(Deserialize)]
        struct PublishDiagnosticsParams {
            uri: String,
            diagnostics: Vec<lsp_types::Diagnostic>,
        }

        let params: PublishDiagnosticsParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Failed to parse diagnostics params: {}", e),
                ))
            })?;

        tracing::debug!(
            "Received {} diagnostic(s) for {}",
            params.diagnostics.len(),
            params.uri
        );

        let mut diagnostics = self.diagnostics.lock().unwrap();
        diagnostics.insert(params.uri, params.diagnostics);

        Ok(())
    }

    /// Take all collected diagnostics and clear the internal storage
    pub fn take_diagnostics(&self) -> HashMap<String, Vec<lsp_types::Diagnostic>> {
        let mut diagnostics = self.diagnostics.lock().unwrap();
        std::mem::take(&mut *diagnostics)
    }

    /// Handle a $/progress notification
    fn handle_progress_notification(&self, params: &serde_json::Value) -> Result<()> {
        #[derive(Deserialize)]
        struct ProgressParams {
            token: serde_json::Value, // Can be string or number
            value: serde_json::Value,
        }

        let params: ProgressParams = serde_json::from_value(params.clone()).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse progress params: {}", e),
            ))
        })?;

        // Convert token to string for storage
        let token_str = match params.token {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            _ => return Ok(()), // Ignore invalid tokens
        };

        // Parse the value field to determine progress state
        let state = if let Some(kind) = params.value.get("kind").and_then(|k| k.as_str()) {
            match kind {
                "begin" => {
                    let title = params
                        .value
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    let message = params
                        .value
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string());

                    tracing::debug!(
                        "Progress begin: {} - {}",
                        title,
                        message.as_deref().unwrap_or("")
                    );
                    ProgressState::Begin { title, message }
                }
                "report" => {
                    let message = params
                        .value
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string());
                    let percentage = params
                        .value
                        .get("percentage")
                        .and_then(|p| p.as_u64())
                        .map(|p| p as u32);

                    tracing::debug!(
                        "Progress report: {} ({}%)",
                        message.as_deref().unwrap_or(""),
                        percentage.unwrap_or(0)
                    );
                    ProgressState::Report {
                        message,
                        percentage,
                    }
                }
                "end" => {
                    let message = params
                        .value
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string());

                    tracing::debug!("Progress end: {}", message.as_deref().unwrap_or(""));
                    ProgressState::End { message }
                }
                _ => return Ok(()),
            }
        } else {
            return Ok(());
        };

        let mut progress = self.progress.lock().unwrap();
        progress.insert(token_str, state);

        Ok(())
    }

    /// Get current progress states
    pub fn get_progress(&self) -> HashMap<String, ProgressState> {
        self.progress.lock().unwrap().clone()
    }

    /// Check if any progress tokens are still active (not ended)
    pub fn has_active_progress(&self) -> bool {
        let progress = self.progress.lock().unwrap();
        progress
            .values()
            .any(|state| !matches!(state, ProgressState::End { .. }))
    }
}
