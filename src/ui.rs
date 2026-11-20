//! App state, event loop, panes and terminal lifecycle.

pub mod app;
pub mod keymap;
pub mod opener;
pub mod overlay;
pub mod pictures;
pub mod terminal;

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
    detect::{StdioDetector, select},
};
use crate::picture_cache::PictureCache;
use crate::render_worker::RenderWorker;
use crate::ui::opener::SystemOpener;

/// Render worker threads.
const RENDER_THREADS: usize = 2;
/// Poll interval while render jobs are outstanding.
const RESULT_POLL: Duration = Duration::from_millis(50);
/// Poll interval when nothing is outstanding (only to notice a quit).
const IDLE_POLL: Duration = Duration::from_secs(1);

/// Run the viewer on `doc` until the user quits; `rel_path` is shown in the status bar.
/// `text_diagrams` is `--text-diagrams`. `protocol` is the `--protocol` choice; the detected one is announced in the status bar.
pub fn run(
    doc: Document,
    rel_path: String,
    protocol: ProtocolChoice,
    text_diagrams: bool,
) -> io::Result<()> {
    terminal::install_panic_hook(terminal::restore_terminal);
    let _guard = terminal::TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    // Detection needs raw mode and the alternate screen, and must precede the first draw.
    let detector = StdioDetector::default();
    let selection = select(protocol, &detector);
    let mut app = App::new(doc, rel_path);
    app.set_opener(Box::new(SystemOpener));
    app.set_text_mode(text_diagrams);
    app.set_cell(selection.cell);
    app.set_pictures(PictureSet::sliced(
        selection.picker(&detector),
        crate::theme::bg_rgba(),
    ));
    app.set_message(format!("Graphics: {}", selection.label()));
    // Diagrams and images render in the background; results come back here.
    let (tx, rx) = mpsc::channel();
    let worker = RenderWorker::new(PictureCache::with_default_limit(), tx, RENDER_THREADS);
    app.set_job_sink(Box::new(pictures::WorkerSink(worker)));
    let mut dirty = true;
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
    }
    Ok(())
}
