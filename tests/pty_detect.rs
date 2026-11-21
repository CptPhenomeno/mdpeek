//! End to end: a terminal that never answers the graphics query must not swallow
//! the first key press (KB-84). Skipped where `python3` or `timeout` is missing.

use std::process::Command;

#[test]
fn first_q_quits_when_the_terminal_never_answers() {
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/pty/quit_after_detect.py"
    );
    let sample = concat!(env!("CARGO_MANIFEST_DIR"), "/SAMPLE.md");
    let status = Command::new("timeout")
        .args([
            "30",
            "python3",
            script,
            env!("CARGO_BIN_EXE_readmd"),
            sample,
        ])
        .status();
    match status {
        Ok(s) => assert!(s.success(), "readmd did not quit after the first q: {s}"),
        Err(e) => eprintln!("skipped: {e}"),
    }
}
