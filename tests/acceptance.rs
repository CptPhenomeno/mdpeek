//! SPEC §5 acceptance checks that need the real render pipeline: the showcase
//! through the real `App` with halfblocks and with diagram text mode on.
//! Pictures are rendered by the real worker (merman, resvg, image decoder).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui_image::picker::Picker;
use readmd::doc::Document;
use readmd::graphics::PictureSet;
use readmd::picture_cache::PictureCache;
use readmd::render_worker::RenderWorker;
use readmd::ui::App;

const WIDTH: u16 = 100;
const HEIGHT: u16 = 700;

fn showcase_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/showcase.md")
}

/// The showcase on a viewport tall enough to show it whole, with every job done.
fn settled_app(text_mode: bool, halfblocks: bool) -> App {
    let doc = Document::from_path(&showcase_path()).expect("showcase is readable");
    let mut app = App::new(doc, "showcase.md".into());
    app.set_cell((10, 20));
    if halfblocks {
        let bg = readmd::theme::bg_rgba();
        app.set_pictures(PictureSet::sliced(Picker::halfblocks(), bg));
    }
    app.set_text_mode(text_mode);
    let (tx, rx) = mpsc::channel();
    let worker = RenderWorker::new(PictureCache::with_default_limit(), tx, 4);
    app.set_job_sink(Box::new(readmd::ui::pictures::WorkerSink(worker)));
    app.resize(WIDTH, HEIGHT);
    let deadline = Instant::now() + Duration::from_secs(120);
    while app.has_outstanding() {
        assert!(Instant::now() < deadline, "jobs did not finish");
        if let Ok(result) = rx.recv_timeout(Duration::from_millis(50)) {
            app.on_result(result);
        }
    }
    app
}

fn screen_lines(app: &mut App) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).expect("terminal");
    let frame = terminal.draw(|f| app.render(f)).expect("draw");
    let buf = frame.buffer;
    (0..HEIGHT)
        .map(|y| {
            (0..WIDTH)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .collect()
        })
        .collect()
}

/// Number of pictures on screen: stretches of rows holding half-block cells.
/// Blank rows do not end a stretch (a diagram can have a blank band inside),
/// a row of text does; every picture of the showcase has text around it.
fn picture_groups(lines: &[String]) -> usize {
    let mut groups = 0;
    let mut in_group = false;
    for l in lines {
        let inner = l.trim_matches(|c: char| c == ' ' || c == '│');
        if l.contains('▀') || l.contains('▄') {
            if !in_group {
                groups += 1;
            }
            in_group = true;
        } else if !inner.is_empty() && !inner.chars().all(|c| "─┌┐└┘│ ".contains(c)) {
            in_group = false;
        }
    }
    groups
}

/// The pictures the showcase must draw: three uses of local image files plus
/// every diagram except the one with the syntax error.
fn expected_pictures() -> usize {
    let text = std::fs::read_to_string(showcase_path()).expect("showcase");
    let fences = text
        .lines()
        .filter(|l| l.trim_start().to_lowercase().starts_with("```mermaid"))
        .count();
    fences - 1 + 3
}

#[test]
fn with_halfblocks_every_showcase_picture_is_drawn_as_halfblocks() {
    let mut app = settled_app(false, true);
    let lines = screen_lines(&mut app);
    let runs = picture_groups(&lines);
    let screen = lines.join("\n");
    assert!(
        !screen.contains("Rendering diagram"),
        "a diagram is still a placeholder"
    );
    assert_eq!(
        runs,
        expected_pictures(),
        "picture runs on screen:\n{screen}"
    );
}

#[test]
fn with_text_mode_on_every_showcase_diagram_shows_text_or_the_unavailable_note() {
    let mut app = settled_app(true, true);
    let lines = screen_lines(&mut app);
    let screen = lines.join("\n");
    assert!(!screen.contains("Rendering diagram"), "still rendering");
    // Only the three uses of the local images remain pictures.
    assert_eq!(picture_groups(&lines), 3, "{screen}");
    let unavailable = screen
        .matches("Text rendering not available for this diagram type")
        .count();
    // gantt, mindmap and pie (no text drawing in merman), and nothing else.
    assert_eq!(unavailable, 3, "{screen}");
    // flowchart, sequence, class and state drawings use box-drawing characters.
    assert!(screen.contains('┌') && screen.contains('│'), "{screen}");
}
