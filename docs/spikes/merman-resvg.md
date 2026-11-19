# Spike: merman 0.10.0 and resvg 0.50.0

Learning tests: `tests/spike_merman.rs` (run with `cargo test --test spike_merman -- --nocapture --test-threads=1` to see the observations quoted below). Task KB-6, SPEC §3.4, §4.7, §4.26.

## Summary

| Question | Answer |
| --- | --- |
| SVG for flowchart, sequence, class, gantt, mindmap, state, pie | Yes, all seven |
| Dark theme | Yes, via the site config (`theme: dark`) or an `%%{init}%%` directive |
| Transparent background | Yes by default: the SVG has no background rect, the rasterised corner pixel has alpha 0 |
| Labels survive resvg | Yes, but only with `SvgPipeline::resvg_safe()`; the default pipeline emits 14 `<foreignObject>` labels and no `<text>` for the sample flowchart |
| Text output | flowchart, sequence, class, state: real diagrams. gantt, mindmap: only a textual outline dump. pie: unsupported (error) |
| Error type | `RenderError::Parse(TerminalDiagnostic)`, message plus source span |
| Panics | None observed on hostile inputs; containment pattern verified with an injected panic only |
| 1000 px raster time | about 9 ms warm, 20 ms cold including system font loading (release build) |

## Cargo features

```toml
merman = { version = "0.10.0", default-features = false, features = [
    "svg", "layout-cytoscape", "ascii",
    "diagram-flowchart", "diagram-sequence", "diagram-class",
    "diagram-gantt", "diagram-mindmap", "diagram-state", "diagram-pie",
] }
```

- The defaults enable all 34 diagram families plus `layout-elk` (EPL-2.0 ELK). Not needed for these seven types, so defaults are off.
- `svg` is the renderer, `layout-cytoscape` is required by mindmap, `ascii` enables `RenderRequest::ascii`.
- `serde_json` is a dev-dependency only, to build `SourcePresentationConfig` in the tests (production code will need it too if it uses the site config; the `%%{init}%%` alternative avoids it).
- Other families (er, journey, timeline, ...) fall outside the card; adding one is a one-line feature.

## API snippets

SVG, dark theme, resvg-safe labels:

```rust
let site = SourcePresentationConfig::from_value(serde_json::json!({ "theme": "dark" }));
let renderer = Renderer::new()
    .with_parse_options(ParseOptions::strict())
    .with_presentation_engine(MermaidEngine::new().with_site_config(site));
let request = SvgRequest { pipeline: Some(SvgPipeline::resvg_safe()), ..Default::default() };
let out = renderer.render(RenderRequest::svg(src, OperationControl::new(), request))?;
let RenderOutput::Svg(Some(svg)) = out else { /* no diagram detected */ };
let svg: &str = svg.svg();
```

Equivalent in-source form: prefix the source with `%%{init: {"theme":"dark"}}%%`. The dark theme changes the palette from default colours (`#22D3EE`, `#2DD4BF`, ...) to Mermaid dark (`#1f2020`, `#cccccc`, `#a44141`, ...).

Transparent background: nothing to configure. The SVG root is `<svg id="merman" width="100%" style="max-width:370px" viewBox="0 0 370 776" ...>` with no background rect; resvg leaves the pixmap transparent (top-left pixel `[0,0,0,0]`). A different background would need `RasterOptions::with_matte` (merman's own PNG path) or a rect.

Text mode:

```rust
let out = renderer.render(RenderRequest::ascii(src, OperationControl::new(), AsciiRequest {
    options: AsciiRenderOptions::unicode(),               // or ::ascii()
    viewport: AsciiViewportPolicy::with_max_width(100).overflow(OverflowPolicy::Fallback),
    ..Default::default()
}))?;
let RenderOutput::Ascii(Some(report)) = out else { /* none */ };
report.text
```

`merman::ascii::ascii_supported_diagram_types()` returns `class, flowchart, gantt, mindmap, sequence, state`; `ascii_diagrammatic_diagram_types()` returns `class, flowchart, sequence, state`. Gantt and mindmap are "supported" but print a semantic dump (`title(bytes=4)="Plan"`, `├── node(bytes=7)="Reading" ...`), not a drawing, and expose internals such as `bytes=`. Pie returns `ASCII rendering does not support diagram type 'pie'`. Flowchart text shows a duplicated arrow head at one merge and the state diagram routes are cramped but readable.

resvg with system fonts loaded once:

```rust
let mut db = resvg::usvg::fontdb::Database::new();
db.load_system_fonts();                       // once, keep in an Arc / OnceLock
let opt = resvg::usvg::Options { fontdb: Arc::new(db), ..Default::default() };
let tree = resvg::usvg::Tree::from_str(&svg, &opt)?;
let size = tree.size();                       // intrinsic size from the viewBox
let scale = width as f32 / size.width();
let mut pm = resvg::tiny_skia::Pixmap::new(width, (size.height() * scale).ceil() as u32).unwrap();
resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
let rgba = pm.take();                          // premultiplied RGBA8
```

`width="100%"` in the root does not hurt: usvg takes the intrinsic size from the viewBox, so the aspect ratio is always available.

## Labels and foreignObject

Default merman output uses HTML labels in `<foreignObject>` (14 of them, 0 `<text>` for the sample flowchart), which resvg ignores. `SvgPipeline::resvg_safe()` (set through `SvgRequest.pipeline`) rewrites them to SVG `<text>`: all seven types then have `<foreignObject>` = 0 and a positive pixel difference between the rasterisation with and without the text elements, so labels are drawn. No workaround beyond that option is needed.

## Per-diagram results (dark theme, resvg_safe)

| Type | SVG | Text | Labels after resvg | Intrinsic size (px) |
| --- | --- | --- | --- | --- |
| flowchart | ok | ok (drawing) | ok | 370 x 776 |
| sequence | ok | ok (drawing) | ok | 694 x 422 |
| class | ok | ok (drawing) | ok | 264 x 512 |
| gantt | ok | outline dump only | ok | 800 x 196 |
| mindmap | ok | outline dump only | ok | 578.3 x 416.2 |
| stateDiagram-v2 | ok | ok (drawing) | ok | 405.3 x 234 |
| pie | ok | unsupported (error) | ok | 576.8 x 450 |

## Errors and panics

Strict parsing (`ParseOptions::strict()`) is required to get errors. The broken diagram of SAMPLE.md returns:

```text
Err(RenderError::Parse(TerminalDiagnostic {
    kind: "diagram_parse",
    message: "Diagram parse error (flowchart-v2): Unterminated node label (missing `]`)",
    details: { code: "merman.parse.diagram_parse", span: Some(18..31), diagram_type: Some("flowchart-v2"), .. } }))
```

`err.to_string()` is the message, usable as shown; the byte span allows highlighting. SVG and text mode return the same error. Other messages seen: `No Mermaid diagram type detected` (empty source, `RenderError::NoDiagram` / parse), `unexpected end of input; expected node identifier, edge label`, `"a" has invalid value: -1. Negative values are not allowed in pie charts`. Valid-but-empty input (`flowchart`, `mindmap`) renders an empty diagram without error.

Panics: 11 hostile inputs (empty, truncated, NUL and RTL characters, negative pie value, bad gantt date) produced errors or SVG, never a panic. Containment is verified with an injected panic wrapped in `catch_unwind(AssertUnwindSafe(..))`, so the real merman panic path is not exercised. `catch_unwind` only works with `panic = "unwind"` (the default); the release profile must not set `panic = "abort"`.

## Timings

Release build (`cargo test --release`), machine of the spike, resvg rasterisation to 1000 px width:

| Diagram | Time |
| --- | --- |
| flowchart (first call, includes `load_system_fonts`) | 20.6 ms |
| flowchart (warm) | 9.2 ms |
| sequence | 12.7 ms |
| class | 10.8 ms |
| gantt | 3.5 ms |
| mindmap | 5.5 ms |
| state | 4.5 ms |
| pie | 5.6 ms |

Debug build is about 10x slower (flowchart 125 ms). merman's own render time was not measured separately. The 1000 px width upscales a 370 px-wide flowchart to 1000x2098; SPEC §3.4 says natural size, scaled down only, so KB-32 should rasterise at `min(natural, available width)`.

## Contradictions and gaps against SPEC §3.4

1. Text mode is not available for every type. pie has none; gantt and mindmap return an outline dump, not a drawing (§3.4 "text mode via merman's ASCII/Unicode output"). Decision needed: fall back to showing the source for those, or accept the dump.
2. The default pipeline is not resvg-compatible: `SvgPipeline::resvg_safe()` is mandatory (not stated in the spec).
3. Errors only occur with `ParseOptions::strict()`; the default parse mode was not probed for lenient behaviour.
4. Dark theme is a Mermaid theme, not a transparent-aware one: the background is transparent but node fills are dark, so the diagram assumes a dark terminal background.
5. A real merman panic could not be provoked; §4.7 containment rests on `catch_unwind` and unwinding panics.
