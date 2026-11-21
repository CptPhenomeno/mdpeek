use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::ui::keymap::BINDINGS;

const LINKS: &str =
    "# Title\n\nSee [one](#title) and notes here.\n\n```mermaid\ngraph TD\n  A-->B\n```\n";

fn question(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
}

fn ctrl_f(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
}

fn help_text(app: &App) -> String {
    app.help_lines().join("\n")
}

#[test]
fn question_mark_opens_the_help_overlay_and_esc_closes_it() {
    let mut app = app_from(LINKS, 60, 20 + CHROME);
    question(&mut app);
    assert!(app.overlay_open());
    let shown = screen_text(&mut app, 60, 20 + CHROME);
    assert!(shown.contains("Help"), "{shown}");
    press(&mut app, KeyCode::Esc);
    assert!(!app.overlay_open());
    // Shift is needed to type `?`: terminals may report it.
    app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));
    assert!(app.overlay_open());
}

#[test]
fn every_binding_in_the_key_map_appears_in_the_help_text() {
    let app = app_from(LINKS, 60, 20 + CHROME);
    let text = help_text(&app);
    for b in BINDINGS {
        assert!(text.contains(b.label), "missing key {:?}", b.label);
        assert!(
            text.contains(b.description),
            "missing description {:?}",
            b.description
        );
    }
}

#[test]
fn help_lists_the_sidebar_and_full_screen_view_keys() {
    let app = app_from(LINKS, 60, 20 + CHROME);
    let text = help_text(&app);
    for heading in [
        "Global",
        "Document",
        "Sidebar",
        "Search input",
        "Mouse",
        "Full-screen view",
        "Contents (TOC)",
    ] {
        assert!(text.contains(heading), "missing heading {heading:?}");
    }
    for needle in [
        "Collapse the directory",
        "Zoom in",
        "Zoom out",
        "Reset to fit",
        "Pan left",
        "Ctrl+Left",
        "Mouse wheel",
        "Double click",
        "Backspace",
    ] {
        assert!(text.contains(needle), "missing {needle:?}");
    }
}

#[test]
fn esc_closes_an_open_overlay_before_clearing_the_search() {
    for open in [question as fn(&mut App), |a: &mut App| {
        press(a, KeyCode::Char('t'))
    }] {
        let mut app = app_from(LINKS, 60, 20 + CHROME);
        ctrl_f(&mut app);
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Enter);
        assert!(app.search_active());
        open(&mut app);
        assert!(app.overlay_open());
        press(&mut app, KeyCode::Esc);
        assert!(!app.overlay_open());
        assert!(app.search_active(), "the first Esc only closed the overlay");
        press(&mut app, KeyCode::Esc);
        assert!(!app.search_active());
    }
}

#[test]
fn esc_clears_the_search_before_the_focus() {
    let mut app = app_from(LINKS, 60, 20 + CHROME);
    press(&mut app, KeyCode::Tab);
    assert!(app.focused().is_some());
    ctrl_f(&mut app);
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Enter);
    question(&mut app);
    press(&mut app, KeyCode::Esc);
    assert!(app.search_active() && app.focused().is_some());
    press(&mut app, KeyCode::Esc);
    assert!(!app.search_active());
    assert!(app.focused().is_some(), "the focus outlives the search");
    press(&mut app, KeyCode::Esc);
    assert!(app.focused().is_none());
}

#[test]
fn q_closes_the_help_overlay_instead_of_quitting() {
    let mut app = app_from(LINKS, 60, 20 + CHROME);
    question(&mut app);
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.overlay_open());
    assert!(!app.should_quit());
    press(&mut app, KeyCode::Char('q'));
    assert!(app.should_quit());
}

#[test]
fn the_help_lists_q_as_the_quit_key_and_no_longer_lists_ctrl_q() {
    let app = app_from(LINKS, 60, 20 + CHROME);
    let lines = app.help_lines();
    assert!(
        lines
            .iter()
            .any(|l| l.trim_start().starts_with("q ") && l.contains("Quit")),
        "{lines:?}"
    );
    assert!(!help_text(&app).contains("Ctrl+Q"));
}

#[test]
fn question_mark_is_typed_as_text_while_the_search_input_is_open() {
    let mut app = app_from(LINKS, 60, 20 + CHROME);
    ctrl_f(&mut app);
    question(&mut app);
    assert!(!app.overlay_open());
    assert!(app.text_input_open());
}

#[test]
fn up_down_and_pages_scroll_a_help_taller_than_the_pane() {
    let mut app = app_from(LINKS, 60, 8 + CHROME);
    question(&mut app);
    let first = screen_text(&mut app, 60, 8 + CHROME);
    assert!(first.contains("Global"));
    for _ in 0..40 {
        press(&mut app, KeyCode::Down);
    }
    let last = screen_text(&mut app, 60, 8 + CHROME);
    assert_ne!(first, last);
    assert!(last.contains("Contents") || last.contains("Esc"), "{last}");
    press(&mut app, KeyCode::PageUp);
    press(&mut app, KeyCode::Up);
    for _ in 0..80 {
        press(&mut app, KeyCode::Up);
    }
    assert_eq!(screen_text(&mut app, 60, 8 + CHROME), first);
}

#[test]
fn keys_and_clicks_do_not_reach_the_document_while_the_help_is_open() {
    let mut app = app_from(&many(40), 60, 10 + CHROME);
    question(&mut app);
    press(&mut app, KeyCode::Char('s'));
    wheel(&mut app, MouseEventKind::ScrollDown);
    assert_eq!(app.scroll(), 0);
}

#[test]
fn snapshot_of_the_help_overlay() {
    let mut app = app_from(LINKS, 70, 30 + CHROME);
    question(&mut app);
    let mut terminal = Terminal::new(TestBackend::new(70, 30 + CHROME)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    insta::assert_snapshot!(terminal.backend());
}
