//! Tool Registry - Built-in tools for agentic chat
//!
//! Each tool has:
//! - name: identifier used by the LLM
//! - description: explains what the tool does
//! - parameters: JSON schema for inputs
//! - handler: executes the tool and returns result

use crate::llm::{Tool, ToolParam};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Result of executing a tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub content: String,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        ToolResult {
            success: true,
            content: content.into(),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        ToolResult {
            success: false,
            content: message.into(),
        }
    }
}

/// Tool handler function type
pub type ToolHandler = fn(Value) -> ToolResult;

/// Entry in the tool registry
pub struct ToolEntry {
    pub tool: Tool,
    pub handler: ToolHandler,
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, ToolEntry>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create a new registry with built-in tools
    pub fn new() -> Self {
        let mut registry = ToolRegistry {
            tools: HashMap::new(),
        };

        // Register built-in tools
        registry.register_builtin_tools();

        registry
    }

    /// Register the built-in tools
    fn register_builtin_tools(&mut self) {
        // read_file - Read contents of a file
        self.register(ToolEntry {
            tool: Tool {
                name: "read_file".to_string(),
                description: "Read the contents of a file at the specified path".to_string(),
                parameters: vec![ToolParam {
                    name: "path".to_string(),
                    param_type: "string".to_string(),
                    description: "The absolute or relative file path to read".to_string(),
                    required: true,
                }],
            },
            handler: handle_read_file,
        });

        // write_file - Write content to a file
        self.register(ToolEntry {
            tool: Tool {
                name: "write_file".to_string(),
                description: "Write content to a file at the specified path".to_string(),
                parameters: vec![
                    ToolParam {
                        name: "path".to_string(),
                        param_type: "string".to_string(),
                        description: "The file path to write to".to_string(),
                        required: true,
                    },
                    ToolParam {
                        name: "content".to_string(),
                        param_type: "string".to_string(),
                        description: "The content to write to the file".to_string(),
                        required: true,
                    },
                ],
            },
            handler: handle_write_file,
        });

        // list_directory - List files in a directory
        self.register(ToolEntry {
            tool: Tool {
                name: "list_directory".to_string(),
                description: "List files and directories at the specified path".to_string(),
                parameters: vec![ToolParam {
                    name: "path".to_string(),
                    param_type: "string".to_string(),
                    description: "The directory path to list".to_string(),
                    required: true,
                }],
            },
            handler: handle_list_directory,
        });

        // run_command - Execute a shell command
        self.register(ToolEntry {
            tool: Tool {
                name: "run_command".to_string(),
                description: "Execute a shell command and return its output".to_string(),
                parameters: vec![ToolParam {
                    name: "command".to_string(),
                    param_type: "string".to_string(),
                    description: "The shell command to execute".to_string(),
                    required: true,
                }],
            },
            handler: handle_run_command,
        });

        // search_files - Search for files matching a pattern
        self.register(ToolEntry {
            tool: Tool {
                name: "search_files".to_string(),
                description: "Search for files matching a glob pattern".to_string(),
                parameters: vec![
                    ToolParam {
                        name: "pattern".to_string(),
                        param_type: "string".to_string(),
                        description: "Glob pattern to match files (e.g., '**/*.rs')".to_string(),
                        required: true,
                    },
                    ToolParam {
                        name: "path".to_string(),
                        param_type: "string".to_string(),
                        description: "Starting directory for search (defaults to current directory)"
                            .to_string(),
                        required: false,
                    },
                ],
            },
            handler: handle_search_files,
        });
    }

    /// Register a tool
    pub fn register(&mut self, entry: ToolEntry) {
        self.tools.insert(entry.tool.name.clone(), entry);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&ToolEntry> {
        self.tools.get(name)
    }

    /// Get all registered tools
    pub fn all_tools(&self) -> Vec<&Tool> {
        self.tools.values().map(|e| &e.tool).collect()
    }

    /// Execute a tool by name with given input
    pub fn execute(&self, name: &str, input: Value) -> ToolResult {
        match self.get(name) {
            Some(entry) => (entry.handler)(input),
            None => ToolResult::err(format!("Unknown tool: {}", name)),
        }
    }

    /// Convert to Tool vec for LLM API
    pub fn to_llm_tools(&self) -> Vec<Tool> {
        self.tools.values().map(|e| e.tool.clone()).collect()
    }
}

// === Tool Handlers ===

fn handle_read_file(input: Value) -> ToolResult {
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolResult::err("Missing required parameter: path"),
    };

    match fs::read_to_string(path) {
        Ok(content) => ToolResult::ok(content),
        Err(e) => ToolResult::err(format!("Failed to read file: {}", e)),
    }
}

fn handle_write_file(input: Value) -> ToolResult {
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolResult::err("Missing required parameter: path"),
    };

    let content = match input.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolResult::err("Missing required parameter: content"),
    };

    // Create parent directories if they don't exist
    if let Some(parent) = Path::new(path).parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult::err(format!("Failed to create directory: {}", e));
            }
        }
    }

    match fs::write(path, content) {
        Ok(()) => ToolResult::ok(format!("Successfully wrote {} bytes to {}", content.len(), path)),
        Err(e) => ToolResult::err(format!("Failed to write file: {}", e)),
    }
}

fn handle_list_directory(input: Value) -> ToolResult {
    let path = match input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolResult::err("Missing required parameter: path"),
    };

    match fs::read_dir(path) {
        Ok(entries) => {
            let mut items: Vec<String> = entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let metadata = e.metadata().ok();
                    let suffix = if metadata.map(|m| m.is_dir()).unwrap_or(false) {
                        "/"
                    } else {
                        ""
                    };
                    format!("{}{}", name, suffix)
                })
                .collect();
            items.sort();
            ToolResult::ok(items.join("\n"))
        }
        Err(e) => ToolResult::err(format!("Failed to list directory: {}", e)),
    }
}

fn handle_run_command(input: Value) -> ToolResult {
    let command = match input.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolResult::err("Missing required parameter: command"),
    };

    match std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let result = if output.status.success() {
                if stderr.is_empty() {
                    stdout.to_string()
                } else {
                    format!("{}\n[stderr]: {}", stdout, stderr)
                }
            } else {
                format!(
                    "Command failed with exit code: {}\nstdout: {}\nstderr: {}",
                    output.status.code().unwrap_or(-1),
                    stdout,
                    stderr
                )
            };

            ToolResult {
                success: output.status.success(),
                content: result,
            }
        }
        Err(e) => ToolResult::err(format!("Failed to execute command: {}", e)),
    }
}

fn handle_search_files(input: Value) -> ToolResult {
    let pattern = match input.get("pattern").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolResult::err("Missing required parameter: pattern"),
    };

    let base_path = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");

    // Simple glob implementation using walkdir would be better,
    // but for now just use find command
    let command = format!("find {} -name '{}' -type f 2>/dev/null | head -100", base_path, pattern);

    match std::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                ToolResult::ok("No files found matching pattern")
            } else {
                ToolResult::ok(stdout.trim().to_string())
            }
        }
        Err(e) => ToolResult::err(format!("Failed to search files: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_builtin_tools() {
        let registry = ToolRegistry::new();
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("list_directory").is_some());
        assert!(registry.get("run_command").is_some());
        assert!(registry.get("search_files").is_some());
    }

    #[test]
    fn test_read_file_missing_path() {
        let result = handle_read_file(json!({}));
        assert!(!result.success);
        assert!(result.content.contains("Missing required parameter"));
    }

    #[test]
    fn test_read_file_nonexistent() {
        let result = handle_read_file(json!({"path": "/nonexistent/file.txt"}));
        assert!(!result.success);
        assert!(result.content.contains("Failed to read file"));
    }

    #[test]
    fn test_list_directory() {
        let result = handle_list_directory(json!({"path": "."}));
        assert!(result.success);
        // Should list at least something in current directory
        assert!(!result.content.is_empty());
    }

    #[test]
    fn test_run_command() {
        let result = handle_run_command(json!({"command": "echo hello"}));
        assert!(result.success);
        assert!(result.content.contains("hello"));
    }

    #[test]
    fn test_run_command_failure() {
        let result = handle_run_command(json!({"command": "exit 1"}));
        assert!(!result.success);
        assert!(result.content.contains("exit code"));
    }

    #[test]
    fn test_tool_result_constructors() {
        let ok = ToolResult::ok("success");
        assert!(ok.success);
        assert_eq!(ok.content, "success");

        let err = ToolResult::err("error message");
        assert!(!err.success);
        assert_eq!(err.content, "error message");
    }

    #[test]
    fn test_execute_unknown_tool() {
        let registry = ToolRegistry::new();
        let result = registry.execute("unknown_tool", json!({}));
        assert!(!result.success);
        assert!(result.content.contains("Unknown tool"));
    }
}
