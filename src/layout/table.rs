//! Table layout: box-drawing borders, alignment and proportional shrinking.

use std::cell::Cell as Counter;

use ratatui::style::{Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::wrap::wrap;
use super::{Row, RowKind, RowSpan, pieces, tag};
use crate::doc::{Align, Cell, Table};
use crate::theme;

type Lines = Vec<Vec<RowSpan>>;

fn line_width(line: &[RowSpan]) -> usize {
    line.iter().map(|s| s.text.width()).sum()
}

fn cell_lines(cell: &Cell, base: Style, max: usize) -> Lines {
    wrap(pieces(cell, base), max.max(1))
}

/// Column widths: natural when they fit `avail`, else shrunk proportionally
/// but never below `min` (2 for columns holding a wide character, else 1).
fn widths(natural: &[usize], min: &[usize], avail: usize) -> Vec<usize> {
    let total: usize = natural.iter().sum();
    if total <= avail {
        return natural.to_vec();
    }
    let mut out: Vec<usize> = (0..natural.len())
        .map(|i| (avail * natural[i] / total).max(min[i]))
        .collect();
    // Take back what the minimums over-allocated, from the widest columns.
    while out.iter().sum::<usize>() > avail {
        let Some(i) = (0..out.len())
            .filter(|&i| out[i] > min[i])
            .max_by_key(|&i| out[i])
        else {
            break;
        };
        out[i] -= 1;
    }
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
        slot: None,
        lazy: None,
        spans: vec![RowSpan {
            text,
            style: theme::rule(),
            link: None,
            unit: None,
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
        unit: None,
    };
    let plain = |n: usize| RowSpan {
        text: " ".repeat(n),
        style: theme::body(),
        link: None,
        unit: None,
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
                slot: None,
                lazy: None,
                spans,
            }
        })
        .collect()
}

/// Rows of `table` for a content width of `content` columns.
///
/// Every row is at most `content` columns wide. When the columns cannot all
/// fit even at their minimum width, only the leading columns that fit are
/// drawn and each row ends with a `…` marker.
pub fn rows(table: &Table, index: usize, content: usize, units: &Counter<usize>) -> Vec<Row> {
    let all_cols = table.header.len();
    if all_cols == 0 {
        return Vec::new();
    }
    let header_style = theme::body().add_modifier(Modifier::BOLD);
    let huge = usize::MAX / 4;
    let mut natural = vec![1usize; all_cols];
    let mut min = vec![1usize; all_cols];
    let every = std::iter::once((&table.header, header_style))
        .chain(table.rows.iter().map(|r| (r, theme::body())));
    // Cells wrapped at unlimited width: reused when their column is not shrunk.
    let mut unwrapped: Vec<Vec<Lines>> = Vec::new();
    for (row, style) in every {
        let mut kept = Vec::new();
        for (c, cell) in row.iter().take(all_cols).enumerate() {
            let lines = cell_lines(cell, style, huge);
            let w = lines.iter().map(|l| line_width(l)).max().unwrap_or(0);
            natural[c] = natural[c].max(w);
            if lines
                .iter()
                .flatten()
                .any(|s| s.text.chars().any(|ch| ch.width() == Some(2)))
            {
                min[c] = 2;
            }
            kept.push(lines);
        }
        unwrapped.push(kept);
    }
    let fits = natural.iter().sum::<usize>() + 3 * all_cols < content;
    let mut cols = all_cols;
    let mut marker = false;
    if !fits && min.iter().sum::<usize>() + 3 * all_cols + 1 > content {
        marker = true;
        cols = 1;
        // One column is left for the marker.
        while cols < all_cols && min[..=cols].iter().sum::<usize>() + 3 * (cols + 1) + 2 <= content
        {
            cols += 1;
        }
    }
    let avail = content.saturating_sub(3 * cols + 1 + usize::from(marker));
    let widths = widths(&natural[..cols], &min[..cols], avail);
    let mut lay = |n: usize, row: &[Cell], style: Style| -> Vec<Lines> {
        (0..cols)
            .map(|c| match row.get(c) {
                Some(cell) => {
                    let mut lines = match unwrapped[n].get_mut(c) {
                        Some(done) if widths[c] >= natural[c] => std::mem::take(done),
                        _ => cell_lines(cell, style, widths[c]),
                    };
                    tag(&mut lines, units);
                    lines
                }
                None => Vec::new(),
            })
            .collect()
    };
    let mut out = vec![border(&widths, '┌', '┬', '┐', index)];
    out.extend(band(
        &lay(0, &table.header, header_style),
        &widths,
        &table.align,
        index,
    ));
    out.push(border(&widths, '├', '┼', '┤', index));
    for (n, row) in table.rows.iter().enumerate() {
        out.extend(band(
            &lay(n + 1, row, theme::body()),
            &widths,
            &table.align,
            index,
        ));
    }
    out.push(border(&widths, '└', '┴', '┘', index));
    if marker {
        for row in &mut out {
            row.spans.push(RowSpan {
                text: "…".into(),
                style: theme::dim(),
                link: None,
                unit: None,
            });
        }
    }
    out
}
