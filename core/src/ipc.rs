//! IPC Server for external control
//!
//! Listens on a Unix Domain Socket and pushes commands to the Editor's queue.
//! Socket is placed in ~/.aimax/ with restricted permissions for security.

use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

/// Get the socket path (~/.aimax/sock)
pub fn socket_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".aimax").join("sock")
}

pub fn start_server(tx: Sender<String>) {
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

fn handle_client(stream: UnixStream, tx: Sender<String>) {
    let mut reader = BufReader::new(stream);
    let mut buffer = String::new();

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                buffer.push_str(&line);

                // Check if we have a complete S-expression (balanced parens)
                // or a simple command (no parens)
                let trimmed = buffer.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if !trimmed.starts_with('(') {
                    // Simple command, send it
                    let _ = tx.send(trimmed.to_string());
                    buffer.clear();
                } else if is_balanced(trimmed) {
                    // Complete S-expression
                    let _ = tx.send(trimmed.to_string());
                    buffer.clear();
                }
                // Otherwise keep accumulating
            }
            Err(_) => break,
        }
    }
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
