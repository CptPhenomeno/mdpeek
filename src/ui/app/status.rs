//! Layout of the status bar (SPEC §3.2): a left part and a right part on one row.
//!
//! The right part (mode indicators, match counter, scroll or zoom percentage)
//! keeps its full width and at least one space of gap. The left part (path,
//! warning, message, search input) gives way: it is truncated with "…" to the
//! columns that remain. Every width is in display columns, so a wide character
//! is never split.
//!
//! When even the right part does not fit, its items are dropped by rank,
//! highest rank number first, until the rest fits and the left part keeps a
//! minimum of columns. Rank 0 is the percentage:
//! it is never dropped; below 5 columns it is clipped from the left instead.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const ELLIPSIS: &str = "…";
/// Columns the left part should keep before lower-priority right items go.
const MIN_LEFT: usize = 16;
const SEPARATOR: &str = " | ";

/// The left part of the status bar.
pub(super) enum Left<'a> {
    /// Path, warning and message: truncated at the end.
    Text(String),
    /// A prompt and the text typed after it: the start of the text is cut, so
    /// the end of it and the cursor stay visible.
    Input { prompt: &'a str, typed: String },
}

/// One status row of exactly `width` columns. `right` lists the items in
/// display order with their drop rank (see the module docs).
pub(super) fn compose(left: &Left, right: &[(String, u8)], width: u16) -> String {
    let width = usize::from(width);
    let mut kept: Vec<&(String, u8)> = right.iter().collect();
    let joined = |items: &[&(String, u8)]| {
        let texts: Vec<&str> = items.iter().map(|(t, _)| t.as_str()).collect();
        format!("{} ", texts.join(SEPARATOR))
    };
    let mut right_text = joined(&kept);
    // One column of left margin doubles as the minimum gap. Items are also
    // dropped while the left part would get fewer than MIN_LEFT columns.
    while (right_text.width() + 1 > width || width - right_text.width() - 1 < MIN_LEFT + 1)
        && kept.len() > 1
    {
        let drop = kept
            .iter()
            .enumerate()
            .max_by_key(|(i, (_, rank))| (*rank, *i))
            .map(|(i, _)| i)
            .unwrap_or(0);
        kept.remove(drop);
        right_text = joined(&kept);
    }
    if right_text.width() + 1 > width {
        let bare = right_text.trim_end();
        return pad_to(clip_start(bare, width), width, true);
    }
    let avail = (width - right_text.width()).saturating_sub(2);
    let left_text = match left {
        Left::Text(text) => clip_end(text, avail),
        Left::Input { prompt, typed } => clip_input(prompt, typed, avail),
    };
    let used = 1 + left_text.width() + right_text.width();
    let gap = width.saturating_sub(used);
    let lead = if left_text.is_empty() { "" } else { " " };
    // Without a left part the margin joins the gap.
    let gap = if left_text.is_empty() { gap + 1 } else { gap };
    format!("{lead}{left_text}{}{right_text}", " ".repeat(gap))
}

fn pad_to(text: String, width: usize, at_start: bool) -> String {
    let missing = width.saturating_sub(text.width());
    let pad = " ".repeat(missing);
    if at_start {
        format!("{pad}{text}")
    } else {
        format!("{text}{pad}")
    }
}

/// `text` cut at the end to `avail` columns, ending with "…" when cut.
fn clip_end(text: &str, avail: usize) -> String {
    if text.width() <= avail {
        return text.to_owned();
    }
    if avail == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > avail - 1 {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push_str(ELLIPSIS);
    out
}

/// The last `avail` columns of `text`, without any marker.
fn clip_start(text: &str, avail: usize) -> String {
    let mut chars = Vec::new();
    let mut used = 0;
    for c in text.chars().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > avail {
            break;
        }
        used += w;
        chars.push(c);
    }
    chars.iter().rev().collect()
}

/// `text` cut at the start to `avail` columns, starting with "…" when cut.
fn clip_start_marked(text: &str, avail: usize) -> String {
    if text.width() <= avail {
        return text.to_owned();
    }
    if avail == 0 {
        return String::new();
    }
    format!("{ELLIPSIS}{}", clip_start(text, avail - 1))
}

/// `prompt` + `typed` + the cursor, with the start of `typed` cut when needed.
fn clip_input(prompt: &str, typed: &str, avail: usize) -> String {
    let body = format!("{typed}_");
    if prompt.width() + body.width() <= avail {
        return format!("{prompt}{body}");
    }
    if avail > prompt.width() + 1 {
        let room = avail - prompt.width();
        return format!("{prompt}{}", clip_start_marked(&body, room));
    }
    // No room for the prompt as well: keep the end of what the user typed.
    clip_start_marked(&body, avail)
}
