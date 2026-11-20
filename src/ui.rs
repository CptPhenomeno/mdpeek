//! App state, event loop, panes and terminal lifecycle.

pub mod app;
pub mod keymap;
pub mod overlay;
pub mod terminal;

pub use app::App;

use std::io;

use crossterm::event::{self, Event};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::doc::Document;

/// Run the viewer on `doc` until the user quits; `rel_path` is shown in the status bar.
pub fn run(doc: Document, rel_path: String) -> io::Result<()> {
    terminal::install_panic_hook(terminal::restore_terminal);
    let _guard = terminal::TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut app = App::new(doc, rel_path);
    while !app.should_quit() {
        terminal.draw(|f| app.render(f))?;
        match event::read()? {
            Event::Key(key) => app.handle_key(key),
            Event::Mouse(mouse) => app.handle_mouse(mouse),
            Event::Resize(width, height) => app.resize(width, height),
            _ => {}
        }
    }
    Ok(())
}
