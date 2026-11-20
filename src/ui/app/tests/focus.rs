use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, style::Modifier};

use super::*;
use crate::layout::{Focusable, PictureKey};

const LINKS: &str = "[one](http://a.b) and [two](#x)\n\n[three](http://c.d)\n";

fn target(app: &App) -> Option<String> {
    match app.focused() {
        Some(Focusable::Link { target, .. }) => Some(target.clone()),
        Some(Focusable::Diagram { key, .. }) => Some(format!("{key:?}")),
        None => None,
    }
}

fn shift_tab(app: &mut App) {
    app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
}

#[test]
fn focus_starts_on_nothing() {
    let mut app = app_from(LINKS, 40, 10 + CHROME);
    assert!(app.focused().is_none());
    let mut terminal = Terminal::new(TestBackend::new(40, 10 + CHROME)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    let buf = terminal.backend().buffer();
    assert!(
        buf.content()
            .iter()
            .all(|c| !c.modifier.contains(Modifier::REVERSED))
    );
}

#[test]
fn tab_focuses_the_next_focusable_in_document_order_and_shift_tab_the_previous() {
    let mut app = app_from(LINKS, 40, 10 + CHROME);
    press(&mut app, KeyCode::Tab);
    assert_eq!(target(&app).as_deref(), Some("http://a.b"));
    press(&mut app, KeyCode::Tab);
    assert_eq!(target(&app).as_deref(), Some("#x"));
    press(&mut app, KeyCode::Tab);
    assert_eq!(target(&app).as_deref(), Some("http://c.d"));
    shift_tab(&mut app);
    assert_eq!(target(&app).as_deref(), Some("#x"));
}

#[test]
fn tab_after_the_last_focusable_wraps_to_the_first_and_shift_tab_from_nothing_or_the_first_wraps_to_the_last()
 {
    let mut app = app_from(LINKS, 40, 10 + CHROME);
    for _ in 0..3 {
        press(&mut app, KeyCode::Tab);
    }
    press(&mut app, KeyCode::Tab);
    assert_eq!(target(&app).as_deref(), Some("http://a.b"));
    shift_tab(&mut app);
    assert_eq!(target(&app).as_deref(), Some("http://c.d"));
    press(&mut app, KeyCode::Esc);
    shift_tab(&mut app);
    assert_eq!(target(&app).as_deref(), Some("http://c.d"));
}

#[test]
fn focusing_an_item_outside_the_viewport_scrolls_it_into_view() {
    let md = format!(
        "[top](http://t.t)\n\n{}\n\n[bottom](http://b.b)\n",
        many(30)
    );
    let mut app = app_from(&md, 40, 10 + CHROME);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.scroll(), 0);
    shift_tab(&mut app);
    assert_eq!(target(&app).as_deref(), Some("http://b.b"));
    // Minimal scroll: the link is the last visible row.
    assert_eq!(app.scroll(), app.row_count() - 10);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.scroll(), 0);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    shift_tab(&mut app);
    assert_eq!(app.scroll(), 0, "no scroll when already visible");
}

#[test]
fn esc_clears_the_focus() {
    let mut app = app_from(LINKS, 40, 10 + CHROME);
    press(&mut app, KeyCode::Tab);
    assert!(app.focused().is_some());
    press(&mut app, KeyCode::Esc);
    assert!(app.focused().is_none());
}

#[test]
fn the_focused_link_is_drawn_in_reverse_video_across_all_its_wrapped_fragments() {
    let md = "intro [a long link text that wraps](http://x.y) outro\n\n[other](http://o.o)\n";
    let mut app = app_from(md, 24, 10 + CHROME);
    press(&mut app, KeyCode::Tab);
    let Some(Focusable::Link { fragments, .. }) = app.focused().cloned() else {
        panic!("a link must be focused");
    };
    assert!(fragments.len() >= 2, "{fragments:?}");
    let mut terminal = Terminal::new(TestBackend::new(24, 10 + CHROME)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    let buf = terminal.backend().buffer();
    let mut reversed = 0;
    for y in 0..10u16 {
        for x in 0..24u16 {
            let cell = &buf[(x, y + 1)];
            let inside = fragments
                .iter()
                .any(|f| f.row == y as usize && x >= f.start + 2 && x < f.end + 2);
            assert_eq!(
                cell.modifier.contains(Modifier::REVERSED),
                inside,
                "cell ({x},{y}) {:?}",
                cell.symbol()
            );
            reversed += usize::from(inside);
            assert_ne!(cell.bg, ratatui::style::Color::Reset);
        }
    }
    assert!(reversed > 0);
}

#[test]
fn focus_stays_on_the_same_item_after_a_relayout() {
    let md = "[a long first link that wraps](http://one) text\n\n[second](http://two)\n";
    let mut app = app_from(md, 40, 10 + CHROME);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    assert_eq!(target(&app).as_deref(), Some("http://two"));
    app.resize(22, 10 + CHROME);
    assert_eq!(target(&app).as_deref(), Some("http://two"));
    app.set_text_mode(true);
    assert_eq!(target(&app).as_deref(), Some("http://two"));
    app.set_cell((8, 16));
    assert_eq!(target(&app).as_deref(), Some("http://two"));
    // Same-target links are told apart by their position.
    let mut app = app_from("[x](http://s) [y](http://s)\n", 40, 10 + CHROME);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    app.resize(30, 10 + CHROME);
    press(&mut app, KeyCode::Tab);
    let Some(Focusable::Link { fragments, .. }) = app.focused() else {
        panic!()
    };
    assert_eq!(fragments[0].start, 0, "wrapped to the first of two");
}

#[test]
fn a_focused_diagram_gets_a_highlighted_frame_around_its_slot() {
    let md = "before\n\n```mermaid\ngraph TD\n A-->B\n```\n\nafter\n";
    let mut app = app_from(md, 30, 14 + CHROME);
    app.add_picture(
        PictureKey::Diagram(0),
        image::DynamicImage::new_rgba8(100, 60),
    );
    press(&mut app, KeyCode::Tab);
    assert!(matches!(app.focused(), Some(Focusable::Diagram { .. })));
    let mut terminal = Terminal::new(TestBackend::new(30, 14 + CHROME)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    insta::assert_snapshot!(terminal.backend());
}
