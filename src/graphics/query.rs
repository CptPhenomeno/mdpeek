//! Terminal query that owns its reads (KB-84).
//!
//! ratatui-image's query leaves a thread blocked on stdin when the terminal never
//! answers, and that thread swallows the first key press. Here the reads happen on
//! the caller's thread, bounded by a deadline, so nothing keeps reading afterwards.

use std::io;
use std::time::Duration;

use ratatui_image::picker::{
    ProtocolType,
    cap_parser::{Parser, Response},
};

/// Where the terminal's answers come from; a fake in tests.
pub trait TtyInput {
    /// Waits up to `wait` for bytes. Returns what arrived (`None` when the wait
    /// ran out) and how long the call took.
    fn read(&mut self, wait: Duration) -> io::Result<(Option<Vec<u8>>, Duration)>;
}

/// What the terminal reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Answer {
    /// Protocol from the capability replies (Kitty, else Sixel), if any.
    pub protocol: Option<ProtocolType>,
    /// Cell size in pixels from the `CSI 16 t` reply.
    pub cell: Option<(u16, u16)>,
}

/// Result of one query round.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// `None` when the terminal did not finish answering in time.
    pub answer: Option<Answer>,
    /// Input that was not part of a terminal report (keys typed meanwhile).
    pub leftover: Vec<u8>,
}

/// One piece of the input stream.
#[derive(Debug, PartialEq, Eq)]
enum Piece {
    /// A complete CSI, OSC, DCS or APC sequence (what terminals reply with).
    Report(Vec<u8>),
    /// A byte outside any such sequence: a key press.
    Plain(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    Csi,
    /// OSC, DCS, APC and friends: end at BEL or ESC `\`.
    Str,
    StrEsc,
}

/// Splits raw input into terminal reports and plain bytes, across reads.
struct Splitter {
    state: State,
    buf: Vec<u8>,
}

impl Splitter {
    fn new() -> Self {
        Self {
            state: State::Ground,
            buf: Vec::new(),
        }
    }

    /// A report has started but not ended yet.
    fn in_report(&self) -> bool {
        self.state != State::Ground
    }

    fn feed(&mut self, bytes: &[u8]) -> Vec<Piece> {
        let mut out = Vec::new();
        for &b in bytes {
            self.buf.push(b);
            self.state = match (self.state, b) {
                (State::Ground, 0x1b) => State::Esc,
                (State::Ground, _) => {
                    self.buf.clear();
                    out.push(Piece::Plain(b));
                    State::Ground
                }
                (State::Esc, b'[') => State::Csi,
                (State::Esc, b']' | b'_' | b'P' | b'^' | b'X') => State::Str,
                (State::Esc, _) => self.abort(&mut out),
                (State::Csi, 0x20..=0x3f) => State::Csi,
                (State::Csi, 0x40..=0x7e) => self.finish(&mut out),
                (State::Csi, _) => self.abort(&mut out),
                (State::Str, 0x07) => self.finish(&mut out),
                (State::Str, 0x1b) => State::StrEsc,
                (State::Str, _) => State::Str,
                (State::StrEsc, b'\\') => self.finish(&mut out),
                (State::StrEsc, 0x1b) => State::StrEsc,
                (State::StrEsc, _) => State::Str,
            };
        }
        out
    }

    fn finish(&mut self, out: &mut Vec<Piece>) -> State {
        out.push(Piece::Report(std::mem::take(&mut self.buf)));
        State::Ground
    }

    /// Not a report after all: its bytes are key presses.
    fn abort(&mut self, out: &mut Vec<Piece>) -> State {
        out.extend(std::mem::take(&mut self.buf).into_iter().map(Piece::Plain));
        State::Ground
    }
}

/// What the parsed replies add up to, as ratatui-image's picker reads them;
/// `None` when there were none.
fn interpret(responses: &[Response]) -> Option<Answer> {
    if responses.is_empty() {
        return None;
    }
    let mut answer = Answer::default();
    for response in responses {
        match response {
            Response::Kitty => answer.protocol = Some(ProtocolType::Kitty),
            Response::Sixel if answer.protocol.is_none() => {
                answer.protocol = Some(ProtocolType::Sixel);
            }
            Response::CellSize(Some((w, h))) if *w > 0 && *h > 0 => answer.cell = Some((*w, *h)),
            _ => {}
        }
    }
    Some(answer)
}

/// Reads the answers to an already sent query until the status reply arrives or
/// `timeout` runs out. After a timeout it keeps discarding late reports for up to
/// `grace` (more while one is half received), so they never reach the key reader.
/// Plain bytes met on the way are returned as `leftover` and end the grace period.
pub fn read_answers(input: &mut dyn TtyInput, timeout: Duration, grace: Duration) -> Outcome {
    let mut parser = Parser::new();
    let mut splitter = Splitter::new();
    let mut responses = Vec::new();
    let mut out = Outcome::default();
    let mut elapsed = Duration::ZERO;
    let mut done = false;
    while !done && elapsed < timeout {
        let Ok((chunk, took)) = input.read(timeout - elapsed) else {
            break;
        };
        elapsed += took.max(Duration::from_millis(1));
        for piece in chunk.iter().flat_map(|c| splitter.feed(c)) {
            match piece {
                Piece::Plain(b) => out.leftover.push(b),
                Piece::Report(bytes) => {
                    for ch in bytes {
                        let more = parser.push(char::from(ch));
                        if more == [Response::Status] {
                            done = true;
                        } else {
                            responses.extend(more);
                        }
                    }
                }
            }
        }
    }
    if done {
        out.answer = interpret(&responses);
        return out;
    }
    // Timed out: whatever arrives now is late.
    let mut spent = Duration::ZERO;
    let cap = grace * 2;
    while out.leftover.is_empty() && (spent < grace || (splitter.in_report() && spent < cap)) {
        let limit = if spent < grace { grace } else { cap };
        let Ok((chunk, took)) = input.read(limit - spent) else {
            break;
        };
        spent += took.max(Duration::from_millis(1));
        for piece in chunk.iter().flat_map(|c| splitter.feed(c)) {
            if let Piece::Plain(b) = piece {
                out.leftover.push(b);
            }
        }
    }
    out
}

/// The process's stdin, read with `poll(2)` and a deadline: no thread, nothing
/// left blocked once a wait ends. The terminal must already be in raw mode.
pub struct StdinTty;

impl TtyInput for StdinTty {
    fn read(&mut self, wait: Duration) -> io::Result<(Option<Vec<u8>>, Duration)> {
        use rustix::event::{PollFd, PollFlags, Timespec, poll};
        let start = std::time::Instant::now();
        let stdin = rustix::stdio::stdin();
        let mut fds = [PollFd::new(&stdin, PollFlags::IN)];
        let timeout = Timespec {
            tv_sec: i64::try_from(wait.as_secs()).unwrap_or(i64::MAX),
            tv_nsec: i64::from(wait.subsec_nanos()),
        };
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Ok((None, start.elapsed())),
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => return Ok((None, start.elapsed())),
            Err(e) => return Err(e.into()),
        }
        let mut buf = [0u8; 256];
        let n = rustix::io::read(stdin, &mut buf)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok((Some(buf[..n].to_vec()), start.elapsed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripted input with a virtual clock: chunks arrive at fixed times.
    struct Script {
        now: Duration,
        chunks: Vec<(Duration, Vec<u8>)>,
    }

    impl Script {
        fn new(chunks: &[(u64, &[u8])]) -> Self {
            Self {
                now: Duration::ZERO,
                chunks: chunks
                    .iter()
                    .map(|(ms, b)| (Duration::from_millis(*ms), b.to_vec()))
                    .collect(),
            }
        }
        fn unread(&self) -> usize {
            self.chunks.len()
        }
    }

    impl TtyInput for Script {
        fn read(&mut self, wait: Duration) -> io::Result<(Option<Vec<u8>>, Duration)> {
            let start = self.now;
            if let Some((at, _)) = self.chunks.first()
                && *at <= start + wait
            {
                let (at, bytes) = self.chunks.remove(0);
                self.now = at.max(start);
                return Ok((Some(bytes), self.now - start));
            }
            self.now = start + wait;
            Ok((None, wait))
        }
    }

    const TIMEOUT: Duration = Duration::from_millis(250);
    const GRACE: Duration = Duration::from_millis(100);

    #[test]
    fn next_key_reaches_the_app_after_a_silent_timeout() {
        // The terminal never answers; the user presses q at 400 ms.
        let mut tty = Script::new(&[(400, b"q")]);
        let out = read_answers(&mut tty, TIMEOUT, GRACE);
        assert_eq!(out.answer, None);
        assert!(out.leftover.is_empty());
        assert_eq!(tty.unread(), 1, "the key press must still be unread");
    }

    #[test]
    fn a_late_answer_is_not_turned_into_key_presses() {
        // Answers trickle in after the timeout: CSI ? 62 ; 4 c, CSI 6 ; 20 ; 10 t, CSI 0 n.
        let mut tty = Script::new(&[
            (300, b"\x1b[?62;4c"),
            (320, b"\x1b[6;20;10t"),
            (340, b"\x1b[0n"),
            (2000, b"q"),
        ]);
        let out = read_answers(&mut tty, TIMEOUT, GRACE);
        assert_eq!(out.answer, None, "a late answer is not adopted");
        assert!(out.leftover.is_empty(), "no report byte may leak as keys");
        assert_eq!(tty.unread(), 1, "only the later key press stays unread");
    }

    #[test]
    fn an_answering_terminal_is_read_up_to_the_status_reply() {
        let mut tty = Script::new(&[
            (5, b"\x1b_Gi=31;OK\x1b\\"),
            (6, b"\x1b[?62;4c\x1b[6;20;10t"),
            (7, b"\x1b[0n"),
            (50, b"q"),
        ]);
        let out = read_answers(&mut tty, TIMEOUT, GRACE);
        assert_eq!(
            out.answer,
            Some(Answer {
                protocol: Some(ProtocolType::Kitty),
                cell: Some((10, 20)),
            })
        );
        assert!(out.leftover.is_empty());
        assert_eq!(tty.unread(), 1);
    }

    #[test]
    fn keys_typed_during_detection_are_kept() {
        let mut tty = Script::new(&[(10, b"j"), (20, b"\x1b[?62;4c\x1b[0n")]);
        let out = read_answers(&mut tty, TIMEOUT, GRACE);
        assert_eq!(out.leftover, b"j");
        assert!(out.answer.is_some());
    }

    #[test]
    fn a_report_split_across_reads_is_still_discarded() {
        let mut tty = Script::new(&[(340, b"\x1b[?62"), (380, b";4c"), (2000, b"q")]);
        let out = read_answers(&mut tty, TIMEOUT, GRACE);
        assert!(out.leftover.is_empty());
        assert_eq!(tty.unread(), 1);
    }
}
