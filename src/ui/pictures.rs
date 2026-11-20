//! Picture pipeline of the viewer: what to render, when, and what came back
//! (SPEC §3.4.3, §3.5, §4.5).
//!
//! Flow:
//! * At load, the header of every local image is measured (`Mode::Measure`,
//!   cheap, off the UI thread) so its slot gets its real size; remote images
//!   are never touched and missing ones stay a "[image not found]" row.
//! * Every slot within one screen above or below the viewport that has no
//!   picture covering its pixel size gets one `Mode::Render` job at the slot
//!   size, ranked by its distance from the viewport. Diagrams are rendered
//!   straight away: the picture comes with its natural size, which sizes the
//!   slot (a diagram stays a placeholder until then).
//! * Results of an outdated generation are ignored; the generation grows when
//!   the pane width changes, which also forgets what was outstanding. Slots
//!   whose size changed are stale (placeholder) until the debounced
//!   re-rasterisation (150 ms after the last resize) delivers a new raster.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use image::RgbaImage;

use crate::doc::{Block, Document, ImageTarget, Inline};
use crate::layout::{DiagramState, DiagramStates, Layout, PictureKey, PictureSizes};
use crate::render_worker::{Job, Mode, Outcome, RenderResult, RenderWorker, Source};

/// Where the viewer sends render jobs; the real one is the background worker.
pub trait JobSink: std::fmt::Debug {
    /// Queue a job; `false` when it was refused (duplicate or stale).
    fn submit(&self, job: Job) -> bool;
    /// Supersede every job of an older generation.
    fn set_generation(&self, generation: u64);
}

/// The background worker as a sink.
pub struct WorkerSink(pub RenderWorker);

impl std::fmt::Debug for WorkerSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkerSink")
    }
}

impl JobSink for WorkerSink {
    fn submit(&self, job: Job) -> bool {
        self.0.submit(job)
    }
    fn set_generation(&self, generation: u64) {
        self.0.set_generation(generation);
    }
}

/// Monotonic time source of the resize debounce; tests drive their own.
pub trait Clock: std::fmt::Debug {
    /// Time elapsed since an arbitrary fixed origin.
    fn now(&self) -> std::time::Duration;
}

/// The real clock: time since it was created.
#[derive(Debug)]
pub struct SystemClock(std::time::Instant);

impl Default for SystemClock {
    fn default() -> Self {
        Self(std::time::Instant::now())
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

/// Quiet time after the last resize before pictures are re-rasterised (SPEC §3.4.6).
pub const RESIZE_DEBOUNCE: Duration = Duration::from_millis(150);

/// A raster that arrived and must be handed to the picture set.
pub struct Arrived {
    pub key: PictureKey,
    pub image: Arc<RgbaImage>,
}

/// Size of a measure job: it has no pixel target.
const MEASURE_PX: (u32, u32) = (0, 0);

/// Tolerance, in pixels, when comparing a raster with the size a slot asks for.
const SLACK: u32 = 2;

#[derive(Debug)]
pub struct Pictures {
    diagram_sources: Vec<String>,
    images: Vec<PathBuf>,
    /// Natural size of every picture known so far.
    naturals: HashMap<PictureKey, (u32, u32)>,
    states: HashMap<usize, DiagramState>,
    /// Text outcomes (`Text` / `TextUnavailable`) of text-mode jobs.
    texts: HashMap<usize, DiagramState>,
    /// Diagrams are drawn as text, not pictures.
    text_mode: bool,
    /// Images that are missing or cannot be decoded.
    failed: HashSet<PictureKey>,
    /// Size of the raster the picture set holds for a key.
    rendered: HashMap<PictureKey, (u32, u32)>,
    /// Jobs submitted in this generation.
    requested: HashSet<(PictureKey, (u32, u32))>,
    inflight: HashSet<(PictureKey, (u32, u32))>,
    generation: u64,
    /// Raster pixels per slot pixel of the jobs submitted (KB-72).
    density: u32,
    sink: Option<Box<dyn JobSink>>,
    clock: Box<dyn Clock>,
    /// Pictures whose raster no longer fits their slot (SPEC §4.21): they show
    /// the placeholder until a raster of the current generation arrives.
    stale: HashSet<PictureKey>,
    /// When re-rasterisation may start, after the last resize.
    due: Option<Duration>,
}

impl Pictures {
    pub fn new(doc: &Document) -> Self {
        let mut diagram_sources = Vec::new();
        let mut images = Vec::new();
        collect(&doc.blocks, &mut diagram_sources, &mut images);
        for note in &doc.footnotes {
            collect(&note.blocks, &mut diagram_sources, &mut images);
        }
        Self {
            diagram_sources,
            images,
            naturals: HashMap::new(),
            states: HashMap::new(),
            texts: HashMap::new(),
            text_mode: false,
            failed: HashSet::new(),
            rendered: HashMap::new(),
            requested: HashSet::new(),
            inflight: HashSet::new(),
            generation: 1,
            density: 1,
            sink: None,
            clock: Box::new(SystemClock::default()),
            stale: HashSet::new(),
            due: None,
        }
    }

    /// Switches diagram text mode; diagram jobs of the other mode are forgotten
    /// so the new mode submits its own (cached pictures are not rendered again).
    pub fn set_text_mode(&mut self, on: bool) {
        if self.text_mode == on {
            return;
        }
        self.text_mode = on;
        let diagram = |id: &(PictureKey, (u32, u32))| matches!(id.0, PictureKey::Diagram(_));
        self.requested.retain(|id| !diagram(id));
        self.inflight.retain(|id| !diagram(id));
    }

    pub fn set_clock(&mut self, clock: Box<dyn Clock>) {
        self.clock = clock;
    }

    /// Whether the picture of `key` is outdated and must not be drawn.
    pub fn is_stale(&self, key: &PictureKey) -> bool {
        self.stale.contains(key)
    }

    /// A resize changed the slots of `changed` (or is still settling): their
    /// rasters are outdated, outstanding jobs are superseded and the debounce
    /// restarts. Returns whether anything is pending.
    pub fn invalidate(&mut self, changed: impl IntoIterator<Item = PictureKey>) -> bool {
        for key in changed {
            self.rendered.remove(&key);
            self.stale.insert(key);
        }
        if self.stale.is_empty() {
            return false;
        }
        self.due = Some(self.clock.now() + RESIZE_DEBOUNCE);
        true
    }

    /// Whether the debounce has elapsed; it is then over.
    pub fn settle(&mut self) -> bool {
        let due = self.due.is_some_and(|d| self.clock.now() >= d);
        if due {
            self.due = None;
        }
        due
    }

    /// Time left until re-rasterisation may start, while a debounce runs.
    pub fn wakeup(&self) -> Option<Duration> {
        self.due.map(|d| d.saturating_sub(self.clock.now()))
    }

    fn settling(&self) -> bool {
        self.due.is_some_and(|d| self.clock.now() < d)
    }

    /// Raster `density` times the slot size (never below 1).
    pub fn set_density(&mut self, density: u32) {
        self.density = density.max(1);
    }

    pub fn set_sink(&mut self, sink: Box<dyn JobSink>) {
        sink.set_generation(self.generation);
        self.sink = Some(sink);
    }

    pub fn has_outstanding(&self) -> bool {
        !self.inflight.is_empty()
    }

    /// A raster of `key` was handed over directly (not through the worker).
    pub fn set_ready(&mut self, key: PictureKey, size: (u32, u32)) {
        if let PictureKey::Diagram(n) = key {
            self.states.insert(n, DiagramState::Ready);
        }
        self.failed.remove(&key);
        self.naturals.insert(key.clone(), size);
        self.rendered.insert(key, size);
    }

    /// Outdated jobs are forgotten: called when the pane width changes.
    pub fn next_generation(&mut self) {
        self.generation += 1;
        self.requested.clear();
        self.inflight.clear();
        if let Some(sink) = &self.sink {
            sink.set_generation(self.generation);
        }
    }

    /// Takes a worker result in. `None` when it changes nothing (outdated);
    /// otherwise the raster to hand to the picture set, if there is one. The
    /// caller lays out again either way.
    pub fn accept(&mut self, result: RenderResult) -> Option<Option<Arrived>> {
        if result.generation != self.generation {
            return None;
        }
        let key = result.key;
        let size = (result.width, result.height);
        self.inflight.remove(&(key.clone(), size));
        if matches!(result.outcome, Outcome::Picture { .. } | Outcome::Error(_)) {
            self.stale.remove(&key);
        }
        match result.outcome {
            Outcome::Picture { image, natural } => {
                if let PictureKey::Diagram(n) = key {
                    self.states.insert(n, DiagramState::Ready);
                }
                self.failed.remove(&key);
                self.naturals.insert(key.clone(), natural);
                // Slot pixels: the raster divided by the density, so a 2x raster
                // does not look like it covers a slot twice as large.
                let d = self.density;
                self.rendered.insert(
                    key.clone(),
                    (image.width().div_ceil(d), image.height().div_ceil(d)),
                );
                Some(Some(Arrived { key, image }))
            }
            Outcome::Natural(w, h) => {
                self.naturals.insert(key, (w, h));
                Some(None)
            }
            Outcome::Error(message) => {
                match key {
                    PictureKey::Diagram(n) => {
                        self.states.insert(n, DiagramState::Error(message));
                    }
                    PictureKey::Image(_) => {
                        self.naturals.remove(&key);
                        self.failed.insert(key);
                    }
                }
                Some(None)
            }
            Outcome::Text(text) => {
                self.set_text(key, DiagramState::Text(text));
                Some(None)
            }
            Outcome::TextUnavailable => {
                self.set_text(key, DiagramState::TextUnavailable);
                Some(None)
            }
        }
    }

    fn set_text(&mut self, key: PictureKey, state: DiagramState) {
        if let PictureKey::Diagram(n) = key {
            self.texts.insert(n, state);
        }
    }

    /// Submits what is due: header measures, and renders for the slots within
    /// one screen of the viewport (`scroll`, `height` rows).
    pub fn request(&mut self, layout: &Layout, scroll: usize, height: usize) {
        if self.sink.is_none() {
            return;
        }
        for path in self.images.clone() {
            let key = PictureKey::Image(path.clone());
            if self.naturals.contains_key(&key) || self.failed.contains(&key) {
                continue;
            }
            self.submit(key, Source::Image(path), Mode::Measure, MEASURE_PX, 0);
        }
        let first = scroll.saturating_sub(height);
        let last = scroll.saturating_add(2 * height);
        for (index, row) in layout.rows.iter().enumerate() {
            if index >= last {
                break;
            }
            let Some(slot) = &row.slot else { continue };
            if index + usize::from(slot.rows) <= first {
                continue;
            }
            let text_job = self.text_mode && matches!(slot.key, PictureKey::Diagram(_));
            let Some(source) = self.render_source(slot.key.clone(), slot.known) else {
                continue;
            };
            if text_job {
                let PictureKey::Diagram(n) = slot.key else {
                    continue;
                };
                if !self.texts.contains_key(&n) {
                    let priority = u32::try_from(self.distance(index, slot.rows, scroll, height))
                        .unwrap_or(u32::MAX);
                    self.submit(slot.key.clone(), source, Mode::Text, slot.px, priority);
                }
                continue;
            }
            if self.covers(&slot.key, slot.px)
                || (self.settling() && self.stale.contains(&slot.key))
            {
                continue;
            }
            let priority =
                u32::try_from(self.distance(index, slot.rows, scroll, height)).unwrap_or(u32::MAX);
            self.submit(slot.key.clone(), source, Mode::Render, slot.px, priority);
        }
    }

    /// Rows between a slot and the viewport (0 when they overlap).
    fn distance(&self, index: usize, rows: u16, scroll: usize, height: usize) -> usize {
        if index >= scroll + height {
            index + 1 - (scroll + height)
        } else {
            (scroll + 1).saturating_sub(index + usize::from(rows))
        }
    }

    /// What a render of `key` reads from, or `None` when nothing is to be rendered.
    fn render_source(&self, key: PictureKey, known: bool) -> Option<Source> {
        match &key {
            PictureKey::Diagram(n) => match self.states.get(n) {
                None | Some(DiagramState::Pending(_) | DiagramState::Ready) => {
                    self.diagram_sources.get(*n).cloned().map(Source::Diagram)
                }
                Some(_) => None,
            },
            PictureKey::Image(path) => known.then(|| Source::Image(path.clone())),
        }
    }

    fn covers(&self, key: &PictureKey, px: (u32, u32)) -> bool {
        self.rendered
            .get(key)
            .is_some_and(|r| r.0 + SLACK >= px.0 || r.1 + SLACK >= px.1)
    }

    fn submit(
        &mut self,
        key: PictureKey,
        source: Source,
        mode: Mode,
        px: (u32, u32),
        priority: u32,
    ) {
        let id = (key.clone(), px);
        if !self.requested.insert(id.clone()) {
            return;
        }
        let job = Job {
            key,
            source,
            mode,
            width: px.0,
            height: px.1,
            density: self.density,
            generation: self.generation,
            priority,
        };
        if self.sink.as_ref().is_some_and(|s| s.submit(job)) {
            self.inflight.insert(id);
        }
    }
}

impl PictureSizes for Pictures {
    fn natural_size(&self, key: &PictureKey) -> Option<(u32, u32)> {
        self.naturals.get(key).copied()
    }
}

impl DiagramStates for Pictures {
    fn state(&self, n: usize) -> DiagramState {
        if self.stale.contains(&PictureKey::Diagram(n)) {
            return DiagramState::Pending(None);
        }
        if self.text_mode {
            return match (self.texts.get(&n), self.states.get(&n)) {
                (Some(text), _) => text.clone(),
                (None, Some(error @ DiagramState::Error(_))) => error.clone(),
                _ => DiagramState::Pending(None),
            };
        }
        self.states
            .get(&n)
            .cloned()
            .unwrap_or(DiagramState::Pending(None))
    }
}

/// Diagram sources (numbered in layout order) and local images of `blocks`.
fn collect(blocks: &[Block], diagrams: &mut Vec<String>, images: &mut Vec<PathBuf>) {
    for block in blocks {
        match block {
            Block::Diagram(src) => diagrams.push(src.clone()),
            Block::Paragraph(inlines) => {
                for inline in inlines {
                    if let Inline::Image(img) = inline
                        && let ImageTarget::Local(path) = &img.target
                        && !images.contains(path)
                    {
                        images.push(path.clone());
                    }
                }
            }
            Block::List(list) => {
                for item in &list.items {
                    collect(&item.blocks, diagrams, images);
                }
            }
            Block::BlockQuote(inner) => collect(inner, diagrams, images),
            _ => {}
        }
    }
}
