//! Enter on the focused link or diagram (SPEC §3.6.2).

use super::{App, Opener};
use crate::layout::PictureKey;

impl App {
    /// Replaces the opener (the real one at start-up, fakes in tests).
    pub fn set_opener(&mut self, opener: Box<dyn Opener>) {
        self.opener = opener;
    }

    /// Diagram the user asked to see full screen, until the view takes it.
    pub fn fullscreen_request(&self) -> Option<&PictureKey> {
        self.fullscreen_request.as_ref()
    }

    /// Enter: act on the focused item.
    pub(super) fn activate(&mut self) {}
}
