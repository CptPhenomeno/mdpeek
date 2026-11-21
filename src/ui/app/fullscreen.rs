//! The full-screen diagram view (SPEC §3.6.3).
//!
//! It replaces everything but the status bar. The picture is drawn by the
//! view worker from the SVG, only for the visible region at the current
//! zoom, and shown centred. While a new raster is on its way the previous
//! one stays on screen; replies of an older generation are dropped. In
//! diagram text mode the view shows the text output instead.

use std::path::PathBuf;

use crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{App, Areas, areas};
use crate::graphics::PictureRequest;
use crate::layout::{DiagramState, DiagramStates, PictureKey, Slot};
use crate::theme;
use crate::ui::fullscreen::{PanDir, ZoomState};
use crate::ui::keymap::{Action, Context, action_for, action_in};
use crate::ui::view_worker::{Region, Reply, Request, ViewSink};

/// Key under which the view's raster lives in the picture set.
fn view_key() -> PictureKey {
    PictureKey::Image(PathBuf::from("\0fullscreen-view"))
}

/// State of the open view.
#[derive(Debug)]
pub struct View {
    source: String,
    diagram: usize,
    /// Diagram text mode: no picture, the text output is shown.
    text: bool,
    /// Known once the natural size arrived.
    zoom: Option<ZoomState>,
    /// Screen area in pixels the zoom state was made for.
    screen_px: (u32, u32),
    /// Pixel size of the raster currently in the picture set.
    image_px: Option<(u32, u32)>,
    /// Generation of the request being waited for.
    generation: u64,
    pending: bool,
    failure: Option<String>,
    text_scroll: usize,
}

impl App {
    /// Where the view sends its render requests (the background worker).
    pub fn set_view_sink(&mut self, sink: Box<dyn ViewSink>) {
        self.view_sink = Some(sink);
    }

    /// Whether the full-screen view is open.
    pub fn view_open(&self) -> bool {
        self.view.is_some()
    }

    pub(super) fn view_outstanding(&self) -> bool {
        self.view.as_ref().is_some_and(|v| v.pending)
    }

    /// Takes the pending full-screen request and opens the view; `true` if it did.
    pub(super) fn open_view(&mut self) -> bool {
        let Some(key) = self.fullscreen_request.take() else {
            return false;
        };
        let PictureKey::Diagram(diagram) = key else {
            return false;
        };
        let Some(source) = self.jobs.diagram_source(diagram).map(str::to_owned) else {
            return false;
        };
        let text = self.text_mode;
        self.view = Some(View {
            source,
            diagram,
            text,
            zoom: None,
            screen_px: self.view_screen_px(),
            image_px: None,
            generation: 0,
            pending: false,
            failure: None,
            text_scroll: 0,
        });
        if !text {
            self.submit_view(None);
        }
        true
    }

    /// Area under the status bar, in pixels.
    fn view_screen_px(&self) -> (u32, u32) {
        let pane = areas(self.terminal_rect(), false).document;
        (
            u32::from(pane.width) * u32::from(self.cell.0),
            u32::from(pane.height) * u32::from(self.cell.1),
        )
    }

    fn submit_view(&mut self, region: Option<Region>) {
        let (Some(view), Some(sink)) = (self.view.as_mut(), self.view_sink.as_ref()) else {
            return;
        };
        self.view_generation += 1;
        view.generation = self.view_generation;
        view.pending = true;
        sink.submit(Request {
            generation: view.generation,
            source: view.source.clone(),
            region,
        });
    }

    /// Asks for the raster of what the zoom state shows.
    fn request_region(&mut self) {
        let Some(zoom) = self.view.as_ref().and_then(|v| v.zoom.as_ref()) else {
            return;
        };
        let rect = zoom.source_rect();
        let dest = zoom.dest_size();
        let region = Region {
            origin: (rect.x, rect.y),
            scale: zoom.scale(),
            px: (
                dest.0.round().max(1.0) as u32,
                dest.1.round().max(1.0) as u32,
            ),
        };
        self.submit_view(Some(region));
    }

    /// The terminal changed size: the zoom state follows and the raster is redone.
    pub(super) fn resize_view(&mut self) {
        let screen_px = self.view_screen_px();
        let Some(view) = self.view.as_mut() else {
            return;
        };
        if view.screen_px == screen_px {
            return;
        }
        view.screen_px = screen_px;
        if let Some(zoom) = view.zoom.as_mut() {
            zoom.resize((f64::from(screen_px.0), f64::from(screen_px.1)));
            self.request_region();
        }
    }

    /// A render reply of the view arrived; replies of an old generation are dropped.
    pub fn on_view_reply(&mut self, reply: Reply) {
        let screen_px = self.view_screen_px();
        let Some(view) = self.view.as_mut() else {
            return;
        };
        if reply.generation() != view.generation {
            return;
        }
        view.pending = false;
        match reply {
            Reply::Natural { size, .. } => {
                view.zoom = Some(ZoomState::new(
                    (f64::from(size.0), f64::from(size.1)),
                    (f64::from(screen_px.0), f64::from(screen_px.1)),
                ));
                self.request_region();
            }
            Reply::Image { image, .. } => {
                view.image_px = Some((image.width(), image.height()));
                self.pictures.insert(
                    view_key(),
                    image::DynamicImage::ImageRgba8((*image).clone()),
                );
            }
            Reply::Error { message, .. } => view.failure = Some(message),
        }
    }

    pub(super) fn handle_view_key(&mut self, key: KeyEvent) {
        let Some(action) = action_in(Context::Fullscreen, key) else {
            // Quitting stays possible from the view.
            if action_for(key) == Some(Action::Quit) {
                self.quit = true;
            }
            return;
        };
        if action == Action::CloseFullscreen {
            self.close_view();
            return;
        }
        let text = self.view.as_ref().is_some_and(|v| v.text);
        if text {
            self.scroll_view_text(action);
            return;
        }
        let Some(zoom) = self.view.as_mut().and_then(|v| v.zoom.as_mut()) else {
            return;
        };
        match action {
            Action::ZoomIn => zoom.zoom_in(),
            Action::ZoomOut => zoom.zoom_out(),
            Action::ZoomReset => zoom.reset(),
            Action::PanLeft => zoom.pan(PanDir::Left),
            Action::PanRight => zoom.pan(PanDir::Right),
            Action::PanUp => zoom.pan(PanDir::Up),
            Action::PanDown => zoom.pan(PanDir::Down),
            _ => return,
        }
        self.request_region();
    }

    fn close_view(&mut self) {
        self.view = None;
        self.pictures.remove(&view_key());
    }

    fn scroll_view_text(&mut self, action: Action) {
        let max = self.view_text_max_scroll();
        let Some(view) = self.view.as_mut() else {
            return;
        };
        match action {
            Action::PanUp => view.text_scroll = view.text_scroll.saturating_sub(1),
            Action::PanDown => view.text_scroll = (view.text_scroll + 1).min(max),
            Action::ZoomReset => view.text_scroll = 0,
            _ => {}
        }
    }

    fn view_text_max_scroll(&self) -> usize {
        let rows = usize::from(areas(self.terminal_rect(), false).document.height);
        self.view_text_lines().0.len().saturating_sub(rows)
    }

    /// Lines of the text view and whether they are an error.
    fn view_text_lines(&self) -> (Vec<String>, ratatui::style::Style) {
        let Some(view) = self.view.as_ref() else {
            return (Vec::new(), theme::body());
        };
        let lines = |t: &str| {
            t.trim_end_matches('\n')
                .lines()
                .map(str::to_owned)
                .collect()
        };
        match self.jobs.state(view.diagram) {
            DiagramState::Text(t) => (lines(&t), theme::body()),
            DiagramState::TextUnavailable => {
                let mut out: Vec<String> = lines(&view.source);
                out.push(String::new());
                out.push("Text rendering not available for this diagram type".to_owned());
                (out, theme::body())
            }
            DiagramState::Error(m) => (lines(&m), theme::error()),
            _ => (vec!["Rendering...".to_owned()], theme::dim()),
        }
    }

    pub(super) fn render_view(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let Areas {
            document: pane,
            status,
            ..
        } = areas(area, false);
        frame.render_widget(Block::new().style(theme::page()), area);
        self.draw_view_body(pane, frame);
        frame.render_widget(Paragraph::new(self.view_status(area.width)), status);
    }

    fn draw_view_body(&mut self, pane: Rect, frame: &mut Frame) {
        let Some(view) = self.view.as_ref() else {
            return;
        };
        if view.text {
            let (lines, style) = self.view_text_lines();
            let scroll = view.text_scroll.min(self.view_text_max_scroll());
            let shown: Vec<Line> = lines
                .into_iter()
                .skip(scroll)
                .take(usize::from(pane.height))
                .map(|l| Line::styled(l, style))
                .collect();
            frame.render_widget(Paragraph::new(shown), pane);
            return;
        }
        if let Some(px) = view.image_px {
            let cols =
                px.0.div_ceil(u32::from(self.cell.0))
                    .clamp(1, u32::from(pane.width.max(1)));
            let cell_rows =
                px.1.div_ceil(u32::from(self.cell.1))
                    .clamp(1, u32::from(pane.height.max(1)));
            let (cols, cell_rows) = (cols as u16, cell_rows as u16);
            let left = (pane.width - cols.min(pane.width)) / 2;
            let top = (pane.height - cell_rows.min(pane.height)) / 2;
            let slot = Slot {
                key: view_key(),
                cols,
                rows: cell_rows,
                px,
                left,
                known: true,
            };
            let request = PictureRequest {
                pane,
                slot_top: i32::from(pane.y) + i32::from(top),
                slot_left: pane.x + left,
                slot_cols: cols,
                slot_rows: cell_rows,
                overlays: &[],
            };
            self.pictures.draw(&slot, &request, frame.buffer_mut());
            return;
        }
        let (text, style) = match &view.failure {
            Some(message) => (message.as_str(), theme::error()),
            None => ("Rendering...", theme::dim()),
        };
        let y = pane.y + pane.height / 2;
        let line = Rect::new(pane.x, y, pane.width, 1.min(pane.height));
        frame.render_widget(
            Paragraph::new(Line::styled(text.to_owned(), style)).centered(),
            line,
        );
    }

    fn view_status(&self, width: u16) -> Line<'static> {
        let right = match self.view.as_ref() {
            Some(v) if v.text => "Text diagram | Esc: back ".to_owned(),
            Some(v) => format!(
                "Zoom {}% | Esc: back ",
                v.zoom.as_ref().map_or(100, ZoomState::percent)
            ),
            None => String::new(),
        };
        let left = format!(" {}", self.rel_path);
        let pad = usize::from(width).saturating_sub(left.width() + right.width());
        Line::styled(format!("{left}{}{right}", " ".repeat(pad)), theme::status())
    }
}
