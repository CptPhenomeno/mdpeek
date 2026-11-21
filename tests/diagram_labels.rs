//! Diagram labels must fit their boxes: the text drawn by resvg is as wide as the
//! text merman measured, so the label viewports (`overflow:hidden`) clip nothing.

#![allow(clippy::unwrap_used, clippy::expect_used)] // panics are fine in tests

use readmd::diagram::{Rendered, render_svg};
use readmd::raster::rasterise;

const CLASS: &str = "classDiagram\n  class Layout\n  class Document\n  class Block\n  Document : +Vec<Row> rows\n  Document : +from_path(Path) Document\n  Layout --> Document\n  Document --> Block\n";
const MINDMAP: &str = "mindmap\n  root((readmd))\n    Navigation\n    Markdown\n    Mermaid\n";
const STATE: &str = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Focused : Enter on a diagram\n  Focused --> Idle\n";

fn svg(src: &str) -> String {
    match render_svg(src) {
        Rendered::Svg(s) => s,
        other => panic!("not an SVG: {other:?}"),
    }
}

fn assert_nothing_clipped(name: &str, src: &str) {
    let clipped = svg(src);
    assert!(
        clipped.contains("overflow:hidden"),
        "{name}: no label viewport to test"
    );
    let open = clipped.replace("overflow:hidden", "overflow:visible");
    let a = rasterise(&clipped, 4000, 4000).expect("raster");
    let b = rasterise(&open, 4000, 4000).expect("raster");
    assert_eq!(a.dimensions(), b.dimensions());
    let differing = a
        .pixels()
        .zip(b.pixels())
        .filter(|(x, y)| x.0.iter().zip(y.0).any(|(p, q)| p.abs_diff(q) > 120))
        .count();
    assert_eq!(
        differing, 0,
        "{name}: label text is clipped by its box ({differing} pixels)"
    );
}

#[test]
fn class_diagram_labels_are_not_clipped() {
    assert_nothing_clipped("class", CLASS);
}

#[test]
fn mindmap_labels_are_not_clipped() {
    assert_nothing_clipped("mindmap", MINDMAP);
}

#[test]
fn state_edge_labels_are_not_clipped() {
    assert_nothing_clipped("state", STATE);
}
