//! SVG rasterisation with resvg (SPEC §3.4.1, §3.5.2, §4.12).
//!
//! Pictures are drawn at their natural size and scaled down only to fit the
//! content width and the pane height, never up. The output keeps transparency;
//! compositing over the theme background happens later, in
//! `graphics::flatten_over`, at encode time (SPEC §4.2), so it is not done here.

use image::RgbaImage;

/// Why an SVG could not be rasterised.
#[derive(Debug)]
pub enum RasterError {
    /// The SVG did not parse.
    Invalid(String),
    /// The SVG has no drawable size, or the bitmap could not be allocated.
    Size,
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(e) => write!(f, "invalid SVG: {e}"),
            Self::Size => write!(f, "SVG has no drawable size"),
        }
    }
}

impl std::error::Error for RasterError {}

/// Intrinsic size of an SVG in pixels, parsed without rasterising.
pub fn intrinsic_size(_svg: &str) -> Result<(f32, f32), RasterError> {
    unimplemented!()
}

/// Rasterise at natural size, scaled down (aspect kept) to fit `max_w` x `max_h`.
pub fn rasterise(_svg: &str, _max_w: u32, _max_h: u32) -> Result<RgbaImage, RasterError> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg(w: u32, h: u32, body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}">{body}</svg>"#)
    }
    const RECT: &str = r##"<rect width="100%" height="100%" fill="#f00"/>"##;

    #[test]
    fn smaller_than_limits_keeps_natural_size() {
        let img = rasterise(&svg(200, 100, RECT), 1000, 1000).unwrap();
        assert_eq!(img.dimensions(), (200, 100));
    }

    #[test]
    fn wider_than_max_width_scales_down_keeping_aspect() {
        let img = rasterise(&svg(400, 200, RECT), 100, 1000).unwrap();
        assert_eq!(img.dimensions(), (100, 50));
    }

    #[test]
    fn taller_than_cap_scales_down_to_cap_height() {
        let img = rasterise(&svg(100, 400, RECT), 1000, 200).unwrap();
        assert_eq!(img.dimensions(), (50, 200));
    }

    #[test]
    fn transparent_background_stays_transparent() {
        let body = r##"<rect x="0" y="0" width="10" height="10" fill="#f00"/>"##;
        let img = rasterise(&svg(20, 20, body), 100, 100).unwrap();
        assert_eq!(img.get_pixel(19, 19).0[3], 0);
        assert_eq!(img.get_pixel(2, 2).0, [255, 0, 0, 255]);
    }

    #[test]
    fn text_is_drawn_with_system_fonts() {
        let body =
            r##"<text x="5" y="40" font-size="40" font-family="sans-serif" fill="#000">Hi</text>"##;
        let img = rasterise(&svg(100, 60, body), 100, 100).unwrap();
        assert!(img.pixels().any(|p| p.0[3] > 0), "no glyph pixels drawn");
    }

    #[test]
    fn intrinsic_size_is_reported_without_rasterising() {
        // A huge canvas would fail to allocate if rasterised.
        let (w, h) = intrinsic_size(&svg(100_000, 50_000, RECT)).unwrap();
        assert_eq!((w, h), (100_000.0, 50_000.0));
    }

    #[test]
    fn invalid_svg_returns_an_error() {
        assert!(matches!(
            rasterise("not svg at all", 100, 100),
            Err(RasterError::Invalid(_))
        ));
        assert!(intrinsic_size("<svg").is_err());
    }
}
