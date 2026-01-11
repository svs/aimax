# Face Architecture: The Bridge Between Structure and Style

This document outlines the architecture for the Face (Theming) system in Aimax. It is designed to combine the scriptability of Emacs with the performance of a native TUI.

## 1. Core Concept: The "Face"
A **Face** is a named collection of display attributes (foreground, background, bold, italic, etc.).
It decouples **Meaning** (e.g., "this is a keyword") from **Appearance** (e.g., "make it purple").

**Key Faces (Emacs Standard):**
- `default`: The root face. All other faces inherit from this (or the terminal defaults).
- `modeline`: The status bar.
- `region`: The selected text.
- `font-lock-*-face`: Syntax highlighting (keyword, string, function-name, etc.).

## 2. Data Structures (Rust)

### `FaceAttributes`
The raw visual data required by the TUI.
```rust
struct FaceAttributes {
    fg: Option<Color>,
    bg: Option<Color>,
    bold: bool,
    italic: bool,
    underline: bool,
}
```

### `FaceRegistry`
The source of truth. Lives in `Editor`.
```rust
struct FaceRegistry {
    // Maps "modeline" -> Attributes
    faces: HashMap<String, FaceAttributes>,
    // Dirty flag for cache invalidation
    version: u64,
}
```

## 3. The Scheme API (Driver)

We expose primitives to manipulate the registry.

`(face-attribute 'face-name :key)`
Gets a value.
`(set-face-attribute 'face-name :key value ...)`
Sets values.

**Example:**
```scheme
(set-face-attribute 'font-lock-keyword-face
    :foreground "#c678dd"
    :weight 'bold)
```

## 4. The Rendering Pipeline (Performance Critical)

We cannot do Hash lookups per-character. We use a **Cached Index**.

1.  **Tree-Sitter Output:** Produces integer IDs (e.g., `5` corresponds to "string").
2.  **The Cache:** `Editor` holds a `Vec<FaceAttributes>` called `syntax_cache`.
    - `syntax_cache[5]` holds the computed style for strings.
3.  **Invalidation:**
    - When `set-face-attribute` is called -> Increment `FaceRegistry.version`.
    - Before Render -> If `cache.version != registry.version`, rebuild the cache.
4.  **Rebuild Logic:**
    - For each Tree-Sitter highlight name (from `HIGHLIGHT_NAMES`):
        1. Resolve Scope Name ("string") -> Face Name ("font-lock-string-face").
        2. Look up Face Name in Registry.
        3. Fallback to `default` face if attributes are missing.
        4. Store in `Vec`.

## 5. Scope Mapping (Glue)
We need to map Tree-Sitter scopes to Face names.
Maintain a `HashMap<String, String>` in Rust (or Scheme):
- "function" -> "font-lock-function-name-face"
- "keyword" -> "font-lock-keyword-face"
- "type" -> "font-lock-type-face"

## Implementation Checklist

1.  [ ] **`core/src/face.rs`**: Define `FaceAttributes` and `FaceRegistry`.
2.  [ ] **`scheme.rs`**: Register `set-face-attribute` primitive.
3.  [ ] **`syntax.rs`**:
    - Remove hardcoded `highlight_color`.
    - Add `resolve_face(scope_id, &FaceRegistry) -> FaceAttributes`.
4.  [ ] **`tui/src/main.rs`**: Update renderer to use `FaceAttributes` instead of raw RGB.
