use ratatui::style::{Color, Modifier};

use super::*;
use crate::doc::{Document, Source};
use unicode_width::UnicodeWidthStr;

const CELL: (u16, u16) = (10, 20);

fn lay(md: &str, width: u16) -> Layout {
    layout(
        &Document::from_bytes(md.as_bytes(), Source::Markdown),
        width,
        CELL,
    )
}

fn texts(l: &Layout) -> Vec<String> {
    l.rows.iter().map(Row::text).collect()
}

fn span<'a>(l: &'a Layout, text: &str) -> &'a RowSpan {
    l.rows
        .iter()
        .flat_map(|r| &r.spans)
        .find(|s| s.text.contains(text))
        .unwrap_or_else(|| panic!("no span with {text:?}"))
}

#[test]
fn paragraph_wraps_at_word_boundaries_within_margins() {
    let l = lay("aaaa bbbb cccc dddd", 22);
    assert_eq!(l.state, State::Ready);
    assert_eq!(texts(&l), ["aaaa bbbb cccc dddd"]);
    let l = lay("aaaa bbbb cccc dddd", 20);
    assert_eq!(texts(&l), ["aaaa bbbb cccc", "dddd"]);
    let l = lay("aaaaaaaa bbbbbbbb cccccccc", 20);
    assert_eq!(texts(&l), ["aaaaaaaa bbbbbbbb", "cccccccc"]);
}

#[test]
fn word_longer_than_the_line_is_broken_across_rows() {
    let l = lay("abcdefghijklmnopqrstuvwxyzabcdefghijkl", 20);
    assert_eq!(
        texts(&l),
        ["abcdefghijklmnopqr", "stuvwxyzabcdefghij", "kl"]
    );
}

#[test]
fn wide_characters_are_measured_by_display_width() {
    let l = lay("漢字漢字 漢字漢字 漢字", 20);
    // content width 18: 8 + 1 + 8 = 17 fits, the third word does not
    assert_eq!(texts(&l), ["漢字漢字 漢字漢字", "漢字"]);
    let l = lay("漢漢漢漢漢漢漢漢漢漢", 20);
    assert_eq!(texts(&l), ["漢漢漢漢漢漢漢漢漢", "漢"]);
}

#[test]
fn h1_and_h2_are_followed_by_a_rule_and_h3_is_not() {
    let l = lay("# A\n\n## B\n\n### C", 30);
    let kinds: Vec<RowKind> = l.rows.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        [
            RowKind::Heading(1),
            RowKind::Rule,
            RowKind::Blank,
            RowKind::Blank,
            RowKind::Heading(2),
            RowKind::Rule,
            RowKind::Blank,
            RowKind::Heading(3),
        ]
    );
}

#[test]
fn each_heading_level_gets_its_own_colour_and_bold() {
    let md = "# a\n\n## b\n\n### c\n\n#### d\n\n##### e\n\n###### f";
    let l = lay(md, 30);
    let mut colours = Vec::new();
    for name in ["a", "b", "c", "d", "e", "f"] {
        let style = span(&l, name).style;
        assert!(style.add_modifier.contains(Modifier::BOLD), "{name}");
        colours.push(style.fg.expect("heading colour"));
    }
    for i in 0..colours.len() {
        for j in i + 1..colours.len() {
            assert_ne!(colours[i], colours[j], "levels {} and {}", i + 1, j + 1);
        }
    }
}

#[test]
fn inline_styles_and_links_are_styled() {
    let l = lay("`code` *em* **strong** ~~gone~~ [link](http://x)", 60);
    assert!(span(&l, "code").style.bg.is_some());
    assert!(span(&l, "em").style.add_modifier.contains(Modifier::ITALIC));
    assert!(
        span(&l, "strong")
            .style
            .add_modifier
            .contains(Modifier::BOLD)
    );
    assert!(
        span(&l, "gone")
            .style
            .add_modifier
            .contains(Modifier::CROSSED_OUT)
    );
    let link = span(&l, "link");
    assert!(link.style.add_modifier.contains(Modifier::UNDERLINED));
    assert!(link.style.fg.is_some());
    assert_eq!(link.link.as_deref(), Some("http://x"));
}

#[test]
fn rule_fills_the_content_width_with_box_drawing_line() {
    let l = lay("a\n\n---\n\nb", 30);
    let rule = l
        .rows
        .iter()
        .find(|r| r.kind == RowKind::Rule)
        .expect("rule");
    assert_eq!(rule.text(), "─".repeat(28));
}

#[test]
fn html_is_laid_out_as_dimmed_raw_text() {
    let l = lay("<div>raw</div>\n\ntext <b>bold</b> end", 40);
    let block = span(&l, "<div>raw</div>");
    assert!(
        block.style.add_modifier.contains(Modifier::DIM) || block.style.fg == Some(Color::DarkGray)
    );
    let inline = span(&l, "<b>");
    assert!(inline.style != Style::default());
    assert!(texts(&l).iter().any(|t| t == "text <b>bold</b> end"));
}

#[test]
fn width_below_twenty_columns_yields_too_narrow_state() {
    let l = lay("hello", 19);
    assert_eq!(l.state, State::TooNarrow);
    assert!(l.rows.is_empty());
    assert_eq!(lay("hello", 20).state, State::Ready);
    assert_eq!(lay("", 40).state, State::Empty);
}

fn table_text(md: &str, width: u16) -> Vec<String> {
    texts(&lay(md, width))
}

#[test]
fn table_is_drawn_with_box_drawing_borders_and_a_bold_header_row() {
    let l = lay("| a | b |\n|---|---|\n| 1 | 2 |", 40);
    assert_eq!(
        texts(&l),
        [
            "┌───┬───┐",
            "│ a │ b │",
            "├───┼───┤",
            "│ 1 │ 2 │",
            "└───┴───┘"
        ]
    );
    assert!(span(&l, "a").style.add_modifier.contains(Modifier::BOLD));
    assert!(!span(&l, "1").style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn table_cells_follow_left_center_and_right_alignment_from_the_delimiter_row() {
    let t = table_text(
        "| l | c | r |\n|:--|:-:|--:|\n| aaaa | bbbb | cccc |\n| x | y | z |",
        60,
    );
    assert_eq!(t[1], "│ l    │  c   │    r │");
    assert_eq!(t[4], "│ x    │  y   │    z │");
}

#[test]
fn table_that_fits_the_pane_keeps_natural_column_widths() {
    let t = table_text("| a | bbb |\n|---|---|\n| cc | d |", 60);
    assert_eq!(t[0], "┌────┬─────┐");
    assert_eq!(t[3], "│ cc │ d   │");
}

#[test]
fn table_wider_than_the_pane_shrinks_its_columns_proportionally_to_fit() {
    let md = "| aaaaaaaaaa | bbbbbbbbbbbbbbbbbbbb |\n|---|---|\n| x | y |";
    let l = lay(md, 20);
    for row in &l.rows {
        assert!(row.text().chars().count() <= 18, "{:?}", row.text());
    }
    let top = l.rows[0].text();
    let cols: Vec<usize> = top.split('┬').map(|s| s.chars().count()).collect();
    // the 20-wide column keeps more room than the 10-wide one
    assert!(cols[1] > cols[0], "{top}");
    assert_eq!(top.chars().count(), 18);
}

#[test]
fn shrunk_table_cell_text_wraps_inside_its_column_and_the_row_grows() {
    let t = table_text("| aaaa bbbb cccc | d |\n|---|---|\n| x | y |", 20);
    // content 18: borders 7, columns 11 -> widths 10 and 1
    assert_eq!(t[0], "┌────────────┬───┐");
    assert_eq!(&t[1..3], ["│ aaaa bbbb  │ d │", "│ cccc       │   │"]);
    assert_eq!(t[4], "│ x          │ y │");
}

#[test]
fn table_cell_inline_styles_survive_wrapping() {
    let l = lay("| **bold words here** | h |\n|---|---|\n| x | y |", 20);
    let bold: Vec<&RowSpan> = l
        .rows
        .iter()
        .flat_map(|r| &r.spans)
        .filter(|s| s.text.contains("bold") || s.text.contains("words") || s.text.contains("here"))
        .collect();
    assert!(bold.len() >= 2);
    assert!(
        bold.iter()
            .all(|s| s.style.add_modifier.contains(Modifier::BOLD))
    );
    let l = lay("| h |\n|---|\n| aa `code` bb cc dd ee |", 20);
    assert!(span(&l, "code").style.bg.is_some());
}

#[test]
fn table_with_more_columns_than_fit_never_exceeds_the_content_width() {
    let md = "| a | b | c | d | e | f | g | h |\n|---|---|---|---|---|---|---|---|\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |";
    let l = lay(md, 20);
    for row in &l.rows {
        assert!(row.text().width() <= 18, "{:?}", row.text());
    }
    assert!(l.rows[1].text().ends_with('…'));
}

#[test]
fn wide_characters_in_a_tightly_shrunk_table_keep_rows_aligned_within_the_content_width() {
    let md = "| 漢漢漢漢漢漢漢漢 | 漢字 漢字 漢字 | 漢 |\n|---|---|---|\n| 漢 | b | c |";
    let l = lay(md, 20);
    let widths: Vec<usize> = l.rows.iter().map(|r| r.text().width()).collect();
    assert!(widths.iter().all(|w| *w == widths[0]), "{widths:?}");
    assert!(widths[0] <= 18, "{widths:?}");
}

#[test]
fn unordered_items_get_bullets_and_ordered_items_get_their_numbers() {
    let l = lay("- a\n- b\n\n3. c\n4. d", 30);
    assert_eq!(texts(&l), ["• a", "• b", "", "3. c", "4. d"]);
}

#[test]
fn nested_list_levels_are_indented_two_columns_per_level() {
    let l = lay("- a\n  - b\n    - c\n- d", 30);
    assert_eq!(texts(&l), ["• a", "  • b", "    • c", "• d"]);
}

#[test]
fn wrapped_item_lines_align_with_the_item_text_not_the_marker() {
    let l = lay("- aaaa bbbb cccc dddd eeee", 20);
    assert_eq!(texts(&l), ["• aaaa bbbb cccc", "  dddd eeee"]);
    let l = lay("10. aaaa bbbb cccc dddd", 20);
    assert_eq!(texts(&l), ["10. aaaa bbbb cccc", "    dddd"]);
}

#[test]
fn task_items_show_empty_and_checked_ballot_box_markers() {
    let l = lay("- [ ] todo\n- [x] done", 30);
    assert_eq!(texts(&l), ["☐ todo", "☑ done"]);
}

#[test]
fn each_quote_nesting_level_adds_a_vertical_bar_and_the_text_is_dimmed() {
    let l = lay("> a\n>\n> > b", 30);
    assert_eq!(texts(&l), ["│ a", "│", "│ │ b"]);
    let s = span(&l, "b");
    assert!(s.style.add_modifier.contains(Modifier::DIM));
    let l = lay("> aaaa bbbb cccc dddd eeee", 20);
    assert_eq!(texts(&l), ["│ aaaa bbbb cccc", "│ dddd eeee"]);
    let l = lay("- > aaaa bbbb cccc dddd", 20);
    assert!(l.rows.iter().all(|r| r.text().width() <= 18));
}

#[test]
fn footnote_references_render_as_bracketed_numbers() {
    let l = lay("see[^a] and[^b]\n\n[^a]: A\n[^b]: B", 40);
    assert_eq!(texts(&l)[0], "see[1] and[2]");
}

#[test]
fn footnote_definitions_are_listed_at_the_end_of_the_document_under_a_rule() {
    let l = lay(
        "see[^a]\n\n[^a]: first note that is long enough to wrap",
        20,
    );
    let t = texts(&l);
    assert_eq!(t[0], "see[1]");
    let rule = t.iter().position(|s| s.starts_with('─')).expect("rule");
    assert_eq!(l.rows[rule].kind, RowKind::Rule);
    assert_eq!(t[rule + 1], "[1] first note");
    assert!(t[rule + 2].starts_with("    "));
    assert!(l.rows.iter().all(|r| r.text().width() <= 18));
}

#[test]
fn tables_nested_in_lists_and_quotes_never_exceed_the_content_width() {
    let md = "- > | aaaaaaaaaa | bbbbbbbbbb | cccccccccc |\n  > |---|---|---|\n  > | 1 | 2 | 3 |\n";
    for w in 20..40 {
        let l = lay(md, w);
        assert!(
            l.rows
                .iter()
                .all(|r| r.text().width() <= usize::from(w) - 2)
        );
    }
}

fn blanks_above(l: &Layout, text: &str) -> usize {
    let at = l
        .rows
        .iter()
        .position(|r| r.text().contains(text))
        .unwrap_or_else(|| panic!("no row with {text:?}"));
    l.rows[..at]
        .iter()
        .rev()
        .take_while(|r| r.kind == RowKind::Blank)
        .count()
}

#[test]
fn h1_and_h2_have_two_blank_rows_above_them_and_h3_to_h6_one() {
    let md = "p0\n\n# a1\n\np1\n\n## b2\n\np2\n\n### c3\n\np3\n\n#### d4\n\np4\n\n##### e5\n\np5\n\n###### f6";
    let l = lay(md, 30);
    for (name, n) in [
        ("a1", 2),
        ("b2", 2),
        ("c3", 1),
        ("d4", 1),
        ("e5", 1),
        ("f6", 1),
    ] {
        assert_eq!(blanks_above(&l, name), n, "{name}");
    }
    // the document never opens with blank rows
    let l = lay("# top\n\ntext", 30);
    assert_eq!(l.rows[0].kind, RowKind::Heading(1));
}

#[test]
fn h1_is_followed_by_a_heavy_rule_and_h2_by_a_thin_rule() {
    let l = lay("# A\n\n## B\n\n### C", 30);
    let rule_after = |h: &str| {
        let at = l.rows.iter().position(|r| r.text() == h).expect(h);
        let r = &l.rows[at + 1];
        assert_eq!(r.kind, RowKind::Rule);
        r.text()
    };
    assert_eq!(rule_after("A"), "═".repeat(28));
    assert_eq!(rule_after("B"), "─".repeat(28));
    let at = l.rows.iter().position(|r| r.text() == "C").expect("C");
    assert_eq!(l.rows.len(), at + 1);
}

#[test]
fn strong_spans_use_a_brighter_foreground_than_body_text_as_well_as_bold() {
    let l = lay("plain **strong**", 40);
    let body = span(&l, "plain").style.fg.expect("body fg");
    let strong = span(&l, "strong").style;
    assert!(strong.add_modifier.contains(Modifier::BOLD));
    assert_eq!(strong.fg, Some(crate::theme::STRONG));
    assert_ne!(strong.fg, Some(body));
    assert_eq!(strong.bg, None);
}

#[test]
fn emphasis_spans_use_a_distinct_foreground_as_well_as_italic() {
    let l = lay("plain *emph* **strong**", 40);
    let body = span(&l, "plain").style.fg;
    let emph = span(&l, "emph").style;
    assert!(emph.add_modifier.contains(Modifier::ITALIC));
    assert_eq!(emph.fg, Some(crate::theme::EMPHASIS));
    assert_ne!(emph.fg, body);
    assert_ne!(emph.fg, span(&l, "strong").style.fg);
}

#[test]
fn styles_still_combine_on_nested_emphasis_inside_headings() {
    let l = lay("## head *it* **bo** ***both***", 50);
    let head = span(&l, "head").style;
    let it = span(&l, "it").style;
    let bo = span(&l, "bo").style;
    let both = span(&l, "both").style;
    assert!(it.add_modifier.contains(Modifier::ITALIC | Modifier::BOLD));
    assert!(bo.add_modifier.contains(Modifier::BOLD));
    assert!(
        both.add_modifier
            .contains(Modifier::ITALIC | Modifier::BOLD)
    );
    // inside a heading the level colour wins over the tints
    for s in [it, bo, both] {
        assert_eq!(s.fg, head.fg);
    }
}

#[test]
fn no_row_exceeds_the_content_width() {
    let md = "# A very long heading that must wrap onto several rows of text\n\n## Another long heading with **strong** and *emphasis* words\n\n- ### nested heading in a list item that is long\n\n> ## quoted heading that is also rather long\n\n[^1]\n\n[^1]: ### note heading text that is long enough to wrap";
    for width in [20u16, 24, 40] {
        let l = lay(md, width);
        for r in &l.rows {
            assert!(
                r.text().width() <= usize::from(width - 2 * MARGIN),
                "{width}: {:?}",
                r.text()
            );
        }
    }
}

// --- image slots and fallback rows (SPEC §3.4.1-2, §3.5.2-5) ---

const DIAGRAM: &str = "```mermaid\ngraph TD\n  A-->B\n```";

fn lay_sized(md: &str, width: u16, height: u16, sizes: &dyn PictureSizes) -> Layout {
    layout_with(
        &Document::from_bytes(md.as_bytes(), Source::Markdown),
        width,
        height,
        CELL,
        sizes,
    )
}

/// Every image and diagram has the same natural size.
fn all_sized(w: u32, h: u32) -> impl Fn(&PictureKey) -> Option<(u32, u32)> {
    move |_: &PictureKey| Some((w, h))
}

fn slots(l: &Layout) -> Vec<&Slot> {
    l.rows.iter().filter_map(|r| r.slot.as_ref()).collect()
}

#[test]
fn slot_rows_equal_ceil_of_pixel_height_over_cell_height() {
    for (h, rows) in [(20, 1), (21, 2), (45, 3), (60, 3), (61, 4)] {
        let l = lay_sized("![a](a.png)", 42, 50, &all_sized(100, h));
        let s = slots(&l);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].rows, rows, "height {h}");
        let picture_rows = l.rows.iter().filter(|r| r.kind == RowKind::Picture).count();
        assert_eq!(picture_rows, usize::from(rows));
    }
}

#[test]
fn small_diagram_keeps_its_natural_size_and_is_not_stretched() {
    let l = lay_sized(DIAGRAM, 82, 40, &all_sized(100, 40));
    let s = slots(&l);
    assert_eq!(s[0].key, PictureKey::Diagram(0));
    assert_eq!(s[0].px, (100, 40));
    assert_eq!((s[0].cols, s[0].rows), (10, 2));
    assert!(s[0].known);
}

#[test]
fn wide_diagram_is_scaled_to_the_content_width_and_capped_at_the_pane_height() {
    // content 40 cols = 400 px; pane 10 rows = 200 px
    let l = lay_sized(DIAGRAM, 42, 10, &all_sized(2000, 1000));
    let s = slots(&l);
    assert_eq!(s[0].px, (400, 200));
    assert_eq!((s[0].cols, s[0].rows), (40, 10));
    // tall and narrow: the height cap wins, the aspect ratio is kept
    let l = lay_sized(DIAGRAM, 42, 10, &all_sized(400, 1000));
    let s = slots(&l);
    assert_eq!(s[0].px, (80, 200));
    assert_eq!((s[0].cols, s[0].rows), (8, 10));
}

#[test]
fn image_slot_never_exceeds_the_natural_size() {
    let l = lay_sized("![a](a.png)", 42, 50, &all_sized(30, 20));
    let s = slots(&l);
    assert_eq!(s[0].px, (30, 20));
    assert_eq!((s[0].cols, s[0].rows), (3, 1));
    assert_eq!(s[0].key, PictureKey::Image("a.png".into()));
}

#[test]
fn picture_is_centred_horizontally_in_the_content_area() {
    // content 40 cols; picture 10 cols -> 15 blank columns on each side
    let l = lay_sized("![a](a.png)", 42, 50, &all_sized(100, 40));
    assert_eq!(slots(&l)[0].left, 15);
    let l = lay_sized("![a](a.png)", 42, 50, &all_sized(2000, 40));
    assert_eq!(slots(&l)[0].left, 0);
}

#[test]
fn missing_image_is_one_dimmed_row_with_the_alt_text() {
    let none = |_: &PictureKey| None;
    let l = lay_sized("![a cat](img/cat.png)", 60, 50, &none);
    assert_eq!(texts(&l), ["[image not found: img/cat.png] a cat"]);
    assert_eq!(l.rows[0].kind, RowKind::ImageNote);
    assert!(l.rows[0].spans.iter().all(|s| s.style == theme::dim()));
    let l = lay_sized("![](cat.png)", 60, 50, &none);
    assert_eq!(texts(&l), ["[image not found: cat.png]"]);
}

#[test]
fn remote_image_is_one_dimmed_row_with_the_alt_text_and_no_size_query() {
    let panics = |_: &PictureKey| -> Option<(u32, u32)> { panic!("remote images are never sized") };
    let l = lay_sized("![logo](https://example.com/a.png)", 60, 50, &panics);
    assert_eq!(
        texts(&l),
        ["[remote image: https://example.com/a.png] logo"]
    );
    assert_eq!(l.rows[0].kind, RowKind::ImageNote);
    assert!(l.rows[0].spans.iter().all(|s| s.style == theme::dim()));
}

#[test]
fn no_text_row_overlaps_a_slot() {
    let l = lay_sized(
        "before ![a](a.png) after\n\nnext",
        42,
        50,
        &all_sized(100, 60),
    );
    assert_eq!(texts(&l), ["before", "", "", "", "after", "", "next"]);
    let kinds: Vec<RowKind> = l.rows.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds[..5],
        [
            RowKind::Text,
            RowKind::Picture,
            RowKind::Picture,
            RowKind::Picture,
            RowKind::Text
        ]
    );
    assert!(l.rows[1].slot.is_some());
    assert!(l.rows[2].slot.is_none() && l.rows[3].slot.is_none());
    for r in l.rows.iter().filter(|r| r.kind == RowKind::Picture) {
        assert!(r.spans.is_empty());
    }
}

#[test]
fn diagram_of_unknown_size_reserves_a_ten_row_placeholder() {
    let none = |_: &PictureKey| None;
    let l = lay_sized(DIAGRAM, 42, 50, &none);
    let s = slots(&l);
    assert_eq!(s[0].rows, 10);
    assert!(!s[0].known);
    assert_eq!(l.rows.len(), 10);
}

#[test]
fn diagrams_are_numbered_in_document_order_including_nested_ones() {
    let md = format!("{DIAGRAM}\n\n- item\n\n  ```mermaid\n  graph\n  ```\n\n> text");
    let l = lay_sized(&md, 42, 50, &all_sized(100, 20));
    let keys: Vec<&PictureKey> = slots(&l).into_iter().map(|s| &s.key).collect();
    assert_eq!(keys, [&PictureKey::Diagram(0), &PictureKey::Diagram(1)]);
}

#[test]
fn pictures_in_lists_and_quotes_keep_the_width_invariant_and_shift_right() {
    let md = "- ![a](a.png)\n\n> ![b](b.png)\n\n- text ![c](c.png)";
    for width in [20u16, 30, 42] {
        let l = lay_sized(md, width, 50, &all_sized(5000, 40));
        let s = slots(&l);
        assert_eq!(s.len(), 3);
        for slot in s {
            assert!(slot.left + slot.cols <= width - 2 * MARGIN, "{slot:?}");
            assert!(slot.left >= 2);
        }
        for r in &l.rows {
            assert!(r.text().width() <= usize::from(width - 2 * MARGIN));
        }
    }
}

// --- code blocks (SPEC §3.3, §4.2) ---

fn code_rows(l: &Layout) -> Vec<&Row> {
    l.rows.iter().filter(|r| r.kind == RowKind::Code).collect()
}

#[test]
fn code_block_is_drawn_inside_a_box_with_the_code_background_colour() {
    let l = lay("```\nab\ncd\n```", 30);
    let rows = code_rows(&l);
    assert_eq!(rows.len(), 4);
    let content = 28;
    let text: Vec<String> = rows.iter().map(|r| r.text()).collect();
    assert!(text[0].starts_with('┌') && text[0].ends_with('┐'));
    assert!(text[1].starts_with("│ ab") && text[1].ends_with(" │"));
    assert!(text[3].starts_with('└') && text[3].ends_with('┘'));
    for r in &rows {
        assert_eq!(r.text().width(), content);
        for s in &r.spans {
            assert_eq!(s.style.bg, Some(theme::CODE_BG), "{:?}", s.text);
            assert!(s.style.fg.is_some());
            assert_ne!(s.style.bg, Some(Color::Reset));
        }
    }
}

#[test]
fn language_label_appears_at_the_top_right_of_the_box() {
    let l = lay("```rust\nlet a = 1;\n```", 30);
    let top = code_rows(&l)[0].text();
    assert!(top.starts_with('┌') && top.ends_with("┐"));
    assert!(top.contains(" rust "), "{top:?}");
    assert!(top.trim_end_matches("─┐").ends_with(" rust "), "{top:?}");
    // fence info after a comma is not part of the label
    let l = lay("```rust,ignore\nlet a = 1;\n```", 30);
    let top = code_rows(&l)[0].text();
    assert!(top.contains(" rust ") && !top.contains("ignore"), "{top:?}");
    // no language: no label, plain border
    let l = lay("```\nx\n```", 30);
    assert!(
        code_rows(&l)[0]
            .text()
            .chars()
            .skip(1)
            .all(|c| c == '─' || c == '┐')
    );
}

#[test]
fn line_longer_than_the_box_is_truncated_with_an_ellipsis_and_not_wrapped() {
    let long = "x".repeat(100);
    for width in [20u16, 30, 60] {
        let l = lay(&format!("```\n{long}\n```"), width);
        let rows = code_rows(&l);
        assert_eq!(rows.len(), 3, "{width}");
        let inner = usize::from(width) - 2 - 4;
        assert_eq!(
            rows[1].text(),
            format!("│ {}… │", "x".repeat(inner - 1)),
            "{width}"
        );
    }
    // a line that exactly fits is not truncated
    let l = lay(&format!("```\n{}\n```", "y".repeat(14)), 20);
    assert_eq!(code_rows(&l)[1].text(), format!("│ {} │", "y".repeat(14)));
}

#[test]
fn highlighted_span_colours_are_kept_inside_the_box() {
    let l = lay("```rust\nfn main() { let a = \"s\"; }\n```", 60);
    let rows = code_rows(&l);
    // Highlighting is deferred: the laid-out spans are plain, the displayed ones coloured.
    let plain: std::collections::HashSet<_> = rows[1].spans.iter().map(|s| s.style.fg).collect();
    let shown = rows[1].display_spans();
    let fgs: std::collections::HashSet<_> = shown
        .iter()
        .filter(|s| s.unit.is_some())
        .map(|s| s.style.fg)
        .collect();
    assert!(plain.len() <= 3);
    assert!(fgs.len() > 1, "{fgs:?}");
    assert_eq!(
        shown.iter().map(|s| s.text.as_str()).collect::<String>(),
        rows[1].text()
    );
    assert!(shown.iter().all(|s| s.style.bg == Some(theme::CODE_BG)));
    // unknown language: plain text in the body foreground
    let l = lay("```nonsense\nhello\n```", 40);
    let s = span(&l, "hello");
    assert_eq!(s.style.fg, Some(theme::FG));
    assert_eq!(s.style.bg, Some(theme::CODE_BG));
}

#[test]
fn truncation_measures_wide_characters_by_display_width() {
    let l = lay(&format!("```\n{}\n```", "漢".repeat(30)), 20);
    let rows = code_rows(&l);
    // inner width 14: seven wide characters would be 14, so six plus the ellipsis (13) and a pad
    for r in &rows {
        assert_eq!(r.text().width(), 18);
    }
    let body = rows[1].text();
    assert!(
        body.contains('…') && body.contains("漢漢漢漢漢漢…"),
        "{body:?}"
    );
    assert!(!body.contains("漢漢漢漢漢漢漢"), "{body:?}");
}

#[test]
fn code_boxes_in_lists_and_quotes_and_indented_code_keep_the_width_invariant() {
    let long = "z".repeat(80);
    let md = format!(
        "- item\n\n  ```rust\n  {long}\n  ```\n\n> ```\n> {long}\n> ```\n\n    {long}\n\n\tindented\twith tabs"
    );
    for width in [20u16, 24, 40] {
        let l = lay(&md, width);
        assert!(!code_rows(&l).is_empty());
        for r in &l.rows {
            assert!(
                r.text().width() <= usize::from(width - 2 * MARGIN),
                "{:?}",
                r.text()
            );
        }
    }
    let l = lay("\tindented\twith tabs", 40);
    assert_eq!(code_rows(&l)[1].text().width(), 38);
    assert!(!code_rows(&l)[1].text().contains('\t'));
}

#[test]
fn mermaid_fence_is_a_diagram_not_a_code_box() {
    let l = lay(DIAGRAM, 40);
    assert!(code_rows(&l).is_empty());
}

// Diagram variants (SPEC §3.4.3-5).

struct Fixed(Vec<DiagramState>);

impl DiagramStates for Fixed {
    fn state(&self, n: usize) -> DiagramState {
        self.0.get(n).cloned().unwrap_or(DiagramState::Ready)
    }
}

fn lay_diagrams(md: &str, width: u16, states: Vec<DiagramState>, text_mode: bool) -> Layout {
    let fixed = Fixed(states);
    layout_diagrams(
        &Document::from_bytes(md.as_bytes(), Source::Markdown),
        width,
        50,
        CELL,
        &all_sized(100, 40),
        Diagrams {
            states: &fixed,
            text_mode,
        },
    )
}

fn assert_within(l: &Layout, width: u16) {
    for r in &l.rows {
        assert!(r.text().width() <= usize::from(width - 2), "{:?}", r.text());
    }
}

#[test]
fn pending_diagram_reserves_a_rendering_placeholder_of_the_expected_height() {
    let l = lay_diagrams(
        DIAGRAM,
        42,
        vec![DiagramState::Pending(Some((100, 50)))],
        false,
    );
    assert_eq!(l.rows.len(), 3);
    let at = l
        .rows
        .iter()
        .position(|r| r.text().contains("Rendering diagram…"));
    assert_eq!(at, Some(1));
    let row = &l.rows[1];
    let text = row.text();
    assert_eq!(text.width(), 40);
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    assert!(lead.abs_diff(trail) <= 1, "centred: {text:?}");
    let s = row
        .spans
        .iter()
        .find(|s| s.text.contains("Rendering"))
        .unwrap();
    assert_eq!(s.style.fg, Some(theme::MUTED));
    assert_eq!(s.style.bg, Some(theme::BG));
    assert!(row.spans.iter().all(|s| s.style.bg == Some(theme::BG)));
    assert!(!l.rows[0].slot.as_ref().unwrap().known);
}

#[test]
fn pending_diagram_of_unknown_size_reserves_10_rows() {
    let l = lay_diagrams(DIAGRAM, 42, vec![DiagramState::Pending(None)], false);
    assert_eq!(l.rows.len(), 10);
    assert!(texts(&l).iter().any(|t| t.contains("Rendering diagram…")));
    assert_within(&l, 42);
}

#[test]
fn failed_diagram_shows_the_message_above_its_source_in_a_red_framed_code_block() {
    let md = "```mermaid\ngraph TD\n  A-->\n```";
    let l = lay_diagrams(
        md,
        42,
        vec![DiagramState::Error("parse error".into())],
        false,
    );
    let t = texts(&l);
    let msg = t.iter().position(|r| r.contains("parse error")).unwrap();
    let src = t.iter().position(|r| r.contains("A-->")).unwrap();
    assert!(msg < src);
    let top = t.iter().position(|r| r.starts_with('┌')).unwrap();
    assert!(top < src && t.last().unwrap().starts_with('└'));
    let border = l.rows[top]
        .spans
        .iter()
        .find(|s| s.text.contains('┌'))
        .unwrap();
    assert_eq!(border.style.fg, Some(theme::ERROR));
    assert_eq!(
        l.rows[msg]
            .spans
            .iter()
            .find(|s| s.text.contains("parse"))
            .unwrap()
            .style
            .fg,
        Some(theme::ERROR)
    );
    assert!(
        l.rows[src]
            .spans
            .iter()
            .any(|s| s.text.contains('│') && s.style.fg == Some(theme::ERROR))
    );
    assert_within(&l, 42);
    // Still focusable, over every row of the block, and searchable like code.
    assert!(
        l.index
            .focusables
            .iter()
            .any(|f| matches!(f, Focusable::Diagram { rows, .. } if rows.len() == l.rows.len()))
    );
}

#[test]
fn a_failed_diagram_does_not_change_the_layout_of_the_blocks_after_it() {
    let md = format!("{DIAGRAM}\n\nafter the diagram\n\n- item");
    let ok = lay_diagrams(&md, 42, vec![DiagramState::Ready], false);
    let bad = lay_diagrams(
        &md,
        42,
        vec![DiagramState::Error("boom\nline two".into())],
        false,
    );
    let tail = |l: &Layout| -> Vec<(usize, String)> {
        let from = l
            .rows
            .iter()
            .position(|r| r.text() == "after the diagram")
            .unwrap();
        l.rows[from..].iter().map(|r| (r.block, r.text())).collect()
    };
    assert_eq!(tail(&ok), tail(&bad));
    assert_ne!(ok.rows.len(), bad.rows.len());
}

#[test]
fn text_mode_lays_out_the_text_output_instead_of_an_image_slot() {
    let drawing = "┌───┐\n│ A │\n└───┘\n".to_string() + &"x".repeat(100);
    let l = lay_diagrams(DIAGRAM, 42, vec![DiagramState::Text(drawing)], true);
    assert!(slots(&l).iter().all(|s| !s.known));
    assert!(l.rows.iter().all(|r| r.kind != RowKind::Picture));
    let t = texts(&l);
    assert!(t.iter().any(|r| r.contains("│ A │")));
    assert!(
        t.iter().any(|r| r.contains('…')),
        "wide output is truncated"
    );
    assert_within(&l, 42);
    // Outside text mode the drawing is not shown.
    let img = lay_diagrams(DIAGRAM, 42, vec![DiagramState::Text("zzz".into())], false);
    assert!(!texts(&img).iter().any(|r| r.contains("zzz")));
}

#[test]
fn text_mode_without_text_output_shows_the_source_with_the_note() {
    let md = "```mermaid\ngantt\n  title T\n```";
    let l = lay_diagrams(md, 60, vec![DiagramState::TextUnavailable], true);
    let t = texts(&l);
    assert!(t.iter().any(|r| r.contains("gantt")));
    assert!(
        t.iter()
            .any(|r| r.contains("Text rendering not available for this diagram type"))
    );
    assert!(t.iter().any(|r| r.starts_with('┌')));
    assert_within(&l, 60);
    let nested = lay_diagrams(
        &format!("- item\n\n  {}", md.replace('\n', "\n  ")),
        30,
        vec![DiagramState::TextUnavailable],
        true,
    );
    assert_within(&nested, 30);
}
