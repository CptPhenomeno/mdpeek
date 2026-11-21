//! Case-insensitive search over the layout's logical block text (SPEC §3.9, §4.14).

use std::ops::Range;

use crate::layout::{Fragment, Layout};

#[cfg(test)]
mod tests;

/// One occurrence of the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Index of the block in `Layout::index.texts`.
    pub block: usize,
    /// Byte range in that block's original (not lowercased) text.
    pub range: Range<usize>,
    /// Screen fragments, one per row the match spans.
    pub fragments: Vec<Fragment>,
}

/// Matches of one query and the current one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Search {
    query: String,
    matches: Vec<Match>,
    current: Option<usize>,
}

impl Search {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn matches(&self) -> &[Match] {
        &self.matches
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn current(&self) -> Option<&Match> {
        self.current.and_then(|i| self.matches.get(i))
    }

    /// Logical position (block, byte offset) of the current match.
    pub fn position(&self) -> Option<(usize, usize)> {
        self.current().map(|m| (m.block, m.range.start))
    }

    /// New query: recompute the matches and select the first one.
    pub fn set_query(&mut self, layout: &Layout, query: &str) {
        self.query = query.to_string();
        self.matches = find_matches(layout, query);
        self.current = if self.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Same query on a new layout; keeps the current match by logical position
    /// (the first match at or after it, else the last one).
    pub fn relayout(&mut self, layout: &Layout) {
        let pos = self.position();
        self.matches = find_matches(layout, &self.query);
        self.current = match pos {
            _ if self.matches.is_empty() => None,
            None => Some(0),
            Some(pos) => Some(
                self.matches
                    .iter()
                    .position(|m| (m.block, m.range.start) >= pos)
                    .unwrap_or(self.matches.len() - 1),
            ),
        };
    }

    pub fn next(&mut self) {
        let n = self.matches.len();
        self.current = self.current.map(|i| (i + 1) % n.max(1));
    }

    pub fn prev(&mut self) {
        let n = self.matches.len();
        self.current = self.current.map(|i| (i + n.max(1) - 1) % n.max(1));
    }
}

/// Lowercase `text` char by char, so each lowered byte maps to the original
/// char it came from (lowercasing can change byte lengths).
/// Returns the lowered text and, per lowered byte, the original char's byte range.
fn lower_with_map(text: &str) -> (String, Vec<(usize, usize)>) {
    let mut lowered = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len());
    for (start, ch) in text.char_indices() {
        let end = start + ch.len_utf8();
        for lc in ch.to_lowercase() {
            lowered.push(lc);
            map.extend(std::iter::repeat_n((start, end), lc.len_utf8()));
        }
    }
    (lowered, map)
}

/// All matches of `query` in document order.
pub fn find_matches(layout: &Layout, query: &str) -> Vec<Match> {
    let needle: String = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (block, text) in layout.index.texts.iter().enumerate() {
        let (lowered, map) = lower_with_map(&text.text);
        let mut from = 0;
        while let Some(found) = lowered[from..].find(&needle) {
            let lo = from + found;
            let hi = lo + needle.len();
            // Whole original chars: start of the first, end of the last.
            let range = map[lo].0..map[hi - 1].1;
            let fragments = text.fragments(range.clone());
            if !fragments.is_empty() {
                out.push(Match {
                    block,
                    range,
                    fragments,
                });
            }
            from = hi;
        }
    }
    out
}
