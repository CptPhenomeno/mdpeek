//! Safe entry point to merman (SPEC §3.4, §4.7). Stub: behaviour comes in the next commit.

/// Outcome of rendering a diagram source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rendered {
    Svg(String),
    Text(String),
    TextUnavailable,
    Error(String),
}

/// Source to SVG.
pub fn render_svg(_src: &str) -> Rendered {
    Rendered::Error(String::new())
}

/// Source to text drawing.
pub fn render_text(_src: &str) -> Rendered {
    Rendered::Error(String::new())
}

/// Runs `f`, turning a panic into `Rendered::Error`.
pub fn contain(f: impl FnOnce() -> Rendered) -> Rendered {
    f()
}

/// Stable hash of a diagram source.
pub fn source_hash(_src: &str) -> u64 {
    0
}
