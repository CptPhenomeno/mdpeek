//! The screen areas of the main view, computed in one place (SPEC §3.2).

use ratatui::layout::Rect;

use super::{BORDER, STATUS_ROWS};

/// Width of the sidebar pane, border included (SPEC §3.2).
pub(super) const SIDEBAR_COLS: u16 = 30;

/// The rectangles of the main view, in terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Areas {
    /// Sidebar pane, border included; zero width when hidden.
    pub sidebar: Rect,
    /// Document pane, border included.
    pub document: Rect,
    /// Document pane inside its border.
    pub document_inner: Rect,
    /// Status bar.
    pub status: Rect,
}

/// Splits `terminal` into the sidebar, document and status areas.
pub fn areas(terminal: Rect, sidebar_visible: bool) -> Areas {
    let body = Rect {
        height: terminal.height.saturating_sub(STATUS_ROWS),
        ..terminal
    };
    let status = Rect {
        y: terminal.y + body.height,
        height: terminal.height - body.height,
        ..terminal
    };
    let cols = if sidebar_visible {
        SIDEBAR_COLS.min(terminal.width / 2)
    } else {
        0
    };
    let sidebar = Rect {
        width: cols,
        ..body
    };
    let document = Rect {
        x: body.x + cols,
        width: body.width - cols,
        ..body
    };
    let b = BORDER / 2;
    let document_inner = Rect {
        x: document.x + b,
        y: document.y + b,
        width: document.width.saturating_sub(BORDER),
        height: document.height.saturating_sub(BORDER),
    };
    Areas {
        sidebar,
        document,
        document_inner,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area_of(r: Rect) -> u32 {
        u32::from(r.width) * u32::from(r.height)
    }

    #[test]
    fn areas_tile_the_terminal_without_overlap() {
        for (w, h) in [(120, 40), (80, 24), (31, 5), (10, 3), (4, 2)] {
            for visible in [true, false] {
                let terminal = Rect::new(0, 0, w, h);
                let a = areas(terminal, visible);
                let parts = [a.sidebar, a.document, a.status];
                let total: u32 = parts.iter().map(|r| area_of(*r)).sum();
                assert_eq!(total, area_of(terminal), "{w}x{h} visible={visible}");
                for (i, p) in parts.iter().enumerate() {
                    assert_eq!(p.intersection(terminal), *p, "inside {w}x{h}");
                    for q in &parts[i + 1..] {
                        assert!(p.intersection(*q).is_empty(), "overlap {w}x{h}");
                    }
                }
                assert_eq!(a.sidebar.width == 0, !visible);
                assert_eq!(a.sidebar.width.min(30), a.sidebar.width);
                if visible {
                    assert_eq!(a.sidebar.width, 30.min(w / 2));
                }
                assert_eq!(a.document.intersection(a.document_inner), a.document_inner);
                assert_eq!(a.document_inner.width, a.document.width.saturating_sub(2));
                assert_eq!(a.document_inner.height, a.document.height.saturating_sub(2));
                assert_eq!(a.status.height, 1.min(h));
            }
        }
    }
}
