//! Picture pipeline of the viewer: what to render, when, and what came back.

use crate::render_worker::Job;

/// Where the viewer sends render jobs; the real one is the background worker.
pub trait JobSink: std::fmt::Debug {
    /// Queue a job; `false` when it was refused (duplicate or stale).
    fn submit(&self, job: Job) -> bool;
    /// Supersede every job of an older generation.
    fn set_generation(&self, generation: u64);
}
