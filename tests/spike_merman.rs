//! Learning tests pinning merman and resvg behaviour (KB-6, SPEC §3.4, §4.7).
//! Findings: docs/spikes/merman-resvg.md.

#![allow(clippy::unwrap_used, clippy::expect_used)] // panics are fine in tests

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

use merman::ascii::{AsciiRenderOptions, AsciiViewportPolicy, OverflowPolicy};
use merman::svg::SvgPipeline;
use merman::{
    AsciiRequest, MermaidEngine, OperationControl, ParseOptions, RenderError, RenderOutput,
    RenderRequest, Renderer, SourcePresentationConfig, SvgRequest,
};

const FLOWCHART: &str = "flowchart TD
    A[Open document] --> B{Has Mermaid?}
    B -- yes --> C[Render with merman]
    B -- no --> D[Plain layout]
    C --> E[Rasterise with resvg]
    E --> F[Draw as picture]
    D --> G[Show text]
    F --> G
";
const SEQUENCE: &str = "sequenceDiagram
    participant U as User
    participant R as mdpeek
    participant M as merman
    U->>R: open SAMPLE.md
    R->>M: render(diagram source)
    M-->>R: SVG
    R-->>U: picture in the document
    Note over R,M: no browser, no Node.js
";
const CLASS: &str = "classDiagram
    class Document {
        +Vec~Block~ blocks
        +bool empty
        +from_path(Path) Document
    }
    class Block
    class Layout {
        +Vec~Row~ rows
    }
    Document \"1\" --> \"*\" Block
    Layout ..> Document : lays out
";
const GANTT: &str = "gantt
    title Plan
    dateFormat  YYYY-MM-DD
    section Core
    Document model   :done,    a1, 2026-11-19, 2d
    Layout           :active,  a2, after a1, 3d
    section Pictures
    Diagrams         :         b1, after a2, 4d
    Images           :         b2, after b1, 3d
";
const MINDMAP: &str = "mindmap
  root((mdpeek))
    Reading
      Markdown
      Code
    Pictures
      Mermaid
      Images
    Navigation
      Sidebar
      Search
";
const STATE: &str = "stateDiagram-v2
    [*] --> Reading
    Reading --> Searching: Ctrl+F
    Searching --> Reading: Esc
    Reading --> FullScreen: Enter on a diagram
    FullScreen --> Reading: Esc
    Reading --> [*]: q
";
const PIE: &str = "pie title Time per phase
    \"Parsing\" : 15
    \"Layout\" : 25
    \"Diagrams\" : 45
    \"Drawing\" : 15
";
const BROKEN: &str = "flowchart TD
    A[Start --> B{
    B ---> ??? C
";

const ALL: [(&str, &str); 7] = [
    ("flowchart", FLOWCHART),
    ("sequence", SEQUENCE),
    ("class", CLASS),
    ("gantt", GANTT),
    ("mindmap", MINDMAP),
    ("state", STATE),
    ("pie", PIE),
];

fn renderer() -> Renderer {
    Renderer::new().with_parse_options(ParseOptions::strict())
}

fn dark_renderer() -> Renderer {
    let site = SourcePresentationConfig::from_value(serde_json::json!({ "theme": "dark" }));
    renderer().with_presentation_engine(MermaidEngine::new().with_site_config(site))
}

fn svg_request() -> SvgRequest {
    SvgRequest {
        pipeline: Some(SvgPipeline::resvg_safe()),
        ..Default::default()
    }
}

fn svg_with(r: &Renderer, src: &str) -> Result<String, RenderError> {
    let out = r.render(RenderRequest::svg(
        src,
        OperationControl::new(),
        svg_request(),
    ))?;
    match out {
        RenderOutput::Svg(Some(svg)) => Ok(svg.svg().to_string()),
        _ => Err(RenderError::NoDiagram),
    }
}

fn text_of(src: &str) -> Result<String, RenderError> {
    let out = renderer().render(RenderRequest::ascii(
        src,
        OperationControl::new(),
        AsciiRequest {
            options: AsciiRenderOptions::unicode(),
            viewport: AsciiViewportPolicy::with_max_width(100).overflow(OverflowPolicy::Fallback),
            ..Default::default()
        },
    ))?;
    match out {
        RenderOutput::Ascii(Some(report)) => Ok(report.text),
        _ => Err(RenderError::NoDiagram),
    }
}

/// Rasterise at `width` px (natural size scaled to fit); returns (w, h, rgba, intrinsic).
fn rasterise(svg: &str, width: u32) -> (u32, u32, Vec<u8>, (f32, f32)) {
    let opt = resvg::usvg::Options {
        fontdb: mdpeek::raster::fontdb(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(svg, &opt).expect("parse svg");
    let size = tree.size();
    let scale = width as f32 / size.width();
    let h = (size.height() * scale).ceil() as u32;
    let mut pm = resvg::tiny_skia::Pixmap::new(width, h).expect("pixmap");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pm.as_mut(),
    );
    (width, h, pm.take(), (size.width(), size.height()))
}

#[test]
fn flowchart_source_produces_an_svg_document() {
    for (name, src) in ALL {
        let svg = svg_with(&renderer(), src).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(svg.trim_start().starts_with("<svg"), "{name}: not svg");
        eprintln!("{name}: svg {} bytes", svg.len());
    }
}

#[test]
fn dark_theme_option_changes_the_svg_colours_compared_with_the_default_theme() {
    let default = svg_with(&renderer(), FLOWCHART).unwrap();
    let dark = svg_with(&dark_renderer(), FLOWCHART).unwrap();
    assert_ne!(default, dark);
    // Same result via an in-source init directive.
    let init = format!("%%{{init: {{\"theme\":\"dark\"}}}}%%\n{FLOWCHART}");
    let dark_init = svg_with(&renderer(), &init).unwrap();
    assert_ne!(default, dark_init);
    eprintln!("default fill colours: {:?}", colours(&default));
    eprintln!("dark fill colours: {:?}", colours(&dark));
}

fn colours(svg: &str) -> Vec<String> {
    let mut v: Vec<String> = svg
        .match_indices('#')
        .filter_map(|(i, _)| svg.get(i..i + 7))
        .filter(|c| c[1..].chars().all(|ch| ch.is_ascii_hexdigit()))
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v.truncate(12);
    v
}

#[test]
fn svg_background_is_transparent() {
    let svg = svg_with(&dark_renderer(), FLOWCHART).unwrap();
    let head: String = svg.chars().take(400).collect();
    eprintln!("svg head: {head}");
    let (w, h, px, _) = rasterise(&svg, 400);
    let corner = &px[0..4];
    eprintln!("top-left pixel {corner:?} of {w}x{h}");
    assert_eq!(corner[3], 0, "corner pixel must be fully transparent");
}

#[test]
fn invalid_diagram_source_returns_an_error_with_a_human_readable_message() {
    let err = svg_with(&renderer(), BROKEN).expect_err("broken diagram must fail");
    let msg = err.to_string();
    eprintln!("error variant: {err:?}\nmessage: {msg}");
    assert!(!msg.trim().is_empty());
    let terr = text_of(BROKEN).expect_err("broken diagram must fail in text mode");
    eprintln!("text-mode error: {terr}");
}

#[test]
fn text_output_support_is_probed_for_each_of_the_five_showcase_diagram_types() {
    eprintln!(
        "ascii_supported_diagram_types: {:?}",
        merman::ascii::ascii_supported_diagram_types()
    );
    for (name, src) in ALL {
        match text_of(src) {
            Ok(t) => eprintln!("TEXT {name}: ok ({} lines)\n{t}", t.lines().count()),
            Err(e) => eprintln!("TEXT {name}: error: {e}"),
        }
    }
    eprintln!(
        "ascii_diagrammatic_diagram_types: {:?}",
        merman::ascii::ascii_diagrammatic_diagram_types()
    );
    // Facts asserted after observation; see findings.
    assert!(text_of(PIE).is_err(), "pie has no text output");
    assert!(text_of(FLOWCHART).is_ok());
}

#[test]
fn a_panic_during_rendering_is_caught_by_catch_unwind_and_reported_as_an_error() {
    // Injected panic: proves the containment pattern.
    let r: Result<String, String> = catch_unwind(AssertUnwindSafe(|| -> Result<String, String> {
        panic!("boom")
    }))
    .unwrap_or_else(|_| Err("diagram renderer panicked".into()));
    assert!(r.is_err());

    // Hostile inputs through the real renderer: none may escape as a panic.
    let inputs = [
        "",
        "flowchart",
        "flowchart TD\n A-->",
        "sequenceDiagram\n A->>",
        "classDiagram\n class",
        "gantt\n dateFormat YYYY\n a :x, 1",
        "mindmap\n",
        "pie\n \"a\" : -1",
        "stateDiagram-v2\n [*] -->",
        "flowchart TD\n A[\u{0}] --> B[\u{202e}é]",
    ];
    let mut panics = 0;
    for src in inputs {
        let res = catch_unwind(AssertUnwindSafe(|| svg_with(&renderer(), src)));
        match res {
            Ok(r) => eprintln!(
                "{:?} -> {}",
                src.chars().take(30).collect::<String>(),
                r.map(|s| format!("svg {}", s.len()))
                    .unwrap_or_else(|e| format!("err {e}"))
            ),
            Err(_) => {
                panics += 1;
                eprintln!("PANIC on {src:?}");
            }
        }
    }
    eprintln!("panics observed: {panics}");
}

#[test]
fn resvg_rasterises_the_flowchart_svg_to_a_non_empty_rgba_image_of_the_requested_width() {
    let svg = svg_with(&dark_renderer(), FLOWCHART).unwrap();
    let t = Instant::now();
    let (w, h, px, (iw, ih)) = rasterise(&svg, 1000);
    eprintln!(
        "raster 1000px: {w}x{h} in {:?}; intrinsic {iw}x{ih}",
        t.elapsed()
    );
    assert_eq!(w, 1000);
    assert_eq!(px.len(), (w * h * 4) as usize);
    assert!(px.chunks(4).any(|p| p[3] != 0));
    for (name, src) in ALL {
        let svg = svg_with(&dark_renderer(), src).unwrap();
        let t = Instant::now();
        let (w, h, _, (iw, ih)) = rasterise(&svg, 1000);
        eprintln!(
            "{name}: intrinsic {iw}x{ih}, raster {w}x{h} in {:?}",
            t.elapsed()
        );
    }
}

#[test]
fn rasterised_flowchart_contains_label_pixels() {
    // Labels must be drawn: compare the SVG rasterised with and without its text.
    for (name, src) in ALL {
        let svg = svg_with(&dark_renderer(), src).unwrap();
        let fo = svg.matches("<foreignObject").count();
        let texts = svg.matches("<text").count();
        let stripped = strip_text(&svg);
        let (_, _, with, _) = rasterise(&svg, 800);
        let (_, _, without, _) = rasterise(&stripped, 800);
        let diff = with.iter().zip(&without).filter(|(a, b)| a != b).count();
        eprintln!("{name}: foreignObject={fo} <text>={texts} differing bytes={diff}");
        assert!(diff > 0, "{name}: labels lost");
    }
}

#[test]
fn default_pipeline_keeps_labels_in_foreign_object_which_resvg_ignores() {
    // Without `SvgPipeline::resvg_safe()` merman emits HTML labels.
    let out = dark_renderer()
        .render(RenderRequest::svg(
            FLOWCHART,
            OperationControl::new(),
            SvgRequest::default(),
        ))
        .unwrap();
    let RenderOutput::Svg(Some(svg)) = out else {
        panic!("no svg");
    };
    let svg = svg.svg().to_string();
    let fo = svg.matches("<foreignObject").count();
    eprintln!(
        "default pipeline: foreignObject={fo}, <text>={}",
        svg.matches("<text").count()
    );
    let safe = svg_with(&dark_renderer(), FLOWCHART).unwrap();
    let (_, _, a, _) = rasterise(&svg, 800);
    let (_, _, b, _) = rasterise(&safe, 800);
    eprintln!("pixels differ between default and resvg_safe: {}", a != b);
}

fn strip_text(svg: &str) -> String {
    let mut out = String::new();
    let mut rest = svg;
    while let Some(i) = rest.find("<text") {
        out.push_str(&rest[..i]);
        match rest[i..].find("</text>") {
            Some(j) => rest = &rest[i + j + 7..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}
