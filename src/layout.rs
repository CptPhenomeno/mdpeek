//! Document model + pane width -> rows of styled spans (SPEC §3.3, §3.11).

use ratatui::style::Style;

use crate::doc::{Block, Document, Inline};
use wrap::{Piece, chunk, wrap};

mod table;
mod theme;
mod wrap;

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
    /// A line of a table: border or cells.
    Table,
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
pub fn layout(doc: &Document, width: u16, _cell: (u16, u16)) -> Layout {
    let state = if width < MIN_WIDTH {
        State::TooNarrow
    } else if doc.empty {
        State::Empty
    } else {
        State::Ready
    };
    let mut out = Layout {
        state,
        rows: Vec::new(),
    };
    if state != State::Ready {
        return out;
    }
    let content = usize::from(width - 2 * MARGIN);
    for (index, block) in doc.blocks.iter().enumerate() {
        let rows = block_rows(block, index, content);
        if rows.is_empty() {
            continue;
        }
        if !out.rows.is_empty() {
            out.rows.push(Row {
                kind: RowKind::Blank,
                block: index,
                spans: Vec::new(),
            });
        }
        out.rows.extend(rows);
    }
    out
}

fn block_rows(block: &Block, index: usize, content: usize) -> Vec<Row> {
    let make = |kind, lines: Vec<Vec<RowSpan>>| -> Vec<Row> {
        lines
            .into_iter()
            .map(|spans| Row {
                kind,
                block: index,
                spans,
            })
            .collect()
    };
    let rule = || Row {
        kind: RowKind::Rule,
        block: index,
        spans: vec![RowSpan {
            text: "─".repeat(content),
            style: theme::rule(),
            link: None,
        }],
    };
    match block {
        Block::Paragraph(inlines) => make(
            RowKind::Text,
            wrap(pieces(inlines, Style::default()), content),
        ),
        Block::Heading { level, content: c } => {
            let mut rows = make(
                RowKind::Heading(*level),
                wrap(pieces(c, theme::heading(*level)), content),
            );
            if *level <= 2 && !rows.is_empty() {
                rows.push(rule());
            }
            rows
        }
        Block::Rule => vec![rule()],
        Block::Html(text) => {
            let like = RowSpan {
                text: String::new(),
                style: theme::dim(),
                link: None,
            };
            make(RowKind::Text, chunk(text, &like, content))
        }
        Block::Table(t) => table::rows(t, index, content),
        // Lists, quotes, code, tables and diagrams are laid out by later stages.
        _ => Vec::new(),
    }
}

/// Model inlines as wrapper input, with `base` under every span style.
fn pieces(inlines: &[Inline], base: Style) -> Vec<Piece> {
    let text = |text: String, style: Style, link: Option<String>| {
        Piece::Text(RowSpan { text, style, link })
    };
    inlines
        .iter()
        .filter_map(|inline| match inline {
            Inline::Span(s) => Some(text(
                s.text.clone(),
                theme::inline(base, s.style, s.link.is_some()),
                s.link.clone(),
            )),
            Inline::Html(h) => Some(text(h.clone(), base.patch(theme::dim()), None)),
            Inline::SoftBreak => Some(text(" ".into(), base, None)),
            Inline::HardBreak => Some(Piece::Break),
            Inline::FootnoteRef(n) => Some(text(format!("[{n}]"), base, None)),
            // Image slots belong to a later stage.
            Inline::Image(_) => None,
        })
        .collect()
}
