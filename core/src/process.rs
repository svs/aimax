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

/// Simple process (no PTY) - just tracks PID for killing
pub struct SimpleProcess {
    pub name: String,
    pub pid: u32,
    pub command: String,
}

/// Registry managing all processes
pub struct ProcessRegistry {
    processes: HashMap<String, Process>,
    simple_processes: HashMap<String, SimpleProcess>,
    next_id: usize,
    /// Channel to send messages to the main thread
    message_tx: Sender<ProcessMessage>,
}

impl ProcessRegistry {
    pub fn new(message_tx: Sender<ProcessMessage>) -> Self {
        ProcessRegistry {
            processes: HashMap::new(),
            simple_processes: HashMap::new(),
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

    /// Spawn a simple process (no PTY, just piped stdout)
    /// Better for non-interactive commands like tail -f
    pub fn spawn_simple(
        &mut self,
        name: &str,
        command: &str,
        args: &[String],
    ) -> Result<String, String> {
        use std::process::{Command, Stdio};
        use std::io::BufRead;

        let mut child = Command::new(command)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn: {}", e))?;

        let stdout = child.stdout.take()
            .ok_or("Failed to get stdout")?;

        let name_clone = name.to_string();
        let tx = self.message_tx.clone();

        let pid = child.id();
        let cmd = format!("{} {}", command, args.join(" "));

        // Store process info
        self.simple_processes.insert(name.to_string(), SimpleProcess {
            name: name.to_string(),
            pid,
            command: cmd,
        });

        let name_for_cleanup = name.to_string();
        let tx_cleanup = self.message_tx.clone();

        // Reader thread - keeps child alive
        thread::spawn(move || {
            let mut child = child; // Keep child alive in this thread
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        let _ = tx.send(ProcessMessage::Output {
                            name: name_clone.clone(),
                            text: format!("{}\n", text),
                        });
                    }
                    Err(_) => break,
                }
            }
            // Wait for child and get exit code
            let exit_code = child.wait().ok().and_then(|s| s.code());
            let _ = tx_cleanup.send(ProcessMessage::Exited {
                name: name_for_cleanup,
                exit_code,
            });
        });

        Ok(name.to_string())
    }

    /// Kill a process by name
    pub fn kill(&mut self, name: &str) -> Result<(), String> {
        // Try PTY process first
        if let Some(proc) = self.processes.get_mut(name) {
            *proc.running.lock().unwrap() = false;
            self.processes.remove(name);
            return Ok(());
        }

        // Try simple process
        if let Some(proc) = self.simple_processes.remove(name) {
            // Use kill command to send SIGTERM
            let _ = std::process::Command::new("kill")
                .arg(proc.pid.to_string())
                .output();
            return Ok(());
        }

        Err(format!("Process '{}' not found", name))
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
        self.processes.get(name).map(|p| p.is_running()).unwrap_or(false)
            || self.simple_processes.contains_key(name)
    }

    /// Get list of all process names
    pub fn list(&self) -> Vec<String> {
        self.processes.keys()
            .chain(self.simple_processes.keys())
            .cloned()
            .collect()
    }

    /// Get process info (name, command, pid) for a simple process
    pub fn get_info(&self, name: &str) -> Option<(String, u32)> {
        self.simple_processes.get(name)
            .map(|p| (p.command.clone(), p.pid))
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
