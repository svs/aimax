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

## 6. Agent-Driven Headless Mode (IPC Critique)

The current IPC is a "Remote Eval" port, not a "Control Port." For agents to drive the editor effectively, we must move beyond fire-and-forget strings.

- **The Problem:** 
    - **Observability:** Agents cannot see events (process exit, buffer changes, logs) without polling.
    - **Structure:** `eval_remote` returns unstructured strings. Agents need machine-readable JSON.
    - **Atomicity:** Getting the full editor state currently requires multiple round-trips.
- **The Solution (JSON-RPC + Events):**
    - **Protocol:** Move to JSON-RPC over the existing Unix Socket.
    - **State Snapshot:** Implement a `get_editor_state` method that returns a JSON blob of all buffers, cursors, and processes.
    - **Subscription Mode:** Allow IPC clients to "subscribe" to a stream of events (e.g., `{"event": "buffer_changed", "data": {...}}`).

## Developer Checklist
- [ ] **JSON-RPC IPC:** Implement structured requests/responses over the Unix socket.
- [ ] **Event Bus:** Broadcast buffer changes and process output to IPC subscribers.
- [ ] **State Dump:** Create a `get-state-json` primitive for atomic environment snapshots.
- [ ] Fix IPC Framing (Length-prefix).
- [ ] Implement `portable-pty` integration.
- [ ] Refactor `SharedState` to use `Rope` instead of `String` to reduce allocation.
- [ ] Implement Window Splits (Visual only).

Keep the build green.
