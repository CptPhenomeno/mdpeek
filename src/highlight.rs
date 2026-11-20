//! Code highlighting with `syntect`, producing styled spans (foreground, bold, italic only).

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style as SynStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

/// One run of text with its style (never carries a background).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub style: Style,
}

/// Bundled dark theme. `base16-ocean.dark` has soft, mid-brightness colours
/// that stay readable on `theme::CODE_BG`; only its foregrounds are used.
const THEME_NAME: &str = "base16-ocean.dark";

struct Engine {
    syntaxes: SyntaxSet,
    theme: Theme,
}

/// Syntax set and theme, loaded on first use and then shared.
fn engine() -> &'static Option<Engine> {
    static ENGINE: OnceLock<Option<Engine>> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let theme = ThemeSet::load_defaults().themes.remove(THEME_NAME)?;
        Some(Engine {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            theme,
        })
    })
}

fn find<'a>(engine: &'a Engine, lang: &str) -> Option<&'a SyntaxReference> {
    let lang = lang.trim();
    if lang.is_empty() {
        return None;
    }
    engine
        .syntaxes
        .find_syntax_by_token(lang)
        .or_else(|| engine.syntaxes.find_syntax_by_extension(lang))
}

fn plain(code: &str) -> Vec<Vec<Piece>> {
    code.lines()
        .map(|l| {
            vec![Piece {
                text: l.to_string(),
                style: Style::default(),
            }]
        })
        .collect()
}

/// Highlight `code` for `lang` (token or extension); unknown or missing → plain.
/// Lines carry no line terminator; the text of each line is kept exactly.
pub fn highlight(code: &str, lang: Option<&str>) -> Vec<Vec<Piece>> {
    let Some(engine) = engine().as_ref() else {
        return plain(code);
    };
    let Some(syntax) = lang.and_then(|l| find(engine, l)) else {
        return plain(code);
    };
    let mut h = HighlightLines::new(syntax, &engine.theme);
    let mut out = Vec::new();
    for line in LinesWithEndings::from(code) {
        let Ok(ranges) = h.highlight_line(line, &engine.syntaxes) else {
            out.push(
                plain(line.trim_end_matches(['\n', '\r']))
                    .pop()
                    .unwrap_or_default(),
            );
            continue;
        };
        let mut pieces = Vec::new();
        for (st, text) in ranges {
            let text = text.trim_end_matches(['\n', '\r']);
            if text.is_empty() {
                continue;
            }
            pieces.push(Piece {
                text: text.to_string(),
                style: to_style(st),
            });
        }
        out.push(pieces);
    }
    out
}

fn to_style(st: SynStyle) -> Style {
    let c = st.foreground;
    let mut style = Style::default().fg(Color::Rgb(c.r, c.g, c.b));
    if st.font_style.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if st.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    style
}

/// Whether `lang` names a known syntax.
pub fn is_known(lang: &str) -> bool {
    engine().as_ref().is_some_and(|e| find(e, lang).is_some())
}

/// Address of the loaded syntax set, for tests of the once-only load.
pub fn syntax_set_ptr() -> usize {
    engine()
        .as_ref()
        .map_or(0, |e| &e.syntaxes as *const SyntaxSet as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
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
