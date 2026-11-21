//! Left clicks on links and diagrams (SPEC §3.6.4).

use ratatui::layout::Rect;

use crate::layout::Layout;

/// Index in `layout.index.focusables` of the item under the screen cell
/// (`x`, `y`), or `None`. `inner` is the document pane inside its border and
/// `scroll` the first visible row.
pub(super) fn hit_test(
    layout: &Layout,
    inner: Rect,
    scroll: usize,
    x: u16,
    y: u16,
) -> Option<usize> {
    let _ = (layout, inner, scroll, x, y);
    None
}
