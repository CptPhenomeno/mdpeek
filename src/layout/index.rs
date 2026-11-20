//! Positions in the laid-out document: headings, focusables and searchable text
//! (SPEC §3.6, §3.9.2, §3.10, §4.14).

use std::ops::Range;

use super::PictureKey;

#[cfg(test)]
mod tests;

/// A heading with rows in the layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingEntry {
    pub level: u8,
    /// Plain text of the heading.
    pub text: String,
    /// Anchor slug, as `Document::heading_slugs` gives it.
    pub slug: String,
    /// First row of the heading.
    pub row: usize,
}

/// A run of one row: display columns `start..end` inside the content area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fragment {
    pub row: usize,
    pub start: u16,
    pub end: u16,
}

/// Something Tab can focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Focusable {
    /// One link occurrence, with a fragment per row it wraps over.
    Link {
        target: String,
        fragments: Vec<Fragment>,
    },
    /// A diagram, covering the rows of its slot.
    Diagram { key: PictureKey, rows: Range<usize> },
}

/// The text of one logical block and where each part of it is on screen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextBlock {
    /// Logical text: wrapping undone, whitespace collapsed, no markers or borders.
    pub text: String,
    pub(super) parts: Vec<Part>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Part {
    /// Byte offset in `text` where this part starts.
    pub offset: usize,
    pub len: usize,
    pub row: usize,
    /// Display column where this part starts.
    pub col: u16,
}

impl TextBlock {
    /// Row fragments covered by the byte range `range` of `text`.
    pub fn fragments(&self, range: Range<usize>) -> Vec<Fragment> {
        let _ = range;
        Vec::new()
    }
}

/// Everything the layout can be asked about positions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Index {
    pub headings: Vec<HeadingEntry>,
    /// In document order.
    pub focusables: Vec<Focusable>,
    pub texts: Vec<TextBlock>,
}
