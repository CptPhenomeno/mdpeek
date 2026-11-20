//! Picture slots, partial visibility and the picture-renderer seam (SPEC §3.5).
//!
//! Spike (KB-7): the geometry is pure and testable; the renderers draw into a
//! ratatui `Buffer` so that every protocol is checked the same way.

use crate::layout::{PictureKey, PictureSizes, Slot};
use image::{DynamicImage, Rgba, RgbaImage, imageops::FilterType};
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    widgets::Widget,
};
use ratatui_image::{
    errors::Errors,
    picker::{Picker, ProtocolType},
    sliced::{SignedPosition, SlicedImage, SlicedProtocol},
};
use std::collections::HashMap;

pub mod detect;
pub mod iterm2;
use iterm2::{Iterm2Renderer, Iterm2Rows};

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
pub fn visible_part(slot_top: i32, slot_rows: u16, pane: Rect) -> Option<VisiblePart> {
    let top = slot_top.max(i32::from(pane.top()));
    let bottom = (slot_top + i32::from(slot_rows)).min(i32::from(pane.bottom()));
    if bottom <= top {
        return None;
    }
    // Both values are bounded by `slot_rows` and the pane, so they fit in u16.
    Some(VisiblePart {
        skip_rows: u16::try_from(top - slot_top).ok()?,
        rows: u16::try_from(bottom - top).ok()?,
        screen_y: u16::try_from(top).ok()?,
    })
}

/// Source pixel rows `(y, height)` that correspond to the visible part of a
/// slot, for the "crop then encode" strategy.
pub fn source_crop(image_height: u32, slot_rows: u16, part: &VisiblePart) -> (u32, u32) {
    let rows = u32::from(slot_rows.max(1));
    let start = image_height * u32::from(part.skip_rows) / rows;
    let end = image_height * u32::from(part.skip_rows + part.rows) / rows;
    (start, end - start)
}

/// The pane area minus the overlay rectangles: the cells a picture may cover.
pub fn exclude_overlays(area: Rect, overlays: &[Rect]) -> Vec<Rect> {
    let mut pieces = vec![area];
    for overlay in overlays {
        pieces = pieces
            .into_iter()
            .flat_map(|piece| subtract(piece, *overlay))
            .collect();
    }
    pieces
}

/// `a` minus `b` as at most four disjoint rectangles (top, bottom, left, right).
fn subtract(a: Rect, b: Rect) -> Vec<Rect> {
    let cut = a.intersection(b);
    if cut.is_empty() {
        return vec![a];
    }
    let mut out = Vec::with_capacity(4);
    if cut.top() > a.top() {
        out.push(Rect::new(a.x, a.y, a.width, cut.top() - a.top()));
    }
    if cut.bottom() < a.bottom() {
        out.push(Rect::new(
            a.x,
            cut.bottom(),
            a.width,
            a.bottom() - cut.bottom(),
        ));
    }
    if cut.left() > a.left() {
        out.push(Rect::new(a.x, cut.y, cut.left() - a.left(), cut.height));
    }
    if cut.right() < a.right() {
        out.push(Rect::new(
            cut.right(),
            cut.y,
            a.right() - cut.right(),
            cut.height,
        ));
    }
    out
}

/// Row bands of the visible part that no overlay touches. Pixel protocols
/// (Sixel, iTerm2) cannot skip individual columns of a row, so a row that an
/// overlay crosses is hidden whole.
pub fn clear_row_bands(request: &PictureRequest<'_>, part: &VisiblePart) -> Vec<VisiblePart> {
    let x0 = request.slot_left;
    let x1 = request.slot_left.saturating_add(request.slot_cols);
    let blocked = |y: u16| {
        request
            .overlays
            .iter()
            .any(|o| o.top() <= y && y < o.bottom() && o.left() < x1 && x0 < o.right())
    };
    let mut bands: Vec<VisiblePart> = Vec::new();
    for i in 0..part.rows {
        let y = part.screen_y + i;
        if blocked(y) {
            continue;
        }
        match bands.last_mut() {
            Some(b) if b.screen_y + b.rows == y => b.rows += 1,
            _ => bands.push(VisiblePart {
                skip_rows: part.skip_rows + i,
                rows: 1,
                screen_y: y,
            }),
        }
    }
    bands
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
    fn render(&mut self, request: &PictureRequest<'_>, buf: &mut Buffer) {
        let Some(part) = visible_part(request.slot_top, request.slot_rows, request.pane) else {
            return;
        };
        let area = Rect::new(
            request.slot_left,
            part.screen_y,
            request.slot_cols,
            part.rows,
        )
        .intersection(request.pane);
        let mut symbol = [0u8; 4];
        let symbol = self.symbol.encode_utf8(&mut symbol);
        for piece in exclude_overlays(area, request.overlays) {
            for y in piece.top()..piece.bottom() {
                for x in piece.left()..piece.right() {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_symbol(symbol);
                    }
                }
            }
        }
    }
}

/// Composites `img` over an opaque background so that no pixel stays
/// transparent (SPEC §4.2: the terminal's own background must never show
/// through a picture).
pub fn flatten_over(img: &DynamicImage, bg: Rgba<u8>) -> DynamicImage {
    let mut out =
        RgbaImage::from_pixel(img.width(), img.height(), Rgba([bg[0], bg[1], bg[2], 255]));
    image::imageops::overlay(&mut out, &img.to_rgba8(), 0, 0);
    // Blending can leave alpha at 254 through rounding; the picture is opaque.
    out.pixels_mut().for_each(|p| p[3] = 255);
    DynamicImage::ImageRgba8(out)
}

/// Terminal-graphics renderer on top of `ratatui-image`'s sliced protocols
/// (Kitty unicode placeholders, per-row iTerm2 images, Sixel band slicing,
/// halfblocks). The image is encoded once at natural size; scrolling only
/// changes which rows are emitted.
pub struct SlicedRenderer {
    protocol: SlicedProtocol,
}

impl SlicedRenderer {
    pub fn new(picker: &Picker, img: DynamicImage, bg: Rgba<u8>) -> Result<Self, Errors> {
        let flat = flatten_over(&img, bg);
        Ok(Self {
            protocol: SlicedProtocol::new(picker, flat, None)?,
        })
    }

    /// Slot size in cells.
    pub fn size(&self) -> Size {
        self.protocol.size()
    }
}

impl PictureRenderer for SlicedRenderer {
    fn render(&mut self, request: &PictureRequest<'_>, buf: &mut Buffer) {
        let Some(part) = visible_part(request.slot_top, request.slot_rows, request.pane) else {
            return;
        };
        for band in clear_row_bands(request, &part) {
            let area = Rect::new(request.pane.x, band.screen_y, request.pane.width, band.rows)
                .intersection(request.pane);
            let (Ok(dx), Ok(dy)) = (
                i16::try_from(i32::from(request.slot_left) - i32::from(request.pane.x)),
                i16::try_from(band.skip_rows),
            ) else {
                continue;
            };
            SlicedImage::new(&self.protocol, SignedPosition { x: dx, y: -dy }).render(area, buf);
        }
    }
}

/// The image to encode for `slot` and the density it is encoded at (KB-72).
///
/// A raster of exactly `density` times the slot size is kept as it is. Any
/// other raster (a bitmap with fewer pixels, or one from an older slot size)
/// falls back to density 1 and is resized to the slot, the only resample left.
/// The result is then padded with `bg` to whole cells of `cell` pixels times
/// the density, so the encoder maps it onto the slot's cells one to one and
/// never stretches it.
pub fn prepare_for_encode(
    img: &DynamicImage,
    slot: &Slot,
    cell: (u32, u32),
    density: u32,
    bg: Rgba<u8>,
) -> (DynamicImage, u32) {
    let density = density.max(1);
    let dims = (img.width(), img.height());
    let want = (
        slot.px.0.max(1).saturating_mul(density),
        slot.px.1.max(1).saturating_mul(density),
    );
    let used = if density > 1 && dims == want {
        density
    } else {
        1
    };
    let target = (slot.px.0.max(1) * used, slot.px.1.max(1) * used);
    let img = if dims == target {
        img.clone()
    } else {
        img.resize_exact(target.0, target.1, FilterType::Triangle)
    };
    let cells = (
        u32::from(slot.cols) * cell.0 * used,
        u32::from(slot.rows) * cell.1 * used,
    );
    if cells.0 <= target.0 && cells.1 <= target.1 {
        return (img, used);
    }
    let mut canvas = RgbaImage::from_pixel(
        cells.0.max(target.0),
        cells.1.max(target.1),
        Rgba([bg[0], bg[1], bg[2], 255]),
    );
    image::imageops::overlay(&mut canvas, &img.to_rgba8(), 0, 0);
    (DynamicImage::ImageRgba8(canvas), used)
}

/// How pictures are turned into renderers.
enum Backend {
    /// Marker symbol per visible cell; no pixels (snapshot tests).
    Placeholder(char),
    /// Real terminal graphics through `ratatui-image`.
    Sliced { picker: Picker, bg: Rgba<u8> },
}

/// A decoded picture and its encoding for the current slot size.
struct Entry {
    img: DynamicImage,
    encoded: Option<((u32, u32), Box<dyn PictureRenderer>)>,
}

/// Decoded pictures keyed by [`PictureKey`]. Each is encoded once per slot
/// size (lazily, on first draw) and then only sliced per frame, so scrolling
/// never re-encodes. Pictures arrive through [`PictureSet::insert`] already
/// decoded (the raster and worker stages feed it).
pub struct PictureSet {
    backend: Backend,
    items: HashMap<PictureKey, Entry>,
    encodes: usize,
    /// Raster pixels per slot pixel the pictures are delivered at.
    density: u32,
}

impl std::fmt::Debug for PictureSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PictureSet")
            .field("pictures", &self.items.len())
            .field("encodes", &self.encodes)
            .finish()
    }
}

impl PictureSet {
    fn with(backend: Backend) -> Self {
        Self {
            backend,
            items: HashMap::new(),
            encodes: 0,
            density: 1,
        }
    }

    /// Draws `symbol` over the visible cells of each picture.
    pub fn placeholder(symbol: char) -> Self {
        Self::with(Backend::Placeholder(symbol))
    }

    /// Draws real graphics with the protocol of `picker`; transparent pixels
    /// are composited over `bg` (SPEC §4.2).
    /// Rasters of `density` times the slot size are encoded as they are, for
    /// iTerm2 only (cell-unit rows); other protocols always encode at 1x.
    pub fn sliced(picker: Picker, bg: Rgba<u8>, density: u32) -> Self {
        let mut set = Self::with(Backend::Sliced { picker, bg });
        set.density = density.max(1);
        set
    }

    /// Adds or replaces the picture of `key`; its encoding is dropped.
    pub fn insert(&mut self, key: PictureKey, img: DynamicImage) {
        self.items.insert(key, Entry { img, encoded: None });
    }

    /// Forgets the picture of `key`.
    pub fn remove(&mut self, key: &PictureKey) {
        self.items.remove(key);
    }

    /// How many encodes happened so far (a test and diagnostics hook).
    pub fn encode_count(&self) -> usize {
        self.encodes
    }

    /// Draws the picture of `slot` as `request` describes, clipped to the pane.
    /// Does nothing for a picture that was not inserted or cannot be encoded.
    pub fn draw(&mut self, slot: &Slot, request: &PictureRequest<'_>, buf: &mut Buffer) {
        let Some(entry) = self.items.get_mut(&slot.key) else {
            return;
        };
        if entry.encoded.as_ref().is_none_or(|(px, _)| *px != slot.px) {
            let renderer: Option<Box<dyn PictureRenderer>> = match &self.backend {
                Backend::Placeholder(symbol) => {
                    Some(Box::new(PlaceholderRenderer { symbol: *symbol }))
                }
                Backend::Sliced { picker, bg } => {
                    let font = picker.font_size();
                    let iterm2 = picker.protocol_type() == ProtocolType::Iterm2;
                    let (img, used) = prepare_for_encode(
                        &entry.img,
                        slot,
                        (u32::from(font.width), u32::from(font.height)),
                        if iterm2 { self.density } else { 1 },
                        *bg,
                    );
                    if iterm2 {
                        let flat = flatten_over(&img, *bg);
                        Iterm2Rows::new(&flat, slot.cols, slot.rows, picker.tmux_detected())
                            .ok()
                            .map(|r| Box::new(Iterm2Renderer::new(r)) as Box<dyn PictureRenderer>)
                    } else {
                        debug_assert_eq!(used, 1);
                        SlicedRenderer::new(picker, img, *bg)
                            .ok()
                            .map(|r| Box::new(r) as Box<dyn PictureRenderer>)
                    }
                }
            };
            self.encodes += 1;
            entry.encoded = renderer.map(|r| (slot.px, r));
        }
        if let Some((_, renderer)) = entry.encoded.as_mut() {
            renderer.render(request, buf);
        }
    }
}

impl PictureSizes for PictureSet {
    fn natural_size(&self, key: &PictureKey) -> Option<(u32, u32)> {
        // Slot pixels: the raster divided by the density it was made at.
        let d = self.density;
        self.items
            .get(key)
            .map(|e| (e.img.width().div_ceil(d), e.img.height().div_ceil(d)))
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

    fn slot(px: (u32, u32)) -> Slot {
        Slot {
            key: PictureKey::Diagram(0),
            cols: 4,
            rows: 4,
            px,
            left: 0,
            known: true,
        }
    }

    const BG: Rgba<u8> = Rgba([1, 2, 3, 255]);

    fn cell_slot(px: (u32, u32), cols: u16, rows: u16) -> Slot {
        Slot {
            cols,
            rows,
            ..slot(px)
        }
    }

    fn red(w: u32, h: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(w, h, Rgba([255, 0, 0, 255])))
    }

    #[test]
    fn an_svg_picture_reaches_the_encoder_at_exactly_the_rasterised_size() {
        // 4x2 cells of 10x20 px at density 2 are 80x80 px: no resize, no padding.
        let img = red(80, 80);
        let (out, d) = prepare_for_encode(&img, &cell_slot((40, 40), 4, 2), (10, 20), 2, BG);
        assert_eq!(d, 2);
        assert_eq!(out.to_rgba8().as_raw(), img.to_rgba8().as_raw());
    }

    #[test]
    fn a_picture_short_of_whole_cells_is_padded_not_resampled() {
        let (out, d) =
            prepare_for_encode(&red(70, 60), &cell_slot((35, 30), 4, 2), (10, 20), 2, BG);
        assert_eq!(d, 2);
        assert_eq!((out.width(), out.height()), (80, 80));
        let rgba = out.to_rgba8();
        assert_eq!(rgba.get_pixel(69, 59).0, [255, 0, 0, 255]);
        assert_eq!(rgba.get_pixel(79, 79).0, [1, 2, 3, 255]);
    }

    #[test]
    fn a_picture_below_the_density_falls_back_to_one_x_without_upscaling() {
        // A 30x20 bitmap in a 30x20 slot has no 2x pixels to offer.
        let (out, d) =
            prepare_for_encode(&red(30, 20), &cell_slot((30, 20), 3, 1), (10, 20), 2, BG);
        assert_eq!(d, 1);
        assert_eq!((out.width(), out.height()), (30, 20));
    }

    #[test]
    fn natural_size_is_reported_in_slot_pixels_not_raster_pixels() {
        let mut set = PictureSet::sliced(Picker::halfblocks(), BG, 2);
        set.insert(PictureKey::Diagram(0), red(80, 40));
        assert_eq!(set.natural_size(&PictureKey::Diagram(0)), Some((40, 20)));
    }

    #[test]
    fn scrolling_a_picture_encodes_it_once_per_slot_size() {
        let mut set = PictureSet::placeholder('#');
        set.insert(
            PictureKey::Diagram(0),
            DynamicImage::ImageRgba8(RgbaImage::new(80, 160)),
        );
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 12));
        for top in [3, 2, 1, 0, -1, -2] {
            let request = PictureRequest {
                pane: pane(),
                slot_top: top,
                slot_left: 30,
                slot_cols: 4,
                slot_rows: 4,
                overlays: &[],
            };
            set.draw(&slot((40, 80)), &request, &mut buf);
        }
        assert_eq!(set.encode_count(), 1);
        let request = PictureRequest {
            pane: pane(),
            slot_top: 3,
            slot_left: 30,
            slot_cols: 2,
            slot_rows: 2,
            overlays: &[],
        };
        set.draw(&slot((20, 40)), &request, &mut buf);
        assert_eq!(set.encode_count(), 2);
    }
}
