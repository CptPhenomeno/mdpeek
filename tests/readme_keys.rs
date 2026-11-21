//! The README key table stays in sync with the key map (every binding, per
//! context, documentation-only rows included).

use readmd::ui::keymap::{BINDINGS, Context};

const README: &str = include_str!("../README.md");

/// The text of the README section whose `###` heading is `heading`.
fn section<'a>(heading: &str) -> &'a str {
    let marker = format!("### {heading}\n");
    let start = README
        .find(&marker)
        .unwrap_or_else(|| panic!("README has no `### {heading}` section"));
    let body = &README[start + marker.len()..];
    let end = body.find("\n#").unwrap_or(body.len());
    &body[..end]
}

fn heading_of(context: Context) -> &'static str {
    Context::HELP_ORDER
        .iter()
        .find(|(c, _)| *c == context)
        .map(|(_, h)| *h)
        .expect("context has a help heading")
}

#[test]
fn every_binding_is_in_the_readme_key_table() {
    let mut missing = Vec::new();
    for b in BINDINGS {
        let heading = heading_of(b.context);
        let row = format!("| `{}` |", b.label);
        if !section(heading).lines().any(|l| l.starts_with(&row)) {
            missing.push(format!("{heading}: {}", b.label));
        }
    }
    assert!(missing.is_empty(), "missing from README: {missing:#?}");
}
