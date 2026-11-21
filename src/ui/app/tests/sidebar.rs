use std::fs;
use std::path::Path;

use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::TestBackend};
use tempfile::TempDir;

use super::*;
use crate::theme;
use crate::tree::{self, Node, Start};

const S: KeyCode = KeyCode::Char('s');
fn ctrl(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::CONTROL));
}

const W: u16 = 80;
const H: u16 = 10 + CHROME;

/// root: a.md (long), z.md, sub/b.md.
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.md"), format!("# Alpha\n\n{}", many(40))).unwrap();
    fs::write(dir.path().join("z.md"), "# Zed\n\nlast file").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/b.md"), "# Bee").unwrap();
    dir
}

fn dir_start(root: &Path) -> Start {
    let tree = tree::scan(root).unwrap();
    Start {
        root: root.to_path_buf(),
        sidebar_visible: true,
        tree: Some(tree),
        initial_file: Some(root.join("a.md")),
    }
}

fn file_start(root: &Path) -> Start {
    Start {
        root: root.to_path_buf(),
        sidebar_visible: false,
        tree: None,
        initial_file: Some(root.join("a.md")),
    }
}

fn app_with(start: Start) -> App {
    let doc = match &start.initial_file {
        Some(p) => Document::from_path(p).unwrap(),
        None => Document {
            empty: true,
            ..Document::default()
        },
    };
    let rel = start
        .initial_file
        .as_ref()
        .and_then(|p| p.strip_prefix(&start.root).ok())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut app = App::new(doc, rel);
    app.set_start(start);
    app.resize(W, H);
    app
}

fn draw(app: &mut App) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    terminal
}

#[test]
fn directory_start_shows_the_sidebar_and_file_start_hides_it() {
    let dir = project();
    assert!(app_with(dir_start(dir.path())).sidebar_visible());
    assert!(!app_with(file_start(dir.path())).sidebar_visible());
}

#[test]
fn s_toggles_the_sidebar_and_the_document_pane_takes_the_freed_width() {
    let dir = project();
    let mut app = app_with(file_start(dir.path()));
    assert_eq!(app.document_width(), W - 2);
    press(&mut app, S);
    assert!(app.sidebar_visible());
    assert_eq!(app.document_width(), W - 2 - 30);
    press(&mut app, S);
    assert!(!app.sidebar_visible());
    assert_eq!(app.document_width(), W - 2);
}

#[test]
fn hiding_the_sidebar_while_it_has_focus_moves_focus_to_the_document() {
    let dir = project();
    let mut app = app_with(dir_start(dir.path()));
    ctrl(&mut app, KeyCode::Left);
    assert!(app.sidebar_focused());
    press(&mut app, S);
    assert!(!app.sidebar_visible());
    assert!(!app.sidebar_focused());
}

#[test]
fn ctrl_arrows_switch_focus_between_sidebar_and_document_only_while_the_sidebar_is_visible() {
    let dir = project();
    let mut app = app_with(file_start(dir.path()));
    ctrl(&mut app, KeyCode::Left);
    assert!(!app.sidebar_focused());
    press(&mut app, S);
    assert!(!app.sidebar_focused());
    ctrl(&mut app, KeyCode::Left);
    assert!(app.sidebar_focused());
    ctrl(&mut app, KeyCode::Right);
    assert!(!app.sidebar_focused());
}

#[test]
fn the_focused_pane_has_the_highlighted_border() {
    let dir = project();
    let mut app = app_with(dir_start(dir.path()));
    let fg = |t: &Terminal<TestBackend>, x: u16| Some(t.backend().buffer()[(x, 0)].fg);
    let (on, off) = (theme::border(true).fg, theme::border(false).fg);
    let t = draw(&mut app);
    assert_eq!((fg(&t, 0), fg(&t, 30)), (off, on));
    ctrl(&mut app, KeyCode::Left);
    let t = draw(&mut app);
    assert_eq!((fg(&t, 0), fg(&t, 30)), (on, off));
}

#[test]
fn opening_a_file_from_the_sidebar_renders_it_and_updates_the_status_bar_path() {
    let dir = project();
    let mut app = app_with(dir_start(dir.path()));
    press(&mut app, KeyCode::PageDown);
    assert!(app.scroll() > 0);
    ctrl(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.rel_path(), "z.md");
    assert_eq!(app.scroll(), 0);
    assert!(app.sidebar_focused());
    assert!(screen_text(&mut app, W, H).contains("last file"));
    // A file that cannot be read keeps the current document and says so.
    fs::remove_file(dir.path().join("a.md")).unwrap();
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.rel_path(), "z.md");
    let text = screen_text(&mut app, W, H);
    assert!(text.contains("last file") && text.contains("z.md |"));
}

#[test]
fn directory_without_supported_files_shows_the_empty_state_message() {
    let dir = TempDir::new().unwrap();
    let start = Start {
        root: dir.path().to_path_buf(),
        sidebar_visible: true,
        tree: Some(Vec::<Node>::new()),
        initial_file: None,
    };
    let mut app = app_with(start);
    assert!(screen_text(&mut app, W, H).contains("No supported files"));
}

#[test]
fn snapshot_of_the_sidebar_next_to_a_document() {
    let dir = project();
    let mut app = app_with(dir_start(dir.path()));
    ctrl(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Down);
    let terminal = draw(&mut app);
    insta::assert_snapshot!(terminal.backend());
}

#[test]
fn function_keys_do_nothing_in_the_sidebar_context() {
    let dir = project();
    let mut app = app_with(dir_start(dir.path()));
    press(&mut app, KeyCode::F(2));
    assert!(app.sidebar_visible());
    press(&mut app, KeyCode::F(6));
    assert!(!app.sidebar_focused());
    ctrl(&mut app, KeyCode::Left);
    assert!(app.sidebar_focused());
    press(&mut app, KeyCode::F(6));
    assert!(app.sidebar_focused());
    press(&mut app, KeyCode::F(2));
    assert!(app.sidebar_visible());
}

#[test]
fn plain_arrows_in_the_sidebar_still_collapse_and_expand() {
    let dir = project();
    let mut app = app_with(dir_start(dir.path()));
    ctrl(&mut app, KeyCode::Left);
    assert!(app.sidebar_focused());
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Left);
    assert!(app.sidebar_focused());
}

#[test]
fn d_toggles_text_mode_from_sidebar_focus_too() {
    let dir = project();
    let mut app = app_with(dir_start(dir.path()));
    ctrl(&mut app, KeyCode::Left);
    assert!(!app.text_mode);
    press(&mut app, KeyCode::Char('d'));
    assert!(app.text_mode);
    press(&mut app, KeyCode::F(8));
    assert!(app.text_mode);
}
