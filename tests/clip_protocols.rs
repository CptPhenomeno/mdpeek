//! Learning tests for KB-7: what `ratatui-image` emits for a fully visible,
//! top-clipped and bottom-clipped picture, per protocol. They pin facts the
//! picture renderer relies on; see docs/spikes/image-clipping.md.

#![allow(clippy::unwrap_used, clippy::expect_used)] // panics are fine in tests

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use ratatui::{
    buffer::{Buffer, CellDiffOption},
    layout::Rect,
    style::Color,
};
use ratatui_image::picker::{Picker, ProtocolType};
use readmd::graphics::{
    PictureRenderer, PictureRequest, SlicedRenderer, clear_row_bands, flatten_over, visible_part,
};

const CELL_W: u32 = 10;
const CELL_H: u32 = 20;
const COLS: u16 = 8;
const ROWS: u16 = 10;
const BG: Rgba<u8> = Rgba([30, 30, 46, 255]);

/// One opaque horizontal stripe per cell row; stripe `i` has red = 20 * i + 10.
fn striped() -> DynamicImage {
    let mut img = RgbaImage::new(u32::from(COLS) * CELL_W, u32::from(ROWS) * CELL_H);
    for (_, y, p) in img.enumerate_pixels_mut() {
        let row = (y / CELL_H) as u8;
        *p = Rgba([20 * row + 10, 200, 100, 255]);
    }
    DynamicImage::ImageRgba8(img)
}

fn picker(kind: ProtocolType) -> Picker {
    let mut p = Picker::halfblocks();
    p.set_protocol_type(kind);
    p
}

fn pane() -> Rect {
    Rect::new(20, 1, 40, 10)
}

fn render(kind: ProtocolType, slot_top: i32, overlays: &[Rect]) -> Buffer {
    let mut r = SlicedRenderer::new(&picker(kind), striped(), BG).unwrap();
    let size = r.size();
    assert_eq!(
        (size.width, size.height),
        (COLS, ROWS),
        "{kind:?} slot size"
    );
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 14));
    let req = PictureRequest {
        pane: pane(),
        slot_top,
        slot_left: 30,
        slot_cols: COLS,
        slot_rows: ROWS,
        overlays,
    };
    r.render(&req, &mut buf);
    buf
}

/// Cells the picture claimed: a non-blank symbol, or a cell the diff must skip
/// (the interior of a Sixel or iTerm2 image).
fn touched(buf: &Buffer) -> Vec<(u16, u16)> {
    let mut v = Vec::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            if cell.symbol() != " " || cell.diff_option == CellDiffOption::Skip {
                v.push((x, y));
            }
        }
    }
    v
}

fn bounds(cells: &[(u16, u16)]) -> (u16, u16, u16, u16) {
    let xs = cells.iter().map(|c| c.0);
    let ys = cells.iter().map(|c| c.1);
    (
        xs.clone().min().unwrap(),
        ys.clone().min().unwrap(),
        xs.max().unwrap(),
        ys.max().unwrap(),
    )
}

fn base64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().take_while(|c| *c != b'=') {
        acc = (acc << 6) | u32::from(val(c));
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// Decode the PNG carried by an iTerm2 `]1337;File=` cell symbol.
fn iterm_png(symbol: &str) -> DynamicImage {
    let start = symbol.find("File=inline=1;").unwrap();
    let payload = &symbol[start..];
    let b64 = &payload[payload.find(':').unwrap() + 1..payload.find('\x07').unwrap()];
    image::load_from_memory(&base64(b64)).unwrap()
}

const ALL: [ProtocolType; 4] = [
    ProtocolType::Kitty,
    ProtocolType::Sixel,
    ProtocolType::Iterm2,
    ProtocolType::Halfblocks,
];

#[test]
fn every_protocol_stays_inside_the_slot_and_the_pane_when_fully_visible_top_or_bottom_clipped() {
    for kind in ALL {
        // fully visible: slot rows 3..13 would overflow the pane (1..11), use top=1
        let full = bounds(&touched(&render(kind, 1, &[])));
        assert_eq!(full, (30, 1, 37, 10), "{kind:?} full");
        let top = bounds(&touched(&render(kind, -2, &[])));
        assert_eq!(
            top,
            (30, 1, 37, 7),
            "{kind:?} top-clipped (3 rows hidden above the pane)"
        );
        let bottom = bounds(&touched(&render(kind, 6, &[])));
        assert_eq!(bottom, (30, 6, 37, 10), "{kind:?} bottom-clipped");
    }
}

#[test]
fn nothing_is_drawn_outside_the_pane_even_with_a_far_off_slot() {
    for kind in ALL {
        assert!(
            touched(&render(kind, -20, &[])).is_empty(),
            "{kind:?} above"
        );
        assert!(touched(&render(kind, 30, &[])).is_empty(), "{kind:?} below");
    }
}

#[test]
fn kitty_uses_unicode_placeholders_and_a_skipped_row_offset_when_top_clipped() {
    let full = render(ProtocolType::Kitty, 1, &[]);
    let clipped = render(ProtocolType::Kitty, -2, &[]);
    let strip = |c: &ratatui::buffer::Cell| {
        let s = c.symbol();
        // row and column diacritics; the third one encodes the random image id
        s[s.find('\u{10EEEE}').unwrap()..]
            .chars()
            .take(3)
            .collect::<String>()
    };
    // first drawn cell of a clipped picture names image row 3 (the 4th)
    assert_eq!(strip(&clipped[(30, 1)]), strip(&full[(30, 4)]));
    // placeholders carry the image id in the foreground colour
    assert!(matches!(clipped[(30, 1)].fg, Color::Rgb(..)));
    // the transmit APC rides on the first cell, exactly once
    let apc: usize = touched(&clipped)
        .iter()
        .filter(|c| clipped[**c].symbol().contains("\x1b_G"))
        .count();
    assert_eq!(apc, 1);
    // cells keep their own background: transparent pixels show the cell bg
    assert_eq!(clipped[(30, 1)].bg, Color::Reset);
}

#[test]
fn iterm2_emits_one_png_per_row_and_a_clipped_picture_carries_the_right_stripes() {
    let clipped = render(ProtocolType::Iterm2, -2, &[]);
    for (i, y) in (1u16..=7).enumerate() {
        let sym = clipped[(30, y)].symbol();
        assert!(
            sym.contains("\x1b[8X"),
            "row {y} clears its own cells first"
        );
        let png = iterm_png(sym);
        assert_eq!(png.dimensions(), (80, 20));
        let want = (20 * (i as u8 + 3)) + 10;
        assert_eq!(
            png.get_pixel(5, 10)[0],
            want,
            "row {y} shows stripe {}",
            i + 3
        );
    }
}

#[test]
fn sixel_clipping_drops_whole_bands_and_loses_under_one_cell_of_accuracy() {
    let count = |b: &Buffer| -> usize { b[(30, 1)].symbol().matches('-').count() };
    let full = count(&render(ProtocolType::Sixel, 1, &[]));
    let clipped = count(&render(ProtocolType::Sixel, -2, &[]));
    // 20 px cell rows = 3.33 sixel bands: a 7-row picture is 140 px = 23 bands
    assert!(full >= 33, "full has {full} bands");
    assert!((22..=24).contains(&clipped), "clipped has {clipped} bands");
    let sym = render(ProtocolType::Sixel, -2, &[]);
    assert!(
        sym[(30, 1)].symbol().contains("\x1b[8X"),
        "clears the visible area first"
    );
}

#[test]
fn halfblocks_colours_follow_the_stripes_after_clipping() {
    let clipped = render(ProtocolType::Halfblocks, -2, &[]);
    for (i, y) in (1u16..=7).enumerate() {
        let want = 20 * (i as u8 + 3) + 10;
        let Color::Rgb(r, g, b) = clipped[(31, y)].fg else {
            panic!("row {y}: not an rgb colour");
        };
        // the half-cell resampling blends a few levels from the next stripe
        assert!(
            r.abs_diff(want) <= 6 && (g, b) == (200, 100),
            "row {y}: {r},{g},{b}"
        );
    }
}

#[test]
fn transparent_pixels_are_flattened_over_the_theme_background() {
    let clear = DynamicImage::ImageRgba8(RgbaImage::from_pixel(80, 40, Rgba([255, 0, 0, 0])));
    let flat = flatten_over(&clear, BG);
    assert!(flat.to_rgba8().pixels().all(|p| *p == BG));
    // half-transparent red blends, never leaves alpha behind
    let half = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 128])));
    let p = flatten_over(&half, BG).to_rgba8()[(0, 0)];
    assert_eq!(p[3], 255);
    assert!(p[0] > 130 && p[0] < 150, "blended red {}", p[0]);

    let mut r = SlicedRenderer::new(&picker(ProtocolType::Halfblocks), clear, BG).unwrap();
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 14));
    let req = PictureRequest {
        pane: pane(),
        slot_top: 1,
        slot_left: 30,
        slot_cols: 8,
        slot_rows: 2,
        overlays: &[],
    };
    r.render(&req, &mut buf);
    assert_eq!(buf[(30, 1)].fg, Color::Rgb(30, 30, 46));
    assert_eq!(buf[(30, 1)].bg, Color::Rgb(30, 30, 46));
}

#[test]
fn rows_crossed_by_an_overlay_are_hidden_for_every_protocol() {
    let overlay = [Rect::new(33, 4, 10, 2)]; // rows 4 and 5 over the picture
    let req = PictureRequest {
        pane: pane(),
        slot_top: 1,
        slot_left: 30,
        slot_cols: COLS,
        slot_rows: ROWS,
        overlays: &overlay,
    };
    let part = visible_part(1, ROWS, pane()).unwrap();
    let bands = clear_row_bands(&req, &part);
    assert_eq!(bands.len(), 2);
    assert_eq!((bands[0].screen_y, bands[0].rows), (1, 3));
    assert_eq!(
        (bands[1].screen_y, bands[1].rows, bands[1].skip_rows),
        (6, 5, 5)
    );
    for kind in ALL {
        let buf = render(kind, 1, &overlay);
        for y in [4u16, 5] {
            for x in 30..38 {
                assert_eq!(buf[(x, y)].symbol(), " ", "{kind:?} cell {x},{y}");
            }
        }
        assert_ne!(
            buf[(30, 1)].symbol(),
            " ",
            "{kind:?} band above the overlay"
        );
        assert_ne!(
            buf[(30, 6)].symbol(),
            " ",
            "{kind:?} band below the overlay"
        );
    }
}

#[test]
fn scrolling_rewrites_every_cell_the_picture_left_so_no_fragment_stays() {
    for kind in ALL {
        let before = render(kind, 1, &[]);
        let after = render(kind, -2, &[]);
        let diff = before.diff(&after);
        // rows 8..=10 held picture in `before` and are blank in `after`
        for y in 8u16..=10 {
            for x in 30..38 {
                assert!(
                    diff.iter().any(|(dx, dy, _)| (*dx, *dy) == (x, y)),
                    "{kind:?}: cell {x},{y} is not redrawn after scrolling"
                );
            }
        }
    }
}
