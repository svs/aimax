//! Minibuffer - Emacs-style prompt and completion
//!
//! The minibuffer handles:
//! - Prompts (find-file, M-x, search)
//! - Completion UI (Vertico-style vertical list)
//! - History
//! - Path expansion (~, environment variables)

use crate::completion::{CompletionSource, Completer, Match};

/// Minibuffer state
pub struct Minibuffer {
    /// Current input text
    pub input: String,
    /// Cursor position in input
    pub point: usize,
    /// The prompt string (e.g., "Find file: ")
    pub prompt: String,
    /// Currently active completion source
    completer: Option<Completer>,
    /// Filtered matches
    matches: Vec<Match>,
    /// Selected match index
    pub selected: usize,
    /// History for this prompt type
    history: Vec<String>,
    /// History navigation position
    history_pos: Option<usize>,
    /// Callback to invoke on completion
    on_complete: Option<String>, // Command name to call with result
}

impl Minibuffer {
    pub fn new() -> Self {
        Minibuffer {
            input: String::new(),
            point: 0,
            prompt: String::new(),
            completer: None,
            matches: Vec::new(),
            selected: 0,
            history: Vec::new(),
            history_pos: None,
            on_complete: None,
        }
    }

    /// Start a new prompt
    pub fn prompt(&mut self, prompt: &str, source: CompletionSource, on_complete: &str) {
        self.input.clear();
        self.point = 0;
        self.prompt = prompt.to_string();
        self.completer = Some(Completer::new(source));
        self.matches.clear();
        self.selected = 0;
        self.history_pos = None;
        self.on_complete = Some(on_complete.to_string());

        // Initial completion with empty input
        self.update_completions();
    }

    /// Clear/cancel the minibuffer
    pub fn clear(&mut self) {
        self.input.clear();
        self.point = 0;
        self.prompt.clear();
        self.completer = None;
        self.matches.clear();
        self.selected = 0;
        self.on_complete = None;
    }

    /// Check if minibuffer is active
    pub fn is_active(&self) -> bool {
        !self.prompt.is_empty()
    }

    /// Get the command to call on completion
    pub fn on_complete_command(&self) -> Option<&str> {
        self.on_complete.as_deref()
    }

    /// Insert a character at point
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.point, c);
        self.point += c.len_utf8();
        self.update_completions();
    }

    /// Delete character before point
    pub fn delete_backward(&mut self) {
        if self.point > 0 {
            // Find the previous char boundary
            let prev = self.input[..self.point]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev);
            self.point = prev;
            self.update_completions();
        }
    }

    /// Delete character at point
    pub fn delete_forward(&mut self) {
        if self.point < self.input.len() {
            self.input.remove(self.point);
            self.update_completions();
        }
    }

    /// Move point forward
    pub fn forward_char(&mut self) {
        if self.point < self.input.len() {
            self.point += self.input[self.point..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
        }
    }

    /// Move point backward
    pub fn backward_char(&mut self) {
        if self.point > 0 {
            self.point = self.input[..self.point]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move to beginning of input
    pub fn beginning_of_line(&mut self) {
        self.point = 0;
    }

    /// Move to end of input
    pub fn end_of_line(&mut self) {
        self.point = self.input.len();
    }

    /// Select next completion
    pub fn next_completion(&mut self) {
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
        }
    }

    /// Select previous completion
    pub fn prev_completion(&mut self) {
        if !self.matches.is_empty() {
            self.selected = if self.selected == 0 {
                self.matches.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    /// Navigate to previous history item
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let new_pos = match self.history_pos {
            None => Some(self.history.len() - 1),
            Some(0) => Some(0),
            Some(n) => Some(n - 1),
        };

        if let Some(pos) = new_pos {
            self.history_pos = Some(pos);
            self.input = self.history[pos].clone();
            self.point = self.input.len();
            self.update_completions();
        }
    }

    /// Navigate to next history item
    pub fn history_next(&mut self) {
        if let Some(pos) = self.history_pos {
            if pos + 1 < self.history.len() {
                self.history_pos = Some(pos + 1);
                self.input = self.history[pos + 1].clone();
            } else {
                self.history_pos = None;
                self.input.clear();
            }
            self.point = self.input.len();
            self.update_completions();
        }
    }

    /// Get the current value (selected completion or input)
    pub fn current_value(&self) -> String {
        if !self.matches.is_empty() && self.selected < self.matches.len() {
            self.matches[self.selected].text.clone()
        } else {
            self.input.clone()
        }
    }

    /// Confirm selection and add to history
    pub fn confirm(&mut self) -> String {
        let value = self.current_value();

        // Add to history if not empty and not duplicate
        if !value.is_empty() {
            if self.history.last() != Some(&value) {
                self.history.push(value.clone());
            }
        }

        value
    }

    /// Get the current matches for display
    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    /// Get selected index
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Update completions based on current input
    fn update_completions(&mut self) {
        if let Some(ref mut completer) = self.completer {
            self.matches = completer.complete(&self.input);
            // Reset selection if it's out of bounds
            if self.selected >= self.matches.len() {
                self.selected = 0;
            }
        }
    }

    /// Set candidates directly (for dynamic sources)
    pub fn set_candidates(&mut self, candidates: Vec<String>) {
        if let Some(ref mut completer) = self.completer {
            completer.set_candidates(candidates);
            self.update_completions();
        }
    }

    /// Set pre-filtered matches directly (bypass internal fuzzy filtering)
    /// Use this when external code handles the filtering (e.g., file completion)
    pub fn set_matches(&mut self, matches: Vec<Match>) {
        self.matches = matches;
        if self.selected >= self.matches.len() {
            self.selected = 0;
        }
    }
}

impl Default for Minibuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Path parsing utilities for file prompts
pub mod path {
    use std::path::PathBuf;

    /// Expand ~ to home directory
    pub fn expand_tilde(path: &str) -> String {
        if path == "~" {
            dirs::home_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "~".to_string())
        } else if let Some(rest) = path.strip_prefix("~/") {
            dirs::home_dir()
                .map(|p| format!("{}/{}", p.to_string_lossy(), rest))
                .unwrap_or_else(|| path.to_string())
        } else {
            path.to_string()
        }
    }

    /// Parse a file path input into directory and filename prefix
    /// Returns (directory_to_list, filename_prefix)
    pub fn parse_file_input(input: &str, base_dir: &PathBuf) -> (PathBuf, String) {
        // First expand tilde
        let expanded = expand_tilde(input);

        // Handle empty input
        if expanded.is_empty() {
            return (base_dir.clone(), String::new());
        }

        let path = PathBuf::from(&expanded);

        // If ends with /, list that directory with empty prefix
        if expanded.ends_with('/') {
            let dir = if path.is_absolute() {
                path
            } else {
                base_dir.join(path)
            };
            return (dir, String::new());
        }

        // Otherwise, split into parent directory and filename prefix
        if let Some(parent) = path.parent() {
            let dir = if parent.as_os_str().is_empty() {
                // No parent means just a filename in current directory
                base_dir.clone()
            } else if parent.is_absolute() || expanded.starts_with('/') {
                parent.to_path_buf()
            } else {
                base_dir.join(parent)
            };

            let prefix = path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();

            (dir, prefix)
        } else {
            // Just a filename, complete in base directory
            (base_dir.clone(), expanded)
        }
    }

    /// Resolve a path input to an absolute path
    pub fn resolve(input: &str, base_dir: &PathBuf) -> PathBuf {
        let expanded = expand_tilde(input);
        let path = PathBuf::from(&expanded);

        if path.is_absolute() {
            path
        } else {
            base_dir.join(path)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_parse_file_input_subdir() {
            let cwd = std::env::current_dir().unwrap();
            eprintln!("cwd: {:?}", cwd);

            // Test: user typed "src/"
            let (dir, prefix) = parse_file_input("src/", &cwd);
            eprintln!("input='src/' -> dir={:?} prefix={:?}", dir, prefix);

            // dir should be cwd/src, prefix should be empty
            assert_eq!(dir, cwd.join("src"), "dir should be cwd/src");
            assert_eq!(prefix, "", "prefix should be empty for 'src/'");

            // Test: user typed "src/b" (partial filename in subdir)
            let (dir2, prefix2) = parse_file_input("src/b", &cwd);
            eprintln!("input='src/b' -> dir={:?} prefix={:?}", dir2, prefix2);

            assert_eq!(dir2, cwd.join("src"), "dir should be cwd/src");
            assert_eq!(prefix2, "b", "prefix should be 'b'");
        }
    }
}
