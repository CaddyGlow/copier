use crate::error::{CopierError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

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

/// JSON-RPC 2.0 transport over stdio with background reader thread
pub struct JsonRpcTransport {
    stdin: Arc<Mutex<ChildStdin>>,
    request_id: AtomicU64,
    diagnostics: Arc<Mutex<HashMap<String, Vec<lsp_types::Diagnostic>>>>,
    progress: Arc<Mutex<HashMap<String, ProgressState>>>,
    // Response routing: maps request ID to channel sender (for reader thread)
    pending_responses: Arc<Mutex<HashMap<u64, Sender<JsonRpcResponse>>>>,
    // Response receivers: maps request ID to channel receiver (for read_response)
    pending_receivers: Arc<Mutex<HashMap<u64, Receiver<JsonRpcResponse>>>>,
    // Background reader thread handle
    reader_thread: Option<JoinHandle<()>>,
}

impl JsonRpcTransport {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        let stdin = Arc::new(Mutex::new(stdin));
        let diagnostics = Arc::new(Mutex::new(HashMap::new()));
        let progress = Arc::new(Mutex::new(HashMap::new()));
        let pending_responses = Arc::new(Mutex::new(HashMap::new()));
        let pending_receivers = Arc::new(Mutex::new(HashMap::new()));

        // Clone Arcs for the reader thread
        let stdin_clone = Arc::clone(&stdin);
        let diagnostics_clone = Arc::clone(&diagnostics);
        let progress_clone = Arc::clone(&progress);
        let pending_responses_clone = Arc::clone(&pending_responses);

        // Spawn background reader thread
        let reader_thread = thread::spawn(move || {
            Self::reader_thread_main(
                BufReader::new(stdout),
                stdin_clone,
                diagnostics_clone,
                progress_clone,
                pending_responses_clone,
            );
        });

        Self {
            stdin,
            request_id: AtomicU64::new(1),
            diagnostics,
            progress,
            pending_responses,
            pending_receivers,
            reader_thread: Some(reader_thread),
        }
    }

    /// Background reader thread that continuously processes messages from stdout
    fn reader_thread_main(
        mut stdout: BufReader<ChildStdout>,
        stdin: Arc<Mutex<ChildStdin>>,
        diagnostics: Arc<Mutex<HashMap<String, Vec<lsp_types::Diagnostic>>>>,
        progress: Arc<Mutex<HashMap<String, ProgressState>>>,
        pending_responses: Arc<Mutex<HashMap<u64, Sender<JsonRpcResponse>>>>,
    ) {
        loop {
            // Read message headers
            let headers = match Self::read_headers_static(&mut stdout) {
                Ok(h) => h,
                Err(_) => {
                    tracing::debug!("Reader thread: EOF or error reading headers, exiting");
                    break;
                }
            };

            let content_length = match headers.get("content-length") {
                Some(len) => match len.parse::<usize>() {
                    Ok(l) => l,
                    Err(_) => {
                        tracing::warn!("Reader thread: invalid Content-Length");
                        continue;
                    }
                },
                None => {
                    tracing::warn!("Reader thread: missing Content-Length header");
                    continue;
                }
            };

            // Read message content
            let mut content = vec![0u8; content_length];
            if let Err(e) = stdout.read_exact(&mut content) {
                tracing::debug!("Reader thread: error reading content: {}", e);
                break;
            }

            // Parse JSON
            let json: serde_json::Value = match serde_json::from_slice(&content) {
                Ok(j) => j,
                Err(e) => {
                    tracing::warn!("Reader thread: failed to parse JSON: {}", e);
                    continue;
                }
            };

            // Route message based on type
            // Check for method field first (notification or server request)
            let method_opt = json
                .get("method")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string());

            if let Some(method) = method_opt {
                // It's a notification or server request
                Self::handle_notification_or_request(json, method, &stdin, &diagnostics, &progress);
            } else if let Some(id) = json.get("id").and_then(|i| i.as_u64()) {
                // It's a response to one of our requests
                Self::route_response(json, id, &pending_responses);
            } else {
                tracing::debug!("Reader thread: unknown message type");
            }
        }

        tracing::debug!("Reader thread exiting");
    }

    /// Helper to read headers (static version for reader thread)
    fn read_headers_static(
        reader: &mut BufReader<ChildStdout>,
    ) -> std::io::Result<HashMap<String, String>> {
        let mut headers = HashMap::new();
        let mut line = String::new();

        loop {
            line.clear();
            reader.read_line(&mut line)?;

            if line == "\r\n" || line == "\n" {
                break;
            }

            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }

        Ok(headers)
    }

    /// Handle incoming notification or server request
    fn handle_notification_or_request(
        json: serde_json::Value,
        method: String,
        stdin: &Arc<Mutex<ChildStdin>>,
        diagnostics: &Arc<Mutex<HashMap<String, Vec<lsp_types::Diagnostic>>>>,
        progress: &Arc<Mutex<HashMap<String, ProgressState>>>,
    ) {
        match method.as_str() {
            "textDocument/publishDiagnostics" => {
                if let Some(params) = json.get("params") {
                    if let Err(e) = Self::handle_diagnostics_static(params, diagnostics) {
                        tracing::warn!("Failed to handle diagnostics: {}", e);
                    }
                }
            }
            "$/progress" => {
                if let Some(params) = json.get("params") {
                    if let Err(e) = Self::handle_progress_static(params, progress) {
                        tracing::warn!("Failed to handle progress: {}", e);
                    }
                }
            }
            _ => {
                // Check if it's a server request (has id)
                if let Some(id) = json.get("id").and_then(|i| i.as_u64()) {
                    tracing::debug!("Reader thread: server request '{}' id={}", method, id);
                    // Send empty success response
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": null
                    });
                    if let Ok(mut stdin_lock) = stdin.lock() {
                        let _ = Self::write_message_static(&mut *stdin_lock, &response);
                    }
                } else {
                    tracing::debug!("Reader thread: notification '{}' (ignoring)", method);
                }
            }
        }
    }

    /// Route response to waiting channel
    fn route_response(
        json: serde_json::Value,
        id: u64,
        pending_responses: &Arc<Mutex<HashMap<u64, Sender<JsonRpcResponse>>>>,
    ) {
        let response: JsonRpcResponse = match serde_json::from_value(json) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Reader thread: failed to parse response: {}", e);
                return;
            }
        };

        // Remove sender from pending map and send response
        let sender = {
            let mut pending = pending_responses.lock().unwrap();
            pending.remove(&id)
        };

        if let Some(tx) = sender {
            if tx.send(response).is_err() {
                tracing::warn!("Reader thread: failed to send response for id={}", id);
            }
        } else {
            tracing::warn!("Reader thread: received response for unknown id={}", id);
        }
    }

    /// Static version of handle_diagnostics_notification
    fn handle_diagnostics_static(
        params: &serde_json::Value,
        diagnostics: &Arc<Mutex<HashMap<String, Vec<lsp_types::Diagnostic>>>>,
    ) -> std::io::Result<()> {
        #[derive(Deserialize)]
        struct PublishDiagnosticsParams {
            uri: String,
            diagnostics: Vec<lsp_types::Diagnostic>,
        }

        let params: PublishDiagnosticsParams = serde_json::from_value(params.clone())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        tracing::debug!(
            "Received {} diagnostic(s) for {}",
            params.diagnostics.len(),
            params.uri
        );

        let mut diag_map = diagnostics.lock().unwrap();
        diag_map.insert(params.uri, params.diagnostics);

        Ok(())
    }

    /// Static version of handle_progress_notification
    fn handle_progress_static(
        params: &serde_json::Value,
        progress: &Arc<Mutex<HashMap<String, ProgressState>>>,
    ) -> std::io::Result<()> {
        #[derive(Deserialize)]
        struct ProgressParams {
            token: serde_json::Value,
            value: serde_json::Value,
        }

        let params: ProgressParams = serde_json::from_value(params.clone())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Convert token to string
        let token_str = match params.token {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            _ => return Ok(()),
        };

        // Parse progress state
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

        let mut prog = progress.lock().unwrap();
        prog.insert(token_str, state);

        Ok(())
    }

    /// Static version of write_message for reader thread
    fn write_message_static<T: Serialize>(
        writer: &mut ChildStdin,
        message: &T,
    ) -> std::io::Result<()> {
        let json = serde_json::to_string(message)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let content = format!("Content-Length: {}\r\n\r\n{}", json.len(), json);
        writer.write_all(content.as_bytes())?;
        writer.flush()?;

        tracing::debug!("Sent: {}", json);
        Ok(())
    }

    /// Generate next request ID
    pub fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a request, creating a channel for the response
    /// Returns the request ID which can be used with read_response()
    pub fn send_request(
        &mut self,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<u64> {
        let id = self.next_id();

        // Create channel for response
        let (tx, rx) = channel();

        // Store sender and receiver in respective maps BEFORE sending request
        // This ensures we don't miss the response if it comes back very fast
        {
            let mut pending_senders = self.pending_responses.lock().unwrap();
            pending_senders.insert(id, tx);
        }
        {
            let mut pending_receivers = self.pending_receivers.lock().unwrap();
            pending_receivers.insert(id, rx);
        }

        // Now send the request
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

    /// Read a response for a specific request ID from the background reader thread
    /// This method blocks until the response arrives or times out (10 seconds)
    pub fn read_response(&mut self, id: u64) -> Result<JsonRpcResponse> {
        // Remove receiver from pending map
        let receiver = {
            let mut pending = self.pending_receivers.lock().unwrap();
            pending.remove(&id).ok_or_else(|| {
                CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("No pending request with id={}", id),
                ))
            })?
        };

        // Wait for response with 10 second timeout
        let response = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| {
                CopierError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("Timeout waiting for response id={}: {}", id, e),
                ))
            })?;

        if let Some(error) = &response.error {
            tracing::error!("JSON-RPC error for id={}: {:?}", id, error);
        }

        Ok(response)
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

        let mut stdin = self.stdin.lock().unwrap();
        stdin
            .write_all(content.as_bytes())
            .map_err(CopierError::Io)?;
        stdin.flush().map_err(CopierError::Io)?;

        tracing::debug!("Sent: {}", json);

        Ok(())
    }

    /// Get the current count of files with diagnostics (without consuming them)
    pub fn diagnostics_count(&self) -> usize {
        self.diagnostics.lock().unwrap().len()
    }

    /// Take all collected diagnostics and clear the internal storage
    pub fn take_diagnostics(&self) -> HashMap<String, Vec<lsp_types::Diagnostic>> {
        let mut diagnostics = self.diagnostics.lock().unwrap();
        std::mem::take(&mut *diagnostics)
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

impl Drop for JsonRpcTransport {
    fn drop(&mut self) {
        // The reader thread should exit when stdout reaches EOF
        // (which happens when the child process terminates)
        if let Some(handle) = self.reader_thread.take() {
            tracing::debug!("Waiting for reader thread to exit (timeout: 500ms)");

            // Give the thread 500ms to exit gracefully
            // Since we kill the child process explicitly, this should be plenty
            let timeout = std::time::Duration::from_millis(500);
            let start = std::time::Instant::now();

            // Poll the thread with a short sleep
            while start.elapsed() < timeout {
                if handle.is_finished() {
                    if let Err(e) = handle.join() {
                        tracing::warn!("Reader thread panicked: {:?}", e);
                    } else {
                        tracing::debug!("Reader thread exited cleanly");
                    }
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            tracing::warn!("Reader thread did not exit within timeout, detaching");
            // Just drop the handle - thread will be detached and continue until process exits
        }
    }
}
