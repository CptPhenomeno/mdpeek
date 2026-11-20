//! Background rendering of the full-screen view (SPEC §3.6.3, §4.20).
//!
//! One thread turns diagram sources into SVG once and then rasterises only
//! the visible region at the zoom scale, so the UI never blocks and a bitmap
//! never exceeds the screen. Requests carry a generation; the thread always
//! works on the newest queued request, and the UI drops stale replies.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use image::RgbaImage;

use crate::diagram::{self, Rendered};
use crate::raster;

/// The part of a diagram to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    /// Top-left corner in natural pixels.
    pub origin: (f64, f64),
    /// Screen pixels per natural pixel.
    pub scale: f64,
    /// Size of the bitmap in screen pixels.
    pub px: (u32, u32),
}

/// A job for the view worker.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub generation: u64,
    /// Mermaid source.
    pub source: String,
    /// `None`: only measure the natural size.
    pub region: Option<Region>,
}

/// What came back.
#[derive(Debug, Clone)]
pub enum Reply {
    Natural {
        generation: u64,
        size: (u32, u32),
    },
    Image {
        generation: u64,
        image: Arc<RgbaImage>,
    },
    Error {
        generation: u64,
        message: String,
    },
}

impl Reply {
    pub fn generation(&self) -> u64 {
        match self {
            Self::Natural { generation, .. }
            | Self::Image { generation, .. }
            | Self::Error { generation, .. } => *generation,
        }
    }
}

/// Where the view sends its requests; a fake in tests.
pub trait ViewSink: std::fmt::Debug {
    fn submit(&self, request: Request);
}

/// The worker thread as a sink.
#[derive(Debug)]
pub struct ThreadSink(Sender<Request>);

impl ViewSink for ThreadSink {
    fn submit(&self, request: Request) {
        // The thread only ends when the sink is dropped.
        let _ = self.0.send(request);
    }
}

/// Starts the worker; replies arrive on `replies`. `None` if no thread could start.
pub fn spawn(replies: Sender<Reply>) -> Option<ThreadSink> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("view-worker".into())
        .spawn(move || run(&rx, &replies))
        .ok()?;
    Some(ThreadSink(tx))
}

fn run(rx: &Receiver<Request>, replies: &Sender<Reply>) {
    let mut cached: Option<(String, String)> = None;
    while let Ok(mut request) = rx.recv() {
        // Only the newest request matters.
        while let Ok(newer) = rx.try_recv() {
            request = newer;
        }
        let generation = request.generation;
        let reply = match svg_of(&mut cached, &request.source) {
            Ok(svg) => render(svg, &request),
            Err(message) => Reply::Error {
                generation,
                message,
            },
        };
        if replies.send(reply).is_err() {
            return;
        }
    }
}

fn svg_of<'a>(cached: &'a mut Option<(String, String)>, source: &str) -> Result<&'a str, String> {
    if cached.as_ref().is_none_or(|(src, _)| src != source) {
        match diagram::render_svg(source) {
            Rendered::Svg(svg) => *cached = Some((source.to_owned(), svg)),
            Rendered::Error(e) => return Err(e),
            _ => return Err("the diagram has no SVG output".to_owned()),
        }
    }
    cached
        .as_ref()
        .map(|(_, svg)| svg.as_str())
        .ok_or_else(|| "the diagram has no SVG output".to_owned())
}

fn render(svg: &str, request: &Request) -> Reply {
    let generation = request.generation;
    let error = |e: raster::RasterError| Reply::Error {
        generation,
        message: e.to_string(),
    };
    match request.region {
        None => match raster::intrinsic_size(svg) {
            Ok((w, h)) => Reply::Natural {
                generation,
                size: (w.ceil() as u32, h.ceil() as u32),
            },
            Err(e) => error(e),
        },
        Some(r) => match raster::rasterise_region(svg, r.origin, r.scale, r.px) {
            Ok(image) => Reply::Image {
                generation,
                image: Arc::new(image),
            },
            Err(e) => error(e),
        },
    }
}
