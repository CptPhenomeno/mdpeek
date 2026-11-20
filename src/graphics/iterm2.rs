//! iTerm2 inline images sized in cells (KB-72).
//!
//! ratatui-image's iTerm2 encoder sizes an image in pixels, so a raster with
//! more pixels than the slot would be drawn bigger. Here every row slice of the
//! picture is its own escape sequence with `width=<cols>;height=1` in cell
//! units: the terminal scales the slice into the cell box, and a raster of
//! `density` times the slot size renders sharper instead of larger.

use image::DynamicImage;

/// The escape sequences of a picture, one per slot row.
pub struct Iterm2Rows {
    rows: Vec<String>,
}

impl Iterm2Rows {
    /// Slices `img` into `rows` equal bands and encodes each for a box of `cols` cells.
    pub fn new(
        _img: &DynamicImage,
        _cols: u16,
        _rows: u16,
        _tmux: bool,
    ) -> Result<Self, image::ImageError> {
        Ok(Self { rows: Vec::new() })
    }

    /// One escape sequence per slot row, top to bottom.
    pub fn slices(&self) -> &[String] {
        &self.rows
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
