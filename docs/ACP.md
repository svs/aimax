# Agent Client Protocol (ACP) for Aimax

## Overview

ACP (Agent Client Protocol) is a JSON-RPC 2.0 based protocol for communicating with AI coding agents like Claude Code, Gemini CLI, Codex, etc. This enables embedding agents directly into the editor rather than just wrapping their CLI output.

**Key insight:** Unlike plain PTY wrapping (which just shows CLI output), ACP gives structured communication - the editor can:
- Know when the agent wants to read/write files
- Present permission dialogs for sensitive operations
- Provide context (current buffer, selection, errors) directly
- Stream responses and show progress

## Protocol Basics

### Message Format (JSON-RPC 2.0)

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "method_name",
  "params": { ... },
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": { ... }
}
```

**Notification (no response expected):**
```json
{
  "jsonrpc": "2.0",
  "method": "notification_name",
  "params": { ... }
}
```

**Error:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32603,
    "message": "Error description"
  }
}
```

### Communication Flow

1. **Spawn agent process** (e.g., `claude --acp`)
2. **Initialize** - negotiate protocol version, capabilities
3. **Authenticate** - agent-specific auth flow
4. **Create session** - start a conversation
5. **Send prompts** - user messages to agent
6. **Handle requests** - agent asks for permissions, file access
7. **Receive responses** - agent output, completions

## Message Types

### Initialization
```json
// Request
{
  "method": "initialize",
  "params": {
    "protocolVersion": "1.0",
    "clientInfo": { "name": "aimax", "version": "0.1.0" }
  }
}

// Response includes agent capabilities
{
  "result": {
    "protocolVersion": "1.0",
    "capabilities": {
      "supportsImages": false,
      "supportsAudio": false
    }
  }
}
```

### Session Management
```json
// Create new session
{ "method": "session/new", "params": {} }

// Send prompt
{
  "method": "session/prompt",
  "params": {
    "sessionId": "abc123",
    "prompt": "Fix the bug in main.rs"
  }
}
```

### Permission Requests (Agent → Editor)
When agent wants to perform sensitive operations:
```json
// Agent requests permission
{
  "method": "session/requestPermission",
  "params": {
    "sessionId": "abc123",
    "operation": "writeFile",
    "path": "/Users/svs/src/main.rs"
  },
  "id": 42
}

// Editor responds
{
  "id": 42,
  "result": { "granted": true }
}
```

### File Operations (Agent → Editor)
```json
// Agent wants to read a file
{
  "method": "files/read",
  "params": { "path": "/path/to/file.rs" },
  "id": 43
}

// Editor responds with content
{
  "id": 43,
  "result": { "content": "fn main() { ... }" }
}
```

## Aimax Implementation Design

### Rust Layer (Minimal)

```rust
// core/src/acp.rs

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

/// ACP Client manages connection to one agent
pub struct AcpClient {
    process: Process,          // Reuse existing PTY/process system
    next_id: u64,
    pending: HashMap<u64, PendingRequest>,
}

impl AcpClient {
    pub fn send_request(&mut self, method: &str, params: Value) -> u64;
    pub fn send_notification(&mut self, method: &str, params: Value);
    pub fn process_incoming(&mut self) -> Vec<AcpMessage>;
}
```

### Scheme Layer (Heavy Lifting)

```scheme
;; Agent lifecycle
(define (acp-start agent-command)
  "Start an ACP agent and return client handle"
  (let ((client (acp-create-client agent-command)))
    (acp-initialize client)
    (acp-authenticate client)
    client))

;; Session management
(define (acp-new-session client)
  "Create new conversation session"
  (acp-send-request client "session/new" '()))

(define (acp-prompt client session-id prompt)
  "Send prompt to agent"
  (acp-send-request client "session/prompt"
    `((sessionId . ,session-id)
      (prompt . ,prompt))))

;; Permission handling (user-customizable)
(define *acp-permission-handler*
  (lambda (operation path)
    ;; Default: ask user via minibuffer
    (completing-read
      (format "Allow ~a on ~a? " operation path)
      '("yes" "no"))))

;; Request handlers (agent asks editor for things)
(define (acp-handle-request client request)
  (let ((method (assoc-ref request "method")))
    (cond
      ((equal? method "files/read")
       (acp-handle-file-read client request))
      ((equal? method "session/requestPermission")
       (acp-handle-permission client request))
      (else
       (acp-send-error client (assoc-ref request "id")
                       -32601 "Method not found")))))
```

### Integration with Editor

```scheme
;; Claude Code mode
(define (claude-code-mode)
  "Start Claude Code agent in current project"
  (let* ((client (acp-start "claude" "--acp"))
         (session (acp-new-session client)))
    ;; Store in buffer-local variables
    (set! *acp-client* client)
    (set! *acp-session* session)
    ;; Set up keybindings
    (local-set-key "C-c C-c" 'acp-send-region)
    (local-set-key "C-c C-p" 'acp-send-buffer)))

(define (acp-send-region)
  "Send selected region as prompt"
  (let ((text (buffer-substring (region-beginning) (region-end))))
    (acp-prompt *acp-client* *acp-session* text)))
```

## Implementation Phases

### Phase 1: JSON-RPC Infrastructure
- [ ] Add `serde_json` to core
- [ ] Create `core/src/acp.rs` with message types
- [ ] Basic send/receive over existing process system

### Phase 2: ACP Client
- [ ] Connection lifecycle (init, auth, session)
- [ ] Request/response correlation (pending map)
- [ ] Notification handling

### Phase 3: Scheme Bindings
- [ ] `(acp-start command)` - spawn agent
- [ ] `(acp-send-request client method params)`
- [ ] `(acp-send-notification client method params)`
- [ ] Request handlers registered from Scheme

### Phase 4: Permission UI
- [ ] Minibuffer prompts for permission requests
- [ ] Configurable permission policies
- [ ] Logging/audit trail

### Phase 5: Agent Modes
- [ ] `claude-code-mode` for Claude Code
- [ ] `gemini-mode` for Gemini CLI
- [ ] Generic `acp-mode` base

## Comparison: PTY vs ACP

| Feature | PTY (current) | ACP |
|---------|--------------|-----|
| Output | Raw terminal text | Structured JSON |
| Input | Stdin text | JSON-RPC requests |
| File access | Agent uses filesystem | Editor mediates |
| Permissions | None (agent has user perms) | Editor can approve/deny |
| Context | Agent reads files itself | Editor provides buffer content |
| Progress | Parse CLI output | Structured notifications |

## References

- [acp.el](https://github.com/xenodium/acp.el) - Emacs ACP implementation
- [agent-shell](https://github.com/xenodium/agent-shell) - High-level agent UI
- Claude Code `--acp` flag (when available)
- Gemini CLI ACP support
