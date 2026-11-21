use super::*;
use crate::doc::{Document, Source};
use crate::layout::layout;

const CELL: (u16, u16) = (10, 20);

fn lay(md: &str, width: u16) -> Layout {
    layout(
        &Document::from_bytes(md.as_bytes(), Source::Markdown),
        width,
        CELL,
    )
}

fn frags(m: &Match) -> Vec<(usize, u16, u16)> {
    m.fragments
        .iter()
        .map(|f| (f.row, f.start, f.end))
        .collect()
}

#[test]
fn matches_are_found_case_insensitively_across_the_rendered_rows() {
    let l = lay("Hello World\n\nhello again", 40);
    assert_eq!(find_matches(&l, "HELLO").len(), 2);
    assert_eq!(find_matches(&l, "wOrLd").len(), 1);
}

#[test]
fn matches_are_returned_in_document_order_with_row_and_column_ranges() {
    let l = lay("# Foo\n\nbar foo\n\nfoo foo", 40);
    let m = find_matches(&l, "foo");
    assert_eq!(m.len(), 4);
    assert!(
        m.windows(2)
            .all(|w| (w[0].block, w[0].range.start) < (w[1].block, w[1].range.start))
    );
    let rows: Vec<_> = m.iter().map(|m| m.fragments[0].row).collect();
    assert!(rows.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(l.rows[m[1].fragments[0].row].text(), "bar foo");
    assert_eq!((m[1].fragments[0].start, m[1].fragments[0].end), (4, 7));
}

#[test]
fn next_after_the_last_match_wraps_to_the_first_and_previous_before_the_first_wraps_to_the_last() {
    let l = lay("a a a", 40);
    let mut s = Search::new();
    s.set_query(&l, "a");
    assert_eq!(s.current_index(), Some(0));
    s.prev();
    assert_eq!(s.current_index(), Some(2));
    s.next();
    assert_eq!(s.current_index(), Some(0));
    s.next();
    s.next();
    assert_eq!(s.current_index(), Some(2));
    s.next();
    assert_eq!(s.current_index(), Some(0));
}

#[test]
fn an_empty_query_returns_no_matches() {
    let l = lay("text", 40);
    assert!(find_matches(&l, "").is_empty());
    let mut s = Search::new();
    s.set_query(&l, "");
    assert_eq!(s.current_index(), None);
    s.next();
    assert_eq!(s.current_index(), None);
}

#[test]
fn matches_inside_code_blocks_and_table_cells_are_found() {
    let l = lay(
        "| a | needle |\n| - | - |\n| b | c |\n\n```\nlet Needle = 1;\n```",
        40,
    );
    let m = find_matches(&l, "needle");
    assert!(!m.is_empty());
    assert!(
        m.iter()
            .any(|m| l.rows[m.fragments[0].row].text().contains("needle"))
    );
}

#[test]
fn a_phrase_split_across_two_wrapped_rows_is_found_and_returned_as_two_fragments() {
    let l = lay("aaaa bbbb cccc dddd eeee ffff", 14);
    assert_eq!(l.rows[0].text(), "aaaa bbbb cccc");
    let m = find_matches(&l, "CCCC dddd");
    assert_eq!(m.len(), 1);
    assert_eq!(frags(&m[0]), [(0, 10, 14), (1, 0, 4)]);
}

#[test]
fn non_ascii_text_maps_back_to_original_bytes_and_display_columns() {
    // 'İ' lowercases to two chars (3 bytes); the match must still cover whole chars.
    let l = lay("İstanbul ẞ 日本語 x", 40);
    let m = find_matches(&l, "stanbul");
    assert_eq!(m.len(), 1);
    let text = &l.index.texts[m[0].block].text;
    assert_eq!(&text[m[0].range.clone()], "stanbul");
    let m = find_matches(&l, "ß");
    assert_eq!(m.len(), 1);
    let m = find_matches(&l, "本語");
    assert_eq!(frags(&m[0]), [(0, 13, 17)]);
}

#[test]
fn relayout_keeps_the_current_match_by_logical_position() {
    let md = "one two three four five six seven eight nine ten two";
    let mut s = Search::new();
    s.set_query(&lay(md, 60), "two");
    s.next();
    assert_eq!(s.current_index(), Some(1));
    let pos = s.position();
    s.relayout(&lay(md, 12));
    assert_eq!(s.position(), pos);
    assert_eq!(s.matches().len(), 2);
}
