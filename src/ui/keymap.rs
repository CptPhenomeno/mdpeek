//! Central key map: key -> action table shared by dispatch and the F1 help.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tree::nav::NavKey;

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
    /// Esc: closes the search input, else clears the search, else the focus.
    Escape,
    Activate,
    Quit,
    // Search (SPEC §3.9).
    SearchOpen,
    SearchNext,
    SearchPrev,
    // Sidebar (SPEC §3.2, §3.7).
    ToggleSidebar,
    FocusSidebar,
    FocusDocument,
    /// A tree navigation key, while the sidebar has focus.
    Nav(NavKey),
    // Full-screen view (SPEC §3.6.3).
    ZoomIn,
    ZoomOut,
    ZoomReset,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    CloseFullscreen,
    // Overlays (SPEC §3.10).
    OpenToc,
    TocUp,
    TocDown,
    TocJump,
    CloseOverlay,
}

/// Where a binding applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// The document pane.
    Document,
    /// The full-screen diagram view.
    Fullscreen,
    /// The sidebar pane, while it has focus.
    Sidebar,
    /// The table-of-contents overlay, while it is open.
    Toc,
    /// Keys that work whichever pane has focus.
    Global,
}

impl Binding {
    /// Whether the key types a character: such bindings must not fire while a
    /// text input (the search input, KB-54) is open.
    pub fn is_letter_key(&self) -> bool {
        matches!(self.code, KeyCode::Char(_))
            && !self
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    }
}

/// One row of the key map.
#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub context: Context,
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
        context: Context::Document,
        code,
        modifiers,
        action,
        label,
        description,
    }
}

const fn global(
    code: KeyCode,
    modifiers: KeyModifiers,
    action: Action,
    label: &'static str,
    description: &'static str,
) -> Binding {
    Binding {
        context: Context::Global,
        code,
        modifiers,
        action,
        label,
        description,
    }
}

const fn side(
    code: KeyCode,
    key: NavKey,
    label: &'static str,
    description: &'static str,
) -> Binding {
    Binding {
        context: Context::Sidebar,
        code,
        modifiers: NONE,
        action: Action::Nav(key),
        label,
        description,
    }
}

const fn view(
    code: KeyCode,
    action: Action,
    label: &'static str,
    description: &'static str,
) -> Binding {
    Binding {
        context: Context::Fullscreen,
        code,
        modifiers: NONE,
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
        Action::Escape,
        "Esc",
        "Close the search input, clear the search, or clear the focus",
    ),
    bind(
        KeyCode::Enter,
        NONE,
        Action::Activate,
        "Enter",
        "Open the focused link or diagram",
    ),
    global(KeyCode::Char('q'), NONE, Action::Quit, "q", "Quit"),
    global(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
        Action::SearchOpen,
        "Ctrl+F",
        "Open the search input",
    ),
    global(
        KeyCode::Char('n'),
        NONE,
        Action::SearchNext,
        "n",
        "Next search match",
    ),
    global(
        KeyCode::Char('N'),
        KeyModifiers::SHIFT,
        Action::SearchPrev,
        "N",
        "Previous search match",
    ),
    global(
        KeyCode::Char('d'),
        NONE,
        Action::ToggleTextDiagrams,
        "d",
        "Toggle text diagrams",
    ),
    global(
        KeyCode::Char('s'),
        NONE,
        Action::ToggleSidebar,
        "s",
        "Show or hide the sidebar",
    ),
    global(
        KeyCode::Left,
        KeyModifiers::CONTROL,
        Action::FocusSidebar,
        "Ctrl+Left",
        "Focus the sidebar (while it is visible)",
    ),
    global(
        KeyCode::Right,
        KeyModifiers::CONTROL,
        Action::FocusDocument,
        "Ctrl+Right",
        "Focus the document",
    ),
    side(KeyCode::Up, NavKey::Up, "Up", "Select the previous entry"),
    side(KeyCode::Down, NavKey::Down, "Down", "Select the next entry"),
    side(
        KeyCode::Left,
        NavKey::Left,
        "Left",
        "Collapse the directory, or select its parent",
    ),
    side(
        KeyCode::Right,
        NavKey::Right,
        "Right",
        "Expand the directory",
    ),
    side(
        KeyCode::Enter,
        NavKey::Enter,
        "Enter",
        "Open the file, or expand the directory",
    ),
    side(
        KeyCode::Home,
        NavKey::Home,
        "Home",
        "Select the first entry",
    ),
    side(KeyCode::End, NavKey::End, "End", "Select the last entry"),
    view(KeyCode::Char('+'), Action::ZoomIn, "+", "Zoom in"),
    view(KeyCode::Char('='), Action::ZoomIn, "=", "Zoom in"),
    view(KeyCode::Char('-'), Action::ZoomOut, "-", "Zoom out"),
    view(KeyCode::Char('0'), Action::ZoomReset, "0", "Reset to fit"),
    view(KeyCode::Home, Action::ZoomReset, "Home", "Reset to fit"),
    view(KeyCode::Left, Action::PanLeft, "Left", "Pan left"),
    view(KeyCode::Right, Action::PanRight, "Right", "Pan right"),
    view(
        KeyCode::Up,
        Action::PanUp,
        "Up",
        "Pan up, or scroll the text",
    ),
    view(
        KeyCode::Down,
        Action::PanDown,
        "Down",
        "Pan down, or scroll the text",
    ),
    view(
        KeyCode::Esc,
        Action::CloseFullscreen,
        "Esc",
        "Back to the document",
    ),
    view(
        KeyCode::Char('q'),
        Action::CloseFullscreen,
        "q",
        "Back to the document",
    ),
];

/// Action bound to `key` in the document (or globally), if any.
pub fn action_for(key: KeyEvent) -> Option<Action> {
    action_in(Context::Document, key).or_else(|| action_in(Context::Global, key))
}

/// Action bound to `key` while the sidebar has focus (or globally), if any.
pub fn sidebar_action(key: KeyEvent) -> Option<Action> {
    action_in(Context::Sidebar, key).or_else(|| action_in(Context::Global, key))
}

/// Action bound to `key` while a text input is open: letter keys are skipped.
pub fn input_action(key: KeyEvent, sidebar_focused: bool) -> Option<Action> {
    let own = if sidebar_focused {
        Context::Sidebar
    } else {
        Context::Document
    };
    action_in_input(own, key).or_else(|| action_in_input(Context::Global, key))
}

/// Action bound to `key` in exactly `context`, if any.
pub fn action_in(context: Context, key: KeyEvent) -> Option<Action> {
    find(context, key).map(|b| b.action)
}

/// Like [`action_in`], but a letter key is ignored: used while a text input is open.
pub fn action_in_input(context: Context, key: KeyEvent) -> Option<Action> {
    find(context, key)
        .filter(|b| !b.is_letter_key())
        .map(|b| b.action)
}

fn find(context: Context, key: KeyEvent) -> Option<&'static Binding> {
    BINDINGS
        .iter()
        .find(|b| b.context == context && b.code == key.code && b.modifiers == key.modifiers)
}
