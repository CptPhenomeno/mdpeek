//! Overlays (popups) drawn over the document.

use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Clear},
};

use crate::theme;

/// Draws a bordered overlay box at `area` and returns its inner rectangle.
///
/// `Clear` alone would leave cells with the default background, which
/// translucent terminals render transparent (SPEC §4.2), so the area is
/// cleared and then repainted with the theme page style and border colour.
/// Pictures hidden under the overlay need no extra care: the page behind them
/// is already painted with the theme background.
pub fn frame(f: &mut Frame, area: Rect, title: &str) -> Rect {
    f.render_widget(Clear, area);
    let block = Block::new()
        .borders(Borders::ALL)
        .title(title)
        .style(theme::page())
        .border_style(theme::border(true));
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}
