//! Where the viewer sends folder scans. The real one runs them on a background
//! thread, so a big folder never blocks a frame; tests inject their own.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use crate::tree::{self, Node};

/// What a finished scan reports.
pub type ScanResult = io::Result<Vec<Node>>;

/// Runs folder scans for the viewer.
pub trait ScanSink: std::fmt::Debug {
    /// Starts scanning `root`. Returns the result when it is already known
    /// (the inline scanner), or `None` when it will come back later through
    /// `App::on_scan`.
    fn request(&self, root: &Path) -> Option<ScanResult>;
}

/// Scans on the calling thread: the default, and what the unit tests use.
#[derive(Debug)]
pub struct InlineScanner;

impl ScanSink for InlineScanner {
    fn request(&self, root: &Path) -> Option<ScanResult> {
        Some(tree::scan(root))
    }
}

/// Scans on a short-lived thread and sends the result down a channel that the
/// event loop polls.
#[derive(Debug)]
pub struct ThreadScanner(pub Sender<ScanResult>);

impl ScanSink for ThreadScanner {
    fn request(&self, root: &Path) -> Option<ScanResult> {
        let tx = self.0.clone();
        let root: PathBuf = root.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("readmd-scan".into())
            .spawn(move || {
                // The receiver is gone only when the viewer quit.
                let _ = tx.send(tree::scan(&root));
            });
        match spawned {
            Ok(_) => None,
            Err(err) => Some(Err(err)),
        }
    }
}
