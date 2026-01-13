//! Aimax Core - Buffer management and Scheme scripting
//!
//! The programmable heart of Aimax. Portable Rust core that can be used from:
//! - TUI (ratatui) - works everywhere
//! - Mac GUI (Swift) - native luxury
//! - Linux GUI (GTK) - native Linux
//!
//! Buffers, commands, keymaps, hooks, and Steel Scheme interpreter live here.

pub mod buffer;
pub mod command;
pub mod completion;
pub mod editor;
pub mod face;
pub mod keymap;
pub mod llm;
pub mod log;
pub mod minibuffer;
pub mod process;
pub mod scheme;
pub mod syntax;
pub mod ipc;
pub mod tools;

pub use buffer::Buffer;
pub use command::{CommandRegistry, CommandResult, Context};
pub use completion::{Completer, CompletionSource, FileCompleter, Match};
pub use editor::{Editor, MinibufferMode};
pub use face::{Color, FaceAttributes, FaceRegistry, FaceCache, ScopeMap};
pub use keymap::{Key, KeyLookup, Keymap, KeymapStack, Mode, ModeType};
pub use minibuffer::{Minibuffer, path as minibuffer_path};
pub use syntax::{Lang, SyntaxHighlighter, HighlightSpan, highlight_color};
pub use scheme::Interpreter;
pub use process::{ProcessRegistry, ProcessMessage, MAX_PROCESS_BUFFER_LINES};
pub use llm::{ChatConfig, Message, StreamEvent, Tool, ToolParam, chat_stream};
pub use tools::{ToolRegistry, ToolResult};
