# mdpeek

A terminal reader for Markdown files and Mermaid diagrams, written in Rust and
shipped as a single binary. It renders GitHub-flavoured Markdown with
syntax-highlighted code, and draws local images and Mermaid diagrams as real
pictures through the terminal's graphics protocol. Diagrams are rendered
natively (`merman` and `resvg`): no Node.js, no Chromium, no network.

## Install

Every release on GitHub ships prebuilt binaries for Linux (amd64 and arm64),
named `mdpeek-<version>-linux-<arch>`, with a `checksums.txt`. On other
systems, build from source.

### With mise

```sh
mise use -g github:CptPhenomeno/mdpeek
```

mise downloads the binary that matches your platform from the latest GitHub
release. Pin a version with `github:CptPhenomeno/mdpeek@0.1.0`.

### By hand

Download the binary for your platform from the
[releases page](https://github.com/CptPhenomeno/mdpeek/releases), check it
against `checksums.txt`, make it executable and put it on your `PATH`.

### From source

`mdpeek` is not published on crates.io. Building it needs Rust 1.95 or newer
(edition 2024).

```sh
cargo install --locked --git https://github.com/CptPhenomeno/mdpeek
```

Or from a clone:

```sh
git clone https://github.com/CptPhenomeno/mdpeek && cd mdpeek
cargo install --locked --path .   # installs `mdpeek` into ~/.cargo/bin
```

## Build

Development targets:

```sh
make check   # cargo fmt --check, clippy -D warnings, cargo test
make dist    # release binary in dist/mdpeek-<version>-<os>-<arch>
```

## Usage

```
mdpeek [OPTIONS] [PATH]
```

| Argument / option | Meaning |
| ----------------- | ------- |
| `PATH` | A file (`.md`, `.markdown`, `.mmd`, `.mermaid`) or a directory. Defaults to the current directory. |
| `--protocol <auto\|kitty\|sixel\|iterm2\|halfblocks>` | Force a graphics protocol. Default `auto`. |
| `--text-diagrams` | Start with diagram text mode on. |
| `-h, --help` / `-V, --version` | Help and version. |

- A file opens directly, with the sidebar hidden. A directory opens with the
  sidebar visible, showing `README.md` if present, else the first document.
- Exit code 0 on a normal exit. Exit code 2 (with a message on stderr) when
  `PATH` does not exist, is unreadable, or has an unsupported extension.

## Keys

Press `?` in the app for the same list. Typed letters (`q`, `n`, `s`, ...) are
text while the search input is open. No action uses function keys.

### Global

| Key | Action |
| --- | ------ |
| `q` | Quit |
| `Ctrl+F` | Open the search input |
| `n` | Next search match |
| `N` | Previous search match |
| `d` | Toggle text diagrams |
| `t` | Table of contents |
| `?` | Help: list all keys |
| `r` | Reload the file and the sidebar tree |
| `s` | Show or hide the sidebar |
| `Ctrl+Left` | Focus the sidebar (while it is visible) |
| `Ctrl+Right` | Focus the document |

### Document

| Key | Action |
| --- | ------ |
| `Up` | Scroll one row up |
| `Down` | Scroll one row down |
| `PgUp` | Scroll one page up |
| `PgDn` | Scroll one page down |
| `Space` | Scroll one page down |
| `Home` | Top of the document |
| `End` | Bottom of the document |
| `Tab` | Focus the next link or diagram |
| `Shift+Tab` | Focus the previous link or diagram |
| `Esc` | Close overlay, then clear search, then clear focus |
| `Enter` | Open the focused link or diagram |

### Sidebar

| Key | Action |
| --- | ------ |
| `Up` | Select the previous entry |
| `Down` | Select the next entry |
| `Left` | Collapse the directory, or select its parent |
| `Right` | Expand the directory |
| `Enter` | Open the file, or expand the directory |
| `Home` | Select the first entry |
| `End` | Select the last entry |

### Search input

| Key | Action |
| --- | ------ |
| `Type` | Edit the query; matches update while typing |
| `Backspace` | Delete the last character |
| `Enter` | Close the input, keep the highlights |
| `Esc` | Close the input and clear the highlights |

### Mouse

| Key | Action |
| --- | ------ |
| `Mouse wheel` | Scroll three rows |
| `Click` | Activate a link, focus a diagram |
| `Double click` | Open a diagram full screen |

### Full-screen view

| Key | Action |
| --- | ------ |
| `+` | Zoom in |
| `=` | Zoom in |
| `-` | Zoom out |
| `0` | Reset to fit |
| `Home` | Reset to fit |
| `Left` | Pan left |
| `Right` | Pan right |
| `Up` | Pan up, or scroll the text |
| `Down` | Pan down, or scroll the text |
| `Esc` | Back to the document |
| `q` | Back to the document |

### Contents (TOC)

| Key | Action |
| --- | ------ |
| `Up` | Select the previous heading |
| `Down` | Select the next heading |
| `Enter` | Jump to the heading |
| `Esc` | Close the overlay |
| `q` | Close the overlay |

### Help

| Key | Action |
| --- | ------ |
| `Up` | Scroll the help up |
| `Down` | Scroll the help down |
| `PgUp` | Scroll the help one page up |
| `PgDn` | Scroll the help one page down |
| `Esc` | Close the help |
| `q` | Close the help |

## Terminals and graphics protocols

With `--protocol auto` (the default) mdpeek detects what the terminal supports
at start-up:

- WezTerm: iTerm2 protocol (WezTerm has no Kitty unicode placeholders).
- Kitty protocol: used where the terminal really supports it, such as kitty.
- Sixel: used by terminals that support it.
- Halfblocks: the fallback when nothing is detected, works anywhere.

Force one with `--protocol`. Inside tmux, graphics depend on
`allow-passthrough`; when detection fails mdpeek falls back to halfblocks.
Beyond that tmux and screen are best effort.

Linux is the target; macOS and Windows may work but are not tested.

### Opaque theme

mdpeek has one dark theme and paints an explicit background on every cell, so
terminal transparency is never visible while reading. In WezTerm keep
`text_background_opacity` at `1.0` (the default); a lower value lets the
terminal blend the text background and spoils the picture edges.

### Recommended font

Use Monaspace, Neon or Xenon, static fonts from
<https://github.com/githubnext/monaspace/releases>. It ships true italic and
bold faces, so headings, bold and italic read well. A Nerd Font symbols
fallback covers icons. In WezTerm:

```lua
config.font = wezterm.font_with_fallback({
  "Monaspace Neon",
  { family = "Symbols Nerd Font Mono", scale = 0.85 },
})
```

The font is chosen by your terminal, not by mdpeek: mdpeek only prints text and
cannot change it.

## Try it

- `mdpeek SAMPLE.md`: a benchmark document with every rendered element.
- `mdpeek fixtures/`: a showcase folder with nested documents, five diagram
  types, a broken diagram, and local, missing and remote images. See
  [fixtures/README.md](fixtures/README.md).

## Out of scope

- Live reload (use `r` to reload by hand).
- Export of diagrams or documents (PNG, SVG, PDF, HTML).
- Remote images: no network requests, ever.
- Following links between local files, and navigation history.
- Light theme and theme selection.
- Vim-style key bindings.
- Editing files.
- Guaranteed support inside tmux or screen.
- Windows and macOS as tested targets.
- Publishing to crates.io.

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

The bundled Liberation Sans fonts (`assets/fonts/`) are under the SIL Open
Font License; see [assets/fonts/LICENSE.txt](assets/fonts/LICENSE.txt).
