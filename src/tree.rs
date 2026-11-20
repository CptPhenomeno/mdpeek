//! Folder scan into a filtered, sorted tree (SPEC §3.7.1-2). No terminal dependency.

use std::io;
use std::path::{Path, PathBuf};

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
/// sorted case-insensitively.
pub fn scan(_root: &Path) -> io::Result<Vec<Node>> {
    Ok(Vec::new())
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
