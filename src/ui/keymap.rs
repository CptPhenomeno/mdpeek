//! Central key map: key -> action table shared by dispatch and the F1 help.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Everything a key can ask the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Quit,
}

/// One row of the key map.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub action: Action,
    /// Key name as listed in the help.
    pub label: &'static str,
    /// What the action does, for the help.
    pub description: &'static str,
}

/// Document-focus bindings (SPEC §3.8).
pub const BINDINGS: &[Binding] = &[];

/// Action bound to `key`, if any.
pub fn action_for(_key: KeyEvent) -> Option<Action> {
    None
}
