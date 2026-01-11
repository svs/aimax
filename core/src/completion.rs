//! Completion engine - fuzzy matching with nucleo
//!
//! Provides Vertico-style completion:
//! - Fuzzy matching
//! - Highlighting of matched characters
//! - Multiple completion sources (files, buffers, commands)

use std::path::PathBuf;
use nucleo::Utf32String;
use nucleo::pattern::{Pattern, CaseMatching, Normalization};
use nucleo_matcher::{Matcher, Config};

/// A completion match with score and highlight info
#[derive(Debug, Clone)]
pub struct Match {
    /// The matched text
    pub text: String,
    /// Match score (higher is better)
    pub score: u32,
    /// Indices of matched characters (for highlighting)
    pub indices: Vec<usize>,
}

/// Types of completion sources
#[derive(Debug, Clone)]
pub enum CompletionSource {
    /// Files in a directory
    Files { directory: PathBuf },
    /// Buffer names
    Buffers,
    /// Command names
    Commands,
    /// Static list of candidates
    Static(Vec<String>),
}

/// Completer with fuzzy matching
pub struct Completer {
    source: CompletionSource,
    candidates: Vec<String>,
}

impl Completer {
    pub fn new(source: CompletionSource) -> Self {
        let candidates = match &source {
            CompletionSource::Files { directory } => {
                Self::list_files(directory)
            }
            CompletionSource::Buffers => {
                // Will be populated externally
                Vec::new()
            }
            CompletionSource::Commands => {
                // Will be populated externally
                Vec::new()
            }
            CompletionSource::Static(items) => {
                items.clone()
            }
        };

        Completer { source, candidates }
    }

    /// List files in directory for completion
    fn list_files(directory: &PathBuf) -> Vec<String> {
        let mut files = Vec::new();

        // Read directory entries
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden files unless we're completing a hidden file
                if name.starts_with('.') {
                    continue;
                }

                if path.is_dir() {
                    files.push(format!("{}/", name));
                } else {
                    files.push(name);
                }
            }
        }

        files.sort();
        files
    }

    /// Update candidates (for dynamic sources)
    pub fn set_candidates(&mut self, candidates: Vec<String>) {
        self.candidates = candidates;
    }

    /// Refresh file list from directory
    pub fn refresh_files(&mut self, directory: &PathBuf) {
        self.candidates = Self::list_files(directory);
    }

    /// Complete with fuzzy matching
    pub fn complete(&self, input: &str) -> Vec<Match> {
        if input.is_empty() {
            // Return all candidates with score 0
            return self.candidates.iter()
                .take(100) // Limit for performance
                .map(|c| Match {
                    text: c.clone(),
                    score: 0,
                    indices: Vec::new(),
                })
                .collect();
        }

        // Use nucleo for fuzzy matching
        let pattern = Pattern::new(
            input,
            CaseMatching::Smart,
            Normalization::Smart,
            nucleo::pattern::AtomKind::Fuzzy,
        );

        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut indices = Vec::new();

        let mut matches: Vec<Match> = self.candidates.iter()
            .filter_map(|candidate| {
                indices.clear();
                let haystack = Utf32String::from(candidate.as_str());
                let score = pattern.indices(haystack.slice(..), &mut matcher, &mut indices);

                score.map(|s| {
                    Match {
                        text: candidate.clone(),
                        score: s,
                        indices: indices.iter().map(|&i| i as usize).collect(),
                    }
                })
            })
            .collect();

        // Sort by score (descending)
        matches.sort_by(|a, b| b.score.cmp(&a.score));

        // Limit results
        matches.truncate(50);
        matches
    }

    /// Get the source type
    pub fn source(&self) -> &CompletionSource {
        &self.source
    }
}

/// File path completer - just lists files in a directory
pub struct FileCompleter {
    current_dir: PathBuf,
    completer: Completer,
}

impl FileCompleter {
    pub fn new(directory: PathBuf) -> Self {
        let completer = Completer::new(CompletionSource::Files { directory: directory.clone() });
        FileCompleter {
            current_dir: directory,
            completer,
        }
    }

    /// Change the directory being completed
    pub fn set_directory(&mut self, dir: PathBuf) {
        if dir != self.current_dir && dir.is_dir() {
            self.current_dir = dir.clone();
            self.completer.refresh_files(&dir);
        }
    }

    /// Get completions for a filename prefix
    pub fn complete(&self, prefix: &str) -> Vec<Match> {
        self.completer.complete(prefix)
    }

    /// Get the current directory
    pub fn current_dir(&self) -> &PathBuf {
        &self.current_dir
    }

    /// Build full path from a filename
    pub fn full_path(&self, filename: &str) -> PathBuf {
        self.current_dir.join(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_completer_subdir() {
        // Test that file completer works when changing directories
        let cwd = std::env::current_dir().unwrap();
        eprintln!("cwd: {:?}", cwd);

        let mut fc = FileCompleter::new(cwd.clone());

        // Initial completions should work
        let initial = fc.complete("");
        eprintln!("initial completions: {:?}", initial.iter().map(|m| &m.text).collect::<Vec<_>>());
        assert!(!initial.is_empty(), "Should have some files in cwd");

        // Find a subdirectory
        let subdir = initial.iter().find(|m| m.text.ends_with('/'));
        if let Some(subdir_match) = subdir {
            eprintln!("Found subdir: {:?}", subdir_match.text);

            // The subdir name includes trailing slash, e.g. "core/"
            let subdir_name = &subdir_match.text;
            let subdir_path = cwd.join(subdir_name.trim_end_matches('/'));

            eprintln!("subdir_path: {:?}, is_dir: {}", subdir_path, subdir_path.is_dir());

            // Change to subdirectory
            fc.set_directory(subdir_path.clone());

            eprintln!("fc.current_dir after set: {:?}", fc.current_dir());

            // Should now complete files in subdirectory
            let subdir_completions = fc.complete("");
            eprintln!("subdir completions: {:?}", subdir_completions.iter().map(|m| &m.text).collect::<Vec<_>>());

            // The current_dir should have changed
            assert_eq!(fc.current_dir(), &subdir_path, "Directory should have changed");
            // And we should have completions from the subdir (different from initial)
        }
    }

    #[test]
    fn test_fuzzy_match() {
        let completer = Completer::new(CompletionSource::Static(vec![
            "find-file".to_string(),
            "forward-char".to_string(),
            "find-buffer".to_string(),
            "save-buffer".to_string(),
        ]));

        let matches = completer.complete("ff");
        assert!(!matches.is_empty());
        // "find-file" should be top match for "ff"
        assert_eq!(matches[0].text, "find-file");
    }

    #[test]
    fn test_empty_input() {
        let completer = Completer::new(CompletionSource::Static(vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
        ]));

        let matches = completer.complete("");
        assert_eq!(matches.len(), 3);
    }
}
