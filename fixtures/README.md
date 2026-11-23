# mdpeek fixtures

A folder to try `mdpeek` on: `mdpeek fixtures/`.

- `showcase.md`: every rendered element, five diagram types, a broken diagram,
  local, missing and remote images. Pick it in the sidebar.
- `other.md`: the target of the local file link in the showcase.
- `diagram.mmd`: a document made of a single diagram.
- `docs/`: nested folders with documents.
- `empty.md`: an empty file ("Empty document").
- `latin1.md`: a file that is not valid UTF-8 (a warning is shown).
- `.hidden/` and `ignored.md` exist on disk but the sidebar skips them
  (a dot-prefixed folder, and a path listed in `.gitignore`).
