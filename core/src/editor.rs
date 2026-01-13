//! Editor - Testable editor core
//!
//! This module provides a headless editor that can be tested without a terminal.
//! The TUI is just a rendering layer on top of this.

use std::collections::HashMap;
use std::path::PathBuf;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::mpsc::{self, Receiver, Sender};
use crate::{Buffer, BufferStore, Interpreter, command::CommandResult, scheme::{Action, ChatMessageData}};
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
    /// Shared buffer store (accessible by Scheme)
    pub buffers: BufferStore,
    pub scheme: Interpreter,
    pub cwd: PathBuf,

    // IPC command queue
    pub ipc_tx: Sender<crate::ipc::IpcRequest>,
    pub ipc_rx: Receiver<crate::ipc::IpcRequest>,
    pub event_bus: std::sync::Arc<crate::ipc::EventBus>,

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

    // Tool system
    pub tool_registry: crate::tools::ToolRegistry,

    // Keybinding requests for TUI to apply
    pub pending_keybindings: Vec<(String, String)>,  // (key, command)

    // Line buffers for process output filtering
    // Accumulates partial lines until newline, then calls filter
    process_line_buffers: HashMap<String, String>,
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
        // Initialize logging
        crate::log::init();
        crate::log::log("info", "editor", "startup", &[
            ("cwd", &cwd.to_string_lossy()),
        ]);

        // Create shared buffer store
        let buffers = BufferStore::new();

        // Create interpreter with shared buffer access
        let scheme = Interpreter::with_buffer_store(buffers.clone());

        let (ipc_tx, ipc_rx) = mpsc::channel();
        let (process_tx, process_rx) = mpsc::channel();
        let (chat_tx, chat_rx) = mpsc::channel();
        let event_bus = std::sync::Arc::new(crate::ipc::EventBus::new());

        let mut editor = Editor {
            buffers,
            scheme,
            cwd,
            ipc_tx,
            ipc_rx,
            event_bus,
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
            // Tool registry with built-in tools
            tool_registry: crate::tools::ToolRegistry::new(),
            pending_keybindings: Vec::new(),
            process_line_buffers: HashMap::new(),
        };

        // Load user init file
        editor.load_user_init();

        crate::log::log("info", "editor", "ready", &[
            ("buffers", &editor.buffers.len().to_string()),
        ]);

        editor
    }

    /// Load user's init.scm from ~/.aimax/init.scm
    fn load_user_init(&mut self) {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let init_path = PathBuf::from(&home).join(".aimax").join("init.scm");

        if init_path.exists() {
            crate::log::log("info", "scheme", "loading-init", &[
                ("path", &init_path.to_string_lossy()),
            ]);
            match std::fs::read_to_string(&init_path) {
                Ok(code) => {
                    if let Err(e) = self.run_scheme("init", &code) {
                        crate::log::log("error", "scheme", "init-error", &[
                            ("error", &e),
                        ]);
                        self.status_message = Some(format!("Error in init.scm: {}", e));
                    } else {
                        crate::log::log("info", "scheme", "init-loaded", &[]);
                    }
                }
                Err(e) => {
                    crate::log::log("error", "scheme", "init-read-error", &[
                        ("error", &e.to_string()),
                    ]);
                }
            }
        }
    }

    /// Get current buffer Arc (caller must lock)
    pub fn current_buffer(&self) -> std::sync::Arc<std::sync::RwLock<Buffer>> {
        self.buffers.current().expect("no current buffer")
    }

    /// Execute a closure with write access to current buffer
    pub fn with_buffer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut Buffer) -> R,
    {
        let buf_arc = self.current_buffer();
        let mut buf = buf_arc.write().unwrap();
        f(&mut buf)
    }

    /// Execute a closure with read access to current buffer
    pub fn with_buffer_ref<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Buffer) -> R,
    {
        let buf_arc = self.current_buffer();
        let buf = buf_arc.read().unwrap();
        f(&buf)
    }

    /// Buffer names for completion
    pub fn buffer_names(&self) -> Vec<String> {
        self.buffers.names()
    }

    /// Get full editor state as JSON for IPC
    pub fn get_state_json(&self) -> serde_json::Value {
        let current_name = self.buffers.current_name();
        let mut buffer_list = Vec::new();

        self.buffers.for_each(|name, buf| {
            buffer_list.push(serde_json::json!({
                "name": buf.name,
                "file_path": buf.file_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                "modified": buf.is_modified(),
                "point": buf.point(),
                "line": buf.current_line(),
                "column": buf.current_column(),
                "current": name == current_name,
            }));
        });

        let processes: Vec<String> = self.processes.list();

        serde_json::json!({
            "buffers": buffer_list,
            "current_buffer": current_name,
            "processes": processes,
            "chat_streaming": self.chat_streaming,
            "chat_message_count": self.chat_messages.len(),
            "minibuffer_active": self.minibuffer_active,
            "cwd": self.cwd.to_string_lossy().to_string(),
        })
    }

    /// Sync buffer state to Scheme globals (lightweight)
    /// With BufferStore, Scheme has direct access, but we still sync some state for legacy code
    fn sync_buffer_state(&mut self) {
        // Update tree-sitter tree before syncing
        if let Some(buf_arc) = self.buffers.current() {
            let file_path_for_lang;
            let buf_name;
            {
                let buf = buf_arc.read().unwrap();
                file_path_for_lang = buf.file_path.clone();
                buf_name = buf.name.clone();
            }
            let lang = crate::syntax::Lang::from_path(
                file_path_for_lang.as_deref().unwrap_or(std::path::Path::new(&buf_name))
            );
            if let Some(ts_lang) = lang.tree_sitter_language() {
                buf_arc.write().unwrap().update_tree(ts_lang);
            }
        }

        // Sync to SharedState for legacy Scheme code
        // With BufferStore, most reads go through buffer primitives directly
        if let Ok(mut state) = self.scheme.shared_state.write() {
            let mut file_paths = Vec::new();
            let mut names = Vec::new();

            self.buffers.for_each(|_name, buf| {
                if let Some(ref path) = buf.file_path {
                    file_paths.push(path.to_string_lossy().to_string());
                }
                names.push(buf.name.clone());
            });

            state.buffer_file_paths = file_paths;
            state.buffer_names = names;

            // Current buffer info
            if let Some(buf_arc) = self.buffers.current() {
                let buf = buf_arc.read().unwrap();
                state.buffer_name = buf.name.clone();
                state.buffer_text = buf.text();
                state.buffer_lines = buf.lines().collect();
                state.buffer_major_mode = buf.major_mode.clone();
                state.buffer_modified = buf.is_modified();
                state.buffer_tree = buf.tree.clone().map(std::sync::Arc::new);
                state.buffer_point = buf.point();
            }

            // Sync running process names
            state.process_names = self.processes.list();
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
                Action::Insert(text) => self.with_buffer(|buf| buf.insert(&text)),
                Action::Delete => self.with_buffer(|buf| buf.delete_backward()),
                Action::CreateBuffer(name) => {
                    // Create new buffer if it doesn't exist
                    // Note: buffer-switch is now immediate in Scheme, so we don't switch here
                    self.buffers.create(&name);
                }
                Action::SwitchBuffer(name) => {
                    // This is now mostly handled immediately by Scheme's buffer-switch
                    // But we keep the action for backwards compatibility
                    if self.buffers.set_current(&name) {
                        self.status_message = Some(format!("Switched to {}", name));
                    } else {
                        self.status_message = Some(format!("Buffer not found: {}", name));
                    }
                }
                Action::Open(path) => {
                    if let Ok(buf) = Buffer::from_file(&path) {
                        let buf_name = buf.name.clone();
                        // Check if already open by file path
                        let existing = self.buffers.with_buffer(&buf_name, |b| {
                            b.file_path.as_ref().map(|p| p.to_string_lossy().to_string())
                        }).flatten();

                        if existing.is_some() {
                            self.buffers.set_current(&buf_name);
                        } else {
                            self.buffers.insert(buf);
                            self.buffers.set_current(&buf_name);
                        }
                        self.status_message = Some(format!("Opened {}", path));
                    }
                }
                Action::Message(msg) => self.status_message = Some(msg),
                // Movement actions
                Action::SetPoint(n) => self.with_buffer(|buf| buf.set_point(n)),
                Action::ForwardChar => self.with_buffer(|buf| buf.forward_char()),
                Action::BackwardChar => self.with_buffer(|buf| buf.backward_char()),
                Action::ForwardWord => self.with_buffer(|buf| buf.forward_word()),
                Action::BackwardWord => self.with_buffer(|buf| buf.backward_word()),
                Action::NextLine => self.with_buffer(|buf| buf.next_line()),
                Action::PreviousLine => self.with_buffer(|buf| buf.previous_line()),
                Action::BeginningOfLine => self.with_buffer(|buf| buf.beginning_of_line()),
                Action::EndOfLine => self.with_buffer(|buf| buf.end_of_line()),
                Action::BeginningOfBuffer => self.with_buffer(|buf| buf.beginning_of_buffer()),
                Action::EndOfBuffer => self.with_buffer(|buf| buf.end_of_buffer()),
                Action::GotoLine(n) => self.with_buffer(|buf| buf.goto_line(n)),
                // Deletion
                Action::DeleteWordBackward => self.with_buffer(|buf| buf.delete_word_backward()),
                Action::DeleteWordForward => self.with_buffer(|buf| buf.delete_word_forward()),
                // Special commands
                Action::Quit => self.should_quit = true,
                Action::FindFileInteractive => self.find_file(),
                Action::SwitchBufferInteractive => self.switch_buffer(),
                Action::SaveBuffer => {
                    let has_path = self.with_buffer_ref(|buf| buf.file_path.is_some());
                    if has_path {
                        let result = self.with_buffer(|buf| buf.save());
                        match result {
                            Ok(()) => self.status_message = Some("Saved".to_string()),
                            Err(e) => self.status_message = Some(format!("Error: {}", e)),
                        }
                    } else {
                        self.status_message = Some("No file path".to_string());
                    }
                }
                Action::KillBuffer => {
                    let name = self.with_buffer_ref(|buf| buf.name.clone());
                    if self.buffers.remove(&name) {
                        self.status_message = Some(format!("Killed {}", name));
                    } else {
                        self.status_message = Some("Can't kill last buffer".to_string());
                    }
                }
                Action::KillBufferNamed(name) => {
                    crate::log::log("debug", "editor", "kill-buffer-action", &[
                        ("name", &name),
                        ("buffers_before", &self.buffers.len().to_string()),
                    ]);
                    if self.buffers.remove(&name) {
                        crate::log::log("info", "editor", "buffer-killed", &[
                            ("name", &name),
                            ("buffers_after", &self.buffers.len().to_string()),
                        ]);
                        self.status_message = Some(format!("Killed {}", name));
                    } else if !self.buffers.exists(&name) {
                        self.status_message = Some(format!("No buffer named {}", name));
                    } else {
                        self.status_message = Some("Can't kill last buffer".to_string());
                    }
                }
                Action::KeyboardQuit => {
                    self.minibuffer_cancel();
                    self.status_message = Some("Quit".to_string());
                }
                Action::Newline => self.with_buffer(|buf| buf.insert_char('\n')),
                Action::SetFaceAttribute { face, key, value } => {
                    self.face_actions.push((face, key, value));
                }
                Action::StartProcess { name, command, args } => {
                    match self.processes.spawn(&name, &command, &args) {
                        Ok(_) => {
                            // Create process buffer if needed
                            self.buffers.create(&name);
                            self.buffers.set_current(&name);
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
                            // Create process buffer if needed
                            self.buffers.create(&name);
                            self.buffers.set_current(&name);

                            // Set buffer locals
                            if let Some(buf_arc) = self.buffers.get(&name) {
                                let mut buf = buf_arc.write().unwrap();
                                buf.major_mode = "process".to_string();
                                buf.set_local("process-name", crate::buffer::LocalVar::String(name.clone()));
                                buf.set_local("process-command", crate::buffer::LocalVar::String(cmd_str));
                                if let Some((_, pid)) = self.processes.get_info(&name) {
                                    buf.set_local("process-pid", crate::buffer::LocalVar::Int(pid as i64));
                                }
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
                Action::ExecuteTool { id, name, input } => {
                    self.execute_tool(&id, &name, &input);
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
                Action::GlobalSetKey { key, command } => {
                    // Queue for TUI to apply to keymap
                    self.pending_keybindings.push((key, command));
                }
                Action::SetBufferLocal { buffer, key, value } => {
                    // Set buffer-local variable
                    let buf_name = buffer.unwrap_or_else(|| self.buffers.current_name());
                    if let Some(buf_arc) = self.buffers.get(&buf_name) {
                        buf_arc.write().unwrap().set_local(&key, crate::buffer::LocalVar::String(value));
                    }
                }
            }
        }
    }

    /// Start a chat with an LLM provider
    fn start_chat(&mut self, provider: &str, api_key: &str, model: &str, system: Option<&str>, messages: Vec<ChatMessageData>) {
        if api_key.is_empty() {
            self.status_message = Some("No API key set. Set *ai-api-key* in init.scm".to_string());
            return;
        }

        // Editor just starts the stream - Scheme handles buffer/formatting via callbacks

        // Build config with tools from registry
        let tools: Vec<_> = self.tool_registry.all_tools()
            .into_iter()
            .cloned()
            .collect();

        let config = ChatConfig {
            provider: crate::llm::Provider::from_str(provider),
            api_key: api_key.to_string(),
            model: model.to_string(),
            system: system.map(|s| s.to_string()),
            max_tokens: 4096,
            tools,
        };

        // Build messages for API
        let api_messages: Vec<Message> = messages.into_iter()
            .map(|m| Message {
                role: m.role,
                content: m.content,
                tool_use_id: m.tool_use_id,
                tool_calls: m.tool_calls.into_iter()
                    .map(|(id, name, input)| crate::llm::ToolCallInfo { id, name, input })
                    .collect(),
            })
            .collect();

        // Start streaming
        self.chat_streaming = true;
        chat_stream(config, api_messages, self.chat_tx.clone());
    }

    /// Execute a tool from the registry and report result to Scheme
    fn execute_tool(&mut self, id: &str, name: &str, input: &str) {
        // Parse input JSON
        let input_json: serde_json::Value = match serde_json::from_str(input) {
            Ok(v) => v,
            Err(e) => {
                let error_msg = format!("Failed to parse tool input: {}", e);
                self.report_tool_result(id, false, &error_msg);
                return;
            }
        };

        // Execute the tool
        let result = self.tool_registry.execute(name, input_json);

        // Report result to Scheme
        self.report_tool_result(id, result.success, &result.content);

        // Display result in chat buffer
        if let Some(buf_arc) = self.buffers.get("*chat*") {
            let status = if result.success { "✓" } else { "✗" };
            let truncated = if result.content.len() > 200 {
                format!("{}...", &result.content[..200])
            } else {
                result.content.clone()
            };
            buf_arc.write().unwrap().append(&format!("\n[{} Result: {}]\n", status, truncated));
        }
    }

    /// Report tool execution result to Scheme
    fn report_tool_result(&mut self, id: &str, success: bool, content: &str) {
        let escaped_id = id.replace("\\", "\\\\").replace("\"", "\\\"");
        let escaped_content = content
            .replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\n");
        let status = if success { "success" } else { "error" };

        let _ = self.run_scheme(
            "tool_result",
            &format!(
                r#"(chat-add-tool-result "{}" "{}" "{}")"#,
                escaped_id, status, escaped_content
            ),
        );
    }

    /// Start find-file command
    pub fn find_file(&mut self) {
        self.minibuffer_mode = MinibufferMode::FindFile;
        self.minibuffer_active = true;
        let cwd = self.cwd.to_string_lossy();
        let escaped = cwd.replace("\\", "\\\\").replace("\"", "\\\"");
        // Use embedded minibuffer-start-find-file which doesn't reset mode
        let _ = self.run_scheme("find_file", &format!(r#"(minibuffer-start-find-file "{}")"#, escaped));
        self.sync_minibuffer_state();
        crate::log::log("debug", "editor", "find-file-activated", &[
            ("minibuffer_active", &self.minibuffer_active.to_string()),
            ("prompt", &self.minibuffer_prompt),
            ("input", &self.minibuffer_input),
            ("matches", &self.minibuffer_matches.len().to_string()),
            ("selected", &self.minibuffer_selected.to_string()),
        ]);
    }

    /// Start switch-buffer command
    pub fn switch_buffer(&mut self) {
        self.minibuffer_mode = MinibufferMode::SwitchBuffer;
        // Set up minibuffer with buffer names as completions
        let current_name = self.buffers.current_name();
        let names: Vec<String> = self.buffers.names()
            .into_iter()
            .filter(|name| *name != current_name)  // exclude current buffer
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
        let current_name = self.buffers.current_name();
        self.minibuffer_matches = self.buffers.names()
            .into_iter()
            .filter(|name| *name != current_name)
            .filter(|name| name.to_lowercase().contains(&input))
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
                crate::log_debug!("editor", "minibuffer-next",
                    "selected" => &self.minibuffer_selected.to_string(),
                    "matches" => &self.minibuffer_matches.len().to_string(),
                );
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

        crate::log::log("debug", "editor", "find-file-submit", &[
            ("input", &input),
            ("rust-input", &self.minibuffer_input),
        ]);

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
                crate::log::log("info", "editor", "file-opened", &[
                    ("path", &path.to_string_lossy()),
                    ("lines", &buf.line_count().to_string()),
                ]);
                if let Some(parent) = path.parent() {
                    self.cwd = parent.to_path_buf();
                }
                // Add to buffer list or switch to existing
                let buf_name = buf.name.clone();
                let buf_path = buf.file_path.clone();

                // Check if file is already open
                let mut existing_name = None;
                self.buffers.for_each(|name, b| {
                    if b.file_path == buf_path {
                        existing_name = Some(name.to_string());
                    }
                });

                if let Some(name) = existing_name {
                    self.buffers.set_current(&name);
                } else {
                    self.buffers.insert(buf);
                    self.buffers.set_current(&buf_name);
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

        if self.buffers.set_current(&name) {
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
        use crate::ipc::IpcRequest;

        let mut processed = false;
        while let Ok(request) = self.ipc_rx.try_recv() {
            processed = true;

            match request {
                IpcRequest::Eval { code, reply } => {
                    // Log truncated preview
                    let preview = if code.len() > 60 {
                        format!("{}...", &code[..60])
                    } else {
                        code.clone()
                    };
                    crate::log::log("info", "ipc", "eval", &[("code", &preview)]);

                    // Execute and return result
                    let result = if !code.starts_with('(') {
                        use crate::command::CommandResult;
                        match self.execute_command(&code) {
                            CommandResult::Ok => Ok("ok".to_string()),
                            CommandResult::Message(m) => Ok(m),
                            CommandResult::Error(e) => Err(e),
                            CommandResult::Quit => Ok("quit".to_string()),
                            CommandResult::NotImplemented(c) => Err(format!("not-implemented: {}", c)),
                        }
                    } else {
                        // Use run_scheme which syncs buffer state first
                        match self.run_scheme("ipc-eval", &code) {
                            Ok(val) => Ok(format!("{}", val)),
                            Err(e) => Err(e),
                        }
                    };

                    let _ = reply.send(result);
                }
                IpcRequest::GetState { reply } => {
                    crate::log::log("info", "ipc", "get_state", &[]);
                    let state = self.get_state_json();
                    let _ = reply.send(state);
                }
            }
        }
        processed
    }

    /// Process messages from running processes (PTY output)
    /// Returns true if any messages were processed
    ///
    /// If buffer has a "process-filter" local variable (Scheme function name),
    /// each complete line is passed through that filter before display.
    /// Filter returns: transformed line, or empty string to skip.
    pub fn process_messages(&mut self) -> bool {
        let mut processed = false;

        while let Ok(msg) = self.process_rx.try_recv() {
            processed = true;
            match msg {
                ProcessMessage::Output { name, text } => {
                    // Find or create process buffer
                    self.buffers.create(&name);

                    // Check for process-filter local variable
                    let filter_fn = self.buffers.with_buffer(&name, |buf| {
                        buf.locals.get("process-filter").and_then(|v| match v {
                            crate::buffer::LocalVar::String(s) => Some(s.clone()),
                            _ => None,
                        })
                    }).flatten();

                    if let Some(filter_name) = filter_fn {
                        // Line-based filtering mode
                        // Accumulate in line buffer
                        let line_buf = self.process_line_buffers
                            .entry(name.clone())
                            .or_insert_with(String::new);
                        line_buf.push_str(&text);

                        // Process complete lines
                        let mut output = String::new();
                        while let Some(newline_pos) = line_buf.find('\n') {
                            let line = line_buf[..newline_pos].to_string();
                            *line_buf = line_buf[newline_pos + 1..].to_string();

                            // Call Scheme filter function
                            let expr = format!("({} \"{}\")",
                                filter_name,
                                line.replace("\\", "\\\\").replace("\"", "\\\""));

                            match self.scheme.run(&expr) {
                                Ok(result) => {
                                    let filtered: String = result.to_string();
                                    // Remove quotes from string result
                                    let filtered = filtered.trim_matches('"');
                                    if !filtered.is_empty() {
                                        output.push_str(filtered);
                                        output.push('\n');
                                    }
                                }
                                Err(e) => {
                                    log_error("process-filter", &format!("{}: {}", filter_name, e));
                                    // On error, pass through unfiltered
                                    output.push_str(&line);
                                    output.push('\n');
                                }
                            }
                        }

                        // Append filtered output to buffer
                        if !output.is_empty() {
                            self.buffers.with_buffer_mut(&name, |buf| {
                                buf.append(&output);
                                // Ring buffer: trim if too many lines
                                let line_count = buf.line_count();
                                if line_count > MAX_PROCESS_BUFFER_LINES {
                                    let lines_to_remove = line_count - MAX_PROCESS_BUFFER_LINES;
                                    buf.delete_lines(0, lines_to_remove);
                                }
                            });
                        }
                    } else {
                        // No filter - direct append
                        self.buffers.with_buffer_mut(&name, |buf| {
                            buf.append(&text);
                            // Ring buffer: trim if too many lines
                            let line_count = buf.line_count();
                            if line_count > MAX_PROCESS_BUFFER_LINES {
                                let lines_to_remove = line_count - MAX_PROCESS_BUFFER_LINES;
                                buf.delete_lines(0, lines_to_remove);
                            }
                        });
                    }
                }
                ProcessMessage::Exited { name, exit_code } => {
                    // Flush any remaining line buffer content
                    if let Some(remaining) = self.process_line_buffers.remove(&name) {
                        if !remaining.is_empty() {
                            self.buffers.with_buffer_mut(&name, |buf| {
                                buf.append(&remaining);
                                buf.append("\n");
                            });
                        }
                    }

                    // Find process buffer and append exit message
                    let msg = match exit_code {
                        Some(code) => format!("\n\nProcess {} exited with code {}\n", name, code),
                        None => format!("\n\nProcess {} exited\n", name),
                    };
                    self.buffers.with_buffer_mut(&name, |buf| {
                        buf.append(&msg);
                    });

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
    /// All buffer manipulation is delegated to Scheme callbacks
    pub fn process_chat(&mut self) -> bool {
        let mut processed = false;

        while let Ok(event) = self.chat_rx.try_recv() {
            processed = true;
            match event {
                StreamEvent::Text(text) => {
                    // Accumulate for completion callback
                    self.chat_response_buffer.push_str(&text);
                    // Call Scheme to handle text - it decides where to put it
                    let escaped = text.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");
                    let _ = self.run_scheme("chat_text", &format!(r#"(chat-on-text "{}")"#, escaped));
                }
                StreamEvent::Done => {
                    self.chat_streaming = false;
                    // Call Scheme with complete response
                    let response = std::mem::take(&mut self.chat_response_buffer);
                    let escaped = response.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");
                    let _ = self.run_scheme("chat_done", &format!(r#"(chat-on-done "{}")"#, escaped));
                }
                StreamEvent::ToolUse { id, name, input } => {
                    // Pass accumulated text and tool info to Scheme
                    let response = std::mem::take(&mut self.chat_response_buffer);
                    let escaped = response.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");
                    let escaped_id = id.replace("\\", "\\\\").replace("\"", "\\\"");
                    let escaped_name = name.replace("\\", "\\\\").replace("\"", "\\\"");
                    let escaped_input = input.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n");

                    // Scheme handles history and display
                    let _ = self.run_scheme("chat_tool", &format!(
                        r#"(chat-on-tool-use "{}" "{}" "{}" "{}")"#,
                        escaped_id, escaped_name, escaped_input, escaped
                    ));
                }
                StreamEvent::Error(err) => {
                    self.chat_streaming = false;
                    self.chat_response_buffer.clear();
                    let escaped = err.replace("\\", "\\\\").replace("\"", "\\\"");
                    let _ = self.run_scheme("chat_error", &format!(r#"(chat-on-error "{}")"#, escaped));
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
        // Don't log movement commands - too noisy
        match cmd {
            "forward-char" => { self.with_buffer(|buf| buf.forward_char()); return CommandResult::Ok; }
            "backward-char" => { self.with_buffer(|buf| buf.backward_char()); return CommandResult::Ok; }
            "next-line" => { self.with_buffer(|buf| buf.next_line()); return CommandResult::Ok; }
            "previous-line" => { self.with_buffer(|buf| buf.previous_line()); return CommandResult::Ok; }
            "forward-word" => { self.with_buffer(|buf| buf.forward_word()); return CommandResult::Ok; }
            "backward-word" => { self.with_buffer(|buf| buf.backward_word()); return CommandResult::Ok; }
            "beginning-of-line" => { self.with_buffer(|buf| buf.beginning_of_line()); return CommandResult::Ok; }
            "end-of-line" => { self.with_buffer(|buf| buf.end_of_line()); return CommandResult::Ok; }
            "beginning-of-buffer" => { self.with_buffer(|buf| buf.beginning_of_buffer()); return CommandResult::Ok; }
            "end-of-buffer" => { self.with_buffer(|buf| buf.end_of_buffer()); return CommandResult::Ok; }
            "delete-backward-char" => { self.with_buffer(|buf| buf.delete_backward()); return CommandResult::Ok; }
            "delete-word-backward" => { self.with_buffer(|buf| buf.delete_word_backward()); return CommandResult::Ok; }
            "delete-word-forward" => { self.with_buffer(|buf| buf.delete_word_forward()); return CommandResult::Ok; }
            "newline" => { self.with_buffer(|buf| buf.insert_char('\n')); return CommandResult::Ok; }
            _ => {}
        }

        // Log non-movement commands
        let buf_name = self.with_buffer_ref(|buf| buf.name.clone());
        crate::log::log("debug", "editor", "command", &[
            ("name", cmd),
            ("buffer", &buf_name),
        ]);

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
        println!("Buffer name: {}", editor.with_buffer_ref(|b| b.name.clone()));

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
        assert_eq!(editor.with_buffer_ref(|b| b.name.clone()), "*scratch*");
        assert_eq!(editor.with_buffer_ref(|b| b.text()), "");

        // Open Cargo.toml which we know exists
        editor.find_file();
        for c in "core/Cargo.toml".chars() {
            editor.minibuffer_insert(c);
        }

        println!("Before submit:");
        println!("  input: {:?}", editor.minibuffer_input);
        println!("  buffer name: {}", editor.with_buffer_ref(|b| b.name.clone()));

        let result = editor.minibuffer_submit();

        println!("After submit:");
        println!("  result: {:?}", result);
        println!("  status: {:?}", editor.status_message);
        println!("  buffer name: {}", editor.with_buffer_ref(|b| b.name.clone()));
        println!("  buffer text (first 100 chars): {:?}", &editor.with_buffer_ref(|b| b.text()).chars().take(100).collect::<String>());

        // Check result
        assert!(result.is_ok(), "Submit failed: {:?}", result);

        // Check buffer name changed
        assert_eq!(editor.with_buffer_ref(|b| b.name.clone()), "Cargo.toml", "Buffer name should be Cargo.toml");

        // Check buffer has content
        let text = editor.with_buffer_ref(|b| b.text());
        assert!(!text.is_empty(), "Buffer should have content");
        assert!(text.contains("[package]"), "Cargo.toml should contain [package], got: {}", &text[..100.min(text.len())]);
    }
}