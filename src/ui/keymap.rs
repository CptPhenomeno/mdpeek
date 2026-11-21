//! Central key map: key -> action table shared by dispatch and the help.

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
    OpenHelp,
    TocUp,
    TocDown,
    TocJump,
    CloseOverlay,
    HelpUp,
    HelpDown,
    HelpPageUp,
    HelpPageDown,
    /// A row that only documents an input handled elsewhere (typing, mouse).
    Info,
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
    /// The help overlay, while it is open.
    Help,
    /// Keys that work whichever pane has focus.
    Global,
    /// The search input, while it is open (typed text is handled by the input).
    Search,
    /// Mouse input; rows are documentation only.
    Mouse,
}

impl Context {
    /// Contexts in help order, with their headings.
    pub const HELP_ORDER: [(Context, &'static str); 8] = [
        (Context::Global, "Global"),
        (Context::Document, "Document"),
        (Context::Sidebar, "Sidebar"),
        (Context::Search, "Search input"),
        (Context::Mouse, "Mouse"),
        (Context::Fullscreen, "Full-screen view"),
        (Context::Toc, "Contents (TOC)"),
        (Context::Help, "Help"),
    ];
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

/// A documentation-only row: it never matches a key event.
const fn info(context: Context, label: &'static str, description: &'static str) -> Binding {
    Binding {
        context,
        code: KeyCode::Null,
        modifiers: NONE,
        action: Action::Info,
        label,
        description,
    }
}

const fn help(
    code: KeyCode,
    action: Action,
    label: &'static str,
    description: &'static str,
) -> Binding {
    Binding {
        context: Context::Help,
        code,
        modifiers: NONE,
        action,
        label,
        description,
    }
}

const fn toc(
    code: KeyCode,
    action: Action,
    label: &'static str,
    description: &'static str,
) -> Binding {
    Binding {
        context: Context::Toc,
        code,
        modifiers: NONE,
        action,
        label,
        description,
    }
}

/// Document-focus bindings (SPEC §3.8); later keys and the help extend this table.
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
        "Close overlay, then clear search, then clear focus",
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
        KeyCode::Char('t'),
        NONE,
        Action::OpenToc,
        "t",
        "Table of contents",
    ),
    global(
        KeyCode::Char('?'),
        NONE,
        Action::OpenHelp,
        "?",
        "Help: list all keys",
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
    toc(
        KeyCode::Up,
        Action::TocUp,
        "Up",
        "Select the previous heading",
    ),
    toc(
        KeyCode::Down,
        Action::TocDown,
        "Down",
        "Select the next heading",
    ),
    toc(
        KeyCode::Enter,
        Action::TocJump,
        "Enter",
        "Jump to the heading",
    ),
    toc(
        KeyCode::Esc,
        Action::CloseOverlay,
        "Esc",
        "Close the overlay",
    ),
    toc(
        KeyCode::Char('q'),
        Action::CloseOverlay,
        "q",
        "Close the overlay",
    ),
    info(
        Context::Search,
        "Type",
        "Edit the query; matches update while typing",
    ),
    info(Context::Search, "Backspace", "Delete the last character"),
    info(
        Context::Search,
        "Enter",
        "Close the input, keep the highlights",
    ),
    info(
        Context::Search,
        "Esc",
        "Close the input and clear the highlights",
    ),
    info(Context::Mouse, "Mouse wheel", "Scroll three rows"),
    info(Context::Mouse, "Click", "Activate a link, focus a diagram"),
    info(Context::Mouse, "Double click", "Open a diagram full screen"),
    help(KeyCode::Up, Action::HelpUp, "Up", "Scroll the help up"),
    help(
        KeyCode::Down,
        Action::HelpDown,
        "Down",
        "Scroll the help down",
    ),
    help(
        KeyCode::PageUp,
        Action::HelpPageUp,
        "PgUp",
        "Scroll the help one page up",
    ),
    help(
        KeyCode::PageDown,
        Action::HelpPageDown,
        "PgDn",
        "Scroll the help one page down",
    ),
    help(KeyCode::Esc, Action::CloseOverlay, "Esc", "Close the help"),
    help(
        KeyCode::Char('q'),
        Action::CloseOverlay,
        "q",
        "Close the help",
    ),
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

/// Action bound to `key` while the TOC overlay is open: nothing else applies.
pub fn toc_action(key: KeyEvent) -> Option<Action> {
    action_in(Context::Toc, key)
}

/// Action bound to `key` while the help overlay is open.
pub fn help_action(key: KeyEvent) -> Option<Action> {
    action_in(Context::Help, key)
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
    // Shift is needed to type `?`, `+` and the like, and terminals differ on
    // whether they report it: for a non-letter character it carries no meaning.
    let modifiers = match key.code {
        KeyCode::Char(c) if !c.is_alphabetic() => key.modifiers - KeyModifiers::SHIFT,
        _ => key.modifiers,
    };
    BINDINGS
        .iter()
        .find(|b| b.context == context && b.code == key.code && b.modifiers == modifiers)
}
