//! The full-screen diagram view (SPEC §3.6.3).

use super::App;
use crate::ui::view_worker::{Reply, ViewSink};

impl App {
    /// Where the view sends its render requests (the background worker).
    pub fn set_view_sink(&mut self, _sink: Box<dyn ViewSink>) {}

    /// A render reply of the view arrived; replies of an old generation are dropped.
    pub fn on_view_reply(&mut self, _reply: Reply) {}

    /// Whether the full-screen view is open.
    pub fn view_open(&self) -> bool {
        false
    }
}
