//! App state, event loop, panes and terminal lifecycle.

pub mod app;
pub mod keymap;
pub mod overlay;
pub mod terminal;

pub use app::App;

use std::io;

use crossterm::event::{self, Event};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::cli::ProtocolChoice;
use crate::doc::Document;
use crate::graphics::{
    PictureSet,
    detect::{StdioDetector, select},
};
use image::Rgba;

/// Run the viewer on `doc` until the user quits; `rel_path` is shown in the status bar.
/// `protocol` is the `--protocol` choice; the detected one is announced in the status bar.
pub fn run(doc: Document, rel_path: String, protocol: ProtocolChoice) -> io::Result<()> {
    terminal::install_panic_hook(terminal::restore_terminal);
    let _guard = terminal::TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    // Detection needs raw mode and the alternate screen, and must precede the first draw.
    let selection = select(protocol, &StdioDetector);
    let mut app = App::new(doc, rel_path);
    app.set_cell(selection.cell);
    app.set_pictures(PictureSet::sliced(
        selection.picker(),
        Rgba([26, 27, 38, 255]),
    ));
    app.set_message(format!("Graphics: {}", selection.label()));
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
