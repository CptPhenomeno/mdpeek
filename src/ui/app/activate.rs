//! Enter on the focused link or diagram (SPEC §3.6.2).

use super::{App, Opener};
use crate::doc::{LinkKind, classify_link};
use crate::layout::{Focusable, PictureKey};

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
    pub(super) fn activate(&mut self) {
        match self.focused().cloned() {
            None => {}
            Some(Focusable::Diagram { key, .. }) => self.fullscreen_request = Some(key),
            Some(Focusable::Link { target, .. }) => match classify_link(&target) {
                LinkKind::External => {
                    self.message = Some(match self.opener.open(&target) {
                        Ok(()) => "Opened in browser".to_owned(),
                        Err(e) => e,
                    });
                }
                LinkKind::Anchor => self.scroll_to_anchor(target.trim_start_matches('#')),
                LinkKind::Unsupported => {
                    self.message = Some("Local file links are not supported".to_owned());
                }
            },
        }
    }

    /// Scrolls the heading with this slug to the top (as far as the end allows).
    fn scroll_to_anchor(&mut self, slug: &str) {
        match self.layout.index.headings.iter().find(|h| h.slug == slug) {
            Some(h) => self.scroll = h.row.min(self.max_scroll()),
            None => self.message = Some("Anchor not found".to_owned()),
        }
    }
}
