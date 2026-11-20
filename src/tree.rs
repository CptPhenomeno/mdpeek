//! Folder scan into a filtered, sorted tree (SPEC §3.7.1-2). No terminal dependency.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::cli::has_supported_extension;

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
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .require_git(false)
        .git_global(false)
        .git_exclude(false)
        .ignore(false)
        .parents(true)
        .follow_links(false)
        .build();
    let mut files = Vec::new();
    for entry in walker {
        let entry = entry.map_err(io::Error::other)?;
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
    let existing = level
        .iter()
        .position(|n| matches!(n, Node::Dir { path: p, .. } if *p == path));
    let idx = existing.unwrap_or_else(|| {
        level.push(Node::Dir {
            name,
            path: path.clone(),
            children: Vec::new(),
        });
        level.len() - 1
    });
    if let Node::Dir { children, .. } = &mut level[idx] {
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
}
