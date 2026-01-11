//! Buffer - The fundamental data structure
//!
//! A buffer is a named container of text with a cursor position (point).
//! Uses ropey for efficient editing of large texts.

use ropey::Rope;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// A buffer: named container of text with a cursor position
#[derive(Debug, Clone)]
pub struct Buffer {
    /// Buffer name (e.g., "*scratch*", "main.rs")
    pub name: String,
    /// File path (if buffer is associated with a file)
    pub file_path: Option<PathBuf>,
    /// Buffer contents (rope for efficient large-file handling)
    rope: Rope,
    /// Cursor position (char index, not byte)
    point: usize,
    /// Has buffer been modified since last save?
    modified: bool,
}

impl Buffer {
    /// Create a new empty buffer
    pub fn new(name: impl Into<String>) -> Self {
        Buffer {
            name: name.into(),
            file_path: None,
            rope: Rope::new(),
            point: 0,
            modified: false,
        }
    }

    /// Create a buffer with initial text
    pub fn with_text(name: impl Into<String>, text: &str) -> Self {
        Buffer {
            name: name.into(),
            file_path: None,
            rope: Rope::from_str(text),
            point: 0,
            modified: false,
        }
    }

    /// Load a buffer from a file
    pub fn from_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let rope = Rope::from_reader(reader)?;

        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        Ok(Buffer {
            name,
            file_path: Some(path.to_path_buf()),
            rope,
            point: 0,
            modified: false,
        })
    }

    /// Save buffer to its associated file
    pub fn save(&mut self) -> io::Result<()> {
        let path = self.file_path.as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No file path set"))?;

        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        self.rope.write_to(&mut writer)?;
        self.modified = false;
        Ok(())
    }

    /// Save buffer to a specific path
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        self.rope.write_to(&mut writer)?;

        self.file_path = Some(path.to_path_buf());
        self.name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();
        self.modified = false;
        Ok(())
    }

    /// Is buffer modified?
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Get buffer text as string (for display)
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Get point (cursor position as char index)
    pub fn point(&self) -> usize {
        self.point
    }

    /// Set point (clamped to valid range)
    pub fn set_point(&mut self, point: usize) {
        self.point = point.min(self.rope.len_chars());
    }

    /// Buffer length in chars
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Buffer length in bytes
    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Is buffer empty?
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    /// Insert a character at point
    pub fn insert_char(&mut self, c: char) {
        self.rope.insert_char(self.point, c);
        self.point += 1;
        self.modified = true;
    }

    /// Insert a string at point
    pub fn insert(&mut self, s: &str) {
        self.rope.insert(self.point, s);
        self.point += s.chars().count();
        self.modified = true;
    }

    /// Delete character before point (backspace)
    pub fn delete_backward(&mut self) {
        if self.point > 0 {
            self.point -= 1;
            self.rope.remove(self.point..self.point + 1);
            self.modified = true;
        }
    }

    /// Delete character at point (delete key)
    pub fn delete_forward(&mut self) {
        if self.point < self.rope.len_chars() {
            self.rope.remove(self.point..self.point + 1);
            self.modified = true;
        }
    }

    /// Move point forward one character
    pub fn forward_char(&mut self) {
        if self.point < self.rope.len_chars() {
            self.point += 1;
        }
    }

    /// Move point backward one character
    pub fn backward_char(&mut self) {
        if self.point > 0 {
            self.point -= 1;
        }
    }

    /// Get current line index (0-indexed)
    fn current_line_idx(&self) -> usize {
        self.rope.char_to_line(self.point)
    }

    /// Move to beginning of current line
    pub fn beginning_of_line(&mut self) {
        let line_idx = self.current_line_idx();
        self.point = self.rope.line_to_char(line_idx);
    }

    /// Move to end of current line (before newline)
    pub fn end_of_line(&mut self) {
        let line_idx = self.current_line_idx();
        let line = self.rope.line(line_idx);
        let line_start = self.rope.line_to_char(line_idx);
        // Don't include the trailing newline
        let line_len = line.len_chars().saturating_sub(
            if line.len_chars() > 0 && line.char(line.len_chars() - 1) == '\n' { 1 } else { 0 }
        );
        self.point = line_start + line_len;
    }

    /// Get current column (0-indexed, in chars)
    pub fn current_column(&self) -> usize {
        let line_idx = self.current_line_idx();
        let line_start = self.rope.line_to_char(line_idx);
        self.point - line_start
    }

    /// Move up one line, trying to preserve column
    pub fn previous_line(&mut self) {
        let line_idx = self.current_line_idx();
        if line_idx > 0 {
            let col = self.current_column();
            let prev_line_idx = line_idx - 1;
            let prev_line = self.rope.line(prev_line_idx);
            let prev_line_len = prev_line.len_chars().saturating_sub(1); // exclude newline
            let prev_line_start = self.rope.line_to_char(prev_line_idx);
            self.point = prev_line_start + col.min(prev_line_len);
        }
    }

    /// Move down one line, trying to preserve column
    pub fn next_line(&mut self) {
        let line_idx = self.current_line_idx();
        let total_lines = self.rope.len_lines();
        if line_idx + 1 < total_lines {
            let col = self.current_column();
            let next_line_idx = line_idx + 1;
            let next_line = self.rope.line(next_line_idx);
            let next_line_len = if next_line_idx + 1 < total_lines {
                next_line.len_chars().saturating_sub(1) // exclude newline
            } else {
                next_line.len_chars() // last line has no trailing newline
            };
            let next_line_start = self.rope.line_to_char(next_line_idx);
            self.point = next_line_start + col.min(next_line_len);
        }
    }

    /// Move to beginning of buffer
    pub fn beginning_of_buffer(&mut self) {
        self.point = 0;
    }

    /// Move to end of buffer
    pub fn end_of_buffer(&mut self) {
        self.point = self.rope.len_chars();
    }

    /// Move forward one word
    pub fn forward_word(&mut self) {
        let len = self.rope.len_chars();
        // Skip non-word chars
        while self.point < len {
            let c = self.rope.char(self.point);
            if c.is_alphanumeric() || c == '_' {
                break;
            }
            self.point += 1;
        }
        // Skip word chars
        while self.point < len {
            let c = self.rope.char(self.point);
            if !c.is_alphanumeric() && c != '_' {
                break;
            }
            self.point += 1;
        }
    }

    /// Move backward one word
    pub fn backward_word(&mut self) {
        // Skip non-word chars backward
        while self.point > 0 {
            let c = self.rope.char(self.point - 1);
            if c.is_alphanumeric() || c == '_' {
                break;
            }
            self.point -= 1;
        }
        // Skip word chars backward
        while self.point > 0 {
            let c = self.rope.char(self.point - 1);
            if !c.is_alphanumeric() && c != '_' {
                break;
            }
            self.point -= 1;
        }
    }

    /// Go to specific line (1-indexed)
    pub fn goto_line(&mut self, line: usize) {
        if line == 0 {
            return;
        }
        let target = line.saturating_sub(1); // Convert to 0-indexed
        if target < self.rope.len_lines() {
            self.point = self.rope.line_to_char(target);
        } else {
            self.end_of_buffer();
        }
    }

    /// Delete word backward (like M-backspace)
    pub fn delete_word_backward(&mut self) {
        let start = self.point;
        self.backward_word();
        let end = self.point;
        if end < start {
            self.rope.remove(end..start);
            self.modified = true;
        }
    }

    /// Delete word forward (like M-d)
    pub fn delete_word_forward(&mut self) {
        let start = self.point;
        self.forward_word();
        let end = self.point;
        if start < end {
            self.rope.remove(start..end);
            self.point = start;
            self.modified = true;
        }
    }

    /// Get current line number (1-indexed, for display)
    pub fn current_line(&self) -> usize {
        self.current_line_idx() + 1
    }

    /// Get total line count
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Get a specific line (0-indexed)
    pub fn line(&self, idx: usize) -> Option<String> {
        if idx < self.rope.len_lines() {
            Some(self.rope.line(idx).to_string())
        } else {
            None
        }
    }

    /// Iterator over lines
    pub fn lines(&self) -> impl Iterator<Item = String> + '_ {
        self.rope.lines().map(|l| l.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert() {
        let mut buf = Buffer::new("test");
        buf.insert_char('h');
        buf.insert_char('i');
        assert_eq!(buf.text(), "hi");
        assert_eq!(buf.point(), 2);
    }

    #[test]
    fn test_insert_string() {
        let mut buf = Buffer::new("test");
        buf.insert("hello");
        assert_eq!(buf.text(), "hello");
        assert_eq!(buf.point(), 5);
    }

    #[test]
    fn test_delete_backward() {
        let mut buf = Buffer::new("test");
        buf.insert("hello");
        buf.delete_backward();
        assert_eq!(buf.text(), "hell");
        assert_eq!(buf.point(), 4);
    }

    #[test]
    fn test_movement() {
        let mut buf = Buffer::with_text("test", "line1\nline2");
        buf.set_point(0);
        buf.next_line();
        assert_eq!(buf.point(), 6); // start of line2
        buf.previous_line();
        assert_eq!(buf.point(), 0); // start of line1
    }

    #[test]
    fn test_line_navigation() {
        let mut buf = Buffer::with_text("test", "hello world");
        buf.set_point(6); // at 'w'
        buf.beginning_of_line();
        assert_eq!(buf.point(), 0);
        buf.end_of_line();
        assert_eq!(buf.point(), 11);
    }

    #[test]
    fn test_utf8() {
        let mut buf = Buffer::new("test");
        buf.insert("héllo"); // é is 1 char (but 2 bytes)
        assert_eq!(buf.point(), 5); // 5 chars, not 6 bytes
        buf.backward_char();
        assert_eq!(buf.point(), 4);
    }

    #[test]
    fn test_large_text() {
        let mut buf = Buffer::new("test");
        // Insert a lot of text - ropey handles this efficiently
        for _ in 0..10000 {
            buf.insert("Lorem ipsum dolor sit amet\n");
        }
        assert_eq!(buf.line_count(), 10001); // 10000 lines + empty last
    }
}
