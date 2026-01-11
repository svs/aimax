# Aimax

**Emacs, the good parts.**

A programmable editor for the AI age. Scheme-scriptable, tree-sitter powered, LSP-enabled. Native on Mac, TUI everywhere.

## Architecture

- **Core** (Rust): ropey + tree-sitter + LSP. Battle-tested libraries, not reinvented wheels.
- **Scripting** (Steel Scheme): Commands, keymaps, hooks, agents.
- **TUI** (ratatui): Universal baseline. Works over SSH.
- **Mac GUI** (Swift): Luxurious native experience with embedded browser.

## Principles

1. Don't reinvent text editing. Use ropey, tree-sitter, LSP.
2. Everything is a command. Keys map to commands. Scheme calls commands.
3. Buffers are the universal abstraction. Text, terminal, browser - all buffers.
4. Agents are Scheme programs that observe and act on buffers.
5. Fast and native, not web-based (except the embedded browser).

## Current State

- Core: ropey-based buffer with file I/O
- Commands: Registry with built-in commands
- Keymaps: Emacs-style key sequences (C-x C-f, etc.)
- Minibuffer: Completing-read with fuzzy matching (nucleo)
- TUI: Working editor with Vertico-style completion

## Skills

### /new-feature

When adding a new feature to Aimax, follow this checklist:

1. **How does Emacs do it?**
   - Read `docs/emacs-architecture.md`
   - Understand the Emacs abstraction (overlays? text properties? hooks?)
   - Document the Emacs approach in the feature design

2. **Can we do the same?**
   - Match the Emacs API where it makes sense
   - Keep Scheme interop in mind (will this be scriptable?)
   - Don't deviate without good reason

3. **Is there something that already does this in Rust?**
   - Check crates.io for battle-tested implementations
   - Examples: ropey (text), tree-sitter (parsing), nucleo (fuzzy), tower-lsp (LSP)
   - Prefer existing libraries over reinventing

4. **Implementation**
   - Core feature goes in `core/src/`
   - TUI rendering goes in `tui/src/`
   - Export from `core/src/lib.rs`
   - Add tests

## Next

- Tree-sitter syntax highlighting in TUI
- Multiple buffers (C-x b)
- Windows (C-x 2, C-x 3)
- LSP client (tower-lsp)
- Steel Scheme scripting
- Mac native GUI
