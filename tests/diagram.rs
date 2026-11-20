//! Tests for the merman wrapper (KB-28, SPEC §3.4, §4.7).

use readmd::diagram::{Rendered, contain, render_svg, render_text, source_hash};

const FLOWCHART: &str = "flowchart TD\n    A[Open] --> B{Ok?}\n    B -- yes --> C[Render]\n";
const STATE: &str = "stateDiagram-v2\n    [*] --> Reading\n    Reading --> [*]\n";
const GANTT: &str = "gantt\n    title Plan\n    dateFormat YYYY-MM-DD\n    section Core\n    Model :done, a1, 2026-11-19, 2d\n";
const PIE: &str = "pie title T\n    \"A\" : 15\n    \"B\" : 85\n";
const MINDMAP: &str = "mindmap\n  root((r))\n    a\n    b\n";

#[test]
fn valid_flowchart_returns_svg_with_the_readmd_theme_and_a_transparent_background() {
    let Rendered::Svg(svg) = render_svg(FLOWCHART) else {
        panic!("expected svg");
    };
    assert!(svg.trim_start().starts_with("<svg"));
    assert!(!svg.contains("<foreignObject"), "labels must be resvg-safe");
    // readmd palette, not Mermaid's dark theme.
    assert!(svg.contains("#61afef"), "readmd theme colours missing");
    assert!(!svg.contains("#1f2020"), "dark theme colours present");
    assert!(!svg.contains("#22D3EE"), "default theme colours present");
    // Transparent: the root element sets no background.
    let root = &svg[..svg.find('>').unwrap_or(svg.len())];
    assert!(
        !root.contains("background"),
        "svg paints a background: {root}"
    );
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
        render_text("graph LR\n A-->B\n"),
        Rendered::Text(_)
    ));
}

#[test]
fn source_hash_is_stable_for_identical_sources_and_differs_for_different_ones() {
    assert_eq!(source_hash(FLOWCHART), source_hash(FLOWCHART));
    assert_ne!(source_hash(FLOWCHART), source_hash(STATE));
    // Pinned value: stable across runs and releases (FNV-1a 64).
    assert_eq!(source_hash(""), 0xcbf2_9ce4_8422_2325);
}

#[test]
fn site_config_sets_the_base_theme_with_variables_from_the_readmd_theme() {
    use readmd::diagram::site_config;
    use readmd::theme::{ACCENT, FG, LINK, hex};
    let cfg = site_config();
    assert_eq!(cfg["theme"], "base");
    let v = &cfg["themeVariables"];
    assert_eq!(v["primaryColor"], hex(LINK));
    assert_eq!(v["primaryBorderColor"], hex(ACCENT));
    assert_eq!(v["textColor"], hex(FG));
    for key in [
        "secondaryColor",
        "tertiaryColor",
        "lineColor",
        "edgeLabelBackground",
        "noteBkgColor",
        "actorBkg",
        "cScale0",
        "cScale11",
        "pie1",
        "pie12",
    ] {
        let s = v[key].as_str().unwrap_or_default();
        assert!(
            s.len() == 7 && s.starts_with('#'),
            "{key} missing or not #rrggbb: {s:?}"
        );
    }
}

#[test]
fn edge_label_backgrounds_are_opaque_in_the_svg() {
    let Rendered::Svg(svg) = render_svg("flowchart LR\n    A -->|yes| B\n") else {
        panic!("no svg");
    };
    let bg = readmd::theme::hex(readmd::theme::EMPHASIS);
    assert!(svg.contains(&bg), "edge label background {bg} not in SVG");
    assert!(
        !svg.contains(".edgeLabel rect{opacity:0.5"),
        "edge label rectangle is faded"
    );
    assert!(svg.contains(".edgeLabel rect{opacity:1;"), "no opaque rule");
    assert!(svg.contains(&format!("fill=\"{bg}\"")), "label rect fill");
}

const SAMPLES: [&str; 7] = [
    "flowchart TD\n    A[Open] --> B{Has?}\n    B -- yes --> C[Render]\n    B -- no --> D[Plain]\n",
    "sequenceDiagram\n    participant U as User\n    participant R as readmd\n    U->>R: open\n    Note over U,R: note\n",
    "classDiagram\n    class Document {\n        +Vec~Block~ blocks\n    }\n    class Block\n    Document \"1\" --> \"*\" Block\n",
    "gantt\n    title Plan\n    dateFormat  YYYY-MM-DD\n    section Core\n    Model :done, a1, 2026-11-19, 2d\n    Layout :active, a2, after a1, 3d\n    section Pictures\n    Diagrams : b1, after a2, 4d\n",
    "mindmap\n  root((readmd))\n    Reading\n      Markdown\n    Pictures\n      Mermaid\n    Navigation\n",
    "stateDiagram-v2\n    [*] --> Reading\n    Reading --> Searching: Ctrl+F\n    Reading --> [*]\n",
    "pie title Time\n    \"Parsing\" : 15\n    \"Layout\" : 25\n    \"Diagrams\" : 45\n",
];

#[test]
fn all_sample_diagram_types_render_with_the_readmd_palette() {
    let link = readmd::theme::hex(readmd::theme::LINK);
    for src in SAMPLES {
        let Rendered::Svg(svg) = render_svg(src) else {
            panic!("no svg for {src}");
        };
        assert!(svg.contains("<svg"), "{src}");
        assert!(
            svg.to_lowercase().contains("#89dceb")
                || svg.to_lowercase().contains(&link)
                || svg.to_lowercase().contains("#ff79c6"),
            "no readmd colour in SVG of {src}"
        );
    }
}

#[test]
fn class_cardinality_labels_are_not_drawn_in_the_page_background_colour() {
    let Rendered::Svg(svg) = render_svg(SAMPLES[2]) else {
        panic!("no svg");
    };
    let bg = readmd::theme::hex(readmd::theme::BG);
    let terminals: Vec<&str> = svg
        .split("<text ")
        .skip(1)
        .filter(|t| {
            let tag = &t[..t.find('>').unwrap_or(t.len())];
            tag.contains("edgeTerminals")
        })
        .collect();
    assert!(terminals.len() >= 2, "no cardinality labels found");
    for t in terminals {
        let tag = &t[..t.find('>').unwrap_or(t.len())];
        assert!(
            !tag.contains(&format!("fill=\"{bg}\"")) && !tag.contains(&format!("fill:{bg}")),
            "dark text on the page background: {tag}"
        );
    }
}
