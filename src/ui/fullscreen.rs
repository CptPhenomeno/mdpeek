//! Zoom and pan state of the full-screen diagram view (SPEC §3.6.3).
//!
//! Pure logic, no terminal. Coordinates are the picture's natural pixels
//! (SVG user units); the screen is measured in device pixels.

/// Maximum zoom relative to fit.
pub const MAX_ZOOM: f64 = 8.0;
/// Multiplier of one zoom step.
pub const ZOOM_STEP: f64 = 1.25;
/// Fraction of the visible area moved by one pan.
pub const PAN_FRACTION: f64 = 0.10;

/// Tolerance for float comparisons.
const SNAP: f64 = 1e-9;

/// A pan direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanDir {
    Left,
    Right,
    Up,
    Down,
}

/// A rectangle in the picture's natural pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Zoom and pan state for one picture on one screen area.
#[derive(Clone, Debug)]
pub struct ZoomState {
    natural: (f64, f64),
    screen: (f64, f64),
    zoom: f64,
    center: (f64, f64),
}

impl ZoomState {
    /// New state at fit, centred. `natural` and `screen` are in pixels.
    pub fn new(natural: (f64, f64), screen: (f64, f64)) -> Self {
        Self {
            natural,
            screen,
            zoom: 1.0,
            center: (natural.0 / 2.0, natural.1 / 2.0),
        }
    }

    /// The screen area changed: keep the zoom and the centre, re-clamp.
    pub fn resize(&mut self, screen: (f64, f64)) {
        self.screen = screen;
        self.clamp();
    }

    pub fn zoom_in(&mut self) {
        let z = self.zoom * ZOOM_STEP;
        // Snap so repeated float products never leave a hair below the cap.
        self.zoom = if z > MAX_ZOOM - SNAP { MAX_ZOOM } else { z };
        self.clamp();
    }

    pub fn zoom_out(&mut self) {
        let z = self.zoom / ZOOM_STEP;
        self.zoom = if z < 1.0 + SNAP { 1.0 } else { z };
        self.clamp();
    }

    /// Back to fit, centred.
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.center = (self.natural.0 / 2.0, self.natural.1 / 2.0);
        self.clamp();
    }

    /// Pan by 10% of the visible area, only along an axis the picture
    /// exceeds the screen on.
    pub fn pan(&mut self, dir: PanDir) {
        let vis = self.visible();
        match dir {
            PanDir::Left | PanDir::Right if self.can_pan_x() => {
                let d = vis.0 * PAN_FRACTION;
                self.center.0 += if dir == PanDir::Left { -d } else { d };
            }
            PanDir::Up | PanDir::Down if self.can_pan_y() => {
                let d = vis.1 * PAN_FRACTION;
                self.center.1 += if dir == PanDir::Up { -d } else { d };
            }
            _ => {}
        }
        self.clamp();
    }

    /// Scale from natural size to screen pixels at the current zoom.
    pub fn scale(&self) -> f64 {
        self.fit_scale() * self.zoom
    }

    /// Scale at 100%: fit to screen. A picture smaller than the screen is
    /// enlarged (SPEC §3.6.3); it is re-rasterised from the SVG, not upscaled.
    pub fn fit_scale(&self) -> f64 {
        let (w, h) = self.natural;
        if w <= 0.0 || h <= 0.0 {
            return 1.0;
        }
        (self.screen.0 / w).min(self.screen.1 / h)
    }

    /// Zoom percentage relative to fit, for the status bar.
    pub fn percent(&self) -> u32 {
        (self.zoom * 100.0).round() as u32
    }

    /// Region of the picture to rasterise, in natural pixels.
    pub fn source_rect(&self) -> SourceRect {
        let (vw, vh) = self.visible();
        SourceRect {
            x: self.center.0 - vw / 2.0,
            y: self.center.1 - vh / 2.0,
            w: vw,
            h: vh,
        }
    }

    /// Size in screen pixels of the rasterised region (it is centred on
    /// the screen when smaller than it).
    pub fn dest_size(&self) -> (f64, f64) {
        let (vw, vh) = self.visible();
        (vw * self.scale(), vh * self.scale())
    }

    /// Viewport centre in natural pixels.
    pub fn center(&self) -> (f64, f64) {
        self.center
    }

    /// Whether the picture exceeds the screen on the horizontal axis.
    pub fn can_pan_x(&self) -> bool {
        self.natural.0 * self.scale() > self.screen.0 + SNAP
    }

    /// Whether the picture exceeds the screen on the vertical axis.
    pub fn can_pan_y(&self) -> bool {
        self.natural.1 * self.scale() > self.screen.1 + SNAP
    }

    /// Visible extent in natural pixels, capped at the picture size.
    fn visible(&self) -> (f64, f64) {
        let s = self.scale();
        if s <= 0.0 {
            return self.natural;
        }
        (
            (self.screen.0 / s).min(self.natural.0),
            (self.screen.1 / s).min(self.natural.1),
        )
    }

    /// Keep the visible region inside the picture; centre an axis that
    /// is fully visible.
    fn clamp(&mut self) {
        let (vw, vh) = self.visible();
        self.center.0 = clamp_axis(self.center.0, vw, self.natural.0);
        self.center.1 = clamp_axis(self.center.1, vh, self.natural.1);
    }
}

fn clamp_axis(center: f64, visible: f64, total: f64) -> f64 {
    if visible >= total {
        total / 2.0
    } else {
        center.clamp(visible / 2.0, total - visible / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    /// 1000x500 picture on a 500x500 screen: fit scale 0.5.
    fn wide() -> ZoomState {
        ZoomState::new((1000.0, 500.0), (500.0, 500.0))
    }

    fn zoomed(n: usize) -> ZoomState {
        let mut s = wide();
        for _ in 0..n {
            s.zoom_in();
        }
        s
    }

    #[test]
    fn zoom_in_steps_by_1_25_up_to_8_times_fit() {
        let mut s = wide();
        assert!((s.fit_scale() - 0.5).abs() < EPS);
        s.zoom_in();
        assert!((s.scale() - 0.5 * 1.25).abs() < EPS);
        assert_eq!(s.percent(), 125);
        for _ in 0..30 {
            s.zoom_in();
        }
        assert!((s.scale() - 4.0).abs() < EPS);
        assert_eq!(s.percent(), 800);
    }

    #[test]
    fn zoom_out_steps_down_to_fit() {
        let mut s = zoomed(3);
        s.zoom_out();
        assert!((s.scale() - 0.5 * 1.25 * 1.25).abs() < EPS);
        for _ in 0..10 {
            s.zoom_out();
        }
        assert!((s.scale() - 0.5).abs() < EPS);
        assert_eq!(s.percent(), 100);
    }

    #[test]
    fn reset_returns_to_fit_and_centre() {
        let mut s = zoomed(5);
        s.pan(PanDir::Right);
        s.reset();
        assert_eq!(s.percent(), 100);
        assert_eq!(s.center(), (500.0, 250.0));
    }

    #[test]
    fn arrows_pan_by_ten_percent_of_visible_area() {
        let mut s = zoomed(10); // 800%: scale 4, visible 125x125 natural
        let r = s.source_rect();
        assert!((r.w - 125.0).abs() < EPS && (r.h - 125.0).abs() < EPS);
        let (cx, cy) = s.center();
        s.pan(PanDir::Right);
        assert!((s.center().0 - (cx + 12.5)).abs() < EPS);
        s.pan(PanDir::Down);
        assert!((s.center().1 - (cy + 12.5)).abs() < EPS);
        s.pan(PanDir::Left);
        s.pan(PanDir::Left);
        assert!((s.center().0 - (cx - 12.5)).abs() < EPS);
        s.pan(PanDir::Up);
        s.pan(PanDir::Up);
        assert!((s.center().1 - (cy - 12.5)).abs() < EPS);
    }

    #[test]
    fn arrows_do_nothing_while_picture_fits() {
        let mut s = wide();
        let before = s.center();
        for d in [PanDir::Left, PanDir::Right, PanDir::Up, PanDir::Down] {
            s.pan(d);
        }
        assert_eq!(s.center(), before);
        assert_eq!(before, (500.0, 250.0));
    }

    #[test]
    fn pan_only_on_axes_that_exceed_the_screen() {
        // 1000x500 at 125%: scaled 625x312 on 500x500: only x exceeds.
        let mut s = zoomed(1);
        assert!(s.can_pan_x() && !s.can_pan_y());
        let cy = s.center().1;
        s.pan(PanDir::Down);
        assert_eq!(s.center().1, cy);
        let cx = s.center().0;
        s.pan(PanDir::Right);
        assert!(s.center().0 > cx);
    }

    #[test]
    fn pan_offset_is_clamped_to_picture_edges() {
        let mut s = zoomed(10);
        for _ in 0..200 {
            s.pan(PanDir::Left);
            s.pan(PanDir::Up);
        }
        let r = s.source_rect();
        assert!(r.x.abs() < EPS && r.y.abs() < EPS);
        for _ in 0..400 {
            s.pan(PanDir::Right);
            s.pan(PanDir::Down);
        }
        let r = s.source_rect();
        assert!((r.x + r.w - 1000.0).abs() < EPS);
        assert!((r.y + r.h - 500.0).abs() < EPS);
    }

    #[test]
    fn zoom_keeps_the_viewport_centre() {
        let mut s = zoomed(10);
        for _ in 0..20 {
            s.pan(PanDir::Left);
        }
        let c = s.center();
        s.zoom_out();
        assert_eq!(s.center(), c);
        let r = s.source_rect();
        assert!((r.x + r.w / 2.0 - c.0).abs() < EPS);
    }

    #[test]
    fn zoom_out_reclamps_centre_near_an_edge() {
        let mut s = zoomed(10);
        for _ in 0..400 {
            s.pan(PanDir::Right);
        }
        s.zoom_out();
        let r = s.source_rect();
        assert!(r.x + r.w <= 1000.0 + EPS);
        s.reset();
        assert_eq!(s.center(), (500.0, 250.0));
    }

    #[test]
    fn percent_is_relative_to_fit_not_natural_size() {
        // Same zoom, different fit scales: same percentage.
        let mut a = ZoomState::new((1000.0, 500.0), (500.0, 500.0));
        let mut b = ZoomState::new((4000.0, 2000.0), (500.0, 500.0));
        for _ in 0..4 {
            a.zoom_in();
            b.zoom_in();
        }
        assert_eq!(a.percent(), b.percent());
        assert_eq!(a.percent(), 244); // 1.25^4 = 2.4414
        assert_eq!(ZoomState::new((10.0, 10.0), (500.0, 500.0)).percent(), 100);
    }

    #[test]
    fn small_picture_is_enlarged_to_fit_the_screen() {
        let mut s = ZoomState::new((100.0, 50.0), (500.0, 500.0));
        assert!((s.fit_scale() - 5.0).abs() < EPS);
        assert_eq!(s.percent(), 100);
        assert_eq!(s.dest_size(), (500.0, 250.0));
        let r = s.source_rect();
        assert_eq!((r.x, r.y, r.w, r.h), (0.0, 0.0, 100.0, 50.0));
        for d in [PanDir::Left, PanDir::Right, PanDir::Up, PanDir::Down] {
            s.pan(d);
        }
        assert_eq!(s.center(), (50.0, 25.0));
        // Zoom goes up to x8 of fit, and the raster stays bounded by the screen.
        for _ in 0..30 {
            s.zoom_in();
        }
        assert!((s.scale() - 40.0).abs() < EPS);
        let (dw, dh) = s.dest_size();
        assert!(dw <= 500.0 + EPS && dh <= 500.0 + EPS);
    }

    #[test]
    fn dest_size_is_screen_sized_when_zoomed_beyond_it() {
        assert_eq!(wide().dest_size(), (500.0, 250.0));
        assert_eq!(zoomed(10).dest_size(), (500.0, 500.0));
    }

    #[test]
    fn max_zoom_is_exact_despite_float_rounding() {
        let mut s = wide();
        for _ in 0..50 {
            s.zoom_in();
        }
        assert_eq!(s.percent(), 800);
        assert_eq!(s.scale(), 0.5 * MAX_ZOOM);
        for _ in 0..50 {
            s.zoom_out();
        }
        assert_eq!(s.percent(), 100);
        assert_eq!(s.scale(), 0.5);
    }

    #[test]
    fn resize_keeps_zoom_and_reclamps() {
        let mut s = zoomed(10);
        for _ in 0..400 {
            s.pan(PanDir::Right);
        }
        s.resize((250.0, 250.0)); // fit scale becomes 0.25
        assert_eq!(s.percent(), 800);
        assert!((s.scale() - 2.0).abs() < EPS);
        let r = s.source_rect();
        assert!((r.w - 125.0).abs() < EPS);
        assert!(r.x + r.w <= 1000.0 + EPS);
        // A bigger screen: the fit scale grows, zoom is kept, and at 100%
        // the whole picture is shown, centred.
        let mut s = zoomed(1);
        s.resize((2000.0, 2000.0)); // fit scale becomes 2
        assert_eq!(s.percent(), 125);
        assert_eq!(s.source_rect().w, 800.0);
        s.reset();
        assert_eq!(s.center(), (500.0, 250.0));
        assert_eq!(s.source_rect().w, 1000.0);
    }
}
