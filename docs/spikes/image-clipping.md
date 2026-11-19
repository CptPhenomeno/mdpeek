# Spike: clipping partially visible pictures (KB-7)

Question: can a picture slot that is partly scrolled out of the document pane
show only its visible part, never cover the sidebar, status bar or an overlay,
and leave no fragment behind when scrolling (SPEC §3.5.6, §5)?

Answer: yes, with `ratatui-image` 11.1.0 `SlicedProtocol` / `SlicedImage`,
which already supports vertical sub-ranges for every protocol. We do not need
to crop and re-encode the source image per scroll step.

Evidence: `tests/clip_protocols.rs` (buffer and escape-sequence checks per
protocol), `src/graphics.rs` (geometry, renderer trait, placeholder),
`examples/clip_spike.rs` (interactive scene and `--bench`).

## Chosen strategy

1. Encode the picture once at natural size (`SlicedProtocol::new`), after
   flattening it over the theme background (see Transparency).
2. Every frame, compute the visible part of the slot against the document pane
   (`visible_part`): rows skipped at the top, rows shown, first screen row.
3. Split the visible part into row bands that no overlay crosses
   (`clear_row_bands`) and render each band with `SlicedImage` into an area
   equal to that band, so the library clips to it.
4. Nothing is ever written outside the pane: the pane rectangle is the clip,
   so the sidebar and status bar are untouched by construction.

The renderer seam is `PictureRenderer` (`PictureRequest` in, `Buffer` out).
`SlicedRenderer` is the real implementation, `PlaceholderRenderer` fills the
visible cells with a symbol for snapshot tests on `TestBackend`.

## Per protocol

Protocol selection is `Picker::set_protocol_type`; `Picker::halfblocks()` gives
a 10x20 px cell without a terminal, which the tests use.

| Protocol | Clipping mechanism | Cleanup when it moves | Accuracy |
| -------- | ------------------ | --------------------- | -------- |
| Kitty | Unicode placeholders (U+10EEEE + row/column diacritics) drawn as ordinary cells; the image is transmitted once (APC on the first drawn cell, chunked) and a clipped picture just names image rows `skip..`. Placeholder cells carry the image id in the foreground colour. | Placeholders are normal cells: the ratatui diff overwrites them, no Kitty delete command needed. | Exact, per cell row. |
| iTerm2 | The image is pre-sliced into one PNG per cell row (`Sliced`); only the visible rows are written, each as an OSC 1337 image preceded by an erase-characters (`ESC [ n X`) of its own cells. | The vacated cells are redrawn by the diff (tested), the erase clears the cells being replaced. | Exact, per cell row. |
| Sixel | Sixel bands (6 px) are dropped at render time; the visible range is `ceil(skip*cell_h/6)` bands to `floor(rows*cell_h/6)`. The visible area is erased first. | Same as iTerm2. | Not pixel exact: at 20 px cell height a clip can lose up to 5 px at the cut edge, and the bottom up to 5 px. Always inside the visible rows, never outside. |
| Halfblocks | Whole image rendered to cells, only the wanted rows copied (`render_with_skip`). | Ordinary cells. | Exact per cell row; colours are resampled to half cells. |

Facts verified by tests, for fully visible, top-clipped and bottom-clipped
pictures: the touched cells are exactly the slot columns by the visible rows in
all four protocols, a slot entirely outside the pane writes nothing, a top
clipped iTerm2 row carries the PNG of the right stripe (decoded in the test),
a top-clipped Kitty cell names the right image row, halfblocks colours follow
the right stripes.

## Scroll cleanup (SPEC §5 "no fragments")

For all four protocols, `Buffer::diff` between a frame and the next one lists
every cell the picture vacated, so the terminal gets a blank written over each
(tested for Kitty, Sixel, iTerm2, halfblocks). Interior cells of Sixel/iTerm2
images are marked `CellDiffOption::Skip` while drawn; once the picture leaves
they are normal blank cells again and are emitted.

Not verifiable without a screen: how WezTerm erases overlapping Sixel/iTerm2
pixels when the blank is written. The write of a blank over an image cell is
what the protocols rely on; this must be eyeballed in WezTerm
(`cargo run --example clip_spike -- --protocol iterm2`, scroll with the arrows,
toggle the overlay with `o`). A pty run of the example for all four protocols
completes and emits one Kitty transmission and the expected number of Sixel and
iTerm2 images, with no panic.

## Overlays

Kitty and halfblocks could be cut per cell (`exclude_overlays` returns the
disjoint rectangles), but Sixel and iTerm2 images cannot skip columns inside a
row. The chosen rule is uniform: **a picture row that an overlay crosses is
hidden whole** (`clear_row_bands`), the rest of the picture stays. The overlay
is then drawn on top of blank cells. Consequence for SPEC §3.5.6 "the covered
part is hidden": slightly more than the covered part is hidden (the full width
of the rows the overlay touches). Flagged, not a spec change; KB-39 can refine
Kitty/halfblocks to per-cell if wanted.

## Transparency (SPEC §4.2)

`ratatui-image` leaves transparent pixels transparent unless it resizes, and
`Picker::set_background_color` only applies on the resize path. Because the
slot is natural size, we flatten the RGBA image over the theme background
before encoding (`flatten_over`, alpha forced to 255 because the blend left 254
in a test). Result: every pixel is opaque and equal to the theme background
where the source was transparent, for all protocols (tested on halfblocks cells
and on the flattened image). Kitty placeholder cells also keep the explicit
cell background set by the theme.

## Costs

`cargo run --release --example clip_spike -- --bench`, 640x1600 px picture
(64x80 cells), 38-row pane, one run on the development machine:

| Protocol | Encode once | Render per scroll step | Re-encode a cropped copy per step (rejected) |
| -------- | ----------- | ---------------------- | -------------------------------------------- |
| Kitty | 9 ms | 69 us | 4.0 ms |
| Sixel | 34 ms | 14 us | 16.5 ms |
| iTerm2 | 4.7 ms | 12 us | 4.5 ms |
| Halfblocks | 5 ms | 30 us | 4.6 ms |

Bytes: Kitty sends the whole image once (5.4 MB uncompressed here because the
test picker has no compression capability; a detected terminal with
`KittyCompression` is much smaller), then only placeholder cells (a few KB per
step). Sixel about 13 KB and iTerm2 about 30 KB per frame for this scene, both
re-sent because every cell moves when scrolling. The crop-and-encode
alternative is 60x to 300x more expensive per step and was not needed.

## Limits and notes for KB-38 / KB-39

- Kitty: placeholders support at most the diacritic table (297 rows / columns);
  larger positions are silently left undrawn. Slots are capped at the pane
  height by the spec, so rows are not a problem; very wide panes are.
- A Kitty image is transmitted when its first cell is first rendered. If the
  application ever clears the terminal, keep the `SlicedRenderer` (do not
  rebuild it) or re-create it to retransmit.
- `SlicedImage` returns early when the slot is wider than the area; slots fit
  the pane width by spec (§3.5.2).
- tmux (§4.10, best effort): the library wraps sequences for tmux when it
  detects it; Kitty placeholders are the tmux-friendly mode. Not exercised.
- Sixel is the only non pixel exact protocol; accepted, see table.
