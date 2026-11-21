//! A directory start paints at once; the scan runs in the background (KB-85).

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ratatui::{Terminal, backend::TestBackend};
use tempfile::TempDir;

use super::*;
use crate::tree::{self, Start};
use crate::ui::scanner::{ScanResult, ScanSink};

const W: u16 = 80;
const H: u16 = 10 + CHROME;

/// Records the requested roots and never answers: the test delivers the result.
#[derive(Debug)]
struct FakeScanner(Arc<Mutex<Vec<std::path::PathBuf>>>);

impl ScanSink for FakeScanner {
    fn request(&self, root: &Path) -> Option<ScanResult> {
        self.0.lock().unwrap().push(root.to_path_buf());
        None
    }
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("Readme.md"), "# Welcome\n\nreadme text").unwrap();
    fs::write(dir.path().join("a.md"), "# Alpha").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/b.md"), "# Bee").unwrap();
    dir
}

/// A directory start whose scan has not finished.
fn pending(root: &Path) -> (App, Arc<Mutex<Vec<std::path::PathBuf>>>) {
    let requests = Arc::default();
    let empty = Document {
        empty: true,
        ..Document::default()
    };
    let mut app = App::new(empty, String::new());
    app.set_scan_sink(Box::new(FakeScanner(Arc::clone(&requests))));
    app.set_start(Start {
        root: root.to_path_buf(),
        sidebar_visible: true,
        tree: None,
        initial_file: None,
    });
    app.resize(W, H);
    (app, requests)
}

fn text(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    screen_text(app, W, H)
}

#[test]
fn a_directory_start_draws_its_first_frame_before_the_scan_finishes() {
    let dir = project();
    let (mut app, requests) = pending(dir.path());
    let shown = text(&mut app);
    assert!(shown.contains("Scanning"), "{shown}");
    assert!(!shown.contains("No supported files"), "{shown}");
    assert!(!shown.contains("readme text"), "{shown}");
    assert_eq!(requests.lock().unwrap().len(), 1, "one scan requested");
    press(&mut app, crossterm::event::KeyCode::Char('r'));
    assert_eq!(
        requests.lock().unwrap().len(),
        1,
        "a second request is coalesced"
    );
}

#[test]
fn when_the_scan_finishes_readme_or_the_first_file_opens_and_the_sidebar_fills() {
    let dir = project();
    let (mut app, _) = pending(dir.path());
    app.on_scan(tree::scan(dir.path()));
    let shown = text(&mut app);
    assert!(shown.contains("readme text"), "{shown}");
    assert!(shown.contains("● Readme.md"), "{shown}");
    assert!(shown.contains("▸ sub"), "{shown}");
    assert!(!shown.contains("Scanning"), "{shown}");
}

#[test]
fn a_file_opened_by_the_user_during_the_scan_is_not_replaced_when_the_scan_ends() {
    let dir = project();
    let (mut app, _) = pending(dir.path());
    app.open_file(&dir.path().join("a.md"));
    app.on_scan(tree::scan(dir.path()));
    let shown = text(&mut app);
    assert!(shown.contains("Alpha"), "{shown}");
    assert!(!shown.contains("readme text"), "{shown}");
    assert!(shown.contains("● a.md"), "{shown}");
}
