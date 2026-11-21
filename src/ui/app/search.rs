//! Search input, highlights and match navigation (SPEC §3.9).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{buffer::Buffer, layout::Rect, style::Modifier};

use super::App;
use crate::layout::State;
use crate::search::Search;
use crate::theme;

impl App {
    /// Ctrl+F: the input opens on the current query, so it can be refined.
    pub(super) fn open_search(&mut self) {
        self.search_input = true;
        self.focus_sidebar(false);
    }

    /// Keys of the open input: text, Backspace, Enter and Esc. Returns whether
    /// the key was consumed; the others (scrolling, Ctrl+F) take the normal path.
    pub(super) fn search_input_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let mut query = self.search.query().to_owned();
                query.push(c);
                self.update_query(&query);
            }
            KeyCode::Backspace => {
                let mut query = self.search.query().to_owned();
                query.pop();
                self.update_query(&query);
            }
            KeyCode::Enter => self.search_input = false,
            KeyCode::Esc => self.escape(),
            _ => return false,
        }
        true
    }

    fn update_query(&mut self, query: &str) {
        self.search.set_query(&self.layout, query);
        self.scroll_match_into_view();
    }

    /// Esc priority: the search input, then the search highlights, then the
    /// focus. Overlays (KB-56, KB-57) go in front of this chain.
    pub(super) fn escape(&mut self) {
        if self.search_input || self.search_active() {
            self.clear_search();
        } else {
            self.clear_focus();
        }
    }

    /// Closes the input and drops the query and its highlights.
    pub(super) fn clear_search(&mut self) {
        self.search_input = false;
        self.search = Search::new();
    }

    /// Highlights are showing: a query is set, even one without matches.
    fn search_active(&self) -> bool {
        !self.search.query().is_empty()
    }

    /// `n` / `N`. Without a search they say so, since a silent key looks broken.
    pub(super) fn search_step(&mut self, forward: bool) {
        if !self.search_active() {
            self.set_message("No active search (Ctrl+F)");
            return;
        }
        if forward {
            self.search.next();
        } else {
            self.search.prev();
        }
        self.scroll_match_into_view();
    }

    /// `match i/n` or `No matches` while highlights are active.
    pub(super) fn search_status(&self) -> Option<String> {
        if !self.search_active() {
            return None;
        }
        Some(match self.search.current_index() {
            Some(i) => format!("match {}/{}", i + 1, self.search.matches().len()),
            None => "No matches".to_owned(),
        })
    }

    /// Minimal scroll that shows the rows of the current match; a match that
    /// is off screen lands a third of the way down the pane.
    fn scroll_match_into_view(&mut self) {
        let Some(m) = self.search.current() else {
            return;
        };
        let (Some(first), Some(last)) = (
            m.fragments.iter().map(|f| f.row).min(),
            m.fragments.iter().map(|f| f.row).max(),
        ) else {
            return;
        };
        let height = self.viewport_height();
        if first < self.scroll || last >= self.scroll + height {
            self.scroll = first.saturating_sub(height / 3).min(self.max_scroll());
        }
    }

    /// Overlays the match colours on the cells already drawn: the text stays,
    /// the style changes, so every wrapped fragment gets it.
    pub(super) fn draw_search(&self, pane: Rect, buf: &mut Buffer) {
        if self.layout.state != State::Ready {
            return;
        }
        let current = self.search.current_index();
        for (i, m) in self.search.matches().iter().enumerate() {
            let is_current = current == Some(i);
            for f in &m.fragments {
                for col in f.start..f.end {
                    let Some(pos) = self.cell_at(pane, f.row, i32::from(col)) else {
                        continue;
                    };
                    let cell = &mut buf[pos];
                    if is_current {
                        cell.fg = theme::CURRENT_MATCH_FG;
                        cell.bg = theme::CURRENT_MATCH_BG;
                        cell.modifier.insert(Modifier::BOLD);
                    } else {
                        cell.fg = theme::MATCH_FG;
                        cell.bg = theme::MATCH_BG;
                    }
                }
            }
        }
    }
}
