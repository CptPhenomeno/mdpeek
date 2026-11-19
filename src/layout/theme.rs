//! Dark-theme style roles (SPEC §4.2); no terminal needed to build them.

use ratatui::style::{Color, Modifier, Style};

use crate::doc::Style as InlineStyle;

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

pub fn dim() -> Style {
    Style::new().fg(Color::DarkGray).add_modifier(Modifier::DIM)
}

pub fn rule() -> Style {
    Style::new().fg(Color::DarkGray)
}

pub fn link() -> Style {
    Style::new()
        .fg(Color::Rgb(97, 175, 239))
        .add_modifier(Modifier::UNDERLINED)
}

/// Terminal style of a model span, layered over `base`.
pub fn inline(base: Style, style: InlineStyle, linked: bool) -> Style {
    let mut out = base;
    if style.emphasis {
        out = out.add_modifier(Modifier::ITALIC);
    }
    if style.strong {
        out = out.add_modifier(Modifier::BOLD);
    }
    if style.strikethrough {
        out = out.add_modifier(Modifier::CROSSED_OUT);
    }
    if style.code {
        out = out.bg(Color::Rgb(60, 63, 72));
    }
    if linked {
        out = out.patch(link());
    }
    out
}
