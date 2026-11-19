//! Terminal lifecycle: raw mode, alternate screen, cursor, panic safety.

use std::io;

/// The three terminal states `readmd` changes and must always undo.
pub trait TerminalOps {
    fn disable_raw_mode(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
}

/// Undo every terminal change; all steps run even if one fails.
pub fn restore(ops: &mut impl TerminalOps) -> io::Result<()> {
    let steps = [
        ops.leave_alternate_screen(),
        ops.disable_raw_mode(),
        ops.show_cursor(),
    ];
    steps.into_iter().collect()
}

/// Install a panic hook that runs `restore_fn` before the previous hook.
pub fn install_panic_hook(restore_fn: fn()) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_fn();
        previous(info);
    }));
}

/// Terminal operations backed by crossterm on stdout.
struct CrosstermOps;

impl TerminalOps for CrosstermOps {
    fn disable_raw_mode(&mut self) -> io::Result<()> {
        crossterm::terminal::disable_raw_mode()
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        crossterm::execute!(io::stdout(), crossterm::cursor::Show)
    }
}

/// Restore the real terminal, ignoring errors (used from the panic hook and `Drop`).
pub fn restore_terminal() {
    let _ = restore(&mut CrosstermOps);
}

/// Puts the terminal in raw mode on the alternate screen; restores it on drop.
pub struct TerminalGuard(());

impl TerminalGuard {
    pub fn enter() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        // From here on the guard owns the restore, even if the next step fails.
        let guard = Self(());
        crossterm::execute!(
            io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::cursor::Hide
        )?;
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    static RESTORED: AtomicBool = AtomicBool::new(false);

    #[test]
    fn restore_routine_runs_when_the_panic_hook_fires() {
        install_panic_hook(|| RESTORED.store(true, Ordering::SeqCst));
        let _ = std::panic::catch_unwind(|| panic!("boom"));
        let _ = std::panic::take_hook();
        assert!(RESTORED.load(Ordering::SeqCst));
    }
}
