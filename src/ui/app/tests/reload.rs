//! `r` reloads the current file and the sidebar tree (KB-59, SPEC §3.8).

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crossterm::event::KeyCode;
use tempfile::TempDir;

use super::*;
use crate::layout::PictureKey;
use crate::render_worker::{Job, Mode, Outcome, RenderResult};
use crate::tree::{self, Start};
use crate::ui::JobSink;

const W: u16 = 80;
const H: u16 = 10 + CHROME;
const R: KeyCode = KeyCode::Char('r');

fn start(root: &Path, sidebar: bool) -> Start {
    Start {
        root: root.to_path_buf(),
        sidebar_visible: sidebar,
        tree: sidebar.then(|| tree::scan(root).unwrap()),
        initial_file: Some(root.join("a.md")),
    }
}

fn open(root: &Path, sidebar: bool) -> App {
    let doc = Document::from_path(&root.join("a.md")).unwrap();
    let mut app = App::new(doc, "a.md".into());
    app.set_start(start(root, sidebar));
    app.resize(W, H);
    app
}

fn project(content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.md"), content).unwrap();
    dir
}

fn write(dir: &TempDir, name: &str, content: &str) {
    fs::write(dir.path().join(name), content).unwrap();
}

#[test]
fn r_re_reads_the_current_file_and_shows_the_new_content() {
    let dir = project("# Before\n\nold text");
    let mut app = open(dir.path(), false);
    assert!(screen_text(&mut app, W, H).contains("old text"));
    write(&dir, "a.md", "# After\n\nnew text");
    press(&mut app, R);
    let text = screen_text(&mut app, W, H);
    assert!(text.contains("new text"), "{text}");
    assert!(!text.contains("old text"), "{text}");
    assert!(text.contains("Reloaded"), "{text}");
}

#[test]
fn the_scroll_position_is_kept_when_the_document_is_still_long_enough() {
    let dir = project(&many(50));
    let mut app = open(dir.path(), false);
    app.scroll_by(20);
    assert_eq!(app.scroll(), 20);
    write(&dir, "a.md", &many(80));
    press(&mut app, R);
    assert_eq!(app.scroll(), 20);
    assert_eq!(app.row_count(), 159);
}

#[test]
fn the_scroll_position_is_clamped_when_the_document_became_shorter() {
    let dir = project(&many(50));
    let mut app = open(dir.path(), false);
    app.scroll_by(60);
    write(&dir, "a.md", &many(10));
    press(&mut app, R);
    assert_eq!(app.row_count(), 19);
    assert_eq!(app.scroll(), 19 - app.viewport_height());
}

#[test]
fn r_rescans_the_sidebar_tree_and_keeps_the_open_file_marked() {
    let dir = project("# Alpha");
    let mut app = open(dir.path(), true);
    write(&dir, "new.md", "# New");
    let before = screen_text(&mut app, W, H);
    assert!(!before.contains("new.md"), "{before}");
    press(&mut app, R);
    let after = screen_text(&mut app, W, H);
    assert!(after.contains("new.md"), "{after}");
    assert!(after.contains("● a.md"), "{after}");
}

#[test]
fn a_file_start_whose_sidebar_was_never_shown_stays_lazy_after_r() {
    let dir = project("# Alpha");
    let mut app = open(dir.path(), false);
    write(&dir, "new.md", "# New");
    press(&mut app, R);
    press(&mut app, KeyCode::Char('s'));
    let text = screen_text(&mut app, W, H);
    assert!(text.contains("new.md"), "{text}");
    assert!(text.contains("● a.md"), "{text}");
}

type Jobs = Arc<Mutex<Vec<Job>>>;

#[derive(Debug)]
struct FakeSink(Jobs);

impl JobSink for FakeSink {
    fn submit(&self, job: Job) -> bool {
        self.0.lock().unwrap().push(job);
        true
    }
    fn set_generation(&self, _: u64) {}
}

fn render_count(jobs: &Jobs) -> usize {
    jobs.lock()
        .unwrap()
        .iter()
        .filter(|j| j.mode == Mode::Render)
        .count()
}

#[test]
fn unchanged_diagrams_are_not_re_rendered_after_a_reload() {
    let diagram = |src: &str| format!("```mermaid\n{src}\n```\n\nsome text");
    let dir = project(&diagram("graph TD\n A-->B"));
    let jobs = Jobs::default();
    let mut app = open(dir.path(), false);
    app.set_job_sink(Box::new(FakeSink(jobs.clone())));
    app.resize(W, H);
    assert_eq!(render_count(&jobs), 1);
    let job = jobs.lock().unwrap()[0].clone();
    app.on_result(RenderResult {
        key: PictureKey::Diagram(0),
        generation: job.generation,
        width: job.width,
        height: job.height,
        outcome: Outcome::Picture {
            image: Arc::new(image::RgbaImage::new(1000, 400)),
            natural: (1000, 400),
        },
    });
    let slots = |app: &App| app.layout.rows.iter().filter(|r| r.slot.is_some()).count();
    let rows = slots(&app);
    assert!(rows > 0);
    // Same diagram, other text around it: no new job, the picture stays.
    write(
        &dir,
        "a.md",
        &format!("{}\n\nmore", diagram("graph TD\n A-->B")),
    );
    press(&mut app, R);
    assert_eq!(render_count(&jobs), 1, "unchanged diagram re-submitted");
    assert_eq!(slots(&app), rows);
    // A changed diagram renders again.
    write(&dir, "a.md", &diagram("graph TD\n A-->C"));
    press(&mut app, R);
    assert_eq!(render_count(&jobs), 2);
}

#[test]
fn a_deleted_current_file_shows_an_error_in_the_status_bar_and_keeps_the_content() {
    let dir = project("# Keep\n\nstill here");
    let mut app = open(dir.path(), false);
    fs::remove_file(dir.path().join("a.md")).unwrap();
    press(&mut app, R);
    let lines = screen(&mut app, W, H);
    assert!(lines.join("\n").contains("still here"), "{lines:?}");
    assert!(lines.last().unwrap().contains("Cannot reload"), "{lines:?}");
}
