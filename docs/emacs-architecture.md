# Emacs Architecture Reference

How Emacs handles the core systems we're implementing.

## Buffers

A buffer is text + metadata. Key properties:

- **Text content** - the actual characters (stored in a gap buffer)
- **Point** - cursor position (an integer)
- **Mark** - other end of selection (when active)
- **Markers** - positions that move with text edits
- **Overlays** - regions with properties (highlighting, invisibility, etc.)
- **Text properties** - per-character metadata (face, read-only, etc.)
- **Local variables** - buffer-specific settings (mode, tab-width, etc.)
- **File association** - path on disk, modification state

```
Buffer
├── content: GapBuffer
├── point: usize
├── mark: Option<usize>
├── markers: Vec<Marker>
├── overlays: IntervalTree<Overlay>
├── file_path: Option<PathBuf>
├── modified: bool
├── major_mode: Mode
├── minor_modes: Vec<Mode>
└── local_vars: HashMap<Symbol, Value>
```

## Overlays

Overlays are regions with properties, stored in an interval tree for efficient lookup.

```elisp
(make-overlay start end buffer)
(overlay-put overlay 'face 'highlight)
(overlay-put overlay 'invisible t)
(overlay-put overlay 'before-string "→")
```

Properties:
- `face` - how to display (color, bold, etc.)
- `invisible` - hide the text
- `before-string` / `after-string` - virtual text
- `priority` - which overlay wins
- `modification-hooks` - callbacks on edit
- `keymap` - local keybindings

**Use cases:**
- Syntax highlighting (though font-lock uses text properties)
- Error underlines (flycheck)
- Git diff markers (diff-hl)
- Selection highlighting
- Folding (hideshow)

## Text Properties

Per-character metadata, travels with the text when copied.

```elisp
(put-text-property start end 'face 'keyword buffer)
(get-text-property point 'face)
```

Common properties:
- `face` - display style
- `font-lock-face` - syntax highlighting (font-lock uses this)
- `read-only` - prevent modification
- `invisible` - hide
- `help-echo` - tooltip
- `keymap` - local bindings

**Overlays vs Text Properties:**
- Overlays: transient, don't copy with text, efficient for sparse regions
- Text properties: persistent, copy with text, efficient for dense regions

## Font-Lock (Syntax Highlighting)

Font-lock applies faces based on syntax.

```elisp
(setq font-lock-keywords
  '(("\\bfn\\b" . font-lock-keyword-face)
    ("\\blet\\b" . font-lock-keyword-face)
    ("//.*$" . font-lock-comment-face)))
```

Modern Emacs has multiple backends:
1. **Regex-based** - pattern matching (fast, simple, imprecise)
2. **Syntax tables** - character classes (parentheses, strings, comments)
3. **Tree-sitter** - full parsing (precise, structural)

Font-lock is incremental:
- JIT (just-in-time) - only highlight visible regions
- After changes, re-fontify affected lines

## Minibuffer

The minibuffer is a special buffer for prompts. Key functions:

### `read-from-minibuffer`
Low-level prompt, returns string.

```elisp
(read-from-minibuffer "Prompt: " initial-input keymap read history)
```

### `completing-read`
Prompt with completion.

```elisp
(completing-read "Command: "
  collection        ; list, alist, hash, or function
  predicate         ; filter candidates
  require-match     ; must match?
  initial-input
  history
  default)
```

### `read-file-name`
File path completion with special handling.

```elisp
(read-file-name "Find file: "
  directory         ; start here
  default-filename
  mustmatch         ; file must exist?
  initial
  predicate)
```

**Special behaviors:**
- `~` expands to home directory
- `//` at start goes to root (ignores prior path)
- `~/` resets to home
- Environment variables expand
- Completion cycles through matches
- History navigation with M-p/M-n

## Completion System

### Collection Types
```elisp
;; List
'("apple" "banana" "cherry")

;; Alist (with metadata)
'(("apple" . 1) ("banana" . 2))

;; Function (dynamic)
(lambda (string pred action)
  (cond
    ((eq action nil) (try-completion string candidates pred))
    ((eq action t) (all-completions string candidates pred))
    ((eq action 'lambda) (test-completion string candidates pred))
    ((eq action 'metadata) '(metadata (category . file)))))
```

### Completion Styles
```elisp
completion-styles: '(basic partial-completion flex)
```

- `basic` - prefix matching
- `partial-completion` - "f-b" matches "foo-bar"
- `flex` - fuzzy/substring
- `orderless` - space-separated patterns (popular package)

### Completion UI
Emacs separates completion logic from UI:

- **Default** - shows in *Completions* buffer
- **Icomplete** - inline in minibuffer
- **Vertico** - vertical list (modern, popular)
- **Ivy/Counsel** - full replacement framework
- **Helm** - heavyweight, feature-rich

This separation is key: collection → style → UI

## Keymaps

```elisp
;; Global keymap
(global-set-key (kbd "C-x C-f") 'find-file)

;; Mode keymap
(define-key rust-mode-map (kbd "C-c C-c") 'rust-compile)

;; Keymap inheritance
(set-keymap-parent rust-mode-map prog-mode-map)
```

Lookup order:
1. `overriding-local-map` (rare, for special modes)
2. `overriding-terminal-local-map`
3. Character property keymaps (text properties, overlays)
4. Emulation mode keymaps (evil-mode, god-mode)
5. Minor mode keymaps (in reverse order of activation)
6. Major mode keymap
7. Global keymap

### Prefix Keys
```elisp
;; C-x is a prefix key
(lookup-key global-map (kbd "C-x"))  ; returns a keymap

;; Binding under prefix
(define-key global-map (kbd "C-x C-f") 'find-file)
```

## Commands

Commands are functions with `(interactive)` spec:

```elisp
(defun my-command (arg)
  "Docstring."
  (interactive "p")  ; p = prefix arg as number
  (message "Got %d" arg))
```

Interactive codes:
- `p` - prefix argument (C-u)
- `r` - region (start, end)
- `b` - buffer name (with completion)
- `f` - file name (with completion)
- `s` - string
- `n` - number

## Hooks

```elisp
;; Run functions when mode activates
(add-hook 'rust-mode-hook 'eglot-ensure)

;; Run before/after save
(add-hook 'before-save-hook 'delete-trailing-whitespace)
(add-hook 'after-save-hook 'executable-make-buffer-file-executable-if-script-p)
```

Common hooks:
- `*-mode-hook` - when mode activates
- `find-file-hook` - after opening file
- `before-save-hook` / `after-save-hook`
- `post-command-hook` - after every command
- `window-configuration-change-hook`

## Windows and Frames

- **Frame** = OS window
- **Window** = pane within frame showing a buffer

```elisp
(split-window-below)    ; C-x 2
(split-window-right)    ; C-x 3
(delete-window)         ; C-x 0
(delete-other-windows)  ; C-x 1
(other-window 1)        ; C-x o
```

Window parameters:
- Which buffer it displays
- Point position (separate from buffer's point!)
- Scroll position
- Size

## Display Engine

Emacs redisplay is complex:

1. **Buffer text** → **Glyph matrix**
2. Apply text properties and overlays
3. Apply faces (merge if overlapping)
4. Handle line wrapping, truncation
5. Handle images, special characters
6. Render to terminal or GUI

Key concepts:
- **Face** - collection of display attributes (fg, bg, bold, etc.)
- **Face merging** - overlapping faces combine
- **Display property** - replace text with something else
- **Invisible property** - hide text but keep it

## What We Should Implement

### Phase 1 (Now)
- [x] Buffer with rope (ropey)
- [x] Point/cursor
- [x] Keymap with sequences
- [x] Commands
- [x] Minibuffer with completing-read
- [ ] File path handling in minibuffer

### Phase 2
- [ ] Overlays (interval tree)
- [ ] Text properties (sparse storage)
- [ ] Syntax highlighting (tree-sitter → faces)
- [ ] Multiple buffers

### Phase 3
- [ ] Windows (split panes)
- [ ] Hooks system
- [ ] Modes (major + minor)
- [ ] Scheme scripting (Steel)

### Phase 4
- [ ] Display properties
- [ ] Images in terminal (sixel?)
- [ ] LSP integration
- [ ] Async operations
