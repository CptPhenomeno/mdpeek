//! Left clicks on links and diagrams (SPEC §3.6.4).

use std::time::Duration;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::App;
use crate::layout::{Focusable, Layout, MARGIN, PictureKey, State};

/// Two clicks on the same diagram at most this far apart are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// Index in `layout.index.focusables` of the item under the screen cell
/// (`x`, `y`), or `None`. `inner` is the document pane inside its border and
/// `scroll` the first visible row. Every fragment of a wrapped link hits the
/// link; a diagram is hit on its slot columns only.
pub(super) fn hit_test(
    layout: &Layout,
    inner: Rect,
    scroll: usize,
    x: u16,
    y: u16,
) -> Option<usize> {
    if layout.state != State::Ready
        || x < inner.x
        || x >= inner.x + inner.width
        || y < inner.y
        || y >= inner.y + inner.height
    {
        return None;
    }
    let row = scroll + usize::from(y - inner.y);
    let col = i32::from(x) - i32::from(inner.x) - i32::from(MARGIN);
    let col = u16::try_from(col).ok()?;
    layout.index.focusables.iter().position(|item| match item {
        Focusable::Link { fragments, .. } => fragments
            .iter()
            .any(|f| f.row == row && (f.start..f.end).contains(&col)),
        Focusable::Diagram { rows, .. } => {
            rows.contains(&row)
                && layout
                    .rows
                    .get(rows.start)
                    .and_then(|r| r.slot.as_ref())
                    .is_some_and(|s| (s.left..s.left + s.cols).contains(&col))
        }
    })
}

impl App {
    /// Whether something covers the page, so clicks must not reach it. Add
    /// new overlays and inputs here.
    pub(super) fn clicks_blocked(&self) -> bool {
        self.view.is_some() || self.overlay.is_some() || self.text_input_open()
    }

    /// Left button pressed: act on the link or diagram under the cursor.
    /// Down (not Up) acts once per click: a drag ends with its own Up and
    /// must not trigger a second time.
    pub(super) fn click(&mut self, mouse: MouseEvent) {
        if self.clicks_blocked() || mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let hit = hit_test(
            &self.layout,
            self.areas().document_inner,
            self.scroll,
            mouse.column,
            mouse.row,
        );
        let Some(index) = hit else {
            return;
        };
        let Some(id) = self.focus_ids().into_iter().nth(index) else {
            return;
        };
        self.focus = Some(id);
        self.message = None;
        match self.layout.index.focusables.get(index) {
            Some(Focusable::Link { .. }) => {
                self.last_click = None;
                self.activate();
            }
            Some(Focusable::Diagram { key, .. }) => self.click_diagram(key.clone()),
            None => {}
        }
    }

    /// A click on a diagram focuses it; a second one soon after opens it.
    fn click_diagram(&mut self, key: PictureKey) {
        let now = self.jobs.now();
        let double = self
            .last_click
            .as_ref()
            .is_some_and(|(k, t)| *k == key && now.saturating_sub(*t) <= DOUBLE_CLICK);
        if double {
            self.last_click = None;
            self.fullscreen_request = Some(key);
        } else {
            self.last_click = Some((key, now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Fragment;

    fn layout_with_link() -> Layout {
        let mut layout = Layout {
            state: State::Ready,
            rows: Vec::new(),
            index: Default::default(),
        };
        layout.index.focusables.push(Focusable::Link {
            target: "#a".into(),
            fragments: vec![
                Fragment {
                    row: 4,
                    start: 10,
                    end: 14,
                },
                Fragment {
                    row: 5,
                    start: 0,
                    end: 3,
                },
            ],
        });
        layout
    }

    const INNER: Rect = Rect {
        x: 31,
        y: 1,
        width: 38,
        height: 8,
    };

    #[test]
    fn every_fragment_of_a_link_hits_it_and_the_cells_around_do_not() {
        let l = layout_with_link();
        // scroll 3: row 4 is screen row 2, row 5 is screen row 3; column + 32
        assert_eq!(hit_test(&l, INNER, 3, 42, 2), Some(0));
        assert_eq!(hit_test(&l, INNER, 3, 45, 2), Some(0));
        assert_eq!(hit_test(&l, INNER, 3, 46, 2), None);
        assert_eq!(hit_test(&l, INNER, 3, 41, 2), None);
        assert_eq!(hit_test(&l, INNER, 3, 32, 3), Some(0));
        assert_eq!(hit_test(&l, INNER, 3, 35, 3), None);
        assert_eq!(hit_test(&l, INNER, 3, 42, 1), None);
    }

    #[test]
    fn cells_outside_the_pane_never_hit() {
        let l = layout_with_link();
        assert_eq!(hit_test(&l, INNER, 3, 10, 2), None);
        assert_eq!(hit_test(&l, INNER, 3, 30, 3), None);
        assert_eq!(hit_test(&l, INNER, 3, 42, 0), None);
    }
}
