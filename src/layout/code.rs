//! Code blocks: a box with the language label at the top right, highlighted
//! lines, and long lines truncated with `…` instead of wrapped (SPEC §3.3).
//!
//! Each code line is its own search unit and only its visible text is indexed.
//! A tab expands to [`TAB`] spaces.

use std::cell::Cell as Counter;
use std::sync::{Arc, OnceLock};

use ratatui::style::Style;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{Row, RowKind, RowSpan, tag};
use crate::highlight::{Piece, highlight, plain};
use crate::theme;

/// A code block whose highlighting waits until a row of it is drawn: layout needs
/// only the line count (long lines are cut, not wrapped), and syntect costs far
/// more than the rest of the layout together.
#[derive(Debug)]
struct LazyBlock {
    lang: String,
    text: String,
    lines: OnceLock<Vec<Vec<Piece>>>,
}

impl PartialEq for LazyBlock {
    fn eq(&self, other: &Self) -> bool {
        self.lang == other.lang && self.text == other.text
    }
}

impl Eq for LazyBlock {}

/// One line of a [`LazyBlock`], on the row that shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLine {
    block: Arc<LazyBlock>,
    line: usize,
    /// Columns available to the code text.
    inner: usize,
}

impl CodeLine {
    pub(super) fn is_first(&self) -> bool {
        self.line == 0
    }

    pub(super) fn is_highlighted(&self) -> bool {
        self.block.lines.get().is_some()
    }

    /// `plain` (the row's spans as laid out) with the code text coloured.
    pub(super) fn highlighted_spans(&self, plain_spans: &[RowSpan]) -> Vec<RowSpan> {
        let block = &self.block;
        let lines = block
            .lines
            .get_or_init(|| highlight(&block.text, Some(&block.lang)));
        let Some(pieces) = lines.get(self.line) else {
            return plain_spans.to_vec();
        };
        // The code text is what search indexes: the spans with a unit.
        let Some(first) = plain_spans.iter().position(|s| s.unit.is_some()) else {
            return plain_spans.to_vec();
        };
        let last = plain_spans
            .iter()
            .rposition(|s| s.unit.is_some())
            .unwrap_or(first);
        let unit = plain_spans[first].unit;
        let pieces = expand(pieces.clone());
        let mut out = plain_spans[..first].to_vec();
        out.extend(truncate(pieces, self.inner).into_iter().map(|mut s| {
            s.unit = unit;
            s
        }));
        out.extend_from_slice(&plain_spans[last + 1..]);
        out
    }
}

/// Pieces with tabs expanded and the code style under them.
fn expand(line: Vec<Piece>) -> Vec<(String, Style)> {
    line.into_iter()
        .map(|p| (p.text.replace('\t', TAB), theme::code().patch(p.style)))
        .collect()
}

/// Spaces a tab expands to.
const TAB: &str = "    ";

fn span(text: String, style: Style) -> RowSpan {
    RowSpan {
        text,
        style,
        link: None,
        unit: None,
    }
}

/// Cut `pieces` to `max` columns; when something is cut the last column is `…`.
fn truncate(pieces: Vec<(String, Style)>, max: usize) -> Vec<RowSpan> {
    let total: usize = pieces.iter().map(|(t, _)| t.width()).sum();
    let budget = if total > max {
        max.saturating_sub(1)
    } else {
        max
    };
    let mut used = 0;
    let mut out = Vec::new();
    'pieces: for (text, style) in pieces {
        let mut kept = String::new();
        for c in text.chars() {
            let w = c.width().unwrap_or(0);
            if used + w > budget {
                if !kept.is_empty() {
                    out.push(span(kept, style));
                }
                break 'pieces;
            }
            used += w;
            kept.push(c);
        }
        if !kept.is_empty() {
            out.push(span(kept, style));
        }
    }
    if total > max {
        // Wide characters can leave the ellipsis short of the edge.
        out.push(span("…".into(), theme::code_label()));
    }
    out
}

fn border(content: usize, left: char, right: char, label: Option<&str>, line: Style) -> Row {
    let mut spans = Vec::new();
    // Corners, two dashes at least, and the label with a space on each side.
    match label {
        Some(l) if l.width() + 6 <= content => {
            let dashes = content - 2 - l.width() - 3;
            spans.push(span(format!("{left}{}", "─".repeat(dashes)), line));
            spans.push(span(format!(" {l} "), theme::code_label()));
            spans.push(span(format!("─{right}"), line));
        }
        _ => spans.push(span(
            format!("{left}{}{right}", "─".repeat(content.saturating_sub(2))),
            line,
        )),
    }
    Row {
        kind: RowKind::Code,
        block: 0,
        spans,
        slot: None,
        lazy: None,
    }
}

/// The box of a code block, `content` columns wide.
pub(super) fn rows(
    lang: Option<&str>,
    text: &str,
    index: usize,
    content: usize,
    units: &Counter<usize>,
) -> Vec<Row> {
    framed(lang, text, index, content, units, theme::code_border())
}

/// Like [`rows`], with the frame drawn in `frame` (a red frame for errors).
pub(super) fn framed(
    lang: Option<&str>,
    text: &str,
    index: usize,
    content: usize,
    units: &Counter<usize>,
    frame: Style,
) -> Vec<Row> {
    let lang = lang.map(|l| l.split(',').next().unwrap_or("").trim());
    let lang = lang.filter(|l| !l.is_empty());
    let inner = content.saturating_sub(4).max(1);
    let mut out = vec![border(content, '┌', '┐', lang, frame)];
    let lazy = lang.map(|l| {
        Arc::new(LazyBlock {
            lang: l.to_owned(),
            text: text.to_owned(),
            lines: OnceLock::new(),
        })
    });
    for (n, line) in plain(text).into_iter().enumerate() {
        let mut spans = truncate(expand(line), inner);
        let used: usize = spans.iter().map(|s| s.text.width()).sum();
        let mut lines = vec![std::mem::take(&mut spans)];
        tag(&mut lines, units);
        let mut spans = lines.remove(0);
        spans.insert(0, span("│ ".into(), frame));
        spans.push(span(
            " ".repeat(inner.saturating_sub(used) + 1),
            theme::code(),
        ));
        spans.push(span("│".into(), frame));
        out.push(Row {
            kind: RowKind::Code,
            block: 0,
            spans,
            slot: None,
            lazy: lazy.as_ref().map(|block| CodeLine {
                block: Arc::clone(block),
                line: n,
                inner,
            }),
        });
    }
    out.push(border(content, '└', '┘', None, frame));
    for row in &mut out {
        row.block = index;
    }
    out
}
