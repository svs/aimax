# The Aimax Manifesto

> "The editor is not just a tool for writing code. It is the environment in which we think."

Emacs was a Lisp machine for text. It viewed the world as a stream of characters to be manipulated by functions. It was brilliant, but it was built for a world where "editing" meant typing.

Today, we don't just type. We **observe, transform, and orchestrate**. We manage clouds, debug distributed systems, wrangle data, and guide AI agents.

The "modern" editors (VS Code, Zed) are faster, but they locked the door. They gave us "Extensions" instead of "Core Access." They treat us as Users, not Architects.

**Aimax is the return of the Architect's Editor.**

## The Grand Unified Theory of the Work OS

Aimax is built on four axioms that define a new era of computing interface:

### 1. The Sensor is Structural (Tree-sitter)
The editor must **understand** text, not just display it.
- **Old Way:** Regex highlighting. Fragile, dumb, text-only.
- **Aimax Way:** The buffer is a **Concrete Syntax Tree**.
    - Markdown isn't text; it's a hierarchy of Sections, Tasks, and Blocks.
    - Code isn't lines; it's Functions, Calls, and Definitions.
    - Logs aren't streams; they are Events, Errors, and Timestamps.
- **The Payoff:** The editor provides **Zero-Leak Context** to AI agents. It doesn't send "the file." It sends "the function."

### 2. The Logic is Recursive (Scheme)
The brain of the editor must be live, malleable, and expressive.
- **Old Way:** Compiled plugins or isolated JS extensions.
- **Aimax Way:** **Scheme (Steel)** running in the core.
    - **Observe:** Scheme watches the Tree-sitter AST.
    - **Transform:** Scheme modifies the tree (code mods, refactors).
    - **Query:** Scheme treats buffers as a distributed database (`select task from notes where status = 'pending'`).

### 3. The Muscle is System-Level (Rust)
The engine must be unbreakably fast and safe.
- **Old Way:** Single-threaded C (Emacs) or heavy Electron (VS Code).
- **Aimax Way:** **Rust**.
    - **Async I/O:** 1,000 parallel file reads.
    - **Green Threads:** Agents running in the background without freezing the UI.
    - **Safety:** No segfaults when the scripting layer makes a mistake.

### 4. The Display is Reactive (UI as a Function of State)
Text is the storage format, but it shouldn't be the only visualization.
- **Old Way:** Monospace grids.
- **Aimax Way:** **Components embedded in Text**.
    - A `[ ]` task in Markdown renders as a clickable Checkbox.
    - A `![chart]` node renders as a graphical plot.
    - A `[Deploy]` button in a note triggers a shell script.
- The UI observes the AST and renders the most useful representation: Text, Widget, or Visualization.

## The Workflow: Observe, Transform, Act

We stop "editing files" and start "running workflows."

1.  **Observe:** You open a log file. Tree-sitter parses it instantly. The UI renders "Error" nodes as expandable red widgets.
2.  **Transform:** You ask the AI: "Group these by error type." The AI doesn't grep; it queries the AST and generates a new "Report" buffer.
3.  **Act:** You review the Report. You click a "Fix" button generated next to a recurring error.
4.  **Orchestrate:** Scheme triggers a background shell script to patch the server and deploy.

## The Promise

Aimax is not "Emacs with AI." It is a **Text-Based Operating System**.

- **No Proprietary Formats:** Your data lives in Markdown, JSON, and Source Code.
- **No API Walls:** If it's in a buffer, you can query it. If it's a process, you can pipe it.
- **No Ceiling:** You start by typing. You end by building a custom IDE for your specific life.

**Welcome to the successor.**
