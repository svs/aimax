# Aimax Roadmap: The Path to 1.0

## 1. Core Architecture (Rust)

- [ ] **Multi-Buffer Support**
    - Refactor `Editor` to hold `buffers: HashMap<String, Buffer>`.
    - Implement `current_buffer` pointer.
    - Commands: `switch-to-buffer`, `kill-buffer`.

- [ ] **Window Management (Splits)**
    - Create `Window` struct (view into a buffer + scroll/cursor state).
    - Create `Layout` tree (Horizontal/Vertical splits).
    - Commands: `split-window-below`, `split-window-right`, `delete-other-windows`.

- [ ] **IPC Server (The AI Interface)**
    - **Crucial:** Background thread listening on Unix Socket (`/tmp/aimax.sock`).
    - Protocol: Text in (Scheme code) -> Text out (Evaluation result).
    - Thread-safety: Command Queue pattern to execute Scheme on the main thread.

- [ ] **Undo/Redo**
    - Implement operation history stack per buffer.

## 2. Scripting API (Scheme)

- [ ] **Buffer Manipulation**
    - Expose `(buffer-insert text)`, `(buffer-text)`, `(buffer-point)`.
    - Expose `(make-buffer name)`, `(set-current-buffer name)`.

- [ ] **Hooks System**
    - `(run-hooks 'hook-name)` implementation.
    - Triggers in Rust: `after-save`, `before-save`, `find-file`.

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
