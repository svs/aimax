# Aimax Roadmap: The Path to 1.0

## 1. Core Architecture (Rust)

- [x] **Multi-Buffer Support**
    - Refactor `Editor` to hold `buffers: Vec<Buffer>`.
    - Implement `current` buffer index.
    - Commands: `switch-to-buffer` (C-x b), `find-file` (C-x C-f).

- [ ] **Window Management (Splits)**
    - Create `Window` struct (view into a buffer + scroll/cursor state).
    - Create `Layout` tree (Horizontal/Vertical splits).
    - Commands: `split-window-below`, `split-window-right`, `delete-other-windows`.

- [x] **IPC Server (The AI Interface)**
    - Background thread listening on Unix Socket (`~/.aimax/aimax.sock`).
    - Protocol: Text in (Scheme code) -> Text out (Evaluation result).
    - Thread-safety: Command Queue pattern to execute Scheme on the main thread.
    - Multi-line S-expression support.

- [ ] **Undo/Redo**
    - Implement operation history stack per buffer.

- [x] **Face System (Theming)**
    - FaceRegistry with named faces (font-lock-keyword-face, etc.).
    - ScopeMap: tree-sitter scopes → face names.
    - Cached color lookups (O(1) via index).
    - Scheme API: `(set-face-attribute face key value)`.

## 2. Scripting API (Scheme)

- [x] **Buffer Manipulation**
    - `(buffer-insert text)`, `(buffer-text)`, `(buffer-line n)`.
    - `(buffer-open path)`, `(buffer-switch name)`.
    - Movement: `(forward-char)`, `(next-line)`, `(goto-line n)`, etc.

- [x] **Hooks System**
    - Variables holding lambdas: `*before-quit*`, `*before-save*`, `*after-save*`.
    - User overrides via `set!` in init.scm.
    - Desktop save/restore on quit/startup.

- [ ] **Keymap Config**
    - Move `C-x C-f` definitions from Rust `CommandRegistry` to `init.scm`.

## 4. Process System (The AI "Eyes")

- [ ] **Process Buffers (Comint-style)**
    - Use `portable-pty` to spawn processes.
    - Stream PTY output into a `Buffer`.
    - Implement Ring Buffer logic (drop old lines to prevent OOM).
    - Handle ANSI escape codes (convert to Faces or strip).

- [ ] **Process Hooks**
    - `(add-hook 'process-output-hook ...)` so agents can react to output in real-time.

## 3. "Agent Ready" Features

- [ ] **Semantic Editing Primitives**
    - Helper functions for agents to edit code reliably.
    - Example: `(replace-range start-line start-col end-line end-col "text")`.
    - Example: `(search-forward "pattern")`.

- [ ] **Headless Mode**
    - Ability to run `aimax --headless` so an agent can use it as a backend engine without the TUI rendering.
