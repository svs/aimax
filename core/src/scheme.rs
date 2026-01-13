//! Scheme scripting with Steel
//!
//! Provides:
//! - Steel interpreter instance
//! - Rust primitives exposed to Scheme (ls, fuzzy-match, etc.)
//! - Bridge for Scheme to control editor

use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;
use steel::rvals::SteelVal;
use std::path::PathBuf;
use tree_sitter::{Query, QueryCursor};
use streaming_iterator::StreamingIterator;

use std::sync::{Arc, Mutex, RwLock};

/// Chat message for the Action queue
#[derive(Debug, Clone)]
pub struct ChatMessageData {
    pub role: String,
    pub content: String,
    pub tool_use_id: Option<String>,
    pub tool_calls: Vec<(String, String, String)>,  // (id, name, input)
}

/// Action requested by Scheme to be performed on the Editor
pub enum Action {
    Insert(String),
    Delete,
    CreateBuffer(String),
    SwitchBuffer(String),
    Open(String),
    Message(String),
    // Movement actions
    SetPoint(usize),
    ForwardChar,
    BackwardChar,
    ForwardWord,
    BackwardWord,
    NextLine,
    PreviousLine,
    BeginningOfLine,
    EndOfLine,
    BeginningOfBuffer,
    EndOfBuffer,
    GotoLine(usize),
    // Deletion
    DeleteWordBackward,
    DeleteWordForward,
    // Special commands (interactive versions)
    Quit,
    FindFileInteractive,
    SwitchBufferInteractive,
    SaveBuffer,
    KillBuffer,
    KillBufferNamed(String),
    KeyboardQuit,
    Newline,
    // Face system
    SetFaceAttribute { face: String, key: String, value: String },
    // Process system
    StartProcess { name: String, command: String, args: Vec<String> },
    StartProcessSimple { name: String, command: String, args: Vec<String> },
    ProcessSendString { name: String, text: String },
    KillProcess { name: String },
    // Chat system
    Chat {
        provider: String,  // "anthropic", "openai", "ollama", etc.
        api_key: String,
        model: String,
        system: Option<String>,
        messages: Vec<ChatMessageData>,
    },
    // Tool execution (from agentic chat)
    ExecuteTool {
        id: String,      // Tool call ID (for response)
        name: String,    // Tool name
        input: String,   // JSON input string
    },
    // Minibuffer - Scheme activated generic minibuffer, pushes state through
    MinibufferActivate { prompt: String },
    // Keybindings
    GlobalSetKey { key: String, command: String },
    // Buffer local variables
    SetBufferLocal { buffer: Option<String>, key: String, value: String },
}

/// Shared state for Scheme to query buffer contents
/// This is synced from Editor before Scheme execution
#[derive(Debug, Default)]
pub struct SharedState {
    pub buffer_text: String,
    pub buffer_lines: Vec<String>,
    pub buffer_file_paths: Vec<String>,  // All open buffer file paths
    pub buffer_names: Vec<String>,        // All buffer names
    pub buffer_modified: bool,            // Is current buffer modified?
    pub buffer_modified_map: std::collections::HashMap<String, bool>,  // name -> modified?
    pub buffer_locals_map: std::collections::HashMap<String, std::collections::HashMap<String, String>>,  // buffer -> (key -> value)
    pub process_names: Vec<String>,       // Names of running processes
    pub buffer_major_mode: String,         // Current buffer's major mode
    pub buffer_name: String,               // Current buffer name
    pub buffer_locals: std::collections::HashMap<String, String>,  // Buffer local vars (stringified)
    pub buffer_tree: Option<std::sync::Arc<tree_sitter::Tree>>, // Current buffer's syntax tree
}

/// The Scheme interpreter
pub struct Interpreter {
    engine: Engine,
    pub pending_actions: Arc<Mutex<Vec<Action>>>,
    pub shared_state: Arc<RwLock<SharedState>>,
}

/// Embedded Scheme code - core UI modules
const CORE_SCHEME: &str = r#"
;;; completion.scm - File completion

(define (string-last-index-of str char)
  (let loop ((i (- (string-length str) 1)))
    (cond ((< i 0) #f)
          ((string=? (substring str i (+ i 1)) char) i)
          (else (loop (- i 1))))))

;; Check if haystack contains needle
(define (string-contains? haystack needle)
  (let ((hlen (string-length haystack))
        (nlen (string-length needle)))
    (if (> nlen hlen)
        #f
        (let loop ((i 0))
          (cond ((> (+ i nlen) hlen) #f)
                ((string=? (substring haystack i (+ i nlen)) needle) #t)
                (else (loop (+ i 1))))))))

(define (parse-file-input input cwd)
  (let* ((expanded (expand-path input cwd))
         (last-slash (string-last-index-of expanded "/")))
    (if last-slash
        (let ((dir (substring expanded 0 (+ last-slash 1)))
              (prefix (substring expanded (+ last-slash 1))))
          (cons dir prefix))
        (cons cwd input))))

(define (file-completion-names input cwd)
  (let* ((parsed (parse-file-input input cwd))
         (dir (car parsed))
         (prefix (cdr parsed))
         (files (ls dir))
         (matches (if (string=? prefix "")
                      files
                      (fuzzy-match prefix files))))
    matches))

;;; minibuffer.scm - Minibuffer state and logic

(define *minibuffer-active* #f)
(define *minibuffer-prompt* "")
(define *minibuffer-input* "")
(define *minibuffer-point* 0)
(define *minibuffer-matches* '())
(define *minibuffer-selected* 0)
(define *completing-file* #f)
(define *cwd* ".")

(define (minibuffer-start-find-file cwd)
  (set! *cwd* cwd)
  (set! *completing-file* #t)
  (set! *minibuffer-active* #t)
  (set! *minibuffer-prompt* "Find file: ")
  (set! *minibuffer-input* "")
  (set! *minibuffer-point* 0)
  (set! *minibuffer-selected* 0)
  (minibuffer-update-completions))

(define (minibuffer-cancel)
  (set! *minibuffer-active* #f)
  (set! *completing-file* #f))

(define (minibuffer-update-completions)
  (when *completing-file*
    (set! *minibuffer-matches* (file-completion-names *minibuffer-input* *cwd*))
    (set! *minibuffer-selected* 0)))

(define (minibuffer-insert-char c)
  (set! *minibuffer-input*
        (string-append
          (substring *minibuffer-input* 0 *minibuffer-point*)
          c
          (substring *minibuffer-input* *minibuffer-point*)))
  (set! *minibuffer-point* (+ *minibuffer-point* 1))
  (when *completing-file*
    (minibuffer-update-completions)))

(define (minibuffer-delete-backward)
  (when (> *minibuffer-point* 0)
    (set! *minibuffer-input*
          (string-append
            (substring *minibuffer-input* 0 (- *minibuffer-point* 1))
            (substring *minibuffer-input* *minibuffer-point*)))
    (set! *minibuffer-point* (- *minibuffer-point* 1))
    (when *completing-file*
      (minibuffer-update-completions))))

(define (minibuffer-complete)
  (when (not (null? *minibuffer-matches*))
    (let* ((selected (list-ref *minibuffer-matches* *minibuffer-selected*))
           (dir-part (if (string-last-index-of *minibuffer-input* "/")
                         (substring *minibuffer-input* 0
                           (+ (string-last-index-of *minibuffer-input* "/") 1))
                         "")))
      (set! *minibuffer-input* (string-append dir-part selected))
      (set! *minibuffer-point* (string-length *minibuffer-input*))
      (minibuffer-update-completions))))

(define (minibuffer-next)
  (when (< *minibuffer-selected* (- (length *minibuffer-matches*) 1))
    (set! *minibuffer-selected* (+ *minibuffer-selected* 1))))

(define (minibuffer-prev)
  (when (> *minibuffer-selected* 0)
    (set! *minibuffer-selected* (- *minibuffer-selected* 1))))

(define (minibuffer-get-value) *minibuffer-input*)
(define (minibuffer-active?) *minibuffer-active*)
(define (minibuffer-get-prompt) *minibuffer-prompt*)
(define (minibuffer-get-matches) *minibuffer-matches*)
(define (minibuffer-get-selected) *minibuffer-selected*)

;; Get the result to use when submitting - uses selected match with dir prefix
(define (minibuffer-get-result)
  (if (null? *minibuffer-matches*)
      *minibuffer-input*
      (let* ((selected (list-ref *minibuffer-matches* *minibuffer-selected*))
             (dir-part (if (string-last-index-of *minibuffer-input* "/")
                           (substring *minibuffer-input* 0
                             (+ (string-last-index-of *minibuffer-input* "/") 1))
                           "")))
        (string-append dir-part selected))))
"#;

impl Interpreter {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        let pending_actions = Arc::new(Mutex::new(Vec::new()));
        let shared_state = Arc::new(RwLock::new(SharedState::default()));

        // Register Rust primitives
        register_primitives(&mut engine, pending_actions.clone(), shared_state.clone());

        Interpreter { engine, pending_actions, shared_state }
    }

    /// Create interpreter with core UI modules loaded
    pub fn with_core() -> Self {
        let mut interp = Self::new();

        // Try to load from scheme/ directory (relative to cwd, or AIMAX_SCHEME_DIR)
        let scheme_dir = std::env::var("AIMAX_SCHEME_DIR")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .or_else(|| {
                // Try relative to current dir
                let path = PathBuf::from("scheme");
                if path.exists() { Some(path) } else { None }
            })
            .or_else(|| {
                // Try parent dir (for tests running from core/)
                let path = PathBuf::from("../scheme");
                if path.exists() { Some(path) } else { None }
            })
            .or_else(|| {
                // Try relative to executable
                std::env::current_exe().ok()
                    .and_then(|p| p.parent().map(|p| p.join("scheme")))
                    .filter(|p| p.exists())
            });

        if let Some(dir) = scheme_dir {
            // Initialize logging early
            crate::log::init();
            crate::log::log("info", "scheme", "loading-core", &[
                ("dir", &dir.to_string_lossy()),
            ]);

            // Helper to load a scheme file with logging
            fn load_scheme_file(interp: &mut Interpreter, path: std::path::PathBuf) {
                if path.exists() {
                    if let Ok(code) = std::fs::read_to_string(&path) {
                        let filename = path.file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        if let Err(e) = interp.run(&code) {
                            crate::log::log("error", "scheme", "load-error", &[
                                ("file", &filename),
                                ("error", &e),
                            ]);
                        } else {
                            crate::log::log("debug", "scheme", "loaded", &[
                                ("file", &filename),
                            ]);
                        }
                    }
                }
            }

            // Load completion.scm first (minibuffer depends on it)
            load_scheme_file(&mut interp, dir.join("completion.scm"));

            // Load minibuffer.scm
            load_scheme_file(&mut interp, dir.join("minibuffer.scm"));

            // Load commands.scm - command definitions
            load_scheme_file(&mut interp, dir.join("commands.scm"));

            // Load desktop.scm - save/restore buffers
            load_scheme_file(&mut interp, dir.join("desktop.scm"));

            // Load chat.scm - AI chat configuration
            load_scheme_file(&mut interp, dir.join("chat.scm"));

            // Load log.scm - Structured log viewing
            load_scheme_file(&mut interp, dir.join("log.scm"));

            crate::log::log("info", "scheme", "core-loaded", &[
                ("files", "completion,minibuffer,commands,desktop,chat,log"),
            ]);
        } else {
            // Fall back to embedded code
            crate::log::init();
            crate::log::log("warn", "scheme", "fallback-mode", &[
                ("reason", "scheme directory not found"),
            ]);
            if let Err(e) = interp.run(CORE_SCHEME) {
                crate::log::log("error", "scheme", "load-error", &[
                    ("file", "embedded-core"),
                    ("error", &format!("{}", e)),
                ]);
            }
        }

        interp
    }

    // ===== Minibuffer API =====

    /// Start find-file in minibuffer
    pub fn minibuffer_find_file(&mut self, cwd: &str) {
        let escaped = cwd.replace("\\", "\\\\").replace("\"", "\\\"");
        let _ = self.run(&format!(r#"(minibuffer-start-find-file "{}")"#, escaped));
    }

    /// Cancel minibuffer
    pub fn minibuffer_cancel(&mut self) {
        let _ = self.run("(minibuffer-cancel)");
    }

    /// Insert character in minibuffer
    pub fn minibuffer_insert(&mut self, c: char) {
        let _ = self.run(&format!(r#"(minibuffer-insert-char "{}")"#, c));
    }

    /// Delete backward in minibuffer
    pub fn minibuffer_delete_backward(&mut self) {
        let _ = self.run("(minibuffer-delete-backward)");
    }

    /// Complete with selected match
    pub fn minibuffer_complete(&mut self) {
        let _ = self.run("(minibuffer-complete)");
    }

    /// Next completion
    pub fn minibuffer_next(&mut self) {
        let _ = self.run("(minibuffer-next)");
    }

    /// Previous completion
    pub fn minibuffer_prev(&mut self) {
        let _ = self.run("(minibuffer-prev)");
    }

    /// Check if minibuffer is active
    pub fn minibuffer_active(&mut self) -> bool {
        self.run("(minibuffer-active?)")
            .map(|v| matches!(v, SteelVal::BoolV(true)))
            .unwrap_or(false)
    }

    /// Get minibuffer prompt
    pub fn minibuffer_prompt(&mut self) -> String {
        self.run("(minibuffer-get-prompt)")
            .ok()
            .and_then(|v| if let SteelVal::StringV(s) = v { Some(s.to_string()) } else { None })
            .unwrap_or_default()
    }

    /// Get minibuffer input value
    pub fn minibuffer_value(&mut self) -> String {
        self.run("(minibuffer-get-value)")
            .ok()
            .and_then(|v| if let SteelVal::StringV(s) = v { Some(s.to_string()) } else { None })
            .unwrap_or_default()
    }

    /// Get completion matches
    pub fn minibuffer_matches(&mut self) -> Vec<String> {
        self.run("(minibuffer-get-matches)")
            .map(|v| self.steel_list_to_strings(v))
            .unwrap_or_default()
    }

    /// Get selected completion index
    pub fn minibuffer_selected(&mut self) -> usize {
        self.run("(minibuffer-get-selected)")
            .ok()
            .and_then(|v| if let SteelVal::IntV(n) = v { Some(n as usize) } else { None })
            .unwrap_or(0)
    }

    /// Convert Steel list to Vec<String>
    fn steel_list_to_strings(&self, val: SteelVal) -> Vec<String> {
        match val {
            SteelVal::ListV(items) => {
                items.iter()
                    .filter_map(|item| {
                        if let SteelVal::StringV(s) = item {
                            Some(s.to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    /// Run Scheme code
    pub fn run(&mut self, code: &str) -> Result<SteelVal, String> {
        // Steel accepts Into<Cow<'static, str>> - owned String works via Cow::Owned
        self.engine
            .run(code.to_string())
            .map(|v| v.into_iter().last().unwrap_or(SteelVal::Void))
            .map_err(|e| format!("{}", e))
    }

    /// Call a Scheme function by name
    pub fn call(&mut self, func_name: &str, args: Vec<SteelVal>) -> Result<SteelVal, String> {
        // Build the call expression
        let args_str: Vec<String> = args.iter().map(|a| format!("{:?}", a)).collect();
        let call = format!("({} {})", func_name, args_str.join(" "));
        self.run(&call)
    }

    /// Load a file
    pub fn load_file(&mut self, path: &str) -> Result<(), String> {
        crate::log::log("debug", "scheme", "load-file-start", &[("path", path)]);
        let code = std::fs::read_to_string(path)
            .map_err(|e| {
                let err = format!("Failed to read {}: {}", path, e);
                crate::log::log("error", "scheme", "load-file-error", &[
                    ("path", path),
                    ("error", &err),
                ]);
                err
            })?;
        self.run(&code)?;
        crate::log::log("debug", "scheme", "load-file-done", &[("path", path)]);
        Ok(())
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

/// Register Rust primitives that Scheme can call
fn register_primitives(
    engine: &mut Engine,
    actions: Arc<Mutex<Vec<Action>>>,
    shared_state: Arc<RwLock<SharedState>>,
) {
    // ls - list files in a directory
    engine.register_fn("ls", scheme_ls);

    // ls-all - list all files including hidden
    engine.register_fn("ls-all", scheme_ls_all);

    // file-exists? - check if file exists
    engine.register_fn("file-exists?", scheme_file_exists);

    // directory? - check if path is a directory
    engine.register_fn("directory?", scheme_is_directory);

    // expand-path - expand ~ and resolve relative paths
    engine.register_fn("expand-path", scheme_expand_path);

    // fuzzy-match - fuzzy match a pattern against candidates
    engine.register_fn("fuzzy-match", scheme_fuzzy_match);

    // write-file - write content to a file
    engine.register_fn("write-file", |path: String, content: String| {
        let _ = std::fs::write(&path, &content);
    });

    // read-file - read a file's contents (returns empty string if not found)
    engine.register_fn("read-file", |path: String| -> String {
        std::fs::read_to_string(&path).unwrap_or_default()
    });

    // ===== Buffer Read Primitives =====
    // These read from shared_state which is synced before Scheme execution

    // (buffer-text) - get entire buffer contents
    let state = shared_state.clone();
    engine.register_fn("buffer-text", move || -> String {
        state.read().map(|s| s.buffer_text.clone()).unwrap_or_default()
    });

    // (buffer-line n) - get specific line (0-indexed)
    let state = shared_state.clone();
    engine.register_fn("buffer-line", move |n: isize| -> String {
        state.read()
            .ok()
            .and_then(|s| s.buffer_lines.get(n as usize).cloned())
            .unwrap_or_default()
    });

    // (buffer-line-count) - get number of lines (from shared state)
    let state = shared_state.clone();
    engine.register_fn("buffer-line-count", move || -> isize {
        state.read().map(|s| s.buffer_lines.len() as isize).unwrap_or(0)
    });

    // (buffer-file-paths) - get all open buffer file paths
    let state = shared_state.clone();
    engine.register_fn("buffer-file-paths", move || -> Vec<String> {
        state.read().map(|s| s.buffer_file_paths.clone()).unwrap_or_default()
    });

    // (buffer-name) - current buffer name
    let state = shared_state.clone();
    engine.register_fn("buffer-name", move || -> String {
        state.read().map(|s| s.buffer_name.clone()).unwrap_or_default()
    });

    // (buffer-names) - all buffer names
    let state = shared_state.clone();
    engine.register_fn("buffer-names", move || -> Vec<String> {
        state.read().map(|s| s.buffer_names.clone()).unwrap_or_default()
    });

    // (process-names) - names of running processes
    let state = shared_state.clone();
    engine.register_fn("process-names", move || -> Vec<String> {
        state.read().map(|s| s.process_names.clone()).unwrap_or_default()
    });

    // (buffer-modified?) - is current buffer modified?
    let state = shared_state.clone();
    engine.register_fn("buffer-modified?", move || -> bool {
        state.read().map(|s| s.buffer_modified).unwrap_or(false)
    });

    // (buffer-modified-p name) - is specific buffer modified?
    let state = shared_state.clone();
    engine.register_fn("buffer-modified-p", move |name: String| -> bool {
        state.read()
            .ok()
            .and_then(|s| s.buffer_modified_map.get(&name).copied())
            .unwrap_or(false)
    });

    // (major-mode) - current buffer's major mode
    let state = shared_state.clone();
    engine.register_fn("major-mode", move || -> String {
        state.read().map(|s| s.buffer_major_mode.clone()).unwrap_or_default()
    });

    // (buffer-local key) - get buffer-local variable
    let state = shared_state.clone();
    engine.register_fn("buffer-local", move |key: String| -> String {
        state.read()
            .ok()
            .and_then(|s| s.buffer_locals.get(&key).cloned())
            .unwrap_or_default()
    });

    // (buffer-local-p buffer-name key) - get local from specific buffer
    let state = shared_state.clone();
    engine.register_fn("buffer-local-p", move |buffer_name: String, key: String| -> String {
        state.read()
            .ok()
            .and_then(|s| s.buffer_locals_map.get(&buffer_name)?.get(&key).cloned())
            .unwrap_or_default()
    });

    // (set-buffer-local! key value) - set local in current buffer
    // (set-buffer-local! buffer-name key value) - set local in named buffer
    // Example: (set-buffer-local! "process-filter" "my-filter-fn")
    // Example: (set-buffer-local! "*log*" "process-filter" "log-filter")
    let actions_clone = actions.clone();
    engine.register_fn("set-buffer-local!", move |key: String, value: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::SetBufferLocal { buffer: None, key, value });
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("set-buffer-local-named!", move |buffer: String, key: String, value: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::SetBufferLocal { buffer: Some(buffer), key, value });
        }
    });

    // ===== Buffer Write Primitives =====

    let actions_clone = actions.clone();
    engine.register_fn("buffer-insert", move |text: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::Insert(text));
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("buffer-delete", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::Delete);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("buffer-create", move |name: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::CreateBuffer(name));
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("buffer-switch", move |name: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::SwitchBuffer(name));
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("buffer-open", move |path: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::Open(path));
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("message", move |msg: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::Message(msg));
        }
    });

    // ===== Movement Primitives =====

    let actions_clone = actions.clone();
    engine.register_fn("set-point", move |n: isize| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::SetPoint(n as usize));
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("forward-char", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::ForwardChar);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("backward-char", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::BackwardChar);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("forward-word", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::ForwardWord);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("backward-word", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::BackwardWord);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("next-line", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::NextLine);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("previous-line", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::PreviousLine);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("beginning-of-line", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::BeginningOfLine);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("end-of-line", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::EndOfLine);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("beginning-of-buffer", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::BeginningOfBuffer);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("end-of-buffer", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::EndOfBuffer);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("goto-line", move |n: isize| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::GotoLine(n as usize));
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("delete-word-backward", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::DeleteWordBackward);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("delete-word-forward", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::DeleteWordForward);
        }
    });

    // Special command primitives
    let actions_clone = actions.clone();
    engine.register_fn("quit!", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::Quit);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("find-file!", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::FindFileInteractive);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("switch-buffer!", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::SwitchBufferInteractive);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("save-buffer!", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::SaveBuffer);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("kill-buffer!", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::KillBuffer);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("kill-buffer-named!", move |name: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::KillBufferNamed(name));
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("keyboard-quit!", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::KeyboardQuit);
        }
    });

    let actions_clone = actions.clone();
    engine.register_fn("newline!", move || {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::Newline);
        }
    });

    // ===== Face Primitives =====

    // (set-face-attribute face-name key value)
    // Example: (set-face-attribute "font-lock-keyword-face" ":foreground" "#ff0000")
    let actions_clone = actions.clone();
    engine.register_fn("set-face-attribute", move |face: String, key: String, value: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::SetFaceAttribute { face, key, value });
        }
    });

    // ===== Process Primitives =====

    // (start-process name command args...) - PTY-based, for interactive
    // Example: (start-process "*shell*" "/bin/bash" '())
    let actions_clone = actions.clone();
    engine.register_fn("start-process", move |name: String, command: String, args: Vec<String>| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::StartProcess { name, command, args });
        }
    });

    // (start-process-simple name command args...) - piped stdout, for tail/streaming
    // Example: (start-process-simple "*tail*" "tail" '("-f" "/var/log/system.log"))
    let actions_clone = actions.clone();
    engine.register_fn("start-process-simple", move |name: String, command: String, args: Vec<String>| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::StartProcessSimple { name, command, args });
        }
    });

    // (process-send-string name text)
    // Example: (process-send-string "*shell*" "ls -la\n")
    let actions_clone = actions.clone();
    engine.register_fn("process-send-string", move |name: String, text: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::ProcessSendString { name, text });
        }
    });

    // (kill-process name) - kill a running process
    let actions_clone = actions.clone();
    engine.register_fn("kill-process", move |name: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::KillProcess { name });
        }
    });

    // (process-running? name) - check if a process is running
    // Returns from shared state (sync'd before Scheme execution)
    let state = shared_state.clone();
    engine.register_fn("process-running?", move |name: String| -> bool {
        state.read()
            .map(|s| s.process_names.contains(&name))
            .unwrap_or(false)
    });

    // (process-list) - get all running process names
    let state = shared_state.clone();
    engine.register_fn("process-list", move || -> Vec<String> {
        state.read()
            .map(|s| s.process_names.clone())
            .unwrap_or_default()
    });

    // ===== Shell Command Primitives =====

    // (shell-command-to-string cmd) - run command synchronously, return output as string
    // Example: (shell-command-to-string "ls -la")
    // Scheme decides what to do with the output (buffer, pipe to LLM, etc.)
    engine.register_fn("shell-command-to-string", |cmd: String| -> String {
        use std::process::Command;
        Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .map(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                if stderr.is_empty() {
                    stdout.to_string()
                } else {
                    format!("{}{}", stdout, stderr)
                }
            })
            .unwrap_or_else(|e| format!("Error: {}", e))
    });

    // ===== Chat Primitives =====

    // (ai-chat provider api-key model system-prompt prompt)
    // provider: "anthropic", "openai", "ollama", "gemini", "deepseek", "groq", "xai"
    // Example: (ai-chat "anthropic" "sk-..." "claude-sonnet-4-20250514" "You help." "Hello")
    let actions_clone = actions.clone();
    engine.register_fn("ai-chat", move |provider: String, api_key: String, model: String, system: String, prompt: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            let system_opt = if system.is_empty() { None } else { Some(system) };
            queue.push(Action::Chat {
                provider,
                api_key,
                model,
                system: system_opt,
                messages: vec![ChatMessageData {
                    role: "user".to_string(),
                    content: prompt,
                    tool_use_id: None,
                    tool_calls: vec![],
                }],
            });
        }
    });

    // (ai-chat-messages provider api-key model system-prompt messages)
    // For multi-turn conversations
    // Messages are lists of (role content tool_use_id ((tool_id tool_name tool_input) ...))
    let actions_clone = actions.clone();
    engine.register_fn("ai-chat-messages", move |provider: String, api_key: String, model: String, system: String, messages: steel::rvals::SteelVal| {
        use steel::rvals::SteelVal;
        if let Ok(mut queue) = actions_clone.lock() {
            let system_opt = if system.is_empty() { None } else { Some(system) };

            // Parse messages from SteelVal
            let msgs: Vec<ChatMessageData> = if let SteelVal::ListV(list) = messages {
                list.iter().filter_map(|msg| {
                    if let SteelVal::ListV(parts) = msg {
                        let parts: Vec<_> = parts.iter().collect();
                        if parts.len() >= 2 {
                            let role = match &parts[0] {
                                SteelVal::StringV(s) => s.to_string(),
                                _ => return None,
                            };
                            let content = match &parts[1] {
                                SteelVal::StringV(s) => s.to_string(),
                                _ => return None,
                            };
                            let tool_use_id = if parts.len() >= 3 {
                                match &parts[2] {
                                    SteelVal::StringV(s) if !s.is_empty() => Some(s.to_string()),
                                    _ => None,
                                }
                            } else { None };

                            // Parse tool_calls from 4th element if present
                            let tool_calls = if parts.len() >= 4 {
                                if let SteelVal::ListV(tools) = &parts[3] {
                                    tools.iter().filter_map(|tc| {
                                        if let SteelVal::ListV(tool_parts) = tc {
                                            let tp: Vec<_> = tool_parts.iter().collect();
                                            if tp.len() >= 3 {
                                                let id = match &tp[0] {
                                                    SteelVal::StringV(s) => s.to_string(),
                                                    _ => return None,
                                                };
                                                let name = match &tp[1] {
                                                    SteelVal::StringV(s) => s.to_string(),
                                                    _ => return None,
                                                };
                                                let input = match &tp[2] {
                                                    SteelVal::StringV(s) => s.to_string(),
                                                    _ => return None,
                                                };
                                                Some((id, name, input))
                                            } else { None }
                                        } else { None }
                                    }).collect()
                                } else { vec![] }
                            } else { vec![] };

                            Some(ChatMessageData { role, content, tool_use_id, tool_calls })
                        } else { None }
                    } else { None }
                }).collect()
            } else { vec![] };

            queue.push(Action::Chat {
                provider,
                api_key,
                model,
                system: system_opt,
                messages: msgs,
            });
        }
    });

    // (ts-query query-text) - Run Tree-sitter query on current buffer
    // Returns list of matches, where each match is a list of (capture-name . text)
    let state = shared_state.clone();
    engine.register_fn("ts-query", move |query_text: String| -> Vec<Vec<(String, String)>> {
        let guard = match state.read() {
            Ok(g) => g,
            Err(_) => return vec![],
        };
        
        let tree = match guard.buffer_tree.as_ref() {
            Some(t) => t,
            None => return vec![],
        };
        
        let text = &guard.buffer_text;
        
        // Map major mode to TS language
        let lang = crate::syntax::Lang::from_extension(&guard.buffer_major_mode);
        let ts_lang = match lang.tree_sitter_language() {
            Some(l) => l,
            None => {
                // Fallback to detection from name
                match crate::syntax::Lang::from_path(std::path::Path::new(&guard.buffer_name)).tree_sitter_language() {
                    Some(l) => l,
                    None => return vec![],
                }
            }
        };
        
        let query = match Query::new(&ts_lang, &query_text) {
            Ok(q) => q,
            Err(e) => {
                eprintln!("TS Query Error: {}", e);
                return vec![];
            }
        };
        
        let mut cursor = QueryCursor::new();
        let text_bytes = text.as_bytes();

        let mut results = Vec::new();
        let mut matches = cursor.matches(&query, tree.root_node(), text_bytes);
        while let Some(m) = matches.next() {
            let mut captures = Vec::new();
            for c in m.captures {
                let name = query.capture_names()[c.index as usize].to_string();
                let node_text = text.get(c.node.byte_range())
                    .unwrap_or("")
                    .to_string();
                captures.push((name, node_text));
            }
            results.push(captures);
        }
        results
    });

    // (execute-rust-tool id name input) - execute a tool from the registry
    let actions_clone = actions.clone();
    engine.register_fn("execute-rust-tool", move |id: String, name: String, input: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::ExecuteTool { id, name, input });
        }
    });

    // minibuffer-activate! - tells Rust minibuffer is now active in generic mode
    let actions_clone = actions.clone();
    engine.register_fn("minibuffer-activate!", move |prompt: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::MinibufferActivate { prompt });
        }
    });

    // (global-set-key key command) - bind key to command
    let actions_clone = actions.clone();
    engine.register_fn("global-set-key", move |key: String, command: String| {
        if let Ok(mut queue) = actions_clone.lock() {
            queue.push(Action::GlobalSetKey { key, command });
        }
    });

    // (getenv name) - get environment variable
    engine.register_fn("getenv", |name: String| -> String {
        std::env::var(&name).unwrap_or_default()
    });
}

/// List files in directory (excluding hidden)
fn scheme_ls(dir: String) -> Vec<String> {
    let path = PathBuf::from(&dir);
    list_dir(&path, false)
}

/// List all files including hidden
fn scheme_ls_all(dir: String) -> Vec<String> {
    let path = PathBuf::from(&dir);
    list_dir(&path, true)
}

fn list_dir(path: &PathBuf, include_hidden: bool) -> Vec<String> {
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden unless requested
            if !include_hidden && name.starts_with('.') {
                continue;
            }

            if entry.path().is_dir() {
                files.push(format!("{}/", name));
            } else {
                files.push(name);
            }
        }
    }

    files.sort();
    files
}

/// Check if file exists
fn scheme_file_exists(path: String) -> bool {
    PathBuf::from(&path).exists()
}

/// Check if path is a directory
fn scheme_is_directory(path: String) -> bool {
    PathBuf::from(&path).is_dir()
}

/// Expand ~ and make path absolute
fn scheme_expand_path(path: String, base_dir: String) -> String {
    let expanded = crate::minibuffer::path::expand_tilde(&path);
    let p = PathBuf::from(&expanded);

    if p.is_absolute() {
        expanded
    } else {
        PathBuf::from(&base_dir)
            .join(&expanded)
            .to_string_lossy()
            .to_string()
    }
}

/// Fuzzy match pattern against candidates, return sorted matches
fn scheme_fuzzy_match(pattern: String, candidates: Vec<String>) -> Vec<String> {
    use nucleo::pattern::{Pattern, CaseMatching, Normalization, AtomKind};
    use nucleo::Utf32String;
    use nucleo_matcher::{Matcher, Config};

    if pattern.is_empty() {
        return candidates;
    }

    let pat = Pattern::new(
        &pattern,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut results: Vec<(String, u32)> = Vec::new();

    for candidate in candidates {
        let haystack = Utf32String::from(candidate.as_str());
        if let Some(score) = pat.score(haystack.slice(..), &mut matcher) {
            results.push((candidate, score));
        }
    }

    // Sort by score descending
    results.sort_by(|a, b| b.1.cmp(&a.1));

    results.into_iter().map(|(s, _)| s).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpreter() {
        let mut interp = Interpreter::new();

        // Basic arithmetic
        let result = interp.run("(+ 1 2)").unwrap();
        assert_eq!(result, SteelVal::IntV(3));
    }

    #[test]
    fn test_ls() {
        let mut interp = Interpreter::new();

        // List current directory
        let result = interp.run("(ls \".\")").unwrap();
        if let SteelVal::ListV(files) = result {
            assert!(!files.is_empty());
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn test_fuzzy_match() {
        let mut interp = Interpreter::new();

        let result = interp.run(r#"
            (fuzzy-match "ff" '("find-file" "forward-char" "save-buffer"))
        "#).unwrap();

        if let SteelVal::ListV(matches) = result {
            // "find-file" should be first (best match for "ff")
            assert!(!matches.is_empty());
        }
    }

    #[test]
    fn test_string_functions() {
        let mut interp = Interpreter::new();

        // Test substring
        let result = interp.run(r#"(substring "hello" 1 3)"#).unwrap();
        assert_eq!(result, SteelVal::StringV("el".into()));

        // Test string-length
        let result = interp.run(r#"(string-length "hello")"#).unwrap();
        assert_eq!(result, SteelVal::IntV(5));

        // Test string-append
        let result = interp.run(r#"(string-append "hello" " " "world")"#).unwrap();
        assert_eq!(result, SteelVal::StringV("hello world".into()));
    }

    #[test]
    fn test_load_completion_module() {
        let mut interp = Interpreter::new();

        // Load completion module
        let completion_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../scheme/completion.scm");
        let result = interp.load_file(completion_path);
        println!("Load result: {:?}", result);

        if let Err(e) = result {
            panic!("Failed to load completion.scm: {}", e);
        }

        // Test the file-completion-names function
        let cwd = env!("CARGO_MANIFEST_DIR");
        let code = format!(r#"(file-completion-names "" "{}")"#, cwd);
        let result = interp.run(&code);
        println!("file-completion-names result: {:?}", result);

        if let Err(e) = &result {
            panic!("file-completion-names failed: {}", e);
        }

        // Test with a prefix
        let code = format!(r#"(file-completion-names "s" "{}")"#, cwd);
        let result = interp.run(&code);
        println!("file-completion-names 's' result: {:?}", result);

        // Test subdirectory - this was the bug!
        let code = format!(r#"(file-completion-names "src/" "{}")"#, cwd);
        let result = interp.run(&code);
        println!("file-completion-names 'src/' result: {:?}", result);

        // Test subdirectory with prefix
        let code = format!(r#"(file-completion-names "src/l" "{}")"#, cwd);
        let result = interp.run(&code);
        println!("file-completion-names 'src/l' result: {:?}", result);
    }

    #[test]
    fn test_with_core_integration() {
        // This mimics exactly what main.rs does
        let mut interp = Interpreter::with_core();

        // Test that core scheme loaded
        let result = interp.run("(+ 1 2)");
        println!("Basic math: {:?}", result);
        assert!(result.is_ok());

        // Test file-completion-names exists and works
        let cwd = env!("CARGO_MANIFEST_DIR");
        let parent = std::path::Path::new(cwd).parent().unwrap();
        let project_root = parent.to_string_lossy();

        // Empty input - should list project root files
        let code = format!(r#"(file-completion-names "" "{}")"#, project_root);
        println!("Code: {}", code);
        let result = interp.run(&code);
        println!("Empty input result: {:?}", result);
        assert!(result.is_ok());

        if let Ok(SteelVal::ListV(items)) = &result {
            println!("Got {} items", items.len());
            for item in items.iter().take(5) {
                println!("  - {:?}", item);
            }
            assert!(!items.is_empty(), "Should have files");
        }

        // Subdirectory - should list core/ contents
        let code = format!(r#"(file-completion-names "core/" "{}")"#, project_root);
        println!("Code: {}", code);
        let result = interp.run(&code);
        println!("core/ result: {:?}", result);
        assert!(result.is_ok());

        if let Ok(SteelVal::ListV(items)) = &result {
            println!("Got {} items in core/", items.len());
            assert!(!items.is_empty(), "core/ should have files");
        }
    }

    #[test]
    fn test_completion_from_project_root() {
        let mut interp = Interpreter::new();

        // Load completion module
        let completion_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../scheme/completion.scm");
        interp.load_file(completion_path).unwrap();

        // Simulate being in the project root (not core/)
        let project_root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

        // Test: type "core/" - should show files in core directory
        let code = format!(r#"(file-completion-names "core/" "{}")"#, project_root);
        let result = interp.run(&code);
        println!("From project root, 'core/' result: {:?}", result);

        if let Ok(SteelVal::ListV(items)) = &result {
            assert!(!items.is_empty(), "Should have files in core/");
            // Should contain src/ directory
            let has_src = items.iter().any(|x| {
                if let SteelVal::StringV(s) = x {
                    s.as_str() == "src/"
                } else {
                    false
                }
            });
            assert!(has_src, "core/ should contain src/ directory");
        } else {
            panic!("Expected list, got: {:?}", result);
        }

        // Test: type "core/src/" - should show files in core/src directory
        let code = format!(r#"(file-completion-names "core/src/" "{}")"#, project_root);
        let result = interp.run(&code);
        println!("From project root, 'core/src/' result: {:?}", result);

        if let Ok(SteelVal::ListV(items)) = &result {
            assert!(!items.is_empty(), "Should have files in core/src/");
        } else {
            panic!("Expected list, got: {:?}", result);
        }
    }

    #[test]
    fn test_chat_message_history() {
        let mut interp = Interpreter::with_core();

        // Initially, chat history should be empty
        let result = interp.run("(chat-message-count)").unwrap();
        assert_eq!(result, SteelVal::IntV(0), "History should start empty");

        // Add a user message
        let result = interp.run(r#"(chat-add-message "user" "Hello")"#);
        assert!(result.is_ok(), "Should be able to add message");

        let result = interp.run("(chat-message-count)").unwrap();
        assert_eq!(result, SteelVal::IntV(1), "Should have 1 message");

        // Add an assistant message (simulates what chat-on-response-complete does)
        let result = interp.run(r#"(chat-on-response-complete "Hi there!")"#);
        assert!(result.is_ok(), "Should be able to add response");

        let result = interp.run("(chat-message-count)").unwrap();
        assert_eq!(result, SteelVal::IntV(2), "Should have 2 messages");

        // Clear history
        let result = interp.run("(chat-clear)");
        assert!(result.is_ok(), "Should be able to clear");

        let result = interp.run("(chat-message-count)").unwrap();
        assert_eq!(result, SteelVal::IntV(0), "History should be empty after clear");
    }

    #[test]
    fn test_tool_result_handling() {
        let mut interp = Interpreter::with_core();

        // Initially, chat history should be empty
        let result = interp.run("(chat-message-count)").unwrap();
        assert_eq!(result, SteelVal::IntV(0), "History should start empty");

        // Simulate adding a tool result (what happens after tool execution)
        let result = interp.run(r#"(chat-add-tool-result "call_123" "success" "File contents here")"#);
        assert!(result.is_ok(), "Should handle tool result callback");

        // Should have added a tool_result message to history
        let result = interp.run("(chat-message-count)").unwrap();
        assert_eq!(result, SteelVal::IntV(1), "Should have 1 message after tool result");

        // Add an error result
        let result = interp.run(r#"(chat-add-tool-result "call_456" "error" "File not found")"#);
        assert!(result.is_ok(), "Should handle error tool result");

        let result = interp.run("(chat-message-count)").unwrap();
        assert_eq!(result, SteelVal::IntV(2), "Should have 2 messages");

        // Clear history
        let result = interp.run("(chat-clear)");
        assert!(result.is_ok(), "Should be able to clear chat");

        let result = interp.run("(chat-message-count)").unwrap();
        assert_eq!(result, SteelVal::IntV(0), "History should be empty after clear");
    }
}
