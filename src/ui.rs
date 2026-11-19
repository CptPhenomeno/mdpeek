//! App state, event loop, panes and terminal lifecycle.

pub mod app;
pub mod terminal;

pub use app::App;

use std::io;

use crossterm::event::{self, Event};
use ratatui::{Terminal, backend::CrosstermBackend};

/// Run the viewer until the user quits.
pub fn run(lines: Vec<String>) -> io::Result<()> {
    terminal::install_panic_hook(terminal::restore_terminal);
    let _guard = terminal::TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut app = App::new(lines);
    while !app.should_quit() {
        terminal.draw(|f| app.render(f))?;
        if let Event::Key(key) = event::read()? {
            app.handle_key(key);
        }
    }
    Ok(())
}
