# Aimax Scheme API Reference

This document describes the Scheme primitives and functions available in Aimax.

## Buffer Operations

### Reading

- **`(buffer-text)`** -> `string`
  Returns the complete content of the current buffer.
- **`(buffer-line n)`** -> `string`
  Returns the content of the n-th line (0-indexed).
- **`(buffer-line-count)`** -> `integer`
  Returns the total number of lines in the current buffer.
- **`(buffer-name)`** -> `string`
  Returns the name of the current buffer.
- **`(buffer-names)`** -> `list<string>`
  Returns a list of all open buffer names.
- **`(buffer-file-paths)`** -> `list<string>`
  Returns a list of file paths associated with open buffers.
- **`(buffer-modified?)`** -> `bool`
  Returns `#t` if the current buffer has unsaved changes.
- **`(buffer-modified-p buffer-name)`** -> `bool`
  Returns `#t` if the named buffer has unsaved changes.
- **`(major-mode)`** -> `string`
  Returns the major mode identifier for the current buffer (e.g., "rust", "scheme", "text").

### Writing / Actions

- **`(buffer-insert text)`**
  Inserts `text` at the current point (cursor).
- **`(buffer-delete)`**
  Deletes the character before the point (backspace).
- **`(buffer-create name)`**
  Creates a new empty buffer with the given name.
- **`(buffer-switch name)`**
  Switches view to the buffer with the given name.
- **`(buffer-open path)`**
  Opens the file at `path` in a new buffer (or switches to it if already open).
- **`(save-buffer!)`**
  Saves the current buffer to its associated file.
- **`(kill-buffer!)`**
  Closes the current buffer.
- **`(kill-buffer-named! name)`**
  Closes the specified buffer.
- **`(message text)`**
  Displays a status message in the echo area.
- **`(newline!)`**
  Inserts a newline character.

### Local Variables

- **`(buffer-local key)`** -> `string`
  Gets the value of a buffer-local variable for the *current* buffer.
- **`(buffer-local-p buffer-name key)`** -> `string`
  Gets the value of a buffer-local variable for a *specific* buffer.

## Movement

- **`(forward-char)`** / **`(backward-char)`**
  Moves cursor one character forward/backward.
- **`(forward-word)`** / **`(backward-word)`**
  Moves cursor one word forward/backward.
- **`(next-line)`** / **`(previous-line)`**
  Moves cursor one line down/up.
- **`(beginning-of-line)`** / **`(end-of-line)`**
  Moves cursor to start/end of current line.
- **`(beginning-of-buffer)`** / **`(end-of-buffer)`**
  Moves cursor to start/end of buffer.
- **`(goto-line n)`**
  Moves cursor to the start of line `n`.
- **`(set-point n)`**
  Moves cursor to absolute character index `n`.

## Minibuffer

These primitives control the minibuffer (prompt area).

- **`(minibuffer-activate! prompt)`**
  Activates the minibuffer with the given prompt string.
- **`(minibuffer-active?)`** -> `bool`
  Returns `#t` if minibuffer is currently active.
- **`(minibuffer-get-prompt)`** -> `string`
  Returns current prompt.
- **`(minibuffer-get-value)`** -> `string`
  Returns current user input in minibuffer.
- **`(minibuffer-get-matches)`** -> `list<string>`
  Returns current list of completion matches.
- **`(minibuffer-get-selected)`** -> `integer`
  Returns index of currently selected match.
- **`(minibuffer-set-state! active prompt input matches selected)`**
  **Low-level:** Pushes complete state snapshot to Rust UI. Used by `(minibuffer-sync)`.

### High-Level Minibuffer Logic (`scheme/minibuffer.scm`)

- **`(minibuffer-prompt prompt callback)`**
  Starts a generic prompt. Calls `callback` with result on Enter.
- **`(minibuffer-find-file cwd callback)`**
  Starts file completion prompt starting at `cwd`.

## Processes (Comint / PTY)

- **`(start-process name command args)`**
  Starts an interactive PTY process (e.g., shell). Output is streamed to buffer `name`.
- **`(start-process-simple name command args)`**
  Starts a process with piped stdout/stderr (non-interactive, e.g., tail -f).
- **`(process-send-string name text)`**
  Sends text to the stdin of the named process.
- **`(kill-process name)`**
  Terminates the named process.
- **`(process-running? name)`** -> `bool`
  Returns `#t` if process is active.
- **`(process-list)`** -> `list<string>`
  Returns names of all running processes.
- **`(shell-command-to-string cmd)`** -> `string`
  Runs `cmd` synchronously using `/bin/sh` and returns stdout+stderr.

## AI / LLM

- **`(ai-chat provider api-key model system-prompt prompt)`**
  Starts a streaming chat response in `*chat*` buffer.
  - `provider`: "anthropic", "openai", "ollama", etc.
- **`(ai-chat-messages provider api-key model system-prompt messages)`**
  Same as above but takes a list of message pairs `(("user" "hello") ("assistant" "hi"))`.

## Face / Theming

- **`(set-face-attribute face-name key value)`**
  Sets a visual attribute for a face.
  - `key`: ":foreground", ":background", ":weight", ":slant", ":underline"
  - `value`: Hex color ("#ff0000"), "bold", "italic", etc.

## File System Primitives

- **`(ls path)`** -> `list<string>`
  List files in directory (skips hidden).
- **`(ls-all path)`** -> `list<string>`
  List all files.
- **`(file-exists? path)`** -> `bool`
- **`(directory? path)`** -> `bool`
- **`(read-file path)`** -> `string`
- **`(write-file path content)`**
- **`(expand-path path base-dir)`** -> `string`
  Resolves `~` and relative paths.
- **`(fuzzy-match pattern candidates)`** -> `list<string>`
  Filters list of strings against pattern using fuzzy matching.

## Environment

- **`(getenv name)`** -> `string`
  Returns value of environment variable.
