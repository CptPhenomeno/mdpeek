//! Table layout: box-drawing borders, alignment and proportional shrinking.

use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::wrap::wrap;
use super::{Row, RowKind, RowSpan, pieces, theme};
use crate::doc::{Align, Cell, Table};

type Lines = Vec<Vec<RowSpan>>;

fn line_width(line: &[RowSpan]) -> usize {
    line.iter().map(|s| s.text.width()).sum()
}

fn cell_lines(cell: &Cell, base: Style, max: usize) -> Lines {
    wrap(pieces(cell, base), max.max(1))
}

/// Column widths: natural when they fit `avail`, else shrunk proportionally.
fn widths(natural: &[usize], avail: usize) -> Vec<usize> {
    let total: usize = natural.iter().sum();
    if total <= avail {
        return natural.to_vec();
    }
    let mut out: Vec<usize> = natural.iter().map(|n| (avail * n / total).max(1)).collect();
    // Hand leftover columns to the widest-shrunk ones, one each.
    let mut spare = avail.saturating_sub(out.iter().sum());
    while spare > 0 {
        let Some(i) = (0..out.len())
            .filter(|&i| out[i] < natural[i])
            .max_by_key(|&i| natural[i] - out[i])
        else {
            break;
        };
        out[i] += 1;
        spare -= 1;
    }
    out
}

fn border(widths: &[usize], left: char, mid: char, right: char, index: usize) -> Row {
    let mut text = String::from(left);
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            text.push(mid);
        }
        text.push_str(&"─".repeat(w + 2));
    }
    text.push(right);
    Row {
        kind: RowKind::Table,
        block: index,
        spans: vec![RowSpan {
            text,
            style: theme::rule(),
            link: None,
        }],
    }
}

/// Rows of one table line: the cells side by side, as tall as the tallest.
fn band(cells: &[Lines], widths: &[usize], align: &[Align], index: usize) -> Vec<Row> {
    let height = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let bar = |text: &str| RowSpan {
        text: text.into(),
        style: theme::rule(),
        link: None,
    };
    let plain = |n: usize| RowSpan {
        text: " ".repeat(n),
        style: Style::default(),
        link: None,
    };
    (0..height)
        .map(|y| {
            let mut spans = vec![bar("│")];
            for (c, w) in widths.iter().enumerate() {
                let empty = Vec::new();
                let line = cells.get(c).and_then(|l| l.get(y)).unwrap_or(&empty);
                let gap = w.saturating_sub(line_width(line));
                let (before, after) = match align.get(c).copied().unwrap_or(Align::Default) {
                    Align::Right => (gap, 0),
                    Align::Center => (gap / 2, gap - gap / 2),
                    _ => (0, gap),
                };
                spans.push(plain(1 + before));
                spans.extend(line.iter().cloned());
                spans.push(plain(after + 1));
                spans.push(bar("│"));
            }
            Row {
                kind: RowKind::Table,
                block: index,
                spans,
            }
        })
        .collect()
}

/// Rows of `table` for a content width of `content` columns.
pub fn rows(table: &Table, index: usize, content: usize) -> Vec<Row> {
    let cols = table.header.len();
    if cols == 0 {
        return Vec::new();
    }
    let header_style = Style::default().add_modifier(Modifier::BOLD);
    let huge = usize::MAX / 4;
    let mut natural = vec![1usize; cols];
    let all = std::iter::once((&table.header, header_style))
        .chain(table.rows.iter().map(|r| (r, Style::default())));
    for (row, style) in all {
        for (c, cell) in row.iter().take(cols).enumerate() {
            let w = cell_lines(cell, style, huge)
                .iter()
                .map(|l| line_width(l))
                .max()
                .unwrap_or(0);
            natural[c] = natural[c].max(w);
        }
    }
    let avail = content.saturating_sub(3 * cols + 1).max(cols);
    let widths = widths(&natural, avail);
    let lay = |row: &[Cell], style: Style| -> Vec<Lines> {
        (0..cols)
            .map(|c| match row.get(c) {
                Some(cell) => cell_lines(cell, style, widths[c]),
                None => Vec::new(),
            })
            .collect()
    };
    let mut out = vec![border(&widths, '┌', '┬', '┐', index)];
    out.extend(band(
        &lay(&table.header, header_style),
        &widths,
        &table.align,
        index,
    ));
    out.push(border(&widths, '├', '┼', '┤', index));
    for row in &table.rows {
        out.extend(band(
            &lay(row, Style::default()),
            &widths,
            &table.align,
            index,
        ));
    }
    out.push(border(&widths, '└', '┴', '┘', index));
    out
}
