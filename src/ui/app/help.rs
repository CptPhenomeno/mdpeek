//! Help overlay (SPEC §3.8), generated from the key map.

use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};
use unicode_width::UnicodeWidthStr;

use super::{App, Overlay};
use crate::theme;
use crate::ui::keymap::{Action, BINDINGS, Context, help_action};
use crate::ui::overlay;

/// Narrowest popup, border included.
const MIN_WIDTH: u16 = 30;
/// Spaces between the key column and the description.
const GAP: usize = 2;

/// State of the open help: the first visible line.
#[derive(Debug, Default)]
pub(super) struct Help {
    top: usize,
}

impl App {
    /// `?`: opens the help at its first line.
    pub(super) fn open_help(&mut self) {
        self.overlay = Some(Overlay::Help(Help::default()));
    }

    /// Lines of the help text: a heading per context, then one row per binding.
    pub(super) fn help_lines(&self) -> Vec<String> {
        let key_width = BINDINGS.iter().map(|b| b.label.width()).max().unwrap_or(0);
        let mut lines = Vec::new();
        for (context, heading) in Context::HELP_ORDER {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(heading.to_owned());
            for b in BINDINGS.iter().filter(|b| b.context == context) {
                let pad = " ".repeat(key_width - b.label.width() + GAP);
                lines.push(format!("  {}{pad}{}", b.label, b.description));
            }
        }
        lines
    }

    pub(super) fn handle_help_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.help_rect().height.saturating_sub(3)).max(1);
        let max = self.help_max_top();
        let Some(Overlay::Help(help)) = &mut self.overlay else {
            return;
        };
        match help_action(key) {
            Some(Action::HelpUp) => help.top = help.top.saturating_sub(1),
            Some(Action::HelpDown) => help.top = (help.top + 1).min(max),
            Some(Action::HelpPageUp) => help.top = help.top.saturating_sub(page),
            Some(Action::HelpPageDown) => help.top = (help.top + page).min(max),
            Some(Action::CloseOverlay) => self.escape(),
            _ => {}
        }
    }

    fn help_max_top(&self) -> usize {
        let rows = usize::from(self.help_rect().height.saturating_sub(2));
        self.help_lines().len().saturating_sub(rows)
    }

    /// Popup rectangle: centred in the document pane, sized to the text and
    /// capped to the pane.
    pub(super) fn help_rect(&self) -> Rect {
        let pane = self.areas().document_inner;
        let widest = self
            .help_lines()
            .iter()
            .map(|l| l.width())
            .max()
            .unwrap_or(0);
        let width = u16::try_from(widest + 4)
            .unwrap_or(u16::MAX)
            .max(MIN_WIDTH)
            .min(pane.width);
        let rows = u16::try_from(self.help_lines().len()).unwrap_or(u16::MAX);
        let height = rows.saturating_add(2).min(pane.height);
        Rect::new(
            pane.x + (pane.width - width) / 2,
            pane.y + (pane.height - height) / 2,
            width,
            height,
        )
    }

    pub(super) fn draw_help(&mut self, frame: &mut Frame) {
        let Some(Overlay::Help(help)) = &self.overlay else {
            return;
        };
        let top = help.top;
        let inner = overlay::frame(frame, self.help_rect(), " Help ");
        let headings: Vec<&str> = Context::HELP_ORDER.iter().map(|(_, h)| *h).collect();
        let content: Vec<Line> = self
            .help_lines()
            .into_iter()
            .skip(top)
            .take(usize::from(inner.height))
            .map(|l| {
                if headings.contains(&l.as_str()) {
                    Line::styled(format!(" {l}"), theme::heading(2))
                } else {
                    Line::styled(format!(" {l}"), theme::body())
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(content), inner);
    }
}
