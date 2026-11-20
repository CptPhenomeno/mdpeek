//! Focus over links and diagrams (SPEC §3.6.1).

use super::App;
use crate::layout::{Focusable, PictureKey};

/// Stable identity of a focusable, independent of row numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusId {
    /// `ordinal`-th link of the top-level block `block`.
    Link {
        block: usize,
        ordinal: usize,
    },
    Diagram(PictureKey),
}

impl App {
    /// The focused item, if any.
    pub fn focused(&self) -> Option<&Focusable> {
        self.layout.index.focusables.first()
    }

    /// Tab (`forward`) or Shift+Tab: move the focus, wrapping around.
    pub(super) fn focus_step(&mut self, _forward: bool) {}

    /// Esc: focus nothing.
    pub(super) fn clear_focus(&mut self) {}
}
