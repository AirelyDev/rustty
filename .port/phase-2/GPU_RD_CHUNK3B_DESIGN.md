# GPU R-d — Chunk 3b DESIGN: the CPU cell-fill

Status: ARCHITECT design — **adversary-validated, pass 2 folded** (verdict
SOUND-WITH-FIXES). All six load-bearing claims §A–§F re-verified against the
reference + the ported crates; the pass-2 adversary's two blocking soundness issues
(§C.3 inline-alias Stacked-Borrows UB; §D.6 `add_cursor` safe-ref → `*const`) and
its six smaller fixes are folded in-line below — this doc is the SINGLE authoritative
source; there is no separate fix list to reconcile. See the changelog at the end of
§H (H.1 pass 1, H.2 pass 2). Design-only; touches no `crates/`. Implementation lands
in a follow-up under this spec.

REPO = `/Users/gael/projects/rustty`. REFERENCE (read-only) =
`/Users/gael/projects/ghostty` (Zig 0.15.2, `src/renderer/generic.zig`).

Predecessor: `GPU_RD_CHUNK3A_DESIGN.md` (the monomorphic `Renderer` struct +
`SwapChain`/`FrameState`/`CustomShaderState` skeleton + `DerivedConfig` struct
with `init<C>`/`deinit` seams + the inhabited `TerminalColor`/`Path`/
`RepeatablePath` placeholders), landed in `crates/rustty-renderer/src/generic.rs`.

> CRITICAL GATE-COVERAGE NOTE (from the task): G3b comment-coverage SKIPS
> `rustty-renderer` (its `lib.rs` says "PORT STATUS: in progress"), so the R1
> comment fidelity of `generic.rs` is **not gate-enforced**. The adversary/verify
> pass is the ONLY R1 backstop for this chunk. Every ported line + comment must be
> checked against the reference by hand. This design calls out each hand-check
> point inline.

---

## 0. TL;DR — the decisions

1. **Scope (3b = the pure cell-fill core).** 3b ports, as `Renderer` methods, the
   subtree that fills the already-ported `Contents` (cell.rs) from
   `self.terminal_state` (`rustty_vt::render::RenderState`):
   `rebuild_cells` → `rebuild_row` → `add_underline`/`add_overline`/
   `add_strikethrough`/`add_glyph`/`add_cursor`/`add_preedit_cell`, plus
   `rebuild_overlay` and the private `PreeditRange` helper. Two small
   prerequisites ride along: a new `row.rs` (`never_extend_bg`, ported from
   `renderer/row.zig`) and the promotion of the `TerminalColor` placeholder to a
   real renderer-local enum.
2. **`updateFrame` moves to 3c, NOT 3b.** It is the frame orchestrator: it calls
   the 3c uniforms method `updateCustomShaderUniformsFromState`, needs a live
   `State { mutex, *mut Terminal }`, and drives the images **kitty** slow-path.
   It is not honestly unit-testable in 3b (would panic at the 3c seam) and its
   natural companion is the uniforms cluster. Landing it in 3c makes it
   end-to-end testable there. §A justifies this against the gate. Its RenderState
   walk is still fully mapped here (item 3) so 3c inherits a ready spec.
3. **`DerivedConfig::init<C>` stays a deferred seam; the golden constructs
   `DerivedConfig` directly.** But `TerminalColor` becomes a REAL renderer-local
   enum NOW (mirroring config's `union(enum) { color, cell-foreground,
   cell-background }`), because the fill destructures it and the Chunk-4 golden
   must construct real cursor/selection/search colors. `Path`/`RepeatablePath`
   stay placeholders (the fill never reads them; the golden uses no
   bg_image/custom_shaders). §E.
4. **Compile/gate boundary.** 3b is green when the workspace compiles under
   `-D warnings` AND a `#[cfg(test)]` test builds an offscreen `Renderer`
   (Metal system-default device + embedded-font `SharedGrid` + `Shaper`), drives
   a tiny `Terminal` through `RenderState::update`, calls `rebuild_cells`, and
   asserts `bg_cells` + a foreground glyph + a cursor entry — **no `drawFrame`,
   no GPU output**. §G. The test skips cleanly on hosts with no Metal device
   (the `metal.rs` precedent).
5. **Aliasing.** The fill is written in the reference around a `state =
   &self.terminal_state` alias held live across `&mut self` mutations of *other*
   `Renderer` fields, plus a shaper cell buffer held across `&mut self` calls.
   Faithful + sound Rust needs raw pointers, but the soundness argument turns on
   ALLOCATION, not field-disjointness (adversary pass 2): a raw alias into a
   **heap** allocation (the `row_data`/shaper/cache buffers) survives a nested
   `&mut *self` reborrow, but a raw alias into **inline `Renderer` memory**
   (`terminal_state.{colors,cursor,…}` read through `state`; the shaper hook's
   `*mut Shaper` → `self.font_shaper`) is popped by that reborrow → use-after-pop
   UB. The remedy is to Copy-hoist inline scalars before any `&mut self` sub-call
   AND/OR drive the whole fill off a raw `*mut Renderer` (§C.3), per the memory
   `rust-mut-args-strongly-protected`. This is the single largest unsafe surface
   in the chunk and the primary adversary target; Miri cannot run the Metal-gated
   test, so the invariants are adversary-only (`MIRI_EXCEPTIONS.md`).

---

## A. SCOPE BOUNDARY (item 1)

### A.1 The method table

Reference line ranges are `src/renderer/generic.zig`.

| Reference method | ref lines | Chunk | Rust name / disposition |
|---|---|---|---|
| `rebuildCells` | 2307–2608 | **3b** | `Renderer::rebuild_cells` (pub) |
| `rebuildRow` | 2610–3056 | **3b** | `Renderer::rebuild_row` (priv, reachable via `rebuild_cells`) |
| `addUnderline` | 3059–3097 | **3b** | `Renderer::add_underline` |
| `addOverline` | 3099–3128 | **3b** | `Renderer::add_overline` |
| `addStrikethrough` | 3131–3159 | **3b** | `Renderer::add_strikethrough` |
| `addGlyph` | 3161–3222 | **3b** | `Renderer::add_glyph` |
| `addCursor` | 3224–3311 | **3b** | `Renderer::add_cursor` |
| `addPreeditCell` | 3313–3353 | **3b** | `Renderer::add_preedit_cell` |
| `rebuildOverlay` | 2226–2292 | **3b** | `Renderer::rebuild_overlay` (pub) |
| `PreeditRange` (struct) | 2294–2298 | **3b** | private `PreeditRange` |
| `renderer/row.zig::neverExtendBg` | row.zig 8–63 | **3b** (prereq) | new `crate::row::never_extend_bg` |
| `TerminalColor`/`Color` promotion | — | **3b** (prereq) | real enum in `generic.rs` (§E) |
| `updateFrame` | 1123–1424 | **3c** | seam now; ported in 3c with uniforms |
| `updateFontGridUniforms` | 1115–1120 | **3c** | uniforms math |
| `updateScreenSizeUniforms` | 1938–1977 | **3c** | uniforms math |
| `updateCustomShaderUniformsFromState` | 2010–2100 | **3c** | uniforms math (updateFrame calls this) |
| `updateCustomShaderUniformsForFrame` | 2102–2224 | **3c** | uniforms math |
| `updateBgImageBuffer` | 1979–2008 | **3c** | uniforms/bg-image |
| `setScreenSize` | 1919–1936 | **3c** | resize + uniforms |
| `changeConfig` | 1841–1917 | **3c** | config swap |
| `setFontGrid` | 1070–1113 | **3c** | grid swap + uniforms |
| `DerivedConfig::init<C>` | 574–649 | **3c** (or higher) | real config extraction (§E) |
| `drawFrame`/`frameCompleted`/`prepBackgroundImage`/`uploadBackgroundImage`/`syncAtlasTexture`/`initShaders`/`deinitShaders`/`Renderer::init`/`deinit`/surface+loop+display methods | 1430–2010, 800–1068, 3358–3372 | **Chunk 4** | GPU golden |

### A.2 Why `updateFrame` is 3c, not 3b (justified against the gate)

The gate is: *"compiles + unit-testable by filling `Contents` from a constructed
`Terminal`/`RenderState`, NO GPU output."* `rebuild_cells` **is** exactly
"fill `Contents` from a constructed `RenderState`" and is directly, honestly
testable. `updateFrame` is not, for three independent reasons:

1. **It calls a 3c method.** `updateFrame` ends with
   `self.updateCustomShaderUniformsFromState()` (ref :1418), which is 3c uniforms
   math. Porting `updateFrame` in 3b would force declaring that method as an
   `unimplemented!()` leaf, and any end-to-end test of `updateFrame` would panic
   at it. That is 300 lines of subtle vt-interaction code with a guaranteed panic
   path and **zero** test coverage — exactly the risk the gate-coverage note
   warns about (no comment-coverage gate on this crate).
2. **It needs a live `State` + the images kitty path.** `updateFrame` locks
   `state.mutex`, reads `state.terminal.modes`, runs
   `self.terminal_state.update(state.terminal)`, and drives
   `self.images.kitty_requires_update`/`kitty_update`/`overlay_update` under
   `self.draw_mutex`. That is a much wider surface than "fill `Contents`," and it
   requires a live `*mut Terminal` behind a `*mut Mutex` — well beyond a
   constructed `RenderState`.
3. **It is the natural companion of the uniforms cluster.** Landing `updateFrame`
   with `updateCustomShaderUniformsFromState` (and the rest of the uniforms math)
   in 3c means it lands with its seam filled, so 3c can test it end-to-end
   (construct `State`+`Terminal`, call `updateFrame`, assert `Contents` +
   `uniforms` + `scrollbar`). That is a strictly better test than a panicking 3b
   stub.

Drawing the 3b/3c line just below `updateFrame` yields a tight, fully testable
3b: `rebuild_cells` and everything it transitively calls. Every dependency of the
3b subtree is already ported (§A.3); `updateFrame`'s one un-ported dependency is
the 3c uniforms leaf, which is precisely what makes it belong in 3c.

Note this does **not** shrink the item-3 analysis obligation: §C maps the full
RenderState walk, including the parts `updateFrame` performs, so 3c inherits a
finished spec.

### A.3 Dependency readiness (all 3b deps verified ported)

Every external symbol the 3b subtree needs is already committed:

- `Contents` API: `resize`/`reset`/`clear`/`set_cursor`/`bg_cell`/`add`
  (`cell.rs`). `Key`, `is_covering`, `is_symbol`, `constraint_width`,
  `no_min_contrast` (`cell.rs`). ✔
- `cursor::Style`, `cursor::style` (`cursor.rs`); `state::Preedit`,
  `state::Codepoint`, `PreeditRange` (`state.rs`). ✔
- `shaders`: `CellText`, `CellBg`, `Atlas`, `CellTextBools`, `Uniforms`
  (+ `UniformsPaddingExtend`, `UniformsBools`). ✔
- vt: `render::RenderState`/`Row`/`Cell`/`Highlight`/`Cursor`/`Viewport`/
  `Colors`/`CellSet`; `render::CellSet::contains`; `style::Style::{bg,fg,
  underline_color}` + `style::Fg` + `style::BoldColor`; `page::Cell::{codepoint,
  grid_width, wide, has_styling, content_tag}`; `page::Row` (Copy) +
  `SemanticPrompt`; `color::{RGB, Palette}`; `size::CellCountInt`;
  `sgr::Underline`. ✔
- font: `SharedGrid::{render_glyph, render_codepoint}` → `Render { glyph,
  presentation }` with `Glyph { width,height,offset_x,offset_y,atlas_x,atlas_y }`;
  `Presentation::{Text,Emoji}`; `RenderOptions { grid_metrics, cell_width:
  Option<u8>, constraint, constraint_width, thicken, thicken_strength }`;
  `Constraint::none`, `ConstraintSize::Fit`; `Metrics`; `sprite::Sprite`
  (all 11 variants) + `SPRITE_INDEX`; `nerd_font_attributes::get_constraint`. ✔
- shaper: `Shaper::{run_iterator, shape, end_frame}`; `shape::{RunOptions
  (+apply_break_config), TextRun, Cell, Cache as ShaperCache}`;
  `RunIterator::next`; `RunIteratorHook { shaper: *mut Shaper }` (already
  raw-ptr-based — makes `run_iterator`+`shape` interleaving sound, §C.3). ✔
- overlay (macos, P1): `Overlay::{init(Size), reset, apply_features(&RenderState,
  &[Feature])}`, `Feature`, `InitError`, `Overlay.surface` (z2d `Surface` with
  `get_width`/`get_height`), `Overlay.cell_size`; `Size::terminal()`. ✔

Only two symbols the reference `rebuildRow` uses are **not yet ported** →
prerequisites in §B: `renderer/row.zig::neverExtendBg`, and a real
`TerminalColor`/`Color`.

---

## B. PREREQUISITES

### B.1 New file `crates/rustty-renderer/src/row.rs` — `never_extend_bg`

The reference imports `rowNeverExtendBg = @import("row.zig").neverExtendBg`
(generic.zig:19); `renderer/row.zig` is a ~60-line pure helper used only by
`rebuildRow`'s `.extend` padding heuristic (ref :2640, :2648). Port it verbatim
as a new module. Register `pub mod row;` in `lib.rs`.

Signature (recommended AoS adaptation):

```rust
//! PORTS: ghostty src/renderer/row.zig  (// TODO: Test neverExtendBg function)
use rustty_vt::color;
use rustty_vt::page;
use rustty_vt::style::{Fg, Style};   // Fg only if needed; bg() needs cell+palette

/// Returns true if the row of this pin should never have its background
/// color extended for filling padding space in the renderer. ...
// PORT NOTE: the reference takes two SoA projections — `cells: []const
// page.Cell` and `styles: []const Style` — obtained from the row's
// MultiArrayList via `.items(.raw)`/`.items(.style)`. rustty's render cells are
// AoS (`render::Cell` carrying `.raw` + `.style`), so this takes the single
// `&[render::Cell]` slice and reads `.raw`/`.style` inline (the same AoS
// adaptation `cell.rs::constraint_width` established). Behavior is identical.
pub fn never_extend_bg(
    row: page::Row,
    cells: &[rustty_vt::render::Cell],
    palette: &color::Palette,
    default_background: color::RGB,
) -> bool {
    // switch (row.semantic_prompt) { .prompt, .prompt_continuation => return true, .none => {} }
    // (Rust: `row.semantic_prompt()` getter — page::Row is a packed struct.)
    // for (0.., cells) |x, *cell| { switch (cell.raw.content_tag()) { ... } }
    //   bg_color_palette|bg_color_rgb: s = if raw.has_styling() cell.style else default;
    //                                  bg = s.bg(&raw, palette) orelse return true;
    //                                  if bg.eql(default_background) return true;
    //   codepoint|codepoint_grapheme: powerline ranges 0xE0B0..=0xE0C8, 0xE0CA,
    //                                  0xE0CC..=0xE0D2, 0xE0D4 => return true;
    //                                  then same style/bg default check.
    // return false;
}
```

Hand-check points (adversary): the four powerline codepoint ranges must be
byte-identical to row.zig:42–45; `content_tag`/`has_styling`/`bg`/`eql`
map 1:1; `semantic_prompt` accessor is a packed getter in rustty (`()`), a field
in Zig. The reference file's `// TODO: Test neverExtendBg function` is preserved
verbatim; row.zig has **no** `test{}` block, so none is ported (R1 satisfied).

> ALTERNATIVE (rejected): keep the reference's two-slice signature (`cells:
> &[page::Cell], styles: &[Style]`) and have `rebuild_row` materialize both
> `Vec`s for the `.extend` call. Rejected: it allocates two `Vec`s per frame on
> rows 0 and N−1 for zero behavioral gain. The AoS single-slice form is the
> established precedent and allocation-free.

### B.2 Promote `TerminalColor` to a real enum (drives §E)

See §E for the full rationale and signatures. This is a prerequisite because
`rebuild_row`/`rebuild_cells` **destructure** `TerminalColor` (`.color =>
color.to_terminal_rgb()`, `.cell-foreground => …`, `.cell-background => …`), which
the current unit-struct placeholder cannot express.

---

## C. THE RENDERSTATE WALK (item 3) + ALIASING HAZARDS

### C.1 What the fill reads from `self.terminal_state` (= `RenderState`)

`rebuild_cells` binds `state = &self.terminal_state` (ref :2313) and reads:

| Reference access | rustty API (`rustty_vt::render`) |
|---|---|
| `state.rows`, `state.cols` | `RenderState.rows`, `.cols` |
| `state.dirty` (`.full`/`.partial`/`.false`) | `RenderState.dirty : Dirty::{Full,Partial,False}` |
| `state.row_data.slice().items(.raw)` | `row_data[y].raw : page::Row` (AoS field) |
| `…items(.cells)` | `row_data[y].cells : Vec<render::Cell>` |
| `…items(.dirty)` (mutated → false) | `row_data[y].dirty : bool` (write-back, §C.3) |
| `…items(.selection)` | `row_data[y].selection : Option<[CellCountInt;2]>` |
| `…items(.highlights)` | `row_data[y].highlights : Vec<render::Highlight>` |
| `state.cursor.viewport` | `RenderState.cursor.viewport : Option<Viewport>` (`.x/.y/.wide_tail`) |
| `state.cursor.cell` (`.wide`) | `RenderState.cursor.cell : page::Cell` |
| `state.cursor` (→ `add_cursor`) | `&RenderState.cursor` |
| `state.colors.{foreground,background,cursor,palette}` | `RenderState.colors.*` |

`rebuild_row` (ref :2620 `state = &self.terminal_state`) additionally reads
`state.colors.palette`/`.foreground`/`.background`, `state.cursor.viewport`, and
`state.cursor.cell` (via `run_iter_opts.cursor_x` and the shaper). It also reads
each `render::Cell`'s `.raw` (`page::Cell`) and `.style` (`Style`).

`page::Cell` methods used: `codepoint()`, `grid_width()`, `wide()` (→
`page::Wide::{Narrow,Wide,SpacerHead,SpacerTail}`), `has_styling()`,
`content_tag()`. `Style` methods: `bg(&cell, &palette) -> Option<RGB>`,
`fg(Fg{default,palette,bold}) -> RGB`, `underline_color(&palette) ->
Option<RGB>`, and the `flags.{inverse,faint,underline,overline,strikethrough,
invisible,bold}` fields. `render::CellSet::contains(&point::Coordinate)` (VERIFIED:
`CellSet = AutoArrayHashMapUnmanaged<point::Coordinate,()>`, `contains` takes the
key by reference — `&point::Coordinate { x, y }`, not by value).

**No write ever targets `self.terminal_state`** during the fill except the
per-row `dirty = false` write-back (ref :2425), which happens in `rebuild_cells`
*before* it calls `rebuild_row`, i.e. not aliased with the shaper reads. This is
what makes the raw-pointer aliasing sound (§C.3).

### C.2 `rebuildOverlay`'s walk

`rebuild_overlay` reads `self.overlay` (`Option<Overlay>`), `self.size`
(`Size` — `size.terminal()`, `size.cell.{width,height}`), `self.terminal_state`
(passed to `apply_features`), `self.alloc`. It calls `Overlay::init(self.size)`
(rustty drops the reference `alloc` arg — `Overlay::init` takes only `Size`) and
`overlay.apply_features(&self.terminal_state, features)` (rustty drops the `alloc`
arg). `features: &[Overlay::Feature]` is passed in; in `updateFrame` (3c) it is
always empty (the inspector is the uninhabited `state::Inspector`), but 3b tests
it with a real `Feature` (§G).

### C.3 Aliasing hazards — the raw-pointer triad (memory `rust-mut-args-strongly-protected`)

The reference structure is: `&mut self` methods that hold a shared alias into
`self.terminal_state` **and** a slice into a shaper/cache-owned buffer, live
across mutations of *other* `self` fields (`self.cells`, `self.uniforms`,
`self.font_shaper`, `self.font_shaper_cache`). Rust's borrow checker rejects this,
and — this is the load-bearing correction from **adversary pass 2** — a naive
`let st = unsafe { &*state }` re-dereferenced across those mutations is not merely
rejected by the checker, it **compiles to UB under Stacked Borrows**, with no Miri
here to catch it (the 3b test is Metal-gated, §G / risk #2). The v1 justification
('never writes `terminal_state`' + 'disjoint fields' + 'Copy-out') is *necessary
but INSUFFICIENT*: the invalidation vector is not sibling-field writes, it is the
nested `&mut *self` reborrow that every `self.rebuild_row(…)`/`self.add_cursor(…)`/
`self.add_preedit_cell(…)`/`self.add_glyph(…)` call creates. Creating `&mut *self`
retags the **whole `Renderer` inline allocation** and pops any earlier alias that
points INTO that inline memory — regardless of whether the sub-call actually
touches the aliased field.

**The correct discriminator is by ALLOCATION, not by field-disjointness:**

- **HEAP-backed aliases — SB-SAFE across `&mut *self`** (may stay raw ptr / slice):
  the `row_data[y].cells`/`highlights` backing buffers, the `cell_buf` slice
  returned by `Shaper::shape`, the `Cache` Box slice from `get`, the row-local
  `cells_raw` `Vec`, and `run_iter_opts.cells`. A `&mut *self` reborrow retags the
  inline `Renderer` allocation, **not** these separate heap allocations, so a raw
  pointer into them survives the reborrow. (This is why §C.3(2) below is sound and
  why the v1 §D.5 demotion of `cells_raw` to a safe `&[page::Cell]` was already
  correctly reasoned.)
- **INLINE-`self` aliases — SB-UNSAFE across `&mut *self`** (must NOT be held live
  across a nested `&mut self` call): the reads THROUGH the `state` pointer of
  `terminal_state.{rows,cols,dirty,colors,cursor}`, and the `RunIteratorHook`'s
  `*mut Shaper` → `self.font_shaper`. These point into the inline `Renderer`
  allocation, so any `&mut *self` reborrow pops them and a later deref is
  use-after-pop UB (the memory `rust-mut-args-strongly-protected`).

**REMEDY — two sanctioned forms (pick per site):**
  1. **Hoist inline scalars to `Copy` locals** BEFORE the row loop / before any
     `&mut self` sub-call. Concretely, the cursor/preedit blocks (ref :2457–2588,
     which run AFTER the row loop's `self.rebuild_row(&mut *self)` calls) need
     `rows/cols/dirty` and `colors.{foreground,background,cursor,palette}`,
     `cursor.{viewport,cell}` — copy them out up front and read the locals, never
     re-deref `state`. The one read that is not a plain scalar — the cursor-style
     SGR lookup `state.row_data[cursor_vp.y].cells[cursor_vp.x]` (ref :2459–2461),
     which depends on the runtime `cursor_vp` — is reached via a raw pointer to the
     **row_data HEAP backing buffer** captured before the loop (heap ⇒ survives
     `&mut *self`), not via a fresh `state` deref.
  2. **Thread the whole fill through a raw `*mut Renderer` self-pointer** (the
     `intrusive-ptr-and-placeholder-fill` doctrine): every sub-call and field access
     is a raw-pointer field projection, so **no `&mut *self` reborrow is ever
     materialized** and the inline aliases are never popped. This is the route for
     the shaper-hook leg, which has *no* scalar to hoist (the hook must stay live
     across `run_iter.next` for the whole run loop).

Register every raw-ptr site in `.port/phase-2/MIRI_EXCEPTIONS.md`. The Metal-gated
3b test **cannot** exercise Miri, so these invariants are **adversary-only** — the
hand review is their sole backstop (risk #2).

**(1) The `terminal_state` alias.** Reference `const state: *RenderState =
&self.terminal_state;` held across `self.cells.*`, `self.rebuild_row`,
`self.add_cursor`, `self.add_preedit_cell`. The alias itself is faithful, but it
must obey the remedy above — no inline read through `state` may be live-crossed by
a `&mut *self` sub-call:

```rust
let state: *const RenderState = &self.terminal_state;
// SAFETY (the REAL invariant — NOT "disjoint fields"): no INLINE-self alias read
// through `state` is held live across a nested `&mut *self` reborrow. Every inline
// scalar a post-`rebuild_row` block needs (`colors.{foreground,background,cursor,
// palette}`, `cursor.{viewport,cell}`, `rows/cols/dirty`) is Copy-hoisted to a
// local BEFORE the row loop (remedy 1), or the whole fill is driven off a raw
// `*mut Renderer` so no `&mut *self` is ever materialized (remedy 2). The per-row
// `dirty=false` write-back goes through a separate `&mut self.terminal_state`
// *before* `rebuild_row`, so it is not aliased with any read. `page::Cell`/`Style`/
// `RGB` are `Copy`, so hoisted values leave the `&*state` region by copy.
let st = unsafe { &*state };   // reads confined to BEFORE the first &mut *self sub-call
```

The v1 "disjoint fields / never writes / Copy-out" bullets are retained ONLY as
the reason the *values* are safe to copy out; they do **not** license re-reading
`state` after a `&mut *self` call — that is exactly the hazard the remedy closes.

**(2) `rebuild_row`'s pointer args.** Reference passes pointers *into*
`self.terminal_state.row_data[y]`:
`fn rebuildRow(self, y, row: page.Row, cells: *MultiArrayList(Cell),
preedit_range, selection, highlights: *const ArrayList(Highlight), links)`. Port
`cells` and `highlights` as **raw const pointers** (mirroring the reference `*` /
`*const`):

```rust
fn rebuild_row(
    &mut self,
    y: CellCountInt,
    row: page::Row,                       // Copy — by value, matches `row: page.Row`
    cells: *const [rustty_vt::render::Cell],   // ref `*MultiArrayList(...)`
    preedit_range: Option<PreeditRange>,
    selection: Option<[CellCountInt; 2]>,
    highlights: *const [rustty_vt::render::Highlight], // ref `*const ArrayList(...)`
    links: &render::CellSet,
) -> Result<(), RebuildRowError>
```

These are **HEAP-backed** (they address the `Vec` backing buffers of
`row_data[y].cells`/`highlights`, not inline `Renderer` memory), so they are
SB-SAFE across the `&mut self` body — a `&mut *self` reborrow does not retag the
heap buffer. `run_iter_opts.cells` (the `RunOptions.cells: &'a [render::Cell]`) is
built from `unsafe { &*cells }`; the resulting `RunIterator` holds only that
synthetic-`'a` slice + a raw `*mut Shaper` (its hook), **not** a `&mut
self.font_shaper` borrow. NOTE the hook is the one INLINE-`self` leg here: it
aliases `self.font_shaper` (inline), so `self.font_shaper.shape(run)` (a `&mut
self.font_shaper`) and `self.add_glyph(&mut *self, …)` — each interleaved with
`run_iter.next()` re-dereffing the hook (ref :2673, :2702, :3020) — pop the hook's
tag unless the fill is driven off a raw `*mut Renderer` (remedy 2). Adopt remedy 2
for the whole `rebuild_row` shape loop.

> IMPLEMENTED STRUCTURE — Miri-proven (SB + TB) on the isolated harness
> `.port/phase-2/mirihar-chunk3b/`. Remedy 2 as stated ("drive off a raw `*mut
> Renderer` so no `&mut *self` is materialized") is **necessary but NOT sufficient**:
> the harness `row_rawsibling` candidate shows that even with a raw `self_raw`,
> calling `shape` through `(*self_raw).font_shaper.shape(run)` (or any SIBLING raw
> `*mut Shaper` into `font_shaper`) still POPS the hook — the sibling reborrow at the
> `font_shaper` location invalidates the hook derived from `run_iterator`'s reborrow.
> The ONLY clean route (harness `row_rawself`/`row_surgical`/`row_persistent`/
> `row_realistic`, all CLEAN under both models) has THREE parts:
> 1. **`shape` is invoked through the hook's OWN raw pointer** —
>    `unsafe { (*run_iter.hooks.shaper).shape(run) }` — a CHILD reborrow of the hook
>    (pushed above it), which does not pop it. NEVER `self.font_shaper.shape(...)`.
> 2. **The in-loop `add_*` helpers are RAW-self associated fns** —
>    `unsafe fn add_glyph(self_raw: *mut Renderer, …)` etc., called via a
>    `let self_raw: *mut Renderer = self;` — using only NARROW `(*self_raw).field`
>    access inside (never a whole `&mut *self_raw`, which would cover `font_shaper`).
> 3. **Direct `self.field` access in the loop stays safe** because it is disjoint from
>    `font_shaper` (`self.cells.bg_cell`, `self.uniforms`, `self.font_shaper_cache`
>    each retag a DIFFERENT location, leaving the hook's `font_shaper` stack intact).
> `add_cursor` similarly takes `self_raw: *mut Renderer` (harness `cursor_rawself`) so
> its receiver does not pop the `cursor_state` alias into inline `self.terminal_state.
> cursor`. See `MIRI_EXCEPTIONS.md` (GPU R-d Chunk 3b) for the full evidence table.

**(3) The shaper cell buffer.** Reference `shaper_cells: ?[]const
font.shape.Cell` aliases either the shaper-owned buffer (from `self.font_shaper.
shape(run)`, "valid until the next shaping call" per ref :2731–2734) or the
cache-owned buffer (from `self.font_shaper_cache.get(run)`), and is held across
`self.add_glyph`, `self.font_shaper_cache.put`, and `run_iter.next`.

**CORRECTION (adversary pass 2): this leg is HEAP-backed and is NOT the primary
hazard the v1 doc made it.** Both `Shaper::shape` and `Cache::get` return a slice
into a **separate heap allocation** — the shaper's `cell_buf` `ArrayList` backing
store, or the cache's Box — reachable via a pointer stored inline, but the pointee
is not inside the `Renderer` allocation. So a raw `*const [shape::Cell]` captured
from it **survives `&mut *self`** (the reborrow retags inline `Renderer` memory,
not the heap buffer). What remains for this leg is a *logical-lifetime* obligation,
**not** a `&mut *self` Stacked-Borrows hazard:
- `Shaper::shape(&mut self, run: shape::TextRun) -> Result<&[shape::Cell], Error>`
  — returns a slice into the shaper's heap `cell_buf`; capturing the raw ptr ends
  the field borrow so the subsequent `&mut self` calls compile.
- `shape::Cache::get(&mut self, run: &TextRun) -> Option<&[shape::Cell]>` — returns
  a slice into the cache's heap storage.
- `shape::Cache::put(&mut self, run: &TextRun, cells: &[shape::Cell])` — **no
  `alloc` arg** (the reference `put(self.alloc, run, cells)` drops it); it COPIES
  `cells` into cache-owned storage (does not free/alias the shaper buffer).

Port as a raw slice pointer:

```rust
let mut shaper_cells: Option<*const [shape::Cell]> = None;
// captured right after `shape`/`get`; read via `unsafe { &*p }` in the glyph loop
```

Sound because `shape::Cell` is `Copy` POD (`x,x_offset,y_offset,glyph_index`) and
the LOGICAL window holds: the pointer is captured immediately after `shape`/`get`
and consumed within the same run's glyph loop; `put(run, new_cells)` clones into
the cache (does not free the shaper buffer); `run_iter.next` resets `shaper_cells =
None` before the next `shape` overwrites `cell_buf`. The reference's documented
"valid until next shaping call" contract (ref :2731–2734 / :3000–3003, ported
verbatim per §D.9) maps exactly onto this window. The adversary must certify the
logical window (put-copies / one-run-scope / next-nulls), but NOT an inline-alias
`&mut *self` hazard — there is none for this leg.

> Rejected alternative for (3): copy each run's shaped cells into a local
> `Vec<shape::Cell>`. It is behavior-identical (Copy POD) and removes the unsafe,
> but it deviates from the reference's zero-copy shaper-buffer contract and the
> project's raw-ptr-faithful doctrine. If the adversary cannot certify the raw
> pointer's LOGICAL lifetime window, the `Vec` copy is the sanctioned fallback —
> flag in OPEN RISKS, do not silently choose it. (This fallback does NOT address the
> inline `state`/hook legs — those need remedy 1 or 2, not a copy.)

### C.4 Fallibility mapping

- `rebuild_cells` → **`()`**. The reference `!void`'s only own error source is
  `try self.cells.resize` (infallible in rustty — `Vec`/`ArrayListCollection`
  over `Global` abort on OOM, per `cell.rs::resize`'s PORT NOTE) and the
  `rebuild_row` call, which is `catch`-logged inline. So `rebuild_cells` cannot
  fail → return `()` (NOT a vacuous `Result<(),_>`). `updateFrame` (3c) calls it
  directly, dropping the reference `catch |err| { assert OOM; log }`.
- `rebuild_row` → **`Result<(), RebuildRowError>`**. The un-caught `try`s are
  `run_iter.next(alloc)?` (`collection::Error`) and `self.font_shaper.shape(run)?`
  (`shaper::Error`). `add_*` calls are `catch`-logged inline (map to
  `.unwrap_or_else(|err| log::warn!(...))`). Define
  `enum RebuildRowError { Collection(collection::Error), Shape(shaper::Error) }`
  with `From` glue (the `macos_impl::Error` precedent). `rebuild_cells` catches it
  → `self.cells.clear(y)` + log (ref :2435–2442).
- `add_underline`/`add_overline`/`add_strikethrough`/`add_glyph`/
  `add_preedit_cell` → **`Result<(), RenderError>`** (`render_glyph`/
  `render_codepoint`'s `shared_grid::RenderError`; `Contents::add` is infallible).
- `add_cursor` → **`()`** (reference `fn addCursor(...) void`; it `catch`-logs
  `render_glyph`/`render_codepoint` internally and `return`s).

---

## D. CONCRETE RUST SIGNATURE BLOCKS

All methods live in `impl Renderer` inside the existing `#[cfg(target_os =
"macos")] mod macos_impl` in `generic.rs`. New imports needed there:
`rustty_vt::{page, style::{self, Style, Fg, BoldColor}, sgr, point, color,
render::{self, Cell as RenderCell, Highlight, CellSet}}`,
`rustty_font::sprite::Sprite`, `rustty_font::glyph::{Constraint, ConstraintSize,
RenderOptions}`, `rustty_font_shaper::{SPRITE_INDEX, Presentation, shape,
collection, shared_grid::RenderError}`, `crate::cursor`, `crate::state`,
`crate::cell::{self, Key}`, `crate::shaders::{CellText, CellTextBools, Atlas}`,
`crate::row::never_extend_bg`, `crate::size::Coordinate`.

### D.1 `PreeditRange` (private struct, ref :2294)

```rust
// const PreeditRange = struct { y, x: [2]CellCountInt, cp_offset: usize };
struct PreeditRange {
    y: CellCountInt,
    x: [CellCountInt; 2],
    cp_offset: usize,
}
```

### D.2 `rebuild_cells` (pub — dead-code entry, §F)

```rust
/// Convert the terminal state to GPU cells stored in CPU memory. ...
/// This requires the draw mutex. Dirty state on terminal state won't be reset.
// PORT NOTE: reference `!void`; the sole error source (`cells.resize`) aborts on
// OOM in rustty and `rebuild_row` is catch-logged, so this is infallible → `()`.
// `preedit` is the cloned `State::Preedit` (owned, by value, as in the ref).
// `cursor_style_` is the pre-computed `renderer.cursorStyle(...)` result
// (rustty `cursor::style`), passed by the caller. `links` is `*const CellSet`.
pub fn rebuild_cells(
    &mut self,
    preedit: Option<state::Preedit>,
    cursor_style_: Option<cursor::Style>,
    links: &CellSet,
) {
    // MUST-PORT verbatim (§D.9 #2): the `// const start = try std.time.Instant.now();
    //   … // defer { … "[rebuildCells time] {}\t{}" … }` timing block (ref :2315–2321).
    // let state: *const RenderState = &self.terminal_state;  (raw alias — §C.3(1))
    // grid_size_diff → self.cells.resize + self.uniforms.grid_size
    // rebuild = dirty==Full || grid_size_diff; if rebuild { self.cells.reset();
    //   match self.config.padding_color { Background=>{}, Extend|ExtendAlways=>{
    //     self.uniforms.padding_extend = PaddingExtend{up,down,left,right:true} } } }
    // preedit_range := via preedit.range(cursor_vp.x, cols-1)  (state::Preedit::range)
    //
    // §C.3 REMEDY: Copy-hoist every inline `terminal_state` scalar the cursor/preedit
    //   blocks read AFTER the row loop, BEFORE the loop's `&mut *self` sub-calls:
    //     let colors = unsafe{&*state}.colors;          // Colors is Copy (RGB + [RGB;256])
    //     let cursor = unsafe{&*state}.cursor;          // Cursor is Copy (viewport + page::Cell)
    //     let (rows, cols) = (unsafe{&*state}.rows, unsafe{&*state}.cols);
    //     let row_data_ptr: *const [_] = unsafe{&*state}.row_data.as_slice(); // HEAP buf ptr
    //   Thereafter the cursor/preedit blocks read `colors`/`cursor`/`rows`/`cols`
    //   (locals) and `unsafe{&*row_data_ptr}` (heap) — they NEVER re-deref `state`
    //   across a `&mut *self` call. (Alternatively drive the whole fn off `*mut Renderer`.)
    // for y in 0..row_len { if !rebuild { if !dirty {continue}; self.cells.clear(y) }
    //   dirty = false (write-back BEFORE rebuild_row — §C.3);
    //   self.rebuild_row(y, row, &cells[..] as *const _, preedit_range,
    //                    selection, &highlights[..] as *const _, links)
    //     .unwrap_or_else(|e| { log::warn!(...); self.cells.clear(y) }); }
    // cursor block (ref :2446–2578): self.cells.set_cursor(None,None);
    //   uniforms.cursor_pos = [u16::MAX; 2];
    //   cursor_vp = cursor.viewport orelse break;   (HOISTED local, not st.cursor.viewport)
    //   cursor_style := the SGR Style AT the cursor cell (ref :2458–2465, distinct
    //     from `style`=the VISUAL cursor style below): let cc =
    //     &unsafe{&*row_data_ptr}[cursor_vp.y].cells[cursor_vp.x]; if cc.raw.has_styling()
    //     { cc.style } else { Style::default() }  — load-bearing: it feeds the
    //     `cell-foreground`/`cell-background` cursor-color + cursor-text branches
    //     (cursor_style.fg(..) / cursor_style.bg(&cursor.cell, &colors.palette)).
    //   if preedit.is_some() break;  let style = cursor_style_ orelse break;
    //   compute cursor_color via colors.cursor / self.config.cursor_color
    //     destructure (§E; uses `cursor_style` for the cell-fg/bg arms);
    //   self.add_cursor(&cursor as *const _, style, cursor_color);  // §D.6: *const render::Cursor
    //   if style==Block { uniforms.cursor_pos/bools.cursor_wide/cursor_color via
    //     self.config.cursor_text destructure (§E; reads `cursor.cell.wide`, `colors.*`). }
    // preedit block (ref :2580–2599): for cp in preedit.codepoints[cp_offset..] {
    //   self.add_preedit_cell(cp, Coordinate{x,y:range.y}, colors.foreground)
    //     .unwrap_or_else(|e| log::warn!(...)); x += if cp.wide {2} else {1}; }
    // self.cells_rebuilt = true;
    // MUST-PORT verbatim (§D.9 #3): the trailing `// Log some things` +
    //   `// log.debug("rebuildCells complete cached_runs={}", …)` block (ref :2604–2607).
}
```

### D.3 `rebuild_row` (private; signature in §C.3(2))

Returns `Result<(), RebuildRowError>`. Body maps ref :2620–3055 line-for-line:
padding `.extend` heuristic via `never_extend_bg(row, unsafe{&*cells}, palette,
bg)` (ref :2634–2656); `RunOptions{ grid: self.font_grid, cells: unsafe{&*cells},
selection, cursor_x }` + `apply_break_config(self.config.font_shaping_break)`
(ref :2658–2673); the preedit-skip run-catch-up loop (ref :2683–2743); the
`selected` precedence enum (ref :2752–2786) — **MUST-ADD (adversary pass 2):** the
reference `const tag: HighlightTag = @enumFromInt(hl.tag)` (ref :2777) needs an
explicit `u8 -> HighlightTag` conversion, because `HighlightTag` (generic.rs:458)
currently has **no** `From`/`TryFrom`. Add one mapping `0 => SearchMatch`,
`1 => SearchMatchSelected` (the `#[repr(u8)]` discriminant order), and make an
out-of-range tag **panic** (`match hl.tag { 0=>.., 1=>.., other =>
unreachable!("invalid HighlightTag {other}") }`), matching Zig's *checked*
`@enumFromInt` (safety-checked illegal behavior / panic in Debug+ReleaseSafe) — do
**NOT** silently wrap or truncate. `render::Highlight.tag` is the `u8` source; the
port then `switch (tag) { .search_match => .search, .search_match_selected =>
.search_selected }`; `bg`/`fg` selection/search
resolution (ref :2788–2875, all `TerminalColor` destructures — §E); the
`bg_alpha` ladder (ref :2884–2911) → `*self.cells.bg_cell(y,x) = [r,g,b,a]`;
`invisible` skip (ref :2926); link underline upgrade via `links.contains(...)`
(ref :2933–2944); `add_underline`/`add_overline` (ref :2949–2967); the shaper
run/`add_glyph` loop (ref :2969–3041, raw shaper-cell ptr §C.3(3)); trailing
`add_strikethrough` (ref :3044–3054). All `add_*` calls `.unwrap_or_else(|e|
log::warn!(...))`. `run_iter.next(self.alloc)?` / `self.font_shaper.shape(run)?`
propagate.

### D.4 `add_underline` / `add_overline` / `add_strikethrough`

```rust
// fn addUnderline(self, x, y, style: Attribute.Underline, color: RGB, alpha: u8) !void
fn add_underline(
    &mut self, x: CellCountInt, y: CellCountInt,
    style: sgr::Underline, color: color::RGB, alpha: u8,
) -> Result<(), RenderError> {
    // sprite = match style { None=>unreachable!(), Single=>Underline,
    //   Double=>UnderlineDouble, Dotted=>UnderlineDotted, Dashed=>UnderlineDashed,
    //   Curly=>UnderlineCurly };
    // render = unsafe{&mut *self.font_grid}.render_glyph(self.alloc, SPRITE_INDEX,
    //   sprite as u32, RenderOptions{ cell_width: Some(1), grid_metrics:
    //   self.grid_metrics, ..RenderOptions::default() })?;
    // self.cells.add(Key::Underline, CellText{ atlas: Atlas::Grayscale,
    //   grid_pos:[x,y], color:[color.r,color.g,color.b,alpha],
    //   glyph_pos:[g.atlas_x,g.atlas_y], glyph_size:[g.width,g.height],
    //   bearings:[g.offset_x as i16, g.offset_y as i16], ..Default::default() });
    // Ok(())
}
```

`add_overline` / `add_strikethrough` are identical with `Sprite::Overline` /
`Sprite::Strikethrough` and `Key::Overline` / `Key::Strikethrough`.

> HAND-CHECK: `self.font_grid` is `*mut SharedGrid` — every call is
> `unsafe { &mut *self.font_grid }.render_glyph(...)`. `render_glyph` takes
> `&mut self` (it caches). This is a raw-ptr deref of the *shared* grid handle
> (the `Options.font_grid` decision); the grid has its own internal `RwLock`, so
> the `&mut` through the raw ptr is the reference's `self.font_grid.renderGlyph`
> exactly. Confirm `RenderOptions` default fields match (`constraint: none`,
> `constraint_width: 1`, `thicken:false`, `thicken_strength:255`) — the
> reference sets only `cell_width` + `grid_metrics` for decorations.

### D.5 `add_glyph`

```rust
// fn addGlyph(self, x, y, cols: usize, cell_raws: []const page.Cell,
//             shaper_cell: shape.Cell, shaper_run: shape.TextRun,
//             color: RGB, alpha: u8) !void
fn add_glyph(
    &mut self, x: CellCountInt, y: CellCountInt, cols: usize,
    cell_raws: &[page::Cell],             // see note — safe slice, NOT the triad
    shaper_cell: shape::Cell,
    shaper_run: shape::TextRun,
    color: color::RGB, alpha: u8,
) -> Result<(), RenderError> {
    // let cell = cell_raws[x]; let cp = cell.codepoint();   // safe slice
    // constraint = get_constraint(cp).unwrap_or_else(|| if cell::is_symbol(cp)
    //   Constraint{ size: ConstraintSize::Fit, ..Constraint::none } else
    //   Constraint::none);
    // render = grid.render_glyph(self.alloc, shaper_run.font_index,
    //   shaper_cell.glyph_index, RenderOptions{ grid_metrics: self.grid_metrics,
    //   thicken: self.config.font_thicken, thicken_strength:
    //   self.config.font_thicken_strength, cell_width: Some(cell.grid_width()),
    //   constraint, constraint_width: cell::constraint_width(cell_raws, x, cols) })?;
    // if render.glyph.width==0 || render.glyph.height==0 { return Ok(()) }
    // atlas = match render.presentation { Emoji=>Atlas::Color, Text=>Atlas::Grayscale };
    // self.cells.add(Key::Text, CellText{ atlas, bools: CellTextBools{
    //   no_min_contrast: cell::no_min_contrast(cp), ..Default::default() },
    //   grid_pos:[x,y], color:[..,alpha], glyph_pos/glyph_size as above,
    //   bearings:[(g.offset_x + shaper_cell.x_offset as i32) as i16,
    //             (g.offset_y + shaper_cell.y_offset as i32) as i16] });
    // Ok(())
}
```

> HAND-CHECK: `cell::constraint_width` (cell.rs) takes `&[page::Cell]` — so
> `add_glyph` needs the `page::Cell` projection, not `render::Cell`. The reference
> `cell_raws: []const page.Cell` is exactly the `.items(.raw)` projection.
> `rebuild_row` supplies it from the `page::Cell` `Vec` it builds once per row
> (`row_cells.iter().map(|c| c.raw).collect::<Vec<_>>()`, the `cell.rs::raw_cells`
> test precedent), matching the reference's SoA `cells_raw`. CORRECTION (reduces
> unsafe): because rustty's `render::Cell` is AoS, `cells_raw` is **necessarily a
> fresh row-local `Vec`** (you cannot reinterpret `&[render::Cell]` as
> `&[page::Cell]`) — it does NOT alias `self.terminal_state`. So `add_glyph`'s
> `cell_raws` is a plain safe `&[page::Cell]` borrowing that local `Vec`; passing
> it to `add_glyph(&mut self, …)` is sound (it borrows the local, not `self`).
> This arg is therefore NOT one of the §C.3 raw-pointer triad — the reference's
> "cells_raw aliases the MAL" rationale is dissolved by the mandatory AoS copy.
> (`cell::constraint_width(cell_raws, x, cols)` likewise takes the safe slice.) `get_constraint` is
> `rustty_font::nerd_font_attributes::get_constraint`. Bearings add
> `shaper_cell.{x,y}_offset` (i16) to `g.offset_{x,y}` (i32) — widen then narrow,
> matching the reference `@intCast(offset + shaper_offset)`.

### D.6 `add_cursor` (returns `()`)

```rust
// fn addCursor(self, cursor_state: *const RenderState.Cursor,
//              cursor_style: renderer.CursorStyle, cursor_color: RGB) void
fn add_cursor(
    &mut self,
    // RAW const ptr — mirrors the reference `cursor_state: *const RenderState.Cursor`
    // (ref :3226). The caller's `&st.cursor` aliases INLINE `self.terminal_state.
    // cursor`; handing a safe `&render::Cursor` into this `&mut self` method is the
    // inline-alias-across-`&mut *self` hazard (§C.3) — it compiles only because the
    // `unsafe{&*state}` lifetime is unbounded, i.e. it silently compiles to UB with
    // no Miri to catch it. Take the raw ptr and Copy-out the two scalars we need at
    // the very top, BEFORE any `self.cells`/`self.font_grid` access.
    cursor_state: *const render::Cursor,
    cursor_style: cursor::Style,
    cursor_color: color::RGB,
) {
    // Copy-out the needed scalars up front (both `Copy`), then never re-deref
    // `cursor_state` after the first `&mut self`-field access:
    //   let viewport = match unsafe{ (*cursor_state).viewport } { Some(v)=>v, None=>return };
    //   let cell_wide = unsafe{ (*cursor_state).cell.wide() };   // page::Wide (Copy)
    // cursor_vp = viewport;   (was: cursor_state.viewport)
    // (wide, x) = if !cursor_vp.wide_tail { (cell_wide == Wide::Wide,
    //   cursor_vp.x) } else { (true, cursor_vp.x - 1) };
    // alpha = if !self.focused { 255 } else { (255.0*self.config.cursor_opacity)
    //   .ceil() as u8 };
    // render = match cursor_style { Block=>CursorRect, BlockHollow=>CursorHollowRect,
    //   Bar=>CursorBar, Underline=>CursorUnderline (via grid.render_glyph(SPRITE_INDEX,
    //   sprite, cell_width: if wide {2} else {1})  .. catch log+return),
    //   Lock=> grid.render_codepoint(0xF023, Style::Regular?, Presentation::Text,
    //   ..) catch log+return; None-inner => log+return };
    //   (VERIFIED: the font `style` arg is `rustty_font_shaper::Style::Regular`
    //    (lib.rs:121; `Regular = 0`) — reference `.regular`. NOTE: there is NO
    //    `codepoint_resolver::Style`; `render_codepoint`'s in-scope `Style` is
    //    `crate::Style` = `rustty_font_shaper::Style` (shared_grid.rs:45,:400). Sig:
    //    `render_codepoint(alloc, cp:u32, Style, Option<Presentation>, RenderOptions)`.)
    // self.cells.set_cursor(Some(CellText{ atlas: Grayscale, bools:
    //   CellTextBools{ is_cursor_glyph:true, ..}, grid_pos:[x, cursor_vp.y],
    //   color:[..,alpha], glyph_pos/size/bearings }), Some(cursor_style));
}
```

> HAND-CHECK: `renderer.CursorStyle` is rustty `cursor::Style` (Block/BlockHollow/
> Bar/Underline/Lock). The reference `sprite` switch has `.lock => unreachable`
> inside the rect arm and a separate `.lock` arm using `renderCodepoint(0xF023)`.
> `render_codepoint`'s `style` arg — the reference passes `.regular` (a
> `font.Style`); the rustty `Style` is `rustty_font_shaper::Style::Regular`
> (lib.rs:121, `Regular = 0`) — the top-level re-export, which is `render_codepoint`'s
> in-scope `Style` (`use crate::{Presentation, Style};`, shared_grid.rs:45). There is
> **no** `codepoint_resolver::Style` (an earlier draft named that path — it does not
> exist), and `p: Some(Presentation::Text)` for the text presentation
> (reference `.text`). `render_codepoint` returns `Result<Option<Render>,_>` — the
> `orelse { log; return }` maps to `match … { Ok(Some(r))=>r, Ok(None)=>{log;
> return}, Err(e)=>{log; return} }`.

### D.7 `add_preedit_cell`

```rust
// fn addPreeditCell(self, cp: State.Preedit.Codepoint, coord: Coordinate,
//                   screen_fg: RGB) !void
fn add_preedit_cell(
    &mut self,
    cp: state::Codepoint,
    coord: point::Coordinate,   // reference terminal.Coordinate — rustty point::Coordinate
    screen_fg: color::RGB,
) -> Result<(), RenderError> {
    // render = grid.render_codepoint(self.alloc, cp.codepoint,
    //   rustty_font_shaper::Style::Regular, Some(Presentation::Text),
    //   RenderOptions{ grid_metrics: self.grid_metrics, ..default })
    //   catch log+return; orelse log+return;
    //   (VERIFIED `rustty_font_shaper::Style::Regular` (lib.rs:121); `render_codepoint`'s
    //    in-scope `Style` = `crate::Style` (shared_grid.rs:45) — NOT
    //    `codepoint_resolver::Style` (no such type); `p: Option<Presentation>` so `Some(..Text)`.)
    // self.cells.add(Key::Text, CellText{ atlas: Grayscale, grid_pos:[coord.x,
    //   coord.y], color:[fg..,255], glyph_pos/size/bearings });
    // self.add_underline(coord.x, coord.y, Underline::Single, screen_fg, 255)?;
    // if cp.wide && coord.x < self.cells.size.columns - 1 {
    //   self.add_underline(coord.x+1, coord.y, Underline::Single, screen_fg, 255)?; }
    // Ok(())
}
```

### D.8 `rebuild_overlay` (pub — dead-code entry, §F)

```rust
// fn rebuildOverlay(self, features: []const Overlay.Feature) Overlay.InitError!void
pub fn rebuild_overlay(
    &mut self,
    features: &[crate::overlay::Feature],
) -> Result<(), crate::overlay::InitError> {
    // if features.is_empty() { if let Some(o)=self.overlay.as_mut(){ o.deinit(alloc)?
    //   -> in rustty Overlay drops; set self.overlay=None } return Ok(()) }
    // existing-size-match reuse (surface.get_width/height vs size.terminal();
    //   cell_size vs size.cell) -> o.reset(); else Overlay::init(self.size)?;
    // overlay.apply_features(&self.terminal_state, features);  (rustty: no alloc arg)
    // Ok(())
}
```

> HAND-CHECK: `Overlay::init` takes only `Size` (drops the reference `alloc`);
> `apply_features(&RenderState, &[Feature])` drops the reference `alloc`;
> `o.reset()` matches. The `break :existing` reuse-vs-reinit control flow (ref
> :2254–2286) must be preserved — it is a labeled-block early-out, port as an
> explicit `if let … { … } ` with a helper or a `'existing: loop { break }`-style
> structure. `Overlay::init` can `Err(InvalidDimensions)`; keep the `Result`.

### D.9 R1 must-port commented-out reference blocks (NOT G3b-gated for this crate)

Because G3b comment-coverage SKIPS `rustty-renderer` (top-of-doc CRITICAL note),
these reference comments are the easiest R1 fidelity casualties — they carry no
executable weight, so nothing but the adversary/verify pass forces them in. Port
each **verbatim** (as Rust `//` comments), in place:

1. **`rebuildOverlay` timing `defer` block** — ref :2230–2238 (the
   `// const start = std.time.Instant.now() … // defer { … log.warn("[rebuildOverlay
   time] …") … }` block). §D.8's sketch must open with it.
2. **`rebuildCells` timing block** — ref :2315–2321 (the `// const start = try
   std.time.Instant.now(); … // defer { … "[rebuildCells time] {}\t{}" … }` block).
   §D.2's sketch must open with it.
3. **`rebuildCells` trailing debug log** — ref :2604–2607 (`// Log some things` +
   `// log.debug("rebuildCells complete cached_runs={}", .{ // self.font_shaper_cache
   .count(), // });`), immediately after `self.cells_rebuilt = true;`.
4. **The per-run "valid until the next shaping call" comment** — ref :2731–2734 AND
   its duplicate at ref :3000–3003 (`// The cells we get from direct shaping are
   always owned by the shaper and valid until the next shaping call so we can safely
   use them.`). Both occurrences appear in `rebuild_row` (the preedit catch-up
   shaping and the main glyph-loop shaping); port BOTH. These are also the
   human-readable statement of the §C.3(3) raw-pointer lifetime contract, so keeping
   them verbatim doubles as the `// SAFETY`-adjacent rationale.

(This list is not exhaustive of every line comment in the subtree — every reference
comment must still be hand-checked per the top-of-doc note — but these commented-out
*blocks* are the ones most likely to be dropped and are called out so they are not.)

---

## E. DerivedConfig / TerminalColor (item 2)

### E.1 DECISION: `TerminalColor` becomes a real renderer-local enum NOW

**Required, not optional.** `rebuild_row`/`rebuild_cells` destructure the union in
seven places (selection bg/fg, search bg/fg, search-selected bg/fg, cursor color,
cursor-text). Example (ref :2808–2812):

```zig
.selection => if (self.config.selection_background) |v| switch (v) {
    .color => |color| color.toTerminalRGB(),
    .@"cell-foreground" => if (style.flags.inverse) bg_style else fg_style,
    .@"cell-background" => if (style.flags.inverse) fg_style else bg_style,
} else state.colors.foreground,
```

The current unit-struct `pub struct TerminalColor;` cannot be destructured, so
the fill cannot be ported faithfully against it. Promote it (replacing the
placeholder in `generic.rs`):

```rust
/// PORTS: `configpkg.Config.Color` (Config.zig:5413). Renderer-LOCAL (not hoisted
/// to render-types) so it can name `rustty_vt::color::RGB` — the renderer already
/// deps vt, so this creates NO cycle; hoisting to render-types WOULD cycle
/// (render-types has no vt edge, and vt→render-types exists).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color { pub r: u8, pub g: u8, pub b: u8 }

impl Color {
    // pub fn toTerminalRGB(self: Color) terminal.color.RGB {
    //     return .{ .r = self.r, .g = self.g, .b = self.b };
    // }
    pub fn to_terminal_rgb(self) -> RGB { RGB { r: self.r, g: self.g, b: self.b } }
}

/// PORTS: `configpkg.Config.TerminalColor` (Config.zig:5549), a
/// `union(enum) { color: Color, @"cell-foreground", @"cell-background" }`.
/// Renderer-local (see `Color`). The fill destructures this directly; `init<C>`
/// (3c) converts from `rustty_config::Config`'s own `TerminalColor` via a `From`
/// bound to the concrete `C` in core (the generic-`C` seam).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalColor {
    Color(Color),
    CellForeground,
    CellBackground,
}
```

The eight `DerivedConfig` fields switch from the placeholder to this real enum
(`cursor_color`/`cursor_text`/`selection_background`/`selection_foreground` are
`Option<TerminalColor>`; the four `search_*` are `TerminalColor`). Their doc
comments and the struct's other fields are unchanged. Note: the reference cell
fill calls the INNER `Color.toTerminalRGB()` (returns `RGB`) after destructuring —
it does NOT call `TerminalColor.toTerminalRGB()` (the `?RGB` one). Port
`Color::to_terminal_rgb`; the `TerminalColor::to_terminal_rgb -> Option<RGB>` and
`parse_cli`/`format_entry`/its `test{}`s are config-parsing surface, deferred with
`init<C>` (they are not on the fill path).

`Path`/`RepeatablePath` stay inhabited placeholders unchanged — the fill never
reads `config.bg_image`/`config.custom_shaders`, and the Chunk-4 golden uses no
background image or custom shaders (GPU_RD_BLUEPRINT offscreen golden gate).

### E.2 DECISION: `DerivedConfig::init<C>` stays a deferred seam; the golden builds `DerivedConfig` directly

`init<C>` extracts from `rustty_config::Config` (`config.@"cursor-color"` etc.).
A `renderer -> config` edge would cycle (G7); it is broken by the generic-`C`
binding that lands in core. Its body is config-parsing, not cell-fill, and the
golden gate does not need config parsing — only fixed color values. So `init<C>`
remains `unimplemented!()` (unchanged from 3a), and both the 3b test and the
Chunk-4 golden construct a `DerivedConfig` **literal** with fixed values via a
test fixture:

```rust
#[cfg(test)]
fn derived_config_for_test() -> DerivedConfig {
    DerivedConfig {
        arena: ArenaAllocator::init(),   // NO arg — `init()` returns `ArenaAllocator<Global>` (arena_allocator.rs:119)
        font_thicken: false, font_thicken_strength: 0,
        font_features: Vec::new(),
        font_styles: StyleStatus::init_fill(true),   // confirm ctor name
        font_shaping_break: FontShapingBreak::default(),
        cursor_color: Some(TerminalColor::Color(Color { r: 0xFF, g: 0, b: 0 })),
        cursor_opacity: 1.0,
        cursor_text: None,
        background: RGB { r: 0, g: 0, b: 0 },
        background_opacity: 1.0, background_opacity_cells: false,
        foreground: RGB { r: 0xFF, g: 0xFF, b: 0xFF },
        selection_background: None, selection_foreground: None,
        search_background: TerminalColor::Color(Color { r: 0xFF, g: 0xE0, b: 0x82 }),
        search_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
        search_selected_background: TerminalColor::Color(Color{ r:0xF2,g:0xA5,b:0x7E }),
        search_selected_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
        bold_color: None, faint_opacity: 255, min_contrast: 1.0,
        padding_color: WindowPaddingColor::Background,   // confirm variant
        custom_shaders: RepeatablePath, bg_image: None, bg_image_opacity: 1.0,
        bg_image_position: BackgroundImagePosition::default(),   // confirm
        bg_image_fit: BackgroundImageFit::default(), bg_image_repeat: false,
        // VERIFIED: `link::Set::empty()` does NOT exist (link.rs only has
        // `from_config`/`deinit`). `Set { links: Vec<Link> }` has a PUB `links`
        // field, so construct the empty set directly (Global-allocated):
        links: link::Set { links: Vec::new() },
        vsync: false, colorspace: WindowColorspace::default(),
        blending: AlphaBlending::default(), background_blur: BackgroundBlur::default(),
        scroll_to_bottom_on_output: false,
    }
}
```

The default color values mirror the reference config defaults (Config.zig:1116,
1117, 1131, 1132 for the four search colors; `cursor-color` default is null but
we set red so `add_cursor` renders a visible color for the golden). Chunk 4
promotes this fixture (or its constants) so the golden's cursor is a known red.

**GOLDEN-GATE PROOF (item 2 justification).** Chunk 4 renders a cursor; that path
runs `rebuild_cells`' cursor block, which reads `state.colors.cursor` then
`self.config.cursor_color` and destructures it (ref :2477–2510) into an `RGB`.
With `cursor_color = Some(TerminalColor::Color(Color{0xFF,0,0}))` the golden's
cursor is deterministically red — impossible with the unit-struct placeholder.
Hence the real enum is a hard requirement of the golden, satisfied in 3b.

---

## F. DEAD-CODE STRATEGY (`-D warnings`, item 4)

In a non-test build the fill methods have no live caller (their real caller,
`updateFrame`, is deferred to 3c), so rustc's `dead_code` lint would fire. Follow
the 3a-sanctioned **"widen to `pub`"** route (the `SwapChain`/`FrameState`/
`HighlightTag` precedent — pub items reachable from the crate root are dead-code
exempt) for the two *entry points*, and rely on reachability for the rest:

- `rebuild_cells` → **`pub`**. Entry point (3b test + future `updateFrame`).
- `rebuild_overlay` → **`pub`**. Its only non-test caller is `updateFrame` (3c),
  so it must be pub to be exempt.
- `rebuild_row`, `add_underline`, `add_overline`, `add_strikethrough`,
  `add_glyph`, `add_cursor`, `add_preedit_cell` → **private**. Each is called
  transitively from the reachable `pub rebuild_cells` (rustc marks a private fn
  used by a reachable fn as live), so none is dead. `never_extend_bg` (row.rs) is
  `pub` (crate module fn) and called from `rebuild_row`.

Document the two `pub` widenings with the same PORT NOTE form 3a used ("widened to
pub for dead-code exemption; reachable from crate root; reverts to reference-
private semantics when `updateFrame` lands in 3c" — but leave them pub, reverting
is churn). This avoids any `#[allow(dead_code)]`.

The new `TerminalColor`/`Color` are already reachable (named by `DerivedConfig`
public fields + `Message::ChangeConfig`), so no dead-code concern.

---

## G. THE 3b UNIT TEST (item 4)

One `#[cfg(test)] mod` in `generic.rs`, macOS-gated with the whole module. It
proves the fill end-to-end with **no draw**.

### G.1 Offscreen `Renderer` fixture

`Renderer::init` is deferred (needs a live surface), so the test assembles a
`Renderer` field-by-field via a `#[cfg(test)] fn test_renderer(cols, rows) ->
Option<Renderer>` (returns `None` — skip — if no Metal device, the `metal.rs`
precedent). It is the Chunk-4 golden harness **minus** the `drawFrame`:

1. `let device = MTLCreateSystemDefaultDevice()?;` → else `eprintln!("skip");
   return None`.
2. `let api = Metal::init_offscreen(blending, width_px, height_px)?;`
   (`metal/metal_impl.rs:206`).
3. `let shaders = api.init_shaders(&[])?;` (`metal/metal_impl.rs:565`; no custom
   shaders).
4. `let swap_chain = SwapChain::init(&api, /*custom_shaders=*/false)?;`
   (generic.rs 3a).
5. Real font grid: build a `SharedGrid` over the embedded `rustty_font::embedded::
   REGULAR` (the `shared_grid.rs::test_grid` recipe — `CodepointResolver` +
   `SharedGrid::init`). Keep it `Box`ed/`'static`-ish and take
   `font_grid: *mut SharedGrid` (leak it in-test or own it beside the Renderer so
   the raw ptr stays valid for the test's scope).
6. `let font_shaper = Shaper::init(shape::Options::default())?;` (coretext).
7. Construct the `Renderer { … }` literal: `config: derived_config_for_test()`,
   `cells: Contents::default()` then `cells.resize(GridSize{rows,columns})`,
   `uniforms`: any valid `Uniforms` (projection/screen/cell/grid all real-or-zero;
   the fill only writes grid_size/padding_extend/cursor_pos/cursor_color/bg_color),
   `terminal_state: RenderState::empty()`, `overlay: None`, `focused: true`,
   `grid_metrics: (*font_grid).metrics` (read under the grid lock), `size: Size{…}`,
   plus the GPU fields from steps 2–4 (`api`, `swap_chain`, `shaders`,
   `bg_image_buffer`), and the remaining defaults (`images: image::State::empty()`,
   `health`, `display_link: None`, `search_*: None`, counters `0`, `scrollbar:
   Scrollbar::ZERO`, etc.).

### G.2 The assertions

```rust
#[test]
fn rebuild_cells_fills_bg_fg_cursor() {
    let Some(mut r) = test_renderer(10, 3) else { return }; // skip w/o Metal
    // small terminal: print "Ab" on row 0, leave a visible block cursor.
    let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
    { let mut s = t.vt_stream(); s.next_slice(b"Ab"); s.deinit(); }
    r.terminal_state.update(Global, &mut t).unwrap();
    let cs = cursor::style(&r.terminal_state, cursor::StyleOptions{
        preedit:false, focused:true, blink_visible:true });
    r.rebuild_cells(None, cs, &CellSet::new());

    // (1) BG: every visible cell has a bg entry (default-bg cells → alpha 0).
    //     Assert bg_cells.len()==30 and a spot check on a printed cell.
    assert_eq!(*r.cells.bg_cell(0, 0), /* expected [r,g,b,alpha] */ );
    // (2) FG glyph: row 0 fg list (index y+1 == 1) is non-empty (glyphs for A,b).
    assert!(!r.cells.fg_rows.lists[1].is_empty());
    let a = r.cells.fg_rows.lists[1][0];
    assert_eq!(a.grid_pos, [0, 0]);
    assert_eq!(a.atlas, Atlas::Grayscale);
    assert!(a.glyph_size[0] > 0 && a.glyph_size[1] > 0);
    // (3) CURSOR: a block cursor glyph is present (Contents cursor list 0),
    //     colored the fixture's red (config.cursor_color).
    let cur = r.cells.get_cursor_glyph().expect("cursor present");
    assert!(cur.bools.is_cursor_glyph);
    assert_eq!(&cur.color[0..3], &[0xFF, 0, 0]);
    // After printing the 2 cells "Ab", the block cursor sits at column x=2 (the
    // narrow empty cell), so BOTH the cursor uniform and the cursor glyph grid_pos
    // are [2, 0] — NOT [0, 0]. (The block-cursor `cursor_pos` = [cursor_vp.x=2,
    // cursor_vp.y=0]; ref :2529–2536. The `[0,0]` in an earlier draft was wrong.)
    assert_eq!(cur.grid_pos, [2, 0]);
    assert_eq!(r.uniforms.cursor_pos, [2, /*cursor y*/ 0]);

    t.deinit(Global);
}
```

Plus a **second** test for `rebuild_overlay`:

```rust
#[test]
fn rebuild_overlay_builds_and_clears() {
    let Some(mut r) = test_renderer(10, 3) else { return };
    // empty features → overlay stays None
    r.rebuild_overlay(&[]).unwrap();
    assert!(r.overlay.is_none());
    // a real feature → overlay is built at the terminal size
    r.rebuild_overlay(&[crate::overlay::Feature::SemanticPrompts]).unwrap();
    assert!(r.overlay.is_some());
    // and clearing removes it
    r.rebuild_overlay(&[]).unwrap();
    assert!(r.overlay.is_none());
}
```

These two tests, with the workspace compiling under `-D warnings`, ARE the 3b
green gate: `Contents` filled from a constructed `Terminal`/`RenderState`, cursor
color proven, overlay built — no `drawFrame`, no golden image, no GPU output.

### G.3 `opts` helper

Reuse the exact `opts(cols, rows)` helper already in `cell.rs`/`cursor.rs` tests
(same `Options` literal with the kitty limits). Copy it into the `generic.rs` test
module (the established test-helper duplication precedent).

---

## H. OPEN RISKS

1. **[HIGH — reclassified LOGICAL-only by adversary pass 2] The shaper-cell raw
   pointer §C.3(3).** The `Option<*const [shape::Cell]>` held across
   `add_glyph`/`put`/`next` is a *logical-lifetime* argument, NOT a `&mut *self`
   Stacked-Borrows hazard: `shape`/`get` return slices into **separate heap
   allocations** (the shaper `cell_buf` ArrayList / the cache Box), which a
   `&mut *self` reborrow does not retag, so the raw ptr survives it. The adversary
   MUST still certify the logical window: (a) `put` clones (does not free) the
   shaper buffer; (b) the pointer is only read within one run's glyph loop, before
   the next `shape`; (c) `next` nulls it first. If that certification fails, fall
   back to the sanctioned per-run `Vec<shape::Cell>` copy (behavior-identical Copy
   POD) — do NOT choose the copy silently; record the decision. (Note: the copy
   fixes only THIS leg; the inline `state`/hook SB hazard in risk #2 is separate.)

2. **[HIGH — the primary UB hazard, per adversary pass 2] Inline-`self` aliases
   live-crossed by `&mut *self` §C.3(1,2).** The real Stacked-Borrows UB is NOT the
   heap legs — it is (a) any read THROUGH the `state` pointer of
   `terminal_state.{rows,cols,dirty,colors,cursor}` re-dereferenced AFTER a
   `self.rebuild_row(&mut *self)`/`add_cursor`/`add_preedit_cell` call (the
   cursor/preedit blocks, ref :2457–2588), and (b) the `RunIteratorHook`'s
   `*mut Shaper` → **inline** `self.font_shaper`, interleaved with
   `self.font_shaper.shape()` and `self.add_glyph(&mut *self)` across
   `run_iter.next()`. Both point into the inline `Renderer` allocation, so the
   `&mut *self` reborrow pops their tag and the later deref is use-after-pop UB. It
   COMPILES (the `unsafe{&*state}` lifetime is unbounded) but is UB — 'disjoint
   fields' + 'never writes' are necessary but INSUFFICIENT. REMEDY (mandatory,
   §C.3): Copy-hoist inline scalars to locals before the row loop / before any
   `&mut self` sub-call (leg a), AND drive the shape loop off a raw `*mut Renderer`
   so no `&mut *self` is materialized (leg b). Replace the old 'disjoint fields'
   `// SAFETY` comments with the real invariant ('no inline-self alias is
   live-crossed by a `&mut *self` reborrow'). Register EVERY raw-ptr site in
   `MIRI_EXCEPTIONS.md`; MIRI cannot run the Metal-gated 3b test, so these are
   **adversary-only** and the hand review is the sole backstop. Any future edit
   that reintroduces a `state`/hook deref across a `&mut *self` call re-opens the UB.

3. **[MED] Offscreen fixture depends on Chunk-2 + font infra.** The 3b test needs
   `Metal::init_offscreen`, `Metal::init_shaders`, `SwapChain::init`,
   `Shaper::init`, and a `SharedGrid` over `embedded::REGULAR` — all verified
   present, but the fixture wiring is nontrivial (grid lifetime as `*mut`, the
   `Shaders`/`Metal` field construction). If any offscreen ctor turns out to need
   a live surface, the fill methods land compile-checked only and the behavioral
   assertions move to Chunk-4's golden. Confirm `init_shaders(&[])` succeeds
   offscreen early.

4. **[RESOLVED — VERIFIED] `page::Row` semantic-prompt accessor + `page::Wide`
   variants.** `page::Row::semantic_prompt(self) -> SemanticPrompt` is a packed
   getter (page.rs:931), `SemanticPrompt::{None=0, Prompt=1, PromptContinuation=2}`
   (matches row.zig's `.prompt, .prompt_continuation => return true, .none => {}`).
   `page::Cell::wide(self) -> Wide` with `Wide::{Narrow, Wide, SpacerTail,
   SpacerHead}` (page.rs:372). No open action.

5. **[RESOLVED — VERIFIED] `RebuildRowError` shape.**
   `RunIterator::next<A>(&mut self, alloc) -> Result<Option<TextRun>,
   collection::Error>` (run.rs) and `Shaper::shape(&mut self, run) ->
   Result<&[shape::Cell], Error>` where `Error` is the active (coretext on macOS)
   shaper's own `enum Error` (coretext.rs:93). So `enum RebuildRowError {
   Collection(collection::Error), Shape(<coretext>::Error) }` + `From` glue;
   `rebuild_cells` catches → `clear(y)`. (`add_*` return `shared_grid::RenderError`,
   shared_grid.rs:108.)

6. **[LOW — mostly VERIFIED] `DerivedConfig` fixture field ctors.** VERIFIED to
   exist: `StyleStatus::init_fill(true)` (codepoint_resolver.rs:540),
   `WindowPaddingColor::Background` (render-types lib.rs:114),
   `image::State::empty()` (image.rs:97), `Scrollbar::ZERO` (pagelist/grow.rs:703),
   `Shaper::init(shape::Options::default())` (coretext.rs:319, no `alloc` arg),
   `Metal::init_offscreen` (metal_impl.rs:206), `Metal::init_shaders(&[])`
   (metal_impl.rs:565, arg `&[&CStr]`). CORRECTED: `link::Set::empty()` does NOT
   exist → `link::Set { links: Vec::new() }` (§E.2). CORRECTED (adversary pass 2):
   `ArenaAllocator::init` takes **NO** argument — `init()` returns
   `ArenaAllocator<Global>` (arena_allocator.rs:119); `ArenaAllocator::init(Global)`
   does NOT compile (§E.2). Still confirm at impl: the render-types `*::default()` ctors
   (`BackgroundImagePosition/Fit`, `AlphaBlending`, `WindowColorspace`,
   `BackgroundBlur`, `FontShapingBreak`).

7. **[RESOLVED — VERIFIED, adversary pass 2 path-corrected] `render_glyph`/
   `render_codepoint` `style` arg.** The font style is
   `rustty_font_shaper::Style::Regular` (lib.rs:121, `Regular = 0`; reference
   `.regular`) — the top-level re-export, which is exactly `render_codepoint`'s
   in-scope `Style` (`use crate::{Presentation, Style};`, shared_grid.rs:45). There
   is **NO** `rustty_font_shaper::codepoint_resolver::Style` — that path (named in an
   earlier draft) does not exist and would not compile. `render_codepoint(alloc,
   cp:u32, style: Style, p: Option<Presentation>, opts: RenderOptions) ->
   Result<Option<Render>, RenderError>` (shared_grid.rs:396). Sketches in §D.6/§D.7
   corrected to `rustty_font_shaper::Style::Regular` + `Some(Presentation::Text)`.

8. **[LOW] Labeled-block control flow.** The cursor block (ref :2446–2578), the
   preedit-skip catch-up (ref :2683–2743), and the overlay reuse-vs-reinit (ref
   :2254–2286) are Zig labeled blocks with `break :label value`. Port each with an
   explicit early-return/`if let`/small closure that preserves the exact
   short-circuit order — the adversary should diff control flow, not just values.

### H.1 Adversary-validation changelog (this pass)

All six load-bearing claims (§A–§F) were re-verified against
`ghostty/src/renderer/{generic.zig,row.zig}` + the ported
`rustty-{renderer,vt,font,font-shaper,render-types}` crates. Results:

- **(a) Scope** — HOLDS. `updateFrame` (:1123–1424) calls the 3c uniforms leaf
  `updateCustomShaderUniformsFromState` at :1418 and is the *sole* caller of
  `rebuildCells` (:1364) + `rebuildOverlay` (:1349); it locks `state.mutex` (:1174).
  Deferring it to 3c is correct; the 3b subtree's every dep is ported.
- **(b) Raw-pointer triad** — HOLDS, and *strengthened*: `Shaper::shape` and
  `Cache::get` each return a slice **borrowing a `self` field**, so the raw pointer
  is mandatory (not merely faithful). `Cache::put` copies (no free), and the
  `dirty=false` write-back precedes `rebuildRow` (:2425 before :2427). All crossed
  values (`page::Cell`/`Style`/`RGB`/`shape::Cell`) are `Copy`. `RunIteratorHook {
  shaper: *mut Shaper }` confirmed raw-ptr-based.
- **(c) RenderState-walk API** — HOLDS. Every cited `render`/`page`/`style`/`color`/
  `sgr` + font/shaper symbol exists with the cited shape (see the inline VERIFIED
  tags). Fixes: `Style::regular`→`Style::Regular`; `CellSet::contains` takes
  `&Coordinate`; `Cache::put`/`get` take `run: &TextRun` and `put` drops `alloc`.
- **(d) TerminalColor cycle-safety** — HOLDS. `renderer→vt::color::RGB` already
  exists; hoisting would cycle. Config anchors exact: `Color` 5413,
  `Color.toTerminalRGB` 5430 (→`RGB`), `TerminalColor` 5549,
  `TerminalColor.toTerminalRGB` 5561 (→`?RGB`); search defaults at 1116/1117/1131/
  1132 with the exact byte values the §E.2 fixture uses. Golden proof (cursor block
  :2477–2510) confirmed.
- **(e) Dead-code** — HOLDS. `rebuildCells`/`rebuildOverlay` are `fn` (private) in
  the reference with `updateFrame` (deferred) their only caller → widen both to
  `pub`; private helpers are reachable transitively (live).
- **(f) Fixtures** — HOLDS except `link::Set::empty()` (does not exist →
  `link::Set { links: Vec::new() }`). `DerivedConfig` field names/types match
  generic.rs; every other cited ctor verified present (see risk #6).

Concrete corrections folded in: `link::Set::empty()` → direct ctor (§E.2);
`Style::regular` → `Style::Regular` (§D.6/§D.7); `CellSet::contains(&Coordinate)`
(§C.1); `add_glyph`'s `cell_raws` demoted from raw ptr to safe `&[page::Cell]`
since the AoS projection is a row-local `Vec` (§D.5); the `cursor_style` SGR-at-
cursor local made explicit (§D.2); `DerivedConfig` fields line-range 536→**572**
(appendix); risks #4/#5/#7 retired, #6 narrowed. No decision was reversed — the v1
structure and all §0 decisions survived validation.

### H.2 Adversary-validation changelog (pass 2 — verdict SOUND-WITH-FIXES)

A second adversary pass reviewed the committed design. Verdict SOUND-WITH-FIXES: no
§0 decision reversed, no method retabled, but the §C.3 soundness rationale was
incomplete and two sketches carried defects. All eight findings verified against
`ghostty/src/renderer/generic.zig` + the ported crates, and folded HERE:

- **[BLOCKING] §C.3 inline-alias Stacked-Borrows UB.** The v1 'disjoint fields /
  never writes / Copy-out' rationale was necessary but INSUFFICIENT: the
  invalidation vector is the nested `&mut *self` reborrow that each
  `rebuild_row`/`add_cursor`/`add_preedit_cell`/`add_glyph` call creates, which
  pops any alias into inline `Renderer` memory. §C.3 rewritten to split aliases by
  ALLOCATION — HEAP-backed (row_data/shaper/cache buffers, `cells_raw`,
  `run_iter_opts.cells`) survive `&mut *self`; INLINE-`self`
  (`terminal_state.{rows,cols,dirty,colors,cursor}` via `state`, the hook's
  `*mut Shaper` → `self.font_shaper`) do NOT. Remedy: Copy-hoist inline scalars
  before the row loop / any `&mut self` sub-call AND/OR drive the fill off a raw
  `*mut Renderer`. SAFETY comments rewritten; §D.2 sketch updated to hoist; §0 item
  5 + risks #1/#2 rebalanced (§C.3(3) is logical-only, the inline legs are the real
  UB). MIRI_EXCEPTIONS.md registration mandated; adversary-only (Metal-gated test,
  no Miri). VERIFIED against the reference: the cursor/preedit blocks (ref
  :2457–2588) genuinely re-read `state.{colors,cursor}` AFTER the row loop's
  `self.rebuildRow(…)` calls, and `run_iter`/`shape`/`addGlyph` genuinely interleave
  the hook (ref :2673/:2702/:3020) — the reference does NOT contradict the fix.
- **[BLOCKING] §D.6 `add_cursor` arg.** Reference is `cursor_state: *const
  RenderState.Cursor` (ref :3226); v1 sketch used a safe `&render::Cursor`, which
  aliases inline `self.terminal_state.cursor` and is the same hazard. Changed to
  `*const render::Cursor`, with the two needed scalars (`viewport`, `cell.wide`)
  Copy-dereffed at the top before any `self.cells`/`self.font_grid` access.
- **[fix] §E.2 fixture** — `ArenaAllocator::init(Global)` → `ArenaAllocator::init()`
  (takes NO arg; returns `ArenaAllocator<Global>`, arena_allocator.rs:119). Verified.
- **[fix] §G.2 test** — `uniforms.cursor_pos` corrected `[0,0]` → `[2,0]` (after
  printing "Ab" the block cursor sits at x=2, ref :2529–2536); cursor-glyph
  `grid_pos == [2,0]` assertion added. Verified.
- **[fix] R1 comment fidelity** — new §D.9 enumerates the must-port commented-out
  reference blocks (rebuildOverlay timing :2230–2238, rebuildCells timing :2315–2321
  + trailing debug :2604–2607, the per-run 'valid until next shaping call' comment
  :2731–2734 / :3000–3003). All verified present in the reference.
- **[fix] `@enumFromInt(hl.tag)` (ref :2777)** — §D.3 now mandates an explicit
  `u8 -> HighlightTag` map (`0=>SearchMatch`, `1=>SearchMatchSelected`) with
  out-of-range **panic** (matching Zig's checked `@enumFromInt`), since
  `HighlightTag` (generic.rs:458) has no `From`/`TryFrom`. Verified.
- **[fix] Font-style path** — `codepoint_resolver::Style::Regular` (does NOT exist)
  → `rustty_font_shaper::Style::Regular` (lib.rs:121; the in-scope `Style` of
  `render_codepoint`, shared_grid.rs:45). Corrected in §D.6/§D.7 + risk #7. Verified
  (`codepoint_resolver.rs` has no `Style`).

No adversary finding was REJECTED — all eight are grounded in the reference and the
ported crates.

---

## Appendix — reference anchors

`src/renderer/generic.zig`: `updateFontGridUniforms` 1115; `updateFrame`
1123–1424 (calls the 3c uniforms leaf `updateCustomShaderUniformsFromState` at
:1418, then `font_shaper.endFrame()` at :1423 — VERIFIED); `DerivedConfig` fields
536–572, `init` 574–649, `deinit` 651–655;
`rebuildOverlay` 2226–2292; `PreeditRange` 2294–2298; `rebuildCells` 2307–2608;
`rebuildRow` 2610–3056; `addUnderline` 3059; `addOverline` 3099;
`addStrikethrough` 3131; `addGlyph` 3161; `addCursor` 3224; `addPreeditCell`
3313; `syncAtlasTexture` 3358. `src/renderer/row.zig`: `neverExtendBg` 8–63.
`src/config/Config.zig`: `Color` 5413, `Color.toTerminalRGB` 5430, `TerminalColor`
5549, search-color defaults 1116/1117/1131/1132.
