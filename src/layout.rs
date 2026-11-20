//! Document model + pane width -> rows of styled spans (SPEC §3.3, §3.11).

use ratatui::style::Style;

use crate::doc::{Block, Document, ImageTarget, Inline, List};
use crate::theme;
use unicode_width::UnicodeWidthChar;
use wrap::{Piece, chunk, wrap};

mod index;
mod table;
mod wrap;

pub use index::{Focusable, Fragment, HeadingEntry, Index, TextBlock};

#[cfg(test)]
mod tests;

/// Narrowest pane that still shows a document (SPEC §4.6).
pub const MIN_WIDTH: u16 = 20;
/// Blank columns on each side of the content (SPEC §3.3).
pub const MARGIN: u16 = 1;

/// What the pane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    /// Pane narrower than `MIN_WIDTH`: show "Window too narrow".
    TooNarrow,
    /// The file has no content: show "Empty document".
    Empty,
}

/// What a row holds; later layout stages add variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// A line of text of the block.
    Text,
    /// A line of a heading of the given level.
    Heading(u8),
    /// A horizontal rule filling the content width.
    Rule,
    /// A line of a table: border or cells.
    Table,
    /// Spacing between blocks.
    Blank,
    /// A row reserved for a picture; no text is drawn over it. Only the first
    /// row of a slot carries the [`Slot`] description.
    Picture,
    /// Dimmed one-row placeholder of a missing or remote image.
    ImageNote,
}

/// Which picture a slot or a size query is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PictureKey {
    /// Mermaid diagram, numbered in document order from 0 (footnotes last).
    Diagram(usize),
    /// Local image file (percent-decoded, resolved against the document directory).
    Image(std::path::PathBuf),
}

/// Natural pixel size of pictures, supplied by whoever can read or render them.
pub trait PictureSizes {
    /// `None`: unknown. For a local image that means missing or unreadable; for a
    /// diagram, not rendered yet.
    fn natural_size(&self, key: &PictureKey) -> Option<(u32, u32)>;
}

impl<F: Fn(&PictureKey) -> Option<(u32, u32)>> PictureSizes for F {
    fn natural_size(&self, key: &PictureKey) -> Option<(u32, u32)> {
        self(key)
    }
}

/// A set of whole rows reserved for one picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub key: PictureKey,
    /// Columns the picture spans.
    pub cols: u16,
    /// Rows the slot reserves (the slot's `Picture` rows).
    pub rows: u16,
    /// Pixel size the picture is drawn at (already scaled down).
    pub px: (u32, u32),
    /// Columns between the left edge of the content area and the picture.
    pub left: u16,
    /// False while the natural size is unknown: the slot is a placeholder.
    pub known: bool,
}

/// A run of text with one style inside a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSpan {
    pub text: String,
    pub style: Style,
    /// Link target, when the run is part of a link.
    pub link: Option<String>,
    /// Logical text unit (paragraph, heading, table cell...) the run belongs to;
    /// `None` for markers, bars, borders and padding, which search skips.
    pub unit: Option<usize>,
}

/// One terminal row of content, without the side margins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    /// Index of the top-level block the row belongs to.
    pub block: usize,
    pub spans: Vec<RowSpan>,
    /// Set on the first row of a picture slot.
    pub slot: Option<Slot>,
}

impl Row {
    /// Concatenated text of the row.
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub state: State,
    pub rows: Vec<Row>,
    pub index: Index,
}

/// Lay `doc` out for a pane `width` columns wide; `cell` is the cell size in pixels.
/// Pictures have no known size here, so images are missing and diagrams are placeholders.
pub fn layout(doc: &Document, width: u16, cell: (u16, u16)) -> Layout {
    layout_with(doc, width, u16::MAX, cell, &|_: &PictureKey| None)
}

/// Like [`layout`], for a pane `height` rows tall, with picture sizes from `sizes`.
pub fn layout_with(
    doc: &Document,
    width: u16,
    height: u16,
    cell: (u16, u16),
    sizes: &dyn PictureSizes,
) -> Layout {
    let cx = Ctx {
        cell: (cell.0.max(1), cell.1.max(1)),
        height: height.max(1),
        sizes,
        diagrams: std::cell::Cell::new(0),
        units: std::cell::Cell::new(0),
        headings: std::cell::RefCell::new(Vec::new()),
    };
    let state = if width < MIN_WIDTH {
        State::TooNarrow
    } else if doc.empty {
        State::Empty
    } else {
        State::Ready
    };
    let mut out = Layout {
        state,
        rows: Vec::new(),
        index: Index::default(),
    };
    if state != State::Ready {
        return out;
    }
    let content = usize::from(width - 2 * MARGIN);
    for (index, block) in doc.blocks.iter().enumerate() {
        let rows = block_rows(block, index, content, &cx);
        if rows.is_empty() {
            continue;
        }
        if !out.rows.is_empty() {
            out.rows.extend((0..gap(block)).map(|_| blank(index)));
        }
        out.rows.extend(rows);
    }
    footnotes(doc, content, &cx, &mut out.rows);
    out.index = index::build(doc, &out.rows, &cx.headings.borrow());
    out
}

/// What picture slots need while laying out.
struct Ctx<'a> {
    cell: (u16, u16),
    /// Pane height in rows: the cap of a picture's height.
    height: u16,
    sizes: &'a dyn PictureSizes,
    /// Diagrams met so far, for their numbering.
    diagrams: std::cell::Cell<usize>,
    /// Text units handed out so far.
    units: std::cell::Cell<usize>,
    /// Text unit of every heading met, in laying-out order.
    headings: std::cell::RefCell<Vec<usize>>,
}

impl Ctx<'_> {
    /// Give all spans of `lines` one new text unit.
    fn tag(&self, lines: &mut [Vec<RowSpan>]) -> usize {
        tag(lines, &self.units)
    }
}

/// Give all spans of `lines` one new unit from `units`.
fn tag(lines: &mut [Vec<RowSpan>], units: &std::cell::Cell<usize>) -> usize {
    let id = units.get();
    units.set(id + 1);
    for span in lines.iter_mut().flatten() {
        span.unit = Some(id);
    }
    id
}

/// Rows a diagram of unknown size reserves (SPEC §3.4.3).
const PLACEHOLDER_ROWS: u16 = 10;

/// Natural size scaled down (never up) into `cols` columns and `rows` rows,
/// keeping the aspect ratio.
fn fit(natural: (u32, u32), cols: usize, rows: u16, cell: (u16, u16)) -> (u32, u32) {
    let (w, h) = (natural.0.max(1), natural.1.max(1));
    let max_w = cols as f64 * f64::from(cell.0);
    let max_h = f64::from(rows) * f64::from(cell.1);
    let f = (max_w / f64::from(w)).min(max_h / f64::from(h)).min(1.0);
    if f >= 1.0 {
        return (w, h);
    }
    let scale = |v: u32| ((f64::from(v) * f).floor() as u32).max(1);
    (scale(w), scale(h))
}

/// A picture's slot: whole rows, centred in `content` columns. With no size the
/// slot is the diagram placeholder.
fn slot_rows(
    key: PictureKey,
    size: Option<(u32, u32)>,
    index: usize,
    content: usize,
    cx: &Ctx,
) -> Vec<Row> {
    let (px, known) = match (size, &key) {
        (Some(natural), _) => (fit(natural, content, cx.height, cx.cell), true),
        (None, _) => {
            let w = content as u32 * u32::from(cx.cell.0);
            (
                (w, u32::from(PLACEHOLDER_ROWS) * u32::from(cx.cell.1)),
                false,
            )
        }
    };
    let cols = px.0.div_ceil(u32::from(cx.cell.0)).min(content as u32) as u16;
    let rows = px.1.div_ceil(u32::from(cx.cell.1)).max(1) as u16;
    let slot = Slot {
        key,
        cols,
        rows,
        px,
        left: ((content - usize::from(cols)) / 2) as u16,
        known,
    };
    (0..rows)
        .map(|i| Row {
            kind: RowKind::Picture,
            block: index,
            spans: Vec::new(),
            slot: (i == 0).then(|| slot.clone()),
        })
        .collect()
}

/// One dimmed row: `label`, then the alt text on the same row, cut to `content`.
fn note_row(label: &str, alt: &str, index: usize, content: usize) -> Row {
    let text = if alt.is_empty() {
        label.to_string()
    } else {
        format!("{label} {alt}")
    };
    let mut row = Row {
        kind: RowKind::ImageNote,
        block: index,
        spans: vec![plain(&text, theme::dim())],
        slot: None,
    };
    clip(&mut row, content);
    row
}

/// A paragraph with its images taken out: the text before, the picture (slot or
/// note row), the text after. Text-only segments of whitespace are dropped.
fn paragraph_rows(inlines: &[Inline], index: usize, content: usize, cx: &Ctx) -> Vec<Row> {
    let mut out = Vec::new();
    let mut run: Vec<Inline> = Vec::new();
    let flush = |run: &mut Vec<Inline>, out: &mut Vec<Row>| {
        let meaningful = run.iter().any(|i| match i {
            Inline::Span(s) => !s.text.trim().is_empty(),
            Inline::Html(h) => !h.trim().is_empty(),
            Inline::FootnoteRef(_) => true,
            _ => false,
        });
        if meaningful {
            let mut lines = wrap(pieces(run, theme::body()), content);
            cx.tag(&mut lines);
            out.extend(lines.into_iter().map(|spans| Row {
                kind: RowKind::Text,
                block: index,
                spans,
                slot: None,
            }));
        }
        run.clear();
    };
    for inline in inlines {
        let Inline::Image(img) = inline else {
            run.push(inline.clone());
            continue;
        };
        flush(&mut run, &mut out);
        match &img.target {
            ImageTarget::Remote(url) => {
                out.push(note_row(
                    &format!("[remote image: {url}]"),
                    &img.alt,
                    index,
                    content,
                ));
            }
            ImageTarget::Local(path) => {
                let key = PictureKey::Image(path.clone());
                let size = cx.sizes.natural_size(&key);
                match size {
                    Some(_) => out.extend(slot_rows(key, size, index, content, cx)),
                    None => out.push(note_row(
                        &format!("[image not found: {}]", path.display()),
                        &img.alt,
                        index,
                        content,
                    )),
                }
            }
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Footnote definitions under a rule at the end of the document.
fn footnotes(doc: &Document, content: usize, cx: &Ctx, rows: &mut Vec<Row>) {
    if doc.footnotes.is_empty() {
        return;
    }
    let index = doc.blocks.len();
    if !rows.is_empty() {
        rows.push(blank(index));
    }
    rows.push(Row {
        kind: RowKind::Rule,
        block: index,
        slot: None,
        spans: vec![RowSpan {
            text: "─".repeat(content),
            style: theme::rule(),
            link: None,
            unit: None,
        }],
    });
    for note in &doc.footnotes {
        let marker = format!("[{}] ", note.number);
        let hang = marker.chars().count();
        let inner = blocks_rows(&note.blocks, index, content.saturating_sub(hang).max(1), cx);
        rows.extend(hang_rows(inner, index, &marker, hang, content));
    }
}

fn blank(index: usize) -> Row {
    Row {
        kind: RowKind::Blank,
        block: index,
        slot: None,
        spans: Vec::new(),
    }
}

/// Child blocks separated by blank rows.
fn blocks_rows(blocks: &[Block], index: usize, width: usize, cx: &Ctx) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    for block in blocks {
        let rows = block_rows(block, index, width, cx);
        if rows.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.extend((0..gap(block)).map(|_| blank(index)));
        }
        out.extend(rows);
    }
    out
}

/// Blank rows above a block: headings open a section, H1 and H2 with more air.
/// Nothing is added above the first block of a container, and list items keep
/// one row between their blocks.
fn gap(block: &Block) -> usize {
    match block {
        Block::Heading { level, .. } if *level <= 2 => 2,
        _ => 1,
    }
}

fn plain(text: &str, style: Style) -> RowSpan {
    RowSpan {
        text: text.to_string(),
        style,
        link: None,
        unit: None,
    }
}

/// Put `prefix` on the first row and `hang` blank columns on the others.
fn hang_rows(mut rows: Vec<Row>, index: usize, marker: &str, hang: usize, max: usize) -> Vec<Row> {
    if rows.is_empty() {
        rows.push(Row {
            kind: RowKind::Text,
            block: index,
            slot: None,
            spans: Vec::new(),
        });
    }
    for (i, row) in rows.iter_mut().enumerate() {
        if row.kind == RowKind::Blank {
            continue;
        }
        let lead = if i == 0 {
            marker.to_string()
        } else {
            " ".repeat(hang)
        };
        row.spans.insert(0, plain(&lead, theme::body()));
        if let Some(slot) = &mut row.slot {
            slot.left += hang as u16;
        }
        clip(row, max);
    }
    rows
}

/// Cut a row that would exceed `max` columns (only possible in very deep nesting).
fn clip(row: &mut Row, max: usize) {
    let mut used = 0;
    let mut keep = Vec::new();
    for mut span in std::mem::take(&mut row.spans) {
        let mut text = String::new();
        for c in span.text.chars() {
            let w = c.width().unwrap_or(0);
            if used + w > max {
                break;
            }
            used += w;
            text.push(c);
        }
        span.text = text;
        if !span.text.is_empty() {
            keep.push(span);
        }
    }
    row.spans = keep;
}

fn list_rows(list: &List, index: usize, content: usize, cx: &Ctx) -> Vec<Row> {
    let mut out = Vec::new();
    for (n, item) in list.items.iter().enumerate() {
        let marker = match (item.task, list.start) {
            (Some(true), _) => "☑ ".to_string(),
            (Some(false), _) => "☐ ".to_string(),
            (None, Some(start)) => format!("{}. ", start + n as u64),
            (None, None) => "• ".to_string(),
        };
        let hang = marker.chars().count();
        let mut rows: Vec<Row> = Vec::new();
        let mut first_laid_out = false;
        for (i, block) in item.blocks.iter().enumerate() {
            // A nested list steps in two columns; other content aligns with the item text.
            let nested = i > 0 && matches!(block, Block::List(_));
            let indent = if nested { 2 } else { hang };
            let child = block_rows(block, index, content.saturating_sub(indent).max(1), cx);
            if child.is_empty() {
                continue;
            }
            if i > 0 && !nested && !rows.is_empty() {
                rows.push(blank(index));
            }
            first_laid_out |= i == 0;
            rows.extend(hang_rows(
                child,
                index,
                &" ".repeat(indent),
                indent,
                content,
            ));
        }
        if first_laid_out {
            // The first row carries the marker in place of its alignment padding.
            rows[0].spans[0].text = marker;
        } else {
            let lone = plain(marker.trim_end(), theme::body());
            rows.insert(
                0,
                Row {
                    kind: RowKind::Text,
                    block: index,
                    slot: None,
                    spans: vec![lone],
                },
            );
        }
        out.extend(rows);
    }
    out
}

fn quote_rows(blocks: &[Block], index: usize, content: usize, cx: &Ctx) -> Vec<Row> {
    let mut rows = blocks_rows(blocks, index, content.saturating_sub(2).max(1), cx);
    for row in &mut rows {
        for span in &mut row.spans {
            span.style = span.style.patch(theme::dim());
        }
        let bar = if row.spans.is_empty() { "│" } else { "│ " };
        row.spans.insert(0, plain(bar, theme::dim()));
        if let Some(slot) = &mut row.slot {
            slot.left += 2;
        }
        clip(row, content);
    }
    rows
}

fn block_rows(block: &Block, index: usize, content: usize, cx: &Ctx) -> Vec<Row> {
    let make = |kind, lines: Vec<Vec<RowSpan>>| -> Vec<Row> {
        lines
            .into_iter()
            .map(|spans| Row {
                kind,
                block: index,
                slot: None,
                spans,
            })
            .collect()
    };
    let rule = |glyph: &str| Row {
        kind: RowKind::Rule,
        block: index,
        slot: None,
        spans: vec![RowSpan {
            text: glyph.repeat(content),
            style: theme::rule(),
            link: None,
            unit: None,
        }],
    };
    match block {
        Block::Paragraph(inlines) => paragraph_rows(inlines, index, content, cx),
        Block::Diagram(_) => {
            let n = cx.diagrams.get();
            cx.diagrams.set(n + 1);
            let key = PictureKey::Diagram(n);
            let size = cx.sizes.natural_size(&key);
            slot_rows(key, size, index, content, cx)
        }
        Block::Heading { level, content: c } => {
            let mut lines = wrap(pieces(c, theme::heading(*level)), content);
            let unit = cx.tag(&mut lines);
            cx.headings.borrow_mut().push(unit);
            let mut rows = make(RowKind::Heading(*level), lines);
            if *level <= 2 && !rows.is_empty() {
                rows.push(rule(if *level == 1 { "═" } else { "─" }));
            }
            rows
        }
        Block::Rule => vec![rule("─")],
        Block::Html(text) => {
            let like = RowSpan {
                text: String::new(),
                style: theme::dim(),
                link: None,
                unit: None,
            };
            let mut lines = chunk(text, &like, content);
            cx.tag(&mut lines);
            make(RowKind::Text, lines)
        }
        Block::Table(t) => table::rows(t, index, content, &cx.units),
        Block::List(list) => list_rows(list, index, content, cx),
        Block::BlockQuote(blocks) => quote_rows(blocks, index, content, cx),
        // Code is laid out by a later stage.
        _ => Vec::new(),
    }
}

/// Model inlines as wrapper input, with `base` under every span style.
fn pieces(inlines: &[Inline], base: Style) -> Vec<Piece> {
    let text = |text: String, style: Style, link: Option<String>| {
        Piece::Text(RowSpan {
            text,
            style,
            link,
            unit: None,
        })
    };
    inlines
        .iter()
        .filter_map(|inline| match inline {
            Inline::Span(s) => Some(text(
                s.text.clone(),
                theme::inline(base, s.style, s.link.is_some()),
                s.link.clone(),
            )),
            Inline::Html(h) => Some(text(h.clone(), base.patch(theme::dim()), None)),
            Inline::SoftBreak => Some(text(" ".into(), base, None)),
            Inline::HardBreak => Some(Piece::Break),
            Inline::FootnoteRef(n) => Some(text(format!("[{n}]"), base, None)),
            // Image slots belong to a later stage.
            Inline::Image(_) => None,
        })
        .collect()
}
