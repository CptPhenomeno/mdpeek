//! SVG rasterisation with resvg (SPEC §3.4.1, §3.5.2, §4.12).
//!
//! Pictures are drawn at their natural size and scaled down only to fit the
//! content width and the pane height, never up. The output keeps transparency;
//! compositing over the theme background happens later, in
//! `graphics::flatten_over`, at encode time (SPEC §4.2), so it is not done here.

use image::RgbaImage;
use resvg::{tiny_skia, usvg};
use std::sync::{Arc, OnceLock};

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

/// Shared font database. Loading system fonts takes about 20 ms (SPEC spike
/// KB-6), so it happens once, on the first parse, and never on the first-paint
/// path: callers run this module from the raster worker.
fn fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

fn parse(svg: &str) -> Result<usvg::Tree, RasterError> {
    let opt = usvg::Options {
        fontdb: fontdb(),
        ..Default::default()
    };
    usvg::Tree::from_str(svg, &opt).map_err(|e| RasterError::Invalid(e.to_string()))
}

/// Intrinsic size of an SVG in pixels, parsed without rasterising.
pub fn intrinsic_size(svg: &str) -> Result<(f32, f32), RasterError> {
    let size = parse(svg)?.size();
    Ok((size.width(), size.height()))
}

/// Rasterise at natural size, scaled down (aspect kept) to fit `max_w` x `max_h`.
///
/// Never scales up. When a limit binds, that dimension equals the limit exactly.
/// The result keeps transparency.
pub fn rasterise(svg: &str, max_w: u32, max_h: u32) -> Result<RgbaImage, RasterError> {
    let tree = parse(svg)?;
    let (w, h) = (tree.size().width(), tree.size().height());
    if !(w > 0.0 && h > 0.0) || max_w == 0 || max_h == 0 {
        return Err(RasterError::Size);
    }
    let (mw, mh) = (max_w as f32, max_h as f32);
    let (pw, ph) = if w <= mw && h <= mh {
        (w.ceil() as u32, h.ceil() as u32)
    } else if mw / w <= mh / h {
        (max_w, ((h * mw / w).round() as u32).clamp(1, max_h))
    } else {
        (((w * mh / h).round() as u32).clamp(1, max_w), max_h)
    };
    let mut pm = tiny_skia::Pixmap::new(pw, ph).ok_or(RasterError::Size)?;
    let transform = tiny_skia::Transform::from_scale(pw as f32 / w, ph as f32 / h);
    resvg::render(&tree, transform, &mut pm.as_mut());
    // tiny-skia pixels are premultiplied; the image crate expects straight alpha.
    let mut data = pm.take();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    RgbaImage::from_raw(pw, ph, data).ok_or(RasterError::Size)
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
