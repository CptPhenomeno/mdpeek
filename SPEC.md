# readmd — specification

`readmd` is a terminal (TUI) Markdown reader for Linux, written in Rust and shipped as a single binary. It renders GitHub-flavoured Markdown with syntax-highlighted code, local images and Mermaid diagrams drawn as real images, using the terminal's graphics protocol. It is for developers who read project documentation in the terminal and want diagrams to look the way they do on GitHub. Mermaid rendering is native (the `merman` crate), so there is no Node.js, no browser and no network access.

## 1. Goal

- Read Markdown documents comfortably in the terminal, with full GFM rendering.
- See Mermaid diagrams (inside Markdown or as standalone `.mmd` files) as crisp images, with a full-screen view to zoom and pan.
- Browse the Markdown files of a folder from a sidebar.
- Work on any terminal: real images where the terminal supports them, a graceful fallback where it does not.

### Out of scope

- Live reload on file change (a manual reload key exists, see §3.8).
- Export of diagrams or documents (PNG, SVG, PDF, HTML).
- Remote images (`http(s)://`): no network requests, ever.
- Following links between local files (`./other.md`) and navigation history / back.
- Light theme and theme selection: dark theme only.
- Vim-style key bindings.
- Editing files.
- Guaranteed support inside tmux/screen (best effort only).
- Windows and macOS as targets (they may work, they are not tested).
- Publishing to crates.io (the name `readmd` is taken there).

## 2. Glossary

| Term | Meaning |
| ---- | ------- |
| Document | A Markdown (`.md`, `.markdown`) or Mermaid (`.mmd`, `.mermaid`) file opened in the viewer. |
| Document model | The parsed, terminal-independent representation of a document: blocks (paragraph, heading, list, table, code, diagram, image, …) and inline spans. |
| Layout | The document model laid out for a given pane width: a list of rows, with the position of every heading, focusable item and image slot. |
| Diagram | A fenced code block with language `mermaid`, or the whole content of a `.mmd`/`.mermaid` file. |
| Image slot | A rectangle of rows reserved in the layout for a picture (diagram or local image), sized from its aspect ratio and the terminal cell size. |
| Graphics protocol | How pixels reach the terminal: Kitty, Sixel or iTerm2 inline images. |
| Halfblocks | Fallback that draws pictures with Unicode half-block characters and 24-bit colours when no graphics protocol is available. |
| Diagram text mode | A global toggle that shows diagrams as ASCII/Unicode art produced by `merman` instead of images. |
| Focusable | An item that Tab can select and Enter can activate: a link or a diagram. |
| Full-screen view | A mode that shows one diagram over the whole screen with zoom and pan. |
| Sidebar | The left pane with the folder tree; it can be shown or hidden. |
| TOC | The table of contents overlay listing the document's headings. |

## 3. Behaviour

### 3.1 Command line

```
readmd [OPTIONS] [PATH]
```

| Argument / option | Meaning |
| ----------------- | ------- |
| `PATH` | A file (`.md`, `.markdown`, `.mmd`, `.mermaid`) or a directory. Defaults to the current directory. |
| `--protocol <auto\|kitty\|sixel\|iterm2\|halfblocks>` | Force a graphics protocol. Default `auto`. |
| `--text-diagrams` | Start with diagram text mode on. |
| `-h, --help` / `-V, --version` | Standard. |

1. `PATH` is a file: open it. The sidebar shows the file's parent directory and starts hidden.
2. `PATH` is a directory: the sidebar shows it and starts visible. Open `README.md` (case-insensitive) if present, otherwise the first supported file in tree order, otherwise show an empty-state message in the document pane.
3. `PATH` does not exist, is unreadable, or is a file with an unsupported extension: print an error to stderr and exit with code 2, without entering the TUI.
4. Normal exit restores the terminal (alternate screen, raw mode, cursor) also on panic.

### 3.2 Screen layout

- **Sidebar** (left, fixed width ~30 columns, hidden/shown with F2).
- **Document pane** (rest of the width): the rendered document, scrollable vertically.
- **Status bar** (bottom row): file path relative to the root, scroll position (percentage), active mode (search, text diagrams), transient messages (e.g. "Opened in browser", "Local file links are not supported").
- **Overlays** drawn over the document pane: help (F1), TOC (F4), search input (in the status bar).
- **Full-screen view** replaces everything except the status bar.

The focused pane (sidebar or document) has a highlighted border. F6 moves focus between them when the sidebar is visible.

### 3.3 Markdown rendering

Parsing uses `pulldown-cmark` with tables, task lists, strikethrough and footnotes enabled. Text is wrapped to the pane width at word boundaries. Content inside the document pane has a 1-column left/right margin.

| Element | Rendering |
| ------- | --------- |
| Headings H1–H6 | Bold, distinct colour per level; H1 and H2 followed by a horizontal rule row. |
| Emphasis, strong, strikethrough, inline code | Italic, bold, crossed-out, distinct background colour. |
| Links | Underlined, link colour; the focused link is reverse-video. |
| Ordered / unordered lists, nested | Numbers / bullets, indentation 2 columns per level, wrapped lines aligned to the item text. |
| Task lists | `☐` / `☑` markers. |
| Block quotes, nested | A `│` bar per nesting level, dimmed text colour. |
| Code blocks | Box with background colour, language label at top right, syntax highlighted with `syntect` (dark theme); unknown language → plain. Long lines are not wrapped: they are truncated with `…`. |
| Tables | Box-drawing borders, header row bold, column alignment from the GFM delimiter row. If the table is wider than the pane, columns shrink proportionally and cell text wraps inside the cell. |
| Horizontal rule | A full-width `─` row. |
| Footnotes | Reference shown as `[n]`; definitions listed at the end of the document under a rule. |
| HTML blocks / inline HTML | Shown as dimmed raw text (not interpreted). |
| Images | See §3.5. |
| Mermaid fenced blocks | Info string `mermaid`, case-insensitive (` ```Mermaid ` counts). See §3.4. |

A `.mmd`/`.mermaid` file is a document made of a single diagram.

### 3.4 Mermaid diagrams

1. The diagram source is passed to `merman` to produce an SVG, using Mermaid's `dark` theme and a transparent background, with strict parsing (`ParseOptions::strict()`, so syntax errors are reported) and the resvg-safe SVG pipeline (`SvgPipeline::resvg_safe()`, so labels are `<text>` and not `<foreignObject>`, which resvg ignores). Before drawing, the transparent pixels are composited over the theme background (§4.2). See `docs/spikes/merman-resvg.md`. The SVG is rasterised with `resvg` at its natural (intrinsic) size, scaled down only when it is wider than the content area (pane width in columns × cell pixel width) or taller than the pane; it is never scaled up. A small diagram therefore stays small, like an image.
2. The picture is centred horizontally in the document pane and occupies an image slot (§3.5 for slot rules).
3. Rasterised pictures are cached in memory, keyed by (source hash, target pixel size), with least-recently-used eviction once the cache exceeds 256 MB of pixel data. Rendering happens off the UI thread; while a diagram renders, its slot shows a "Rendering diagram…" placeholder of the expected height (or 10 rows if unknown).
4. **Parse or render error**: the block shows the diagram source as a code block, framed in red, with the error message from `merman` above it. The rest of the document is unaffected.
5. **Diagram text mode** (F8 toggles, global): every diagram is shown as the ASCII/Unicode output of `merman` instead of an image. Only real drawings count as text output: flowchart, sequence, class and state. For every other type — including gantt and mindmap, for which `merman` returns only a raw outline dump, and pie, which it does not support — show the source as a code block with the note "Text rendering not available for this diagram type" (user decision after the KB-6 spike). The raw outline dump is never shown.
6. On terminal resize, the layout is recomputed and diagrams are re-rasterised for the new width (debounced, ~150 ms).

### 3.5 Images and graphics

1. **Protocol detection** at start-up (unless `--protocol` is given): use `ratatui-image`'s detection (`Picker::from_query_stdio()`), which knows each terminal's real support — on WezTerm it picks iTerm2, because WezTerm has no Kitty unicode placeholders; Kitty is used only where the terminal supports it (e.g. kitty itself); if nothing is detected, use halfblocks. Pictures are encoded once at their slot size and drawn as vertical slices while scrolling (see `docs/spikes/image-clipping.md`). The cell size in pixels is read from the terminal; if unavailable, assume 10×20 px.
2. **Local images** `![alt](path)`: `path` is resolved relative to the document's directory. Supported formats: PNG, JPEG, GIF (first frame only), SVG (rasterised with `resvg`), WebP. The picture is scaled down to fit the pane width (never scaled up beyond its natural size) and capped at the pane height, keeping the aspect ratio.
3. **Missing or unreadable image**: one dimmed row `[image not found: <path>]`, followed by the alt text if any.
4. **Remote image** (`http://`, `https://`): one dimmed row `[remote image: <url>]` plus alt text. No network request.
5. **Image slots** reserve whole rows: `rows = ceil(pixel_height / cell_pixel_height)`. Text never overlaps a slot.
6. **Partial visibility**: when a slot is partly scrolled out of view, only the visible part of the picture is drawn, clipped to the document pane. A picture never draws over the sidebar, status bar or overlays; while an overlay covers a picture, the covered part is hidden.

### 3.6 Focus, links and full-screen diagrams

1. Tab / Shift+Tab move the focus to the next / previous focusable in document order (links and diagrams), scrolling so that it is visible. Focus starts on nothing; Esc clears it.
2. Enter on a focused item:
   - `http://`, `https://`, `mailto:` link: open it with the system handler (`xdg-open` via the `open` crate); status message "Opened in browser" or the error.
   - `#anchor` link: scroll so the heading whose GitHub-style slug matches is at the top; if no heading matches, status message "Anchor not found".
   - Any other link (local file, other scheme): status message "Local file links are not supported".
   - Diagram: open the full-screen view.
3. **Full-screen view**: the diagram is rasterised to fit the screen (minus status bar), centred.

   | Key | Action |
   | --- | ------ |
   | `+` / `=` | Zoom in (×1.25, max ×8) |
   | `-` | Zoom out (÷1.25, min: fit to screen) |
   | `0` or Home | Reset to fit |
   | Arrow keys | Pan by 10% of the visible area (only when zoomed beyond the screen) |
   | Esc | Back to the document, same scroll position and focus |

   Zooming re-rasterises from the SVG at the new scale (no pixel upscaling). The status bar shows the zoom percentage. In diagram text mode, the full-screen view shows the text output, scrollable with arrow keys.

### 3.7 Sidebar

1. Shows the root directory as a tree: directories and supported files only (`.md`, `.markdown`, `.mmd`, `.mermaid`, case-insensitive). Hidden entries (dot-prefixed) and paths ignored by `.gitignore` are skipped. Directories that contain no supported file at any depth are omitted.
2. Sorting: directories first, then files, each alphabetical case-insensitive.
3. Keys when the sidebar has focus: ↑/↓ move the selection; Enter on a file opens it (focus stays on the sidebar); Enter or → on a directory expands it, ← collapses it (or moves to the parent); Home/End go to first/last entry.
4. The currently open file is marked. The tree is read at start-up; F5 also re-reads it (§3.8).
5. F2 toggles visibility; hiding the sidebar while it has focus moves focus to the document.

### 3.8 Keys (document focus)

| Key | Action |
| --- | ------ |
| ↑ / ↓ | Scroll one row |
| PgUp / PgDn, Space | Scroll one page (minus one row) |
| Home / End | Top / bottom of the document |
| Mouse wheel | Scroll three rows |
| Tab / Shift+Tab | Next / previous focusable (§3.6) |
| Enter | Activate focused item |
| Esc | Close overlay / clear search / clear focus, in that order |
| Ctrl+F | Open the search input (§3.9) |
| F3 / Shift+F3 | Next / previous search match |
| F1 | Help overlay listing all keys |
| F2 | Toggle sidebar |
| F4 | TOC overlay (§3.10) |
| F5 | Reload the current file and the sidebar tree from disk, keeping the scroll position when possible |
| F6 | Switch focus between sidebar and document |
| F8 | Toggle diagram text mode |
| q | Quit (from document or sidebar focus). While the search input is open `q` is typed as text. In the TOC overlay, the help overlay and the full-screen diagram view `q` acts like Esc and does not quit. Ctrl+Q and F10 are not bound (user decision, KB-67). |

### 3.9 Search

1. Ctrl+F opens a one-line input in the status bar. Search is incremental (updates while typing) and case-insensitive.
2. Matches are searched in the logical text of each block (a whole paragraph, list item, heading, quote line, table cell or code line), not row by row, so a phrase split by word wrapping is still found; the match is then mapped back to the screen rows it spans, possibly as several fragments. Pictures are excluded. All matches are highlighted; the current match has a distinct colour and is scrolled into view.
3. Enter closes the input keeping the highlights; F3 / Shift+F3 move to the next / previous match, wrapping around; the status bar shows `match i/n` or "No matches".
4. Esc closes the input and clears highlights.

### 3.10 Table of contents

F4 opens an overlay listing all headings, indented by level (2 columns per level below H1). ↑/↓ select, Enter jumps to the heading (placed at the top of the pane) and closes the overlay, Esc closes it. The heading nearest above the current scroll position is pre-selected.

### 3.11 Architecture (guidance)

Modules with pure logic first, so most behaviour is testable without a terminal:

| Module | Responsibility |
| ------ | -------------- |
| `cli` | Argument parsing (`clap`), path validation, exit codes. |
| `doc` | Markdown/Mermaid file → document model (`pulldown-cmark`), heading slugs, links. |
| `highlight` | Code highlighting with `syntect` → styled spans. |
| `layout` | Document model + width + cell size → rows, focusables, headings, image slots, search index. |
| `diagram` | `merman` wrapper: source → SVG or text, errors. |
| `raster` | SVG / bitmap → scaled RGBA image (`resvg`, `image`), cache. |
| `graphics` | Protocol detection, drawing and clipping pictures (`ratatui-image`). |
| `tree` | Folder scanning for the sidebar (`ignore` crate). |
| `ui` | App state, event loop (`crossterm`), panes, overlays, key map, full-screen view (`ratatui`). |

Rust edition 2024; minimum Rust version is the one required by `merman` (1.95).

## 4. Edge cases and decisions

1. **Native Mermaid only.** Diagrams are rendered by `merman`; no Node.js/Chromium fallback. Small visual differences from mermaid.js are accepted.
2. **Dark theme only, opaque.** One fixed dark theme; no detection. `readmd` paints an explicit RGB background on every cell of the screen (margins, borders, sidebar, status bar, overlays), so terminal transparency is never visible while reading. Headings, bold and italic are made distinct by colour and spacing as well as by font face, because many terminal fonts have no bold or italic (KB-68).
3. **Non-UTF-8 files** are decoded lossily; the status bar shows "File is not valid UTF-8".
4. **Empty file** shows "Empty document" in the pane.
5. **Very large documents** (≥ 1 MB): layout must stay responsive; diagrams and images are rasterised lazily, only when their slot is within one screen of the viewport.
6. **Pane narrower than 20 columns**: show "Window too narrow" instead of the document.
7. **Diagram failure never breaks the document** (§3.4.4); a panic inside `merman` is caught and treated as a render error.
8. **Local file links are not followed** (user decision: navigation is sidebar-only).
9. **Anchors** use GitHub's slug rules (lowercase, spaces → `-`, punctuation removed, `-1`, `-2` suffixes for duplicates).
10. **tmux**: graphics depend on `allow-passthrough`; when detection fails, halfblocks are used. No special handling beyond that.
11. **Name**: binary and package are `readmd`; the crates.io name is taken, so the crate is not published.
12. **Diagram size**: diagrams are never scaled above their natural size (§3.4.1), same rule as images.
13. **Picture cache limit**: LRU eviction above 256 MB of pixel data (§3.4.3).
14. **Search across wrapping**: matching runs on logical block text, so wrapped phrases match (§3.9.2).
15. **Mermaid info string** is matched case-insensitively (§3.3).
16. **Transient status messages** are cleared by the next key press.
17. **Tab focus wraps around** from the last focusable to the first (and back with Shift+Tab).
18. **Focused diagram** is shown with a highlighted frame around its slot.
19. **Missing/remote image placeholder** and its alt text share the same row.
20. **Full-screen zoom**: 100% means fit-to-screen; zooming keeps the viewport centre; only the visible region is rasterised, so ×8 does not allocate a whole giant bitmap.
21. **During the resize debounce** slots show the rendering placeholder, not the stale picture.
22. **Sidebar start state**: the root and the directories containing the open file are expanded; others are collapsed.
23. **F5 on a deleted file**: status bar error, the old content stays on screen.
24. **`.gitignore`** is honoured also when the folder is not inside a git repository.
25. **Large documents**: laying out a 1 MB document takes under 100 ms in a release build.
26. **merman unknowns** (text output per diagram type, dark theme selection, labels in `foreignObject` that resvg would drop) are verified by an early spike; any clash with §3.4 goes back to the user before dependent work starts.

## 5. Quality bar

- Common DoD for every task:
  - Tests written first (red → green → refactor) for all logic in `cli`, `doc`, `highlight`, `layout`, `diagram`, `raster`, `tree`, and for UI state transitions (key handling) in `ui`.
  - UI rendering covered by snapshot tests on `ratatui`'s `TestBackend` (`insta`), with pictures replaced by a placeholder renderer.
  - `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` all clean.
  - No `unwrap()`/`expect()` outside tests and start-up code, unless the reason is commented.
  - Code, comments, docs and commit messages in English. No AI attribution lines (`Co-Authored-By`, "Generated with …") in commits.
- Acceptance criteria of the whole work:
  - `SAMPLE.md` at the repository root (with `sample-assets/`) is the benchmark document: it shows every element `readmd` must render, and `readmd SAMPLE.md` renders each of its sections as the document describes.
  - A `fixtures/` folder contains a showcase document, built from `SAMPLE.md`, with every element of §3.3, at least five Mermaid diagram types (flowchart, sequence, class, gantt, mindmap), one diagram with a syntax error, local PNG/SVG images, a missing image and a remote image.
  - `readmd fixtures/` in WezTerm (iTerm2 protocol, as detected) shows diagrams and images as sharp pictures, and scrolling never leaves picture fragments on screen.
  - The same command with `--protocol halfblocks` shows all pictures with halfblocks; F8 shows diagrams as text.
  - Every key in §3.6–§3.10 works as specified and is listed in the F1 help.
  - Opening the showcase document takes under 300 ms to first paint (diagrams may still be rendering) on a modern laptop.
  - `readmd missing.md` exits with code 2 and a clear message.

## 6. Open questions

- (none)
