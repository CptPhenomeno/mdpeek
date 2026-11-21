use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};

use super::*;

/// Seven sections of three one-line paragraphs: about 10 rows each.
fn sections() -> String {
    (0..7)
        .map(|i| format!("## S{i}\n\none\n\ntwo\n\nthree"))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn press_t(app: &mut App) {
    press(app, KeyCode::Char('t'));
}

fn heading_row(app: &App, n: usize) -> usize {
    app.layout.index.headings[n].row
}

#[test]
fn t_opens_an_overlay_listing_all_headings_indented_two_columns_per_level_below_h1() {
    let mut app = app_from(
        "# One\n\ntext\n\n## Two\n\n### Three\n\n#### Four\n",
        50,
        14 + CHROME,
    );
    assert!(!app.overlay_open());
    press_t(&mut app);
    assert!(app.overlay_open());
    assert_eq!(app.toc_lines(), ["One", "  Two", "    Three", "      Four"]);
}

#[test]
fn the_heading_nearest_above_the_current_scroll_position_is_pre_selected() {
    let mut app = app_from(&sections(), 50, 8 + CHROME);
    press_t(&mut app);
    assert_eq!(app.toc_selected(), Some(0));
    press(&mut app, KeyCode::Esc);
    app.scroll = heading_row(&app, 3) + 2;
    press_t(&mut app);
    assert_eq!(app.toc_selected(), Some(3));
    press(&mut app, KeyCode::Esc);
    app.scroll = heading_row(&app, 3);
    press_t(&mut app);
    assert_eq!(app.toc_selected(), Some(3));
}

#[test]
fn up_and_down_move_the_selection() {
    let mut app = app_from(&sections(), 50, 8 + CHROME);
    press_t(&mut app);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.toc_selected(), Some(0));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.toc_selected(), Some(2));
    press(&mut app, KeyCode::Up);
    assert_eq!(app.toc_selected(), Some(1));
    for _ in 0..20 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.toc_selected(), Some(6));
    assert_eq!(app.scroll(), 0, "selecting does not scroll the document");
}

#[test]
fn enter_scrolls_the_selected_heading_to_the_top_of_the_pane_and_closes_the_overlay() {
    let mut app = app_from(&sections(), 50, 8 + CHROME);
    press_t(&mut app);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert!(!app.overlay_open());
    assert_eq!(app.scroll(), heading_row(&app, 2));
    // The last heading cannot reach the top: clamped like an anchor.
    press_t(&mut app);
    for _ in 0..10 {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.scroll(), app.max_scroll());
    assert!(heading_row(&app, 6) > app.max_scroll());
}

#[test]
fn esc_closes_the_overlay_without_scrolling() {
    let mut app = app_from(&sections(), 50, 8 + CHROME);
    app.scroll = 5;
    press_t(&mut app);
    assert!(app.overlay_open());
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Esc);
    assert!(!app.overlay_open());
    assert_eq!(app.scroll(), 5);
}

#[test]
fn snapshot_of_the_toc_overlay_over_a_document() {
    let md =
        "# Title\n\nIntro text.\n\n## Section\n\nBody.\n\n### Detail\n\nMore.\n\n## Last\n\nEnd.\n";
    let mut app = app_from(md, 40, 14 + CHROME);
    press_t(&mut app);
    press(&mut app, KeyCode::Down);
    let mut terminal = Terminal::new(TestBackend::new(40, 14 + CHROME)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    insta::assert_snapshot!(terminal.backend());
}

#[test]
fn q_closes_the_toc_overlay_instead_of_quitting() {
    let mut app = app_from(&sections(), 50, 8 + CHROME);
    press_t(&mut app);
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.overlay_open());
    assert!(!app.should_quit());
    press(&mut app, KeyCode::Char('q'));
    assert!(app.should_quit());
}

#[test]
fn document_keys_and_clicks_are_blocked_while_the_overlay_is_open() {
    let mut app = app_from(&sections(), 50, 8 + CHROME);
    press_t(&mut app);
    assert!(app.overlay_open());
    press(&mut app, KeyCode::PageDown);
    press(&mut app, KeyCode::End);
    assert_eq!(app.scroll(), 0);
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    });
    assert!(app.clicks_blocked());
}

#[test]
fn t_opens_from_the_sidebar_but_not_while_the_search_input_is_open() {
    let mut app = app_from(&sections(), 50, 8 + CHROME);
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
    press_t(&mut app);
    assert!(app.overlay_open());
    press(&mut app, KeyCode::Esc);
    app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
    press_t(&mut app);
    assert!(!app.overlay_open());
    assert_eq!(app.search.query(), "t");
}

#[test]
fn a_document_without_headings_shows_a_no_headings_line() {
    let mut app = app_from("just text\n", 40, 10 + CHROME);
    press_t(&mut app);
    assert!(app.overlay_open());
    assert!(screen_text(&mut app, 40, 10 + CHROME).contains("No headings"));
    press(&mut app, KeyCode::Enter);
    assert!(!app.overlay_open());
}

#[test]
fn a_long_list_scrolls_inside_the_overlay_keeping_the_selection_visible() {
    let md: String = (0..30).map(|i| format!("## H{i:02}\n\n")).collect();
    let mut app = app_from(&md, 40, 8 + CHROME);
    press_t(&mut app);
    for _ in 0..25 {
        press(&mut app, KeyCode::Down);
    }
    let text = screen_text(&mut app, 40, 8 + CHROME);
    assert!(text.contains("H25"), "{text}");
    assert!(!text.contains("H00"), "{text}");
}
