//! The one dark theme (SPEC §4.2): every colour of the viewer lives here.
//!
//! Terminals apply window transparency only to cells with the default
//! background, so the page background is an explicit RGB colour painted on
//! every cell, and no style in the viewer ever resets it. Span styles set
//! the foreground only (inline code is the one span with its own background).

use ratatui::style::{Color, Modifier, Style};

use crate::doc::Style as InlineStyle;

/// [`BG`] as the RGBA colour pictures are composited over.
pub fn bg_rgba() -> image::Rgba<u8> {
    match BG {
        Color::Rgb(r, g, b) => image::Rgba([r, g, b, 255]),
        _ => image::Rgba([0, 0, 0, 255]),
    }
}

/// Page background, painted on every cell.
pub const BG: Color = Color::Rgb(26, 27, 38);
/// Body text foreground.
pub const FG: Color = Color::Rgb(205, 214, 244);
/// Secondary text: quote bars, HTML, notices (also dimmed by colour alone).
pub const MUTED: Color = Color::Rgb(127, 132, 156);
/// Rules and unfocused borders.
pub const LINE: Color = Color::Rgb(88, 91, 112);
/// Focused border and status bar text.
pub const ACCENT: Color = Color::Rgb(137, 220, 235);
/// Link foreground.
pub const LINK: Color = Color::Rgb(97, 175, 239);
/// Strong text foreground: brighter than `FG`, so bold shows without a bold face.
pub const STRONG: Color = Color::Rgb(255, 255, 255);
/// Emphasis foreground: a warm tint, so italic shows without an italic face.
pub const EMPHASIS: Color = Color::Rgb(249, 226, 175);
/// Background of inline code.
pub const CODE_BG: Color = Color::Rgb(49, 50, 68);

/// Page style: explicit background and body foreground.
pub fn page() -> Style {
    Style::new().fg(FG).bg(BG)
}

/// Base style of body text: foreground only, so it patches over the page.
pub fn body() -> Style {
    Style::new().fg(FG)
}

pub fn heading(level: u8) -> Style {
    let fg = match level {
        1 => Color::Rgb(255, 121, 198),
        2 => Color::Rgb(255, 184, 108),
        3 => Color::Rgb(241, 250, 140),
        4 => Color::Rgb(80, 250, 123),
        5 => Color::Rgb(139, 233, 253),
        _ => Color::Rgb(189, 147, 249),
    };
    Style::new().fg(fg).add_modifier(Modifier::BOLD)
}

/// Muted text: DIM plus a muted colour, so DIM is never the only cue.
pub fn dim() -> Style {
    Style::new().fg(MUTED).add_modifier(Modifier::DIM)
}

pub fn rule() -> Style {
    Style::new().fg(LINE)
}

pub fn link() -> Style {
    Style::new().fg(LINK).add_modifier(Modifier::UNDERLINED)
}

pub fn border(focused: bool) -> Style {
    Style::new().fg(if focused { ACCENT } else { LINE })
}

pub fn status() -> Style {
    Style::new().fg(ACCENT)
}

/// Terminal style of a model span, layered over `base`.
///
/// The strong and emphasis tints replace the body foreground only: over any
/// other base (headings, table headers) the base colour is kept and only the
/// modifiers combine. Strong wins over emphasis when both apply.
pub fn inline(base: Style, style: InlineStyle, linked: bool) -> Style {
    let mut out = base;
    let tint = base.fg == Some(FG);
    if style.emphasis {
        out = out.add_modifier(Modifier::ITALIC);
        if tint {
            out = out.fg(EMPHASIS);
        }
    }
    if style.strong {
        out = out.add_modifier(Modifier::BOLD);
        if tint {
            out = out.fg(STRONG);
        }
    }
    if style.strikethrough {
        out = out.add_modifier(Modifier::CROSSED_OUT);
    }
    if style.code {
        out = out.bg(CODE_BG);
    }
    if linked {
        out = out.patch(link());
    }
    out
}

/// Error frame and message colour.
pub const ERROR: Color = Color::Rgb(255, 110, 108);

/// Code box frame of a failed diagram, on the code background.
pub fn error_border() -> Style {
    Style::new().fg(ERROR).bg(CODE_BG)
}

/// Error message text, on the page background.
pub fn error() -> Style {
    Style::new().fg(ERROR).bg(BG)
}

/// Placeholder text of a diagram being rendered, on the page background.
pub fn placeholder() -> Style {
    Style::new().fg(MUTED).bg(BG)
}

/// Code box text: body foreground on the code background.
pub fn code() -> Style {
    Style::new().fg(FG).bg(CODE_BG)
}

/// Code box border, drawn on the code background.
pub fn code_border() -> Style {
    Style::new().fg(LINE).bg(CODE_BG)
}

/// Language label and truncation ellipsis of a code box.
pub fn code_label() -> Style {
    Style::new().fg(MUTED).bg(CODE_BG)
}

#[cfg(test)]
mod bg_tests {
    use super::*;

    #[test]
    fn bg_rgba_matches_bg() {
        assert_eq!(BG, Color::Rgb(26, 27, 38));
        assert_eq!(bg_rgba(), image::Rgba([26, 27, 38, 255]));
    }
}

#[cfg(test)]
mod error_colour_tests {
    use super::*;

    fn channels(c: Color) -> (f64, f64, f64) {
        let Color::Rgb(r, g, b) = c else {
            panic!("not an RGB colour")
        };
        (f64::from(r), f64::from(g), f64::from(b))
    }

    fn luminance(c: Color) -> f64 {
        let lin = |v: f64| {
            let v = v / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = channels(c);
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn error_colour_is_clearly_red_and_readable_on_both_backgrounds() {
        let (r, g, b) = channels(ERROR);
        assert!(
            r >= 200.0 && g <= 110.0 && b <= 110.0,
            "not red enough: {r} {g} {b}"
        );
        assert!(contrast(ERROR, BG) >= 4.5, "{}", contrast(ERROR, BG));
        assert!(
            contrast(ERROR, CODE_BG) >= 4.5,
            "{}",
            contrast(ERROR, CODE_BG)
        );
    }
}
