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
    pub fn set_query(&mut self, _layout: &Layout, query: &str) {
        self.query = query.to_string();
    }

    /// Same query on a new layout; keeps the current match by logical position.
    pub fn relayout(&mut self, _layout: &Layout) {}

    pub fn next(&mut self) {}

    pub fn prev(&mut self) {}
}

/// All matches of `query` in document order.
pub fn find_matches(_layout: &Layout, _query: &str) -> Vec<Match> {
    Vec::new()
}
