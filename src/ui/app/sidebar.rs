//! Sidebar pane: visibility, focus switching and opening files (SPEC §3.2, §3.7).

use super::App;
use crate::tree::Start;

impl App {
    /// Applies the start-up decision: root, visibility, tree and open file.
    pub fn set_start(&mut self, _start: Start) {}

    /// Whether the sidebar is shown.
    pub fn sidebar_visible(&self) -> bool {
        false
    }

    /// Whether the sidebar (not the document) has the keyboard focus.
    pub fn sidebar_focused(&self) -> bool {
        false
    }

    /// Path shown in the status bar, relative to the root.
    pub fn rel_path(&self) -> &str {
        &self.rel_path
    }

    /// Columns available to the document inside its border.
    pub fn document_width(&self) -> u16 {
        self.pane_width_of(self.size.0)
    }
}
