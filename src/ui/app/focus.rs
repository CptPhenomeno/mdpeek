//! Focus over links and diagrams (SPEC §3.6.1).

use std::ops::Range;

use ratatui::{buffer::Buffer, layout::Rect, style::Modifier};

use super::App;
use crate::layout::{Focusable, MARGIN, PictureKey, RowKind, State};
use crate::theme;

/// Stable identity of a focusable, independent of row numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusId {
    /// `ordinal`-th link of the top-level block `block`.
    Link {
        block: usize,
        ordinal: usize,
    },
    Diagram(PictureKey),
}

impl App {
    /// Identity of every focusable, in document order.
    pub(super) fn focus_ids(&self) -> Vec<FocusId> {
        let mut seen: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        self.layout
            .index
            .focusables
            .iter()
            .map(|f| match f {
                Focusable::Diagram { key, .. } => FocusId::Diagram(key.clone()),
                Focusable::Link { fragments, .. } => {
                    let block = fragments
                        .first()
                        .and_then(|f| self.layout.rows.get(f.row))
                        .map_or(0, |r| r.block);
                    let ordinal = seen.entry(block).or_insert(0);
                    *ordinal += 1;
                    FocusId::Link {
                        block,
                        ordinal: *ordinal - 1,
                    }
                }
            })
            .collect()
    }

    fn focus_position(&self) -> Option<usize> {
        let id = self.focus.as_ref()?;
        self.focus_ids().iter().position(|i| i == id)
    }

    /// The focused item, if any.
    pub fn focused(&self) -> Option<&Focusable> {
        self.layout.index.focusables.get(self.focus_position()?)
    }

    /// Tab (`forward`) or Shift+Tab: move the focus, wrapping around.
    pub(super) fn focus_step(&mut self, forward: bool) {
        let ids = self.focus_ids();
        let n = ids.len();
        if n == 0 {
            self.focus = None;
            return;
        }
        let next = match (self.focus_position(), forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.focus = Some(ids[next].clone());
        self.scroll_focus_into_view();
    }

    /// Esc: focus nothing.
    pub(super) fn clear_focus(&mut self) {
        self.focus = None;
    }

    /// Drop a focus whose item left the layout (a diagram turned into text).
    pub(super) fn validate_focus(&mut self) {
        if self.focus.is_some() && self.focus_position().is_none() {
            self.focus = None;
        }
    }

    fn focus_rows(&self) -> Option<Range<usize>> {
        Some(match self.focused()? {
            Focusable::Diagram { rows, .. } => rows.clone(),
            Focusable::Link { fragments, .. } => {
                let first = fragments.first()?.row;
                let last = fragments.last()?.row;
                first..last + 1
            }
        })
    }

    /// Minimal scroll that shows the whole item, or its top when it does not fit.
    fn scroll_focus_into_view(&mut self) {
        let Some(rows) = self.focus_rows() else {
            return;
        };
        let height = self.viewport_height();
        if rows.start < self.scroll || rows.len() > height {
            self.scroll = rows.start;
        } else if rows.end > self.scroll + height {
            self.scroll = rows.end - height;
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// Screen cell of document `row` and content column `col`, when visible in `pane`.
    pub(super) fn cell_at(&self, pane: Rect, row: usize, col: i32) -> Option<(u16, u16)> {
        let y = i32::from(pane.y) + i32::try_from(row).ok()? - i32::try_from(self.scroll).ok()?;
        let x = i32::from(pane.x) + i32::from(MARGIN) + col;
        let inside = x >= i32::from(pane.x)
            && x < i32::from(pane.x + pane.width)
            && y >= i32::from(pane.y)
            && y < i32::from(pane.y + pane.height);
        if inside {
            Some((u16::try_from(x).ok()?, u16::try_from(y).ok()?))
        } else {
            None
        }
    }

    /// Marks the focused item on `buf`; `pane` is the inner area of the document.
    pub(super) fn draw_focus(&self, pane: Rect, buf: &mut Buffer) {
        if self.layout.state != State::Ready {
            return;
        }
        let Some(item) = self.focused() else {
            return;
        };
        let cell_at = |row: usize, col: i32| self.cell_at(pane, row, col);
        match item {
            Focusable::Link { fragments, .. } => {
                for f in fragments {
                    for col in f.start..f.end {
                        if let Some(pos) = cell_at(f.row, i32::from(col)) {
                            buf[pos].modifier.insert(Modifier::REVERSED);
                        }
                    }
                }
            }
            Focusable::Diagram { rows, .. } => {
                let Some(slot) = self
                    .layout
                    .rows
                    .get(rows.start)
                    .and_then(|r| r.slot.as_ref())
                else {
                    return;
                };
                let (left, right) = (i32::from(slot.left) - 1, i32::from(slot.left + slot.cols));
                let (top, bottom) = (rows.start as isize - 1, rows.end as isize);
                let style = theme::focus_frame();
                let mut put = |row: isize, col: i32, symbol: &str| {
                    let Ok(row) = usize::try_from(row) else {
                        return;
                    };
                    if let Some(pos) = cell_at(row, col) {
                        buf[pos].set_symbol(symbol).set_style(style);
                    }
                };
                // Only free cells are decorated: the bars sit in the columns beside
                // the slot (margin or the picture rows' own blank cells), and an
                // edge goes on a row only when that row is spacing.
                let blank = |row: isize| {
                    usize::try_from(row)
                        .ok()
                        .and_then(|r| self.layout.rows.get(r))
                        .is_some_and(|r| r.kind == RowKind::Blank)
                };
                for row in rows.clone() {
                    put(row as isize, left, "│");
                    put(row as isize, right, "│");
                }
                for (row, (l, r)) in [(top, ("┌", "┐")), (bottom, ("└", "┘"))] {
                    if !blank(row) {
                        continue;
                    }
                    for col in left + 1..right {
                        put(row, col, "─");
                    }
                    put(row, left, l);
                    put(row, right, r);
                }
            }
        }
    }
}
