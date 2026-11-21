//! App state, event loop, panes and terminal lifecycle.

pub mod app;
pub mod fullscreen;
pub mod keymap;
pub mod opener;
pub mod overlay;
pub mod pictures;
pub mod terminal;
pub mod view_worker;

pub use app::App;
pub use pictures::JobSink;

use std::io;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::cli::ProtocolChoice;
use crate::doc::Document;
use crate::graphics::{
    PictureSet,
    detect::{Detector, Selection, StdioDetector, select},
};
use crate::picture_cache::PictureCache;
use crate::render_worker::RenderWorker;
use crate::tree::Start;
use crate::ui::opener::SystemOpener;

/// Render worker threads.
const RENDER_THREADS: usize = 2;
/// Poll interval while render jobs are outstanding.
const RESULT_POLL: Duration = Duration::from_millis(50);
/// Poll interval when nothing is outstanding (only to notice a quit).
const IDLE_POLL: Duration = Duration::from_secs(1);

/// How long auto-detection waits for the terminal. It runs after the first frame,
/// so it never delays the first paint; this only bounds how long a terminal that
/// leaves the query unanswered keeps the keyboard busy (the library default is 2 s).
const DETECT_TIMEOUT: Duration = Duration::from_millis(250);

/// Draws the first frame, then runs `detect`, which adopts what the terminal
/// reports (protocol, cell size). Detection can wait for the terminal, so the
/// document is on screen before it starts.
pub fn first_frame_then_detect<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    detect: impl FnOnce(&mut App),
) -> io::Result<()> {
    terminal
        .draw(|f| app.render(f))
        .map_err(|e| io::Error::other(e.to_string()))?;
    detect(app);
    Ok(())
}

/// Makes `selection` the protocol and cell size of `app`.
fn adopt(app: &mut App, selection: &Selection, detector: &dyn Detector) {
    app.set_cell(selection.cell);
    app.set_pictures(PictureSet::sliced(
        selection.picker(detector),
        crate::theme::bg_rgba(),
    ));
    app.set_message(format!("Graphics: {}", selection.label()));
}

/// Run the viewer on `doc` until the user quits; `rel_path` is shown in the status bar.
/// `text_diagrams` is `--text-diagrams`. `protocol` is the `--protocol` choice; the detected one is announced in the status bar.
pub fn run(
    doc: Document,
    rel_path: String,
    start: Start,
    protocol: ProtocolChoice,
    text_diagrams: bool,
) -> io::Result<()> {
    terminal::install_panic_hook(terminal::restore_terminal);
    let _guard = terminal::TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    // Detection needs raw mode and the alternate screen. A forced protocol is
    // known now; with auto, the first frame uses halfblocks and the window's cell
    // size, and the query runs right after it is on screen.
    let detector = StdioDetector::new(DETECT_TIMEOUT);
    let auto = protocol == ProtocolChoice::Auto;
    let provisional = select(
        if auto {
            ProtocolChoice::Halfblocks
        } else {
            protocol
        },
        &detector,
    );
    let mut app = App::new(doc, rel_path);
    app.set_start(start);
    app.set_opener(Box::new(SystemOpener));
    app.set_text_mode(text_diagrams);
    adopt(&mut app, &provisional, &detector);
    if auto {
        app.set_message("Graphics: detecting");
    }
    // Diagrams and images render in the background; results come back here.
    let (tx, rx) = mpsc::channel();
    let worker = RenderWorker::new(PictureCache::with_default_limit(), tx, RENDER_THREADS);
    app.set_job_sink(Box::new(pictures::WorkerSink(worker)));
    // The full-screen view renders on its own thread.
    let (view_tx, view_rx) = mpsc::channel();
    if let Some(sink) = view_worker::spawn(view_tx) {
        app.set_view_sink(Box::new(sink));
    }
    first_frame_then_detect(&mut terminal, &mut app, |app| {
        if auto {
            adopt(app, &select(protocol, &detector), &detector);
        }
    })?;
    let mut dirty = auto;
    while !app.should_quit() {
        if dirty {
            terminal.draw(|f| app.render(f))?;
        }
        // Wake up regularly only while jobs are outstanding.
        let mut timeout = if app.has_outstanding() {
            RESULT_POLL
        } else {
            IDLE_POLL
        };
        // The resize debounce deadline wakes the loop too.
        if let Some(wait) = app.next_wakeup() {
            timeout = timeout.min(wait);
        }
        dirty = false;
        if event::poll(timeout)? {
            dirty = true;
            match event::read()? {
                Event::Key(key) => app.handle_key(key),
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                Event::Resize(width, height) => app.resize(width, height),
                _ => {}
            }
        }
        dirty |= app.tick();
        while let Ok(result) = rx.try_recv() {
            dirty = true;
            app.on_result(result);
        }
        while let Ok(reply) = view_rx.try_recv() {
            dirty = true;
            app.on_view_reply(reply);
        }
    }
    Ok(())
}
