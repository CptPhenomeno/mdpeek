//! Sidebar pane: visibility, focus switching, drawing and opening files
//! (SPEC §3.2, §3.7).

use std::path::{Path, PathBuf};

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{App, BORDER, areas::SIDEBAR_COLS};
use crate::doc::Document;
use crate::theme;
use crate::tree::{
    self, Start,
    nav::{NavKey, Outcome, Row, SidebarNav},
};

/// State of the sidebar pane.
#[derive(Debug, Default)]
pub(super) struct Sidebar {
    pub(super) visible: bool,
    /// The sidebar, not the document, has the keyboard focus.
    pub(super) focused: bool,
    root: PathBuf,
    open_file: Option<PathBuf>,
    /// `None` until the tree is scanned (a file start scans on first show).
    nav: Option<SidebarNav>,
    /// First row drawn.
    top: usize,
}

impl App {
    /// Applies the start-up decision: root, visibility, tree and open file.
    pub fn set_start(&mut self, start: Start) {
        self.sidebar.nav = start
            .tree
            .map(|nodes| SidebarNav::new(nodes, start.initial_file.as_deref()));
        self.sidebar.root = start.root;
        self.sidebar.open_file = start.initial_file;
        self.set_sidebar_visible(start.sidebar_visible);
    }

    /// Whether the sidebar is shown.
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar.visible
    }

    /// Whether the sidebar (not the document) has the keyboard focus.
    pub fn sidebar_focused(&self) -> bool {
        self.sidebar.focused
    }

    /// Path shown in the status bar, relative to the root.
    pub fn rel_path(&self) -> &str {
        &self.rel_path
    }

    /// Columns available to the document inside its border.
    pub fn document_width(&self) -> u16 {
        self.pane_width_of(self.size.0)
    }

    pub(super) fn toggle_sidebar(&mut self) {
        self.set_sidebar_visible(!self.sidebar.visible);
    }

    fn set_sidebar_visible(&mut self, visible: bool) {
        if self.sidebar.visible == visible {
            return;
        }
        let old = self.pane_width_of(self.size.0);
        self.sidebar.visible = visible;
        if visible {
            self.ensure_tree();
        } else {
            // Hiding the focused sidebar hands the focus to the document.
            self.sidebar.focused = false;
        }
        if old != self.pane_width_of(self.size.0) {
            self.reflow(true, false);
        }
    }

    /// Scans the tree the first time it is needed; a failure leaves it empty.
    fn ensure_tree(&mut self) {
        if self.sidebar.nav.is_some() {
            return;
        }
        let nodes = tree::scan(&self.sidebar.root).unwrap_or_else(|err| {
            self.message = Some(format!("Cannot read folder: {err}"));
            Vec::new()
        });
        self.sidebar.nav = Some(SidebarNav::new(nodes, self.sidebar.open_file.as_deref()));
    }

    /// Ctrl+Left / Ctrl+Right: moves the focus to the sidebar or the document;
    /// it does nothing while the sidebar is hidden.
    pub(super) fn focus_sidebar(&mut self, sidebar: bool) {
        if self.sidebar.visible {
            self.sidebar.focused = sidebar;
        }
    }

    /// A navigation key while the sidebar has focus.
    pub(super) fn sidebar_key(&mut self, key: NavKey) {
        let Some(nav) = self.sidebar.nav.as_mut() else {
            return;
        };
        if let Outcome::Open(path) = nav.handle(key) {
            self.open_file(&path);
        }
    }

    /// Shows `path` in the document pane; the sidebar keeps the focus. A load
    /// error leaves the current document and says so in the status bar.
    fn open_file(&mut self, path: &Path) {
        let doc = match Document::from_path(path) {
            Ok(doc) => doc,
            Err(err) => {
                self.message = Some(format!("Cannot open {}: {err}", self.relative(path)));
                return;
            }
        };
        self.message = doc
            .invalid_utf8
            .then(|| "File is not valid UTF-8".to_owned());
        // Pictures of the old document must not show: a new generation drops
        // late results, and the picture set starts empty.
        self.jobs.reset_for(&doc);
        self.pictures.clear();
        self.doc = doc;
        self.rel_path = self.relative(path);
        self.scroll = 0;
        self.focus = None;
        self.clear_search();
        self.fullscreen_request = None;
        self.relayout();
        self.request_pictures();
        self.sidebar.open_file = Some(path.to_path_buf());
        if let Some(nav) = self.sidebar.nav.as_mut() {
            nav.set_open_file(Some(path));
        }
    }

    /// `r`: re-reads the current file and the sidebar tree.
    ///
    /// The tree is rescanned only when it was already scanned (a file start
    /// whose sidebar was never shown stays lazy). A file that cannot be read
    /// leaves the current content and says so in the status bar (SPEC §4.23).
    pub(super) fn reload(&mut self) {
        if let Some(nav) = self.sidebar.nav.as_mut() {
            match tree::scan(&self.sidebar.root) {
                Ok(nodes) => nav.rebuild(nodes),
                Err(err) => self.message = Some(format!("Cannot read folder: {err}")),
            }
        }
        let Some(path) = self.sidebar.open_file.clone() else {
            return;
        };
        let doc = match Document::from_path(&path) {
            Ok(doc) => doc,
            Err(err) => {
                self.message = Some(format!("Cannot reload {}: {err}", self.relative(&path)));
                return;
            }
        };
        self.message = Some(if doc.invalid_utf8 {
            "Reloaded | File is not valid UTF-8".to_owned()
        } else {
            "Reloaded".to_owned()
        });
        for key in self.jobs.reload_for(&doc) {
            self.pictures.remove(&key);
        }
        self.doc = doc;
        self.relayout();
        self.scroll = self.scroll.min(self.max_scroll());
        self.request_pictures();
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.sidebar.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    /// Draws the sidebar in `area`, scrolled to keep the selection visible.
    pub(super) fn draw_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .borders(Borders::ALL)
            .style(theme::page())
            .border_style(theme::border(self.sidebar.focused));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let height = usize::from(inner.height);
        let width = usize::from(inner.width);
        let focused = self.sidebar.focused;
        let Some(nav) = self.sidebar.nav.as_ref() else {
            return;
        };
        let rows = nav.rows();
        let selected = nav.selected_index();
        let mut top = self.sidebar.top;
        if let Some(sel) = selected {
            if sel < top {
                top = sel;
            } else if height > 0 && sel >= top + height {
                top = sel + 1 - height;
            }
        }
        top = top.min(rows.len().saturating_sub(height));
        self.sidebar.top = top;
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(top)
            .take(height)
            .map(|(i, row)| {
                let style = if Some(i) == selected {
                    theme::selected(focused)
                } else if row.is_dir {
                    theme::heading(3).remove_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    theme::body()
                };
                let text = row_text(row, width);
                let pad = width.saturating_sub(text.width());
                Line::from(Span::styled(format!("{text}{}", " ".repeat(pad)), style))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// `indent marker name`, cut with an ellipsis to `width` columns.
fn row_text(row: &Row, width: usize) -> String {
    let marker = if row.is_dir {
        if row.expanded { "▾ " } else { "▸ " }
    } else if row.open {
        "● "
    } else {
        "  "
    };
    let text = format!("{}{marker}{}", "  ".repeat(row.depth), row.name);
    truncate(&text, width)
}

fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

const _: () = assert!(SIDEBAR_COLS > BORDER);
