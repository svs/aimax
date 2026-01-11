# Aimax: The Work OS

**Scheme is the brain. Rust is the muscle.**

## What This Is

Not a text editor. A **programmable workspace** where AI agents are first-class citizens.

```
OLD THINKING: Editor that can talk to AI
NEW THINKING: Work OS where agents operate on buffers
```

You don't edit text. You:
- Curate context (what does the agent see?)
- Describe intent (what do you want?)
- Review changes (accept/reject)
- Orchestrate workflows (agents working together)

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                        SCHEME                           │
│                       (the brain)                       │
│                                                         │
│  • Agent definitions                                   │
│  • Orchestration graphs                                │
│  • Context gathering                                   │
│  • Response handlers                                   │
│  • App packages (recruiting, sidegig, personal)        │
│  • UI layouts                                          │
│  • Everything user-customizable                        │
└─────────────────────────────────────────────────────────┘
                          │
                   futures, callbacks
                          │
┌─────────────────────────────────────────────────────────┐
│                         RUST                            │
│                      (the muscle)                       │
│                                                         │
│  • HTTP client (async, parallel, streaming)            │
│  • Agent executor (tokio, N agents in parallel)        │
│  • Browser automation                                  │
│  • Process management (PTY, IPC)                       │
│  • TUI rendering                                       │
│  • Mac GUI (Swift bridge)                              │
└─────────────────────────────────────────────────────────┘
```

**Scheme never blocks. Scheme never waits. Scheme says what, Rust does how.**

## Agents

Agents are Scheme definitions executed by Rust:

```scheme
(define-agent sourcer
  :provider claude
  :model "claude-sonnet-4-20250514"
  :system "You find candidates for {role}. Be thorough."
  :tools (web-search linkedin-scrape)
  :context (lambda () (buffer-text "job-spec"))
  :on-result (lambda (r) (buffer-append "*candidates*" r)))

(define-agent screener
  :provider claude
  :model "claude-sonnet-4-20250514"
  :system "Evaluate and rank candidates."
  :context (lambda () (buffer-text "*candidates*"))
  :on-result (lambda (r) (buffer-append "*ranked*" r)))
```

## Orchestration

No YAML. No frameworks. Just Scheme expressions:

```scheme
;; Sequential
(-> agent1 agent2 agent3)

;; Parallel
(parallel agent1 agent2 agent3)

;; Human in the loop
(-> sourcer (human-gate "Review candidates?") screener)

;; Conditional
(-> researcher
    (branch
      [(good-results?) writer]
      [else (-> researcher writer)]))

;; Full pipeline
(define recruiting-flow
  (-> (parallel sourcer-linkedin sourcer-github sourcer-referrals)
      (merge)
      screener
      (human-gate)
      outreach))

(run-flow recruiting-flow :role "Senior Rust Engineer")
```

## App Packages

Domain-specific bundles of agents, buffers, and workflows:

```
recruiting/
  agents/
    sourcer.scm
    screener.scm
    outreach.scm
  buffers/
    candidate.scm      ; structured buffer type
    pipeline.scm       ; kanban-style view
  flows/
    full-pipeline.scm
  ui/
    dashboard.scm      ; rich TUI layout

sidegig/
  agents/
    project-pm.scm     ; nags about deadlines
    invoice-gen.scm
  buffers/
    client.scm
    invoice.scm
  flows/
    monthly-billing.scm

personal/
  agents/
    journal-prompter.scm
    weekly-reviewer.scm
  buffers/
    journal-entry.scm
  flows/
    morning-routine.scm
```

## Buffers Are Not Text

Buffers are typed data containers:

```scheme
(define-buffer-type 'candidate
  :fields '((name . string)
            (email . string)
            (stage . (enum sourced screening interview offer rejected))
            (score . number)
            (notes . text))
  :render 'candidate-card
  :actions '(advance reject schedule-call send-email))
```

Rendered as rich TUI (cards, tables, charts), not raw text.

## Browser

Controllable from Scheme:

```scheme
(browser-open "https://linkedin.com/search?keywords=rust")
(browser-wait ".search-results")
(browser-eval "document.querySelectorAll('.profile-link')")
(browser-screenshot)
```

The web is where work happens. The browser is just another buffer.

## The Chat Interface

Not gptel. Not a chat window. Context-aware assistance:

```scheme
(define (chat prompt)
  (let* ((context (gather-context))  ; current buffer, selection, etc.
         (agent (make-agent
                  :system "You help with the current task."
                  :context context)))
    (spawn-agent agent :prompt prompt)))

;; Usage: M-Space → "find candidates like this one"
;; Agent sees: current candidate buffer, your selection, job spec
;; Agent does: searches, returns results to *candidates* buffer
```

## Rust Primitives

What Rust exposes to Scheme:

```scheme
;; HTTP (async, streaming)
(http-post url headers body on-chunk on-done)
(http-get url headers on-done)

;; Browser
(browser-open url)
(browser-eval js-string)
(browser-wait selector)

;; Process
(spawn-process name command args)
(process-send name text)

;; JSON
(json-parse string)
(json-encode obj)

;; Agent execution
(spawn-agent agent-def params)
(cancel-agent agent-id)
```

## Security

Agents generate tool calls, not arbitrary code. Tools are:
- Defined in Scheme (you control what exists)
- Sandboxed (no shell access unless you expose it)
- Auditable (all calls logged)

Destructive operations require `(human-gate)`:
```scheme
(-> file-editor
    (human-gate "Apply these changes?")
    file-writer)
```

## What We're Building

1. **Foundation** (now)
   - Steel Scheme embedded in Rust
   - Buffer system
   - IPC socket
   - Basic TUI

2. **Chat** (next)
   - HTTP client with streaming
   - Claude API from Scheme
   - Simple chat buffer

3. **Agents**
   - Agent definition DSL
   - Tool system
   - Spawn/cancel

4. **Orchestration**
   - Pipeline primitives
   - Parallel execution
   - Human gates

5. **App Packages**
   - Buffer types
   - Rich TUI components
   - Domain-specific agents

6. **Browser**
   - Embedded webkit (Mac)
   - CDP protocol (TUI)
   - Scheme bindings

---

*The goal is not to build an editor. The goal is to build the environment where you and your AI agents get work done.*
