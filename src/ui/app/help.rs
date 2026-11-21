//! Help overlay (SPEC §3.8), generated from the key map.

use super::App;

impl App {
    /// Lines of the help text: a heading per context, then one row per binding.
    pub(super) fn help_lines(&self) -> Vec<String> {
        Vec::new()
    }
}
