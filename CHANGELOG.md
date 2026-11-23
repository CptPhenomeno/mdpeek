# Changelog

All notable changes to mdpeek are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Dual license, MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).
- README install instructions: prebuilt binaries with mise or by hand, or
  from source with `cargo install --git`.

### Changed

- The tool is now called mdpeek (formerly readmd): the crate, the binary, the
  `make dist` and release asset names (`mdpeek-<version>-<os>-<arch>`), the
  documentation and the fixtures all use the new name.
- Table body rows are separated by horizontal rules, one per logical row.

### Fixed

- SVG text drawn blank on hosts without fontdb's default generic fonts: generic
  families with no installed face now fall back to the bundled Liberation Sans.

## [0.1.0] - 2026-11-21

First release: a terminal reader for Markdown files and Mermaid diagrams,
shipped as a single binary.

### Added

- GitHub-flavoured Markdown rendering: headings, lists, tables, task lists,
  quotes, links and syntax-highlighted code blocks.
- Local images and Mermaid diagrams (inside Markdown or as `.mmd` files)
  drawn as real images through the Kitty, Sixel or iTerm2 protocol, with a
  halfblocks fallback and a text mode for diagrams.
- A full-screen diagram view with zoom and pan.
- A sidebar with the Markdown files of a folder, and a table of contents
  overlay.
- Command line `readmd [OPTIONS] [PATH]` with `--protocol` and
  `--text-diagrams`.
- GitHub workflows: `make check` on every push to develop, and release
  binaries for linux and darwin (amd64, arm64) with checksums from `v*` tags.
