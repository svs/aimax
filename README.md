# aimax

**A programmable workspace for humans and AI agents.**

```
┌─────────────────────────────────────────────────────────┐
│                        SCHEME                           │
│                       (the brain)                       │
│                                                         │
│   agents · orchestration · context · customization     │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────┴─────────────────────────────┐
│                         RUST                            │
│                      (the muscle)                       │
│                                                         │
│   http · streaming · browser · processes · rendering   │
└─────────────────────────────────────────────────────────┘
```

## What is this?

Not a text editor. A **work OS** where AI agents are first-class citizens.

You don't type code. You:
- **Curate context** — what does the agent see?
- **Describe intent** — what do you want?
- **Review changes** — accept or reject
- **Orchestrate** — agents working together

## The idea

```scheme
;; Define an agent
(define-agent sourcer
  :system "You find candidates for {role}."
  :tools (web-search linkedin-scrape)
  :context (buffer-text "job-spec"))

;; Run a pipeline
(-> (parallel sourcer-linkedin sourcer-github)
    (merge)
    screener
    (human-gate)
    outreach)
```

No YAML. No frameworks. Just Scheme.

## Architecture

**Scheme says what. Rust does how.**

Scheme handles:
- Agent definitions
- Orchestration logic
- Context gathering
- User customization

Rust handles:
- Async HTTP + streaming
- Parallel agent execution
- Browser automation
- Terminal rendering

Scheme never blocks. Scheme never waits.

## App packages

Bundle agents, buffers, and workflows for different domains:

```
recruiting/     → source, screen, outreach
sidegig/        → clients, invoices, projects
personal/       → journal, habits, finance
```

Each package defines buffer types (not just text), views (cards, tables, dashboards), and domain-specific AI agents.

## Status

Early. Building in public.

- [x] Buffer system (ropey)
- [x] Scheme scripting (Steel)
- [x] IPC socket
- [x] Process buffers (PTY)
- [x] Syntax highlighting (tree-sitter)
- [ ] HTTP streaming
- [ ] Agent DSL
- [ ] Orchestration primitives
- [ ] Browser integration

## Running

```bash
cd tui && cargo run
```

Talk to it via socket:
```bash
echo '(message "hello")' | nc -U /tmp/aimax.sock
```

## Philosophy

The Emacs insight: everything is a buffer, everything is programmable.

The new insight: AI agents are just another thing that operates on buffers.

The browser is a buffer. The terminal is a buffer. The candidate pipeline is a buffer. The AI agent reads buffers, writes buffers, transforms buffers.

You orchestrate. The agents execute.

## License

MIT

---

*The goal is not to build an editor.*
*The goal is to build the environment where you and your AI agents get work done.*
