//! Table-of-contents overlay (SPEC §3.10).

use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};
use unicode_width::UnicodeWidthChar;

use super::{App, Overlay};
use crate::theme;
use crate::ui::keymap::{Action, toc_action};
use crate::ui::overlay;

/// Columns of indent per heading level below H1.
const INDENT: usize = 2;
/// Narrowest popup, border included.
const MIN_WIDTH: u16 = 24;

/// State of the open TOC: the selected heading and the first visible entry.
#[derive(Debug, Default)]
pub(super) struct Toc {
    selected: usize,
    top: usize,
}

impl App {
    /// `t`: opens the TOC with the heading nearest above the scroll position selected.
    pub(super) fn open_toc(&mut self) {
        let headings = &self.layout.index.headings;
        let selected = headings
            .iter()
            .rposition(|h| h.row <= self.scroll)
            .unwrap_or(0);
        self.overlay = Some(Overlay::Toc(Toc { selected, top: 0 }));
        self.keep_toc_selection_visible();
    }

    /// Keys while an overlay is open: only its own bindings apply.
    pub(super) fn handle_overlay_key(&mut self, key: KeyEvent) {
        match toc_action(key) {
            Some(Action::TocUp) => self.move_toc(-1),
            Some(Action::TocDown) => self.move_toc(1),
            Some(Action::TocJump) => self.toc_jump(),
            Some(Action::CloseOverlay) => self.escape(),
            _ => {}
        }
    }

    fn move_toc(&mut self, delta: isize) {
        let last = self.layout.index.headings.len().saturating_sub(1);
        if let Some(Overlay::Toc(toc)) = &mut self.overlay {
            toc.selected = toc.selected.saturating_add_signed(delta).min(last);
        }
        self.keep_toc_selection_visible();
    }

    /// Enter: the heading goes to the top of the pane, as far as the end allows.
    fn toc_jump(&mut self) {
        let Some(Overlay::Toc(toc)) = self.overlay.take() else {
            return;
        };
        if let Some(h) = self.layout.index.headings.get(toc.selected) {
            self.scroll = h.row.min(self.max_scroll());
        }
    }

    /// Scrolls the entry list so the selection shows.
    fn keep_toc_selection_visible(&mut self) {
        let height = usize::from(self.toc_rect().height.saturating_sub(2)).max(1);
        if let Some(Overlay::Toc(toc)) = &mut self.overlay {
            if toc.selected < toc.top {
                toc.top = toc.selected;
            } else if toc.selected >= toc.top + height {
                toc.top = toc.selected + 1 - height;
            }
        }
    }

    /// Entry texts, indented by level.
    pub(super) fn toc_lines(&self) -> Vec<String> {
        self.layout
            .index
            .headings
            .iter()
            .map(|h| {
                let indent = usize::from(h.level.saturating_sub(1)) * INDENT;
                format!("{}{}", " ".repeat(indent), h.text)
            })
            .collect()
    }

    /// Selected entry of the open TOC.
    #[cfg(test)]
    pub(super) fn toc_selected(&self) -> Option<usize> {
        self.overlay.as_ref().map(|Overlay::Toc(toc)| toc.selected)
    }

    /// Whether an overlay owns the keyboard.
    pub(super) fn overlay_open(&self) -> bool {
        self.overlay.is_some()
    }

    /// Popup rectangle: centred in the document pane, as wide as the longest
    /// entry (at least `MIN_WIDTH`) and as tall as the list, capped to the pane.
    fn toc_rect(&self) -> Rect {
        let pane = self.areas().document_inner;
        let widest = self
            .toc_lines()
            .iter()
            .map(|l| l.chars().filter_map(UnicodeWidthChar::width).sum::<usize>())
            .max()
            .unwrap_or("No headings".len());
        let width = u16::try_from(widest + 4) // one padding column and one border per side
            .unwrap_or(u16::MAX)
            .clamp(MIN_WIDTH, pane.width.max(1))
            .min(pane.width);
        let rows = u16::try_from(self.layout.index.headings.len().max(1)).unwrap_or(u16::MAX);
        let height = rows.saturating_add(2).min(pane.height);
        Rect::new(
            pane.x + (pane.width - width) / 2,
            pane.y + (pane.height - height) / 2,
            width,
            height,
        )
    }

    /// Rectangles of the overlays on screen.
    pub(super) fn overlay_rects(&self) -> Vec<Rect> {
        match self.overlay {
            Some(Overlay::Toc(_)) if self.view.is_none() => vec![self.toc_rect()],
            _ => Vec::new(),
        }
    }

    pub(super) fn draw_overlay(&mut self, frame: &mut Frame) {
        let Some(Overlay::Toc(toc)) = &self.overlay else {
            return;
        };
        let (selected, top) = (toc.selected, toc.top);
        let inner = overlay::frame(frame, self.toc_rect(), " Contents ");
        let width = usize::from(inner.width).saturating_sub(2);
        let lines = self.toc_lines();
        let content: Vec<Line> = if lines.is_empty() {
            vec![Line::styled(" No headings", theme::dim())]
        } else {
            lines
                .iter()
                .enumerate()
                .skip(top)
                .take(usize::from(inner.height))
                .map(|(i, text)| {
                    let text = clip(text, width);
                    let line = format!(" {text:<width$} ");
                    if i == selected {
                        Line::styled(line, theme::selected(true))
                    } else {
                        Line::styled(line, theme::body())
                    }
                })
                .collect()
        };
        frame.render_widget(Paragraph::new(content), inner);
    }
}

/// `text` cut to at most `width` display columns.
fn clip(text: &str, width: usize) -> String {
    let mut used = 0;
    text.chars()
        .take_while(|c| {
            used += c.width().unwrap_or(0);
            used <= width
        })
        .collect()
}
