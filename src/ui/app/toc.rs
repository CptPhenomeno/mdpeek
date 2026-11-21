//! Table-of-contents overlay (SPEC §3.10).

use super::App;

#[allow(dead_code)] // stubs until the feat commit
impl App {
    /// Whether an overlay owns the keyboard.
    pub(super) fn overlay_open(&self) -> bool {
        false
    }

    /// Selected entry of the open TOC.
    pub(super) fn toc_selected(&self) -> Option<usize> {
        None
    }

    /// Text of the TOC entries, indented by level.
    pub(super) fn toc_lines(&self) -> Vec<String> {
        Vec::new()
    }
}
