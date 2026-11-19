//! Table layout: box-drawing borders, alignment and proportional shrinking.

use super::Row;
use crate::doc::Table;

/// Rows of `table` for a content width of `content` columns.
pub fn rows(_table: &Table, _index: usize, _content: usize) -> Vec<Row> {
    Vec::new()
}
