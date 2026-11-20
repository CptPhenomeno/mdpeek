//! Graphics protocol and cell-size selection (SPEC §3.5.1, §4.10).
//!
//! Detection sits behind [`Detector`] so the selection rules are testable
//! without a terminal.

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
}

/// The protocol and cell size pictures are encoded and laid out with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub protocol: ProtocolType,
    pub cell: (u16, u16),
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
            cell: DEFAULT_CELL,
        };
    }
    match detector.detect() {
        Ok(found) => Selection {
            protocol: found.protocol,
            cell: found.cell.unwrap_or(DEFAULT_CELL),
        },
        Err(_) => Selection {
            protocol: ProtocolType::Halfblocks,
            cell: DEFAULT_CELL,
        },
    }
}

impl Selection {
    /// Picker for the sliced renderer, using this protocol and cell size.
    pub fn picker(&self) -> Picker {
        // No other constructor takes a cell size: it covers forced protocols and the
        // 10x20 fallback, so the Picker always agrees with the layout's cell size.
        #[allow(deprecated)]
        let mut picker = Picker::from_fontsize(FontSize::new(self.cell.0, self.cell.1));
        picker.set_protocol_type(self.protocol);
        picker
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

/// Detector backed by ratatui-image's `Picker::from_query_stdio()`.
pub struct StdioDetector;

impl Detector for StdioDetector {
    fn cell_size(&self) -> Option<(u16, u16)> {
        None
    }

    fn detect(&self) -> Result<Detection, DetectError> {
        let picker = Picker::from_query_stdio().map_err(|e| DetectError(e.to_string()))?;
        let font = picker.font_size();
        let (w, h) = (font.width, font.height);
        Ok(Detection {
            protocol: picker.protocol_type(),
            cell: (w > 0 && h > 0).then_some((w, h)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

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
