//! Syntax - Tree-sitter highlighting using the official crate
//!
//! Uses tree-sitter-highlight for battle-tested syntax highlighting.
//! Queries come from the grammar crates themselves - no hardcoding.
//! Colors come from the Face system - fully customizable from Scheme.

use std::collections::HashMap;
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};
use crate::face::{FaceRegistry, ScopeMap};

/// Standard highlight names recognized by tree-sitter-highlight
pub const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "function",
    "function.builtin",
    "keyword",
    "module",
    "number",
    "operator",
    "property",
    "property.builtin",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

/// Supported languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Rust,
    JavaScript,
    Json,
    Markdown,
    Html,
    Bash,
    Scheme,
    Plain,
}

impl Lang {
    /// Detect language from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Lang::Rust,
            "js" | "mjs" | "cjs" | "jsx" => Lang::JavaScript,
            "ts" | "tsx" => Lang::JavaScript,
            "json" => Lang::Json,
            "md" | "markdown" => Lang::Markdown,
            "html" | "htm" => Lang::Html,
            "sh" | "bash" | "zsh" => Lang::Bash,
            "scm" | "ss" | "rkt" => Lang::Scheme,
            _ => Lang::Plain,
        }
    }

    /// Detect from file path
    pub fn from_path(path: &std::path::Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .map(Self::from_extension)
            .unwrap_or(Lang::Plain)
    }

    /// Get Tree-sitter language for this Lang
    pub fn tree_sitter_language(&self) -> Option<tree_sitter::Language> {
        match self {
            Lang::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
            Lang::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
            Lang::Json => Some(tree_sitter_json::LANGUAGE.into()),
            Lang::Markdown => Some(tree_sitter_md::LANGUAGE.into()),
            Lang::Html => Some(tree_sitter_html::LANGUAGE.into()),
            Lang::Bash => Some(tree_sitter_bash::LANGUAGE.into()),
            Lang::Scheme => Some(tree_sitter_scheme::LANGUAGE.into()),
            Lang::Plain => None,
        }
    }
}

/// A highlighted span in the text
#[derive(Debug, Clone)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub highlight_index: usize,
}

/// Map highlight index to RGB color
pub fn highlight_color(index: usize) -> (u8, u8, u8) {
    match HIGHLIGHT_NAMES.get(index) {
        Some(&"keyword") => (198, 120, 221),           // Purple
        Some(&"type") | Some(&"type.builtin") => (229, 192, 123), // Yellow
        Some(&"function") | Some(&"function.builtin") => (97, 175, 239), // Blue
        Some(&"variable") | Some(&"variable.builtin") | Some(&"variable.parameter") => (224, 108, 117), // Red
        Some(&"string") | Some(&"string.special") => (152, 195, 121), // Green
        Some(&"number") => (209, 154, 102),            // Orange
        Some(&"comment") => (92, 99, 112),             // Gray
        Some(&"operator") => (86, 182, 194),           // Cyan
        Some(&"punctuation") | Some(&"punctuation.bracket") | Some(&"punctuation.delimiter") | Some(&"punctuation.special") => (171, 178, 191), // Light gray
        Some(&"property") | Some(&"property.builtin") => (224, 108, 117), // Red
        Some(&"constant") | Some(&"constant.builtin") => (209, 154, 102), // Orange
        Some(&"attribute") => (198, 120, 221),         // Purple
        Some(&"constructor") => (229, 192, 123),       // Yellow
        Some(&"module") => (97, 175, 239),             // Blue
        Some(&"tag") => (224, 108, 117),               // Red
        Some(&"embedded") => (171, 178, 191),          // Light gray
        _ => (171, 178, 191),                          // Default
    }
}

/// Syntax highlighter that manages language configurations
pub struct SyntaxHighlighter {
    highlighter: Highlighter,
    configs: HashMap<Lang, HighlightConfiguration>,
    pub faces: FaceRegistry,
    scope_map: ScopeMap,
    // Cached colors for O(1) lookup by highlight index
    color_cache: Vec<(u8, u8, u8)>,
    cache_version: u64,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        let mut sh = SyntaxHighlighter {
            highlighter: Highlighter::new(),
            configs: HashMap::new(),
            faces: FaceRegistry::new(),
            scope_map: ScopeMap::new(),
            color_cache: Vec::new(),
            cache_version: 0,
        };
        sh.init_languages();
        sh.rebuild_color_cache();
        sh
    }

    /// Rebuild color cache from faces (call after face changes)
    fn rebuild_color_cache(&mut self) {
        self.color_cache.clear();
        for scope in HIGHLIGHT_NAMES {
            let face_name = self.scope_map.get_face(scope);
            let attrs = self.faces.get_or_default(face_name);
            let color = attrs.fg.map(|c| (c.r, c.g, c.b)).unwrap_or((171, 178, 191));
            self.color_cache.push(color);
        }
        self.cache_version = self.faces.version;
    }

    /// Get color for a highlight index - O(1) array lookup
    fn face_color(&mut self, index: usize) -> (u8, u8, u8) {
        // Rebuild cache if faces changed
        if self.cache_version != self.faces.version {
            self.rebuild_color_cache();
        }
        self.color_cache.get(index).copied().unwrap_or((171, 178, 191))
    }

    fn init_languages(&mut self) {
        // Rust
        match HighlightConfiguration::new(
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        ) {
            Ok(mut config) => {
                config.configure(HIGHLIGHT_NAMES);
                self.configs.insert(Lang::Rust, config);
            }
            Err(e) => {
                eprintln!("Failed to create Rust highlight config: {:?}", e);
            }
        }

        // JavaScript
        match HighlightConfiguration::new(
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ) {
            Ok(mut config) => {
                config.configure(HIGHLIGHT_NAMES);
                self.configs.insert(Lang::JavaScript, config);
            }
            Err(e) => {
                eprintln!("Failed to create JavaScript highlight config: {:?}", e);
            }
        }

        // JSON
        match HighlightConfiguration::new(
            tree_sitter_json::LANGUAGE.into(),
            "json",
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            Ok(mut config) => {
                config.configure(HIGHLIGHT_NAMES);
                self.configs.insert(Lang::Json, config);
            }
            Err(e) => {
                eprintln!("Failed to create JSON highlight config: {:?}", e);
            }
        }

        // Markdown
        match HighlightConfiguration::new(
            tree_sitter_md::LANGUAGE.into(),
            "markdown",
            "", // TODO: Add Markdown queries
            "",
            "",
        ) {
            Ok(mut config) => {
                config.configure(HIGHLIGHT_NAMES);
                self.configs.insert(Lang::Markdown, config);
            }
            Err(e) => {
                eprintln!("Failed to create Markdown highlight config: {:?}", e);
            }
        }

        // HTML
        match HighlightConfiguration::new(
            tree_sitter_html::LANGUAGE.into(),
            "html",
            tree_sitter_html::HIGHLIGHTS_QUERY,
            tree_sitter_html::INJECTIONS_QUERY,
            "",
        ) {
            Ok(mut config) => {
                config.configure(HIGHLIGHT_NAMES);
                self.configs.insert(Lang::Html, config);
            }
            Err(e) => {
                eprintln!("Failed to create HTML highlight config: {:?}", e);
            }
        }

        // Bash
        match HighlightConfiguration::new(
            tree_sitter_bash::LANGUAGE.into(),
            "bash",
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        ) {
            Ok(mut config) => {
                config.configure(HIGHLIGHT_NAMES);
                self.configs.insert(Lang::Bash, config);
            }
            Err(e) => {
                eprintln!("Failed to create Bash highlight config: {:?}", e);
            }
        }

        // Scheme (for .scm files, logs, and Steel scripting)
        match HighlightConfiguration::new(
            tree_sitter_scheme::LANGUAGE.into(),
            "scheme",
            tree_sitter_scheme::HIGHLIGHTS_QUERY,
            "",
            "",
        ) {
            Ok(mut config) => {
                config.configure(HIGHLIGHT_NAMES);
                self.configs.insert(Lang::Scheme, config);
            }
            Err(e) => {
                eprintln!("Failed to create Scheme highlight config: {:?}", e);
            }
        }
    }

    /// Get highlights for source code
    pub fn highlight(&mut self, lang: Lang, source: &[u8]) -> Vec<HighlightSpan> {
        let config = match self.configs.get(&lang) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let mut spans = Vec::new();
        let mut highlight_stack: Vec<usize> = Vec::new();

        let highlights = match self.highlighter.highlight(config, source, None, |_| None) {
            Ok(h) => h,
            Err(_) => return spans,
        };

        for event in highlights {
            match event {
                Ok(HighlightEvent::Source { start, end }) => {
                    if let Some(&highlight_index) = highlight_stack.last() {
                        spans.push(HighlightSpan {
                            start,
                            end,
                            highlight_index,
                        });
                    }
                }
                Ok(HighlightEvent::HighlightStart(Highlight(index))) => {
                    highlight_stack.push(index);
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    highlight_stack.pop();
                }
                Err(_) => break,
            }
        }

        spans
    }

    /// Get highlights for visible lines (1-indexed, inclusive)
    /// Returns vec of highlights per line: vec of (col_start, col_end, color)
    /// Parses file ONCE, not per-line!
    pub fn highlights_for_lines(
        &mut self,
        lang: Lang,
        source: &str,
        start_line: usize,
        end_line: usize,
    ) -> Vec<Vec<(usize, usize, (u8, u8, u8))>> {
        // Parse once
        let spans = self.highlight(lang, source.as_bytes());

        // Build line byte ranges
        let mut line_ranges: Vec<(usize, usize)> = Vec::new();
        let mut line_start = 0;
        for (i, ch) in source.char_indices() {
            if ch == '\n' {
                line_ranges.push((line_start, i));
                line_start = i + 1;
            }
        }
        // Last line (no trailing newline)
        if line_start <= source.len() {
            line_ranges.push((line_start, source.len()));
        }

        // Build result for requested lines
        let mut result = Vec::new();
        for line_num in start_line..=end_line {
            let line_idx = line_num.saturating_sub(1);
            if let Some(&(ls, le)) = line_ranges.get(line_idx) {
                let line_highlights: Vec<_> = spans
                    .iter()
                    .filter(|span| span.start < le && span.end > ls)
                    .map(|span| {
                        let col_start = span.start.saturating_sub(ls);
                        let col_end = (span.end - ls).min(le - ls);
                        let color = self.face_color(span.highlight_index);
                        (col_start, col_end, color)
                    })
                    .collect();
                result.push(line_highlights);
            } else {
                result.push(Vec::new());
            }
        }
        result
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_detection() {
        assert_eq!(Lang::from_extension("rs"), Lang::Rust);
        assert_eq!(Lang::from_extension("js"), Lang::JavaScript);
        assert_eq!(Lang::from_extension("json"), Lang::Json);
        assert_eq!(Lang::from_extension("txt"), Lang::Plain);
    }

    #[test]
    fn test_highlight_rust() {
        let mut highlighter = SyntaxHighlighter::new();

        // Check if Rust config exists
        assert!(highlighter.configs.contains_key(&Lang::Rust), "Rust config should exist");

        let code = "fn main() {}";
        let spans = highlighter.highlight(Lang::Rust, code.as_bytes());

        // Should have some highlights
        assert!(!spans.is_empty(), "Should have highlights for Rust code");
    }

    #[test]
    fn test_highlight_json() {
        let mut highlighter = SyntaxHighlighter::new();

        assert!(highlighter.configs.contains_key(&Lang::Json), "JSON config should exist");

        let code = r#"{"key": "value", "num": 42}"#;
        let spans = highlighter.highlight(Lang::Json, code.as_bytes());

        assert!(!spans.is_empty(), "Should have highlights for JSON code");
    }
}
