//! Window management
//!
//! Each window displays a buffer with its own point (cursor) and scroll position.
//! Windows are arranged in a tree layout with horizontal and vertical splits.

/// Unique identifier for a window
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub usize);

/// A window displaying a buffer
#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    pub buffer_idx: usize,    // Index into Editor's buffer list
    pub point: usize,         // Cursor position (per-window)
    pub scroll_top: usize,    // First visible line
}

impl Window {
    pub fn new(id: WindowId, buffer_idx: usize) -> Self {
        Window {
            id,
            buffer_idx,
            point: 0,
            scroll_top: 0,
        }
    }
}

/// Layout tree for window arrangement
#[derive(Debug, Clone)]
pub enum WindowLayout {
    /// Single window
    Leaf(WindowId),
    /// Horizontal split (windows stacked vertically, like C-x 2)
    HSplit {
        children: Vec<WindowLayout>,
        ratios: Vec<f32>,
    },
    /// Vertical split (windows side by side, like C-x 3)
    VSplit {
        children: Vec<WindowLayout>,
        ratios: Vec<f32>,
    },
}

impl WindowLayout {
    /// Get all window IDs in this layout (left-to-right, top-to-bottom)
    pub fn window_ids(&self) -> Vec<WindowId> {
        match self {
            WindowLayout::Leaf(id) => vec![*id],
            WindowLayout::HSplit { children, .. } | WindowLayout::VSplit { children, .. } => {
                children.iter().flat_map(|c| c.window_ids()).collect()
            }
        }
    }

    /// Count windows in this layout
    pub fn window_count(&self) -> usize {
        match self {
            WindowLayout::Leaf(_) => 1,
            WindowLayout::HSplit { children, .. } | WindowLayout::VSplit { children, .. } => {
                children.iter().map(|c| c.window_count()).sum()
            }
        }
    }

    /// Find the parent layout containing a window and its index
    fn find_parent(&mut self, target: WindowId) -> Option<(&mut Vec<WindowLayout>, &mut Vec<f32>, usize)> {
        match self {
            WindowLayout::Leaf(_) => None,
            WindowLayout::HSplit { children, ratios } | WindowLayout::VSplit { children, ratios } => {
                // Check if target is a direct child
                for (i, child) in children.iter().enumerate() {
                    if let WindowLayout::Leaf(id) = child {
                        if *id == target {
                            return Some((children, ratios, i));
                        }
                    }
                }
                // Recursively search children
                for child in children.iter_mut() {
                    if let Some(result) = child.find_parent(target) {
                        return Some(result);
                    }
                }
                None
            }
        }
    }

    /// Split a window horizontally (create window below)
    pub fn split_horizontal(&mut self, target: WindowId, new_id: WindowId) -> bool {
        match self {
            WindowLayout::Leaf(id) if *id == target => {
                // Replace this leaf with an HSplit containing both
                *self = WindowLayout::HSplit {
                    children: vec![
                        WindowLayout::Leaf(target),
                        WindowLayout::Leaf(new_id),
                    ],
                    ratios: vec![0.5, 0.5],
                };
                true
            }
            WindowLayout::HSplit { children, ratios } => {
                // Check if target is a direct child leaf
                for (i, child) in children.iter_mut().enumerate() {
                    if let WindowLayout::Leaf(id) = child {
                        if *id == target {
                            // Insert new window after this one
                            let old_ratio = ratios[i] / 2.0;
                            ratios[i] = old_ratio;
                            children.insert(i + 1, WindowLayout::Leaf(new_id));
                            ratios.insert(i + 1, old_ratio);
                            return true;
                        }
                    }
                }
                // Recurse into children
                for child in children.iter_mut() {
                    if child.split_horizontal(target, new_id) {
                        return true;
                    }
                }
                false
            }
            WindowLayout::VSplit { children, .. } => {
                // Must recurse into children
                for child in children.iter_mut() {
                    if child.split_horizontal(target, new_id) {
                        return true;
                    }
                }
                false
            }
            WindowLayout::Leaf(_) => false,
        }
    }

    /// Split a window vertically (create window to the right)
    pub fn split_vertical(&mut self, target: WindowId, new_id: WindowId) -> bool {
        match self {
            WindowLayout::Leaf(id) if *id == target => {
                // Replace this leaf with a VSplit containing both
                *self = WindowLayout::VSplit {
                    children: vec![
                        WindowLayout::Leaf(target),
                        WindowLayout::Leaf(new_id),
                    ],
                    ratios: vec![0.5, 0.5],
                };
                true
            }
            WindowLayout::VSplit { children, ratios } => {
                // Check if target is a direct child leaf
                for (i, child) in children.iter_mut().enumerate() {
                    if let WindowLayout::Leaf(id) = child {
                        if *id == target {
                            // Insert new window after this one
                            let old_ratio = ratios[i] / 2.0;
                            ratios[i] = old_ratio;
                            children.insert(i + 1, WindowLayout::Leaf(new_id));
                            ratios.insert(i + 1, old_ratio);
                            return true;
                        }
                    }
                }
                // Recurse into children
                for child in children.iter_mut() {
                    if child.split_vertical(target, new_id) {
                        return true;
                    }
                }
                false
            }
            WindowLayout::HSplit { children, .. } => {
                // Must recurse into children
                for child in children.iter_mut() {
                    if child.split_vertical(target, new_id) {
                        return true;
                    }
                }
                false
            }
            WindowLayout::Leaf(_) => false,
        }
    }

    /// Remove a window from the layout, returning true if removed
    /// If the parent split has only one child left, it collapses
    pub fn remove_window(&mut self, target: WindowId) -> bool {
        match self {
            WindowLayout::Leaf(id) => *id == target, // Signal to parent to remove us
            WindowLayout::HSplit { children, ratios } | WindowLayout::VSplit { children, ratios } => {
                // Find and remove the target
                let mut remove_idx = None;
                for (i, child) in children.iter().enumerate() {
                    if let WindowLayout::Leaf(id) = child {
                        if *id == target {
                            remove_idx = Some(i);
                            break;
                        }
                    }
                }

                if let Some(idx) = remove_idx {
                    children.remove(idx);
                    ratios.remove(idx);
                    // Normalize ratios
                    let sum: f32 = ratios.iter().sum();
                    if sum > 0.0 {
                        for r in ratios.iter_mut() {
                            *r /= sum;
                        }
                    }
                    // If only one child left, we'll collapse at the parent level
                    return true;
                }

                // Recurse into children
                for (i, child) in children.iter_mut().enumerate() {
                    if child.remove_window(target) {
                        // Child was removed or collapsed
                        // If child is now a single-child split, collapse it
                        if let WindowLayout::HSplit { children: c, .. } | WindowLayout::VSplit { children: c, .. } = child {
                            if c.len() == 1 {
                                let collapsed = c.remove(0);
                                children[i] = collapsed;
                            }
                        }
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Collapse single-child splits after removal
    pub fn collapse_single_children(&mut self) {
        match self {
            WindowLayout::Leaf(_) => {}
            WindowLayout::HSplit { children, ratios } | WindowLayout::VSplit { children, ratios } => {
                // First recurse
                for child in children.iter_mut() {
                    child.collapse_single_children();
                }
                // Then check if we should collapse
                if children.len() == 1 {
                    // This case is handled by the parent
                }
            }
        }
    }
}

/// Manages windows and their arrangement
pub struct WindowManager {
    windows: Vec<Window>,
    layout: WindowLayout,
    selected: WindowId,
    next_id: usize,
}

impl WindowManager {
    pub fn new(initial_buffer_idx: usize) -> Self {
        let initial_id = WindowId(0);
        let window = Window::new(initial_id, initial_buffer_idx);

        WindowManager {
            windows: vec![window],
            layout: WindowLayout::Leaf(initial_id),
            selected: initial_id,
            next_id: 1,
        }
    }

    /// Get the currently selected window
    pub fn selected_window(&self) -> &Window {
        self.windows.iter()
            .find(|w| w.id == self.selected)
            .expect("Selected window must exist")
    }

    /// Get the currently selected window (mutable)
    pub fn selected_window_mut(&mut self) -> &mut Window {
        self.windows.iter_mut()
            .find(|w| w.id == self.selected)
            .expect("Selected window must exist")
    }

    /// Get a window by ID
    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    /// Get a window by ID (mutable)
    pub fn window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    /// Get all windows
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// Get the layout
    pub fn layout(&self) -> &WindowLayout {
        &self.layout
    }

    /// Get selected window ID
    pub fn selected_id(&self) -> WindowId {
        self.selected
    }

    /// Split the selected window horizontally (C-x 2)
    pub fn split_below(&mut self) -> WindowId {
        let new_id = WindowId(self.next_id);
        self.next_id += 1;

        let selected = self.selected_window();
        let new_window = Window::new(new_id, selected.buffer_idx);

        self.windows.push(new_window);
        self.layout.split_horizontal(self.selected, new_id);

        new_id
    }

    /// Split the selected window vertically (C-x 3)
    pub fn split_right(&mut self) -> WindowId {
        let new_id = WindowId(self.next_id);
        self.next_id += 1;

        let selected = self.selected_window();
        let new_window = Window::new(new_id, selected.buffer_idx);

        self.windows.push(new_window);
        self.layout.split_vertical(self.selected, new_id);

        new_id
    }

    /// Delete the selected window (C-x 0)
    /// Returns false if it's the only window
    pub fn delete_window(&mut self) -> bool {
        if self.windows.len() <= 1 {
            return false;
        }

        let target = self.selected;

        // Find the next window to select
        let window_ids = self.layout.window_ids();
        let current_idx = window_ids.iter().position(|&id| id == target).unwrap_or(0);
        let next_idx = if current_idx > 0 { current_idx - 1 } else { 1 };
        let next_selected = window_ids.get(next_idx).copied().unwrap_or(window_ids[0]);

        // Remove from layout
        self.layout.remove_window(target);

        // Handle case where layout collapsed to a single child
        if let WindowLayout::HSplit { children, .. } | WindowLayout::VSplit { children, .. } = &mut self.layout {
            if children.len() == 1 {
                self.layout = children.remove(0);
            }
        }

        // Remove from windows list
        self.windows.retain(|w| w.id != target);

        // Update selection
        self.selected = next_selected;

        true
    }

    /// Delete all other windows (C-x 1)
    pub fn delete_other_windows(&mut self) {
        let selected = self.selected;

        // Keep only the selected window
        self.windows.retain(|w| w.id == selected);

        // Reset layout to single window
        self.layout = WindowLayout::Leaf(selected);
    }

    /// Cycle to next window (C-x o)
    pub fn other_window(&mut self, count: i32) {
        let window_ids = self.layout.window_ids();
        if window_ids.is_empty() {
            return;
        }

        let current_idx = window_ids.iter()
            .position(|&id| id == self.selected)
            .unwrap_or(0) as i32;

        let len = window_ids.len() as i32;
        let new_idx = ((current_idx + count) % len + len) % len;

        self.selected = window_ids[new_idx as usize];
    }

    /// Get number of windows
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_window() {
        let wm = WindowManager::new(0);
        assert_eq!(wm.window_count(), 1);
        assert_eq!(wm.selected_window().buffer_idx, 0);
    }

    #[test]
    fn test_split_below() {
        let mut wm = WindowManager::new(0);
        let new_id = wm.split_below();

        assert_eq!(wm.window_count(), 2);
        assert!(wm.window(new_id).is_some());

        // Both should show buffer 0
        assert_eq!(wm.selected_window().buffer_idx, 0);
        assert_eq!(wm.window(new_id).unwrap().buffer_idx, 0);
    }

    #[test]
    fn test_split_right() {
        let mut wm = WindowManager::new(0);
        let new_id = wm.split_right();

        assert_eq!(wm.window_count(), 2);
        assert!(wm.window(new_id).is_some());
    }

    #[test]
    fn test_other_window() {
        let mut wm = WindowManager::new(0);
        let w1 = wm.selected_id();
        let w2 = wm.split_below();

        assert_eq!(wm.selected_id(), w1);

        wm.other_window(1);
        assert_eq!(wm.selected_id(), w2);

        wm.other_window(1);
        assert_eq!(wm.selected_id(), w1);

        // Negative wrap
        wm.other_window(-1);
        assert_eq!(wm.selected_id(), w2);
    }

    #[test]
    fn test_delete_window() {
        let mut wm = WindowManager::new(0);
        let _w2 = wm.split_below();

        assert_eq!(wm.window_count(), 2);

        assert!(wm.delete_window());
        assert_eq!(wm.window_count(), 1);

        // Can't delete last window
        assert!(!wm.delete_window());
        assert_eq!(wm.window_count(), 1);
    }

    #[test]
    fn test_delete_other_windows() {
        let mut wm = WindowManager::new(0);
        wm.split_below();
        wm.split_right();

        assert_eq!(wm.window_count(), 3);

        wm.delete_other_windows();
        assert_eq!(wm.window_count(), 1);
    }

    #[test]
    fn test_complex_layout() {
        let mut wm = WindowManager::new(0);

        // Create: [A | B]
        //         [  C  ]
        let _b = wm.split_right();  // [A | B]
        wm.split_below();           // A is now split with C below

        assert_eq!(wm.window_count(), 3);

        let ids = wm.layout().window_ids();
        assert_eq!(ids.len(), 3);
    }
}
