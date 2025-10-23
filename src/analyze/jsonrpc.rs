use crate::error::{CopierError, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// JSON-RPC 2.0 transport over stdio
pub struct JsonRpcTransport {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    request_id: AtomicU64,
}

impl JsonRpcTransport {
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin,
            stdout: BufReader::new(stdout),
            request_id: AtomicU64::new(1),
        }
    }

    /// Generate next request ID
    pub fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a request and return the request ID
    pub fn send_request(&mut self, method: impl Into<String>, params: serde_json::Value) -> Result<u64> {
        let id = self.next_id();
        let request = JsonRpcRequest::new(id, method, params);
        self.write_message(&request)?;
        Ok(id)
    }

    /// Send a notification (no response expected)
    pub fn send_notification(&mut self, method: impl Into<String>, params: serde_json::Value) -> Result<()> {
        let notification = JsonRpcNotification::new(method, params);
        self.write_message(&notification)?;
        Ok(())
    }

    /// Read a response from the LSP server
    pub fn read_response(&mut self) -> Result<JsonRpcResponse> {
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
        self.stdout.read_exact(&mut content).map_err(CopierError::Io)?;

        let response: JsonRpcResponse = serde_json::from_slice(&content).map_err(|e| {
            CopierError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse JSON-RPC response: {}", e),
            ))
        })?;

        if let Some(error) = &response.error {
            tracing::error!("JSON-RPC error: {:?}", error);
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

        self.stdin.write_all(content.as_bytes()).map_err(CopierError::Io)?;
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
                headers.insert(
                    key.trim().to_lowercase(),
                    value.trim().to_string(),
                );
            }
        }

        Ok(headers)
    }
}
