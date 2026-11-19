use ratatui::style::{Color, Modifier};

use super::*;
use crate::doc::{Document, Source};

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
