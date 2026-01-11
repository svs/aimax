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

use std::sync::{Arc, Mutex, RwLock};

/// Action requested by Scheme to be performed on the Editor
#[derive(Debug, Clone)]
pub enum Action {
    Insert(String),
    Delete,
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
    KeyboardQuit,
    Newline,
    // Face system
    SetFaceAttribute { face: String, key: String, value: String },
}

/// Shared state for Scheme to query buffer contents
/// This is synced from Editor before Scheme execution
#[derive(Debug, Default)]
pub struct SharedState {
    pub buffer_text: String,
    pub buffer_lines: Vec<String>,
    pub buffer_file_paths: Vec<String>,  // All open buffer file paths
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
            // Load completion.scm first (minibuffer depends on it)
            let completion_path = dir.join("completion.scm");
            if completion_path.exists() {
                if let Ok(code) = std::fs::read_to_string(&completion_path) {
                    if let Err(e) = interp.run(&code) {
                        eprintln!("Warning: Failed to load {}: {}", completion_path.display(), e);
                    }
                }
            }

            // Load minibuffer.scm
            let minibuffer_path = dir.join("minibuffer.scm");
            if minibuffer_path.exists() {
                if let Ok(code) = std::fs::read_to_string(&minibuffer_path) {
                    if let Err(e) = interp.run(&code) {
                        eprintln!("Warning: Failed to load {}: {}", minibuffer_path.display(), e);
                    }
                }
            }

            // Load commands.scm - command definitions
            let commands_path = dir.join("commands.scm");
            if commands_path.exists() {
                if let Ok(code) = std::fs::read_to_string(&commands_path) {
                    if let Err(e) = interp.run(&code) {
                        eprintln!("Warning: Failed to load {}: {}", commands_path.display(), e);
                    }
                }
            }

            // Load desktop.scm - save/restore buffers
            let desktop_path = dir.join("desktop.scm");
            if desktop_path.exists() {
                if let Ok(code) = std::fs::read_to_string(&desktop_path) {
                    if let Err(e) = interp.run(&code) {
                        eprintln!("Warning: Failed to load {}: {}", desktop_path.display(), e);
                    }
                }
            }
        } else {
            // Fall back to embedded code
            if let Err(e) = interp.run(CORE_SCHEME) {
                eprintln!("Warning: Failed to load core Scheme modules: {}", e);
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
        let code = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path, e))?;
        self.run(&code)?;
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
}
