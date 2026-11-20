//! Overlays (popups) drawn over the document.

use ratatui::{Frame, layout::Rect, widgets::Clear};

/// Clears `area` and returns its inner rectangle (stub: still the raw clear).
pub fn frame(f: &mut Frame, area: Rect, _title: &str) -> Rect {
    f.render_widget(Clear, area);
    area
}
