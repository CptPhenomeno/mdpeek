//! iTerm2 inline images sized in cells (KB-72).
//!
//! ratatui-image's iTerm2 encoder sizes an image in pixels, so a raster with
//! more pixels than the slot would be drawn bigger. Here every row slice of the
//! picture is its own escape sequence with `width=<cols>;height=1` in cell
//! units: the terminal scales the slice into the cell box, and a raster of
//! `density` times the slot size renders sharper instead of larger.

use crate::graphics::{PictureRenderer, PictureRequest, clear_row_bands, visible_part};
use base64::Engine;
use image::DynamicImage;
use ratatui::buffer::{Buffer, CellDiffOption};
use std::{fmt::Write, io::Cursor, num::NonZeroU16};

/// The escape sequences of a picture, one per slot row.
pub struct Iterm2Rows {
    rows: Vec<String>,
    cols: u16,
}

impl Iterm2Rows {
    /// Slices `img` into `rows` equal bands and encodes each for a box of `cols` cells.
    pub fn new(
        img: &DynamicImage,
        cols: u16,
        rows: u16,
        tmux: bool,
    ) -> Result<Self, image::ImageError> {
        let band = (img.height() / u32::from(rows.max(1))).max(1);
        let (start, escape, end) = if tmux {
            ("\x1bPtmux;", "\x1b\x1b", "\x1b\\")
        } else {
            ("", "\x1b", "")
        };
        let mut out = Vec::with_capacity(usize::from(rows));
        for i in 0..u32::from(rows) {
            let y = (i * band).min(img.height().saturating_sub(1));
            let slice = img.crop_imm(0, y, img.width(), band.min(img.height() - y));
            let mut png = Vec::new();
            slice.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)?;
            let mut seq = String::from(start);
            // Erase the cells first: a translucent edge must not show stale text.
            let _ = write!(
                seq,
                "{escape}[{cols}X{escape}]1337;File=inline=1;size={};width={cols};height=1;preserveAspectRatio=0;doNotMoveCursor=1:",
                png.len()
            );
            base64::engine::general_purpose::STANDARD.encode_string(&png, &mut seq);
            let _ = write!(seq, "\x07{end}");
            out.push(seq);
        }
        Ok(Self { rows: out, cols })
    }

    /// One escape sequence per slot row, top to bottom.
    pub fn slices(&self) -> &[String] {
        &self.rows
    }
}

/// Draws the row slices of a picture, clipped to the pane and the overlays.
pub struct Iterm2Renderer {
    rows: Iterm2Rows,
}

impl Iterm2Renderer {
    pub fn new(rows: Iterm2Rows) -> Self {
        Self { rows }
    }
}

impl PictureRenderer for Iterm2Renderer {
    fn render(&mut self, request: &PictureRequest<'_>, buf: &mut Buffer) {
        let cols = self.rows.cols;
        let Some(part) = visible_part(request.slot_top, request.slot_rows, request.pane) else {
            return;
        };
        if request.slot_left < request.pane.left()
            || u32::from(request.slot_left) + u32::from(cols) > u32::from(request.pane.right())
        {
            return;
        }
        let Some(forced) = NonZeroU16::new(1) else {
            return;
        };
        for band in clear_row_bands(request, &part) {
            for i in 0..band.rows {
                let Some(seq) = self.rows.rows.get(usize::from(band.skip_rows + i)) else {
                    continue;
                };
                let y = band.screen_y + i;
                for dx in 0..cols {
                    let Some(cell) = buf.cell_mut((request.slot_left + dx, y)) else {
                        continue;
                    };
                    if dx == 0 {
                        cell.set_symbol(seq)
                            .set_diff_option(CellDiffOption::ForcedWidth(forced));
                    } else {
                        cell.set_diff_option(CellDiffOption::Skip);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use image::{Rgba, RgbaImage};

    fn payload(seq: &str) -> RgbaImage {
        let start = seq.find(':').unwrap() + 1;
        let end = seq.find('\x07').unwrap();
        let png = base64::engine::general_purpose::STANDARD
            .decode(&seq[start..end])
            .unwrap();
        image::load_from_memory(&png).unwrap().to_rgba8()
    }

    #[test]
    fn rows_are_sized_in_cells_one_slice_per_row_at_density_height() {
        // 4x3 cells of 10x20 px at density 2: 80x120 px, slices 80x40 px.
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(80, 120, Rgba([9, 8, 7, 255])));
        let rows = Iterm2Rows::new(&img, 4, 3, false).unwrap();
        assert_eq!(rows.slices().len(), 3);
        for seq in rows.slices() {
            assert!(
                seq.contains(";width=4;height=1;preserveAspectRatio=0;"),
                "{}",
                &seq[..seq.find(':').unwrap()]
            );
            assert!(!seq.contains("px"), "pixel units would scale the slice");
            assert_eq!(payload(seq).dimensions(), (80, 40));
        }
    }

    #[test]
    fn tmux_wraps_each_row_in_a_passthrough() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(20, 20));
        let rows = Iterm2Rows::new(&img, 2, 1, true).unwrap();
        assert!(rows.slices()[0].starts_with("\x1bPtmux;"));
        assert!(rows.slices()[0].ends_with("\x1b\\"));
    }
}
