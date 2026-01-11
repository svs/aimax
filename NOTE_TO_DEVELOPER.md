# Architectural Directives from the Old Man

We are building a highly concurrent, agent-ready editor. Do not deviate from these patterns.

## 1. Concurrency Model (The "Snapshot-Actor")

- **Main Thread (King):** Handles TUI rendering, Input, and the Steel Interpreter. **Never Block.**
- **Background Threads:** Handle IPC (Unix Socket), File I/O, and future heavy lifting (LSP, Tree-Sitter).
- **Communication:**
    - **Inbound (To Editor):** `mpsc::channel` (Action Queue).
    - **Outbound (To Workers):** Snapshot the `Rope` (O(1) clone) and send it.

## 2. Scheme Integration (The "Muscle")

Scheme drives the editor via the `Action` queue.
- **Pattern:** Scheme calls `(forward-char)` -> Rust pushes `Action::ForwardChar` -> Editor executes it.
- **State Sync (Current Bottleneck):** `Editor` syncs buffer text to `SharedState` before running Scheme.
    - **WARNING:** This creates a copy of the buffer. It is O(N). Avoid calling `run_scheme` in a tight loop on large files until we optimize this (using `ExternalData`).
    - **Staleness:** Scheme reads the *pre-execution* state. Mutations happen *after* the script returns.

## 3. The IPC System (The "Control Port")

- **Transport:** Unix Domain Socket (`/tmp/aimax.sock`).
- **Protocol:** Currently raw strings.
    - **TODO:** Switch to **Length-Prefixed Framing** (4 byte u32 length + payload) to support multi-line Scheme programs safely.
- **Role:** The IPC thread reads data, wraps it in a "Run Scheme" action, and sends it to the Main Thread.

## 4. Process System (Roadmap Priority)

We are building "Process Buffers" (Comint).
- **Tool:** `portable-pty`.
- **Flow:** `PTY -> Streamer Thread -> Action::AppendText -> Buffer`.
- **Hooks:** Implement `process-output-hook` so Agents can "watch" the stream.

## 5. Theming (Faces)

- **Registry:** `FaceRegistry` in Rust maps names to attributes.
- **Logic:** Scheme defines faces (`set-face-attribute`).
- **Render:** TUI uses a cached index (`Vec<FaceAttributes>`) derived from Tree-Sitter scopes.

## Developer Checklist
- [ ] Fix IPC Framing (Length-prefix).
- [ ] Implement `portable-pty` integration.
- [ ] Refactor `SharedState` to use `Rope` instead of `String` to reduce allocation.
- [ ] Implement Window Splits (Visual only).

Keep the build green.
