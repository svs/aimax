# aimax

**Emacs for the AI age.**

A programmable editor where AI agents are first-class citizens. Scheme-scriptable. Rust-powered. No Electron. No cloud. Just you, your buffers, and your agents.

```
 C-x C-f  find-file          M-x ask-ai  "refactor this function"
 C-x b    switch-buffer      M-x shell   run commands, stream output
 C-x C-s  save               M-x agent   launch an autonomous agent
```

## Why?

Every "AI coding tool" is a chatbot bolted onto an editor. You paste code in, get code out, paste it back. That's not integration. That's a workaround.

Aimax starts from scratch: **what if buffers and agents were designed together?**

- Agents read buffers, write buffers, watch buffers
- You see what they see, approve what they do
- Everything is scriptable in Scheme
- Everything runs locally, streams fast

## Demo

```scheme
;; Ask AI about the current buffer
(define (ask-ai)
  (minibuffer-prompt "Ask AI: "
    (lambda (prompt)
      (chat "anthropic" *ai-api-key* "claude-sonnet-4-20250514"
        `(("user" ,(string-append prompt "\n\n" (buffer-text))))))))

;; Stream shell output to a buffer (tail -f, builds, etc.)
(define (tail-logs)
  (buffer-create "*logs*")
  (start-process-simple "*logs*" "tail" '("-f" "/var/log/system.log")))

;; Kill the process when done
(define (kill-logs)
  (kill-process "*logs*"))
```

## What works today

- **Basic editing** — ropey-backed buffers, keymaps, minibuffer with completion. Editing is not a core concern. We just need basic editing because LLMs write most of the stuff here.
- **M-x command system** — extensible command registry with fuzzy completion
- **Process buffers** — PTY-based shells, streaming command output
- **AI chat** — streaming responses from Claude/OpenAI/Ollama
- **Scheme scripting** — Steel Scheme with full access to editor primitives
- **IPC socket** — control from external scripts (`echo '(message "hi")' | nc -U /tmp/aimax.sock`)
- **Tree-sitter syntax highlighting** — fast, accurate, extensible

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      SCHEME (Steel)                     │
│                                                         │
│   commands · keymaps · agents · hooks · init.scm       │
└───────────────────────────┬─────────────────────────────┘
                            │ actions
┌───────────────────────────┴─────────────────────────────┐
│                      RUST (core)                        │
│                                                         │
│   buffers (ropey) · processes (pty) · llm (streaming)  │
│   tree-sitter · ipc · rendering                         │
└─────────────────────────────────────────────────────────┘
```

**Scheme says what. Rust does how.**

The pattern: Scheme pushes `Action`s to a queue. Rust processes them. No callbacks, no async in Scheme, no complexity.

```rust
enum Action {
    Insert(String),
    SwitchBuffer(String),
    StartProcess { name: String, command: String, args: Vec<String> },
    Chat { provider: String, model: String, messages: Vec<Message> },
    // ...
}
```

## Getting started

```bash
git clone https://github.com/anthropics/aimax
cd aimax
cargo run --release
```

Create `~/.aimax/init.scm`:

```scheme
;; Your API key
(define *ai-api-key* "sk-ant-...")

;; Custom keybinding
(global-set-key "C-c a" 'ask-ai)

;; Hook that runs on save
(set! *after-save* (lambda () (message "Saved!")))
```

Talk to it from the terminal:

```bash
echo '(message "hello from outside")' | nc -U /tmp/aimax.sock
echo '(find-file "/tmp/test.txt")' | nc -U /tmp/aimax.sock
```

## Keybindings

| Key | Command |
|-----|---------|
| `C-x C-f` | Find file |
| `C-x C-s` | Save buffer |
| `C-x b` | Switch buffer |
| `C-x k` | Kill buffer |
| `C-g` | Keyboard quit |
| `M-x` | Execute command |
| `C-f/b/n/p` | Forward/back char, next/prev line |
| `C-a/e` | Beginning/end of line |
| `M-f/b` | Forward/back word |

## The vision

Today: a fast, scriptable editor with AI chat.

Tomorrow: **a work OS where AI agents are first-class**.

```scheme
;; Define an agent
(define-agent code-reviewer
  :system "Review code for bugs, security issues, and style."
  :context (buffer-text)
  :tools (read-file write-file run-tests))

;; Orchestrate agents
(-> (get-diff "main")
    code-reviewer
    (human-gate)        ; you approve
    (apply-suggestions))
```

The Emacs insight: everything is a buffer, everything is programmable.

The new insight: **AI agents are just another thing that operates on buffers**.

## Status

Early. Building in public.

- [x] Ropey buffers with full cursor movement
- [x] Steel Scheme scripting
- [x] Emacs-style keymaps (C-x prefix, M-x)
- [x] Minibuffer with fuzzy completion
- [x] Process buffers (PTY + simple streaming)
- [x] AI chat with streaming
- [x] Tree-sitter syntax highlighting
- [x] IPC socket control
- [ ] Multiple windows (C-x 2, C-x 3)
- [ ] LSP client
- [ ] Agent DSL
- [ ] Native Mac GUI

## Philosophy

No Electron. No web tech. No "runs in the cloud."

Just Rust for speed, Scheme for soul, and a deep belief that the best tools are the ones you can take apart and rebuild.

## License

MPL-2.0

---

*Built by humans with AI for humans who work with AI.*
