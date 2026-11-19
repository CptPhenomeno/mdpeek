//! Application state and key handling.

use crossterm::event::{KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::keymap::{Action, action_for};
use crate::doc::Document;
use crate::layout::{self, Layout, MARGIN, State};
use crate::theme;

#[cfg(test)]
mod tests;

/// Cell size in pixels until the terminal is queried (pictures come later).
const CELL: (u16, u16) = (10, 20);
/// Rows a mouse wheel notch scrolls (SPEC §3.8).
const WHEEL_ROWS: usize = 3;
/// Terminal rows taken by the status bar.
const STATUS_ROWS: u16 = 1;
/// Border columns / rows around the document, per axis.
const BORDER: u16 = 2;

/// State of the viewer: document, layout and scroll.
#[derive(Debug)]
pub struct App {
    doc: Document,
    layout: Layout,
    /// Path shown in the status bar, relative to the root.
    rel_path: String,
    /// Terminal size, once known.
    size: (u16, u16),
    scroll: usize,
    message: Option<String>,
    /// Whether the document pane has the focus (the sidebar arrives later).
    focused: bool,
    quit: bool,
}

impl App {
    pub fn new(doc: Document, rel_path: String) -> Self {
        let message = doc
            .invalid_utf8
            .then(|| "File is not valid UTF-8".to_owned());
        let mut app = Self {
            doc,
            layout: Layout {
                state: State::Ready,
                rows: Vec::new(),
            },
            rel_path,
            size: (80, 24),
            scroll: 0,
            message,
            focused: true,
            quit: false,
        };
        app.relayout();
        app
    }

    /// Terminal size changed (or is known for the first time); the top row stays put.
    pub fn resize(&mut self, width: u16, height: u16) {
        if self.size == (width, height) {
            return;
        }
        let width_changed = self.pane_width_of(self.size.0) != self.pane_width_of(width);
        self.size = (width, height);
        if width_changed {
            let anchor = self.top_anchor();
            self.relayout();
            self.scroll = anchor.map_or(0, |a| self.scroll_of(a));
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// Index of the first visible row.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Rows of the current layout.
    pub fn row_count(&self) -> usize {
        self.layout.rows.len()
    }

    /// Rows the pane shows at once.
    pub fn viewport_height(&self) -> usize {
        usize::from(self.size.1.saturating_sub(STATUS_ROWS + BORDER)).max(1)
    }

    /// (block, row offset inside the block) of the top visible row.
    pub fn top_anchor(&self) -> Option<(usize, usize)> {
        let block = self.layout.rows.get(self.scroll)?.block;
        Some((block, self.scroll - self.first_row_of(block)?))
    }

    /// Show `message` in the status bar until the next key press.
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        self.message = None;
        let Some(action) = action_for(key) else {
            return;
        };
        match action {
            Action::ScrollUp => self.scroll_by(-1),
            Action::ScrollDown => self.scroll_by(1),
            Action::PageUp => self.scroll_by(-self.page()),
            Action::PageDown => self.scroll_by(self.page()),
            Action::Top => self.scroll = 0,
            Action::Bottom => self.scroll = self.max_scroll(),
            Action::Quit => self.quit = true,
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        let rows = WHEEL_ROWS as isize;
        match mouse.kind {
            MouseEventKind::ScrollDown => self.scroll_by(rows),
            MouseEventKind::ScrollUp => self.scroll_by(-rows),
            _ => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.resize(area.width, area.height);
        let pane = Rect {
            height: area.height.saturating_sub(STATUS_ROWS),
            ..area
        };
        let status = Rect {
            y: area.y + pane.height,
            height: area.height - pane.height,
            ..area
        };
        // Paint every cell first: later widgets only patch the foreground.
        frame.render_widget(Block::new().style(theme::page()), area);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(theme::border(self.focused));
        let inner = block.inner(pane);
        frame.render_widget(block, pane);
        frame.render_widget(Paragraph::new(self.pane_lines(inner.height)), inner);
        frame.render_widget(Paragraph::new(self.status_line(status.width)), status);
    }

    fn pane_width_of(&self, terminal_width: u16) -> u16 {
        terminal_width.saturating_sub(BORDER)
    }

    fn relayout(&mut self) {
        self.layout = layout::layout(&self.doc, self.pane_width_of(self.size.0), CELL);
    }

    fn first_row_of(&self, block: usize) -> Option<usize> {
        self.layout.rows.iter().position(|r| r.block == block)
    }

    /// Row index for an anchor in the current layout.
    fn scroll_of(&self, (block, offset): (usize, usize)) -> usize {
        let Some(first) = self.first_row_of(block) else {
            return 0;
        };
        let len = self.layout.rows.iter().filter(|r| r.block == block).count();
        first + offset.min(len.saturating_sub(1))
    }

    fn max_scroll(&self) -> usize {
        self.row_count().saturating_sub(self.viewport_height())
    }

    fn page(&self) -> isize {
        self.viewport_height().saturating_sub(1).max(1) as isize
    }

    fn scroll_by(&mut self, delta: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.max_scroll());
    }

    fn percent(&self) -> usize {
        match self.max_scroll() {
            0 => 100,
            max => self.scroll * 100 / max,
        }
    }

    fn pane_lines(&self, height: u16) -> Vec<Line<'static>> {
        let notice = |text: &'static str| vec![Line::styled(text, theme::dim())];
        match self.layout.state {
            State::TooNarrow => notice("Window too narrow"),
            State::Empty => notice("Empty document"),
            State::Ready => self
                .layout
                .rows
                .iter()
                .skip(self.scroll)
                .take(usize::from(height))
                .map(|row| {
                    let margin = " ".repeat(usize::from(MARGIN));
                    let mut spans = vec![Span::raw(margin)];
                    spans.extend(
                        row.spans
                            .iter()
                            .map(|s| Span::styled(s.text.clone(), s.style)),
                    );
                    Line::from(spans)
                })
                .collect(),
        }
    }

    fn status_line(&self, width: u16) -> Line<'static> {
        let left = match &self.message {
            Some(message) => format!(" {} | {message}", self.rel_path),
            None => format!(" {}", self.rel_path),
        };
        // The mode area (search, text diagrams) is empty until those modes exist.
        let right = format!("{}% ", self.percent());
        let pad = usize::from(width).saturating_sub(left.width() + right.width());
        let text = format!("{left}{}{right}", " ".repeat(pad));
        Line::styled(text, theme::status())
    }
}
