//! Application state and key handling.

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::Frame;

use crate::doc::Document;

#[cfg(test)]
mod tests;

/// State of the viewer: document, layout and scroll.
#[derive(Debug)]
pub struct App {
    quit: bool,
}

impl App {
    pub fn new(_doc: Document, _rel_path: String) -> Self {
        Self { quit: false }
    }

    /// Terminal size changed (or is known for the first time).
    pub fn resize(&mut self, _width: u16, _height: u16) {}

    /// Index of the first visible row.
    pub fn scroll(&self) -> usize {
        0
    }

    /// Rows of the current layout.
    pub fn row_count(&self) -> usize {
        0
    }

    /// Rows the pane shows at once.
    pub fn viewport_height(&self) -> usize {
        1
    }

    /// (block, row offset inside the block) of the top visible row.
    pub fn top_anchor(&self) -> Option<(usize, usize)> {
        None
    }

    pub fn set_message(&mut self, _message: impl Into<String>) {}

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn handle_key(&mut self, _key: KeyEvent) {}

    pub fn handle_mouse(&mut self, _mouse: MouseEvent) {}

    pub fn render(&mut self, _frame: &mut Frame) {}
}
