//! Code highlighting with `syntect`, producing styled spans (foreground, bold, italic only).

use ratatui::style::Style;

/// One run of text with its style (never carries a background).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub style: Style,
}

/// Highlight `code` for `lang` (token or extension); unknown or missing → plain.
pub fn highlight(_code: &str, _lang: Option<&str>) -> Vec<Vec<Piece>> {
    Vec::new()
}

/// Whether `lang` names a known syntax.
pub fn is_known(_lang: &str) -> bool {
    false
}

/// Address of the loaded syntax set, for tests of the once-only load.
pub fn syntax_set_ptr() -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;
    use std::collections::HashSet;

    const RUST: &str = "fn main() {\n    let x = 42; // answer\n    println!(\"hi {}\", x);\n}\n";

    fn fgs(lines: &[Vec<Piece>]) -> HashSet<Option<Color>> {
        lines.iter().flatten().map(|p| p.style.fg).collect()
    }

    fn joined(lines: &[Vec<Piece>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|p| p.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn rust_has_several_foreground_colours() {
        let out = highlight(RUST, Some("rust"));
        assert!(fgs(&out).len() > 1);
        assert!(out.iter().flatten().all(|p| p.style.bg.is_none()));
    }

    #[test]
    fn unknown_language_is_plain() {
        let out = highlight("a b\nc\n", Some("unknownlang"));
        assert_eq!(joined(&out), vec!["a b", "c"]);
        assert!(out.iter().flatten().all(|p| p.style == Style::default()));
    }

    #[test]
    fn no_language_is_plain() {
        let out = highlight("fn main() {}\nx\n", None);
        assert_eq!(joined(&out), vec!["fn main() {}", "x"]);
        assert!(out.iter().flatten().all(|p| p.style == Style::default()));
    }

    #[test]
    fn aliases_resolve() {
        for l in ["rs", "py", "sh", "rust", "python", "bash", "json", "yaml"] {
            assert!(is_known(l), "{l}");
        }
        assert!(!is_known("unknownlang"));
        let a = highlight("def f(): pass\n", Some("py"));
        assert!(fgs(&a).len() > 1);
    }

    #[test]
    fn text_is_preserved() {
        let samples = [
            (RUST, Some("rs")),
            ("{\"a\": [1, 2, null]}\n", Some("json")),
            ("echo \"x\" | grep y\n\n  tail", Some("sh")),
            ("tab\tand  spaces  \nno newline at end", Some("nope")),
            ("", Some("rust")),
        ];
        for (code, lang) in samples {
            let expected: Vec<&str> = code.lines().collect();
            assert_eq!(joined(&highlight(code, lang)), expected, "{lang:?}");
        }
    }

    #[test]
    fn syntax_set_loaded_once() {
        let _ = highlight("x", Some("rust"));
        let a = syntax_set_ptr();
        let _ = highlight("y", Some("py"));
        assert_ne!(a, 0);
        assert_eq!(a, syntax_set_ptr());
    }
}
