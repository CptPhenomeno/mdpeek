//! Terminal query that owns its reads (KB-84).
//!
//! ratatui-image's query leaves a thread blocked on stdin when the terminal never
//! answers, and that thread swallows the first key press. Here the reads happen on
//! the caller's thread, bounded by a deadline, so nothing keeps reading afterwards.

use std::io;
use std::time::Duration;

use ratatui_image::picker::ProtocolType;

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

/// Reads the answers to an already sent query until the status reply arrives or
/// `timeout` runs out, then keeps discarding late reports for `grace`.
pub fn read_answers(input: &mut dyn TtyInput, timeout: Duration, _grace: Duration) -> Outcome {
    // STUB (red): blocks until the input is exhausted, like the library's thread.
    while let Ok((Some(_), _)) = input.read(timeout * 1000) {}
    Outcome::default()
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
}
