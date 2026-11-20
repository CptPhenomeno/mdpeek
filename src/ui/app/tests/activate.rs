use std::{cell::RefCell, rc::Rc};

use crossterm::event::KeyCode;

use super::*;
use crate::layout::PictureKey;
use crate::ui::opener::Opener;

#[derive(Debug, Clone, Default)]
struct Fake {
    urls: Rc<RefCell<Vec<String>>>,
    error: Option<String>,
}

impl Opener for Fake {
    fn open(&self, url: &str) -> Result<(), String> {
        self.urls.borrow_mut().push(url.to_owned());
        self.error.clone().map_or(Ok(()), Err)
    }
}

fn with_fake(md: &str, error: Option<&str>) -> (App, Rc<RefCell<Vec<String>>>) {
    let fake = Fake {
        error: error.map(str::to_owned),
        ..Fake::default()
    };
    let urls = fake.urls.clone();
    let mut app = app_from(md, 70, 10 + CHROME);
    app.set_opener(Box::new(fake));
    (app, urls)
}

fn enter_on_first(app: &mut App) -> String {
    press(app, KeyCode::Tab);
    press(app, KeyCode::Enter);
    screen_text(app, 70, 10 + CHROME)
}

#[test]
fn enter_on_an_http_https_or_mailto_link_calls_the_opener_and_shows_opened_in_browser() {
    for url in ["http://a.b", "https://a.b/x", "mailto:me@a.b"] {
        let (mut app, urls) = with_fake(&format!("[l]({url})\n"), None);
        let text = enter_on_first(&mut app);
        assert_eq!(*urls.borrow(), vec![url.to_owned()]);
        assert!(text.contains("Opened in browser"), "{text}");
    }
}

#[test]
fn an_opener_failure_shows_the_error_in_the_status_bar() {
    let (mut app, urls) = with_fake("[l](http://a.b)\n", Some("no handler"));
    let text = enter_on_first(&mut app);
    assert_eq!(urls.borrow().len(), 1);
    assert!(text.contains("no handler"), "{text}");
    assert!(!text.contains("Opened in browser"), "{text}");
}

#[test]
fn enter_on_an_anchor_link_scrolls_the_matching_heading_to_the_top() {
    let md = format!(
        "[go](#target)\n\n{}\n\n## Target\n\nbody\n\n{}",
        many(30),
        many(30)
    );
    let (mut app, urls) = with_fake(&md, None);
    enter_on_first(&mut app);
    let row = app
        .layout
        .index
        .headings
        .iter()
        .find(|h| h.slug == "target")
        .unwrap()
        .row;
    assert!(row > 10);
    assert_eq!(app.scroll(), row);
    assert!(urls.borrow().is_empty());
}

#[test]
fn an_anchor_without_a_matching_heading_shows_anchor_not_found() {
    let (mut app, _) = with_fake("[go](#nope)\n\n## Other\n", None);
    let text = enter_on_first(&mut app);
    assert!(text.contains("Anchor not found"), "{text}");
    assert_eq!(app.scroll(), 0);
}

#[test]
fn enter_on_a_local_file_link_or_other_scheme_shows_local_file_links_are_not_supported() {
    for target in ["other.md", "./a/b.md#x", "ftp://h/f"] {
        let (mut app, urls) = with_fake(&format!("[l]({target})\n"), None);
        let text = enter_on_first(&mut app);
        assert!(
            text.contains("Local file links are not supported"),
            "{target}: {text}"
        );
        assert!(urls.borrow().is_empty());
    }
}

#[test]
fn enter_on_a_focused_diagram_requests_the_full_screen_view() {
    let md = "before\n\n```mermaid\ngraph TD\n A-->B\n```\n\nafter\n";
    let (mut app, urls) = with_fake(md, None);
    press(&mut app, KeyCode::Tab);
    assert!(app.fullscreen_request().is_none());
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.fullscreen_request(), Some(&PictureKey::Diagram(0)));
    assert!(urls.borrow().is_empty());
}

#[test]
fn enter_with_nothing_focused_does_nothing() {
    let (mut app, urls) = with_fake("[l](http://a.b)\n\n[m](#x)\n", None);
    press(&mut app, KeyCode::Enter);
    assert!(urls.borrow().is_empty());
    assert!(app.fullscreen_request().is_none());
    assert_eq!(app.scroll(), 0);
    let text = screen_text(&mut app, 70, 10 + CHROME);
    assert!(
        !text.contains("Opened") && !text.contains("not found") && !text.contains("not supported")
    );
}
