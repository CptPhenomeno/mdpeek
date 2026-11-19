//! GitHub-style heading slugs and link classification (SPEC §3.6.2, §4.9).

use super::{Block, Document, Inline};

/// What Enter does with a link target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// `http`, `https` or `mailto`: handed to the system opener.
    External,
    /// `#anchor`: scroll to the matching heading.
    Anchor,
    /// Local paths and other schemes.
    Unsupported,
}

/// Slug of one heading text, without duplicate suffix.
pub fn slugify(_text: &str) -> String {
    todo!()
}

/// Classifies a link target.
pub fn classify_link(_target: &str) -> LinkKind {
    todo!()
}

impl Document {
    /// Unique slugs of all headings in document order (duplicates get `-1`, `-2`, ...).
    pub fn heading_slugs(&self) -> Vec<String> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Source;

    fn slugs(md: &str) -> Vec<String> {
        Document::from_bytes(md.as_bytes(), Source::Markdown).heading_slugs()
    }

    #[test]
    fn slug_lowercases_the_heading_and_turns_spaces_into_hyphens() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugs("# Getting Started\n"), vec!["getting-started"]);
    }

    #[test]
    fn slug_removes_punctuation_but_keeps_hyphens_and_underscores() {
        assert_eq!(slugify("What's new? (v2.0) - a_b!"), "whats-new-v20---a_b");
    }

    #[test]
    fn duplicate_headings_get_suffixes_in_document_order() {
        assert_eq!(
            slugs("# Intro\n## Intro\n### Intro\n# Other\n"),
            vec!["intro", "intro-1", "intro-2", "other"]
        );
    }

    #[test]
    fn slug_keeps_non_ascii_letters() {
        assert_eq!(slugify("Über Café 日本語"), "über-café-日本語");
    }

    #[test]
    fn http_https_and_mailto_links_are_external() {
        for t in [
            "http://a.io",
            "https://a.io/x#y",
            "mailto:a@b.c",
            "HTTPS://A.IO",
        ] {
            assert_eq!(classify_link(t), LinkKind::External, "{t}");
        }
    }

    #[test]
    fn links_starting_with_hash_are_anchors() {
        assert_eq!(classify_link("#intro"), LinkKind::Anchor);
        assert_eq!(classify_link("#"), LinkKind::Anchor);
    }

    #[test]
    fn relative_paths_and_other_schemes_are_unsupported() {
        for t in [
            "other.md",
            "./a/b.md#x",
            "/abs/p",
            "ftp://h/f",
            "file:///x",
            "javascript:x",
            "",
        ] {
            assert_eq!(classify_link(t), LinkKind::Unsupported, "{t}");
        }
    }
}
