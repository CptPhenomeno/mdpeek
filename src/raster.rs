//! SVG rasterisation with resvg (SPEC §3.4.1, §3.5.2, §4.12).
//!
//! Pictures are drawn at their natural size and scaled down only to fit the
//! content width and the pane height, never up. The output keeps transparency;
//! compositing over the theme background happens later, in
//! `graphics::flatten_over`, at encode time (SPEC §4.2), so it is not done here.

use image::RgbaImage;
use resvg::{tiny_skia, usvg};
use std::path::Path;
use std::sync::{Arc, OnceLock};

/// Why an SVG could not be rasterised.
#[derive(Debug)]
pub enum RasterError {
    /// The SVG did not parse.
    Invalid(String),
    /// The SVG has no drawable size, or the bitmap could not be allocated.
    Size,
    /// The file is missing, unreadable or not a decodable image (SPEC §3.5.3).
    NotFound,
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(e) => write!(f, "invalid SVG: {e}"),
            Self::Size => write!(f, "SVG has no drawable size"),
            Self::NotFound => write!(f, "image not found or unreadable"),
        }
    }
}

impl std::error::Error for RasterError {}

/// Shared font database. Loading system fonts takes about 20 ms (SPEC spike
/// KB-6), so it happens once, on the first parse, and never on the first-paint
/// path: callers run this module from the raster worker.
///
/// Also the font setup tests should rasterise with: system fonts, the bundled
/// faces and the generic-family fallback.
pub fn fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        load_bundled(&mut db);
        Arc::new(db)
    })
    .clone()
}

/// Loads the faces diagrams are measured with (see `diagram::FONT_FAMILY`)
/// and makes every generic family resolve to something.
fn load_bundled(db: &mut usvg::fontdb::Database) {
    for face in crate::diagram::FONT_FACES {
        db.load_font_data(face.to_vec());
    }
    apply_generic_fallbacks(db);
}

/// Points every generic family whose configured name has no face in `db` at
/// the bundled Liberation Sans; families that resolve are left alone. Without
/// this, `sans-serif` (and usvg's last-resort serif) draw nothing on hosts
/// lacking fontdb's default names.
fn apply_generic_fallbacks(db: &mut usvg::fontdb::Database) {
    apply_generic_fallbacks_to(db, "Liberation Sans");
}

fn apply_generic_fallbacks_to(db: &mut usvg::fontdb::Database, fallback: &str) {
    use usvg::fontdb::{Database, Family};
    type Setter = fn(&mut Database, String);
    let generics: [(Family<'static>, Setter); 5] = [
        (Family::Serif, |d, n| d.set_serif_family(n)),
        (Family::SansSerif, |d, n| d.set_sans_serif_family(n)),
        (Family::Monospace, |d, n| d.set_monospace_family(n)),
        (Family::Cursive, |d, n| d.set_cursive_family(n)),
        (Family::Fantasy, |d, n| d.set_fantasy_family(n)),
    ];
    for (family, set) in generics {
        let name = db.family_name(&family).to_string();
        let present = db
            .faces()
            .any(|f| f.families.iter().any(|(n, _)| *n == name));
        if !present {
            set(db, fallback.to_string());
        }
    }
}

fn parse(svg: &str) -> Result<usvg::Tree, RasterError> {
    parse_with(svg, fontdb())
}

fn parse_with(svg: &str, fontdb: Arc<usvg::fontdb::Database>) -> Result<usvg::Tree, RasterError> {
    let opt = usvg::Options {
        fontdb,
        ..Default::default()
    };
    usvg::Tree::from_str(svg, &opt).map_err(|e| RasterError::Invalid(e.to_string()))
}

/// Intrinsic size of an SVG in pixels, parsed without rasterising.
pub fn intrinsic_size(svg: &str) -> Result<(f32, f32), RasterError> {
    let size = parse(svg)?.size();
    Ok((size.width(), size.height()))
}

/// Pixel size for a picture of natural size `w` x `h` fitted into
/// `max_w` x `max_h`: natural size (rounded up) when it fits, otherwise scaled
/// down with the aspect kept so the binding limit is met exactly. Never scales
/// up. Shared by diagrams, SVG and bitmaps so they all round identically.
/// Callers guarantee positive `w`, `h`, `max_w` and `max_h`.
fn fit_size(w: f32, h: f32, max_w: u32, max_h: u32) -> (u32, u32) {
    let (mw, mh) = (max_w as f32, max_h as f32);
    if w <= mw && h <= mh {
        (w.ceil() as u32, h.ceil() as u32)
    } else if mw / w <= mh / h {
        (max_w, ((h * mw / w).round() as u32).clamp(1, max_h))
    } else {
        (((w * mh / h).round() as u32).clamp(1, max_w), max_h)
    }
}

/// Rasterise at natural size, scaled down (aspect kept) to fit `max_w` x `max_h`
/// (the slot size). The SVG is drawn once, at that final size, one pixel per
/// screen pixel (KB-74): the terminal never has to resample it.
///
/// Never scales up. When a limit binds, that dimension equals the limit
/// exactly. The result keeps transparency.
pub fn rasterise(svg: &str, max_w: u32, max_h: u32) -> Result<RgbaImage, RasterError> {
    let tree = parse(svg)?;
    let (w, h) = (tree.size().width(), tree.size().height());
    if !(w > 0.0 && h > 0.0) || max_w == 0 || max_h == 0 {
        return Err(RasterError::Size);
    }
    let (pw, ph) = fit_size(w, h, max_w, max_h);
    let mut pm = tiny_skia::Pixmap::new(pw, ph).ok_or(RasterError::Size)?;
    let transform = tiny_skia::Transform::from_scale(pw as f32 / w, ph as f32 / h);
    resvg::render(&tree, transform, &mut pm.as_mut());
    pixmap_image(pm)
}

/// Pixmap to straight-alpha image (tiny-skia pixels are premultiplied; the
/// image crate expects straight alpha).
fn pixmap_image(pm: tiny_skia::Pixmap) -> Result<RgbaImage, RasterError> {
    let (pw, ph) = (pm.width(), pm.height());
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

/// Rasterise only the region whose top-left corner is `origin` (natural pixels) at `scale` into a
/// `px` sized bitmap, so the memory is bounded by the screen at any zoom
/// (SPEC §4.20). Drawn from the vector source: nothing is upscaled.
pub fn rasterise_region(
    svg: &str,
    origin: (f64, f64),
    scale: f64,
    px: (u32, u32),
) -> Result<RgbaImage, RasterError> {
    let tree = parse(svg)?;
    if px.0 == 0 || px.1 == 0 || scale.is_nan() || scale <= 0.0 {
        return Err(RasterError::Size);
    }
    let mut pm = tiny_skia::Pixmap::new(px.0, px.1).ok_or(RasterError::Size)?;
    let s = scale as f32;
    let transform = tiny_skia::Transform::from_scale(s, s)
        .post_translate(-(origin.0 as f32) * s, -(origin.1 as f32) * s);
    resvg::render(&tree, transform, &mut pm.as_mut());
    pixmap_image(pm)
}

fn is_svg(path: &Path, head: &[u8]) -> bool {
    let ext = path.extension().and_then(|e| e.to_str());
    if ext.is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        return true;
    }
    // No extension hint: sniff for an <svg root in the first bytes.
    let head = String::from_utf8_lossy(head);
    head.contains("<svg")
}

fn read_head(path: &Path) -> Result<Vec<u8>, RasterError> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(1024).read_to_end(&mut buf))
        .map_err(|_| RasterError::NotFound)?;
    Ok(buf)
}

fn read_svg(path: &Path) -> Result<String, RasterError> {
    std::fs::read_to_string(path).map_err(|_| RasterError::NotFound)
}

/// Natural pixel size of a local image, read from the header without decoding
/// the pixels (SVG: parsed, rounded up like `rasterise`). Missing or
/// unreadable files are `NotFound` (SPEC §3.5.3).
pub fn image_size(path: &Path) -> Result<(u32, u32), RasterError> {
    let head = read_head(path)?;
    if is_svg(path, &head) {
        let (w, h) = intrinsic_size(&read_svg(path)?).map_err(|_| RasterError::NotFound)?;
        return Ok((w.ceil() as u32, h.ceil() as u32));
    }
    image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map_err(|_| RasterError::NotFound)?
        .into_dimensions()
        .map_err(|_| RasterError::NotFound)
}

/// Decode a local image (PNG, JPEG, WebP, GIF first frame, SVG) to
/// straight-alpha RGBA, scaled down to fit `max_w` x `max_h` and never up.
/// Missing, unreadable or corrupt files are `NotFound` (SPEC §3.5.3).
pub fn decode_image(path: &Path, max_w: u32, max_h: u32) -> Result<RgbaImage, RasterError> {
    let head = read_head(path)?;
    if is_svg(path, &head) {
        return rasterise(&read_svg(path)?, max_w, max_h).map_err(|_| RasterError::NotFound);
    }
    let img = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .map_err(|_| RasterError::NotFound)?
        .decode()
        .map_err(|_| RasterError::NotFound)?
        .into_rgba8();
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 || max_w == 0 || max_h == 0 {
        return Err(RasterError::Size);
    }
    let (pw, ph) = fit_size(w as f32, h as f32, max_w, max_h);
    if (pw, ph) == (w, h) {
        return Ok(img);
    }
    Ok(image::imageops::resize(
        &img,
        pw,
        ph,
        image::imageops::FilterType::Lanczos3,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg(w: u32, h: u32, body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}">{body}</svg>"#)
    }
    #[test]
    fn region_raster_draws_only_the_requested_part_at_the_scale() {
        // Left half blue, right half red; ask for the right half at x2.
        let body = r##"<rect width="50" height="100" fill="#00f"/><rect x="50" width="50" height="100" fill="#f00"/>"##;
        let img = rasterise_region(&svg(100, 100, body), (50.0, 0.0), 2.0, (100, 200)).unwrap();
        assert_eq!((img.width(), img.height()), (100, 200));
        assert_eq!(img.get_pixel(1, 1).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(98, 198).0, [255, 0, 0, 255]);
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

    /// A database holding only the bundled faces, as on a bare CI host.
    fn bundled_only() -> Arc<usvg::fontdb::Database> {
        let mut db = usvg::fontdb::Database::new();
        load_bundled(&mut db);
        Arc::new(db)
    }

    fn drawn_with(db: Arc<usvg::fontdb::Database>, family: &str) -> bool {
        let body = format!(
            r##"<text x="5" y="40" font-size="40" font-family="{family}" fill="#000">Hi</text>"##
        );
        let tree = parse_with(&svg(100, 60, &body), db).unwrap();
        let mut pm = tiny_skia::Pixmap::new(100, 60).unwrap();
        resvg::render(&tree, tiny_skia::Transform::identity(), &mut pm.as_mut());
        pm.pixels().iter().any(|p| p.alpha() > 0)
    }

    #[test]
    fn bundled_only_database_draws_sans_serif_text() {
        assert!(drawn_with(bundled_only(), "sans-serif"));
    }

    #[test]
    fn bundled_only_database_draws_serif_text() {
        assert!(drawn_with(bundled_only(), "serif"));
    }

    #[test]
    fn unresolved_generic_families_map_to_liberation_sans() {
        use usvg::fontdb::Family;
        let db = bundled_only();
        for f in [Family::Monospace, Family::Cursive, Family::Fantasy] {
            assert_eq!(db.family_name(&f), "Liberation Sans");
        }
        assert!(drawn_with(db, "monospace"));
    }

    #[test]
    fn resolving_generic_family_keeps_its_mapping() {
        use usvg::fontdb::Family;
        let mut db = usvg::fontdb::Database::new();
        load_bundled(&mut db);
        // Serif points at a present face; monospace at one that does not exist.
        db.set_serif_family("Liberation Sans");
        db.set_monospace_family("No Such Font");
        apply_generic_fallbacks_to(&mut db, "Fallback Probe");
        assert_eq!(db.family_name(&Family::Serif), "Liberation Sans");
        assert_eq!(db.family_name(&Family::Monospace), "Fallback Probe");
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

    // ---- local images (KB-33) ----

    use image::{ImageEncoder, Rgba};
    use std::path::PathBuf;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba(c))
    }

    fn save(dir: &tempfile::TempDir, name: &str, img: &RgbaImage) -> PathBuf {
        let p = dir.path().join(name);
        img.save(&p).unwrap();
        p
    }

    fn near(a: [u8; 4], b: [u8; 4], tol: i32) -> bool {
        a.iter()
            .zip(b)
            .all(|(x, y)| (*x as i32 - y as i32).abs() <= tol)
    }

    #[test]
    fn png_jpeg_webp_gif_decode_to_rgba() {
        let dir = tempfile::tempdir().unwrap();
        let red = solid(16, 8, [255, 0, 0, 255]);
        let png = save(&dir, "a.png", &red);
        let gif = save(&dir, "a.gif", &red);
        let webp = dir.path().join("a.webp");
        let mut buf = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut buf)
            .write_image(red.as_raw(), 16, 8, image::ExtendedColorType::Rgba8)
            .unwrap();
        std::fs::write(&webp, buf).unwrap();
        let jpeg = dir.path().join("a.jpg");
        image::DynamicImage::ImageRgba8(red.clone())
            .to_rgb8()
            .save(&jpeg)
            .unwrap();
        for p in [png, gif, webp, jpeg] {
            let img = decode_image(&p, 100, 100).unwrap();
            assert_eq!(img.dimensions(), (16, 8), "{p:?}");
            assert!(near(img.get_pixel(8, 4).0, [255, 0, 0, 255], 12), "{p:?}");
        }
    }

    #[test]
    fn animated_gif_uses_its_first_frame() {
        use image::codecs::gif::GifEncoder;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("anim.gif");
        let mut enc = GifEncoder::new(std::fs::File::create(&p).unwrap());
        enc.encode_frame(image::Frame::new(solid(8, 8, [255, 0, 0, 255])))
            .unwrap();
        enc.encode_frame(image::Frame::new(solid(8, 8, [0, 0, 255, 255])))
            .unwrap();
        drop(enc);
        let img = decode_image(&p, 100, 100).unwrap();
        assert_eq!(img.get_pixel(4, 4).0, [255, 0, 0, 255]);
    }

    #[test]
    fn svg_image_file_is_rasterised_through_resvg() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("pic.svg");
        std::fs::write(&p, svg(40, 20, RECT)).unwrap();
        let img = decode_image(&p, 100, 100).unwrap();
        assert_eq!(img.dimensions(), (40, 20));
        assert_eq!(img.get_pixel(5, 5).0, [255, 0, 0, 255]);
        assert_eq!(image_size(&p).unwrap(), (40, 20));
    }

    #[test]
    fn image_wider_than_pane_scales_down_to_pane_width() {
        let dir = tempfile::tempdir().unwrap();
        let p = save(&dir, "w.png", &solid(400, 200, [0, 255, 0, 255]));
        let img = decode_image(&p, 100, 1000).unwrap();
        assert_eq!(img.dimensions(), (100, 50));
    }

    #[test]
    fn image_smaller_than_pane_keeps_natural_size() {
        let dir = tempfile::tempdir().unwrap();
        let p = save(&dir, "s.png", &solid(30, 20, [0, 255, 0, 255]));
        let img = decode_image(&p, 1000, 1000).unwrap();
        assert_eq!(img.dimensions(), (30, 20));
    }

    #[test]
    fn image_taller_than_pane_is_capped_keeping_aspect() {
        let dir = tempfile::tempdir().unwrap();
        let p = save(&dir, "t.png", &solid(100, 400, [0, 255, 0, 255]));
        let img = decode_image(&p, 1000, 200).unwrap();
        assert_eq!(img.dimensions(), (50, 200));
    }

    #[test]
    fn image_dimensions_are_read_without_decoding_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = save(&dir, "d.png", &solid(64, 32, [1, 2, 3, 255]));
        // Keep the header, drop the pixel data: a full decode would fail.
        let bytes = std::fs::read(&p).unwrap();
        let cut = dir.path().join("cut.png");
        std::fs::write(&cut, &bytes[..64.min(bytes.len())]).unwrap();
        assert_eq!(image_size(&cut).unwrap(), (64, 32));
        assert!(decode_image(&cut, 100, 100).is_err());
    }

    #[test]
    fn missing_or_corrupt_file_returns_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.png");
        assert!(matches!(
            decode_image(&missing, 100, 100),
            Err(RasterError::NotFound)
        ));
        assert!(matches!(image_size(&missing), Err(RasterError::NotFound)));
        let bad = dir.path().join("bad.png");
        std::fs::write(&bad, b"definitely not a png").unwrap();
        assert!(matches!(
            decode_image(&bad, 100, 100),
            Err(RasterError::NotFound)
        ));
        assert!(matches!(image_size(&bad), Err(RasterError::NotFound)));
    }
}
