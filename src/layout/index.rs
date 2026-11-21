//! Positions in the laid-out document: headings, focusables and searchable text
//! (SPEC §3.6, §3.9.2, §3.10, §4.14).

use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use crate::doc::Document;

use super::{PictureKey, Row, RowKind};

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
        let mut out: Vec<Fragment> = Vec::new();
        for part in &self.parts {
            let lo = range.start.max(part.offset);
            let hi = range.end.min(part.offset + part.len);
            if lo >= hi {
                continue;
            }
            let text = &self.text[part.offset..part.offset + part.len];
            let cols = |abs: usize| {
                let mut rel = (abs - part.offset).min(text.len());
                while !text.is_char_boundary(rel) {
                    rel -= 1;
                }
                part.col.saturating_add(text[..rel].width() as u16)
            };
            let (start, end) = (cols(lo), cols(hi));
            if start == end {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.row == part.row && last.end == start => last.end = end,
                _ => out.push(Fragment {
                    row: part.row,
                    start,
                    end,
                }),
            }
        }
        out
    }
}

/// Build the indexes from the finished `rows`. `heading_units` lists the text
/// unit of each heading in laying-out order; the first ones are the document's
/// own headings, in the order of `Document::heading_slugs`.
pub(super) fn build(doc: &Document, rows: &[Row], heading_units: &[usize]) -> Index {
    // Units are numbered densely from 0 in laying-out order, so these are plain vectors.
    let mut blocks: Vec<TextBlock> = Vec::new();
    let mut heads: Vec<Option<(usize, u8)>> = Vec::new();
    // Link occurrence still open in each unit, as an index into `found`.
    let mut open: Vec<Option<usize>> = Vec::new();
    // (row, col, focusable), sorted by position at the end.
    let mut found: Vec<(usize, u16, Focusable)> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        if let Some(slot) = &row.slot
            && matches!(slot.key, PictureKey::Diagram(_))
        {
            found.push((
                r,
                0,
                Focusable::Diagram {
                    key: slot.key.clone(),
                    rows: r..r + usize::from(slot.rows),
                },
            ));
        }
        if matches!(row.kind, RowKind::Picture | RowKind::ImageNote) {
            continue;
        }
        let mut col: u16 = 0;
        for span in &row.spans {
            let width = span.text.width().min(usize::from(u16::MAX)) as u16;
            if let Some(unit) = span.unit {
                if unit >= blocks.len() {
                    blocks.resize_with(unit + 1, TextBlock::default);
                    heads.resize(unit + 1, None);
                    open.resize(unit + 1, None);
                }
                if let RowKind::Heading(level) = row.kind {
                    heads[unit].get_or_insert((r, level));
                }
                let block = &mut blocks[unit];
                let new_row = block.parts.last().is_some_and(|p| p.row != r);
                if new_row
                    && !block.text.ends_with(char::is_whitespace)
                    && !span.text.starts_with(char::is_whitespace)
                {
                    block.text.push(' ');
                }
                block.parts.push(Part {
                    offset: block.text.len(),
                    len: span.text.len(),
                    row: r,
                    col,
                });
                block.text.push_str(&span.text);
                match &span.link {
                    None => open[unit] = None,
                    Some(target) => {
                        let reuse = open[unit].filter(|&i| {
                            matches!(&found[i].2, Focusable::Link { target: t, .. } if t == target)
                        });
                        let i = reuse.unwrap_or_else(|| {
                            found.push((
                                r,
                                col,
                                Focusable::Link {
                                    target: target.clone(),
                                    fragments: Vec::new(),
                                },
                            ));
                            open[unit] = Some(found.len() - 1);
                            found.len() - 1
                        });
                        if let Focusable::Link { fragments, .. } = &mut found[i].2 {
                            let end = col.saturating_add(width);
                            match fragments.last_mut() {
                                Some(f) if f.row == r && f.end == col => f.end = end,
                                _ => fragments.push(Fragment {
                                    row: r,
                                    start: col,
                                    end,
                                }),
                            }
                        }
                    }
                }
            }
            col = col.saturating_add(width);
        }
    }
    found.sort_by_key(|&(r, c, _)| (r, c));
    let slugs = doc.heading_slugs();
    let headings = heading_units
        .iter()
        .zip(slugs)
        .filter_map(|(unit, slug)| {
            let (row, level) = (*heads.get(*unit)?)?;
            Some(HeadingEntry {
                level,
                text: blocks.get(*unit)?.text.clone(),
                slug,
                row,
            })
        })
        .collect();
    Index {
        headings,
        focusables: found.into_iter().map(|(_, _, f)| f).collect(),
        texts: blocks
            .into_iter()
            .filter(|b| !b.text.trim().is_empty())
            .collect(),
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
