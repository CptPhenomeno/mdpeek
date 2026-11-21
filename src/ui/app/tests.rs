use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};

use super::App;
use crate::doc::{Document, Source};
use crate::theme;
use ratatui::buffer::Buffer;

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

/// Interior rows of the document pane for a terminal with a `w` x `h` pane
/// (two border columns, two border rows and the status bar around it).
fn narrow_pane(w: u16, h: u16) -> (Vec<String>, Buffer) {
    let (tw, th) = (w + 2, h + CHROME);
    let mut app = app_from("hello", tw, th);
    let mut terminal = Terminal::new(TestBackend::new(tw, th)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let rows = (1..=h)
        .map(|y| {
            (1..=w)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect()
        })
        .collect();
    (rows, buffer)
}

#[test]
fn narrow_message_is_fully_readable_in_a_pane_of_16_columns() {
    let (rows, _) = narrow_pane(16, 8);
    let text = rows.join("\n");
    assert!(text.contains("Window"), "{text}");
    assert!(text.contains("narrow"), "{text}");
    assert!(text.contains("too"), "{text}");
}

#[test]
fn narrow_message_never_clips_a_word_at_any_pane_size() {
    let full: &[&str] = &["Window", "too", "narrow"];
    let shorts: [&[&str]; 3] = [&["Too", "narrow"], &["Narrow"], &["…"]];
    for w in 1..=20u16 {
        for h in 1..=5u16 {
            let (rows, buffer) = narrow_pane(w, h);
            let words: Vec<&str> = rows.iter().flat_map(|r| r.split_whitespace()).collect();
            if w >= crate::layout::MIN_WIDTH {
                continue; // wide enough for the document
            }
            assert!(
                words == full || shorts.iter().any(|s| words == *s),
                "{w}x{h}: {rows:?}"
            );
            // Nothing outside the pane, and every cell keeps the opaque theme.
            let area = buffer.area;
            for y in 0..area.height {
                for x in 0..area.width {
                    let cell = &buffer[(x, y)];
                    let inside = (1..=w).contains(&x) && (1..=h).contains(&y);
                    if !inside && y < h + 2 {
                        assert!(
                            !cell.symbol().chars().any(char::is_alphabetic),
                            "{w}x{h}: text outside the pane at {x},{y}"
                        );
                    }
                    assert_ne!(cell.bg, ratatui::style::Color::Reset, "{w}x{h} {x},{y}");
                }
            }
        }
    }
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
fn q_quits_from_document_focus() {
    let mut app = app_from("hi", 40, 10);
    assert!(!app.should_quit());
    press(&mut app, KeyCode::Char('q'));
    assert!(app.should_quit());
}

#[test]
fn ctrl_q_no_longer_quits() {
    let mut app = app_from("hi", 40, 10);
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
    assert!(!app.should_quit());
    app.handle_key(KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT));
    assert!(!app.should_quit());
}

#[test]
fn snapshot_of_a_short_document_with_headings_paragraphs_and_inline_styles() {
    let md = "# Title\n\nSome *emphasis*, **strong** and `code` text.\n\n## Section\n\nA [link](https://example.com) and ~~gone~~.\n";
    let mut app = app_from(md, 40, 14);
    let mut terminal = Terminal::new(TestBackend::new(40, 14)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    insta::assert_snapshot!(terminal.backend());
}

fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    terminal.backend().buffer().clone()
}

fn assert_all_bg(buf: &Buffer) {
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            assert_eq!(buf[(x, y)].bg, theme::BG, "cell ({x}, {y})");
        }
    }
}

const RICH: &str = "# Title\n\nbody `code` [link](http://x.y) *em* **b**\n\n> quote\n\n```rust\nlet a = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n- item\n";

#[test]
fn no_cell_of_a_rendered_frame_has_the_default_background() {
    let mut app = app_from(RICH, 50, 24);
    let buf = buffer(&mut app, 50, 24);
    for cell in buf.content() {
        assert_ne!(cell.bg, ratatui::style::Color::Reset);
    }
}

#[test]
fn empty_area_below_a_short_document_has_the_theme_background() {
    let mut app = app_from("hi", 40, 20);
    let buf = buffer(&mut app, 40, 20);
    for y in 5..17 {
        for x in 1..39 {
            assert_eq!(buf[(x, y)].bg, theme::BG, "cell ({x}, {y})");
        }
    }
}

#[test]
fn status_bar_and_pane_border_use_the_theme_background() {
    let mut app = app_from("hi", 40, 10);
    let buf = buffer(&mut app, 40, 10);
    for x in 0..40 {
        assert_eq!(buf[(x, 9)].bg, theme::BG, "status ({x})");
        assert_eq!(buf[(x, 0)].bg, theme::BG, "top border ({x})");
        assert_eq!(buf[(x, 8)].bg, theme::BG, "bottom border ({x})");
    }
    for y in 0..10 {
        assert_eq!(buf[(0, y)].bg, theme::BG);
        assert_eq!(buf[(39, y)].bg, theme::BG);
    }
}

#[test]
fn body_text_spans_get_the_theme_foreground() {
    let mut app = app_from("hello", 40, 10);
    let buf = buffer(&mut app, 40, 10);
    // border (1) + margin, then the text.
    let cell = (0..40)
        .map(|x| &buf[(x, 1)])
        .find(|c| c.symbol() == "h")
        .unwrap();
    assert_eq!(cell.fg, theme::FG);
    assert_eq!(cell.bg, theme::BG);
}

#[test]
fn too_narrow_and_empty_screens_have_the_theme_background() {
    let mut narrow = app_from("text", 21, 10);
    assert_all_bg(&buffer(&mut narrow, 21, 10));
    let mut empty = app_from("", 40, 10);
    assert_all_bg(&buffer(&mut empty, 40, 10));
}

// Picture clipping (KB-39), with the placeholder renderer.

use crate::layout::PictureKey;
use ratatui::layout::Rect;

const PIC_MD_PRE: usize = 8;

/// Eight paragraphs, a diagram, then more text; the diagram's slot is 4x4 cells.
fn pic_app(width: u16, height: u16) -> App {
    let md = format!(
        "{}\n\n```mermaid\ngraph TD\nA-->B\n```\n\n{}",
        many(PIC_MD_PRE),
        many(20)
    );
    let mut app = app_from(&md, width, height);
    app.add_picture(
        PictureKey::Diagram(0),
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(40, 80)),
    );
    app.resize(width, height);
    app
}

/// (first row index, screen x of the slot's left edge) of the only slot.
fn slot_at(app: &App) -> (usize, u16) {
    let (i, row) = app
        .layout
        .rows
        .iter()
        .enumerate()
        .find(|(_, r)| r.slot.is_some())
        .expect("a slot");
    let slot = row.slot.as_ref().unwrap();
    assert_eq!((slot.cols, slot.rows, slot.known), (4, 4, true));
    (i, 1 + crate::layout::MARGIN + slot.left)
}

fn hashes(buf: &Buffer) -> Vec<(u16, u16)> {
    let mut v = Vec::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            if buf[(x, y)].symbol() == "#" {
                v.push((x, y));
            }
        }
    }
    v
}

fn rect_cells(x: u16, y: u16, w: u16, h: u16) -> Vec<(u16, u16)> {
    let mut v = Vec::new();
    for yy in y..y + h {
        for xx in x..x + w {
            v.push((xx, yy));
        }
    }
    v
}

#[test]
fn slot_partly_above_the_viewport_draws_only_its_visible_lower_part() {
    let mut app = pic_app(40, 10 + CHROME);
    let (idx, x) = slot_at(&app);
    app.scroll = idx + 2;
    let buf = buffer(&mut app, 40, 10 + CHROME);
    assert_eq!(hashes(&buf), rect_cells(x, 1, 4, 2));
}

#[test]
fn slot_partly_below_the_viewport_draws_only_its_visible_upper_part() {
    let mut app = pic_app(40, 6 + CHROME);
    let (idx, x) = slot_at(&app);
    app.scroll = idx - 4;
    let buf = buffer(&mut app, 40, 6 + CHROME);
    assert_eq!(hashes(&buf), rect_cells(x, 5, 4, 2));
}

#[test]
fn picture_never_writes_cells_outside_the_document_pane() {
    let (w, h) = (40, 6 + CHROME);
    let mut app = pic_app(w, h);
    let (idx, _) = slot_at(&app);
    let mut seen = 0;
    for scroll in 0..app.max_scroll() {
        app.scroll = scroll;
        let buf = buffer(&mut app, w, h);
        let cells = hashes(&buf);
        let visible = scroll < idx + 4 && idx < scroll + 6;
        assert_eq!(!cells.is_empty(), visible, "scroll {scroll}");
        seen += usize::from(visible);
        for (x, y) in cells {
            assert!(
                (1..w - 1).contains(&x) && (1..1 + 6).contains(&y),
                "{x},{y}"
            );
        }
    }
    assert!(seen > 0);
}

#[test]
fn an_overlay_covering_a_picture_hides_the_covered_part() {
    let mut app = pic_app(40, 10 + CHROME);
    let (idx, x) = slot_at(&app);
    app.scroll = idx;
    let overlay = Rect::new(x + 2, 2, 5, 2);
    app.set_overlays(vec![overlay]);
    let buf = buffer(&mut app, 40, 10 + CHROME);
    // Exact per cell with the placeholder renderer. Sixel and iTerm2 hide the
    // whole rows an overlay crosses instead (see graphics::clear_row_bands).
    let expected: Vec<(u16, u16)> = rect_cells(x, 1, 4, 4)
        .into_iter()
        .filter(|&(cx, cy)| !overlay.contains((cx, cy).into()))
        .collect();
    assert_eq!(hashes(&buf), expected);
}

#[test]
fn a_picture_scrolled_out_of_view_is_cleared() {
    let (w, h) = (40, 10 + CHROME);
    let mut app = pic_app(w, h);
    let (idx, x) = slot_at(&app);
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    app.scroll = idx;
    terminal.draw(|f| app.render(f)).unwrap();
    assert_eq!(hashes(terminal.backend().buffer()), rect_cells(x, 1, 4, 4));
    app.scroll = idx + 4;
    terminal.draw(|f| app.render(f)).unwrap();
    assert!(hashes(terminal.backend().buffer()).is_empty());
}

#[test]
fn snapshot_of_a_picture_scrolled_half_out_of_the_top_with_the_placeholder_renderer() {
    let mut app = pic_app(30, 8 + CHROME);
    let (idx, _) = slot_at(&app);
    app.scroll = idx + 2;
    let mut terminal = Terminal::new(TestBackend::new(30, 8 + CHROME)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    insta::assert_snapshot!(terminal.backend());
}

#[test]
fn no_cell_has_the_default_background_with_a_picture_and_an_overlay_crossing_it() {
    use ratatui::style::Color;
    use ratatui_image::picker::{Picker, ProtocolType};
    let (w, h) = (40, 10 + CHROME);
    let mut app = pic_app(w, h);
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Iterm2);
    let mut set = crate::graphics::PictureSet::sliced(picker, image::Rgba([26, 27, 38, 255]));
    set.insert(
        PictureKey::Diagram(0),
        image::DynamicImage::ImageRgba8(image::RgbaImage::new(40, 80)),
    );
    app.set_pictures(set);
    let (idx, x) = slot_at(&app);
    app.scroll = idx;
    let popup = Rect::new(x + 1, 2, 10, 4);
    app.set_overlays(vec![popup]);
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    // The frame buffer, not the backend: cells inside an image are skipped by the diff.
    let frame = terminal
        .draw(|f| {
            app.render(f);
            crate::ui::overlay::frame(f, popup, "Popup");
        })
        .unwrap();
    let buf = frame.buffer;
    for y in 0..h {
        for x in 0..w {
            assert_ne!(
                buf[(x, y)].bg,
                Color::Reset,
                "cell ({x}, {y}) {:?}",
                buf[(x, y)]
            );
        }
    }
}

// ---- pictures wired to the render worker (KB-40) ----

mod pictures {
    use std::sync::{Arc, Mutex};

    use image::RgbaImage;

    use super::*;
    use crate::layout::PictureKey;
    use crate::render_worker::{Job, Mode, Outcome, RenderResult};
    use crate::ui::JobSink;

    type Jobs = Arc<Mutex<Vec<Job>>>;

    /// Records every submitted job; results are fed back by the tests.
    #[derive(Debug)]
    struct FakeSink(Jobs);

    impl JobSink for FakeSink {
        fn submit(&self, job: Job) -> bool {
            self.0.lock().unwrap().push(job);
            true
        }
        fn set_generation(&self, _: u64) {}
    }

    fn pic_doc_app(md: &str, width: u16, height: u16) -> (App, Jobs) {
        let jobs = Jobs::default();
        let mut app = App::new(doc(md), "a.md".into());
        app.set_job_sink(Box::new(FakeSink(jobs.clone())));
        app.resize(width, height);
        (app, jobs)
    }

    fn renders(jobs: &Jobs) -> Vec<PictureKey> {
        jobs.lock()
            .unwrap()
            .iter()
            .filter(|j| j.mode == Mode::Render)
            .map(|j| j.key.clone())
            .collect()
    }

    fn last_job(jobs: &Jobs, key: &PictureKey, mode: Mode) -> Job {
        jobs.lock()
            .unwrap()
            .iter()
            .rev()
            .find(|j| &j.key == key && j.mode == mode)
            .cloned()
            .expect("job submitted")
    }

    fn result_of(job: &Job, outcome: Outcome) -> RenderResult {
        RenderResult {
            key: job.key.clone(),
            generation: job.generation,
            width: job.width,
            height: job.height,
            outcome,
        }
    }

    fn deliver_picture(app: &mut App, jobs: &Jobs, key: PictureKey, size: (u32, u32)) {
        let job = last_job(jobs, &key, Mode::Render);
        let image = Arc::new(RgbaImage::new(size.0, size.1));
        app.on_result(result_of(
            &job,
            Outcome::Picture {
                image,
                natural: size,
            },
        ));
    }

    /// A diagram, then `n` one-line paragraphs, so the next diagram is far away.
    fn diagrams_far_apart() -> String {
        let filler = many(40);
        format!(
            "```mermaid\ngraph TD\n A-->B\n```\n\n{filler}\n\n```mermaid\ngraph TD\n C-->D\n```\n\n{filler}\n"
        )
    }

    // ---- debounced re-rasterisation after a resize (KB-41) ----

    use std::time::Duration;

    #[derive(Debug, Clone, Default)]
    struct FakeClock(Arc<Mutex<Duration>>);

    impl crate::ui::pictures::Clock for FakeClock {
        fn now(&self) -> Duration {
            *self.0.lock().unwrap()
        }
    }

    impl FakeClock {
        fn advance(&self, ms: u64) {
            *self.0.lock().unwrap() += Duration::from_millis(ms);
        }
    }

    const NATURAL: (u32, u32) = (1000, 400);

    /// A diagram whose raster arrived at width 40, with a controllable clock.
    fn settled_diagram() -> (App, Jobs, FakeClock) {
        let clock = FakeClock::default();
        let jobs = Jobs::default();
        let mut app = App::new(
            doc(&format!("```mermaid\ngraph TD\n A-->B\n```\n\n{}", many(5))),
            "a.md".into(),
        );
        app.set_clock(Box::new(clock.clone()));
        app.set_job_sink(Box::new(FakeSink(jobs.clone())));
        app.resize(40, 20 + CHROME);
        deliver_picture(&mut app, &jobs, PictureKey::Diagram(0), NATURAL);
        (app, jobs, clock)
    }

    fn count(jobs: &Jobs) -> usize {
        renders(jobs).len()
    }

    #[test]
    fn resize_relayouts_immediately_and_re_rasterises_once_after_150_ms_without_further_resizes() {
        let (mut app, jobs, clock) = settled_diagram();
        assert_eq!(count(&jobs), 1);
        let old = last_job(&jobs, &PictureKey::Diagram(0), Mode::Render);
        app.resize(30, 20 + CHROME);
        let slot = app.layout.rows.iter().find_map(|r| r.slot.clone()).unwrap();
        assert_eq!(slot.px.0, 260, "layout follows the new width at once");
        assert_eq!(count(&jobs), 1, "nothing is submitted yet");
        clock.advance(149);
        app.tick();
        assert_eq!(count(&jobs), 1);
        assert!(app.next_wakeup().is_some());
        clock.advance(1);
        app.tick();
        assert_eq!(count(&jobs), 2);
        let new = last_job(&jobs, &PictureKey::Diagram(0), Mode::Render);
        assert_eq!(new.width, 260);
        assert!(new.generation > old.generation);
        app.tick();
        assert_eq!(count(&jobs), 2, "once only");
        assert!(app.next_wakeup().is_none());
    }

    #[test]
    fn a_burst_of_resize_events_triggers_a_single_re_rasterisation() {
        let (mut app, jobs, clock) = settled_diagram();
        for width in [36, 33, 30, 28] {
            app.resize(width, 20 + CHROME);
            clock.advance(100);
            app.tick();
            assert_eq!(count(&jobs), 1, "still debouncing at width {width}");
        }
        clock.advance(150);
        app.tick();
        assert_eq!(count(&jobs), 2);
        assert_eq!(
            last_job(&jobs, &PictureKey::Diagram(0), Mode::Render).width,
            240
        );
    }

    #[test]
    fn slots_whose_pixel_size_changed_show_the_placeholder_until_the_new_picture_arrives() {
        let (mut app, jobs, clock) = settled_diagram();
        let text = screen_text(&mut app, 40, 20 + CHROME);
        assert!(text.contains('#'), "{text}");
        app.resize(30, 20 + CHROME);
        let text = screen_text(&mut app, 30, 20 + CHROME);
        assert!(text.contains("Rendering diagram"), "{text}");
        assert!(!text.contains('#'), "{text}");
        clock.advance(150);
        app.tick();
        let text = screen_text(&mut app, 30, 20 + CHROME);
        assert!(text.contains("Rendering diagram"), "{text}");
        deliver_picture(&mut app, &jobs, PictureKey::Diagram(0), NATURAL);
        let text = screen_text(&mut app, 30, 20 + CHROME);
        assert!(!text.contains("Rendering diagram"), "{text}");
        assert!(text.contains('#'), "{text}");
    }

    #[test]
    fn results_rendered_for_the_old_width_are_discarded() {
        let (mut app, jobs, clock) = settled_diagram();
        let old = last_job(&jobs, &PictureKey::Diagram(0), Mode::Render);
        app.resize(30, 20 + CHROME);
        clock.advance(150);
        app.tick();
        let image = Arc::new(RgbaImage::new(NATURAL.0, NATURAL.1));
        app.on_result(result_of(
            &old,
            Outcome::Picture {
                image,
                natural: NATURAL,
            },
        ));
        let text = screen_text(&mut app, 30, 20 + CHROME);
        assert!(text.contains("Rendering diagram"), "{text}");
        deliver_picture(&mut app, &jobs, PictureKey::Diagram(0), NATURAL);
        let text = screen_text(&mut app, 30, 20 + CHROME);
        assert!(text.contains('#'), "{text}");
    }

    #[test]
    fn diagram_slot_shows_the_rendering_placeholder_until_its_picture_arrives() {
        let (mut app, jobs) = pic_doc_app("```mermaid\ngraph TD\n A-->B\n```\n", 40, 12 + CHROME);
        let text = screen_text(&mut app, 40, 12 + CHROME);
        assert!(text.contains("Rendering diagram"), "{text}");
        assert_eq!(renders(&jobs), vec![PictureKey::Diagram(0)]);
        assert!(!text.contains('#'));
        deliver_picture(&mut app, &jobs, PictureKey::Diagram(0), (100, 40));
        let text = screen_text(&mut app, 40, 12 + CHROME);
        assert!(!text.contains("Rendering diagram"), "{text}");
        assert!(text.contains('#'), "{text}");
    }

    #[test]
    fn an_arriving_picture_triggers_a_relayout_with_the_real_slot_height_and_keeps_the_reading_position()
     {
        let md = format!("```mermaid\ngraph TD\n A-->B\n```\n\n{}", many(30));
        let (mut app, jobs) = pic_doc_app(&md, 40, 10 + CHROME);
        let rows_before = app.row_count();
        app.scroll_by(14);
        let anchor = app.top_anchor();
        assert!(anchor.is_some_and(|(block, _)| block > 0));
        deliver_picture(&mut app, &jobs, PictureKey::Diagram(0), (100, 40));
        // 10 placeholder rows became 2 picture rows.
        assert_eq!(app.row_count(), rows_before - 8);
        assert_eq!(app.top_anchor(), anchor);
        assert_eq!(app.scroll(), 6);
    }

    #[test]
    fn only_slots_within_one_screen_of_the_viewport_are_submitted_for_rendering() {
        let (_app, jobs) = pic_doc_app(&diagrams_far_apart(), 40, 10 + CHROME);
        assert_eq!(renders(&jobs), vec![PictureKey::Diagram(0)]);
    }

    #[test]
    fn scrolling_a_slot_into_range_submits_its_render_job_once() {
        let (mut app, jobs) = pic_doc_app(&diagrams_far_apart(), 40, 10 + CHROME);
        let second = app.layout.rows.iter().position(|r| {
            r.slot
                .as_ref()
                .is_some_and(|s| s.key == PictureKey::Diagram(1))
        });
        let second = second.expect("second diagram laid out");
        // One page at a time until the slot is one screen below the viewport.
        loop {
            press(&mut app, KeyCode::PageDown);
            if app.scroll() + 2 * app.viewport_height() > second {
                break;
            }
            assert_eq!(renders(&jobs), vec![PictureKey::Diagram(0)]);
        }
        assert_eq!(
            renders(&jobs),
            vec![PictureKey::Diagram(0), PictureKey::Diagram(1)]
        );
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('k'));
        press(&mut app, KeyCode::PageUp);
        press(&mut app, KeyCode::PageDown);
        assert_eq!(renders(&jobs).len(), 2);
    }

    #[test]
    fn cached_pictures_are_reused_when_scrolling_back() {
        let (mut app, jobs) = pic_doc_app(&diagrams_far_apart(), 40, 10 + CHROME);
        deliver_picture(&mut app, &jobs, PictureKey::Diagram(0), (100, 40));
        press(&mut app, KeyCode::Char('G'));
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.scroll(), 0);
        let text = screen_text(&mut app, 40, 10 + CHROME);
        assert!(text.contains('#'), "{text}");
        let again = renders(&jobs)
            .into_iter()
            .filter(|k| *k == PictureKey::Diagram(0))
            .count();
        assert_eq!(again, 1);
    }

    #[test]
    fn a_failed_diagram_shows_the_error_block_while_the_rest_of_the_document_renders() {
        let md = "before\n\n```mermaid\nnot a diagram\n```\n\nafter\n";
        let (mut app, jobs) = pic_doc_app(md, 50, 20 + CHROME);
        let job = last_job(&jobs, &PictureKey::Diagram(0), Mode::Render);
        app.on_result(result_of(
            &job,
            Outcome::Error("Parse error on line 1".into()),
        ));
        let text = screen_text(&mut app, 50, 20 + CHROME);
        assert!(text.contains("Parse error on line 1"), "{text}");
        assert!(text.contains("not a diagram"), "{text}");
        assert!(text.contains("before") && text.contains("after"), "{text}");
        assert!(!text.contains("Rendering diagram"), "{text}");
    }

    #[test]
    fn snapshot_of_a_document_with_a_diagram_a_local_image_a_missing_image_and_a_remote_image() {
        let md = "```mermaid\ngraph TD\n A-->B\n```\n\n![local](pic.png)\n\n![gone](gone.png)\n\n![far](https://example.com/x.png)\n\nend\n";
        let (mut app, jobs) = pic_doc_app(md, 40, 30 + CHROME);
        deliver_picture(&mut app, &jobs, PictureKey::Diagram(0), (200, 60));
        let local = PictureKey::Image("pic.png".into());
        let gone = PictureKey::Image("gone.png".into());
        let measure = last_job(&jobs, &local, Mode::Measure);
        app.on_result(result_of(&measure, Outcome::Natural(120, 60)));
        deliver_picture(&mut app, &jobs, local, (120, 60));
        let measure = last_job(&jobs, &gone, Mode::Measure);
        app.on_result(result_of(
            &measure,
            Outcome::Error("image not found".into()),
        ));
        // The remote image is never fetched or measured.
        assert!(jobs.lock().unwrap().iter().all(|j| matches!(
            &j.key,
            PictureKey::Diagram(_) | PictureKey::Image(_)
        ) && j.key
            != PictureKey::Image("https://example.com/x.png".into())));
        let mut terminal = Terminal::new(TestBackend::new(40, 30 + CHROME)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        insta::assert_snapshot!(terminal.backend());
    }

    // ---- diagram text mode (KB-30) ----

    fn jobs_of(jobs: &Jobs, mode: Mode) -> usize {
        jobs.lock()
            .unwrap()
            .iter()
            .filter(|j| j.mode == mode)
            .count()
    }

    fn deliver_text(app: &mut App, jobs: &Jobs, n: usize, outcome: Outcome) {
        let job = last_job(jobs, &PictureKey::Diagram(n), Mode::Text);
        app.on_result(result_of(&job, outcome));
    }

    const FLOW: &str = "```mermaid\ngraph TD\n A-->B\n```\n";

    #[test]
    fn d_toggles_diagram_text_mode_and_recomputes_the_layout() {
        let md = format!("{FLOW}\n{}", many(30));
        let (mut app, jobs) = pic_doc_app(&md, 40, 12 + CHROME);
        deliver_picture(&mut app, &jobs, PictureKey::Diagram(0), (100, 40));
        let picture_rows = app.row_count();
        app.scroll_by(6);
        let anchor = app.top_anchor();
        let renders_before = jobs_of(&jobs, Mode::Render);

        press(&mut app, KeyCode::Char('d'));
        assert_eq!(jobs_of(&jobs, Mode::Text), 1);
        assert_ne!(app.row_count(), picture_rows, "placeholder is taller");
        deliver_text(
            &mut app,
            &jobs,
            0,
            Outcome::Text("┌───┐\n│ A │\n└───┘".into()),
        );

        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.row_count(), picture_rows);
        assert_eq!(app.top_anchor(), anchor);
        assert_eq!(
            jobs_of(&jobs, Mode::Render),
            renders_before,
            "cached picture reused"
        );
    }

    #[test]
    fn text_drawings_and_the_unavailable_note_replace_the_picture_in_text_mode() {
        let md = format!("{FLOW}\n```mermaid\npie\n \"a\": 1\n```\n");
        let (mut app, jobs) = pic_doc_app(&md, 70, 30 + CHROME);
        press(&mut app, KeyCode::Char('d'));
        deliver_text(
            &mut app,
            &jobs,
            0,
            Outcome::Text("┌───┐\n│ A │\n└───┘".into()),
        );
        deliver_text(&mut app, &jobs, 1, Outcome::TextUnavailable);
        let text = screen_text(&mut app, 70, 30 + CHROME);
        assert!(text.contains("│ A │"), "{text}");
        assert!(
            text.contains("Text rendering not available for this diagram type"),
            "{text}"
        );
        assert!(text.contains("pie"), "{text}");
    }

    #[test]
    fn text_diagrams_option_starts_with_text_mode_on() {
        let jobs = Jobs::default();
        let mut app = App::new(doc(FLOW), "a.md".into());
        app.set_text_mode(true);
        app.set_job_sink(Box::new(FakeSink(jobs.clone())));
        app.resize(40, 12 + CHROME);
        assert_eq!(jobs_of(&jobs, Mode::Text), 1);
        assert!(renders(&jobs).is_empty(), "no picture render in text mode");
    }

    #[test]
    fn status_bar_shows_the_text_diagrams_mode_while_it_is_on() {
        let (mut app, _jobs) = pic_doc_app(FLOW, 60, 12 + CHROME);
        let off = screen(&mut app, 60, 12 + CHROME);
        assert!(!off.last().unwrap().contains("Text diagrams"));
        press(&mut app, KeyCode::Char('d'));
        let on = screen(&mut app, 60, 12 + CHROME);
        assert!(on.last().unwrap().contains("Text diagrams"), "{on:?}");
        press(&mut app, KeyCode::Char('d'));
        let off = screen(&mut app, 60, 12 + CHROME);
        assert!(!off.last().unwrap().contains("Text diagrams"));
    }

    #[test]
    fn snapshot_of_a_document_with_a_diagram_in_text_mode() {
        let md = format!("before\n\n{FLOW}\nafter\n");
        let (mut app, jobs) = pic_doc_app(&md, 40, 14 + CHROME);
        press(&mut app, KeyCode::Char('d'));
        deliver_text(
            &mut app,
            &jobs,
            0,
            Outcome::Text("┌───┐\n│ A │\n└─┬─┘\n┌─▼─┐\n│ B │\n└───┘".into()),
        );
        let mut terminal = Terminal::new(TestBackend::new(40, 14 + CHROME)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        insta::assert_snapshot!(terminal.backend());
    }

    // ---- search stays stable while diagrams finish (KB-87) ----

    const SEARCH_DOC: &str = "alpha diagram\n\n```mermaid\ngraph TD\n A-->B\n```\n\n```mermaid\nbroken diagram\n```\n\nbeta diagram\n\ngamma diagram\n";

    fn finish_diagrams(app: &mut App, jobs: &Jobs) {
        deliver_picture(app, jobs, PictureKey::Diagram(0), NATURAL);
        let job = last_job(jobs, &PictureKey::Diagram(1), Mode::Render);
        app.on_result(result_of(
            &job,
            Outcome::Error("Diagram parse error on line 1".into()),
        ));
    }

    fn search_status_text(app: &App) -> String {
        app.search_status().unwrap_or_default()
    }

    fn current_text(app: &App) -> String {
        let m = app.search.current().expect("a current match");
        app.layout.index.texts[m.block].text.clone()
    }

    fn start_search() -> (App, Jobs) {
        let (mut app, jobs) = pic_doc_app(SEARCH_DOC, 50, 20 + CHROME);
        app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        for c in "diagram".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Enter);
        (app, jobs)
    }

    #[test]
    fn the_current_match_stays_the_same_match_when_a_diagram_result_relayouts_the_document() {
        let (mut app, jobs) = start_search();
        assert_eq!(search_status_text(&app), "match 1/3");
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(current_text(&app), "gamma diagram");
        finish_diagrams(&mut app, &jobs);
        assert_eq!(search_status_text(&app), "match 3/3");
        assert_eq!(current_text(&app), "gamma diagram");
    }

    #[test]
    fn n_from_the_first_match_wraps_to_the_last_match_after_diagrams_finish_rendering() {
        let (mut app, jobs) = start_search();
        finish_diagrams(&mut app, &jobs);
        assert_eq!(search_status_text(&app), "match 1/3");
        app.handle_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT));
        assert_eq!(search_status_text(&app), "match 3/3");
        assert_eq!(current_text(&app), "gamma diagram");
    }
}

mod activate;
mod focus;
mod fullscreen;
mod help;
mod perf;
mod reload;
mod scan_async;
mod search;
mod sidebar;
mod toc;

#[test]
fn no_binding_in_the_key_map_uses_a_function_key() {
    use crate::ui::keymap::BINDINGS;
    let f: Vec<_> = BINDINGS
        .iter()
        .filter(|b| matches!(b.code, KeyCode::F(_)))
        .map(|b| b.label)
        .collect();
    assert!(f.is_empty(), "function-key bindings: {f:?}");
}
mod click;
