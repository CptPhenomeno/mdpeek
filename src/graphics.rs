//! Picture slots, partial visibility and the picture-renderer seam (SPEC §3.5).
//!
//! Spike (KB-7): the geometry is pure and testable; the renderers draw into a
//! ratatui `Buffer` so that every protocol is checked the same way.

use ratatui::{buffer::Buffer, layout::Rect};

/// The part of an image slot that intersects the document pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisiblePart {
    /// Slot rows scrolled out above the pane (rows to skip at the top).
    pub skip_rows: u16,
    /// Number of visible slot rows.
    pub rows: u16,
    /// Screen row of the first visible slot row.
    pub screen_y: u16,
}

/// Visible part of a slot whose first row sits at `slot_top` (screen
/// coordinates, may be negative or beyond the pane) and spans `slot_rows`.
pub fn visible_part(_slot_top: i32, _slot_rows: u16, _pane: Rect) -> Option<VisiblePart> {
    unimplemented!("spike")
}

/// Source pixel rows `(y, height)` that correspond to the visible part of a
/// slot, for the "crop then encode" strategy.
pub fn source_crop(_image_height: u32, _slot_rows: u16, _part: &VisiblePart) -> (u32, u32) {
    unimplemented!("spike")
}

/// The pane area minus the overlay rectangles: the cells a picture may cover.
pub fn exclude_overlays(_area: Rect, _overlays: &[Rect]) -> Vec<Rect> {
    unimplemented!("spike")
}

/// Everything a renderer needs to draw one picture slot.
pub struct PictureRequest<'a> {
    /// Document pane: the clip rectangle.
    pub pane: Rect,
    /// Screen row of the slot's first row (may be negative).
    pub slot_top: i32,
    /// Screen column of the slot's left edge.
    pub slot_left: u16,
    /// Slot width in cells.
    pub slot_cols: u16,
    /// Slot height in cells.
    pub slot_rows: u16,
    /// Open overlays (popups) that hide whatever is below them.
    pub overlays: &'a [Rect],
}

/// Draws a picture slot clipped to the pane. Implemented by the real
/// terminal-graphics renderer and by the placeholder used in snapshot tests.
pub trait PictureRenderer {
    fn render(&mut self, request: &PictureRequest<'_>, buf: &mut Buffer);
}

/// Fills the visible cells of a slot with a marker symbol.
pub struct PlaceholderRenderer {
    pub symbol: char,
}

impl PictureRenderer for PlaceholderRenderer {
    fn render(&mut self, _request: &PictureRequest<'_>, _buf: &mut Buffer) {
        unimplemented!("spike")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane() -> Rect {
        Rect::new(20, 1, 60, 10)
    }

    #[test]
    fn visible_part_returns_the_slot_rows_that_intersect_the_viewport() {
        // fully inside
        assert_eq!(
            visible_part(3, 4, pane()),
            Some(VisiblePart {
                skip_rows: 0,
                rows: 4,
                screen_y: 3
            })
        );
        // top clipped: 2 rows above the pane (pane starts at y=1, slot at -1)
        assert_eq!(
            visible_part(-1, 6, pane()),
            Some(VisiblePart {
                skip_rows: 2,
                rows: 4,
                screen_y: 1
            })
        );
        // bottom clipped: pane ends at y=11 (exclusive), slot 8..14
        assert_eq!(
            visible_part(8, 6, pane()),
            Some(VisiblePart {
                skip_rows: 0,
                rows: 3,
                screen_y: 8
            })
        );
        // taller than the pane: both sides clipped
        assert_eq!(
            visible_part(-2, 20, pane()),
            Some(VisiblePart {
                skip_rows: 3,
                rows: 10,
                screen_y: 1
            })
        );
    }

    #[test]
    fn slot_fully_outside_the_viewport_has_no_visible_part() {
        assert_eq!(visible_part(-5, 4, pane()), None); // ends at -1
        assert_eq!(visible_part(-3, 4, pane()), None); // ends at 1: touches only
        assert_eq!(visible_part(11, 3, pane()), None); // starts at pane end
        assert_eq!(visible_part(30, 3, pane()), None);
        assert_eq!(visible_part(2, 0, pane()), None);
    }

    #[test]
    fn source_crop_for_a_slot_scrolled_half_out_of_the_top_keeps_the_bottom_half() {
        // slot of 10 rows, image 200 px high (20 px per row), 5 rows hidden above
        let part = visible_part(-4, 10, pane()).unwrap();
        assert_eq!(part.skip_rows, 5);
        assert_eq!(source_crop(200, 10, &part), (100, 100));
        // bottom half out: keeps the top half
        let part = visible_part(6, 10, pane()).unwrap();
        assert_eq!(part.rows, 5);
        assert_eq!(source_crop(200, 10, &part), (0, 100));
        // last slot row holds fewer pixels than a full row: crop stays in bounds
        let part = visible_part(-4, 10, pane()).unwrap();
        assert_eq!(source_crop(195, 10, &part), (97, 98));
    }

    #[test]
    fn placeholder_renderer_fills_only_the_visible_part_of_a_slot_on_test_backend() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 12));
        let mut r = PlaceholderRenderer { symbol: '#' };
        let request = PictureRequest {
            pane: pane(),
            slot_top: -1,
            slot_left: 30,
            slot_cols: 8,
            slot_rows: 6,
            overlays: &[],
        };
        r.render(&request, &mut buf);
        for y in 0..12u16 {
            for x in 0..80u16 {
                let inside = (30..38).contains(&x) && (1..5).contains(&y);
                let sym = buf[(x, y)].symbol();
                assert_eq!(sym == "#", inside, "cell {x},{y} = {sym:?}");
            }
        }
    }

    #[test]
    fn cells_covered_by_an_overlay_rectangle_are_excluded_from_the_picture_area() {
        let area = Rect::new(30, 2, 8, 6);
        let overlay = Rect::new(34, 4, 10, 2);
        let pieces = exclude_overlays(area, &[overlay]);
        let covered = |x: u16, y: u16| pieces.iter().any(|r| r.contains((x, y).into()));
        for y in 0..12 {
            for x in 28..48 {
                let in_area = area.contains((x, y).into());
                let in_overlay = overlay.contains((x, y).into());
                assert_eq!(covered(x, y), in_area && !in_overlay, "cell {x},{y}");
            }
        }
        // pieces never overlap each other
        let total: u32 = pieces.iter().map(|r| r.area()).sum();
        assert_eq!(total, 8 * 6 - 4 * 2);
    }
}
