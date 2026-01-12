//! Keymap system - Emacs-style key bindings
//!
//! Supports:
//! - Key sequences (C-x C-f, C-x b)
//! - Prefix keys (C-x waits for next key)
//! - Keymap stacking (minor mode > major mode > global)
//! - Modal keymaps

use std::collections::HashMap;

/// A single key event (normalized string)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(pub String);

impl Key {
    /// Create a key from modifier flags and key code
    pub fn new(ctrl: bool, alt: bool, code: &str) -> Self {
        let mut s = String::new();
        if ctrl { s.push_str("C-"); }
        if alt { s.push_str("M-"); }
        s.push_str(code);
        Key(s)
    }

    /// Parse a key string like "C-f" or "M-x"
    pub fn parse(s: &str) -> Self {
        Key(s.to_string())
    }

    /// Get the string representation
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Result of looking up a key in a keymap
#[derive(Debug, Clone)]
pub enum KeyLookup {
    /// Key maps to a command
    Command(String),
    /// Key is a prefix - more keys needed
    Prefix,
    /// Key not bound
    Unbound,
}

/// A keymap node - either a command or a prefix map
#[derive(Debug, Clone)]
enum KeymapNode {
    Command(String),
    Prefix(HashMap<Key, KeymapNode>),
}

/// A keymap - mapping from key sequences to commands
#[derive(Debug, Clone)]
pub struct Keymap {
    name: String,
    root: HashMap<Key, KeymapNode>,
}

impl Keymap {
    pub fn new(name: &str) -> Self {
        Keymap {
            name: name.to_string(),
            root: HashMap::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Bind a key sequence to a command
    /// Keys are space-separated: "C-x C-f" or just "C-f"
    pub fn bind(&mut self, keys: &str, command: &str) {
        let key_seq: Vec<Key> = keys.split_whitespace()
            .map(Key::parse)
            .collect();

        if key_seq.is_empty() {
            return;
        }

        self.bind_sequence(&key_seq, command);
    }

    fn bind_sequence(&mut self, keys: &[Key], command: &str) {
        if keys.len() == 1 {
            // Single key - direct binding
            self.root.insert(keys[0].clone(), KeymapNode::Command(command.to_string()));
        } else {
            // Multiple keys - need prefix structure
            let first = &keys[0];
            let rest = &keys[1..];

            // Get or create prefix node
            let node = self.root.entry(first.clone()).or_insert_with(|| {
                KeymapNode::Prefix(HashMap::new())
            });

            // Navigate/create prefix chain
            Self::bind_in_node(node, rest, command);
        }
    }

    fn bind_in_node(node: &mut KeymapNode, keys: &[Key], command: &str) {
        match node {
            KeymapNode::Command(_) => {
                // Overwrite command with prefix - this key is now a prefix
                let mut map = HashMap::new();
                Self::insert_into_map(&mut map, keys, command);
                *node = KeymapNode::Prefix(map);
            }
            KeymapNode::Prefix(map) => {
                Self::insert_into_map(map, keys, command);
            }
        }
    }

    fn insert_into_map(map: &mut HashMap<Key, KeymapNode>, keys: &[Key], command: &str) {
        if keys.len() == 1 {
            map.insert(keys[0].clone(), KeymapNode::Command(command.to_string()));
        } else {
            let first = &keys[0];
            let rest = &keys[1..];
            let node = map.entry(first.clone()).or_insert_with(|| {
                KeymapNode::Prefix(HashMap::new())
            });
            Self::bind_in_node(node, rest, command);
        }
    }

    /// Look up a single key (used during key sequence accumulation)
    pub fn lookup_single(&self, key: &Key) -> KeyLookup {
        match self.root.get(key) {
            Some(KeymapNode::Command(cmd)) => KeyLookup::Command(cmd.clone()),
            Some(KeymapNode::Prefix(_)) => KeyLookup::Prefix,
            None => KeyLookup::Unbound,
        }
    }

    /// Look up within a prefix (after first key of sequence)
    pub fn lookup_in_prefix(&self, prefix_keys: &[Key], key: &Key) -> KeyLookup {
        let mut current = &self.root;

        // Navigate to the prefix node
        for pk in prefix_keys {
            match current.get(pk) {
                Some(KeymapNode::Prefix(map)) => current = map,
                _ => return KeyLookup::Unbound,
            }
        }

        // Look up the final key
        match current.get(key) {
            Some(KeymapNode::Command(cmd)) => KeyLookup::Command(cmd.clone()),
            Some(KeymapNode::Prefix(_)) => KeyLookup::Prefix,
            None => KeyLookup::Unbound,
        }
    }

    /// Create the default global keymap with Emacs bindings
    pub fn default_global() -> Self {
        let mut km = Keymap::new("global");

        // === Movement - Emacs style ===
        km.bind("C-f", "forward-char");
        km.bind("C-b", "backward-char");
        km.bind("C-n", "next-line");
        km.bind("C-p", "previous-line");
        km.bind("C-a", "beginning-of-line");
        km.bind("C-e", "end-of-line");
        km.bind("M-<", "beginning-of-buffer");
        km.bind("M->", "end-of-buffer");
        km.bind("C-v", "scroll-down");
        km.bind("M-v", "scroll-up");

        // === Movement - Arrow keys ===
        km.bind("right", "forward-char");
        km.bind("left", "backward-char");
        km.bind("down", "next-line");
        km.bind("up", "previous-line");
        km.bind("home", "beginning-of-line");
        km.bind("end", "end-of-line");
        km.bind("pagedown", "scroll-down");
        km.bind("pageup", "scroll-up");

        // === Editing ===
        km.bind("backspace", "delete-backward-char");
        km.bind("C-d", "delete-forward-char");
        km.bind("delete", "delete-forward-char");
        km.bind("return", "newline");
        km.bind("tab", "indent");
        km.bind("C-k", "kill-line");
        km.bind("C-y", "yank");
        km.bind("M-w", "copy-region");
        km.bind("C-w", "kill-region");

        // === C-x prefix commands ===
        km.bind("C-x C-f", "find-file");
        km.bind("C-x C-s", "save-buffer");
        km.bind("C-x C-w", "write-file");
        km.bind("C-x b", "switch-buffer");
        km.bind("C-x k", "kill-buffer");
        km.bind("C-x C-c", "quit");
        km.bind("C-x 0", "delete-window");
        km.bind("C-x 1", "delete-other-windows");
        km.bind("C-x 2", "split-window-below");
        km.bind("C-x 3", "split-window-right");
        km.bind("C-x o", "other-window");

        // === M-x and help ===
        km.bind("M-x", "execute-extended-command");
        km.bind("escape x", "execute-extended-command");  // Escape as Meta prefix
        km.bind("C-h k", "describe-key");
        km.bind("C-h f", "describe-function");

        // === Search ===
        km.bind("C-s", "isearch-forward");
        km.bind("C-r", "isearch-backward");

        // === Undo ===
        km.bind("C-/", "undo");
        km.bind("C-_", "undo");

        // === Cancel ===
        km.bind("C-g", "keyboard-quit");
        km.bind("escape escape", "keyboard-quit");  // Double-escape to quit

        // === Fallback for terminal issues ===
        km.bind("F2", "save-buffer");

        // === AI ===
        km.bind("C-x a", "ask-ai");

        km
    }
}

/// Mode types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeType {
    Major,
    Minor,
}

/// A mode with its keymap
#[derive(Debug, Clone)]
pub struct Mode {
    pub name: String,
    pub mode_type: ModeType,
    pub keymap: Keymap,
}

impl Mode {
    pub fn new(name: &str, mode_type: ModeType) -> Self {
        Mode {
            name: name.to_string(),
            mode_type,
            keymap: Keymap::new(name),
        }
    }
}

/// Keymap stack with key sequence state
pub struct KeymapStack {
    global: Keymap,
    major_mode: Option<Mode>,
    minor_modes: Vec<Mode>,
    /// Accumulated prefix keys
    pending_keys: Vec<Key>,
}

impl KeymapStack {
    pub fn new() -> Self {
        KeymapStack {
            global: Keymap::default_global(),
            major_mode: None,
            minor_modes: Vec::new(),
            pending_keys: Vec::new(),
        }
    }

    /// Set the major mode
    pub fn set_major_mode(&mut self, mode: Mode) {
        self.major_mode = Some(mode);
    }

    /// Add a minor mode
    pub fn add_minor_mode(&mut self, mode: Mode) {
        self.minor_modes.push(mode);
    }

    /// Remove a minor mode by name
    pub fn remove_minor_mode(&mut self, name: &str) {
        self.minor_modes.retain(|m| m.name != name);
    }

    /// Clear pending key sequence (on C-g or timeout)
    pub fn clear_pending(&mut self) {
        self.pending_keys.clear();
    }

    /// Check if we're in the middle of a key sequence
    pub fn has_pending(&self) -> bool {
        !self.pending_keys.is_empty()
    }

    /// Get pending keys as string for display
    pub fn pending_display(&self) -> String {
        self.pending_keys.iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Process a key press, return command if sequence is complete
    pub fn process_key(&mut self, key: &Key) -> KeyLookup {
        // Check keymaps in order: minor modes (reverse), major mode, global
        let keymaps: Vec<&Keymap> = self.minor_modes.iter().rev()
            .map(|m| &m.keymap)
            .chain(self.major_mode.as_ref().map(|m| &m.keymap))
            .chain(std::iter::once(&self.global))
            .collect();

        // Special case: C-g always cancels
        if key.as_str() == "C-g" || key.as_str() == "escape" {
            if self.has_pending() {
                self.clear_pending();
                return KeyLookup::Command("keyboard-quit".to_string());
            }
        }

        for keymap in keymaps {
            let result = if self.pending_keys.is_empty() {
                keymap.lookup_single(key)
            } else {
                keymap.lookup_in_prefix(&self.pending_keys, key)
            };

            match result {
                KeyLookup::Command(cmd) => {
                    self.clear_pending();
                    return KeyLookup::Command(cmd);
                }
                KeyLookup::Prefix => {
                    self.pending_keys.push(key.clone());
                    return KeyLookup::Prefix;
                }
                KeyLookup::Unbound => continue,
            }
        }

        // Key not found in any keymap
        if self.has_pending() {
            // Invalid sequence - clear and report unbound
            self.clear_pending();
        }
        KeyLookup::Unbound
    }

    /// Direct lookup without modifying state (for introspection)
    pub fn lookup(&self, key: &Key) -> Option<&str> {
        // For backwards compatibility - simple single-key lookup
        let keymaps: Vec<&Keymap> = self.minor_modes.iter().rev()
            .map(|m| &m.keymap)
            .chain(self.major_mode.as_ref().map(|m| &m.keymap))
            .chain(std::iter::once(&self.global))
            .collect();

        for keymap in keymaps {
            if let KeyLookup::Command(cmd) = keymap.lookup_single(key) {
                // This is a bit ugly but needed for the signature
                // In practice, use process_key for real input handling
                return Some(Box::leak(cmd.into_boxed_str()));
            }
        }
        None
    }

    /// Get the global keymap for modification
    pub fn global_mut(&mut self) -> &mut Keymap {
        &mut self.global
    }
}

impl Default for KeymapStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_key_binding() {
        let mut km = Keymap::new("test");
        km.bind("C-f", "forward-char");

        assert!(matches!(
            km.lookup_single(&Key::parse("C-f")),
            KeyLookup::Command(cmd) if cmd == "forward-char"
        ));
    }

    #[test]
    fn test_key_sequence() {
        let mut km = Keymap::new("test");
        km.bind("C-x C-f", "find-file");
        km.bind("C-x C-s", "save-buffer");

        // C-x should be a prefix
        assert!(matches!(km.lookup_single(&Key::parse("C-x")), KeyLookup::Prefix));

        // C-x C-f should find the command
        assert!(matches!(
            km.lookup_in_prefix(&[Key::parse("C-x")], &Key::parse("C-f")),
            KeyLookup::Command(cmd) if cmd == "find-file"
        ));
    }

    #[test]
    fn test_keymap_stack_sequence() {
        let mut stack = KeymapStack::new();

        // Press C-x
        let result = stack.process_key(&Key::parse("C-x"));
        assert!(matches!(result, KeyLookup::Prefix));
        assert!(stack.has_pending());

        // Press C-f
        let result = stack.process_key(&Key::parse("C-f"));
        assert!(matches!(result, KeyLookup::Command(cmd) if cmd == "find-file"));
        assert!(!stack.has_pending());
    }
}
