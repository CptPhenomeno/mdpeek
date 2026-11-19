//! The one dark theme (SPEC §4.2): every colour of the viewer lives here.

use ratatui::style::Color;

/// Page background, painted on every cell.
pub const BG: Color = Color::Rgb(0, 0, 0);
/// Body text foreground.
pub const FG: Color = Color::Rgb(0, 0, 0);
