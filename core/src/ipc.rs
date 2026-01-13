//! JSON-RPC IPC Server for external control
//!
//! Protocol: JSON-RPC 2.0 over Unix Domain Socket
//! Socket: ~/.aimax/sock (mode 600)
//!
//! Methods:
//! - eval: {"method": "eval", "params": {"code": "(buffer-text)"}, "id": 1}
//! - get_state: {"method": "get_state", "id": 2}
//! - subscribe: {"method": "subscribe", "params": {"events": ["buffer_changed"]}, "id": 3}
//!
//! Events (notifications to subscribers):
//! - {"method": "event", "params": {"type": "buffer_changed", "buffer": "*chat*"}}

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// Get the socket path (~/.aimax/sock)
pub fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".aimax").join("sock")
}

/// JSON-RPC Request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub id: Option<serde_json::Value>,
}

/// JSON-RPC Response (for sending)
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

/// JSON-RPC Response (for parsing on client side)
#[derive(Debug, Deserialize)]
struct JsonRpcResponseOwned {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
    #[allow(dead_code)]
    pub id: serde_json::Value,
}

/// JSON-RPC Error
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

/// JSON-RPC Event (notification)
#[derive(Debug, Serialize, Clone)]
pub struct JsonRpcEvent {
    pub jsonrpc: &'static str,
    pub method: &'static str,
    pub params: EventParams,
}

#[derive(Debug, Serialize, Clone)]
pub struct EventParams {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// IPC request types sent to editor
pub enum IpcRequest {
    /// Evaluate Scheme code and return result
    Eval {
        code: String,
        reply: Sender<Result<String, String>>,
    },
    /// Get full editor state as JSON
    GetState {
        reply: Sender<serde_json::Value>,
    },
}

/// Event types that can be subscribed to
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    BufferChanged,
    BufferCreated,
    BufferKilled,
    ChatMessage,
    ProcessOutput,
    ProcessExit,
}

/// Shared state for event subscribers
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<Arc<Mutex<UnixStream>>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        EventBus {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Broadcast an event to all subscribers
    pub fn broadcast(&self, event: JsonRpcEvent) {
        let json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(_) => return,
        };

        let mut subs = self.subscribers.lock().unwrap();
        subs.retain(|stream| {
            if let Ok(mut s) = stream.lock() {
                writeln!(s, "{}", json).is_ok()
            } else {
                false
            }
        });
    }

    /// Add a subscriber
    fn add_subscriber(&self, stream: Arc<Mutex<UnixStream>>) {
        self.subscribers.lock().unwrap().push(stream);
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

pub fn start_server(tx: Sender<IpcRequest>, event_bus: Arc<EventBus>) {
    let sock_path = socket_path();
    let aimax_dir = sock_path.parent().unwrap();

    // Create ~/.aimax with mode 700 (owner only)
    if !aimax_dir.exists() {
        if let Err(e) = fs::create_dir_all(aimax_dir) {
            eprintln!("Failed to create {}: {}", aimax_dir.display(), e);
            return;
        }
    }
    if let Err(e) = fs::set_permissions(aimax_dir, fs::Permissions::from_mode(0o700)) {
        eprintln!("Failed to set permissions on {}: {}", aimax_dir.display(), e);
    }

    // Clean up old socket
    let _ = fs::remove_file(&sock_path);

    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind to socket {}: {}", sock_path.display(), e);
            return;
        }
    };

    if let Err(e) = fs::set_permissions(&sock_path, fs::Permissions::from_mode(0o600)) {
        eprintln!("Failed to set permissions on socket: {}", e);
    }

    eprintln!("IPC listening on {}", sock_path.display());

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let tx = tx.clone();
                    let event_bus = event_bus.clone();
                    thread::spawn(move || {
                        handle_client(stream, tx, event_bus);
                    });
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                    break;
                }
            }
        }
    });
}

fn handle_client(stream: UnixStream, tx: Sender<IpcRequest>, event_bus: Arc<EventBus>) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };

    let writer = Arc::new(Mutex::new(stream));
    let mut reader = BufReader::new(reader_stream);

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Parse JSON-RPC request
                let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                    Ok(r) => r,
                    Err(e) => {
                        send_error(&writer, serde_json::Value::Null, -32700, &format!("Parse error: {}", e));
                        continue;
                    }
                };

                let id = request.id.clone().unwrap_or(serde_json::Value::Null);

                match request.method.as_str() {
                    "eval" => {
                        let code = request.params.get("code")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        if code.is_empty() {
                            send_error(&writer, id, -32602, "Missing 'code' parameter");
                            continue;
                        }

                        let (reply_tx, reply_rx) = mpsc::channel();
                        let _ = tx.send(IpcRequest::Eval {
                            code: code.to_string(),
                            reply: reply_tx,
                        });

                        match reply_rx.recv_timeout(std::time::Duration::from_secs(30)) {
                            Ok(Ok(result)) => {
                                send_result(&writer, id, serde_json::Value::String(result));
                            }
                            Ok(Err(e)) => {
                                send_error(&writer, id, -32000, &e);
                            }
                            Err(_) => {
                                send_error(&writer, id, -32000, "Timeout waiting for response");
                            }
                        }
                    }
                    "get_state" => {
                        let (reply_tx, reply_rx) = mpsc::channel();
                        let _ = tx.send(IpcRequest::GetState { reply: reply_tx });

                        match reply_rx.recv_timeout(std::time::Duration::from_secs(5)) {
                            Ok(state) => {
                                send_result(&writer, id, state);
                            }
                            Err(_) => {
                                send_error(&writer, id, -32000, "Timeout getting state");
                            }
                        }
                    }
                    "subscribe" => {
                        // Add this connection to subscribers
                        event_bus.add_subscriber(writer.clone());
                        send_result(&writer, id, serde_json::json!({"subscribed": true}));
                        // Keep connection open for events - don't break
                    }
                    _ => {
                        send_error(&writer, id, -32601, &format!("Method not found: {}", request.method));
                    }
                }
            }
            Err(_) => break,
        }
    }
}

fn send_result(writer: &Arc<Mutex<UnixStream>>, id: serde_json::Value, result: serde_json::Value) {
    let response = JsonRpcResponse {
        jsonrpc: "2.0",
        result: Some(result),
        error: None,
        id,
    };
    if let Ok(json) = serde_json::to_string(&response) {
        if let Ok(mut w) = writer.lock() {
            let _ = writeln!(w, "{}", json);
        }
    }
}

fn send_error(writer: &Arc<Mutex<UnixStream>>, id: serde_json::Value, code: i32, message: &str) {
    let response = JsonRpcResponse {
        jsonrpc: "2.0",
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
        }),
        id,
    };
    if let Ok(json) = serde_json::to_string(&response) {
        if let Ok(mut w) = writer.lock() {
            let _ = writeln!(w, "{}", json);
        }
    }
}

/// Client: connect to running aimax and call a method
pub fn call_method(method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let sock_path = socket_path();

    let mut stream = UnixStream::connect(&sock_path)
        .map_err(|e| format!("Cannot connect to {}: {}", sock_path.display(), e))?;

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    });

    writeln!(stream, "{}", request)
        .map_err(|e| format!("Failed to send: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response)
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let resp: JsonRpcResponseOwned = serde_json::from_str(&response)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if let Some(err) = resp.error {
        Err(format!("Error {}: {}", err.code, err.message))
    } else {
        Ok(resp.result.unwrap_or(serde_json::Value::Null))
    }
}

/// Client: evaluate Scheme expression (convenience wrapper)
pub fn eval_remote(expr: &str) -> Result<String, String> {
    let result = call_method("eval", serde_json::json!({"code": expr}))?;
    Ok(result.as_str().unwrap_or(&result.to_string()).to_string())
}

/// Client: get editor state
pub fn get_state() -> Result<serde_json::Value, String> {
    call_method("get_state", serde_json::json!({}))
}
