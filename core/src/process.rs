//! Process System - PTY-based process management
//!
//! Provides:
//! - Process spawning with PTY (pseudo-terminal)
//! - Background reader threads for non-blocking output capture
//! - Process registry for managing multiple processes
//!
//! This is the foundation for:
//! - Running shells (M-x shell)
//! - Build commands (cargo build)
//! - AI agent integration (claude, opencode)

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;

use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtyPair};

/// Maximum lines to keep in a process buffer (ring buffer behavior)
pub const MAX_PROCESS_BUFFER_LINES: usize = 5000;

/// Message from reader thread to main thread
#[derive(Debug, Clone)]
pub enum ProcessMessage {
    /// Output from process (already ANSI-stripped)
    Output { name: String, text: String },
    /// Process exited
    Exited { name: String, exit_code: Option<i32> },
}

/// A running process with PTY
pub struct Process {
    pub id: usize,
    pub name: String,
    /// Writer to send input to the process
    writer: Box<dyn Write + Send>,
    /// Handle to the reader thread (for cleanup)
    _reader_handle: thread::JoinHandle<()>,
    /// Flag to signal shutdown
    running: Arc<Mutex<bool>>,
}

impl Process {
    /// Send text to the process (write to PTY)
    pub fn send(&mut self, text: &str) -> std::io::Result<()> {
        self.writer.write_all(text.as_bytes())?;
        self.writer.flush()
    }

    /// Check if process is still running
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }
}

/// Registry managing all processes
pub struct ProcessRegistry {
    processes: HashMap<String, Process>,
    next_id: usize,
    /// Channel to send messages to the main thread
    message_tx: Sender<ProcessMessage>,
}

impl ProcessRegistry {
    pub fn new(message_tx: Sender<ProcessMessage>) -> Self {
        ProcessRegistry {
            processes: HashMap::new(),
            next_id: 0,
            message_tx,
        }
    }

    /// Spawn a new process
    ///
    /// # Arguments
    /// * `name` - Process name (used for buffer name like "*shell*")
    /// * `command` - Program to run (e.g., "/bin/bash", "claude")
    /// * `args` - Command arguments
    ///
    /// Returns the process name on success
    pub fn spawn(
        &mut self,
        name: &str,
        command: &str,
        args: &[String],
    ) -> Result<String, String> {
        // Create PTY
        let pty_system = native_pty_system();

        let pair: PtyPair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        // Build command
        let mut cmd = CommandBuilder::new(command);
        for arg in args {
            cmd.arg(arg);
        }

        // Spawn the child process
        let _child = pair.slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn process: {}", e))?;

        // Get reader and writer from master
        let mut reader = pair.master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;

        let writer = pair.master
            .take_writer()
            .map_err(|e| format!("Failed to take PTY writer: {}", e))?;

        // Running flag for coordination
        let running = Arc::new(Mutex::new(true));
        let running_clone = running.clone();
        let name_clone = name.to_string();
        let tx = self.message_tx.clone();

        // Spawn reader thread
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; 4096];

            loop {
                // Check if we should stop
                if !*running_clone.lock().unwrap() {
                    break;
                }

                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF - process exited
                        *running_clone.lock().unwrap() = false;
                        let _ = tx.send(ProcessMessage::Exited {
                            name: name_clone.clone(),
                            exit_code: None,
                        });
                        break;
                    }
                    Ok(n) => {
                        // Convert to string, stripping ANSI codes
                        let raw = String::from_utf8_lossy(&buf[..n]);
                        let stripped = strip_ansi(&raw);

                        if !stripped.is_empty() {
                            let _ = tx.send(ProcessMessage::Output {
                                name: name_clone.clone(),
                                text: stripped,
                            });
                        }
                    }
                    Err(e) => {
                        // Read error - process likely died
                        eprintln!("PTY read error for {}: {}", name_clone, e);
                        *running_clone.lock().unwrap() = false;
                        let _ = tx.send(ProcessMessage::Exited {
                            name: name_clone.clone(),
                            exit_code: None,
                        });
                        break;
                    }
                }
            }
        });

        let id = self.next_id;
        self.next_id += 1;

        let process = Process {
            id,
            name: name.to_string(),
            writer,
            _reader_handle: reader_handle,
            running,
        };

        self.processes.insert(name.to_string(), process);
        Ok(name.to_string())
    }

    /// Send text to a process by name
    pub fn send(&mut self, name: &str, text: &str) -> Result<(), String> {
        let proc = self.processes
            .get_mut(name)
            .ok_or_else(|| format!("Process '{}' not found", name))?;

        proc.send(text)
            .map_err(|e| format!("Failed to send to process: {}", e))
    }

    /// Check if a process is running
    pub fn is_running(&self, name: &str) -> bool {
        self.processes
            .get(name)
            .map(|p| p.is_running())
            .unwrap_or(false)
    }

    /// Get list of all process names
    pub fn list(&self) -> Vec<String> {
        self.processes.keys().cloned().collect()
    }

    /// Remove a dead process from the registry
    pub fn remove(&mut self, name: &str) -> Option<Process> {
        self.processes.remove(name)
    }
}

/// Strip ANSI escape codes from text
fn strip_ansi(text: &str) -> String {
    // strip_ansi_escapes::strip returns Vec<u8> directly
    let bytes = strip_ansi_escapes::strip(text);
    String::from_utf8_lossy(&bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[31mred text\x1b[0m";
        let output = strip_ansi(input);
        assert_eq!(output, "red text");
    }

    #[test]
    fn test_spawn_echo() {
        let (tx, rx) = mpsc::channel();
        let mut registry = ProcessRegistry::new(tx);

        // Spawn echo - should exit immediately after printing
        let result = registry.spawn("*test*", "echo", &["hello".to_string()]);
        assert!(result.is_ok());

        // Wait for output (with timeout)
        let msg = rx.recv_timeout(std::time::Duration::from_secs(2));

        if let Ok(ProcessMessage::Output { name, text }) = msg {
            assert_eq!(name, "*test*");
            assert!(text.contains("hello"), "Expected 'hello' in output: {}", text);
        }
    }
}
