//! Tests for the merman wrapper (KB-28, SPEC §3.4, §4.7).

use readmd::diagram::{Rendered, contain, render_svg, render_text, source_hash};

const FLOWCHART: &str = "flowchart TD\n    A[Open] --> B{Ok?}\n    B -- yes --> C[Render]\n";
const STATE: &str = "stateDiagram-v2\n    [*] --> Reading\n    Reading --> [*]\n";
const GANTT: &str = "gantt\n    title Plan\n    dateFormat YYYY-MM-DD\n    section Core\n    Model :done, a1, 2026-11-19, 2d\n";
const PIE: &str = "pie title T\n    \"A\" : 15\n    \"B\" : 85\n";
const MINDMAP: &str = "mindmap\n  root((r))\n    a\n    b\n";

#[test]
fn valid_flowchart_returns_svg_with_the_dark_theme_and_a_transparent_background() {
    let Rendered::Svg(svg) = render_svg(FLOWCHART) else {
        panic!("expected svg");
    };
    assert!(svg.trim_start().starts_with("<svg"));
    assert!(!svg.contains("<foreignObject"), "labels must be resvg-safe");
    // Mermaid dark theme palette, not the default one.
    assert!(svg.contains("#1f2020"), "dark theme colours missing");
    assert!(!svg.contains("#22D3EE"), "default theme colours present");
    // Transparent: no full-size background rect directly under the root.
    assert!(!svg.contains("background-color"), "svg paints a background");
}

#[test]
fn an_init_directive_in_the_source_does_not_break_rendering() {
    let src = format!("%%{{init: {{\"theme\":\"forest\"}}}}%%\n{FLOWCHART}");
    assert!(matches!(render_svg(&src), Rendered::Svg(_)));
}

#[test]
fn syntax_error_returns_a_render_error_carrying_mermans_message() {
    let Rendered::Error(msg) = render_svg("flowchart TD\n    A[Start --> B{\n") else {
        panic!("expected error");
    };
    assert!(msg.contains("Unterminated node label"), "got: {msg}");
    assert!(matches!(
        render_text("flowchart TD\n    A[Start --> B{\n"),
        Rendered::Error(_)
    ));
}

#[test]
fn panic_inside_merman_is_caught_and_returned_as_a_render_error() {
    let r = contain(|| panic!("boom"));
    assert!(matches!(r, Rendered::Error(m) if !m.is_empty()));
    assert!(matches!(
        contain(|| Rendered::TextUnavailable),
        Rendered::TextUnavailable
    ));
}

#[test]
fn release_profile_keeps_panic_unwind_so_catch_unwind_works() {
    let toml = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .unwrap_or_default();
    assert!(!toml.contains("panic = \"abort\""));
}

#[test]
fn text_mode_returns_unicode_output_for_a_diagram_type_that_supports_it() {
    for src in [FLOWCHART, STATE] {
        let Rendered::Text(t) = render_text(src) else {
            panic!("expected text for {src}");
        };
        assert!(t.lines().count() > 2);
        assert!(!t.contains("bytes="), "raw outline dump leaked");
    }
}

#[test]
fn text_mode_for_a_type_without_text_output_returns_text_unavailable() {
    for src in [GANTT, PIE, MINDMAP] {
        assert_eq!(render_text(src), Rendered::TextUnavailable, "{src}");
    }
    // Leading comments, init directives and blank lines do not fool detection.
    let src = format!("\n%% note\n%%{{init: {{\"theme\":\"dark\"}}}}%%\n{GANTT}");
    assert_eq!(render_text(&src), Rendered::TextUnavailable);
    let src = format!("\n%% note\n{FLOWCHART}");
    assert!(matches!(render_text(&src), Rendered::Text(_)));
    assert!(matches!(
        render_text(&format!("graph LR\n A-->B\n")),
        Rendered::Text(_)
    ));
}

#[test]
fn source_hash_is_stable_for_identical_sources_and_differs_for_different_ones() {
    assert_eq!(source_hash(FLOWCHART), source_hash(&FLOWCHART.to_string()));
    assert_ne!(source_hash(FLOWCHART), source_hash(STATE));
    // Pinned value: stable across runs and releases (FNV-1a 64).
    assert_eq!(source_hash(""), 0xcbf2_9ce4_8422_2325);
}
