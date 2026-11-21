use std::{cell::RefCell, rc::Rc};

use crossterm::event::KeyCode;
use image::RgbaImage;

use super::*;
use crate::layout::{Focusable, PictureKey};
use crate::render_worker::{Outcome, RenderResult};
use crate::ui::view_worker::{Reply, Request, ViewSink};

const DIAGRAM: &str = "```mermaid\ngraph TD\n A-->B\n```\n";
/// Terminal of 80x24 with 10x20 px cells: the view area is 800x460 px.
const W: u16 = 80;
const H: u16 = 24;

#[derive(Debug, Clone, Default)]
struct Fake(Rc<RefCell<Vec<Request>>>);

impl ViewSink for Fake {
    fn submit(&self, request: Request) {
        self.0.borrow_mut().push(request);
    }
}

type Requests = Rc<RefCell<Vec<Request>>>;

fn setup(md: &str) -> (App, Requests) {
    let mut app = app_from(md, W, H);
    let fake = Fake::default();
    let requests = fake.0.clone();
    app.set_view_sink(Box::new(fake));
    (app, requests)
}

/// Focus the first diagram and press Enter; the event loop then ticks.
fn enter(app: &mut App) {
    press(app, KeyCode::Tab);
    press(app, KeyCode::Enter);
    app.tick();
}

fn last(requests: &Requests) -> Request {
    let requests = requests.borrow();
    assert!(!requests.is_empty(), "no render request was submitted");
    requests[requests.len() - 1].clone()
}

fn reply_natural(app: &mut App, requests: &Requests, size: (u32, u32)) {
    let generation = last(requests).generation;
    app.on_view_reply(Reply::Natural { generation, size });
}

fn reply_image(app: &mut App, requests: &Requests) {
    let request = last(requests);
    let region = request.region;
    assert!(region.is_some(), "the last request has no region");
    let px = region.map_or((1, 1), |r| r.px);
    app.on_view_reply(Reply::Image {
        generation: request.generation,
        image: std::sync::Arc::new(RgbaImage::new(px.0, px.1)),
    });
}

/// Open the view on a diagram of `natural` size with its first raster in place.
fn open(natural: (u32, u32)) -> (App, Requests) {
    let (mut app, requests) = setup(DIAGRAM);
    enter(&mut app);
    reply_natural(&mut app, &requests, natural);
    reply_image(&mut app, &requests);
    (app, requests)
}

fn marks(app: &mut App) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (y, row) in screen(app, W, H).iter().enumerate() {
        for (x, symbol) in row.chars().enumerate() {
            if symbol == '#' {
                found.push((x, y));
            }
        }
    }
    found
}

#[test]
fn opening_the_view_rasterises_the_diagram_to_fit_the_screen_minus_the_status_bar_centred() {
    let (mut app, requests) = open((1600, 1000));
    assert!(app.view_open());
    assert!(app.fullscreen_request().is_none());
    let region = requests.borrow()[1].region.unwrap();
    // 800x460 px area: scale 0.46 fits the height, 736 px wide.
    assert!((region.scale - 0.46).abs() < 1e-9);
    assert_eq!(region.px, (736, 460));
    let marks = marks(&mut app);
    let (xs, ys): (Vec<_>, Vec<_>) = marks.iter().copied().unzip();
    // 74 columns centred in 80, 23 rows above the status bar.
    assert_eq!(xs.iter().min(), Some(&3));
    assert_eq!(xs.iter().max(), Some(&76));
    assert_eq!(ys.iter().min(), Some(&0));
    assert_eq!(ys.iter().max(), Some(&22));
}

#[test]
fn zooming_re_rasterises_from_the_svg_at_the_new_scale_instead_of_upscaling_pixels() {
    let (mut app, requests) = open((1600, 1000));
    let before = last(&requests);
    press(&mut app, KeyCode::Char('+'));
    let after = last(&requests);
    assert!(after.generation > before.generation);
    // The request names the diagram source: the worker draws from the SVG.
    assert!(after.source.contains("graph TD"));
    let region = after.region.unwrap();
    assert!((region.scale - 0.46 * 1.25).abs() < 1e-9);
}

#[test]
fn at_maximum_zoom_only_the_visible_region_is_rasterised() {
    let (mut app, requests) = open((1600, 1000));
    for _ in 0..15 {
        press(&mut app, KeyCode::Char('='));
    }
    let region = last(&requests).region.unwrap();
    assert!((region.scale - 0.46 * 8.0).abs() < 1e-9);
    // Never more than the screen, while the whole picture would be 5888x3680.
    assert!(region.px.0 <= 800 && region.px.1 <= 460);
    press(&mut app, KeyCode::Right);
    let panned = last(&requests).region.unwrap();
    assert!(panned.origin.0 > region.origin.0);
    assert!(panned.px.0 <= 800 && panned.px.1 <= 460);
}

#[test]
fn the_status_bar_shows_the_zoom_percentage() {
    let (mut app, _) = open((1600, 1000));
    assert!(screen(&mut app, W, H)[23].contains("Zoom 100%"));
    press(&mut app, KeyCode::Char('+'));
    assert!(screen(&mut app, W, H)[23].contains("Zoom 125%"));
    press(&mut app, KeyCode::Char('0'));
    assert!(screen(&mut app, W, H)[23].contains("Zoom 100%"));
}

#[test]
fn esc_returns_to_the_document_with_the_same_scroll_position_and_focus() {
    let md = format!("{}\n{DIAGRAM}\n{}", many(30), many(30));
    let (mut app, requests) = setup(&md);
    press(&mut app, KeyCode::Tab);
    let scroll = app.scroll();
    assert!(scroll > 0);
    press(&mut app, KeyCode::Enter);
    app.tick();
    reply_natural(&mut app, &requests, (400, 200));
    press(&mut app, KeyCode::Char('+'));
    press(&mut app, KeyCode::Esc);
    assert!(!app.view_open());
    assert_eq!(app.scroll(), scroll);
    assert!(matches!(app.focused(), Some(Focusable::Diagram { .. })));
    let text = screen_text(&mut app, W, H);
    assert!(text.contains("para"), "document is back:\n{text}");
    assert!(!text.contains("Zoom"));
}

#[test]
fn q_leaves_the_full_screen_view_instead_of_quitting() {
    let (mut app, requests) = setup(DIAGRAM);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Enter);
    app.tick();
    reply_natural(&mut app, &requests, (400, 200));
    assert!(app.view_open());
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.view_open());
    assert!(!app.should_quit());
}

#[test]
fn in_text_mode_the_view_shows_the_text_output_scrollable_with_the_arrow_keys() {
    let (mut app, requests) = setup(DIAGRAM);
    app.set_text_mode(true);
    let text: String = (0..60).map(|i| format!("line {i}\n")).collect();
    app.on_result(RenderResult {
        key: PictureKey::Diagram(0),
        generation: 1,
        width: 0,
        height: 0,
        outcome: Outcome::Text(text),
    });
    enter(&mut app);
    assert!(app.view_open());
    assert!(requests.borrow().is_empty(), "text mode renders no picture");
    let first = screen(&mut app, W, H);
    assert!(first[0].contains("line 0"));
    press(&mut app, KeyCode::Down);
    let second = screen(&mut app, W, H);
    assert!(second[0].contains("line 1") && !second.join("\n").contains("line 0\n"));
    press(&mut app, KeyCode::Up);
    assert!(screen(&mut app, W, H)[0].contains("line 0"));
}

#[test]
fn snapshot_of_the_full_screen_view_with_the_placeholder_renderer() {
    let (mut app, _) = open((400, 200));
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    for y in 0..H {
        for x in 0..W {
            assert_eq!(buffer[(x, y)].bg, theme::BG, "cell {x},{y} has no theme bg");
        }
    }
    insta::assert_snapshot!(terminal.backend());
}
