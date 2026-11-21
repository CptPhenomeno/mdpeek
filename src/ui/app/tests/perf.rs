//! Large documents and first paint (KB-62, SPEC §4.5, §5).

use std::sync::{Arc, Mutex};

use ratatui::{Terminal, backend::TestBackend};

use super::*;
use crate::graphics::detect::Selection;
use crate::layout::PictureKey;
use crate::render_worker::{Job, Mode};
use crate::ui::JobSink;
use ratatui_image::picker::ProtocolType;

const W: u16 = 80;
const H: u16 = 10 + CHROME;

/// `n` Rust code blocks separated by a paragraph.
fn code_blocks(n: usize) -> String {
    (0..n)
        .map(|i| format!("para {i}\n\n```rust\nfn f{i}() {{\n    let x = {i};\n}}\n```\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn drawing_a_frame_only_processes_the_visible_rows() {
    let mut app = app_from(&code_blocks(500), W, H);
    let total = 500;
    assert_eq!(app.code_blocks_in_document(), total);
    // Laying out and drawing the first screen highlights what is on it, no more.
    screen(&mut app, W, H);
    let done = app.highlighted_code_blocks();
    assert!(done >= 1, "the visible code block must be highlighted");
    assert!(
        done <= 5,
        "{done} of {total} code blocks highlighted for a 10-row viewport"
    );
    // Scrolling far away highlights the blocks that come into view, not the ones skipped.
    press(&mut app, KeyCode::End);
    screen(&mut app, W, H);
    let done = app.highlighted_code_blocks();
    assert!(done <= 10, "{done} code blocks highlighted after End");
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

fn diagrams_with_little_between(n: usize) -> String {
    (0..n)
        .map(|i| {
            format!(
                "```mermaid\ngraph TD\n A{i}-->B{i}\n```\n\n{}",
                (0..3)
                    .map(|j| format!("filler {i}.{j}"))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn only_pictures_within_one_screen_of_the_viewport_are_rasterised_in_a_large_document() {
    let jobs = Jobs::default();
    let mut app = App::new(doc(&diagrams_with_little_between(300)), "a.md".into());
    app.set_job_sink(Box::new(FakeSink(jobs.clone())));
    app.resize(W, H);
    let rendered = |jobs: &Jobs| -> Vec<usize> {
        jobs.lock()
            .unwrap()
            .iter()
            .filter(|j| j.mode == Mode::Render)
            .filter_map(|j| match j.key {
                PictureKey::Diagram(n) => Some(n),
                _ => None,
            })
            .collect()
    };
    let first = rendered(&jobs);
    assert!(!first.is_empty(), "the diagram on screen is rendered");
    assert!(
        first.iter().all(|&n| n <= 5),
        "far diagrams were rendered: {first:?}"
    );
    // Jump to the end: only the diagrams near the end join the queue.
    press(&mut app, KeyCode::End);
    let all = rendered(&jobs);
    assert!(
        all.len() < 20,
        "{} of 300 diagrams rendered after two screens",
        all.len()
    );
    assert!(
        all.iter().any(|&n| n >= 295),
        "no diagram near the end: {all:?}"
    );
    assert!(!all.contains(&150), "a diagram never near the viewport");
}

#[test]
fn the_first_frame_is_drawn_before_protocol_detection_and_before_any_diagram_finishes() {
    let md = "# Title of the doc\n\n```mermaid\ngraph TD\n A-->B\n```\n\ntext";
    let jobs = Jobs::default();
    let mut app = App::new(doc(md), "a.md".into());
    app.set_job_sink(Box::new(FakeSink(jobs.clone())));
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    let mut seen = None;
    crate::ui::first_frame_then_detect(&mut terminal, &mut app, |app| {
        // Detection runs now: the frame must already be on screen, and no diagram done.
        let frames = app.frames_drawn();
        seen = Some((frames, terminal_text(app)));
        Some(Selection {
            protocol: ProtocolType::Halfblocks,
            cell: (10, 20),
            detected: true,
        })
    })
    .unwrap();
    let (frames, text) = seen.expect("detection ran");
    assert_eq!(frames, 1, "no frame was drawn before detection");
    assert!(text.contains("Title of the doc"), "{text}");
    assert!(
        jobs.lock().unwrap().iter().any(|j| j.mode == Mode::Render),
        "diagram jobs are queued with the first frame"
    );
}

/// The document text as the app lays it out now.
fn terminal_text(app: &mut App) -> String {
    screen_text(app, W, H)
}
