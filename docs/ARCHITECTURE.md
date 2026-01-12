# Aimax Architecture

Aimax is a hybrid Rust/Scheme editor designed for the AI age. It combines the performance of a native Rust core with the scriptability of Scheme (Steel).

## High-Level Overview

```
┌─────────────────────────────────────────────────────────┐
│                        TUI (Rust)                       │
│  (ratatui, crossterm)                                   │
│  • Pure rendering layer                                 │
│  • Reads state from Editor struct                       │
│  • No business logic                                    │
└───────────────────────────┬─────────────────────────────┘
                            │ Reads State
                            ▼
┌─────────────────────────────────────────────────────────┐
│                       CORE (Rust)                       │
│                                                         │
│  ┌──────────────┐   ┌──────────────┐   ┌─────────────┐  │
│  │    Buffer    │   │   Process    │   │     LLM     │  │
│  │   (Ropey)    │   │    (PTY)     │   │ (Streaming) │  │
│  └──────────────┘   └──────────────┘   └─────────────┘  │
│                                                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │                Scheme Interpreter                 │  │
│  │                 (Steel Engine)                    │  │
│  └──────────────────────────┬────────────────────────┘  │
└─────────────────────────────┼───────────────────────────┘
                              │ Actions (Push)
                              ▼
                     ┌──────────────────┐
                     │  Editor State    │
                     │ (Buffers, Mode)  │
                     └──────────────────┘
```

## 1. The Core (Rust)

The `core` crate contains the heavy lifting. It is designed to be headless and testable.

### Buffer Management
- **Storage:** Uses `ropey` for efficient text storage (Gap Buffer alternative).
- **State:** Each buffer has content, point (cursor), markers, and file path.
- **Synchronization:** Rust syncs buffer content to Scheme's `SharedState` before execution, allowing Scheme to read (but not write) directly.

### The Scheme Bridge
- **Engine:** Uses `steel-interpreter`.
- **Primitives:** Rust registers functions like `(buffer-insert "text")`, `(find-file!)`, `(start-process ...)`.
- **Actions:** Scheme functions do not mutate Rust state directly. Instead, they push `Action` enums to a queue (`Vec<Action>`).
- **Execution Loop:**
  1. Editor runs Scheme code.
  2. Scheme pushes Actions.
  3. Editor drains Actions queue and applies changes (mutating Buffers, spawning processes, etc.).

## 2. Process System (PTY & Comint)

Aimax implements a robust process system to handle shells, builds, and AI agents.

### Architecture
- **Crate:** `portable-pty` handles OS-level PTY details.
- **Registry:** `ProcessRegistry` holds active processes.
- **Threading:**
  - **Main Thread:** Handles UI and Scheme execution.
  - **Reader Threads:** One thread per process reads stdout/stderr from the PTY and sends `ProcessMessage` to the main thread via channels.
- **Output Handling:**
  - Output is appended to a standard Buffer (e.g., `*shell*`).
  - Ring buffer logic prevents memory leaks (trims top lines).

### Comint (Command Interpreter)
Aimax implements "Comint" logic in Scheme (see `scheme/process.scm` or `commands.scm`):
1.  **Input:** User types in the buffer.
2.  **Enter:** Scheme hook detects `Return` key.
3.  **Send:** `(process-send-string name text)` sends the input to the PTY.
4.  **Output:** PTY echoes back input + output, which appears in the buffer.

This allows `*shell*` buffers to act like real terminals while being editable text buffers.

## 3. Minibuffer & Completion

The Minibuffer is a modal interface for user input.

### Hybrid State Management
- **Rust:** Holds the "View" state (`prompt`, `input`, `matches`, `selected`) for the TUI to render.
- **Scheme:** Holds the "Logic" state and implements behavior (`completing-read`, `find-file`).
- **Sync:** Scheme pushes state updates to Rust via `(minibuffer-activate!)` or implicit syncs (currently transition to explicit push model).

### File Completion
- **Logic:** `scheme/completion.scm` implements fuzzy matching (`nucleo-matcher` binding) and file listing.
- **Descend:** Typing `core/` triggers a directory descend, refreshing candidates.

## 4. AI & Chat Integration

AI is a first-class citizen.

- **Streaming:** LLM responses are streamed via `crossbeam::channel` to the main thread.
- **Rendering:** Tokens are appended to the `*chat*` buffer in real-time.
- **Configuration:** `ChatConfig` (Provider, API Key, Model) is passed from Scheme.

## 5. Face & Theming

- **FaceAttributes:** Struct defining style (fg, bg, bold).
- **FaceRegistry:** Maps names (`font-lock-keyword-face`) to attributes.
- **Scheme Control:** `(set-face-attribute ...)` allows users to theme the editor dynamically.
- **Rendering:** TUI looks up faces during the render pass to style text.

## Directory Structure

- `core/`: Rust logic (Buffers, Scheme, PTY, LLM).
- `tui/`: Ratatui-based frontend.
- `scheme/`: Standard library (behavior, modes, commands).
- `docs/`: Documentation.
