//! Application state and key handling.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{Frame, text::Line, widgets::Paragraph};

/// State of the plain-text viewer.
#[derive(Debug)]
pub struct App {
    lines: Vec<String>,
    scroll: usize,
    viewport_height: usize,
    quit: bool,
}

impl App {
    pub fn new(lines: Vec<String>) -> Self {
        Self {
            lines,
            scroll: 0,
            viewport_height: 1,
            quit: false,
        }
    }

    /// Index of the first visible row.
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn set_viewport_height(&mut self, rows: usize) {
        self.viewport_height = rows.max(1);
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn max_scroll(&self) -> usize {
        self.lines.len().saturating_sub(self.viewport_height)
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        match key.code {
            KeyCode::Down => self.scroll = (self.scroll + 1).min(self.max_scroll()),
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.quit = true;
            }
            _ => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.set_viewport_height(usize::from(area.height));
        let rows: Vec<Line> = self
            .lines
            .iter()
            .skip(self.scroll)
            .map(|l| Line::raw(l.as_str()))
            .collect();
        frame.render_widget(Paragraph::new(rows), area);
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};

    use super::App;

    fn app(n: usize, viewport: usize) -> App {
        let mut app = App::new((0..n).map(|i| format!("line {i}")).collect());
        app.set_viewport_height(viewport);
        app
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn down_scrolls_one_row_and_stops_at_the_last_row() {
        let mut app = app(5, 3);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.scroll(), 1);
        for _ in 0..10 {
            press(&mut app, KeyCode::Down);
        }
        // 5 lines, 3 visible: the last page starts at row 2.
        assert_eq!(app.scroll(), 2);
    }

    #[test]
    fn up_does_not_scroll_above_the_first_row() {
        let mut app = app(5, 3);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.scroll(), 0);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.scroll(), 0);
    }

    #[test]
    fn ctrl_q_sets_the_quit_flag() {
        let mut app = app(1, 3);
        assert!(!app.should_quit());
        press(&mut app, KeyCode::Char('q'));
        assert!(!app.should_quit());
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(app.should_quit());
    }

    #[test]
    fn test_backend_snapshot_shows_first_rows_as_plain_text() {
        let mut app = App::new(vec![
            "# Title".into(),
            "".into(),
            "some text".into(),
            "more".into(),
        ]);
        let mut terminal = Terminal::new(TestBackend::new(12, 3)).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        insta::assert_snapshot!(terminal.backend());
    }
}
