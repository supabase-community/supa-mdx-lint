#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::{json, Value};

/// A test client that communicates with the LSP server via JSON-RPC
pub struct TestClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    request_id: AtomicI64,
}

impl TestClient {
    /// Spawn a new LSP server process and connect to it
    pub fn new() -> Self {
        // Find the LSP binary - try debug first, then release
        let binary_path = std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("target/debug/supa-mdx-lint-lsp");

        let binary_path = if binary_path.exists() {
            binary_path
        } else {
            std::env::current_dir()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("target/release/supa-mdx-lint-lsp")
        };

        assert!(
            binary_path.exists(),
            "LSP binary not found at {:?}. Run `cargo build -p supa-mdx-lint-lsp` first.",
            binary_path
        );

        let mut child = Command::new(binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to spawn LSP server");

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = child.stdout.take().expect("Failed to get stdout");

        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            request_id: AtomicI64::new(1),
        }
    }

    /// Send a JSON-RPC request and return the response
    pub fn request(&mut self, method: &str, params: Value) -> JsonRpcResponse {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed);
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        self.send_message(&request);
        self.wait_for_response(id)
    }

    /// Send a JSON-RPC notification (no response expected)
    pub fn notify(&mut self, method: &str, params: Value) {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        self.send_message(&notification);
    }

    /// Initialize the LSP connection
    pub fn initialize(&mut self, root_uri: &str, init_options: Option<Value>) -> Value {
        let params = json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {},
            "initializationOptions": init_options.unwrap_or(json!({}))
        });

        let response = self.request("initialize", params);
        assert!(
            response.error.is_none(),
            "Initialize failed: {:?}",
            response.error
        );

        // Send initialized notification
        self.notify("initialized", json!({}));

        response.result.unwrap_or(json!(null))
    }

    /// Open a text document
    pub fn open_document(&mut self, uri: &str, language_id: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text
                }
            }),
        );
    }

    /// Change a text document
    pub fn change_document(&mut self, uri: &str, version: i32, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {
                    "uri": uri,
                    "version": version
                },
                "contentChanges": [{
                    "text": text
                }]
            }),
        );
    }

    /// Save a text document
    pub fn save_document(&mut self, uri: &str, text: Option<&str>) {
        let mut params = json!({
            "textDocument": {
                "uri": uri
            }
        });

        if let Some(t) = text {
            params["text"] = json!(t);
        }

        self.notify("textDocument/didSave", params);
    }

    /// Close a text document
    pub fn close_document(&mut self, uri: &str) {
        self.notify(
            "textDocument/didClose",
            json!({
                "textDocument": {
                    "uri": uri
                }
            }),
        );
    }

    /// Request hover information
    pub fn hover(&mut self, uri: &str, line: u32, character: u32) -> Option<Value> {
        let response = self.request(
            "textDocument/hover",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": line,
                    "character": character
                }
            }),
        );

        response.result
    }

    /// Wait for and return the next notification with a specific method
    pub fn wait_for_notification(&mut self, expected_method: &str) -> Value {
        loop {
            let message = self.read_message();
            if let Some(method) = message.get("method").and_then(|m| m.as_str()) {
                if method == expected_method {
                    return message.get("params").cloned().unwrap_or(json!(null));
                }
            }
        }
    }

    /// Wait for diagnostics to be published for a specific URI
    pub fn wait_for_diagnostics(&mut self, uri: &str) -> Vec<Value> {
        loop {
            let params = self.wait_for_notification("textDocument/publishDiagnostics");
            if let Some(notif_uri) = params.get("uri").and_then(|u| u.as_str()) {
                if notif_uri == uri {
                    return params
                        .get("diagnostics")
                        .and_then(|d| d.as_array())
                        .cloned()
                        .unwrap_or_default();
                }
            }
        }
    }

    /// Send shutdown request and exit notification
    pub fn shutdown(&mut self) {
        let _ = self.request("shutdown", json!(null));
        self.notify("exit", json!(null));
    }

    fn send_message(&mut self, message: &Value) {
        let content = serde_json::to_string(message).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        self.stdin.write_all(header.as_bytes()).unwrap();
        self.stdin.write_all(content.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn read_message(&mut self) -> Value {
        // Read headers
        let mut content_length = 0;
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).unwrap();

            if line == "\r\n" {
                break;
            }

            if line.starts_with("Content-Length:") {
                content_length = line
                    .trim_start_matches("Content-Length:")
                    .trim()
                    .parse()
                    .unwrap();
            }
        }

        // Read content
        let mut content = vec![0u8; content_length];
        std::io::Read::read_exact(&mut self.stdout, &mut content).unwrap();

        serde_json::from_slice(&content).unwrap()
    }

    fn wait_for_response(&mut self, expected_id: i64) -> JsonRpcResponse {
        loop {
            let message = self.read_message();
            if let Some(id) = message.get("id").and_then(|i| i.as_i64()) {
                if id == expected_id {
                    return JsonRpcResponse {
                        result: message.get("result").cloned(),
                        error: message.get("error").cloned(),
                    };
                }
            }
        }
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug)]
pub struct JsonRpcResponse {
    pub result: Option<Value>,
    pub error: Option<Value>,
}
