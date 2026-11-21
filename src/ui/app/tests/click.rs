use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::*;
use crate::layout::{Focusable, MARGIN};
use crate::ui::opener::Opener;
use crate::ui::pictures::Clock;

const W: u16 = 70;
const H: u16 = 10 + CHROME;
const DIAGRAM: &str = "```mermaid\ngraph TD\n A-->B\n```\n";

#[derive(Debug, Clone, Default)]
struct Fake(Rc<RefCell<Vec<String>>>);

impl Opener for Fake {
    fn open(&self, url: &str) -> Result<(), String> {
        self.0.borrow_mut().push(url.to_owned());
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
struct FakeClock(Arc<Mutex<Duration>>);

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        *self.0.lock().unwrap()
    }
}

impl FakeClock {
    fn advance(&self, ms: u64) {
        *self.0.lock().unwrap() += Duration::from_millis(ms);
    }
}

fn setup(md: &str, width: u16) -> (App, Rc<RefCell<Vec<String>>>, FakeClock) {
    let fake = Fake::default();
    let urls = fake.0.clone();
    let clock = FakeClock::default();
    let mut app = app_from(md, width, H);
    app.set_opener(Box::new(fake));
    app.set_clock(Box::new(clock.clone()));
    (app, urls, clock)
}

fn click(app: &mut App, x: u16, y: u16) {
    app.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    });
}

/// Screen cell of content column `col` on layout row `row`, given the
/// document pane's left edge (border included).
fn cell(app: &App, pane_x: u16, row: usize, col: u16) -> (u16, u16) {
    let y = 1 + u16::try_from(row - app.scroll()).unwrap();
    (pane_x + 1 + MARGIN + col, y)
}

fn link_fragments(app: &App, n: usize) -> Vec<crate::layout::Fragment> {
    match &app.layout.index.focusables[n] {
        Focusable::Link { fragments, .. } => fragments.clone(),
        other => panic!("not a link: {other:?}"),
    }
}

fn diagram_cell(app: &App) -> (u16, u16) {
    let Focusable::Diagram { rows, .. } = &app.layout.index.focusables[0] else {
        panic!("no diagram");
    };
    let slot = app.layout.rows[rows.start].slot.as_ref().unwrap();
    cell(app, 0, rows.start, slot.left)
}

#[test]
fn a_click_on_a_link_fragment_focuses_the_link_and_activates_it_like_enter() {
    let (mut app, urls, _) = setup("[l](http://a.b)\n", W);
    let f = link_fragments(&app, 0)[0];
    let (x, y) = cell(&app, 0, f.row, f.start);
    click(&mut app, x, y);
    assert!(matches!(app.focused(), Some(Focusable::Link { .. })));
    assert_eq!(*urls.borrow(), vec!["http://a.b".to_owned()]);
    assert!(screen_text(&mut app, W, H).contains("Opened in browser"));
}

#[test]
fn a_click_on_any_wrapped_fragment_of_a_link_hits_the_same_link() {
    let md = "x [aaa bbb ccc ddd eee fff ggg hhh iii jjj kkk lll](http://a.b) y\n";
    let (mut app, urls, _) = setup(md, 32);
    let fragments = link_fragments(&app, 0);
    assert!(fragments.len() >= 2, "{fragments:?}");
    for f in &fragments {
        urls.borrow_mut().clear();
        app.focus = None;
        let (x, y) = cell(&app, 0, f.row, f.end - 1);
        click(&mut app, x, y);
        assert_eq!(*urls.borrow(), vec!["http://a.b".to_owned()], "{f:?}");
        assert!(matches!(app.focused(), Some(Focusable::Link { .. })));
    }
}

#[test]
fn a_click_on_a_diagram_focuses_it_without_opening_it() {
    let (mut app, _, _) = setup(DIAGRAM, W);
    let (x, y) = diagram_cell(&app);
    click(&mut app, x, y);
    assert!(matches!(app.focused(), Some(Focusable::Diagram { .. })));
    assert!(app.fullscreen_request().is_none());
    assert!(!app.view_open());
}

#[test]
fn two_clicks_on_the_same_diagram_within_400_ms_open_the_full_screen_view_and_slower_ones_do_not() {
    let (mut app, _, clock) = setup(DIAGRAM, W);
    let (x, y) = diagram_cell(&app);
    click(&mut app, x, y);
    clock.advance(500);
    click(&mut app, x, y);
    assert!(app.fullscreen_request().is_none(), "too slow");
    clock.advance(300);
    click(&mut app, x, y);
    assert!(app.fullscreen_request().is_some(), "within 400 ms");
}

#[test]
fn clicks_on_plain_text_margins_or_the_sidebar_border_change_nothing() {
    let (mut app, urls, _) = setup("plain text [l](http://a.b)\n", W);
    let f = link_fragments(&app, 0)[0];
    let (x, y) = cell(&app, 0, f.row, f.start);
    click(&mut app, x, y);
    let focus = app.focus.clone();
    assert!(focus.is_some());
    assert_eq!(urls.borrow().len(), 1);
    // plain text, left margin, border, past the end, status bar, below the text
    for (cx, cy) in [
        (3, y),
        (1, y),
        (0, y),
        (W - 1, y),
        (x, 0),
        (x, H - 1),
        (x, y + 4),
    ] {
        click(&mut app, cx, cy);
        assert_eq!(app.focus, focus, "({cx}, {cy})");
        assert_eq!(urls.borrow().len(), 1, "({cx}, {cy})");
        assert!(app.fullscreen_request().is_none());
    }
}

#[test]
fn hit_testing_accounts_for_scroll_position_and_sidebar_width() {
    let md = format!("{}\n\n[far](http://a.b)\n", many(30));
    let (mut app, urls, _) = setup(&md, W);
    press(&mut app, KeyCode::Char('s'));
    press(&mut app, KeyCode::End);
    assert!(app.scroll() > 0);
    let f = link_fragments(&app, 0)[0];
    // Sidebar is 30 columns wide: the document border is at column 30.
    let (x, y) = cell(&app, 30, f.row, f.start);
    assert_eq!(x, 32);
    click(&mut app, x - 30, y);
    assert!(urls.borrow().is_empty(), "ignoring the sidebar offset hits");
    let unscrolled = y + u16::try_from(app.scroll()).unwrap();
    click(&mut app, x, unscrolled);
    assert!(urls.borrow().is_empty(), "ignoring the scroll offset hits");
    click(&mut app, x, y);
    assert_eq!(*urls.borrow(), vec!["http://a.b".to_owned()]);
}
