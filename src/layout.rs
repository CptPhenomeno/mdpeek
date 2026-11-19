//! Document model + pane width -> rows of styled spans (SPEC §3.3, §3.11).

use ratatui::style::Style;

use crate::doc::Document;

#[cfg(test)]
mod tests;

/// Narrowest pane that still shows a document (SPEC §4.6).
pub const MIN_WIDTH: u16 = 20;
/// Blank columns on each side of the content (SPEC §3.3).
pub const MARGIN: u16 = 1;

/// What the pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    /// Pane narrower than `MIN_WIDTH`: show "Window too narrow".
    TooNarrow,
    /// The file has no content: show "Empty document".
    Empty,
}

/// What a row holds; later layout stages add variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// A line of text of the block.
    Text,
    /// A line of a heading of the given level.
    Heading(u8),
    /// A horizontal rule filling the content width.
    Rule,
    /// Spacing between blocks.
    Blank,
}

/// A run of text with one style inside a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSpan {
    pub text: String,
    pub style: Style,
    /// Link target, when the run is part of a link.
    pub link: Option<String>,
}

/// One terminal row of content, without the side margins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    /// Index of the top-level block the row belongs to.
    pub block: usize,
    pub spans: Vec<RowSpan>,
}

impl Row {
    /// Concatenated text of the row.
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub state: State,
    pub rows: Vec<Row>,
}

/// Lay `doc` out for a pane `width` columns wide; `cell` is the cell size in pixels.
pub fn layout(_doc: &Document, _width: u16, _cell: (u16, u16)) -> Layout {
    Layout {
        state: State::Ready,
        rows: Vec::new(),
    }
}
