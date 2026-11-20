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
    ToggleTextDiagrams,
    FocusNext,
    FocusPrev,
    ClearFocus,
    Activate,
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

const NONE: KeyModifiers = KeyModifiers::NONE;

const fn bind(
    code: KeyCode,
    modifiers: KeyModifiers,
    action: Action,
    label: &'static str,
    description: &'static str,
) -> Binding {
    Binding {
        code,
        modifiers,
        action,
        label,
        description,
    }
}

/// Document-focus bindings (SPEC §3.8); later keys and the F1 help extend this table.
pub const BINDINGS: &[Binding] = &[
    bind(
        KeyCode::Up,
        NONE,
        Action::ScrollUp,
        "Up",
        "Scroll one row up",
    ),
    bind(
        KeyCode::Down,
        NONE,
        Action::ScrollDown,
        "Down",
        "Scroll one row down",
    ),
    bind(
        KeyCode::PageUp,
        NONE,
        Action::PageUp,
        "PgUp",
        "Scroll one page up",
    ),
    bind(
        KeyCode::PageDown,
        NONE,
        Action::PageDown,
        "PgDn",
        "Scroll one page down",
    ),
    bind(
        KeyCode::Char(' '),
        NONE,
        Action::PageDown,
        "Space",
        "Scroll one page down",
    ),
    bind(
        KeyCode::Home,
        NONE,
        Action::Top,
        "Home",
        "Top of the document",
    ),
    bind(
        KeyCode::End,
        NONE,
        Action::Bottom,
        "End",
        "Bottom of the document",
    ),
    bind(
        KeyCode::Tab,
        NONE,
        Action::FocusNext,
        "Tab",
        "Focus the next link or diagram",
    ),
    bind(
        KeyCode::BackTab,
        KeyModifiers::SHIFT,
        Action::FocusPrev,
        "Shift+Tab",
        "Focus the previous link or diagram",
    ),
    bind(
        KeyCode::Esc,
        NONE,
        Action::ClearFocus,
        "Esc",
        "Clear the focus",
    ),
    bind(
        KeyCode::Enter,
        NONE,
        Action::Activate,
        "Enter",
        "Open the focused link or diagram",
    ),
    bind(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL,
        Action::Quit,
        "Ctrl+Q",
        "Quit",
    ),
    bind(
        KeyCode::F(8),
        NONE,
        Action::ToggleTextDiagrams,
        "F8",
        "Toggle text diagrams",
    ),
    bind(KeyCode::F(10), NONE, Action::Quit, "F10", "Quit"),
];

/// Action bound to `key`, if any.
pub fn action_for(key: KeyEvent) -> Option<Action> {
    BINDINGS
        .iter()
        .find(|b| b.code == key.code && b.modifiers == key.modifiers)
        .map(|b| b.action)
}
