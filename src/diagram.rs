//! Safe entry point to merman (SPEC §3.4, §4.7).
//!
//! Nothing here returns a merman error or lets a panic escape: every outcome is a
//! [`Rendered`]. The release profile must keep `panic = "unwind"` (tested), otherwise
//! `catch_unwind` cannot contain a panic.
//!
//! Theme: applied through merman's site config (`base` theme plus theme variables from `theme.rs`), which needs
//! `serde_json` as a normal dependency but leaves the user's source untouched, so a
//! diagram with its own `%%{init}%%` directive still renders (and may override the theme).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use merman::ascii::{AsciiRenderOptions, AsciiViewportPolicy, OverflowPolicy};
use merman::svg::export::{NativeFontContext, NativeFontLimits};
use merman::svg::{SvgEnvironment, SvgPipeline};
use merman::{
    AsciiRequest, MermaidEngine, OperationControl, ParseOptions, RenderOutput, RenderRequest,
    Renderer, SourcePresentationConfig, SvgRequest,
};

/// Outcome of rendering a diagram source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rendered {
    /// SVG document, Mermaid base theme with the readmd palette, transparent background, `<text>` labels.
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

/// Font that merman measures labels with and resvg draws them with. It is bundled
/// (Liberation Sans, SIL OFL, `assets/fonts`), so the label boxes fit on any machine:
/// with system fonts the drawn text could be wider than the measured one and the
/// label viewports (`overflow:hidden`) would cut both ends of it.
pub const FONT_FAMILY: &str = "Liberation Sans";

/// Faces of [`FONT_FAMILY`]: regular and bold.
pub const FONT_FACES: [&[u8]; 2] = [
    include_bytes!("../assets/fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../assets/fonts/LiberationSans-Bold.ttf"),
];

/// merman's measuring context over the bundled faces; `None` if it cannot be built
/// (measurement then falls back to merman's built-in tables).
fn font_context() -> Option<&'static NativeFontContext> {
    static CONTEXT: OnceLock<Option<NativeFontContext>> = OnceLock::new();
    CONTEXT
        .get_or_init(|| {
            NativeFontContext::from_font_bytes(
                FONT_FACES.iter().map(|f| f.to_vec()),
                NativeFontLimits::default(),
                &OperationControl::new(),
            )
            .ok()
        })
        .as_ref()
}

fn environment() -> SvgEnvironment {
    let base = SvgEnvironment::deterministic();
    match font_context() {
        Some(ctx) => base
            .clone()
            .with_native_font_context(ctx.clone())
            .unwrap_or(base),
        None => base,
    }
}

/// The merman site config: Mermaid's `base` theme with variables from `theme.rs`.
pub fn site_config() -> serde_json::Value {
    use crate::theme::{
        BG, CODE_BG, DIAGRAM_ON_FILL, ERROR, FG, LINE, deepen, diagram_line, diagram_scale, hex,
        pastel, primary_fill,
    };
    let scale = diagram_scale();
    let on_fill = hex(DIAGRAM_ON_FILL);
    let primary = hex(primary_fill());
    let border = hex(deepen(primary_fill()));
    let line = hex(diagram_line());
    let soft_error = hex(scale[9]);
    let fixed: Vec<(&str, String)> = vec![
        ("fontFamily", FONT_FAMILY.to_string()),
        ("background", hex(BG)),
        ("primaryColor", primary.clone()),
        ("primaryTextColor", on_fill.clone()),
        ("primaryBorderColor", border.clone()),
        ("secondaryColor", hex(scale[5])),
        ("secondaryTextColor", on_fill.clone()),
        ("secondaryBorderColor", border.clone()),
        ("tertiaryColor", hex(scale[1])),
        ("tertiaryTextColor", on_fill.clone()),
        ("tertiaryBorderColor", border.clone()),
        ("lineColor", line.clone()),
        ("textColor", hex(FG)),
        ("titleColor", hex(FG)),
        ("mainBkg", primary.clone()),
        ("nodeBkg", primary.clone()),
        ("nodeBorder", border.clone()),
        ("nodeTextColor", on_fill.clone()),
        ("clusterBkg", hex(CODE_BG)),
        ("clusterBorder", hex(LINE)),
        ("defaultLinkColor", line.clone()),
        ("edgeLabelBackground", hex(scale[8])),
        ("labelBackground", hex(scale[8])),
        ("labelTextColor", on_fill.clone()),
        ("noteBkgColor", hex(scale[8])),
        ("noteTextColor", on_fill.clone()),
        ("noteBorderColor", hex(scale[8])),
        ("actorBkg", primary.clone()),
        ("actorBorder", border.clone()),
        ("actorTextColor", on_fill.clone()),
        ("actorLineColor", hex(LINE)),
        ("signalColor", hex(FG)),
        ("signalTextColor", hex(FG)),
        ("labelBoxBkgColor", hex(CODE_BG)),
        ("labelBoxBorderColor", border.clone()),
        ("loopTextColor", hex(FG)),
        ("activationBkgColor", hex(scale[1])),
        ("activationBorderColor", border.clone()),
        ("sequenceNumberColor", on_fill.clone()),
        ("cScaleLabel0", on_fill.clone()),
        ("pieOpacity", "1".to_string()),
        ("pieTitleTextColor", hex(FG)),
        ("pieSectionTextColor", on_fill.clone()),
        ("pieLegendTextColor", hex(FG)),
        ("pieStrokeColor", hex(BG)),
        ("pieOuterStrokeColor", hex(BG)),
        ("classText", on_fill.clone()),
        ("stateBkg", primary.clone()),
        ("stateLabelColor", on_fill.clone()),
        ("altBackground", hex(CODE_BG)),
        ("transitionColor", line.clone()),
        ("transitionLabelColor", on_fill.clone()),
        ("compositeBackground", hex(CODE_BG)),
        ("compositeTitleBackground", hex(CODE_BG)),
        ("compositeBorder", hex(LINE)),
        ("specialStateColor", border.clone()),
        ("taskBkgColor", primary.clone()),
        ("taskBorderColor", border.clone()),
        ("taskTextColor", on_fill.clone()),
        ("taskTextLightColor", on_fill.clone()),
        ("taskTextDarkColor", on_fill.clone()),
        ("taskTextOutsideColor", hex(FG)),
        ("activeTaskBkgColor", hex(scale[3])),
        ("activeTaskBorderColor", border.clone()),
        ("doneTaskBkgColor", hex(scale[7])),
        ("doneTaskBorderColor", border.clone()),
        ("critBkgColor", soft_error.clone()),
        ("critBorderColor", hex(deepen(scale[9]))),
        ("gridColor", hex(LINE)),
        ("sectionBkgColor", hex(CODE_BG)),
        ("altSectionBkgColor", hex(BG)),
        ("sectionBkgColor2", hex(CODE_BG)),
        ("todayLineColor", hex(pastel(ERROR))),
        ("excludeBkgColor", hex(CODE_BG)),
    ];
    let mut vars: serde_json::Map<String, serde_json::Value> = fixed
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.into()))
        .collect();
    for (i, c) in scale.iter().enumerate() {
        vars.insert(format!("cScale{i}"), hex(*c).into());
        vars.insert(format!("cScaleLabel{i}"), on_fill.clone().into());
        vars.insert(format!("cScaleInv{i}"), hex(*c).into());
        vars.insert(format!("git{i}"), hex(*c).into());
        vars.insert(format!("pie{}", i + 1), hex(*c).into());
    }
    serde_json::json!({
        "theme": "base",
        "fontFamily": FONT_FAMILY,
        "themeVariables": vars,
    })
}

fn renderer() -> Renderer {
    let site = SourcePresentationConfig::from_value(site_config());
    Renderer::new()
        .with_parse_options(ParseOptions::strict())
        .with_presentation_engine(MermaidEngine::new().with_site_config(site))
}

/// Mermaid fades the edge-label rectangle to half opacity in its stylesheet, whatever the
/// theme variables say; over the page background that reads as a grey smear. The label
/// background (`edgeLabelBackground`) is made fully opaque.
fn opaque_edge_labels(svg: &str) -> String {
    let svg = svg.replace(".edgeLabel rect{opacity:0.5;", ".edgeLabel rect{opacity:1;");
    let svg = svg.replace(
        "<line stroke=\"currentColor\"",
        &format!("<line stroke=\"{}\"", crate::theme::hex(crate::theme::LINE)),
    );
    light_terminal_labels(&svg)
}

/// Class cardinality labels (`edgeTerminals`) are drawn straight on the page with the
/// same dark text colour as node labels, and have no background. They get the body
/// foreground instead.
fn light_terminal_labels(svg: &str) -> String {
    use crate::theme::{BG, FG, hex};
    let (dark, light) = (hex(BG), hex(FG));
    let mut out = String::with_capacity(svg.len());
    for (i, part) in svg.split("<text ").enumerate() {
        if i == 0 {
            out.push_str(part);
            continue;
        }
        let end = part.find("</text>").unwrap_or(part.len());
        let (body, rest) = part.split_at(end);
        out.push_str("<text ");
        let tag_end = body.find('>').unwrap_or(body.len());
        if body[..tag_end].contains("edgeTerminals") {
            out.push_str(&body.replace(&dark, &light));
        } else {
            out.push_str(body);
        }
        out.push_str(rest);
    }
    out
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
            environment: environment(),
            ..Default::default()
        };
        match renderer().render(RenderRequest::svg(src, OperationControl::new(), request)) {
            Ok(RenderOutput::Svg(Some(svg))) => Rendered::Svg(opaque_edge_labels(svg.svg())),
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
