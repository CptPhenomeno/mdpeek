use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};

use super::App;
use crate::doc::{Document, Source};

/// Terminal rows taken by the borders (2) and the status bar (1).
const CHROME: u16 = 3;

fn doc(md: &str) -> Document {
    Document::from_bytes(md.as_bytes(), Source::Markdown)
}

fn app_from(md: &str, width: u16, height: u16) -> App {
    let mut app = App::new(doc(md), "docs/a.md".into());
    app.resize(width, height);
    app
}

/// `n` one-line paragraphs: 2n - 1 rows.
fn many(n: usize) -> String {
    (0..n)
        .map(|i| format!("para {i}"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn wheel(app: &mut App, kind: MouseEventKind) {
    app.handle_mouse(MouseEvent {
        kind,
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
}

fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect()
        })
        .collect()
}

fn screen_text(app: &mut App, width: u16, height: u16) -> String {
    screen(app, width, height).join("\n")
}

#[test]
fn pgdn_scrolls_by_the_pane_height_minus_one_row() {
    let mut app = app_from(&many(50), 40, 10 + CHROME);
    assert_eq!(app.viewport_height(), 10);
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.scroll(), 9);
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.scroll(), 18);
}

#[test]
fn space_scrolls_one_page_like_pgdn_and_pgup_scrolls_one_page_back() {
    let mut app = app_from(&many(50), 40, 10 + CHROME);
    press(&mut app, KeyCode::Char(' '));
    assert_eq!(app.scroll(), 9);
    press(&mut app, KeyCode::PageUp);
    assert_eq!(app.scroll(), 0);
    press(&mut app, KeyCode::PageUp);
    assert_eq!(app.scroll(), 0);
}

#[test]
fn end_puts_the_last_row_at_the_bottom_of_the_pane_and_home_returns_to_the_top() {
    let mut app = app_from(&many(50), 40, 10 + CHROME);
    press(&mut app, KeyCode::End);
    assert_eq!(app.scroll(), app.row_count() - 10);
    press(&mut app, KeyCode::Home);
    assert_eq!(app.scroll(), 0);
}

#[test]
fn mouse_wheel_down_scrolls_three_rows() {
    let mut app = app_from(&many(50), 40, 10 + CHROME);
    wheel(&mut app, MouseEventKind::ScrollDown);
    assert_eq!(app.scroll(), 3);
    wheel(&mut app, MouseEventKind::ScrollUp);
    assert_eq!(app.scroll(), 0);
}

#[test]
fn scroll_offset_is_clamped_at_the_end_of_the_document() {
    let mut app = app_from(&many(8), 40, 10 + CHROME);
    // 15 rows, 10 visible: the last page starts at row 5.
    for _ in 0..10 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.scroll(), 5);
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.scroll(), 5);
    wheel(&mut app, MouseEventKind::ScrollDown);
    assert_eq!(app.scroll(), 5);
}

#[test]
fn status_bar_shows_the_path_relative_to_the_root_and_the_scroll_percentage() {
    let mut app = app_from(&many(50), 40, 10 + CHROME);
    let lines = screen(&mut app, 40, 10 + CHROME);
    let status = lines.last().unwrap();
    assert!(status.contains("docs/a.md"), "{status:?}");
    assert!(status.contains("0%"), "{status:?}");
    press(&mut app, KeyCode::End);
    let lines = screen(&mut app, 40, 10 + CHROME);
    assert!(lines.last().unwrap().contains("100%"));
}

#[test]
fn transient_status_message_is_cleared_by_the_next_key_press() {
    let mut app = app_from(&many(50), 40, 10 + CHROME);
    app.set_message("Opened in browser");
    assert!(screen_text(&mut app, 40, 10 + CHROME).contains("Opened in browser"));
    press(&mut app, KeyCode::Down);
    assert!(!screen_text(&mut app, 40, 10 + CHROME).contains("Opened in browser"));
}

#[test]
fn empty_file_renders_empty_document() {
    let mut app = app_from("", 40, 10);
    assert!(screen_text(&mut app, 40, 10).contains("Empty document"));
}

#[test]
fn pane_narrower_than_20_columns_renders_window_too_narrow() {
    // Inner width = terminal width - 2 borders.
    let mut app = app_from("hello", 21, 10);
    assert!(screen_text(&mut app, 21, 10).contains("Window too narrow"));
    let mut app = app_from("hello", 22, 10);
    let text = screen_text(&mut app, 22, 10);
    assert!(!text.contains("Window too narrow"));
    assert!(text.contains("hello"));
}

#[test]
fn non_utf8_file_shows_file_is_not_valid_utf8_in_the_status_bar() {
    let document = Document::from_bytes(b"caf\xff ok", Source::Markdown);
    let mut app = App::new(document, "a.md".into());
    app.resize(60, 10);
    let lines = screen(&mut app, 60, 10);
    assert!(lines.last().unwrap().contains("File is not valid UTF-8"));
}

#[test]
fn resize_recomputes_the_layout_and_keeps_the_same_document_position_at_the_top() {
    let long = "lorem ipsum dolor sit amet ".repeat(12);
    let md = (0..30)
        .map(|_| long.trim().to_owned())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut app = app_from(&md, 40, 10 + CHROME);
    let rows_before = app.row_count();
    for _ in 0..200 {
        if app.top_anchor().is_some_and(|(b, o)| b >= 4 && o == 0) {
            break;
        }
        press(&mut app, KeyCode::Down);
    }
    let (block, _) = app.top_anchor().expect("anchor");
    app.resize(100, 10 + CHROME);
    assert_ne!(app.row_count(), rows_before);
    assert_eq!(app.top_anchor(), Some((block, 0)));
    // Mid-block anchors stay inside the same block.
    app.resize(40, 10 + CHROME);
    press(&mut app, KeyCode::Down);
    let (block, offset) = app.top_anchor().unwrap();
    assert!(offset > 0);
    app.resize(60, 10 + CHROME);
    assert_eq!(app.top_anchor().unwrap().0, block);
}

#[test]
fn f10_and_ctrl_q_quit() {
    let mut app = app_from("hi", 40, 10);
    assert!(!app.should_quit());
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.should_quit());
    press(&mut app, KeyCode::F(10));
    assert!(app.should_quit());
    let mut app = app_from("hi", 40, 10);
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    assert!(app.should_quit());
}

#[test]
fn snapshot_of_a_short_document_with_headings_paragraphs_and_inline_styles() {
    let md = "# Title\n\nSome *emphasis*, **strong** and `code` text.\n\n## Section\n\nA [link](https://example.com) and ~~gone~~.\n";
    let mut app = app_from(md, 40, 12);
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    insta::assert_snapshot!(terminal.backend());
}
