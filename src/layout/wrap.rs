//! Word wrapping of styled text by display width.

use unicode_width::UnicodeWidthChar;

use super::RowSpan;

/// Input of the wrapper: styled text or a forced line break.
pub enum Piece {
    Text(RowSpan),
    Break,
}

enum Token {
    Word(Vec<RowSpan>),
    Space(RowSpan),
    Break,
}

fn width(text: &str) -> usize {
    text.chars().map(|c| c.width().unwrap_or(0)).sum()
}

fn tokens(pieces: Vec<Piece>) -> Vec<Token> {
    let mut out = Vec::new();
    let mut word: Vec<RowSpan> = Vec::new();
    let flush = |word: &mut Vec<RowSpan>, out: &mut Vec<Token>| {
        if !word.is_empty() {
            out.push(Token::Word(std::mem::take(word)));
        }
    };
    for piece in pieces {
        let span = match piece {
            Piece::Break => {
                flush(&mut word, &mut out);
                out.push(Token::Break);
                continue;
            }
            Piece::Text(span) => span,
        };
        let part = |text: &str| RowSpan {
            text: text.to_owned(),
            style: span.style,
            link: span.link.clone(),
            unit: span.unit,
        };
        // Runs of whitespace and of non-whitespace, sliced out of the text.
        let mut start = 0;
        let mut run_space = false;
        let emit = |text: &str, space: bool, word: &mut Vec<RowSpan>, out: &mut Vec<Token>| {
            if text.is_empty() {
                return;
            }
            if space {
                flush(word, out);
                out.push(Token::Space(part(" ")));
            } else {
                word.push(part(text));
            }
        };
        for (i, c) in span.text.char_indices() {
            let space = c.is_whitespace();
            if space != run_space {
                emit(&span.text[start..i], run_space, &mut word, &mut out);
                start = i;
                run_space = space;
            }
        }
        emit(&span.text[start..], run_space, &mut word, &mut out);
    }
    flush(&mut word, &mut out);
    out
}

struct Lines {
    max: usize,
    rows: Vec<Vec<RowSpan>>,
    cur: Vec<RowSpan>,
    used: usize,
}

impl Lines {
    fn push(&mut self, text: &str, like: &RowSpan) {
        match self.cur.last_mut() {
            Some(last) if last.style == like.style && last.link == like.link => {
                last.text.push_str(text);
            }
            _ => self.cur.push(RowSpan {
                text: text.to_owned(),
                style: like.style,
                link: like.link.clone(),
                unit: like.unit,
            }),
        }
    }

    fn end_line(&mut self) {
        self.rows.push(std::mem::take(&mut self.cur));
        self.used = 0;
    }

    /// Add a word to the current line, breaking it by characters when needed.
    fn place(&mut self, word: &[RowSpan]) {
        for span in word {
            // Fast path: the whole run fits on the line.
            let whole = width(&span.text);
            if self.used + whole <= self.max {
                self.push(&span.text, span);
                self.used += whole;
                continue;
            }
            for c in span.text.chars() {
                let w = c.width().unwrap_or(0);
                if self.used + w > self.max && self.used > 0 {
                    self.end_line();
                }
                self.push(c.encode_utf8(&mut [0; 4]), span);
                self.used += w;
            }
        }
    }
}

/// Wrap `pieces` into lines of at most `max` display columns.
pub fn wrap(pieces: Vec<Piece>, max: usize) -> Vec<Vec<RowSpan>> {
    let mut lines = Lines {
        max,
        rows: Vec::new(),
        cur: Vec::new(),
        used: 0,
    };
    let mut pending: Option<RowSpan> = None;
    for token in tokens(pieces) {
        match token {
            Token::Break => {
                pending = None;
                lines.end_line();
            }
            Token::Space(space) => pending = Some(space),
            Token::Word(word) => {
                let w: usize = word.iter().map(|s| width(&s.text)).sum();
                if lines.used > 0 && lines.used + 1 + w > max {
                    lines.end_line();
                } else if let (Some(space), true) = (pending.as_ref(), lines.used > 0) {
                    lines.push(" ", space);
                    lines.used += 1;
                }
                pending = None;
                lines.place(&word);
            }
        }
    }
    if !lines.cur.is_empty() {
        lines.end_line();
    }
    lines.rows
}

/// Split `text` into chunks of at most `max` columns, keeping every space.
pub fn chunk(text: &str, like: &RowSpan, max: usize) -> Vec<Vec<RowSpan>> {
    let mut lines = Lines {
        max,
        rows: Vec::new(),
        cur: Vec::new(),
        used: 0,
    };
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            lines.end_line();
        }
        lines.place(&[RowSpan {
            text: line.to_owned(),
            style: like.style,
            link: like.link.clone(),
            unit: like.unit,
        }]);
    }
    if !lines.cur.is_empty() {
        lines.end_line();
    }
    lines.rows
}
