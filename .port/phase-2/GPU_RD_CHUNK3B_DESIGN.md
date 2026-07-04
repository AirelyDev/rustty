# GPU R-d — Chunk 3b DESIGN: the CPU cell-fill

Status: ARCHITECT design. Design-only; touches no `crates/`. Implementation lands
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
   `Renderer` fields, plus a shaper-owned cell buffer held across `&mut self`
   calls. Faithful + sound Rust requires a **raw-pointer triad** (§C.3), per the
   memory `rust-mut-args-strongly-protected`. This is the single largest unsafe
   surface in the chunk and the primary adversary target.

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
invisible,bold}` fields. `render::CellSet::contains(point::Coordinate)`.

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
`self.font_shaper`, `self.font_shaper_cache`). Rust's borrow checker rejects this
even though the fields are disjoint and no write targets the aliased memory. The
faithful + sound port uses three raw-pointer aliases. None of these is a
performance choice — each is the *only* way to keep the reference's method
shapes without restructuring (which R1 forbids).

**(1) The `terminal_state` alias.** Reference `const state: *RenderState =
&self.terminal_state;` held across `self.cells.*`, `self.rebuild_row`,
`self.add_cursor`, `self.add_preedit_cell`. Port as a raw const pointer captured
once and dereferenced per read:

```rust
let state: *const RenderState = &self.terminal_state;
// SAFETY: `self.terminal_state` is never written through `state` and is a field
// disjoint from every `self` field mutated below (`cells`/`uniforms`/
// `font_shaper`/`font_shaper_cache`); the per-row `dirty=false` write-back
// happens through a separate `&mut self.terminal_state` *before* any read here.
let st = unsafe { &*state };
```

Sound because: (a) the fill never mutates `terminal_state` through `state`
(read-only view); (b) the mutated fields are disjoint; (c) `page::Cell`/`Style`/
`RGB` are all `Copy`, so every value crosses out of the `unsafe { &*state }`
region by copy, not by borrow.

**(2) `rebuild_row`'s pointer args.** Reference passes pointers *into*
`self.terminal_state.row_data[y]`:
`fn rebuildRow(self, y, row: page.Row, cells: *MultiArrayList(Cell),
preedit_range, selection, highlights: *const ArrayList(Highlight), links)`. Port
`cells` and `highlights` as **raw const pointers** (mirroring the reference `*` /
`*const`) so no tracked borrow of `self.terminal_state` is held across the
`&mut self` body:

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

`run_iter_opts.cells` (the `RunOptions.cells: &'a [render::Cell]`) is built from
`unsafe { &*cells }`; the resulting `RunIterator` holds only that synthetic-`'a`
slice + a raw `*mut Shaper` (its hook), **not** a `&mut self.font_shaper` borrow
— so `self.font_shaper.shape(run)` and `self.add_glyph(&mut self, …)` may be
called while `run_iter` is alive.

**(3) The shaper cell buffer.** Reference `shaper_cells: ?[]const
font.shape.Cell` aliases either the shaper-owned buffer (from `self.font_shaper.
shape(run)`, "valid until the next shaping call" per ref :2731–2734) or the
cache-owned buffer (from `self.font_shaper_cache.get(run)`), and is held across
`self.add_glyph`, `self.font_shaper_cache.put`, and `run_iter.next` — all
`&mut self` or `&mut`-field ops. A safe `&[Cell]` cannot coexist with those.
Port as a raw slice pointer:

```rust
let mut shaper_cells: Option<*const [shape::Cell]> = None;
// captured right after `shape`/`get`; read via `unsafe { &*p }` in the glyph loop
```

Sound because `shape::Cell` is `Copy` POD (`x,x_offset,y_offset,glyph_index`);
the pointer is captured immediately after `shape`/`get` and consumed within the
same run's glyph loop; `put(run, new_cells)` clones into the cache (does not free
the shaper buffer); `run_iter.next` resets `shaper_cells = None` before the next
`shape`. The reference's documented "valid until next shaping call" contract maps
exactly onto this window. This is the subtlest lifetime argument in the chunk and
the #1 thing the adversary must certify.

> Rejected alternative for (3): copy each run's shaped cells into a local
> `Vec<shape::Cell>`. It is behavior-identical (Copy POD) and removes the unsafe,
> but it deviates from the reference's zero-copy shaper-buffer contract and the
> project's raw-ptr-faithful doctrine. If the adversary cannot certify the raw
> pointer's lifetime window, the `Vec` copy is the sanctioned fallback — flag in
> OPEN RISKS, do not silently choose it.

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
    // let state: *const RenderState = &self.terminal_state;  (raw alias — §C.3(1))
    // grid_size_diff → self.cells.resize + self.uniforms.grid_size
    // rebuild = dirty==Full || grid_size_diff; if rebuild { self.cells.reset();
    //   match self.config.padding_color { Background=>{}, Extend|ExtendAlways=>{
    //     self.uniforms.padding_extend = PaddingExtend{up,down,left,right:true} } } }
    // preedit_range := via preedit.range(cursor_vp.x, cols-1)  (state::Preedit::range)
    // for y in 0..row_len { if !rebuild { if !dirty {continue}; self.cells.clear(y) }
    //   dirty = false (write-back BEFORE rebuild_row — §C.3);
    //   self.rebuild_row(y, row, &cells[..] as *const _, preedit_range,
    //                    selection, &highlights[..] as *const _, links)
    //     .unwrap_or_else(|e| { log::warn!(...); self.cells.clear(y) }); }
    // cursor block (ref :2446–2578): self.cells.set_cursor(None,None);
    //   uniforms.cursor_pos = [u16::MAX; 2]; ... compute cursor_color via
    //   state.colors.cursor / self.config.cursor_color destructure (§E);
    //   self.add_cursor(&st.cursor, style, cursor_color);
    //   if style==Block { uniforms.cursor_pos/bools.cursor_wide/cursor_color via
    //     self.config.cursor_text destructure (§E). }
    // preedit block (ref :2580–2599): for cp in preedit.codepoints[cp_offset..] {
    //   self.add_preedit_cell(cp, Coordinate{x,y:range.y}, st.colors.foreground)
    //     .unwrap_or_else(|e| log::warn!(...)); x += if cp.wide {2} else {1}; }
    // self.cells_rebuilt = true;
}
```

### D.3 `rebuild_row` (private; signature in §C.3(2))

Returns `Result<(), RebuildRowError>`. Body maps ref :2620–3055 line-for-line:
padding `.extend` heuristic via `never_extend_bg(row, unsafe{&*cells}, palette,
bg)` (ref :2634–2656); `RunOptions{ grid: self.font_grid, cells: unsafe{&*cells},
selection, cursor_x }` + `apply_break_config(self.config.font_shaping_break)`
(ref :2658–2673); the preedit-skip run-catch-up loop (ref :2683–2743); the
`selected` precedence enum (ref :2752–2786); `bg`/`fg` selection/search
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
    cell_raws: *const [page::Cell],       // raw — projected from row cells (§C.3)
    shaper_cell: shape::Cell,
    shaper_run: shape::TextRun,
    color: color::RGB, alpha: u8,
) -> Result<(), RenderError> {
    // let cell = unsafe{&*cell_raws}[x]; let cp = cell.codepoint();
    // constraint = get_constraint(cp).unwrap_or_else(|| if cell::is_symbol(cp)
    //   Constraint{ size: ConstraintSize::Fit, ..Constraint::none } else
    //   Constraint::none);
    // render = grid.render_glyph(self.alloc, shaper_run.font_index,
    //   shaper_cell.glyph_index, RenderOptions{ grid_metrics: self.grid_metrics,
    //   thicken: self.config.font_thicken, thicken_strength:
    //   self.config.font_thicken_strength, cell_width: Some(cell.grid_width()),
    //   constraint, constraint_width: cell::constraint_width(unsafe{&*cell_raws},
    //   x, cols) })?;
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
> `rebuild_row` supplies it as a raw ptr to a `page::Cell` slice it builds once
> per row (`row_cells.iter().map(|c| c.raw).collect::<Vec<_>>()`), matching the
> reference's SoA `cells_raw`. `get_constraint` is
> `rustty_font::nerd_font_attributes::get_constraint`. Bearings add
> `shaper_cell.{x,y}_offset` (i16) to `g.offset_{x,y}` (i32) — widen then narrow,
> matching the reference `@intCast(offset + shaper_offset)`.

### D.6 `add_cursor` (returns `()`)

```rust
// fn addCursor(self, cursor_state: *const RenderState.Cursor,
//              cursor_style: renderer.CursorStyle, cursor_color: RGB) void
fn add_cursor(
    &mut self,
    cursor_state: &render::Cursor,
    cursor_style: cursor::Style,
    cursor_color: color::RGB,
) {
    // cursor_vp = match cursor_state.viewport { Some(v)=>v, None=>return };
    // (wide, x) = if !cursor_vp.wide_tail { (cursor_state.cell.wide()==Wide::Wide,
    //   cursor_vp.x) } else { (true, cursor_vp.x - 1) };
    // alpha = if !self.focused { 255 } else { (255.0*self.config.cursor_opacity)
    //   .ceil() as u8 };
    // render = match cursor_style { Block=>CursorRect, BlockHollow=>CursorHollowRect,
    //   Bar=>CursorBar, Underline=>CursorUnderline (via grid.render_glyph(SPRITE_INDEX,
    //   sprite, cell_width: if wide {2} else {1})  .. catch log+return),
    //   Lock=> grid.render_codepoint(0xF023, Style::regular?, Presentation::Text,
    //   ..) catch log+return; None-inner => log+return };
    // self.cells.set_cursor(Some(CellText{ atlas: Grayscale, bools:
    //   CellTextBools{ is_cursor_glyph:true, ..}, grid_pos:[x, cursor_vp.y],
    //   color:[..,alpha], glyph_pos/size/bearings }), Some(cursor_style));
}
```

> HAND-CHECK: `renderer.CursorStyle` is rustty `cursor::Style` (Block/BlockHollow/
> Bar/Underline/Lock). The reference `sprite` switch has `.lock => unreachable`
> inside the rect arm and a separate `.lock` arm using `renderCodepoint(0xF023)`.
> `render_codepoint`'s `style` arg — the reference passes `.regular` (a
> `font.Style`); confirm the rustty `Style` (font `codepoint_resolver::Style`)
> `.Regular` variant, and `p: Some(Presentation::Text)` for the text presentation
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
    // render = grid.render_codepoint(self.alloc, cp.codepoint, Style::regular?,
    //   Presentation::Text, RenderOptions{ grid_metrics: self.grid_metrics,
    //   ..default }) catch log+return; orelse log+return;
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
        arena: ArenaAllocator::init(Global),
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
        links: link::Set::empty(),    // confirm ctor; Global-allocated
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
    assert_eq!(r.uniforms.cursor_pos, [0, /*cursor y*/ 0]);

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

1. **[HIGH] The shaper-cell raw pointer §C.3(3).** The `Option<*const
   [shape::Cell]>` held across `add_glyph`/`put`/`next` is the subtlest lifetime
   argument in the chunk, on a crate with **no** comment-coverage gate. The
   adversary MUST certify: (a) `put` clones (does not free) the shaper buffer;
   (b) the pointer is only read within one run's glyph loop, before the next
   `shape`; (c) `next` nulls it first. If certification fails, fall back to the
   sanctioned per-run `Vec<shape::Cell>` copy (behavior-identical Copy POD) — do
   NOT choose the copy silently; record the decision.

2. **[HIGH] The `terminal_state` + `rebuild_row` pointer args §C.3(1,2).** Three
   `unsafe { &* }` sites reading `self.terminal_state`. Sound only because the
   fill never writes `terminal_state` through them and the per-row `dirty=false`
   write-back precedes `rebuild_row`. Any future edit that writes `terminal_state`
   inside the fill breaks this — leave a `// SAFETY:` invariant comment at each
   site. MIRI cannot run these tests (Metal), so this is adversary-only — flag for
   `MIRI_EXCEPTIONS.md` if the raw-ptr sites are added to any miri-run crate later
   (they are not, here).

3. **[MED] Offscreen fixture depends on Chunk-2 + font infra.** The 3b test needs
   `Metal::init_offscreen`, `Metal::init_shaders`, `SwapChain::init`,
   `Shaper::init`, and a `SharedGrid` over `embedded::REGULAR` — all verified
   present, but the fixture wiring is nontrivial (grid lifetime as `*mut`, the
   `Shaders`/`Metal` field construction). If any offscreen ctor turns out to need
   a live surface, the fill methods land compile-checked only and the behavioral
   assertions move to Chunk-4's golden. Confirm `init_shaders(&[])` succeeds
   offscreen early.

4. **[MED] `page::Row` semantic-prompt accessor + `page::Wide` variants.**
   `never_extend_bg` reads `row.semantic_prompt` (a packed getter `()` in rustty
   vs a field in Zig) and `add_cursor`/`rebuild_row` read `cell.wide()` →
   `page::Wide`. Confirm the exact getter names and the `Wide` variant spellings
   (`Narrow/Wide/SpacerHead/SpacerTail`) against page.rs during impl.

5. **[MED] `RebuildRowError` shape.** `rebuild_row` aggregates `collection::Error`
   (from `RunIterator::next`) and `shaper::Error` (coretext `shape`). Confirm both
   enum paths and add `From` glue. `rebuild_cells` catches → `clear(y)`.

6. **[LOW] `DerivedConfig` fixture field ctors.** `StyleStatus::init_fill(true)`,
   `WindowPaddingColor::Background`, `BackgroundImagePosition/Fit::default`,
   `link::Set::empty()`, `AlphaBlending/WindowColorspace/BackgroundBlur::default`
   — confirm each exact ctor/variant name in `derived_config_for_test`. Also
   confirm `ArenaAllocator::init(Global)` is the right ctor.

7. **[LOW] `render_glyph`/`render_codepoint` `style` arg.** `add_cursor` (Lock)
   and `add_preedit_cell` pass the reference `.regular` `font.Style`. Confirm the
   rustty `codepoint_resolver::Style::Regular` (or the shaper's `Style`) is the
   right type for `render_codepoint`'s `style: Style` parameter.

8. **[LOW] Labeled-block control flow.** The cursor block (ref :2446–2578), the
   preedit-skip catch-up (ref :2683–2743), and the overlay reuse-vs-reinit (ref
   :2254–2286) are Zig labeled blocks with `break :label value`. Port each with an
   explicit early-return/`if let`/small closure that preserves the exact
   short-circuit order — the adversary should diff control flow, not just values.

---

## Appendix — reference anchors

`src/renderer/generic.zig`: `updateFontGridUniforms` 1115; `updateFrame`
1123–1424; `DerivedConfig` struct 536–566, `init` 574–649, `deinit` 651–655;
`rebuildOverlay` 2226–2292; `PreeditRange` 2294–2298; `rebuildCells` 2307–2608;
`rebuildRow` 2610–3056; `addUnderline` 3059; `addOverline` 3099;
`addStrikethrough` 3131; `addGlyph` 3161; `addCursor` 3224; `addPreeditCell`
3313; `syncAtlasTexture` 3358. `src/renderer/row.zig`: `neverExtendBg` 8–63.
`src/config/Config.zig`: `Color` 5413, `Color.toTerminalRGB` 5430, `TerminalColor`
5549, search-color defaults 1116/1117/1131/1132.
