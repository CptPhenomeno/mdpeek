use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

use super::*;

const DOC: &str = "First notes here\n\nsecond paragraph\n\nThird NOTES line\n";

fn ctrl_f(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn search_for(md: &str, height: u16, query: &str) -> App {
    let mut app = app_from(md, 50, height + CHROME);
    ctrl_f(&mut app);
    type_text(&mut app, query);
    app
}

fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    terminal.backend().buffer().clone()
}

/// Cells whose background is `bg`, as (x, y).
fn cells_with_bg(buf: &Buffer, bg: ratatui::style::Color) -> Vec<(u16, u16)> {
    let area = buf.area;
    let mut out = Vec::new();
    for y in 0..area.height {
        for x in 0..area.width {
            if buf[(x, y)].bg == bg {
                out.push((x, y));
            }
        }
    }
    out
}

fn status(app: &mut App, height: u16) -> String {
    screen(app, 50, height).pop().unwrap()
}

#[test]
fn ctrl_f_opens_the_search_input_in_the_status_bar() {
    let mut app = app_from(DOC, 50, 8 + CHROME);
    assert!(!app.text_input_open());
    ctrl_f(&mut app);
    assert!(app.text_input_open());
    type_text(&mut app, "abc");
    assert!(status(&mut app, 8 + CHROME).contains("abc"));
}

#[test]
fn typing_updates_the_matches_and_scrolls_the_current_match_into_view() {
    let md = format!("{}\n\nfinal needle\n", many(40));
    let mut app = app_from(&md, 50, 8 + CHROME);
    assert_eq!(app.scroll(), 0);
    ctrl_f(&mut app);
    type_text(&mut app, "needle");
    assert!(app.scroll() > 0);
    let text = screen_text(&mut app, 50, 8 + CHROME);
    assert!(text.contains("final needle"), "{text}");
    press(&mut app, KeyCode::Backspace);
    assert!(status(&mut app, 8 + CHROME).contains("needl"));
}

#[test]
fn all_matches_are_highlighted_and_the_current_one_has_a_distinct_colour() {
    let mut app = search_for(DOC, 8, "notes");
    let buf = buffer(&mut app, 50, 8 + CHROME);
    let others = cells_with_bg(&buf, theme::MATCH_BG);
    let current = cells_with_bg(&buf, theme::CURRENT_MATCH_BG);
    assert_eq!(current.len(), 5, "first match is current");
    assert_eq!(others.len(), 5, "the other match");
    assert!(current.iter().all(|c| !others.contains(c)));
    // Every other cell keeps the explicit page background.
    let page = cells_with_bg(&buf, theme::BG).len();
    assert!(page > 0);
    insta::assert_snapshot!(terminal_snapshot(&mut app));
}

fn terminal_snapshot(app: &mut App) -> String {
    screen(app, 50, 8 + CHROME).join("\n")
}

#[test]
fn enter_closes_the_input_and_keeps_the_highlights() {
    let mut app = search_for(DOC, 8, "notes");
    press(&mut app, KeyCode::Enter);
    assert!(!app.text_input_open());
    let buf = buffer(&mut app, 50, 8 + CHROME);
    assert_eq!(cells_with_bg(&buf, theme::CURRENT_MATCH_BG).len(), 5);
    assert_eq!(cells_with_bg(&buf, theme::MATCH_BG).len(), 5);
}

#[test]
fn n_and_shift_n_move_to_the_next_and_previous_match() {
    let mut app = search_for(DOC, 8, "notes");
    press(&mut app, KeyCode::Enter);
    assert!(status(&mut app, 8 + CHROME).contains("match 1/2"));
    press(&mut app, KeyCode::Char('n'));
    assert!(status(&mut app, 8 + CHROME).contains("match 2/2"));
    press(&mut app, KeyCode::Char('n'));
    assert!(status(&mut app, 8 + CHROME).contains("match 1/2"));
    app.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT));
    assert!(status(&mut app, 8 + CHROME).contains("match 2/2"));
}

#[test]
fn status_bar_shows_match_i_of_n_or_no_matches() {
    let mut app = search_for(DOC, 8, "notes");
    assert!(status(&mut app, 8 + CHROME).contains("match 1/2"));
    type_text(&mut app, "zzz");
    assert!(status(&mut app, 8 + CHROME).contains("No matches"));
    press(&mut app, KeyCode::Enter);
    assert!(status(&mut app, 8 + CHROME).contains("No matches"));
}

#[test]
fn esc_closes_the_input_and_clears_the_highlights() {
    let mut app = search_for(DOC, 8, "notes");
    let buf = buffer(&mut app, 50, 8 + CHROME);
    assert_eq!(cells_with_bg(&buf, theme::CURRENT_MATCH_BG).len(), 5);
    press(&mut app, KeyCode::Esc);
    assert!(!app.text_input_open());
    let buf = buffer(&mut app, 50, 8 + CHROME);
    assert!(cells_with_bg(&buf, theme::CURRENT_MATCH_BG).is_empty());
    assert!(cells_with_bg(&buf, theme::MATCH_BG).is_empty());
    let line = status(&mut app, 8 + CHROME);
    assert!(!line.contains("match") && !line.contains("No matches"));
}

#[test]
fn q_is_typed_as_text_while_the_search_input_is_open_and_does_not_quit() {
    let mut app = app_from("a quick question\n", 50, 8 + CHROME);
    ctrl_f(&mut app);
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.should_quit());
    assert!(status(&mut app, 8 + CHROME).contains('q'));
    assert!(status(&mut app, 8 + CHROME).contains("match 1/2"));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('q'));
    assert!(app.should_quit());
}
