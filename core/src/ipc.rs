//! IPC Server for external control
//!
//! Listens on a Unix Domain Socket and pushes commands to the Editor's queue.
//! Socket is placed in ~/.aimax/ with restricted permissions for security.
//!
//! Protocol:
//! - Fire-and-forget: `(expr)` - execute, no response
//! - Sync request: `?(expr)` - execute and return result

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;

/// Get the socket path (~/.aimax/sock)
pub fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".aimax").join("sock")
}

/// IPC request types
pub enum IpcRequest {
    /// Fire-and-forget command (no response expected)
    Fire(String),
    /// Sync command with reply channel
    Sync {
        code: String,
        reply: Sender<String>,
    },
}

pub fn start_server(tx: Sender<IpcRequest>) {
    let sock_path = socket_path();
    let aimax_dir = sock_path.parent().unwrap();

    // Create ~/.aimax with mode 700 (owner only)
    if !aimax_dir.exists() {
        if let Err(e) = fs::create_dir_all(aimax_dir) {
            eprintln!("Failed to create {}: {}", aimax_dir.display(), e);
            return;
        }
    }
    // Set directory permissions to 700
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

    // Set socket permissions to 600 (owner only)
    if let Err(e) = fs::set_permissions(&sock_path, fs::Permissions::from_mode(0o600)) {
        eprintln!("Failed to set permissions on socket: {}", e);
    }

    eprintln!("IPC listening on {}", sock_path.display());

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let tx = tx.clone();
                    thread::spawn(move || {
                        handle_client(stream, tx);
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

fn handle_client(mut stream: UnixStream, tx: Sender<IpcRequest>) {
    // Clone for reading, keep original for writing
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(reader_stream);
    let mut buffer = String::new();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                buffer.push_str(&line);

                let trimmed = buffer.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Check for sync request (starts with ?)
                let is_sync = trimmed.starts_with('?');
                let code = if is_sync {
                    &trimmed[1..] // Strip the ?
                } else {
                    trimmed
                };

                // Check if we have a complete S-expression or simple command
                let is_complete = if !code.starts_with('(') {
                    true // Simple command
                } else {
                    is_balanced(code)
                };

                if is_complete {
                    if is_sync {
                        // Create reply channel and wait for response
                        let (reply_tx, reply_rx) = mpsc::channel();
                        let _ = tx.send(IpcRequest::Sync {
                            code: code.to_string(),
                            reply: reply_tx,
                        });

                        // Wait for response and write back (with timeout)
                        match reply_rx.recv_timeout(std::time::Duration::from_secs(30)) {
                            Ok(result) => {
                                let _ = writeln!(stream, "{}", result);
                            }
                            Err(_) => {
                                let _ = writeln!(stream, "error: timeout waiting for response");
                            }
                        }
                    } else {
                        // Fire and forget
                        let _ = tx.send(IpcRequest::Fire(code.to_string()));
                    }
                    buffer.clear();
                }
                // Otherwise keep accumulating
            }
            Err(_) => break,
        }
    }
}

/// Client: connect to running aimax and evaluate expression synchronously
pub fn eval_remote(expr: &str) -> Result<String, String> {
    let sock_path = socket_path();

    let mut stream = UnixStream::connect(&sock_path)
        .map_err(|e| format!("Cannot connect to {}: {}", sock_path.display(), e))?;

    // Send sync request (with ? prefix)
    writeln!(stream, "?{}", expr)
        .map_err(|e| format!("Failed to send: {}", e))?;

    // Read response
    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response)
        .map_err(|e| format!("Failed to read response: {}", e))?;

    Ok(response.trim().to_string())
}

/// Check if parentheses are balanced in a string
fn is_balanced(s: &str) -> bool {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;

    for c in s.chars() {
        if escape {
            escape = false;
            continue;
        }

        match c {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }

    depth == 0 && !in_string
}
