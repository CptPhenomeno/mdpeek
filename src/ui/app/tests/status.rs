//! The status bar never lets the left part run into the right part (KB-83, SPEC §3.2).

use ratatui::{Terminal, backend::TestBackend};
use unicode_width::UnicodeWidthStr;

use super::*;

const LONG: &str = "Cannot reload a.md: No such file or directory (os error 2), try again later";

/// The status row at `width`, one string per terminal column: the trailing
/// cell of a wide character is skipped, so the string width is the row width.
fn status_row(app: &mut App, width: u16) -> String {
    let height = 10 + CHROME;
    app.resize(width, height);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut row = String::new();
    let mut skip = 0;
    for x in 0..width {
        if skip > 0 {
            skip -= 1;
            continue;
        }
        let symbol = buf[(x, height - 1)].symbol();
        skip = symbol.width().saturating_sub(1);
        row.push_str(symbol);
    }
    row
}

fn with_message(path: &str, message: &str) -> App {
    let mut app = App::new(doc(&many(50)), path.into());
    if !message.is_empty() {
        app.set_message(message);
    }
    app
}

#[test]
fn a_long_status_message_is_truncated_with_an_ellipsis_and_leaves_a_gap_before_the_right_part() {
    let mut app = with_message("a.md", LONG);
    let row = status_row(&mut app, 70);
    let right = format!("… {}% ", app.percent());
    assert_eq!(row.width(), 70, "{row:?}");
    assert!(row.ends_with(&right), "{row:?}");
    assert!(row.starts_with(" a.md | Cannot reload"), "{row:?}");
}

#[test]
fn the_scroll_percentage_is_always_fully_visible_at_narrow_widths() {
    for (path, message) in [
        ("SAMPLE.md", "Graph"),
        ("文書/日本語のファイル名.md", LONG),
        ("a.md", LONG),
    ] {
        for width in 1..=120u16 {
            let mut app = with_message(path, message);
            let row = status_row(&mut app, width);
            let w = usize::from(width);
            let pct = app.percent();
            assert!(row.width() <= w, "{width}: {row:?}");
            if w >= 4 {
                assert!(
                    row.trim_end().ends_with(&format!("{pct}%")),
                    "{width}: {row:?}"
                );
            }
            if w >= 6 {
                // At least one space between the left part and the right part.
                assert!(row.ends_with(&format!(" {pct}% ")), "{width}: {row:?}");
            }
            if w >= 12 {
                assert!(row.starts_with(' '), "{width}: {row:?}");
                assert!(
                    row.contains('…') || row.contains(message),
                    "{width}: {row:?}"
                );
            }
        }
    }
}

#[test]
fn a_wide_character_is_never_split_by_the_truncation() {
    let mut app = with_message("文書/日本語のファイル名.md", "");
    for width in 14..=30u16 {
        let row = status_row(&mut app, width);
        assert!(row.width() <= usize::from(width), "{width}: {row:?}");
        assert!(
            row.ends_with(" 100% ") || row.ends_with(" 0% "),
            "{width}: {row:?}"
        );
        assert!(row.contains('…'), "{width}: {row:?}");
    }
}

#[test]
fn the_search_input_keeps_the_end_of_the_query_and_the_cursor_visible() {
    let mut app = app_from(&many(50), 50, 10 + CHROME);
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    for c in "a very long query that cannot fit END".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    for width in 20..=60u16 {
        let row = status_row(&mut app, width);
        assert!(row.width() <= usize::from(width), "{width}: {row:?}");
        assert!(row.contains("Search: …"), "{width}: {row:?}");
        assert!(row.contains("END_ "), "{width}: {row:?}");
        assert!(row.ends_with("% "), "{width}: {row:?}");
    }
}

#[test]
fn right_items_that_do_not_fit_are_dropped_before_the_percentage() {
    let mut app = with_message("a.md", "");
    app.text_mode = true;
    let row = status_row(&mut app, 12);
    assert!(row.ends_with("% "), "{row:?}");
    assert!(row.width() <= 12, "{row:?}");
    let row = status_row(&mut app, 60);
    assert!(row.contains("Text diagrams | "), "{row:?}");
}
