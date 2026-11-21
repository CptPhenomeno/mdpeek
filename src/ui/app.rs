//! Application state and key handling.

use crossterm::event::{KeyEvent, KeyEventKind, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::keymap::{Action, action_for, input_action, sidebar_action};
use crate::doc::Document;
use crate::graphics::{PictureRequest, PictureSet};
use crate::layout::{self, Diagrams, Layout, MARGIN, PictureKey, State};
use crate::theme;

mod activate;
mod areas;
pub use areas::{Areas, areas};
mod click;
mod focus;
mod fullscreen;
mod help;
mod search;
mod sidebar;
#[cfg(test)]
mod tests;
mod toc;

pub use super::opener::{Opener, SystemOpener};
use super::pictures::{Clock, JobSink, Pictures};

/// Cell size in pixels until the terminal is queried.
const CELL: (u16, u16) = (10, 20);
/// Rows a mouse wheel notch scrolls (SPEC §3.8).
const WHEEL_ROWS: usize = 3;
/// Terminal rows taken by the status bar.
const STATUS_ROWS: u16 = 1;
/// Border columns / rows around the document, per axis.
const BORDER: u16 = 2;

/// An open overlay (popup) over the document pane: it owns the keyboard and
/// blocks clicks. The help overlay (KB-57) adds its variant here.
#[derive(Debug)]
enum Overlay {
    Toc(toc::Toc),
}

/// State of the viewer: document, layout and scroll.
#[derive(Debug)]
pub struct App {
    doc: Document,
    layout: Layout,
    /// Path shown in the status bar, relative to the root.
    rel_path: String,
    /// Terminal size, once known.
    size: (u16, u16),
    /// Whether `size` is the real terminal size: no picture is requested before.
    sized: bool,
    scroll: usize,
    message: Option<String>,
    sidebar: sidebar::Sidebar,
    pictures: PictureSet,
    /// Render jobs, natural sizes and diagram states.
    jobs: Pictures,
    /// Cell size in pixels, as the terminal reports it.
    cell: (u16, u16),
    /// Rectangles of the overlays on screen: pictures leave them uncovered.
    overlays: Vec<Rect>,
    /// The open overlay, if any.
    overlay: Option<Overlay>,
    /// Diagrams are shown as text drawings (`d`, `--text-diagrams`).
    text_mode: bool,
    /// Focused link or diagram (SPEC §3.6.1).
    focus: Option<focus::FocusId>,
    /// Hands URLs to the system (a fake in tests).
    opener: Box<dyn Opener>,
    /// Diagram Enter asked to see full screen; the full-screen view consumes it.
    fullscreen_request: Option<PictureKey>,
    /// The open full-screen view, if any.
    view: Option<fullscreen::View>,
    /// Where the view's render requests go.
    view_sink: Option<Box<dyn super::view_worker::ViewSink>>,
    /// Generation of the last view request: older replies are dropped.
    view_generation: u64,
    /// Diagram last clicked and when, for the double click.
    last_click: Option<(PictureKey, std::time::Duration)>,
    /// Matches of the search and the current one (SPEC §3.9).
    search: crate::search::Search,
    /// The search input is open and owns the keyboard.
    search_input: bool,
    quit: bool,
}

impl App {
    pub fn new(doc: Document, rel_path: String) -> Self {
        let message = doc
            .invalid_utf8
            .then(|| "File is not valid UTF-8".to_owned());
        let jobs = Pictures::new(&doc);
        let mut app = Self {
            doc,
            layout: Layout {
                state: State::Ready,
                rows: Vec::new(),
                index: Default::default(),
            },
            rel_path,
            size: (80, 24),
            sized: false,
            scroll: 0,
            message,
            sidebar: sidebar::Sidebar::default(),
            pictures: PictureSet::placeholder('#'),
            jobs,
            cell: CELL,
            overlays: Vec::new(),
            overlay: None,
            text_mode: false,
            focus: None,
            opener: Box::new(super::opener::Unavailable),
            fullscreen_request: None,
            view: None,
            view_sink: None,
            view_generation: 0,
            last_click: None,
            search: crate::search::Search::new(),
            search_input: false,
            quit: false,
        };
        app.relayout();
        app
    }

    /// Terminal size changed (or is known for the first time); the top row stays put.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.resize_document(width, height);
        self.resize_view();
    }

    fn resize_document(&mut self, width: u16, height: u16) {
        let first = !self.sized;
        self.sized = true;
        if self.size == (width, height) {
            if first {
                self.request_pictures();
            }
            return;
        }
        let width_changed = self.pane_width_of(self.size.0) != self.pane_width_of(width);
        // Picture slots are fitted to the pane height.
        let height_changed = self.size.1 != height;
        self.size = (width, height);
        self.reflow(width_changed, height_changed);
    }

    /// Lays the document out again after the pane changed size, keeping the
    /// top row, and refreshes the pictures whose slots changed.
    fn reflow(&mut self, width_changed: bool, height_changed: bool) {
        if width_changed {
            self.jobs.next_generation();
        }
        if width_changed || (height_changed && self.layout.rows.iter().any(|r| r.slot.is_some())) {
            let anchor = self.top_anchor();
            let before = self.slot_sizes();
            self.relayout();
            // Rasters of slots that changed size are outdated: placeholders
            // until the debounced re-rasterisation arrives (SPEC §4.21).
            let after = self.slot_sizes();
            let changed: Vec<PictureKey> = after
                .iter()
                .filter(|(key, px)| before.get(*key).is_some_and(|b| b != *px))
                .map(|(key, _)| key.clone())
                .collect();
            if self.jobs.invalidate(changed) {
                self.jobs.next_generation();
                self.relayout();
            }
            self.scroll = anchor.map_or(0, |a| self.scroll_of(a));
        }
        self.scroll = self.scroll.min(self.max_scroll());
        self.request_pictures();
    }

    /// Pixel size of every picture slot that has a known size.
    fn slot_sizes(&self) -> std::collections::HashMap<PictureKey, (u32, u32)> {
        self.layout
            .rows
            .iter()
            .filter_map(|r| r.slot.as_ref())
            .filter(|s| s.known)
            .map(|s| (s.key.clone(), s.px))
            .collect()
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
        usize::from(self.areas().document_inner.height).max(1)
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

    /// Replace the picture set (renderer choice) and lay out again.
    pub fn set_pictures(&mut self, pictures: PictureSet) {
        self.pictures = pictures;
        self.relayout_keeping_position();
    }

    /// Cell size in pixels (from terminal detection); slots are sized with it.
    pub fn set_cell(&mut self, cell: (u16, u16)) {
        self.cell = cell;
        self.relayout_keeping_position();
    }

    /// Hand a decoded picture to the pane: slots are laid out at its size.
    pub fn add_picture(&mut self, key: PictureKey, img: image::DynamicImage) {
        self.jobs
            .set_ready(key.clone(), (img.width(), img.height()));
        self.pictures.insert(key, img);
        self.relayout_keeping_position();
    }

    /// Where render jobs go (the background worker); due jobs are submitted now.
    pub fn set_job_sink(&mut self, sink: Box<dyn JobSink>) {
        self.jobs.set_sink(sink);
        self.request_pictures();
    }

    /// A render job finished: a picture goes to the picture set and the document
    /// is laid out again with the real slot size, keeping the reading position.
    pub fn on_result(&mut self, result: crate::render_worker::RenderResult) {
        let Some(arrived) = self.jobs.accept(result) else {
            return;
        };
        if let Some(arrived) = arrived {
            self.pictures.insert(
                arrived.key,
                image::DynamicImage::ImageRgba8((*arrived.image).clone()),
            );
        }
        self.relayout_keeping_position();
    }

    /// Replaces the clock the resize debounce reads (tests).
    pub fn set_clock(&mut self, clock: Box<dyn Clock>) {
        self.jobs.set_clock(clock);
    }

    /// Submits re-rasterisation once the debounce has elapsed; `true` when it did.
    pub fn tick(&mut self) -> bool {
        if self.open_view() {
            return true;
        }
        if !self.jobs.settle() {
            return false;
        }
        self.request_pictures();
        true
    }

    /// How long until the debounce is due, when one is pending.
    pub fn next_wakeup(&self) -> Option<std::time::Duration> {
        self.jobs.wakeup()
    }

    /// Whether render jobs are still outstanding.
    pub fn has_outstanding(&self) -> bool {
        self.jobs.has_outstanding() || self.view_outstanding()
    }

    fn request_pictures(&mut self) {
        if !self.sized {
            return;
        }
        self.jobs
            .request(&self.layout, self.scroll, self.viewport_height());
    }

    /// Rectangles of open overlays; pictures never draw under them.
    pub fn set_overlays(&mut self, overlays: Vec<Rect>) {
        self.overlays = overlays;
    }

    fn relayout_keeping_position(&mut self) {
        let anchor = self.top_anchor();
        self.relayout();
        self.scroll = anchor
            .map_or(0, |a| self.scroll_of(a))
            .min(self.max_scroll());
        self.request_pictures();
    }

    /// Starts (or leaves) diagram text mode, as `--text-diagrams` asks.
    pub fn set_text_mode(&mut self, on: bool) {
        if self.text_mode == on {
            return;
        }
        self.text_mode = on;
        self.jobs.set_text_mode(on);
        self.relayout_keeping_position();
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Whether a text input owns the keyboard. Letter keys are then typed as
    /// text, not run as commands: true while the search input is open.
    fn text_input_open(&self) -> bool {
        self.search_input
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        self.message = None;
        if self.view.is_some() {
            self.handle_view_key(key);
            return;
        }
        if self.overlay_open() {
            self.handle_overlay_key(key);
            self.request_pictures();
            return;
        }
        if self.text_input_open() && self.search_input_key(key) {
            self.request_pictures();
            return;
        }
        let action = match (self.text_input_open(), self.sidebar.focused) {
            (false, true) => sidebar_action(key),
            (false, false) => action_for(key),
            (true, focused) => input_action(key, focused),
        };
        let Some(action) = action else {
            return;
        };
        match action {
            Action::ScrollUp => self.scroll_by(-1),
            Action::ScrollDown => self.scroll_by(1),
            Action::PageUp => self.scroll_by(-self.page()),
            Action::PageDown => self.scroll_by(self.page()),
            Action::Top => self.scroll = 0,
            Action::Bottom => self.scroll = self.max_scroll(),
            Action::ToggleTextDiagrams => {
                self.set_text_mode(!self.text_mode);
                return;
            }
            Action::FocusNext => self.focus_step(true),
            Action::FocusPrev => self.focus_step(false),
            Action::Escape => self.escape(),
            Action::OpenToc => self.open_toc(),
            Action::OpenHelp => {}
            Action::SearchOpen => self.open_search(),
            Action::SearchNext => self.search_step(true),
            Action::SearchPrev => self.search_step(false),
            Action::Activate => self.activate(),
            Action::Quit => self.quit = true,
            Action::ToggleSidebar => self.toggle_sidebar(),
            Action::FocusSidebar => self.focus_sidebar(true),
            Action::FocusDocument => self.focus_sidebar(false),
            Action::Nav(key) => self.sidebar_key(key),
            _ => {}
        }
        self.request_pictures();
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.view.is_some() {
            return;
        }
        let rows = WHEEL_ROWS as isize;
        match mouse.kind {
            MouseEventKind::ScrollDown => self.scroll_by(rows),
            MouseEventKind::ScrollUp => self.scroll_by(-rows),
            MouseEventKind::Down(_) => self.click(mouse),
            _ => {}
        }
        self.request_pictures();
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.resize(area.width, area.height);
        if self.view.is_some() {
            self.render_view(frame);
            return;
        }
        let Areas {
            sidebar,
            document,
            document_inner: inner,
            status,
        } = areas(area, self.sidebar.visible);
        // Paint every cell first: later widgets only patch the foreground.
        frame.render_widget(Block::new().style(theme::page()), area);
        if sidebar.width > 0 {
            self.draw_sidebar(frame, sidebar);
        }
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(theme::border(!self.sidebar.focused));
        frame.render_widget(block, document);
        frame.render_widget(Paragraph::new(self.pane_lines(inner.height)), inner);
        self.draw_pictures(inner, frame.buffer_mut());
        self.draw_search(inner, frame.buffer_mut());
        self.draw_focus(inner, frame.buffer_mut());
        self.draw_overlay(frame);
        frame.render_widget(Paragraph::new(self.status_line(status.width)), status);
    }

    /// Draws the pictures whose slots touch the viewport, clipped to `pane`.
    fn draw_pictures(&mut self, pane: Rect, buf: &mut ratatui::buffer::Buffer) {
        if self.layout.state != State::Ready {
            return;
        }
        // A slot is at most one pane tall, so it starts within a pane height above.
        let height = usize::from(pane.height);
        let overlays = [self.overlays.clone(), self.overlay_rects()].concat();
        let first = self.scroll.saturating_sub(height);
        let last = (self.scroll + height).min(self.layout.rows.len());
        for index in first..last {
            let Some(slot) = self.layout.rows[index].slot.as_ref() else {
                continue;
            };
            let slot_rows = usize::from(slot.rows);
            if !slot.known || self.jobs.is_stale(&slot.key) || index + slot_rows <= self.scroll {
                continue;
            }
            let (Ok(row), Ok(scroll)) = (i32::try_from(index), i32::try_from(self.scroll)) else {
                continue;
            };
            let request = PictureRequest {
                pane,
                slot_top: i32::from(pane.y) + row - scroll,
                slot_left: pane.x.saturating_add(MARGIN).saturating_add(slot.left),
                slot_cols: slot.cols,
                slot_rows: slot.rows,
                overlays: &overlays,
            };
            self.pictures.draw(slot, &request, buf);
        }
    }

    /// The main view's areas for the current terminal size.
    fn areas(&self) -> Areas {
        areas(self.terminal_rect(), self.sidebar.visible)
    }

    fn terminal_rect(&self) -> Rect {
        Rect::new(0, 0, self.size.0, self.size.1)
    }

    /// Columns the document gets inside its border on a terminal `terminal_width` wide.
    fn pane_width_of(&self, terminal_width: u16) -> u16 {
        areas(
            Rect::new(0, 0, terminal_width, self.size.1),
            self.sidebar.visible,
        )
        .document_inner
        .width
    }

    fn relayout(&mut self) {
        self.layout = layout::layout_diagrams(
            &self.doc,
            self.pane_width_of(self.size.0),
            self.viewport_height().try_into().unwrap_or(u16::MAX),
            self.cell,
            &self.jobs,
            Diagrams {
                states: &self.jobs,
                text_mode: self.text_mode,
            },
        );
        self.validate_focus();
        self.search.relayout(&self.layout);
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
            State::Empty if self.rel_path.is_empty() => notice("No supported files in this folder"),
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
            _ if self.search_input => format!(" Search: {}_", self.search.query()),
            Some(message) => format!(" {} | {message}", self.rel_path),
            None => format!(" {}", self.rel_path),
        };
        let mode = if self.text_mode {
            "Text diagrams | "
        } else {
            ""
        };
        let found = self
            .search_status()
            .map(|s| format!("{s} | "))
            .unwrap_or_default();
        let right = format!("{found}{mode}{}% ", self.percent());
        let pad = usize::from(width).saturating_sub(left.width() + right.width());
        let text = format!("{left}{}{right}", " ".repeat(pad));
        Line::styled(text, theme::status())
    }
}
