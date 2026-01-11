//! Face system - named collections of display attributes
//!
//! Decouples meaning ("this is a keyword") from appearance ("make it purple").

use std::collections::HashMap;

/// RGB color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parse hex color like "#c678dd" or "c678dd"
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }
}

/// Display attributes for a face
#[derive(Debug, Clone, Default)]
pub struct FaceAttributes {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl FaceAttributes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge with another face, using other's values where set
    pub fn merge(&self, other: &FaceAttributes) -> FaceAttributes {
        FaceAttributes {
            fg: other.fg.or(self.fg),
            bg: other.bg.or(self.bg),
            bold: other.bold || self.bold,
            italic: other.italic || self.italic,
            underline: other.underline || self.underline,
        }
    }
}

/// Registry of all faces - source of truth
pub struct FaceRegistry {
    faces: HashMap<String, FaceAttributes>,
    /// Incremented on any change, for cache invalidation
    pub version: u64,
}

impl FaceRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            faces: HashMap::new(),
            version: 0,
        };
        registry.init_defaults();
        registry
    }

    /// Initialize standard faces with sensible defaults
    fn init_defaults(&mut self) {
        // Default face - everything inherits from this
        self.set("default", FaceAttributes {
            fg: Some(Color::new(220, 220, 220)),
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        });

        // Modeline
        self.set("modeline", FaceAttributes {
            fg: Some(Color::new(255, 255, 255)),
            bg: Some(Color::new(68, 68, 68)),
            bold: false,
            italic: false,
            underline: false,
        });

        // Region (selection)
        self.set("region", FaceAttributes {
            fg: None,
            bg: Some(Color::new(68, 96, 136)),
            bold: false,
            italic: false,
            underline: false,
        });

        // Syntax highlighting faces (One Dark inspired)
        self.set("font-lock-keyword-face", FaceAttributes {
            fg: Some(Color::new(198, 120, 221)), // purple
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        });

        self.set("font-lock-string-face", FaceAttributes {
            fg: Some(Color::new(152, 195, 121)), // green
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        });

        self.set("font-lock-comment-face", FaceAttributes {
            fg: Some(Color::new(92, 99, 112)), // gray
            bg: None,
            bold: false,
            italic: true,
            underline: false,
        });

        self.set("font-lock-function-name-face", FaceAttributes {
            fg: Some(Color::new(97, 175, 239)), // blue
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        });

        self.set("font-lock-type-face", FaceAttributes {
            fg: Some(Color::new(229, 192, 123)), // yellow
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        });

        self.set("font-lock-constant-face", FaceAttributes {
            fg: Some(Color::new(209, 154, 102)), // orange
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        });

        self.set("font-lock-variable-name-face", FaceAttributes {
            fg: Some(Color::new(224, 108, 117)), // red
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        });

        self.set("font-lock-builtin-face", FaceAttributes {
            fg: Some(Color::new(86, 182, 194)), // cyan
            bg: None,
            bold: false,
            italic: false,
            underline: false,
        });
    }

    /// Get a face by name
    pub fn get(&self, name: &str) -> Option<&FaceAttributes> {
        self.faces.get(name)
    }

    /// Get face, falling back to default
    pub fn get_or_default(&self, name: &str) -> FaceAttributes {
        self.faces.get(name)
            .cloned()
            .unwrap_or_else(|| self.faces.get("default").cloned().unwrap_or_default())
    }

    /// Set a face
    pub fn set(&mut self, name: &str, attrs: FaceAttributes) {
        self.faces.insert(name.to_string(), attrs);
        self.version += 1;
    }

    /// Set a single attribute on a face
    pub fn set_attribute(&mut self, name: &str, key: &str, value: &str) -> Result<(), String> {
        let face = self.faces.entry(name.to_string()).or_default();

        match key {
            ":foreground" | "foreground" | "fg" => {
                face.fg = Color::from_hex(value);
            }
            ":background" | "background" | "bg" => {
                face.bg = Color::from_hex(value);
            }
            ":weight" | "weight" => {
                face.bold = value == "bold";
            }
            ":slant" | "slant" => {
                face.italic = value == "italic";
            }
            ":underline" | "underline" => {
                face.underline = value == "t" || value == "true" || value == "line";
            }
            _ => return Err(format!("Unknown face attribute: {}", key)),
        }

        self.version += 1;
        Ok(())
    }
}

/// Maps tree-sitter scope names to face names
pub struct ScopeMap {
    map: HashMap<String, String>,
}

impl ScopeMap {
    pub fn new() -> Self {
        let mut map = HashMap::new();

        // Tree-sitter scope -> Emacs face name
        map.insert("keyword".into(), "font-lock-keyword-face".into());
        map.insert("keyword.control".into(), "font-lock-keyword-face".into());
        map.insert("keyword.function".into(), "font-lock-keyword-face".into());
        map.insert("keyword.operator".into(), "font-lock-keyword-face".into());
        map.insert("keyword.return".into(), "font-lock-keyword-face".into());

        map.insert("string".into(), "font-lock-string-face".into());
        map.insert("string.special".into(), "font-lock-string-face".into());

        map.insert("comment".into(), "font-lock-comment-face".into());
        map.insert("comment.line".into(), "font-lock-comment-face".into());
        map.insert("comment.block".into(), "font-lock-comment-face".into());

        map.insert("function".into(), "font-lock-function-name-face".into());
        map.insert("function.method".into(), "font-lock-function-name-face".into());
        map.insert("function.macro".into(), "font-lock-function-name-face".into());

        map.insert("type".into(), "font-lock-type-face".into());
        map.insert("type.builtin".into(), "font-lock-type-face".into());

        map.insert("constant".into(), "font-lock-constant-face".into());
        map.insert("constant.builtin".into(), "font-lock-constant-face".into());
        map.insert("number".into(), "font-lock-constant-face".into());
        map.insert("boolean".into(), "font-lock-constant-face".into());

        map.insert("variable".into(), "font-lock-variable-name-face".into());
        map.insert("variable.parameter".into(), "font-lock-variable-name-face".into());
        map.insert("property".into(), "font-lock-variable-name-face".into());

        map.insert("operator".into(), "default".into());
        map.insert("punctuation".into(), "default".into());
        map.insert("punctuation.bracket".into(), "default".into());
        map.insert("punctuation.delimiter".into(), "default".into());

        Self { map }
    }

    /// Get face name for a tree-sitter scope
    pub fn get_face(&self, scope: &str) -> &str {
        self.map.get(scope)
            .map(|s| s.as_str())
            .unwrap_or("default")
    }
}

/// Cached face attributes indexed by tree-sitter highlight ID
/// For O(1) lookup during rendering
pub struct FaceCache {
    cache: Vec<FaceAttributes>,
    version: u64,
}

impl FaceCache {
    pub fn new() -> Self {
        Self {
            cache: Vec::new(),
            version: 0,
        }
    }

    /// Get cached attributes for a highlight ID
    pub fn get(&self, id: usize) -> Option<&FaceAttributes> {
        self.cache.get(id)
    }

    /// Rebuild cache if registry has changed
    pub fn rebuild_if_needed(
        &mut self,
        registry: &FaceRegistry,
        scope_map: &ScopeMap,
        highlight_names: &[&str],
    ) {
        if self.version == registry.version && !self.cache.is_empty() {
            return;
        }

        self.cache.clear();
        for scope in highlight_names {
            let face_name = scope_map.get_face(scope);
            let attrs = registry.get_or_default(face_name);
            self.cache.push(attrs);
        }
        self.version = registry.version;
    }
}
