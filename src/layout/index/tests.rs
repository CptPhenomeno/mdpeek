use super::*;
use crate::doc::{Document, Source};
use crate::layout::{Layout, PictureKey, RowKind, layout};

const CELL: (u16, u16) = (10, 20);
const DIAGRAM: &str = "```mermaid\ngraph TD\n  A-->B\n```";

fn doc(md: &str) -> Document {
    Document::from_bytes(md.as_bytes(), Source::Markdown)
}

fn lay(md: &str, width: u16) -> Layout {
    layout(&doc(md), width, CELL)
}

fn link(target: &str, fragments: &[(usize, u16, u16)]) -> Focusable {
    Focusable::Link {
        target: target.into(),
        fragments: fragments
            .iter()
            .map(|&(row, start, end)| Fragment { row, start, end })
            .collect(),
    }
}

#[test]
fn heading_index_lists_level_text_slug_and_row_in_document_order() {
    let md = "# Title\n\ntext\n\n## Sub *part*\n\n### Deep\n\n# Title";
    let d = doc(md);
    let l = layout(&d, 30, CELL);
    let got: Vec<_> = l
        .index
        .headings
        .iter()
        .map(|h| (h.level, h.text.as_str(), h.slug.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (1, "Title", "title"),
            (2, "Sub part", "sub-part"),
            (3, "Deep", "deep"),
            (1, "Title", "title-1"),
        ]
    );
    let slugs = d.heading_slugs();
    for (h, slug) in l.index.headings.iter().zip(&slugs) {
        assert_eq!(&h.slug, slug);
        assert_eq!(l.rows[h.row].kind, RowKind::Heading(h.level));
        assert!(h.row == 0 || l.rows[h.row - 1].kind != RowKind::Heading(h.level));
    }
    assert_eq!(l.rows[l.index.headings[1].row].text(), "Sub part");
}

#[test]
fn a_link_wrapped_over_two_rows_is_one_focusable_with_two_fragments() {
    let l = lay("aaaa [bbbb cccc dddd eeee](http://x) ffff", 20);
    assert_eq!(l.rows[0].text(), "aaaa bbbb cccc");
    assert_eq!(l.rows[1].text(), "dddd eeee ffff");
    assert_eq!(
        l.index.focusables,
        [link("http://x", &[(0, 5, 14), (1, 0, 9)])]
    );
}

#[test]
fn focusables_are_ordered_by_document_position_mixing_links_and_diagrams() {
    let md = format!("[a](http://one)\n\n{DIAGRAM}\n\n[b](#two) and [c](http://three)");
    let l = lay(&md, 30);
    let kinds: Vec<String> = l
        .index
        .focusables
        .iter()
        .map(|f| match f {
            Focusable::Link { target, .. } => target.clone(),
            Focusable::Diagram { key, .. } => format!("{key:?}"),
        })
        .collect();
    assert_eq!(kinds, ["http://one", "Diagram(0)", "#two", "http://three"]);
}

#[test]
fn a_diagram_focusable_covers_its_slot_rows() {
    let l = lay(&format!("intro\n\n{DIAGRAM}\n\noutro"), 30);
    let first = l.rows.iter().position(|r| r.slot.is_some()).unwrap();
    let rows = l.rows[first].slot.as_ref().unwrap().rows as usize;
    assert_eq!(rows, 10);
    assert_eq!(
        l.index.focusables,
        [Focusable::Diagram {
            key: PictureKey::Diagram(0),
            rows: first..first + rows
        }]
    );
}

#[test]
fn search_text_includes_table_cell_text() {
    let l = lay("| h1 | h2 |\n|---|---|\n| foo bar | baz |", 40);
    let texts: Vec<&str> = l.index.texts.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["h1", "h2", "foo bar", "baz"]);
    assert!(texts.iter().all(|t| !t.contains('│') && !t.contains('─')));
}

#[test]
fn search_text_excludes_picture_slots() {
    let md = format!(
        "before\n\n![alt text](https://x/y.png)\n\n{DIAGRAM}\n\n![gone](nope.png)\n\nafter"
    );
    let l = lay(&md, 40);
    let texts: Vec<&str> = l.index.texts.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["before", "after"]);
}

#[test]
fn a_text_offset_in_a_wrapped_paragraph_maps_to_the_right_screen_row_and_column() {
    let l = lay("aaaa bbbb cccc dddd", 20);
    let t = &l.index.texts[0];
    assert_eq!(t.text, "aaaa bbbb cccc dddd");
    let at = t.text.find("cccc dddd").unwrap();
    assert_eq!(
        t.fragments(at..at + 9),
        [
            Fragment {
                row: 0,
                start: 10,
                end: 14
            },
            Fragment {
                row: 1,
                start: 0,
                end: 4
            },
        ]
    );
    // markers are not part of the text and shift the columns
    let l = lay("- aaaa bbbb cccc dddd", 20);
    let t = &l.index.texts[0];
    assert_eq!(t.text, "aaaa bbbb cccc dddd");
    let at = t.text.find("dddd").unwrap();
    assert_eq!(
        t.fragments(at..at + 4),
        [Fragment {
            row: 1,
            start: 2,
            end: 6
        }]
    );
}

#[test]
fn index_rows_change_when_the_layout_is_recomputed_for_another_width() {
    let md = "# H\n\naaaa bbbb cccc dddd eeee ffff [link text here](http://x)\n\n## Two";
    let d = doc(md);
    let narrow = layout(&d, 20, CELL);
    let wide = layout(&d, 80, CELL);
    assert_ne!(narrow.index.headings[1].row, wide.index.headings[1].row);
    assert_ne!(narrow.index.focusables, wide.index.focusables);
    assert_eq!(wide.index.texts[1].fragments(0..4).len(), 1);
    assert_eq!(narrow.index.headings.len(), wide.index.headings.len());
}

#[test]
fn code_block_lines_are_included_in_the_layout_s_search_index_and_map_back_to_their_rows() {
    let l = lay("para\n\n```rust\nfn a() {}\n\nlet b = 1;\n```", 40);
    let texts: Vec<&str> = l.index.texts.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["para", "fn a() {}", "let b = 1;"]);
    for (t, line) in l.index.texts[1..].iter().zip(["fn a() {}", "let b = 1;"]) {
        let row = l.rows.iter().position(|r| r.text().contains(line)).unwrap();
        let at = t.text.find(&line[3..6]).unwrap();
        assert_eq!(
            t.fragments(at..at + 3),
            [Fragment {
                row,
                start: 5,
                end: 8
            }]
        );
    }
    // a truncated line indexes only what is on screen
    let l = lay(&format!("```\n{}\n```", "q".repeat(60)), 20);
    assert_eq!(l.index.texts[0].text, format!("{}…", "q".repeat(13)));
}
