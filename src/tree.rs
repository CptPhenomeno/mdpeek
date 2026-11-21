//! Folder scan into a filtered, sorted tree (SPEC §3.7.1-2). No terminal dependency.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

pub mod nav;

use ignore::WalkBuilder;

use crate::cli::{StartTarget, has_supported_extension};

/// One entry of the scanned tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A directory that leads to at least one supported file.
    Dir {
        name: String,
        path: PathBuf,
        children: Vec<Node>,
    },
    /// A supported document.
    File { name: String, path: PathBuf },
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Self::Dir { name, .. } | Self::File { name, .. } => name,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Dir { path, .. } | Self::File { path, .. } => path,
        }
    }
}

/// Scan `root` and return its top-level entries, directories first, each group
/// sorted case-insensitively. Hidden entries and `.gitignore` matches are
/// skipped, with or without a git repository around `root`.
pub fn scan(root: &Path) -> io::Result<Vec<Node>> {
    // Only an unreadable or missing root is an error; deeper failures are skipped.
    std::fs::read_dir(root)?;
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .require_git(false)
        .git_global(false)
        .git_exclude(false)
        .ignore(false)
        .parents(true)
        .follow_links(false)
        .sort_by_file_name(|a, b| a.cmp(b))
        .build();
    let mut files = Vec::new();
    for entry in walker {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_some_and(|t| t.is_file()) && has_supported_extension(entry.path()) {
            files.push(entry.into_path());
        }
    }
    let mut top = Vec::new();
    for file in files {
        if let Ok(rel) = file.strip_prefix(root) {
            let parts: Vec<_> = rel.components().map(|c| c.as_os_str()).collect();
            insert(&mut top, root, &parts, &file);
        }
    }
    sort(&mut top);
    Ok(top)
}

fn insert(level: &mut Vec<Node>, dir: &Path, parts: &[&OsStr], file: &Path) {
    let Some((first, rest)) = parts.split_first() else {
        return;
    };
    let name = first.to_string_lossy().into_owned();
    if rest.is_empty() {
        level.push(Node::File {
            name,
            path: file.to_path_buf(),
        });
        return;
    }
    let path = dir.join(first);
    // The walk is sorted depth-first, so a directory's files are contiguous
    // and only the last pushed entry can be the directory we need.
    if !matches!(level.last(), Some(Node::Dir { path: p, .. }) if *p == path) {
        level.push(Node::Dir {
            name,
            path: path.clone(),
            children: Vec::new(),
        });
    }
    if let Some(Node::Dir { children, .. }) = level.last_mut() {
        insert(children, &path, rest, file);
    }
}

fn sort(nodes: &mut [Node]) {
    nodes.sort_by_cached_key(|n| (matches!(n, Node::File { .. }), n.name().to_lowercase()));
    for n in nodes {
        if let Node::Dir { children, .. } = n {
            sort(children);
        }
    }
}

/// The start-up decision derived from the start target (SPEC §3.1.1-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub root: PathBuf,
    pub sidebar_visible: bool,
    /// `None` when not scanned yet: a file target starts with the sidebar hidden,
    /// so the scan is deferred until the sidebar is first shown.
    pub tree: Option<Vec<Node>>,
    /// `None` means the empty state.
    pub initial_file: Option<PathBuf>,
}

/// Root, sidebar visibility and start file of `target` without touching the
/// disk: a directory is not scanned (the viewer scans it in the background and
/// then applies `start_file`), a file starts with its parent as the root.
pub fn start_pending(target: &StartTarget) -> Start {
    match target {
        StartTarget::File(file) => {
            let root = match file.parent() {
                Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
                _ => PathBuf::from("."),
            };
            Start {
                root,
                sidebar_visible: false,
                tree: None,
                initial_file: Some(file.clone()),
            }
        }
        StartTarget::Directory(root) => Start {
            root: root.clone(),
            sidebar_visible: true,
            tree: None,
            initial_file: None,
        },
    }
}

/// Decide root, sidebar visibility, tree and initial file for `target`,
/// scanning a directory right away.
pub fn start(target: &StartTarget) -> io::Result<Start> {
    let mut start = start_pending(target);
    if let StartTarget::Directory(root) = target {
        let tree = scan(root)?;
        start.initial_file = start_file(&tree);
        start.tree = Some(tree);
    }
    Ok(start)
}

/// The document a directory start opens: README.md among the root's own files
/// (case-insensitive), else the first file in tree order (SPEC §3.1.2).
pub fn start_file(tree: &[Node]) -> Option<PathBuf> {
    let readme = tree.iter().find_map(|n| match n {
        Node::File { name, path } if name.eq_ignore_ascii_case("readme.md") => Some(path.clone()),
        _ => None,
    });
    readme.or_else(|| first_file(tree))
}

/// First file in depth-first tree order.
fn first_file(nodes: &[Node]) -> Option<PathBuf> {
    nodes.iter().find_map(|n| match n {
        Node::File { path, .. } => Some(path.clone()),
        Node::Dir { children, .. } => first_file(children),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, "x").unwrap();
    }

    fn dir_target(root: &Path) -> StartTarget {
        StartTarget::Directory(root.to_path_buf())
    }

    #[test]
    fn file_target_uses_parent_as_root_and_hides_sidebar() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "docs/a.md");
        touch(tmp.path(), "docs/b.md");
        let file = tmp.path().join("docs/b.md");
        let s = start(&StartTarget::File(file.clone())).unwrap();
        assert_eq!(s.root, tmp.path().join("docs"));
        assert!(!s.sidebar_visible);
        assert_eq!(s.initial_file, Some(file));
        assert!(s.tree.is_none(), "file target must not scan its parent");
    }

    #[test]
    fn file_target_never_fails_because_of_its_parent_directory() {
        let file = PathBuf::from("/nonexistent-readmd-dir/a.md");
        let s = start(&StartTarget::File(file.clone())).unwrap();
        assert_eq!(s.initial_file, Some(file));
    }

    #[test]
    fn directory_target_opens_readme_case_insensitively() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "a/first.md");
        touch(tmp.path(), "ReadMe.MD");
        let s = start(&dir_target(tmp.path())).unwrap();
        assert_eq!(s.root, tmp.path());
        assert!(s.sidebar_visible);
        assert_eq!(s.initial_file, Some(tmp.path().join("ReadMe.MD")));
    }

    #[test]
    fn directory_without_readme_opens_first_file_in_tree_order() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "a.md");
        touch(tmp.path(), "z/inner.md");
        touch(tmp.path(), "sub/README.md"); // nested README is not the start file
        let s = start(&dir_target(tmp.path())).unwrap();
        assert_eq!(s.initial_file, Some(tmp.path().join("sub/README.md")));
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "b.md");
        touch(tmp.path(), "a/x/deep.md");
        let s = start(&dir_target(tmp.path())).unwrap();
        assert_eq!(s.initial_file, Some(tmp.path().join("a/x/deep.md")));
    }

    #[test]
    fn directory_without_supported_files_yields_empty_state() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "notes.txt");
        let s = start(&dir_target(tmp.path())).unwrap();
        assert!(s.sidebar_visible);
        assert_eq!(s.tree, Some(Vec::new()));
        assert_eq!(s.initial_file, None);
    }

    /// Render the tree as indented names, directories suffixed with `/`.
    fn render(nodes: &[Node], depth: usize, out: &mut Vec<String>) {
        for n in nodes {
            match n {
                Node::Dir { name, children, .. } => {
                    out.push(format!("{}{name}/", "  ".repeat(depth)));
                    render(children, depth + 1, out);
                }
                Node::File { name, .. } => out.push(format!("{}{name}", "  ".repeat(depth))),
            }
        }
    }

    fn listing(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        render(&scan(root).unwrap(), 0, &mut out);
        out
    }

    #[test]
    fn lists_only_directories_and_supported_files_in_any_case() {
        let t = TempDir::new().unwrap();
        for f in [
            "a.md",
            "b.markdown",
            "c.mmd",
            "d.mermaid",
            "E.MD",
            "F.Markdown",
            "g.txt",
            "h.rs",
            "noext",
            "docs/i.mmd",
        ] {
            touch(t.path(), f);
        }
        assert_eq!(
            listing(t.path()),
            [
                "docs/",
                "  i.mmd",
                "a.md",
                "b.markdown",
                "c.mmd",
                "d.mermaid",
                "E.MD",
                "F.Markdown"
            ]
        );
    }

    #[test]
    fn skips_dot_prefixed_files_and_directories() {
        let t = TempDir::new().unwrap();
        for f in [
            ".hidden.md",
            ".git/x.md",
            ".notes/y.md",
            "docs/.z.md",
            "docs/ok.md",
        ] {
            touch(t.path(), f);
        }
        assert_eq!(listing(t.path()), ["docs/", "  ok.md"]);
    }

    #[test]
    fn skips_paths_matched_by_gitignore() {
        let t = TempDir::new().unwrap();
        fs::create_dir(t.path().join(".git")).unwrap();
        touch(t.path(), ".gitignore");
        fs::write(t.path().join(".gitignore"), "ignored.md\nbuild/\n").unwrap();
        for f in ["ignored.md", "build/a.md", "kept.md"] {
            touch(t.path(), f);
        }
        assert_eq!(listing(t.path()), ["kept.md"]);
    }

    #[test]
    fn honours_gitignore_outside_a_git_repository() {
        let t = TempDir::new().unwrap();
        fs::write(t.path().join(".gitignore"), "ignored.md\nbuild/\n").unwrap();
        for f in ["ignored.md", "build/a.md", "kept.md"] {
            touch(t.path(), f);
        }
        assert_eq!(listing(t.path()), ["kept.md"]);
    }

    #[test]
    fn omits_directories_without_supported_files_at_any_depth() {
        let t = TempDir::new().unwrap();
        for f in ["empty_of_docs/a.txt", "deep/er/est/b.rs", "has/x/y/doc.md"] {
            touch(t.path(), f);
        }
        fs::create_dir(t.path().join("really_empty")).unwrap();
        assert_eq!(
            listing(t.path()),
            ["has/", "  x/", "    y/", "      doc.md"]
        );
    }

    #[test]
    fn sorts_directories_first_then_files_case_insensitively() {
        let t = TempDir::new().unwrap();
        for f in [
            "b.md",
            "A.md",
            "c.md",
            "Zeta/z.md",
            "alpha/a.md",
            "Beta/b.md",
            "Beta/b2.md",
            "Beta/A.md",
        ] {
            touch(t.path(), f);
        }
        assert_eq!(
            listing(t.path()),
            [
                "alpha/", "  a.md", "Beta/", "  A.md", "  b.md", "  b2.md", "Zeta/", "  z.md",
                "A.md", "b.md", "c.md"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn skips_an_unreadable_subdirectory_without_aborting_the_scan() {
        use std::os::unix::fs::PermissionsExt;
        let t = TempDir::new().unwrap();
        touch(t.path(), "locked/secret.md");
        touch(t.path(), "readable.md");
        let locked = t.path().join("locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let result = scan(t.path());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if fs::read_dir(&locked).is_ok()
            && fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).is_ok()
        {
            let still_readable = fs::read_dir(&locked).is_ok();
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
            if still_readable {
                return; // running as root: permissions do not bite
            }
        }
        let mut out = Vec::new();
        render(&result.unwrap(), 0, &mut out);
        assert_eq!(out, ["readable.md"]);
    }
}
