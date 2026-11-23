//! Large-document performance (SPEC §4.5, §5): layout of ~1 MB.

use std::time::Instant;

use mdpeek::doc::{Document, Source};
use mdpeek::layout;

/// A generated document of about `target` bytes mixing every block kind.
fn generated(target: usize) -> String {
    let mut md = String::with_capacity(target + 4096);
    let mut n = 0;
    while md.len() < target {
        md.push_str(&format!("## Section {n}\n\n"));
        md.push_str(&format!(
            "Paragraph {n} with **bold**, *italic*, `code` and a [link](https://example.com/{n}). \
             Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor \
             incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud.\n\n"
        ));
        md.push_str(
            "- first item with some text\n- second item\n  - nested item\n- third item\n\n",
        );
        md.push_str("| Name | Value | Notes |\n|:-----|------:|:-----:|\n");
        for r in 0..4 {
            md.push_str(&format!("| row {r} | {} | note number {n} |\n", r * n));
        }
        md.push('\n');
        md.push_str("```rust\nfn main() {\n    let x = 42;\n    println!(\"{x}\");\n}\n```\n\n");
        md.push_str("> A quoted remark for section.\n\n");
        n += 1;
    }
    md
}

fn lay_out(md: &str) -> (usize, std::time::Duration) {
    let doc = Document::from_bytes(md.as_bytes(), Source::Markdown);
    let start = Instant::now();
    let l = layout::layout(&doc, 100, (10, 20));
    (l.rows.len(), start.elapsed())
}

/// Release budget is 100 ms; this generous bound (debug builds included)
/// only catches large regressions.
#[test]
fn layout_of_one_megabyte_stays_within_a_generous_bound() {
    let md = generated(1_000_000);
    assert!(md.len() >= 1_000_000);
    let (rows, took) = lay_out(&md);
    assert!(rows > 10_000);
    assert!(took.as_millis() < 3000, "layout took {took:?}");
}

/// `cargo test --release --test perf_large -- --ignored --nocapture`
#[test]
#[ignore = "benchmark: run in release"]
fn layout_of_one_megabyte_under_100_ms_in_release() {
    let md = generated(1_000_000);
    let mut best = std::time::Duration::MAX;
    for _ in 0..5 {
        let (rows, took) = lay_out(&md);
        best = best.min(took);
        eprintln!("layout: {rows} rows in {took:?}");
    }
    assert!(best.as_millis() < 100, "best layout {best:?}");
}
