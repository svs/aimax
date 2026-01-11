//! Command system - named operations that can be bound to keys
//!
//! Commands are the extension point. Everything the user can do
//! is a command. Keys map to commands. Scheme code calls commands.

use std::collections::HashMap;
use std::sync::Arc;

use crate::Buffer;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq)]
pub enum CommandResult {
    /// Command succeeded
    Ok,
    /// Command succeeded with a message to display
    Message(String),
    /// Command failed with an error
    Error(String),
    /// Request to quit the editor
    Quit,
    /// Command not implemented yet
    NotImplemented(String),
}

/// Editor context passed to commands
pub struct Context<'a> {
    pub buffer: &'a mut Buffer,
    // Will expand: windows, all buffers, minibuffer, etc.
}

/// A command function
pub type CommandFn = Arc<dyn Fn(&mut Context) -> CommandResult + Send + Sync>;

/// Command registry - all available commands
pub struct CommandRegistry {
    commands: HashMap<String, CommandFn>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut registry = CommandRegistry {
            commands: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    /// Register a command
    pub fn register(&mut self, name: &str, cmd: CommandFn) {
        self.commands.insert(name.to_string(), cmd);
    }

    /// Get a command by name
    pub fn get(&self, name: &str) -> Option<&CommandFn> {
        self.commands.get(name)
    }

    /// Execute a command by name
    pub fn execute(&self, name: &str, ctx: &mut Context) -> CommandResult {
        match self.get(name) {
            Some(cmd) => cmd(ctx),
            None => CommandResult::Error(format!("Unknown command: {}", name)),
        }
    }

    /// Register all built-in commands
    fn register_builtins(&mut self) {
        // Movement
        self.register("forward-char", Arc::new(|ctx| {
            ctx.buffer.forward_char();
            CommandResult::Ok
        }));

        self.register("backward-char", Arc::new(|ctx| {
            ctx.buffer.backward_char();
            CommandResult::Ok
        }));

        self.register("next-line", Arc::new(|ctx| {
            ctx.buffer.next_line();
            CommandResult::Ok
        }));

        self.register("previous-line", Arc::new(|ctx| {
            ctx.buffer.previous_line();
            CommandResult::Ok
        }));

        self.register("beginning-of-line", Arc::new(|ctx| {
            ctx.buffer.beginning_of_line();
            CommandResult::Ok
        }));

        self.register("end-of-line", Arc::new(|ctx| {
            ctx.buffer.end_of_line();
            CommandResult::Ok
        }));

        self.register("beginning-of-buffer", Arc::new(|ctx| {
            ctx.buffer.beginning_of_buffer();
            CommandResult::Ok
        }));

        self.register("end-of-buffer", Arc::new(|ctx| {
            ctx.buffer.end_of_buffer();
            CommandResult::Ok
        }));

        // Editing
        self.register("delete-backward-char", Arc::new(|ctx| {
            ctx.buffer.delete_backward();
            CommandResult::Ok
        }));

        self.register("delete-forward-char", Arc::new(|ctx| {
            ctx.buffer.delete_forward();
            CommandResult::Ok
        }));

        self.register("newline", Arc::new(|ctx| {
            ctx.buffer.insert_char('\n');
            CommandResult::Ok
        }));

        self.register("indent", Arc::new(|ctx| {
            ctx.buffer.insert("    ");
            CommandResult::Ok
        }));

        // File operations
        self.register("save-buffer", Arc::new(|ctx| {
            if ctx.buffer.file_path.is_some() {
                match ctx.buffer.save() {
                    Ok(()) => CommandResult::Message("Saved".to_string()),
                    Err(e) => CommandResult::Error(format!("Save failed: {}", e)),
                }
            } else {
                CommandResult::Error("No file path set".to_string())
            }
        }));

        // Quit
        self.register("quit", Arc::new(|_ctx| {
            CommandResult::Quit
        }));
    }

    /// List all command names
    pub fn list(&self) -> Vec<&str> {
        self.commands.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}
