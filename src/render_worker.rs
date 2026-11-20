//! Background render worker for diagrams and images (SPEC §3.4.3, §4.5, §4.7, §5).
//!
//! The UI thread calls [`RenderWorker::submit`] (never blocks: a short mutex
//! hold, no rendering under the lock) and later drains the `mpsc::Receiver`
//! given to the worker with `try_recv` after each `event::poll` timeout, e.g.
//! poll with a short timeout (about 50 ms) while jobs are outstanding.
//! Each [`RenderResult`] carries the job's key, size and generation so the UI
//! can ignore anything that no longer matches its layout.
//!
//! Generations: the worker remembers the highest generation submitted. A job
//! from a lower one (a superseded layout width, say) is refused at submit,
//! removed from the queue when a newer generation arrives, and its result is
//! dropped if it was already rendering. Raising the generation is therefore
//! how the caller cancels everything outstanding (resize, KB-41).
//!
//! Coalescing: a job identical (key, mode, size, generation) to one queued or
//! rendering is refused (`submit` returns `false`); its result arrives once.
//!
//! Priority: the queue runs the lowest `priority` first, FIFO among equals, so
//! the caller can rank by distance from the viewport (SPEC §4.5).

use std::collections::HashMap;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use image::RgbaImage;

use crate::diagram::{self, Rendered};
use crate::layout::PictureKey;
use crate::picture_cache::{CacheKey, PictureCache, image_source_hash};
use crate::raster;

/// What a job renders from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Diagram(String),
    Image(PathBuf),
}

/// What a job produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Full raster at the target pixel size (`Outcome::Picture`).
    Render,
    /// Natural size only: SVG intrinsic size or image header (`Outcome::Natural`),
    /// so the layout can reserve the slot before the raster exists.
    Measure,
    /// Diagram as a Unicode drawing (`Outcome::Text` / `TextUnavailable`).
    Text,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub key: PictureKey,
    pub source: Source,
    pub mode: Mode,
    /// Target pixel limits (the slot size; raster fits inside them).
    pub width: u32,
    pub height: u32,
    /// Pixels per slot pixel: the raster is `density` times the slot size.
    pub density: u32,
    pub generation: u64,
    /// Lower runs first (e.g. distance from the viewport).
    pub priority: u32,
}

/// What the renderer function returns for a job.
#[derive(Debug)]
pub enum Output {
    Picture(RgbaImage, (u32, u32)),
    Natural(u32, u32),
    Text(String),
    TextUnavailable,
    Error(String),
}

/// A finished job. The picture comes with its natural size, so no separate
/// measure is needed when the full render is requested first.
#[derive(Debug, Clone)]
pub enum Outcome {
    Picture {
        image: Arc<RgbaImage>,
        natural: (u32, u32),
    },
    Natural(u32, u32),
    Text(String),
    TextUnavailable,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub key: PictureKey,
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    pub outcome: Outcome,
}

/// Injectable renderer, so tests need no merman or files.
pub type RenderFn = dyn Fn(&Job) -> Output + Send + Sync;

type JobId = (PictureKey, Mode, u32, u32, u32, u64);

fn job_id(j: &Job) -> JobId {
    (
        j.key.clone(),
        j.mode,
        j.width,
        j.height,
        j.density,
        j.generation,
    )
}

#[derive(Default)]
struct State {
    queue: Vec<(u64, Job)>,
    /// Queued and rendering jobs, for coalescing.
    pending: HashSet<JobId>,
    generation: u64,
    seq: u64,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    cache: PictureCache,
    /// Natural sizes by cache source hash, so a cache hit still reports one.
    naturals: Mutex<HashMap<u64, (u32, u32)>>,
    render: Arc<RenderFn>,
    tx: Mutex<Sender<RenderResult>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // Nothing here is left half-updated by a panic, so keep going.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct RenderWorker {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl RenderWorker {
    /// Worker running the real diagram and raster pipeline on `threads` threads.
    pub fn new(cache: PictureCache, tx: Sender<RenderResult>, threads: usize) -> Self {
        Self::with_renderer(cache, tx, threads, Arc::new(render_job))
    }

    pub fn with_renderer(
        cache: PictureCache,
        tx: Sender<RenderResult>,
        threads: usize,
        render: Arc<RenderFn>,
    ) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            cache,
            naturals: Mutex::new(HashMap::new()),
            render,
            tx: Mutex::new(tx),
        });
        let threads = (0..threads.max(1))
            .filter_map(|_| {
                let s = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name("render-worker".into())
                    .spawn(move || run(&s))
                    .ok()
            })
            .collect();
        Self { shared, threads }
    }

    /// Queues a job without blocking. Returns `false` when the job is stale
    /// (older generation than one already submitted) or a duplicate of one
    /// queued or rendering; submitting a newer generation drops every queued
    /// job of older ones.
    pub fn submit(&self, job: Job) -> bool {
        let mut st = lock(&self.shared.state);
        if job.generation < st.generation {
            return false;
        }
        if job.generation > st.generation {
            raise(&mut st, job.generation);
        }
        if !st.pending.insert(job_id(&job)) {
            return false;
        }
        st.seq += 1;
        let seq = st.seq;
        st.queue.push((seq, job));
        drop(st);
        self.shared.wake.notify_one();
        true
    }

    /// Supersedes every job of an older generation without submitting one.
    pub fn set_generation(&self, generation: u64) {
        let mut st = lock(&self.shared.state);
        if generation > st.generation {
            raise(&mut st, generation);
        }
    }
}

fn raise(st: &mut State, generation: u64) {
    st.generation = generation;
    let (keep, drop): (Vec<_>, Vec<_>) = std::mem::take(&mut st.queue)
        .into_iter()
        .partition(|(_, j)| j.generation >= generation);
    st.queue = keep;
    for (_, j) in drop {
        st.pending.remove(&job_id(&j));
    }
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        lock(&self.shared.state).shutdown = true;
        self.shared.wake.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn run(shared: &Shared) {
    loop {
        let job = {
            let mut st = lock(&shared.state);
            loop {
                if st.shutdown {
                    return;
                }
                let next = st
                    .queue
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (seq, j))| (j.priority, *seq))
                    .map(|(i, _)| i);
                if let Some(i) = next {
                    break st.queue.remove(i).1;
                }
                st = shared.wake.wait(st).unwrap_or_else(PoisonError::into_inner);
            }
        };
        let outcome = execute(shared, &job);
        let current = {
            let mut st = lock(&shared.state);
            st.pending.remove(&job_id(&job));
            job.generation >= st.generation
        };
        if current {
            let result = RenderResult {
                key: job.key,
                generation: job.generation,
                width: job.width,
                height: job.height,
                outcome,
            };
            // A closed receiver means the UI is gone; nothing to report to.
            let _ = lock(&shared.tx).send(result);
        }
    }
}

/// Runs one job through the cache and the renderer; a panic becomes an error.
fn execute(shared: &Shared, job: &Job) -> Outcome {
    catch_unwind(AssertUnwindSafe(|| execute_inner(shared, job)))
        .unwrap_or_else(|_| Outcome::Error("renderer panicked".to_string()))
}

fn execute_inner(shared: &Shared, job: &Job) -> Outcome {
    if job.mode != Mode::Render {
        return from_output((shared.render)(job));
    }
    let source = match &job.source {
        Source::Diagram(src) => Some(diagram::source_hash(src)),
        Source::Image(path) => image_source_hash(path),
    };
    let Some(source) = source else {
        return Outcome::Error("image not found or unreadable".to_string());
    };
    let key = CacheKey {
        source,
        width: job.width,
        height: job.height,
        density: job.density,
    };
    if let Some(image) = shared.cache.get(&key) {
        let natural = lock(&shared.naturals)
            .get(&source)
            .copied()
            .unwrap_or((image.width(), image.height()));
        return Outcome::Picture { image, natural };
    }
    match (shared.render)(job) {
        Output::Picture(img, natural) => {
            lock(&shared.naturals).insert(source, natural);
            Outcome::Picture {
                image: shared.cache.insert(key, img),
                natural,
            }
        }
        other => from_output(other),
    }
}

fn from_output(o: Output) -> Outcome {
    match o {
        Output::Picture(img, natural) => Outcome::Picture {
            image: Arc::new(img),
            natural,
        },
        Output::Natural(w, h) => Outcome::Natural(w, h),
        Output::Text(t) => Outcome::Text(t),
        Output::TextUnavailable => Outcome::TextUnavailable,
        Output::Error(e) => Outcome::Error(e),
    }
}

/// The real pipeline: merman to SVG to resvg, or the image decoder.
pub fn render_job(job: &Job) -> Output {
    match (&job.source, job.mode) {
        (Source::Diagram(src), Mode::Text) => match diagram::render_text(src) {
            Rendered::Text(t) => Output::Text(t),
            Rendered::TextUnavailable => Output::TextUnavailable,
            Rendered::Error(e) => Output::Error(e),
            Rendered::Svg(_) => Output::Error("unexpected SVG from text render".to_string()),
        },
        (Source::Diagram(src), mode) => match diagram::render_svg(src) {
            Rendered::Svg(svg) => svg_output(&svg, mode, job),
            Rendered::Error(e) => Output::Error(e),
            Rendered::Text(_) | Rendered::TextUnavailable => {
                Output::Error("unexpected text from SVG render".to_string())
            }
        },
        (Source::Image(path), Mode::Measure) => match raster::image_size(path) {
            Ok((w, h)) => Output::Natural(w, h),
            Err(e) => Output::Error(e.to_string()),
        },
        (Source::Image(path), _) => {
            let natural = raster::image_size(path);
            match (
                natural,
                raster::decode_image(path, job.width, job.height, job.density),
            ) {
                (Ok(n), Ok(img)) => Output::Picture(img, n),
                (Err(e), _) | (_, Err(e)) => Output::Error(e.to_string()),
            }
        }
    }
}

fn natural_of(svg: &str) -> Result<(u32, u32), raster::RasterError> {
    let (w, h) = raster::intrinsic_size(svg)?;
    Ok((w.ceil() as u32, h.ceil() as u32))
}

fn svg_output(svg: &str, mode: Mode, job: &Job) -> Output {
    let natural = match natural_of(svg) {
        Ok(n) => n,
        Err(e) => return Output::Error(e.to_string()),
    };
    if mode == Mode::Measure {
        return Output::Natural(natural.0, natural.1);
    }
    match raster::rasterise(svg, job.width, job.height, job.density) {
        Ok(img) => Output::Picture(img, natural),
        Err(e) => Output::Error(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{Receiver, channel};
    use std::thread::{self, ThreadId};
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(5);
    const QUIET: Duration = Duration::from_millis(150);

    fn job(n: usize, generation: u64) -> Job {
        Job {
            key: PictureKey::Diagram(n),
            source: Source::Diagram(format!("graph TD; A{n}-->B")),
            mode: Mode::Render,
            width: 4,
            height: 3,
            density: 1,
            generation,
            priority: 0,
        }
    }

    fn ok(_: &Job) -> Output {
        Output::Picture(RgbaImage::new(4, 3), (40, 30))
    }

    fn worker(threads: usize, f: Arc<RenderFn>) -> (RenderWorker, Receiver<RenderResult>) {
        let (tx, rx) = channel();
        (
            RenderWorker::with_renderer(PictureCache::new(1 << 20), tx, threads, f),
            rx,
        )
    }

    /// Renderer that reports it started, then blocks until the gate sends.
    fn gated(
        calls: Arc<AtomicUsize>,
    ) -> (Arc<RenderFn>, Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (started_tx, started_rx) = channel();
        let (gate_tx, gate_rx) = channel::<()>();
        let gate_rx = Mutex::new(gate_rx);
        let started_tx = Mutex::new(started_tx);
        let f: Arc<RenderFn> = Arc::new(move |j: &Job| {
            calls.fetch_add(1, Ordering::SeqCst);
            if let Ok(s) = started_tx.lock() {
                let _ = s.send(());
            }
            if let Ok(g) = gate_rx.lock() {
                let _ = g.recv_timeout(WAIT);
            }
            ok(j)
        });
        (f, started_rx, gate_tx)
    }

    #[test]
    fn job_completes_on_worker_thread_and_posts_result() {
        let ids: Arc<Mutex<Vec<ThreadId>>> = Arc::default();
        let seen = Arc::clone(&ids);
        let (w, rx) = worker(
            1,
            Arc::new(move |j: &Job| {
                if let Ok(mut v) = seen.lock() {
                    v.push(thread::current().id());
                }
                ok(j)
            }),
        );
        assert!(w.submit(job(0, 1)));
        let r = rx.recv_timeout(WAIT).expect("a result");
        assert_eq!(r.key, PictureKey::Diagram(0));
        assert_eq!(r.generation, 1);
        assert_eq!((r.width, r.height), (4, 3));
        match r.outcome {
            Outcome::Picture { image, natural } => {
                assert_eq!((image.width(), image.height()), (4, 3));
                assert_eq!(natural, (40, 30));
            }
            other => panic!("unexpected {other:?}"),
        }
        let ids = ids.lock().unwrap();
        assert_eq!(ids.len(), 1);
        assert_ne!(ids[0], thread::current().id());
    }

    #[test]
    fn submit_returns_immediately_while_a_slow_job_renders() {
        let (f, started, gate) = gated(Arc::default());
        let (w, rx) = worker(1, f);
        assert!(w.submit(job(0, 1)));
        started.recv_timeout(WAIT).expect("job started");
        // The only worker is stuck in the first job: these must still return.
        assert!(w.submit(job(1, 1)));
        assert!(w.submit(job(2, 1)));
        assert!(rx.try_recv().is_err());
        gate.send(()).unwrap();
        gate.send(()).unwrap();
        gate.send(()).unwrap();
        for _ in 0..3 {
            rx.recv_timeout(WAIT).expect("result");
        }
    }

    #[test]
    fn duplicate_jobs_for_the_same_key_render_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (f, started, gate) = gated(Arc::clone(&calls));
        let (w, rx) = worker(2, f);
        assert!(w.submit(job(0, 1)));
        started.recv_timeout(WAIT).expect("job started");
        assert!(!w.submit(job(0, 1)), "in-flight duplicate coalesced");
        assert!(!w.submit(job(0, 1)));
        gate.send(()).unwrap();
        rx.recv_timeout(WAIT).expect("one result");
        assert!(rx.recv_timeout(QUIET).is_err(), "no second result");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn results_of_a_superseded_generation_are_discarded() {
        let (f, started, gate) = gated(Arc::default());
        let (w, rx) = worker(1, f);
        assert!(w.submit(job(0, 1))); // in flight
        started.recv_timeout(WAIT).expect("job started");
        assert!(w.submit(job(1, 1))); // queued
        assert!(w.submit(job(2, 2))); // newer generation supersedes both
        assert!(!w.submit(job(3, 1)), "stale submit refused");
        gate.send(()).unwrap();
        gate.send(()).unwrap();
        let r = rx.recv_timeout(WAIT).expect("current-generation result");
        assert_eq!((r.key, r.generation), (PictureKey::Diagram(2), 2));
        assert!(rx.recv_timeout(QUIET).is_err(), "stale results dropped");
    }

    #[test]
    fn render_error_is_delivered_as_an_error_result() {
        let (w, rx) = worker(1, Arc::new(|_: &Job| Output::Error("boom".into())));
        assert!(w.submit(job(0, 1)));
        let r = rx.recv_timeout(WAIT).expect("a result");
        assert!(matches!(r.outcome, Outcome::Error(ref m) if m == "boom"));
    }

    #[test]
    fn renderer_panic_is_delivered_as_an_error_result() {
        let (w, rx) = worker(1, Arc::new(|_: &Job| -> Output { panic!("bad") }));
        assert!(w.submit(job(0, 1)));
        let r = rx.recv_timeout(WAIT).expect("a result");
        assert!(matches!(r.outcome, Outcome::Error(_)));
        // The worker survives and keeps serving.
        assert!(w.submit(job(1, 1)));
        assert!(rx.recv_timeout(WAIT).is_ok());
    }
}
