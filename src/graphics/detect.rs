//! Graphics protocol and cell-size selection (SPEC §3.5.1, §4.10).
//!
//! Detection sits behind [`Detector`] so the selection rules are testable
//! without a terminal.

use std::cell::RefCell;

use crate::cli::ProtocolChoice;
use ratatui_image::{
    FontSize,
    picker::{Picker, ProtocolType},
};

/// Cell size in pixels assumed when the terminal does not report one.
pub const DEFAULT_CELL: (u16, u16) = (10, 20);

/// What a terminal query found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub protocol: ProtocolType,
    /// Cell size in pixels, when the terminal reported it.
    pub cell: Option<(u16, u16)>,
}

/// Detection failed or timed out (e.g. tmux without `allow-passthrough`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectError(pub String);

/// Queries the terminal; must run once raw mode and the alternate screen are on.
pub trait Detector {
    fn detect(&self) -> Result<Detection, DetectError>;
    /// Cell size in pixels from the window size, without querying protocols.
    fn cell_size(&self) -> Option<(u16, u16)>;
    /// The picker built by the last successful detection, if the detector keeps one.
    fn take_picker(&self) -> Option<Picker> {
        None
    }
}

/// The protocol and cell size pictures are encoded and laid out with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub protocol: ProtocolType,
    pub cell: (u16, u16),
    /// Protocol and cell size both come from a successful detection.
    pub detected: bool,
}

/// Applies the SPEC §3.5.1 rules to `choice` and the detector's answer.
///
/// A forced protocol skips detection (and so uses [`DEFAULT_CELL`]); `Auto`
/// trusts the detector; a detection error means halfblocks.
pub fn select(choice: ProtocolChoice, detector: &dyn Detector) -> Selection {
    let forced = match choice {
        ProtocolChoice::Auto => None,
        ProtocolChoice::Kitty => Some(ProtocolType::Kitty),
        ProtocolChoice::Sixel => Some(ProtocolType::Sixel),
        ProtocolChoice::Iterm2 => Some(ProtocolType::Iterm2),
        ProtocolChoice::Halfblocks => Some(ProtocolType::Halfblocks),
    };
    if let Some(protocol) = forced {
        return Selection {
            protocol,
            cell: detector.cell_size().unwrap_or(DEFAULT_CELL),
            detected: false,
        };
    }
    match detector.detect() {
        Ok(found) => Selection {
            protocol: found.protocol,
            cell: found.cell.unwrap_or(DEFAULT_CELL),
            detected: true,
        },
        Err(_) => Selection {
            protocol: ProtocolType::Halfblocks,
            cell: detector.cell_size().unwrap_or(DEFAULT_CELL),
            detected: false,
        },
    }
}

impl Selection {
    /// Picker for the sliced renderer, using this protocol and cell size. After a
    /// successful detection it is the detected picker itself, keeping its capabilities.
    pub fn picker(&self, detector: &dyn Detector) -> Picker {
        if self.detected
            && let Some(picker) = detector.take_picker()
        {
            return picker;
        }
        self.fallback_picker()
    }

    fn fallback_picker(&self) -> Picker {
        // No other constructor takes a cell size: it covers forced protocols and the
        // 10x20 fallback, so the Picker always agrees with the layout's cell size.
        #[allow(deprecated)]
        let mut picker = Picker::from_fontsize(FontSize::new(self.cell.0, self.cell.1));
        picker.set_protocol_type(self.protocol);
        picker
    }

    /// Raster pixels per slot pixel (KB-72). Only iTerm2 gets 2: its rows are
    /// sent in cell units (`graphics::iterm2`), so the terminal scales the
    /// denser raster to the cell box. Kitty (placement size follows the pixel
    /// size), Sixel (drawn at pixel size) and halfblocks would show it bigger.
    pub fn density(&self) -> u32 {
        match self.protocol {
            ProtocolType::Iterm2 => 2,
            ProtocolType::Kitty | ProtocolType::Sixel | ProtocolType::Halfblocks => 1,
        }
    }

    /// Human-readable protocol name, for the status bar.
    pub fn label(&self) -> &'static str {
        match self.protocol {
            ProtocolType::Kitty => "Kitty",
            ProtocolType::Sixel => "Sixel",
            ProtocolType::Iterm2 => "iTerm2",
            ProtocolType::Halfblocks => "halfblocks",
        }
    }
}

/// Cell size in pixels from a window size, when the terminal reports pixels.
fn cell_from_window(columns: u16, rows: u16, width: u16, height: u16) -> Option<(u16, u16)> {
    if columns == 0 || rows == 0 || width == 0 || height == 0 {
        return None;
    }
    Some((width / columns, height / rows)).filter(|&(w, h)| w > 0 && h > 0)
}

/// Detector backed by ratatui-image's `Picker::from_query_stdio()`.
#[derive(Default)]
pub struct StdioDetector {
    picker: RefCell<Option<Picker>>,
}

impl Detector for StdioDetector {
    fn cell_size(&self) -> Option<(u16, u16)> {
        let size = crossterm::terminal::window_size().ok()?;
        cell_from_window(size.columns, size.rows, size.width, size.height)
    }

    fn take_picker(&self) -> Option<Picker> {
        self.picker.borrow_mut().take()
    }

    fn detect(&self) -> Result<Detection, DetectError> {
        let picker = Picker::from_query_stdio().map_err(|e| DetectError(e.to_string()))?;
        let font = picker.font_size();
        let (w, h) = (font.width, font.height);
        let found = Detection {
            protocol: picker.protocol_type(),
            cell: (w > 0 && h > 0).then_some((w, h)),
        };
        *self.picker.borrow_mut() = Some(picker);
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn density_is_two_for_iterm2_only() {
        let density = |protocol| {
            Selection {
                protocol,
                cell: (10, 20),
                detected: false,
            }
            .density()
        };
        assert_eq!(density(ProtocolType::Iterm2), 2);
        assert_eq!(density(ProtocolType::Kitty), 1);
        assert_eq!(density(ProtocolType::Sixel), 1);
        assert_eq!(density(ProtocolType::Halfblocks), 1);
    }

    struct Fake {
        result: Result<Detection, DetectError>,
        calls: Cell<u32>,
        window_cell: Option<(u16, u16)>,
    }

    impl Fake {
        fn new(result: Result<Detection, DetectError>) -> Self {
            Self {
                result,
                calls: Cell::new(0),
                window_cell: None,
            }
        }
    }

    impl Detector for Fake {
        fn detect(&self) -> Result<Detection, DetectError> {
            self.calls.set(self.calls.get() + 1);
            self.result.clone()
        }
        fn cell_size(&self) -> Option<(u16, u16)> {
            self.window_cell
        }
    }

    fn found(protocol: ProtocolType, cell: Option<(u16, u16)>) -> Fake {
        Fake::new(Ok(Detection { protocol, cell }))
    }

    #[test]
    fn forced_protocol_skips_detection() {
        let cases = [
            (ProtocolChoice::Kitty, ProtocolType::Kitty),
            (ProtocolChoice::Sixel, ProtocolType::Sixel),
            (ProtocolChoice::Iterm2, ProtocolType::Iterm2),
            (ProtocolChoice::Halfblocks, ProtocolType::Halfblocks),
        ];
        for (choice, expected) in cases {
            let det = found(ProtocolType::Halfblocks, Some((7, 14)));
            let sel = select(choice, &det);
            assert_eq!(sel.protocol, expected);
            assert_eq!(det.calls.get(), 0, "{choice:?} must not query the terminal");
        }
    }

    #[test]
    fn window_size_gives_cell_size() {
        assert_eq!(cell_from_window(100, 50, 900, 950), Some((9, 19)));
        assert_eq!(cell_from_window(100, 50, 0, 0), None);
        assert_eq!(cell_from_window(0, 0, 900, 950), None);
    }

    #[test]
    fn forced_protocol_uses_reported_cell_size() {
        let mut det = found(ProtocolType::Halfblocks, None);
        det.window_cell = Some((9, 19));
        let sel = select(ProtocolChoice::Sixel, &det);
        assert_eq!(sel.cell, (9, 19));
        assert_eq!(det.calls.get(), 0);
        let det = found(ProtocolType::Halfblocks, None);
        assert_eq!(select(ProtocolChoice::Sixel, &det).cell, DEFAULT_CELL);
    }

    #[test]
    fn auto_uses_detected_protocol_and_cell() {
        let det = found(ProtocolType::Iterm2, Some((9, 18)));
        let sel = select(ProtocolChoice::Auto, &det);
        assert_eq!(sel.protocol, ProtocolType::Iterm2);
        assert_eq!(sel.cell, (9, 18));
        let det = found(ProtocolType::Halfblocks, Some((9, 18)));
        assert_eq!(
            select(ProtocolChoice::Auto, &det).protocol,
            ProtocolType::Halfblocks
        );
    }

    #[test]
    fn unknown_cell_size_falls_back_to_10x20() {
        let det = found(ProtocolType::Sixel, None);
        let sel = select(ProtocolChoice::Auto, &det);
        assert_eq!(sel.cell, (10, 20));
        assert_eq!(sel.protocol, ProtocolType::Sixel);
    }

    #[test]
    fn detection_failure_selects_halfblocks() {
        let det = Fake::new(Err(DetectError("timeout".into())));
        let sel = select(ProtocolChoice::Auto, &det);
        assert_eq!(sel.protocol, ProtocolType::Halfblocks);
        assert_eq!(sel.cell, DEFAULT_CELL);
    }
}
