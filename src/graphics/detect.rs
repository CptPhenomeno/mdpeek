//! Graphics protocol and cell-size selection (SPEC §3.5.1, §4.10).
//!
//! Detection sits behind [`Detector`] so the selection rules are testable
//! without a terminal.

use crate::cli::ProtocolChoice;
use ratatui_image::picker::ProtocolType;

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
}

/// The protocol and cell size pictures are encoded and laid out with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub protocol: ProtocolType,
    pub cell: (u16, u16),
}

/// Applies the SPEC §3.5.1 rules to `choice` and the detector's answer.
pub fn select(choice: ProtocolChoice, detector: &dyn Detector) -> Selection {
    let _ = (choice, detector);
    Selection {
        protocol: ProtocolType::Kitty,
        cell: (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Fake {
        result: Result<Detection, DetectError>,
        calls: Cell<u32>,
    }

    impl Fake {
        fn new(result: Result<Detection, DetectError>) -> Self {
            Self {
                result,
                calls: Cell::new(0),
            }
        }
    }

    impl Detector for Fake {
        fn detect(&self) -> Result<Detection, DetectError> {
            self.calls.set(self.calls.get() + 1);
            self.result.clone()
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
