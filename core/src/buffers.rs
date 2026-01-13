//! Shared Buffer Store
//!
//! Provides shared access to buffers between Editor and Scheme.
//! - Editor owns the BufferStore
//! - Scheme gets Arc clone for direct reads
//! - Writes still go through action queue for safety

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::Buffer;

/// Shared buffer storage accessible by both Editor and Scheme
#[derive(Clone)]
pub struct BufferStore {
    /// All buffers keyed by name
    buffers: Arc<RwLock<HashMap<String, Arc<RwLock<Buffer>>>>>,
    /// Current buffer name (Scheme can update this immediately)
    current: Arc<RwLock<String>>,
}

impl BufferStore {
    /// Create a new BufferStore with a scratch buffer
    pub fn new() -> Self {
        let mut buffers = HashMap::new();
        let scratch = Buffer::new("*scratch*");
        buffers.insert("*scratch*".to_string(), Arc::new(RwLock::new(scratch)));

        BufferStore {
            buffers: Arc::new(RwLock::new(buffers)),
            current: Arc::new(RwLock::new("*scratch*".to_string())),
        }
    }

    /// Get current buffer name
    pub fn current_name(&self) -> String {
        self.current.read().unwrap().clone()
    }

    /// Set current buffer name (immediate, used by Scheme buffer-switch)
    pub fn set_current(&self, name: &str) -> bool {
        // Only switch if buffer exists
        if self.buffers.read().unwrap().contains_key(name) {
            *self.current.write().unwrap() = name.to_string();
            true
        } else {
            false
        }
    }

    /// Get list of all buffer names
    pub fn names(&self) -> Vec<String> {
        self.buffers.read().unwrap().keys().cloned().collect()
    }

    /// Check if a buffer exists
    pub fn exists(&self, name: &str) -> bool {
        self.buffers.read().unwrap().contains_key(name)
    }

    /// Create a new buffer, returns true if created (false if already exists)
    pub fn create(&self, name: &str) -> bool {
        let mut buffers = self.buffers.write().unwrap();
        if buffers.contains_key(name) {
            false
        } else {
            buffers.insert(name.to_string(), Arc::new(RwLock::new(Buffer::new(name))));
            true
        }
    }

    /// Get a buffer by name (returns Arc for direct access)
    pub fn get(&self, name: &str) -> Option<Arc<RwLock<Buffer>>> {
        self.buffers.read().unwrap().get(name).cloned()
    }

    /// Get current buffer
    pub fn current(&self) -> Option<Arc<RwLock<Buffer>>> {
        let name = self.current_name();
        self.get(&name)
    }

    /// Insert or replace a buffer
    pub fn insert(&self, buffer: Buffer) {
        let name = buffer.name.clone();
        self.buffers.write().unwrap().insert(name, Arc::new(RwLock::new(buffer)));
    }

    /// Remove a buffer, returns true if removed
    /// Won't remove the last buffer
    pub fn remove(&self, name: &str) -> bool {
        let mut buffers = self.buffers.write().unwrap();
        if buffers.len() <= 1 {
            return false;
        }
        if buffers.remove(name).is_some() {
            // If we removed the current buffer, switch to another
            let current = self.current_name();
            if current == name {
                if let Some(other_name) = buffers.keys().next() {
                    *self.current.write().unwrap() = other_name.clone();
                }
            }
            true
        } else {
            false
        }
    }

    /// Execute a closure with read access to current buffer
    pub fn with_current<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&Buffer) -> R,
    {
        let buf_arc = self.current()?;
        let buf = buf_arc.read().ok()?;
        Some(f(&buf))
    }

    /// Execute a closure with write access to current buffer
    pub fn with_current_mut<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut Buffer) -> R,
    {
        let buf_arc = self.current()?;
        let mut buf = buf_arc.write().ok()?;
        Some(f(&mut buf))
    }

    /// Execute a closure with read access to a named buffer
    pub fn with_buffer<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Buffer) -> R,
    {
        let buf_arc = self.get(name)?;
        let buf = buf_arc.read().ok()?;
        Some(f(&buf))
    }

    /// Execute a closure with write access to a named buffer
    pub fn with_buffer_mut<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&mut Buffer) -> R,
    {
        let buf_arc = self.get(name)?;
        let mut buf = buf_arc.write().ok()?;
        Some(f(&mut buf))
    }

    /// Iterate over all buffers (for sync/state purposes)
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&str, &Buffer),
    {
        let buffers = self.buffers.read().unwrap();
        for (name, buf_arc) in buffers.iter() {
            if let Ok(buf) = buf_arc.read() {
                f(name, &buf);
            }
        }
    }

    /// Get buffer count
    pub fn len(&self) -> usize {
        self.buffers.read().unwrap().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.buffers.read().unwrap().is_empty()
    }
}

impl Default for BufferStore {
    fn default() -> Self {
        Self::new()
    }
}
