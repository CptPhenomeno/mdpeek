//! The `fixtures/` showcase (SPEC §5): snapshots of the whole document with
//! placeholder pictures, the element coverage of §3.3, the diagrams and the
//! sidebar edge cases.

use std::path::{Path, PathBuf};

use readmd::diagram::{Rendered, render_svg};
use readmd::doc::{Block, Document, Image, ImageTarget, Inline};
use readmd::layout::{
    self, DiagramState, DiagramStates, Diagrams, Layout, PictureKey, PictureSizes, RowKind,
};
use readmd::tree::{self, Node};

const CELL: (u16, u16) = (10, 20);
/// Natural size of every rendered diagram in the snapshots.
const DIAGRAM_PX: (u32, u32) = (500, 240);
const GRADIENT_PX: (u32, u32) = (240, 90);
const LOGO_PX: (u32, u32) = (160, 160);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn showcase() -> Document {
    Document::from_path(&fixtures().join("showcase.md")).expect("fixtures/showcase.md is readable")
}

/// Every block of the document, depth first, footnotes last.
fn all_blocks(doc: &Document) -> Vec<&Block> {
    fn walk<'a>(blocks: &'a [Block], out: &mut Vec<&'a Block>) {
        for b in blocks {
            out.push(b);
            match b {
                Block::List(l) => l.items.iter().for_each(|i| walk(&i.blocks, out)),
                Block::BlockQuote(inner) => walk(inner, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.blocks, &mut out);
    for f in &doc.footnotes {
        walk(&f.blocks, &mut out);
    }
    out
}

fn inlines_of(block: &Block) -> Vec<&Inline> {
    match block {
        Block::Paragraph(c) | Block::Heading { content: c, .. } => c.iter().collect(),
        Block::Table(t) => t
            .header
            .iter()
            .chain(t.rows.iter().flatten())
            .flatten()
            .collect(),
        _ => Vec::new(),
    }
}

fn diagram_sources(doc: &Document) -> Vec<String> {
    all_blocks(doc)
        .into_iter()
        .filter_map(|b| match b {
            Block::Diagram(src) => Some(src.clone()),
            _ => None,
        })
        .collect()
}

/// Fixed pictures: local files that exist get a known size, the rest are missing.
struct Sizes;

impl PictureSizes for Sizes {
    fn natural_size(&self, key: &PictureKey) -> Option<(u32, u32)> {
        match key {
            PictureKey::Diagram(_) => Some(DIAGRAM_PX),
            PictureKey::Image(p) if !p.exists() => None,
            PictureKey::Image(p) => match p.extension().and_then(|e| e.to_str()) {
                Some("png") => Some(GRADIENT_PX),
                Some("svg") => Some(LOGO_PX),
                _ => None,
            },
        }
    }
}

/// Diagram states as the worker would report them: merman decides valid or broken.
struct States(Vec<DiagramState>);

impl DiagramStates for States {
    fn state(&self, n: usize) -> DiagramState {
        self.0.get(n).cloned().unwrap_or(DiagramState::Ready)
    }
}

fn states(doc: &Document) -> States {
    States(
        diagram_sources(doc)
            .iter()
            .map(|src| match render_svg(src) {
                Rendered::Error(_) => DiagramState::Error("syntax error in diagram".into()),
                _ => DiagramState::Ready,
            })
            .collect(),
    )
}

fn full_layout(doc: &Document, width: u16) -> Layout {
    let states = states(doc);
    layout::layout_diagrams(
        doc,
        width,
        u16::MAX,
        CELL,
        &Sizes,
        Diagrams {
            states: &states,
            text_mode: false,
        },
    )
}

/// The whole document as text, one line per row, between a left and right margin
/// column marked with `|`. Picture slots are filled with `#`, as the placeholder
/// renderer does. Styles are not shown: the layout tests cover them.
fn render(layout: &Layout, width: u16) -> String {
    let content = usize::from(width) - 2 * usize::from(layout::MARGIN);
    let mut slot: Option<(usize, usize)> = None;
    let mut lines = Vec::new();
    for row in &layout.rows {
        let body = if row.kind == RowKind::Picture {
            if let Some(s) = &row.slot {
                slot = Some((usize::from(s.left), usize::from(s.cols)));
            }
            let (left, cols) = slot.unwrap_or((0, 0));
            format!("{}{}", " ".repeat(left), "#".repeat(cols))
        } else {
            // Image paths are absolute: keep the snapshot independent of the checkout.
            row.text()
                .replace(fixtures().to_string_lossy().as_ref(), "fixtures")
        };
        let pad = content.saturating_sub(body.chars().count());
        lines.push(format!("|{body}{}|", " ".repeat(pad)));
    }
    lines.join("\n")
}

#[test]
fn showcase_snapshot_at_80_columns_matches_with_the_placeholder_picture_renderer() {
    let doc = showcase();
    let layout = full_layout(&doc, 80);
    insta::assert_snapshot!(render(&layout, 80));
}

#[test]
fn showcase_snapshot_at_120_columns_matches_with_the_placeholder_picture_renderer() {
    let doc = showcase();
    let layout = full_layout(&doc, 120);
    insta::assert_snapshot!(render(&layout, 120));
}

#[test]
fn the_showcase_document_model_contains_every_spec_3_3_element_kind() {
    let doc = showcase();
    let blocks = all_blocks(&doc);
    let has = |f: &dyn Fn(&Block) -> bool| blocks.iter().any(|b| f(b));
    let inline =
        |f: &dyn Fn(&Inline) -> bool| blocks.iter().any(|b| inlines_of(b).into_iter().any(f));

    for level in 1..=6 {
        assert!(
            has(&|b| matches!(b, Block::Heading { level: l, .. } if *l == level)),
            "heading level {level}"
        );
    }
    let span =
        |f: &dyn Fn(&readmd::doc::Span) -> bool| inline(&|i| matches!(i, Inline::Span(s) if f(s)));
    assert!(span(&|s| s.style.emphasis), "emphasis");
    assert!(span(&|s| s.style.strong), "strong");
    assert!(span(&|s| s.style.strikethrough), "strikethrough");
    assert!(span(&|s| s.style.code), "inline code");
    assert!(span(&|s| s.link.is_some()), "links");
    assert!(inline(&|i| matches!(i, Inline::SoftBreak)), "soft break");
    assert!(inline(&|i| matches!(i, Inline::HardBreak)), "hard break");
    assert!(inline(&|i| matches!(i, Inline::Html(_))), "inline html");
    assert!(
        inline(&|i| matches!(i, Inline::FootnoteRef(_))),
        "footnote reference"
    );
    assert!(!doc.footnotes.is_empty(), "footnote definitions");

    let list =
        |ordered: bool| has(&|b| matches!(b, Block::List(l) if l.start.is_some() == ordered));
    assert!(list(true), "ordered list");
    assert!(list(false), "unordered list");
    assert!(
        has(&|b| matches!(b, Block::List(l) if l.items.iter().any(|i| i.task == Some(true)))),
        "checked task"
    );
    assert!(
        has(&|b| matches!(b, Block::List(l) if l.items.iter().any(|i| i.task == Some(false)))),
        "open task"
    );
    assert!(
        has(
            &|b| matches!(b, Block::List(l) if l.items.iter().any(|i| i.blocks.iter().any(|c| matches!(c, Block::List(_)))))
        ),
        "nested list"
    );
    assert!(
        has(
            &|b| matches!(b, Block::BlockQuote(q) if q.iter().any(|c| matches!(c, Block::BlockQuote(_))))
        ),
        "nested block quote"
    );
    assert!(
        has(&|b| matches!(b, Block::CodeBlock { lang: Some(l), .. } if l == "rust")),
        "highlighted code"
    );
    assert!(
        has(&|b| matches!(b, Block::CodeBlock { lang: None, .. })),
        "code without language"
    );
    assert!(
        has(&|b| matches!(b, Block::CodeBlock { lang: Some(l), .. } if l == "unknownlang")),
        "unknown language"
    );
    assert!(has(&|b| matches!(b, Block::Rule)), "horizontal rule");
    assert!(has(&|b| matches!(b, Block::Html(_))), "html block");
    assert!(has(&|b| matches!(b, Block::Diagram(_))), "diagram");
    let aligns =
        |a: readmd::doc::Align| has(&|b| matches!(b, Block::Table(t) if t.align.contains(&a)));
    assert!(aligns(readmd::doc::Align::Left), "left column");
    assert!(aligns(readmd::doc::Align::Center), "centred column");
    assert!(aligns(readmd::doc::Align::Right), "right column");

    let images: Vec<&Image> = blocks
        .iter()
        .flat_map(|b| inlines_of(b))
        .filter_map(|i| match i {
            Inline::Image(img) => Some(img),
            _ => None,
        })
        .collect();
    let local = |ext: &str| {
        images.iter().any(|i| matches!(&i.target, ImageTarget::Local(p) if p.exists() && p.extension().is_some_and(|e| e == ext)))
    };
    assert!(local("png"), "local png");
    assert!(local("svg"), "local svg");
    assert!(
        images
            .iter()
            .any(|i| matches!(&i.target, ImageTarget::Local(p) if !p.exists())),
        "missing image"
    );
    assert!(
        images
            .iter()
            .any(|i| matches!(&i.target, ImageTarget::Remote(_))),
        "remote image"
    );
}

#[test]
fn merman_renders_the_flowchart_sequence_class_gantt_and_mindmap_diagrams_and_rejects_the_broken_one()
 {
    let doc = showcase();
    let sources = diagram_sources(&doc);
    let kind_of = |src: &str| src.split_whitespace().next().unwrap_or("").to_lowercase();
    let (broken, valid): (Vec<_>, Vec<_>) = sources
        .iter()
        .partition(|s| matches!(render_svg(s), Rendered::Error(_)));
    assert_eq!(broken.len(), 1, "exactly one diagram has a syntax error");
    for expected in [
        "flowchart",
        "sequencediagram",
        "classdiagram",
        "gantt",
        "mindmap",
    ] {
        assert!(
            valid.iter().any(|s| kind_of(s) == expected),
            "no valid {expected} diagram"
        );
    }
    for src in &valid {
        assert!(
            matches!(render_svg(src), Rendered::Svg(_)),
            "does not render: {src}"
        );
    }
}

fn names(nodes: &[Node], out: &mut Vec<String>) {
    for n in nodes {
        out.push(n.name().to_string());
        if let Node::Dir { children, .. } = n {
            names(children, out);
        }
    }
}

#[test]
fn the_showcase_document_model_has_an_ordered_list_with_start_10() {
    let doc = showcase();
    let found = all_blocks(&doc)
        .iter()
        .any(|b| matches!(b, Block::List(l) if l.start == Some(10)));
    assert!(found, "no ordered list starts at 10");
}

#[test]
fn the_sidebar_tree_of_fixtures_hides_the_hidden_folder_and_the_gitignored_file() {
    assert!(
        fixtures().join("ignored.md").exists(),
        "the ignored file exists"
    );
    assert!(
        fixtures().join(".hidden").is_dir(),
        "the hidden folder exists"
    );
    let nodes = tree::scan(&fixtures()).expect("fixtures/ is scannable");
    let mut all = Vec::new();
    names(&nodes, &mut all);
    for shown in ["showcase.md", "diagram.mmd", "empty.md", "latin1.md"] {
        assert!(
            all.iter().any(|n| n == shown),
            "{shown} missing from {all:?}"
        );
    }
    assert!(
        nodes
            .iter()
            .any(|n| matches!(n, Node::Dir { children, .. } if !children.is_empty())),
        "a nested folder with documents"
    );
    assert!(!all.iter().any(|n| n == "ignored.md"), "ignored.md shown");
    assert!(
        !all.iter().any(|n| n == ".hidden" || n == "secret.md"),
        "hidden shown: {all:?}"
    );
}

/// The showcase through the real `App` on a `TestBackend`, pictures drawn by the
/// placeholder renderer (`#`). No job sink is set, so diagrams stay "Rendering diagram…".
fn app_screen(app: &mut readmd::ui::App) -> ratatui::backend::TestBackend {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("test terminal");
    terminal.draw(|f| app.render(f)).expect("draw");
    terminal.backend().clone()
}

fn showcase_app() -> readmd::ui::App {
    let mut app = readmd::ui::App::new(showcase(), "showcase.md".into());
    app.resize(80, 24);
    app
}

#[test]
fn showcase_first_screen_on_a_test_backend_matches() {
    let mut app = showcase_app();
    insta::assert_snapshot!(app_screen(&mut app));
}

#[test]
fn showcase_screen_with_the_local_png_drawn_by_the_placeholder_renderer_matches() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = showcase_app();
    let png = fixtures().join("assets/gradient.png");
    app.add_picture(
        PictureKey::Image(png),
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(GRADIENT_PX.0, GRADIENT_PX.1)),
    );
    // Page down until the first picture is on screen (bounded, deterministic).
    for _ in 0..60 {
        if format!("{}", app_screen(&mut app)).contains("####") {
            break;
        }
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    }
    insta::assert_snapshot!(app_screen(&mut app));
}
