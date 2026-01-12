//! Editor - Testable editor core
//!
//! This module provides a headless editor that can be tested without a terminal.
//! The TUI is just a rendering layer on top of this.

use std::path::PathBuf;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::mpsc::{self, Receiver, Sender};
use crate::{Buffer, Interpreter, command::CommandResult, scheme::Action};
use crate::process::{ProcessRegistry, ProcessMessage, MAX_PROCESS_BUFFER_LINES};
use crate::llm::{ChatConfig, Message, StreamEvent, chat_stream};

/// Log errors to /tmp/aimax.log
fn log_error(context: &str, error: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/aimax.log")
    {
        let _ = writeln!(file, "ERROR [{}]: {}", context, error);
    }
}

/// Editor state - testable without TUI
pub struct Editor {
    pub buffers: Vec<Buffer>,
    pub current: usize,
    pub scheme: Interpreter,
    pub cwd: PathBuf,

    // IPC command queue
    pub ipc_tx: Sender<String>,
    pub ipc_rx: Receiver<String>,

    // Minibuffer state (mirrors Scheme state for rendering)
    pub minibuffer_active: bool,
    pub minibuffer_prompt: String,
    pub minibuffer_input: String,
    pub minibuffer_matches: Vec<String>,
    pub minibuffer_selected: usize,
    pub minibuffer_mode: MinibufferMode,

    pub status_message: Option<String>,
    pub should_quit: bool,

    // Face changes for TUI to apply to SyntaxHighlighter
    pub face_actions: Vec<(String, String, String)>,  // (face, key, value)

    // Process system
    pub processes: ProcessRegistry,
    process_rx: Receiver<ProcessMessage>,

    // Chat system
    pub chat_config: ChatConfig,
    pub chat_messages: Vec<Message>,
    pub chat_streaming: bool,
    chat_tx: Sender<StreamEvent>,
    chat_rx: Receiver<StreamEvent>,
    /// Accumulates the current assistant response for the completion callback
    chat_response_buffer: String,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum MinibufferMode {
    #[default]
    None,
    FindFile,
    SwitchBuffer,
    /// Generic Scheme-controlled prompt (ask-ai, M-x, etc.)
    Generic,
}

impl Editor {
    pub fn new(cwd: PathBuf) -> Self {
        let scheme = Interpreter::with_core();
        let (ipc_tx, ipc_rx) = mpsc::channel();
        let (process_tx, process_rx) = mpsc::channel();
        let (chat_tx, chat_rx) = mpsc::channel();

        let mut editor = Editor {
            buffers: vec![Buffer::new("*scratch*")],
            current: 0,
            scheme,
            cwd,
            ipc_tx,
            ipc_rx,
            minibuffer_active: false,
            minibuffer_prompt: String::new(),
            minibuffer_input: String::new(),
            minibuffer_matches: vec![],
            minibuffer_selected: 0,
            minibuffer_mode: MinibufferMode::None,
            status_message: None,
            should_quit: false,
            face_actions: vec![],
            processes: ProcessRegistry::new(process_tx),
            process_rx,
            // Chat config comes from Scheme when chat is invoked
            chat_config: ChatConfig::default(),
            chat_messages: vec![],
            chat_streaming: false,
            chat_tx,
            chat_rx,
            chat_response_buffer: String::new(),
        };

        // Load user init file
        editor.load_user_init();

        editor
    }

    /// Load user's init.scm from ~/.aimax/init.scm
    fn load_user_init(&mut self) {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let init_path = PathBuf::from(&home).join(".aimax").join("init.scm");

        if init_path.exists() {
            match std::fs::read_to_string(&init_path) {
                Ok(code) => {
                    if let Err(e) = self.run_scheme("init", &code) {
                        log_error("init.scm", &e);
                        self.status_message = Some(format!("Error in init.scm: {}", e));
                    }
                }
                Err(e) => {
                    log_error("init.scm", &format!("Failed to read: {}", e));
                }
            }
        }
    }

    /// Current buffer (mutable)
    pub fn buffer(&mut self) -> &mut Buffer {
        &mut self.buffers[self.current]
    }

    /// Current buffer (immutable)
    pub fn buffer_ref(&self) -> &Buffer {
        &self.buffers[self.current]
    }

    /// Buffer names for completion
    pub fn buffer_names(&self) -> Vec<String> {
        self.buffers.iter().map(|b| b.name.clone()).collect()
    }

    /// Sync buffer state to Scheme globals (lightweight)
    fn sync_buffer_state(&mut self) {
        if let Ok(mut state) = self.scheme.shared_state.write() {
            // Sync buffer file paths (needed for desktop-save)
            state.buffer_file_paths = self.buffers.iter()
                .filter_map(|b| b.file_path.as_ref())
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            // Sync all buffer names
            state.buffer_names = self.buffers.iter()
                .map(|b| b.name.clone())
                .collect();
            // Sync modified status and locals for each buffer
            state.buffer_modified_map.clear();
            state.buffer_locals_map.clear();
            for buf in &self.buffers {
                state.buffer_modified_map.insert(buf.name.clone(), buf.is_modified());
                // Stringify locals for this buffer
                let mut locals = std::collections::HashMap::new();
                for (k, v) in &buf.locals {
                    let val = match v {
                        crate::buffer::LocalVar::String(s) => s.clone(),
                        crate::buffer::LocalVar::Int(n) => n.to_string(),
                        crate::buffer::LocalVar::Bool(b) => b.to_string(),
                    };
                    locals.insert(k.clone(), val);
                }
                state.buffer_locals_map.insert(buf.name.clone(), locals);
            }
            // Sync running process names
            state.process_names = self.processes.list();
            // Sync current buffer info
            let buf = &self.buffers[self.current];
            state.buffer_name = buf.name.clone();
            state.buffer_major_mode = buf.major_mode.clone();
            state.buffer_modified = buf.is_modified();
            // Sync buffer locals (stringify values)
            state.buffer_locals.clear();
            for (k, v) in &buf.locals {
                let val = match v {
                    crate::buffer::LocalVar::String(s) => s.clone(),
                    crate::buffer::LocalVar::Int(n) => n.to_string(),
                    crate::buffer::LocalVar::Bool(b) => b.to_string(),
                };
                state.buffer_locals.insert(k.clone(), val);
            }
        }
    }

    /// Run Scheme code, logging any errors
    pub fn run_scheme(&mut self, context: &str, code: &str) -> Result<steel::rvals::SteelVal, String> {
        // Sync buffer state before running Scheme
        self.sync_buffer_state();

        let result = match self.scheme.run(code) {
            Ok(val) => Ok(val),
            Err(e) => {
                log_error(context, &format!("code={} error={}", code, e));
                Err(e)
            }
        };

        // Process any side-effects requested by Scheme
        self.process_actions();

        result
    }

    /// Process actions requested by Scheme (Insert, Delete, etc.)
    fn process_actions(&mut self) {
        let actions = {
            if let Ok(mut queue) = self.scheme.pending_actions.lock() {
                let drained: Vec<Action> = queue.drain(..).collect();
                drained
            } else {
                return;
            }
        };

        for action in actions {
            match action {
                Action::Insert(text) => self.buffer().insert(&text),
                Action::Delete => self.buffer().delete_backward(),
                Action::CreateBuffer(name) => {
                    // Create new buffer if it doesn't exist, switch to it
                    if let Some(idx) = self.buffers.iter().position(|b| b.name == name) {
                        self.current = idx;
                    } else {
                        let buf = Buffer::new(&name);
                        self.buffers.push(buf);
                        self.current = self.buffers.len() - 1;
                    }
                }
                Action::SwitchBuffer(name) => {
                    if let Some(idx) = self.buffers.iter().position(|b| b.name == name) {
                        self.current = idx;
                        self.status_message = Some(format!("Switched to {}", name));
                    } else {
                        self.status_message = Some(format!("Buffer not found: {}", name));
                    }
                }
                Action::Open(path) => {
                    if let Ok(buf) = Buffer::from_file(&path) {
                        if let Some(idx) = self.buffers.iter().position(|b| b.file_path == buf.file_path) {
                            self.current = idx;
                        } else {
                            self.buffers.push(buf);
                            self.current = self.buffers.len() - 1;
                        }
                        self.status_message = Some(format!("Opened {}", path));
                    }
                }
                Action::Message(msg) => self.status_message = Some(msg),
                // Movement actions
                Action::SetPoint(n) => self.buffer().set_point(n),
                Action::ForwardChar => self.buffer().forward_char(),
                Action::BackwardChar => self.buffer().backward_char(),
                Action::ForwardWord => self.buffer().forward_word(),
                Action::BackwardWord => self.buffer().backward_word(),
                Action::NextLine => self.buffer().next_line(),
                Action::PreviousLine => self.buffer().previous_line(),
                Action::BeginningOfLine => self.buffer().beginning_of_line(),
                Action::EndOfLine => self.buffer().end_of_line(),
                Action::BeginningOfBuffer => self.buffer().beginning_of_buffer(),
                Action::EndOfBuffer => self.buffer().end_of_buffer(),
                Action::GotoLine(n) => self.buffer().goto_line(n),
                // Deletion
                Action::DeleteWordBackward => self.buffer().delete_word_backward(),
                Action::DeleteWordForward => self.buffer().delete_word_forward(),
                // Special commands
                Action::Quit => self.should_quit = true,
                Action::FindFileInteractive => self.find_file(),
                Action::SwitchBufferInteractive => self.switch_buffer(),
                Action::SaveBuffer => {
                    if self.buffer_ref().file_path.is_some() {
                        match self.buffer().save() {
                            Ok(()) => self.status_message = Some("Saved".to_string()),
                            Err(e) => self.status_message = Some(format!("Error: {}", e)),
                        }
                    } else {
                        self.status_message = Some("No file path".to_string());
                    }
                }
                Action::KillBuffer => {
                    if self.buffers.len() > 1 {
                        let name = self.buffer_ref().name.clone();
                        self.buffers.remove(self.current);
                        if self.current >= self.buffers.len() {
                            self.current = self.buffers.len() - 1;
                        }
                        self.status_message = Some(format!("Killed {}", name));
                    } else {
                        self.status_message = Some("Can't kill last buffer".to_string());
                    }
                }
                Action::KillBufferNamed(name) => {
                    if let Some(idx) = self.buffers.iter().position(|b| b.name == name) {
                        if self.buffers.len() > 1 {
                            self.buffers.remove(idx);
                            if self.current >= self.buffers.len() {
                                self.current = self.buffers.len() - 1;
                            } else if self.current > idx {
                                self.current -= 1;
                            }
                            self.status_message = Some(format!("Killed {}", name));
                        } else {
                            self.status_message = Some("Can't kill last buffer".to_string());
                        }
                    } else {
                        self.status_message = Some(format!("No buffer named {}", name));
                    }
                }
                Action::KeyboardQuit => {
                    self.minibuffer_cancel();
                    self.status_message = Some("Quit".to_string());
                }
                Action::Newline => self.buffer().insert_char('\n'),
                Action::SetFaceAttribute { face, key, value } => {
                    self.face_actions.push((face, key, value));
                }
                Action::StartProcess { name, command, args } => {
                    match self.processes.spawn(&name, &command, &args) {
                        Ok(_) => {
                            // Create or switch to process buffer
                            if let Some(idx) = self.buffers.iter().position(|b| b.name == name) {
                                self.current = idx;
                            } else {
                                let buf = Buffer::new(&name);
                                self.buffers.push(buf);
                                self.current = self.buffers.len() - 1;
                            }
                            self.status_message = Some(format!("Started {}", name));
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Failed to start process: {}", e));
                        }
                    }
                }
                Action::StartProcessSimple { name, command, args } => {
                    let cmd_str = format!("{} {}", command, args.join(" "));
                    match self.processes.spawn_simple(&name, &command, &args) {
                        Ok(_) => {
                            // Create or switch to process buffer
                            let idx = if let Some(idx) = self.buffers.iter().position(|b| b.name == name) {
                                idx
                            } else {
                                let buf = Buffer::new(&name);
                                self.buffers.push(buf);
                                self.buffers.len() - 1
                            };
                            self.current = idx;

                            // Set buffer locals
                            let buf = &mut self.buffers[idx];
                            buf.major_mode = "process".to_string();
                            buf.set_local("process-name", crate::buffer::LocalVar::String(name.clone()));
                            buf.set_local("process-command", crate::buffer::LocalVar::String(cmd_str));
                            if let Some((_, pid)) = self.processes.get_info(&name) {
                                buf.set_local("process-pid", crate::buffer::LocalVar::Int(pid as i64));
                            }

                            self.status_message = Some(format!("Started {}", name));
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Failed: {}", e));
                        }
                    }
                }
                Action::ProcessSendString { name, text } => {
                    if let Err(e) = self.processes.send(&name, &text) {
                        self.status_message = Some(format!("Failed to send: {}", e));
                    }
                }
                Action::KillProcess { name } => {
                    match self.processes.kill(&name) {
                        Ok(_) => self.status_message = Some(format!("Killed {}", name)),
                        Err(e) => self.status_message = Some(e),
                    }
                }
                Action::Chat { provider, api_key, model, system, messages } => {
                    self.start_chat(&provider, &api_key, &model, system.as_deref(), messages);
                }
                Action::MinibufferActivate { prompt } => {
                    // Scheme activated minibuffer in generic mode
                    self.minibuffer_mode = MinibufferMode::Generic;
                    self.minibuffer_active = true;
                    self.minibuffer_prompt = prompt;
                    self.minibuffer_input = String::new();
                    self.minibuffer_matches = vec![];
                    self.minibuffer_selected = 0;
                }
            }
        }
    }

    /// Start a chat with an LLM provider
    fn start_chat(&mut self, provider: &str, api_key: &str, model: &str, system: Option<&str>, messages: Vec<(String, String)>) {
        if api_key.is_empty() {
            self.status_message = Some("No API key set. Set *ai-api-key* in init.scm".to_string());
            return;
        }

        // Create or find *chat* buffer
        let chat_buf_idx = self.buffers.iter()
            .position(|b| b.name == "*chat*")
            .unwrap_or_else(|| {
                let buf = Buffer::new("*chat*");
                self.buffers.push(buf);
                self.buffers.len() - 1
            });

        // Append user message to buffer
        if let Some((_, content)) = messages.last() {
            self.buffers[chat_buf_idx].append(&format!("\n>>> {}\n\n", content));
        }

        // Switch to chat buffer
        self.current = chat_buf_idx;

        // Build config
        let config = ChatConfig {
            provider: crate::llm::Provider::from_str(provider),
            api_key: api_key.to_string(),
            model: model.to_string(),
            system: system.map(|s| s.to_string()),
            max_tokens: 4096,
        };

        // Build messages for API
        let api_messages: Vec<Message> = messages.into_iter()
            .map(|(role, content)| Message { role, content })
            .collect();

        // Start streaming
        self.chat_streaming = true;
        chat_stream(config, api_messages, self.chat_tx.clone());
    }

    /// Start find-file command
    pub fn find_file(&mut self) {
        self.minibuffer_mode = MinibufferMode::FindFile;
        let cwd = self.cwd.to_string_lossy();
        let escaped = cwd.replace("\\", "\\\\").replace("\"", "\\\"");
        // minibuffer.scm: (minibuffer-find-file cwd callback)
        // We pass #f for callback since Rust handles the result
        let _ = self.run_scheme("find_file", &format!(r#"(minibuffer-find-file "{}" #f)"#, escaped));
        self.sync_minibuffer_state();
    }

    /// Start switch-buffer command
    pub fn switch_buffer(&mut self) {
        self.minibuffer_mode = MinibufferMode::SwitchBuffer;
        // Set up minibuffer with buffer names as completions
        let names: Vec<String> = self.buffers.iter()
            .enumerate()
            .filter(|(i, _)| *i != self.current)  // exclude current buffer
            .map(|(_, b)| b.name.clone())
            .collect();

        self.minibuffer_active = true;
        self.minibuffer_prompt = "Switch to buffer: ".to_string();
        self.minibuffer_input = String::new();
        self.minibuffer_matches = names;
        self.minibuffer_selected = 0;
    }

    /// Insert character in minibuffer
    pub fn minibuffer_insert(&mut self, c: char) {
        if !self.minibuffer_active {
            return;
        }
        match self.minibuffer_mode {
            MinibufferMode::FindFile | MinibufferMode::Generic => {
                let _ = self.run_scheme("minibuffer_insert", &format!(r#"(minibuffer-insert #\{})"#, c));
                self.sync_minibuffer_state();
            }
            MinibufferMode::SwitchBuffer => {
                self.minibuffer_input.push(c);
                self.update_buffer_matches();
            }
            MinibufferMode::None => {}
        }
    }

    /// Update buffer matches based on input (for switch-buffer)
    fn update_buffer_matches(&mut self) {
        let input = self.minibuffer_input.to_lowercase();
        self.minibuffer_matches = self.buffers.iter()
            .enumerate()
            .filter(|(i, _)| *i != self.current)
            .filter(|(_, b)| b.name.to_lowercase().contains(&input))
            .map(|(_, b)| b.name.clone())
            .collect();
        self.minibuffer_selected = 0;
    }

    /// Delete backward in minibuffer
    pub fn minibuffer_delete_backward(&mut self) {
        if !self.minibuffer_active {
            return;
        }
        match self.minibuffer_mode {
            MinibufferMode::FindFile | MinibufferMode::Generic => {
                let _ = self.run_scheme("minibuffer_delete_backward", "(minibuffer-delete-backward)");
                self.sync_minibuffer_state();
            }
            MinibufferMode::SwitchBuffer => {
                self.minibuffer_input.pop();
                self.update_buffer_matches();
            }
            MinibufferMode::None => {}
        }
    }

    /// Tab complete
    pub fn minibuffer_complete(&mut self) {
        if !self.minibuffer_active {
            return;
        }
        let _ = self.run_scheme("minibuffer_complete", "(minibuffer-complete)");
        self.sync_minibuffer_state();
    }

    /// Next completion
    pub fn minibuffer_next(&mut self) {
        if !self.minibuffer_active {
            return;
        }
        match self.minibuffer_mode {
            MinibufferMode::FindFile | MinibufferMode::Generic => {
                let _ = self.run_scheme("minibuffer_next", "(minibuffer-next-completion)");
                self.sync_minibuffer_state();
            }
            MinibufferMode::SwitchBuffer => {
                if self.minibuffer_selected < self.minibuffer_matches.len().saturating_sub(1) {
                    self.minibuffer_selected += 1;
                }
            }
            MinibufferMode::None => {}
        }
    }

    /// Previous completion
    pub fn minibuffer_prev(&mut self) {
        if !self.minibuffer_active {
            return;
        }
        match self.minibuffer_mode {
            MinibufferMode::FindFile | MinibufferMode::Generic => {
                let _ = self.run_scheme("minibuffer_prev", "(minibuffer-prev-completion)");
                self.sync_minibuffer_state();
            }
            MinibufferMode::SwitchBuffer => {
                if self.minibuffer_selected > 0 {
                    self.minibuffer_selected -= 1;
                }
            }
            MinibufferMode::None => {}
        }
    }

    /// Cancel minibuffer
    pub fn minibuffer_cancel(&mut self) {
        self.minibuffer_mode = MinibufferMode::None;
        self.minibuffer_active = false;
        let _ = self.run_scheme("minibuffer_cancel", "(minibuffer-cancel)");
        self.sync_minibuffer_state();
    }

    /// Submit minibuffer
    pub fn minibuffer_submit(&mut self) -> Result<(), String> {
        if !self.minibuffer_active {
            return Ok(());
        }

        match self.minibuffer_mode {
            MinibufferMode::FindFile => self.submit_find_file(),
            MinibufferMode::SwitchBuffer => self.submit_switch_buffer(),
            MinibufferMode::Generic => self.submit_generic(),
            MinibufferMode::None => {
                self.minibuffer_cancel();
                Ok(())
            }
        }
    }

    /// Submit generic minibuffer - calls Scheme callback
    fn submit_generic(&mut self) -> Result<(), String> {
        // Call Scheme's minibuffer-submit which invokes the callback
        let result = self.run_scheme("minibuffer_submit", "(minibuffer-submit)");

        // Process any actions (callback might open new minibuffer)
        self.process_actions();

        // Only reset if callback didn't open a new minibuffer
        if !self.minibuffer_active {
            self.minibuffer_mode = MinibufferMode::None;
        }

        self.sync_minibuffer_state();
        result.map(|_| ())
    }

    fn submit_find_file(&mut self) -> Result<(), String> {
        // Get result from Scheme (handles selected completion)
        let input = self.scheme.run("(minibuffer-get-result)")
            .ok()
            .and_then(|v| {
                use steel::rvals::SteelVal;
                if let SteelVal::StringV(s) = v { Some(s.to_string()) } else { None }
            })
            .unwrap_or_else(|| self.minibuffer_input.clone());

        // If it's a directory, descend into it instead of opening
        if input.ends_with('/') {
            // Update Scheme state and refresh
            let _ = self.run_scheme("descend_dir", &format!(
                r#"(begin (set! *minibuffer-input* "{}") (set! *minibuffer-point* {}) (minibuffer-update-completions))"#,
                input.replace("\"", "\\\""),
                input.len()
            ));
            self.sync_minibuffer_state();
            return Ok(());
        }

        self.minibuffer_cancel();

        // Resolve the path
        let path = crate::minibuffer::path::resolve(&input, &self.cwd);

        // Open the file
        match Buffer::from_file(path.to_string_lossy().as_ref()) {
            Ok(buf) => {
                if let Some(parent) = path.parent() {
                    self.cwd = parent.to_path_buf();
                }
                // Add to buffer list or switch to existing
                if let Some(idx) = self.buffers.iter().position(|b| b.file_path == buf.file_path) {
                    self.current = idx;
                } else {
                    self.buffers.push(buf);
                    self.current = self.buffers.len() - 1;
                }
                self.status_message = Some(format!("Opened {}", path.display()));
                Ok(())
            }
            Err(e) => {
                let msg = format!("Error: {}", e);
                self.status_message = Some(msg.clone());
                Err(msg)
            }
        }
    }

    fn submit_switch_buffer(&mut self) -> Result<(), String> {
        let name = if self.minibuffer_matches.is_empty() {
            self.minibuffer_input.clone()
        } else {
            self.minibuffer_matches.get(self.minibuffer_selected)
                .cloned()
                .unwrap_or_else(|| self.minibuffer_input.clone())
        };

        self.minibuffer_cancel();

        if let Some(idx) = self.buffers.iter().position(|b| b.name == name) {
            self.current = idx;
            self.status_message = Some(format!("Switched to {}", name));
            Ok(())
        } else {
            let msg = format!("No buffer named {}", name);
            self.status_message = Some(msg.clone());
            Err(msg)
        }
    }

    /// Process IPC commands and Scheme requests
    pub fn process_ipc(&mut self) -> bool {
        let mut processed = false;
        while let Ok(code) = self.ipc_rx.try_recv() {
            processed = true;

            // If it's a simple command string, execute it
            if !code.starts_with('(') {
                let _ = self.execute_command(&code);
                continue;
            }

            // Otherwise run as scheme
            if let Err(e) = self.run_scheme("ipc", &code) {
                log_error("ipc", &format!("Scheme error: {}", e));
            }

            // Process any actions queued by Scheme
            self.process_actions();
        }
        processed
    }

    /// Process messages from running processes (PTY output)
    /// Returns true if any messages were processed
    pub fn process_messages(&mut self) -> bool {
        let mut processed = false;

        while let Ok(msg) = self.process_rx.try_recv() {
            processed = true;
            match msg {
                ProcessMessage::Output { name, text } => {
                    // Find or create process buffer
                    let buf_idx = self.buffers.iter()
                        .position(|b| b.name == name)
                        .unwrap_or_else(|| {
                            let buf = Buffer::new(&name);
                            self.buffers.push(buf);
                            self.buffers.len() - 1
                        });

                    // Append output to buffer
                    let buf = &mut self.buffers[buf_idx];
                    buf.append(&text);

                    // Ring buffer: trim if too many lines
                    let line_count = buf.line_count();
                    if line_count > MAX_PROCESS_BUFFER_LINES {
                        let lines_to_remove = line_count - MAX_PROCESS_BUFFER_LINES;
                        buf.delete_lines(0, lines_to_remove);
                    }
                }
                ProcessMessage::Exited { name, exit_code } => {
                    // Find process buffer and append exit message
                    if let Some(buf_idx) = self.buffers.iter().position(|b| b.name == name) {
                        let msg = match exit_code {
                            Some(code) => format!("\n\nProcess {} exited with code {}\n", name, code),
                            None => format!("\n\nProcess {} exited\n", name),
                        };
                        self.buffers[buf_idx].append(&msg);
                    }
                    // Remove from registry
                    self.processes.remove(&name);
                    self.status_message = Some(format!("Process {} exited", name));
                }
            }
        }

        processed
    }

    /// Process chat stream events (Claude responses)
    /// Returns true if any events were processed
    pub fn process_chat(&mut self) -> bool {
        let mut processed = false;

        while let Ok(event) = self.chat_rx.try_recv() {
            processed = true;
            match event {
                StreamEvent::Text(text) => {
                    // Accumulate response for completion callback
                    self.chat_response_buffer.push_str(&text);
                    // Find *chat* buffer and append
                    if let Some(buf_idx) = self.buffers.iter().position(|b| b.name == "*chat*") {
                        self.buffers[buf_idx].append(&text);
                    }
                }
                StreamEvent::Done => {
                    // Append newlines for separation
                    if let Some(buf_idx) = self.buffers.iter().position(|b| b.name == "*chat*") {
                        self.buffers[buf_idx].append("\n\n");
                    }
                    self.chat_streaming = false;

                    // Call Scheme callback with the complete response
                    let response = std::mem::take(&mut self.chat_response_buffer);
                    let escaped = response.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");
                    let _ = self.run_scheme("chat_complete", &format!(r#"(chat-on-response-complete "{}")"#, escaped));
                }
                StreamEvent::Error(err) => {
                    self.chat_streaming = false;
                    self.chat_response_buffer.clear();
                    self.status_message = Some(format!("Chat error: {}", err));
                    // Also append error to chat buffer
                    if let Some(buf_idx) = self.buffers.iter().position(|b| b.name == "*chat*") {
                        self.buffers[buf_idx].append(&format!("\n[Error: {}]\n", err));
                    }
                }
            }
        }

        processed
    }

    /// Sync Rust state from Scheme state (for rendering)
    fn sync_minibuffer_state(&mut self) {
        use steel::rvals::SteelVal;

        // Active?
        self.minibuffer_active = self.scheme.run("(minibuffer-active?)")
            .map(|v| matches!(v, SteelVal::BoolV(true)))
            .unwrap_or(false);

        // Prompt
        self.minibuffer_prompt = self.scheme.run("(minibuffer-get-prompt)")
            .ok()
            .and_then(|v| if let SteelVal::StringV(s) = v { Some(s.to_string()) } else { None })
            .unwrap_or_default();

        // Input
        self.minibuffer_input = self.scheme.run("(minibuffer-get-value)")
            .ok()
            .and_then(|v| if let SteelVal::StringV(s) = v { Some(s.to_string()) } else { None })
            .unwrap_or_default();

        // Matches
        self.minibuffer_matches = self.scheme.run("(minibuffer-get-matches)")
            .ok()
            .map(|v| {
                if let SteelVal::ListV(items) = v {
                    items.iter()
                        .filter_map(|item| {
                            if let SteelVal::StringV(s) = item {
                                Some(s.to_string())
                            } else {
                                None
                            }
                        })
                        .collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default();

        // Selected
        self.minibuffer_selected = self.scheme.run("(minibuffer-get-selected)")
            .ok()
            .and_then(|v| if let SteelVal::IntV(n) = v { Some(n as usize) } else { None })
            .unwrap_or(0);
    }

    /// Execute a command by name
    pub fn execute_command(&mut self, cmd: &str) -> CommandResult {
        // Fast path for simple commands (no Scheme overhead)
        match cmd {
            "forward-char" => { self.buffer().forward_char(); return CommandResult::Ok; }
            "backward-char" => { self.buffer().backward_char(); return CommandResult::Ok; }
            "next-line" => { self.buffer().next_line(); return CommandResult::Ok; }
            "previous-line" => { self.buffer().previous_line(); return CommandResult::Ok; }
            "forward-word" => { self.buffer().forward_word(); return CommandResult::Ok; }
            "backward-word" => { self.buffer().backward_word(); return CommandResult::Ok; }
            "beginning-of-line" => { self.buffer().beginning_of_line(); return CommandResult::Ok; }
            "end-of-line" => { self.buffer().end_of_line(); return CommandResult::Ok; }
            "beginning-of-buffer" => { self.buffer().beginning_of_buffer(); return CommandResult::Ok; }
            "end-of-buffer" => { self.buffer().end_of_buffer(); return CommandResult::Ok; }
            "delete-backward-char" => { self.buffer().delete_backward(); return CommandResult::Ok; }
            "delete-word-backward" => { self.buffer().delete_word_backward(); return CommandResult::Ok; }
            "delete-word-forward" => { self.buffer().delete_word_forward(); return CommandResult::Ok; }
            "newline" => { self.buffer().insert_char('\n'); return CommandResult::Ok; }
            _ => {}
        }

        // Commands that need Scheme (hooks, complex logic)
        let scheme_cmd = format!("({})", cmd);
        match self.run_scheme("command", &scheme_cmd) {
            Ok(_) => {
                if self.should_quit {
                    CommandResult::Quit
                } else {
                    CommandResult::Ok
                }
            }
            Err(e) => {
                self.status_message = Some(format!("{}: {}", cmd, e));
                CommandResult::Error(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cwd() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
    }

    #[test]
    fn test_editor_find_file_flow() {
        let mut editor = Editor::new(test_cwd());

        // Initially minibuffer is not active
        assert!(!editor.minibuffer_active);

        // Start find-file
        editor.find_file();
        assert!(editor.minibuffer_active);
        assert_eq!(editor.minibuffer_prompt, "Find file: ");
        assert_eq!(editor.minibuffer_input, "");

        // Should have completions (files in project root)
        println!("Initial matches: {:?}", editor.minibuffer_matches);
        assert!(!editor.minibuffer_matches.is_empty());
        assert!(editor.minibuffer_matches.contains(&"core/".to_string()));
    }

    #[test]
    fn test_editor_type_subdirectory() {
        let mut editor = Editor::new(test_cwd());

        editor.find_file();

        // Type "core/"
        for c in "core/".chars() {
            editor.minibuffer_insert(c);
        }

        println!("After typing 'core/':");
        println!("  input: {}", editor.minibuffer_input);
        println!("  matches: {:?}", editor.minibuffer_matches);

        assert_eq!(editor.minibuffer_input, "core/");
        // Should show files IN core/, not files matching "core/"
        assert!(!editor.minibuffer_matches.is_empty());
        assert!(editor.minibuffer_matches.contains(&"src/".to_string()),
            "core/ should contain src/, got: {:?}", editor.minibuffer_matches);
    }

    #[test]
    fn test_editor_tab_complete() {
        let mut editor = Editor::new(test_cwd());

        editor.find_file();

        // Type "cor" - should uniquely match core/
        for c in "cor".chars() {
            editor.minibuffer_insert(c);
        }
        println!("After 'cor': matches = {:?}", editor.minibuffer_matches);
        assert!(editor.minibuffer_matches.contains(&"core/".to_string()));

        // Tab to complete - should complete to core/
        editor.minibuffer_complete();
        println!("After tab: input = {}", editor.minibuffer_input);

        // Should have completed to "core/"
        assert_eq!(editor.minibuffer_input, "core/");

        // And now should show files in core/
        println!("Matches in core/: {:?}", editor.minibuffer_matches);
        assert!(editor.minibuffer_matches.contains(&"src/".to_string()));
    }

    #[test]
    fn test_editor_nested_directory() {
        let mut editor = Editor::new(test_cwd());

        editor.find_file();

        // Type "core/src/"
        for c in "core/src/".chars() {
            editor.minibuffer_insert(c);
        }

        println!("After 'core/src/':");
        println!("  input: {}", editor.minibuffer_input);
        println!("  matches: {:?}", editor.minibuffer_matches);

        // Should show .rs files
        assert!(!editor.minibuffer_matches.is_empty());
        assert!(editor.minibuffer_matches.iter().any(|m| m.ends_with(".rs")),
            "Should have .rs files, got: {:?}", editor.minibuffer_matches);
    }

    #[test]
    fn test_editor_minibuffer_navigation() {
        let mut editor = Editor::new(test_cwd());

        editor.find_file();
        assert!(editor.minibuffer_active);
        assert!(!editor.minibuffer_matches.is_empty());

        // Initially selected = 0
        assert_eq!(editor.minibuffer_selected, 0);

        // Move down
        editor.minibuffer_next();
        assert_eq!(editor.minibuffer_selected, 1, "Down arrow should select next item");

        // Move down again
        editor.minibuffer_next();
        assert_eq!(editor.minibuffer_selected, 2, "Down arrow should select next item");

        // Move up
        editor.minibuffer_prev();
        assert_eq!(editor.minibuffer_selected, 1, "Up arrow should select previous item");

        // Move up back to 0
        editor.minibuffer_prev();
        assert_eq!(editor.minibuffer_selected, 0, "Up arrow should select previous item");

        // Move up at 0 should stay at 0 (not wrap or go negative)
        editor.minibuffer_prev();
        assert_eq!(editor.minibuffer_selected, 0, "Up at 0 should stay at 0");
    }

    #[test]
    fn test_editor_open_file_updates_cwd() {
        let mut editor = Editor::new(test_cwd());
        let initial_cwd = editor.cwd.clone();

        println!("Initial cwd: {:?}", initial_cwd);

        // Start find-file
        editor.find_file();

        // Type "core/src/lib.rs"
        for c in "core/src/lib.rs".chars() {
            editor.minibuffer_insert(c);
        }

        println!("Input: {}", editor.minibuffer_input);

        // Submit to open the file
        let result = editor.minibuffer_submit();
        println!("Submit result: {:?}", result);
        println!("New cwd: {:?}", editor.cwd);
        println!("Buffer name: {}", editor.buffer().name);

        // cwd should now be core/src/
        assert!(editor.cwd.ends_with("core/src"),
            "cwd should be core/src, got: {:?}", editor.cwd);

        // Now find-file again
        editor.find_file();

        println!("After second find-file:");
        println!("  matches: {:?}", editor.minibuffer_matches);

        // Should show files in core/src/ (like lib.rs, buffer.rs, etc)
        assert!(editor.minibuffer_matches.iter().any(|m| m.ends_with(".rs")),
            "Should show .rs files in core/src/, got: {:?}", editor.minibuffer_matches);
    }

    #[test]
    fn test_editor_file_actually_loads_content() {
        let mut editor = Editor::new(test_cwd());

        // Buffer should start empty (scratch buffer)
        assert_eq!(editor.buffer().name, "*scratch*");
        assert_eq!(editor.buffer().text(), "");

        // Open Cargo.toml which we know exists
        editor.find_file();
        for c in "core/Cargo.toml".chars() {
            editor.minibuffer_insert(c);
        }

        println!("Before submit:");
        println!("  input: {:?}", editor.minibuffer_input);
        println!("  buffer name: {}", editor.buffer().name);

        let result = editor.minibuffer_submit();

        println!("After submit:");
        println!("  result: {:?}", result);
        println!("  status: {:?}", editor.status_message);
        println!("  buffer name: {}", editor.buffer().name);
        println!("  buffer text (first 100 chars): {:?}", &editor.buffer().text().chars().take(100).collect::<String>());

        // Check result
        assert!(result.is_ok(), "Submit failed: {:?}", result);

        // Check buffer name changed
        assert_eq!(editor.buffer().name, "Cargo.toml", "Buffer name should be Cargo.toml");

        // Check buffer has content
        let text = editor.buffer().text();
        assert!(!text.is_empty(), "Buffer should have content");
        assert!(text.contains("[package]"), "Cargo.toml should contain [package], got: {}", &text[..100.min(text.len())]);
    }
}