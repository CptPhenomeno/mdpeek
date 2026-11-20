//! Background render worker for diagrams and images (SPEC §3.4.3, §4.5, §5).
//!
//! Stub: types only, so the tests compile; the behaviour lands in the next commit.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use image::RgbaImage;

use crate::layout::PictureKey;
use crate::picture_cache::PictureCache;

/// What a job renders from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Diagram(String),
    Image(PathBuf),
}

/// What a job produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Full raster at the target pixel size.
    Render,
    /// Natural size only (SVG intrinsic size / image header).
    Measure,
    /// Diagram as a Unicode drawing instead of a picture.
    Text,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub key: PictureKey,
    pub source: Source,
    pub mode: Mode,
    pub width: u32,
    pub height: u32,
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

pub type RenderFn = dyn Fn(&Job) -> Output + Send + Sync;

pub struct RenderWorker {
    _tx: Sender<RenderResult>,
}

impl RenderWorker {
    pub fn new(_cache: PictureCache, tx: Sender<RenderResult>, _threads: usize) -> Self {
        Self { _tx: tx }
    }

    pub fn with_renderer(
        _cache: PictureCache,
        tx: Sender<RenderResult>,
        _threads: usize,
        _render: Arc<RenderFn>,
    ) -> Self {
        Self { _tx: tx }
    }

    /// Queues a job; `false` if coalesced or stale.
    pub fn submit(&self, _job: Job) -> bool {
        false
    }

    pub fn set_generation(&self, _generation: u64) {}
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
