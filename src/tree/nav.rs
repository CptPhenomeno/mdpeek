//! Keyboard navigation state of the sidebar tree (SPEC §3.7.3-4). No terminal dependency.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::Node;

/// A navigation key, already decoded from the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Home,
    End,
}

/// What the caller must do after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// Open this file; focus stays on the sidebar.
    Open(PathBuf),
}

/// One visible row, ready for drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
    /// This file is the one shown in the document pane.
    pub open: bool,
}

/// Expansion, selection and open-file mark over a scanned tree. Expansion and
/// selection are keyed by path, so they survive a rescan.
#[derive(Debug, Clone, Default)]
pub struct SidebarNav {
    nodes: Vec<Node>,
    expanded: HashSet<PathBuf>,
    selected: Option<PathBuf>,
    open_file: Option<PathBuf>,
}

impl SidebarNav {
    /// Top-level entries are always visible (the root is implicitly expanded);
    /// the ancestors of `open_file` start expanded and it is marked.
    pub fn new(nodes: Vec<Node>, open_file: Option<&Path>) -> Self {
        let mut nav = Self {
            nodes,
            ..Self::default()
        };
        nav.set_open_file(open_file);
        if nav.selected.is_none() {
            nav.selected = nav.rows().first().map(|r| r.path.clone());
        }
        nav
    }

    /// Replace the tree after a rescan, keeping expansion and selection where
    /// the paths still exist.
    pub fn rebuild(&mut self, nodes: Vec<Node>) {
        let mut dirs = HashSet::new();
        collect_dirs(&nodes, &mut dirs);
        self.expanded.retain(|p| dirs.contains(p));
        self.nodes = nodes;
        let rows = self.rows();
        let kept = self
            .selected
            .as_ref()
            .is_some_and(|s| rows.iter().any(|r| &r.path == s));
        if !kept {
            self.selected = rows.first().map(|r| r.path.clone());
        }
    }

    /// Mark `path` as open and expand its ancestors; it becomes the selection
    /// when it is in the tree.
    pub fn set_open_file(&mut self, path: Option<&Path>) {
        self.open_file = path.map(Path::to_path_buf);
        let Some(path) = path else { return };
        let mut dirs = HashSet::new();
        collect_dirs(&self.nodes, &mut dirs);
        for ancestor in path.ancestors().skip(1) {
            if dirs.contains(ancestor) {
                self.expanded.insert(ancestor.to_path_buf());
            }
        }
        if self.rows().iter().any(|r| r.path == path) {
            self.selected = Some(path.to_path_buf());
        }
    }

    /// Visible rows, top to bottom.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        self.flatten(&self.nodes, 0, &mut rows);
        rows
    }

    fn flatten(&self, nodes: &[Node], depth: usize, out: &mut Vec<Row>) {
        for node in nodes {
            let expanded = self.expanded.contains(node.path());
            out.push(Row {
                name: node.name().to_string(),
                path: node.path().to_path_buf(),
                depth,
                is_dir: matches!(node, Node::Dir { .. }),
                expanded,
                open: self.open_file.as_deref() == Some(node.path()),
            });
            if let Node::Dir { children, .. } = node
                && expanded
            {
                self.flatten(children, depth + 1, out);
            }
        }
    }

    /// Index of the selection in `rows()`, for scrolling the pane.
    pub fn selected_index(&self) -> Option<usize> {
        let sel = self.selected.as_ref()?;
        self.rows().iter().position(|r| &r.path == sel)
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    pub fn handle(&mut self, key: NavKey) -> Outcome {
        let rows = self.rows();
        if rows.is_empty() {
            return Outcome::None;
        }
        let idx = self.selected_index().unwrap_or(0);
        let row = &rows[idx];
        let select = |nav: &mut Self, i: usize| nav.selected = Some(rows[i].path.clone());
        match key {
            NavKey::Up => select(self, idx.saturating_sub(1)),
            NavKey::Down => select(self, (idx + 1).min(rows.len() - 1)),
            NavKey::Home => select(self, 0),
            NavKey::End => select(self, rows.len() - 1),
            NavKey::Enter | NavKey::Right => {
                if row.is_dir {
                    self.expanded.insert(row.path.clone());
                } else if key == NavKey::Enter {
                    return Outcome::Open(row.path.clone());
                }
            }
            NavKey::Left => {
                if row.is_dir && row.expanded {
                    self.expanded.remove(&row.path);
                } else if let Some(p) = rows[..idx].iter().rposition(|r| r.depth < row.depth) {
                    select(self, p);
                }
            }
        }
        Outcome::None
    }
}

fn collect_dirs(nodes: &[Node], out: &mut HashSet<PathBuf>) {
    for node in nodes {
        if let Node::Dir { path, children, .. } = node {
            out.insert(path.clone());
            collect_dirs(children, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(p: &str) -> Node {
        let path = PathBuf::from(p);
        Node::File {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            path,
        }
    }

    fn dir(p: &str, children: Vec<Node>) -> Node {
        let path = PathBuf::from(p);
        Node::Dir {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            path,
            children,
        }
    }

    /// /r: docs/{a.md, deep/b.md}, z.md
    fn tree() -> Vec<Node> {
        vec![
            dir(
                "/r/docs",
                vec![
                    dir("/r/docs/deep", vec![file("/r/docs/deep/b.md")]),
                    file("/r/docs/a.md"),
                ],
            ),
            file("/r/z.md"),
        ]
    }

    fn names(nav: &SidebarNav) -> Vec<String> {
        nav.rows().into_iter().map(|r| r.name).collect()
    }

    fn selected(nav: &SidebarNav) -> String {
        nav.selected_path().unwrap().to_string_lossy().into_owned()
    }

    #[test]
    fn up_and_down_move_over_visible_entries() {
        let mut nav = SidebarNav::new(tree(), None);
        assert_eq!(names(&nav), ["docs", "z.md"]);
        assert_eq!(selected(&nav), "/r/docs");
        nav.handle(NavKey::Down);
        assert_eq!(selected(&nav), "/r/z.md");
        nav.handle(NavKey::Down);
        assert_eq!(selected(&nav), "/r/z.md");
        nav.handle(NavKey::Up);
        nav.handle(NavKey::Up);
        assert_eq!(selected(&nav), "/r/docs");
        assert_eq!(nav.selected_index(), Some(0));
    }

    #[test]
    fn enter_or_right_expands_a_collapsed_directory() {
        let mut nav = SidebarNav::new(tree(), None);
        assert_eq!(nav.handle(NavKey::Enter), Outcome::None);
        assert_eq!(names(&nav), ["docs", "deep", "a.md", "z.md"]);
        assert_eq!(nav.rows()[1].depth, 1);
        let mut nav = SidebarNav::new(tree(), None);
        nav.handle(NavKey::Right);
        assert_eq!(names(&nav), ["docs", "deep", "a.md", "z.md"]);
    }

    #[test]
    fn left_collapses_an_expanded_directory() {
        let mut nav = SidebarNav::new(tree(), None);
        nav.handle(NavKey::Right);
        nav.handle(NavKey::Left);
        assert_eq!(names(&nav), ["docs", "z.md"]);
        assert_eq!(selected(&nav), "/r/docs");
    }

    #[test]
    fn left_on_a_file_or_collapsed_directory_selects_its_parent() {
        let mut nav = SidebarNav::new(tree(), None);
        nav.handle(NavKey::Right);
        nav.handle(NavKey::Down);
        nav.handle(NavKey::Right);
        nav.handle(NavKey::Down);
        assert_eq!(selected(&nav), "/r/docs/deep/b.md");
        nav.handle(NavKey::Left);
        assert_eq!(selected(&nav), "/r/docs/deep");
        nav.handle(NavKey::Left);
        nav.handle(NavKey::Left);
        assert_eq!(selected(&nav), "/r/docs");
        // A top-level entry has no parent row: nothing moves.
        nav.handle(NavKey::Left);
        assert_eq!(selected(&nav), "/r/docs");
    }

    #[test]
    fn home_and_end_select_first_and_last_visible_entry() {
        let mut nav = SidebarNav::new(tree(), None);
        nav.handle(NavKey::Right);
        nav.handle(NavKey::End);
        assert_eq!(selected(&nav), "/r/z.md");
        nav.handle(NavKey::Home);
        assert_eq!(selected(&nav), "/r/docs");
    }

    #[test]
    fn enter_on_a_file_requests_opening_it() {
        let mut nav = SidebarNav::new(tree(), None);
        nav.handle(NavKey::End);
        assert_eq!(
            nav.handle(NavKey::Enter),
            Outcome::Open(PathBuf::from("/r/z.md"))
        );
        assert_eq!(selected(&nav), "/r/z.md");
        // Right on a file does nothing.
        assert_eq!(nav.handle(NavKey::Right), Outcome::None);
    }

    #[test]
    fn the_open_file_is_marked() {
        let mut nav = SidebarNav::new(tree(), Some(Path::new("/r/z.md")));
        let marked: Vec<_> = nav
            .rows()
            .into_iter()
            .filter(|r| r.open)
            .map(|r| r.name)
            .collect();
        assert_eq!(marked, ["z.md"]);
        nav.set_open_file(Some(Path::new("/r/docs/a.md")));
        let marked: Vec<_> = nav
            .rows()
            .into_iter()
            .filter(|r| r.open)
            .map(|r| r.name)
            .collect();
        assert_eq!(marked, ["a.md"]);
    }

    #[test]
    fn ancestors_of_the_open_file_start_expanded() {
        let nav = SidebarNav::new(tree(), Some(Path::new("/r/docs/deep/b.md")));
        assert_eq!(names(&nav), ["docs", "deep", "b.md", "a.md", "z.md"]);
        assert_eq!(selected(&nav), "/r/docs/deep/b.md");
        let mut nav = SidebarNav::new(tree(), Some(Path::new("/r/docs/deep/b.md")));
        nav.handle(NavKey::End);
        // Rescan keeps expansion and selection while the paths exist.
        nav.rebuild(tree());
        assert_eq!(names(&nav), ["docs", "deep", "b.md", "a.md", "z.md"]);
        assert_eq!(selected(&nav), "/r/z.md");
        nav.rebuild(vec![file("/r/y.md")]);
        assert_eq!(selected(&nav), "/r/y.md");
    }
}
