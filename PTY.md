# Process System Architecture (PTY)

This document outlines the architecture for the Process System in Aimax. This is the foundation for:
1.  Running shells/terminals (`M-x shell`).
2.  Running build commands (`cargo build`).
3.  **Crucially:** Integrating AI Agents (`claude`, `opencode`) by wrapping their CLIs.

## 1. The Core Dependency: `portable-pty`
We use `portable-pty` to handle the OS-level complexity of Pseudo-Terminals.
It gives us a `Master` (our side) and a `Slave` (the process side).

## 2. Data Structures (Rust)

### `Process`
Lives in `Editor` (or a dedicated `ProcessRegistry`).
```rust
struct Process {
    id: usize,
    name: String,
    pty_master: Box<dyn MasterPty>,
    // The writer to send input to the process
    writer: Box<dyn Write + Send>,
    // Optional: PID, Status, etc.
}
```

### `ProcessRegistry`
```rust
struct ProcessRegistry {
    procs: HashMap<String, Process>,
    next_id: usize,
}
```

## 3. The Threading Model (Streamer)

When a process starts, we must spawn a **Background Reader Thread**.
The PTY Master is a blocking reader. We cannot read it on the Main Thread.

**The Loop:**
1.  Read chunk `[u8]` from PTY Master.
2.  Feed bytes into the `vt100` parser (held in the `Process` struct).
3.  **The Abstraction:** We don't just dump text. We maintain a virtual screen state.
    - **Crates:** Use `tui-term` for rendering and `vt100` for state.
4.  **Notification:** Send `Action::Redraw` to the Editor. Unlike text buffers, terminal buffers are state machines, not just ropes.

## 4. The Scheme API

`(start-term "name" "cmd" "args")`
- Spawns a full terminal emulator buffer.
- Uses `vt100` backend.

`(start-process "name" "cmd" "args")`
- Spawns a raw process (Comint-style).
- Output is stripped of ANSI and appended to a text buffer.
- Best for "Chat with Agent" or "Build Output".

## 5. Ring Buffer Logic (Safety)
If `claude` dumps 10MB of text, we can't let the `Rope` grow forever.
- In `Editor::process_actions`, handling `InsertProcessOutput`:
    - If `buffer.len_lines() > MAX_LINES` (e.g., 5000):
        - `buffer.delete_range(0, N_lines)` to trim the top.
    - This keeps memory usage stable.

## 6. Implementation Checklist

1.  [ ] Add `portable-pty` to `Cargo.toml`.
2.  [ ] Create `core/src/process.rs`.
    - Define `Process` struct.
    - Implement `spawn` function.
3.  [ ] Update `Editor` to hold a `ProcessRegistry`.
4.  [ ] Implement the `Reader Thread` logic.
5.  [ ] Register Scheme primitives.
6.  [ ] Implement `(start-process)` in `scheme/process.scm`.

## 7. Special Note on Context Injection
For the "Chat with Buffer" feature:
- We will rely on `(process-send-string)` to dump code into the CLI.
- Ensure the PTY writer handles large writes (don't block the main thread if the pipe fills up). Use a thread for writing if necessary, or ensure `portable-pty` handles it non-blockingly.
