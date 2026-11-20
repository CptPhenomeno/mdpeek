//! Safe entry point to merman (SPEC §3.4, §4.7).
//!
//! Nothing here returns a merman error or lets a panic escape: every outcome is a
//! [`Rendered`]. The release profile must keep `panic = "unwind"` (tested), otherwise
//! `catch_unwind` cannot contain a panic.
//!
//! Dark theme: applied through merman's site config (`{"theme": "dark"}`), which needs
//! `serde_json` as a normal dependency but leaves the user's source untouched, so a
//! diagram with its own `%%{init}%%` directive still renders (and may override the theme).

use std::panic::{AssertUnwindSafe, catch_unwind};

use merman::ascii::{AsciiRenderOptions, AsciiViewportPolicy, OverflowPolicy};
use merman::svg::SvgPipeline;
use merman::{
    AsciiRequest, MermaidEngine, OperationControl, ParseOptions, RenderOutput, RenderRequest,
    Renderer, SourcePresentationConfig, SvgRequest,
};

/// Outcome of rendering a diagram source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rendered {
    /// SVG document, Mermaid dark theme, transparent background, `<text>` labels.
    Svg(String),
    /// Unicode drawing (flowchart, sequence, class, state only).
    Text(String),
    /// The diagram type has no real text drawing; show the source instead.
    TextUnavailable,
    /// merman's message, or a note that the renderer panicked.
    Error(String),
}

/// Diagram families with a real text drawing. merman's outline dump for the others
/// (gantt, mindmap) is never surfaced.
const TEXT_KEYWORDS: [&str; 8] = [
    "graph",
    "flowchart",
    "flowchart-v2",
    "sequencediagram",
    "classdiagram",
    "classdiagram-v2",
    "statediagram",
    "statediagram-v2",
];

fn renderer() -> Renderer {
    let site = SourcePresentationConfig::from_value(serde_json::json!({ "theme": "dark" }));
    Renderer::new()
        .with_parse_options(ParseOptions::strict())
        .with_presentation_engine(MermaidEngine::new().with_site_config(site))
}

/// Runs `f`, turning a panic into `Rendered::Error`.
pub fn contain(f: impl FnOnce() -> Rendered) -> Rendered {
    catch_unwind(AssertUnwindSafe(f))
        .unwrap_or_else(|_| Rendered::Error("diagram renderer panicked".to_string()))
}

/// Diagram source to SVG.
pub fn render_svg(src: &str) -> Rendered {
    contain(|| {
        let request = SvgRequest {
            pipeline: Some(SvgPipeline::resvg_safe()),
            ..Default::default()
        };
        match renderer().render(RenderRequest::svg(src, OperationControl::new(), request)) {
            Ok(RenderOutput::Svg(Some(svg))) => Rendered::Svg(svg.svg().to_string()),
            Ok(_) => Rendered::Error("No Mermaid diagram type detected".to_string()),
            Err(e) => Rendered::Error(e.to_string()),
        }
    })
}

/// Diagram source to a text drawing, or `TextUnavailable` for types without one.
pub fn render_text(src: &str) -> Rendered {
    contain(|| {
        if let Some(kind) = diagram_keyword(src)
            && !TEXT_KEYWORDS.contains(&kind.as_str())
            && is_known_keyword(&kind)
        {
            return Rendered::TextUnavailable;
        }
        let request = AsciiRequest {
            options: AsciiRenderOptions::unicode(),
            viewport: AsciiViewportPolicy::with_max_width(100).overflow(OverflowPolicy::Fallback),
            ..Default::default()
        };
        match renderer().render(RenderRequest::ascii(src, OperationControl::new(), request)) {
            Ok(RenderOutput::Ascii(Some(report))) => Rendered::Text(report.text),
            Ok(_) => Rendered::Error("No Mermaid diagram type detected".to_string()),
            Err(e) => Rendered::Error(e.to_string()),
        }
    })
}

/// Any first keyword other than the allow-list is "unavailable", except an empty source,
/// which falls through so merman reports its own error.
fn is_known_keyword(kind: &str) -> bool {
    !kind.is_empty()
}

/// First keyword of the diagram, lowercased, skipping blank lines, `%%` comments and
/// init directives (also multi-line ones) and a leading `---` front matter block.
fn diagram_keyword(src: &str) -> Option<String> {
    let mut rest = src.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("---") {
            // Front matter: skip to the closing fence.
            let end = after.find("\n---")?;
            rest = after[end + 4..].trim_start();
        } else if let Some(after) = rest.strip_prefix("%%{") {
            let end = after.find("}%%")?;
            rest = after[end + 3..].trim_start();
        } else if rest.starts_with("%%") {
            rest = rest.split_once('\n').map_or("", |(_, r)| r).trim_start();
        } else {
            break;
        }
    }
    let word: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ';')
        .collect();
    Some(word.to_lowercase())
}

/// Stable hash of a diagram source (FNV-1a 64). Deterministic across runs and
/// releases, so it is safe as a cache key even if a cache is ever persisted.
pub fn source_hash(src: &str) -> u64 {
    src.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
