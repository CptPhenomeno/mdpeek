//! Document model + pane width -> rows of styled spans (SPEC §3.3, §3.11).

use ratatui::style::Style;

use crate::doc::{Block, Document, Inline, List};
use unicode_width::UnicodeWidthChar;
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
    footnotes(doc, content, &mut out.rows);
    out
}

/// Footnote definitions under a rule at the end of the document.
fn footnotes(doc: &Document, content: usize, rows: &mut Vec<Row>) {
    if doc.footnotes.is_empty() {
        return;
    }
    let index = doc.blocks.len();
    if !rows.is_empty() {
        rows.push(blank(index));
    }
    rows.push(Row {
        kind: RowKind::Rule,
        block: index,
        spans: vec![RowSpan {
            text: "─".repeat(content),
            style: theme::rule(),
            link: None,
        }],
    });
    for note in &doc.footnotes {
        let marker = format!("[{}] ", note.number);
        let hang = marker.chars().count();
        let inner = blocks_rows(&note.blocks, index, content.saturating_sub(hang).max(1));
        rows.extend(hang_rows(inner, index, &marker, hang, content));
    }
}

fn blank(index: usize) -> Row {
    Row {
        kind: RowKind::Blank,
        block: index,
        spans: Vec::new(),
    }
}

/// Child blocks separated by blank rows.
fn blocks_rows(blocks: &[Block], index: usize, width: usize) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for block in blocks {
        let rows = block_rows(block, index, width);
        if rows.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(blank(index));
        }
        out.extend(rows);
    }
    out
}

fn plain(text: &str, style: Style) -> RowSpan {
    RowSpan {
        text: text.to_string(),
        style,
        link: None,
    }
}

/// Put `prefix` on the first row and `hang` blank columns on the others.
fn hang_rows(mut rows: Vec<Row>, index: usize, marker: &str, hang: usize, max: usize) -> Vec<Row> {
    if rows.is_empty() {
        rows.push(Row {
            kind: RowKind::Text,
            block: index,
            spans: Vec::new(),
        });
    }
    for (i, row) in rows.iter_mut().enumerate() {
        if row.kind == RowKind::Blank {
            continue;
        }
        let lead = if i == 0 {
            marker.to_string()
        } else {
            " ".repeat(hang)
        };
        row.spans.insert(0, plain(&lead, Style::default()));
        clip(row, max);
    }
    rows
}

/// Cut a row that would exceed `max` columns (only possible in very deep nesting).
fn clip(row: &mut Row, max: usize) {
    let mut used = 0;
    let mut keep = Vec::new();
    for mut span in std::mem::take(&mut row.spans) {
        let mut text = String::new();
        for c in span.text.chars() {
            let w = c.width().unwrap_or(0);
            if used + w > max {
                break;
            }
            used += w;
            text.push(c);
        }
        span.text = text;
        if !span.text.is_empty() {
            keep.push(span);
        }
    }
    row.spans = keep;
}

fn list_rows(list: &List, index: usize, content: usize) -> Vec<Row> {
    let mut out = Vec::new();
    for (n, item) in list.items.iter().enumerate() {
        let marker = match (item.task, list.start) {
            (Some(true), _) => "☑ ".to_string(),
            (Some(false), _) => "☐ ".to_string(),
            (None, Some(start)) => format!("{}. ", start + n as u64),
            (None, None) => "• ".to_string(),
        };
        let hang = marker.chars().count();
        let mut rows: Vec<Row> = Vec::new();
        let mut first_laid_out = false;
        for (i, block) in item.blocks.iter().enumerate() {
            // A nested list steps in two columns; other content aligns with the item text.
            let nested = i > 0 && matches!(block, Block::List(_));
            let indent = if nested { 2 } else { hang };
            let child = block_rows(block, index, content.saturating_sub(indent).max(1));
            if child.is_empty() {
                continue;
            }
            if i > 0 && !nested && !rows.is_empty() {
                rows.push(blank(index));
            }
            first_laid_out |= i == 0;
            rows.extend(hang_rows(
                child,
                index,
                &" ".repeat(indent),
                indent,
                content,
            ));
        }
        if first_laid_out {
            // The first row carries the marker in place of its alignment padding.
            rows[0].spans[0].text = marker;
        } else {
            let lone = plain(marker.trim_end(), Style::default());
            rows.insert(
                0,
                Row {
                    kind: RowKind::Text,
                    block: index,
                    spans: vec![lone],
                },
            );
        }
        out.extend(rows);
    }
    out
}

fn quote_rows(blocks: &[Block], index: usize, content: usize) -> Vec<Row> {
    let mut rows = blocks_rows(blocks, index, content.saturating_sub(2).max(1));
    for row in &mut rows {
        for span in &mut row.spans {
            span.style = span.style.patch(theme::dim());
        }
        let bar = if row.spans.is_empty() { "│" } else { "│ " };
        row.spans.insert(0, plain(bar, theme::dim()));
        clip(row, content);
    }
    rows
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
        Block::List(list) => list_rows(list, index, content),
        Block::BlockQuote(blocks) => quote_rows(blocks, index, content),
        // Code and diagrams are laid out by later stages.
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
