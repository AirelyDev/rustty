# GPU R-d Chunk 3c — FRAME ORCHESTRATOR + UNIFORMS (design)

**Status:** design only (no `crates/` edits). Architect hand-off for the Chunk-3c
port + adversary/verify. Landing file: `crates/rustty-renderer/src/generic.rs`
(`mod macos_impl`, `impl Renderer`), plus one tiny cross-cutting derive in
`crates/rustty-renderer/src/state.rs` (§B.3) and — if the implementer keeps the
consts local — nothing new in `metal_impl.rs` (the three `custom_shader_*` /
`SWAP_CHAIN_COUNT` consts already exist; §E.5).

**Reference (read-only):** `/Users/gael/projects/ghostty/src/renderer/generic.zig`
(Zig 0.15.2). Verified method line ranges:

| method | ref lines | 3c disposition |
|---|---|---|
| `setFontGrid` | 1067–1110 | **DEFER** (§A.2) |
| `updateFontGridUniforms` | 1112–1120 | **3c-core** |
| `updateFrame` | 1122–1424 | **3c-core** (orchestrator) |
| `drawFrame` | 1426→ | Chunk 4 (GPU submit) |
| `changeConfig` | 1840–1916 | **DEFER** (§A.2) |
| `setScreenSize` | 1918–1933 | **DEFER** (§A.2) |
| `updateScreenSizeUniforms` | 1935–1971 | **3c-core** |
| `updateBgImageBuffer` | 1973–2005 | **3c-core** |
| `updateCustomShaderUniformsFromState` | 2007–2097 | **3c-core** |
| `updateCustomShaderUniformsForFrame` | 2099–2222 | **3c-core** |
| `prepBackgroundImage` / `uploadBackgroundImage` | 1790–1838 | **DEFER** (§A.2) |
| `markDirty` | (called by the deferred trio) | **DEFER** (§A.2) |

R1 (100% fidelity) applies verbatim; **G3b comment-coverage skips
rustty-renderer** (its `lib.rs` says "PORT STATUS: in progress"), so
`generic.rs` R1 comment/branch fidelity is **adversary/verify-only** — every
ported line + comment must be hand-checked against the reference.

---

## A. SCOPE BOUNDARY (decision 1)

### A.1 — 3c-core = 6 methods (frame orchestration + all uniforms math)

```
update_frame                         (updateFrame,                     :1122)
update_font_grid_uniforms            (updateFontGridUniforms,          :1112)
update_screen_size_uniforms          (updateScreenSizeUniforms,        :1935)
update_bg_image_buffer               (updateBgImageBuffer,             :1973)
update_custom_shader_uniforms_from_state  (…FromState,                 :2007)
update_custom_shader_uniforms_for_frame   (…ForFrame,                  :2099)
```

`update_frame` is the star: it is the **sole caller** of the already-ported
`rebuild_cells` (:1364) and `rebuild_overlay` (:1349), and of
`update_custom_shader_uniforms_from_state` (:1418) and `font_shaper.end_frame()`
(:1423). Landing it retires the two "real caller lands in 3c" dead-code
placeholders 3b left behind (GPU_RD_CHUNK3B_DESIGN §F).

The four uniforms helpers are **pure CPU math** on already-ported POD
(`shaders::Uniforms`, `shadertoy::Uniforms`, `size::Size`, `grid_metrics`,
`BgImage`). `…FromState` is reachable through `update_frame`; the other three
have **no 3c-core caller** (their only callers are the deferred `setScreenSize` /
`setFontGrid` / `changeConfig`), so they land **`pub` + directly unit-tested**
(the 3a "widen to `pub` for dead-code exemption" precedent — §F). `…ForFrame` is
nominally the `drawFrame` half ("called exactly once per frame, inside
drawFrame"), but it issues **zero GPU calls** — it is self-contained uniform math
over `self.size` + `self.cells.get_cursor_glyph()` + timing — so it lands cleanly
in 3c and de-risks Chunk 4. This exactly matches the prompt's recommended split.

### A.2 — DEFER (with justification)

| deferred | why it does NOT block `update_frame` |
|---|---|
| `set_screen_size` (:1918) | resize plumbing; `update_frame` never calls it. Calls `update_screen_size_uniforms` (ported) + `log.debug`. Lands with the resize path (Chunk 4/5). |
| `set_font_grid` (:1067) | font-grid swap; needs `ShaperCache` reset + `swap_chain.frames[*].{grayscale,color}_modified = 0` + `markDirty`. `update_frame` never calls it. |
| `change_config` (:1840) | config swap; needs `Shaper::init`, `prep_background_image`, `markDirty`, `api.blending`, `custom_shaders.equal`, `bg_image.equal`. Heavy config surface; `update_frame` never calls it. |
| `prep_background_image` / `upload_background_image` (:1790/1828) | bg-image lifecycle; called only by `change_config`/`Renderer::init`. |
| `mark_dirty` | called only by the deferred trio; **not** by `update_frame`. |
| `DerivedConfig::init<C>` (generic.rs:274) | config-coupled seam (`renderer → config` = G7 cycle). `update_frame` builds `DerivedConfig` from the fixed-color test fixture (`derived_config_for_test`, generic.rs:2944), never via `init`. Stays `unimplemented!()`. |

**Confirmed:** grepping the `update_frame` body (`:1122–1424`) shows it calls
none of `markDirty`, `prepBackgroundImage`, `setScreenSize`, `changeConfig`,
`setFontGrid`, `DerivedConfig.init`, `updateScreenSizeUniforms`,
`updateFontGridUniforms`, `updateBgImageBuffer`, or `…ForFrame`. The deferral is
a clean gate-green cut.

### A.3 — gate-green boundary

Compiles under `-D warnings` (workspace) with `cargo build`/`clippy`; the 6
methods are reachable (`pub` entry points or transitively-reachable), so no
`#[allow(dead_code)]` except the two faithful mirror-of-reference exceptions in
§B.2/§D.6 (`Critical.mouse` write-only field; the arena vestige). Unit-testable
**without `drawFrame`/GPU submit**: build a live `state::State { mutex, &mut
Terminal, … }` + drive a real `RenderState`, call `update_frame`, assert
`Contents` filled + `uniforms.*` + `scrollbar`; call each uniforms helper
directly and assert its `uniforms` writes (§F/§G). macOS/Metal-gated + Metal-skip
exactly like the 3b test.

---

## B. THE LIVE `State` TYPE (decision 2)

### B.1 — it ALREADY EXISTS — no port, no placeholder-fill

The renderer-thread state `updateFrame` takes — Zig `state: *renderer.State`
(`renderer/State.zig`) — is **already fully ported** at
`crates/rustty-renderer/src/state.rs` as `pub struct State` (PORT STATUS: done,
both `test{}` blocks ported):

```rust
// crates/rustty-renderer/src/state.rs
pub struct State {
    pub mutex: *mut rustty_zigstd::thread::Mutex,   // Zig *std.Thread.Mutex
    pub terminal: *mut Terminal,                    // Zig *Terminal
    pub inspector: Option<*mut Inspector>,          // Inspector = uninhabited placeholder enum{}
    pub preedit: Option<Preedit>,
    pub mouse: Mouse,                                // { point: Option<Coordinate>, mods: Mods }
}
```

> **Prompt correction (important):** the brief warned that "the ported
> `state::State` … is Preedit" and that the live `State{mutex,terminal,…}` may
> need porting. That is **not so** — `state.rs` defines the FULL renderer thread
> `State` (mutex + terminal + inspector + preedit + mouse); `Preedit`/`Mouse`/
> `Codepoint`/`PreeditRange` are its *sub-types* in the same module. `state::State`
> **is** `renderer.State`. No porting or placeholder-fill is required for 3c.

`state.mutex.lock()` maps to the guard-less Zig-shaped
`rustty_zigstd::thread::Mutex` (`lock(&self)` / `unlock(&self)` /
`try_lock(&self)`, thread.rs:79/106/93) — the CORE_BLUEPRINT mutex model
(unlock/relock across function boundaries). Interior atomics ⇒ `lock` takes
`&self`, so a **shared** `&State` suffices to lock (see §B.2 signature).

### B.2 — `update_frame`'s `state` parameter shape

```rust
// Zig: pub fn updateFrame(self: *Self, state: *renderer.State, cursor_blink_visible: bool) Allocator.Error!void
pub fn update_frame(
    &mut self,
    state: &state::State,       // Zig `*renderer.State` -> `&state::State`  (see rationale)
    cursor_blink_visible: bool,
) -> Result<(), VtOutOfMemory>  // Zig `Allocator.Error!void`; the ONLY propagated `try` is terminal_state.update
```

> **COMPILE-BLOCKER — `OutOfMemory` name clash (do NOT write bare `OutOfMemory`
> here).** Inside `mod macos_impl` the bare name `OutOfMemory` is already bound to
> **`rustty_font::atlas::OutOfMemory`** (generic.rs:387). But the only propagated
> `?` is `self.terminal_state.update(...)`, whose type is
> **`rustty_vt::pagelist::OutOfMemory`** (`RenderState::update -> Result<(),
> OutOfMemory>`, render.rs:28/300 — `use crate::pagelist::OutOfMemory`). These are
> **two distinct types**, and the only `From` impls are `From<atlas::OutOfMemory>
> for {ReserveError, Error}` (generic.rs:465) — there is **no**
> `From<pagelist::OutOfMemory> for atlas::OutOfMemory`, so the `?` desugar
> `map_err(From::from)` fails to compile (E0277). **Fix:** import the vt type under
> a distinct alias and return *it* so `?` needs no conversion:
> ```rust
> use rustty_vt::pagelist::OutOfMemory as VtOutOfMemory;  // add to mod macos_impl (pub mod pagelist, rustty-vt lib.rs:117)
> // ...
> self.terminal_state.update(self.alloc, unsafe { &mut *t })?;  // pagelist::OutOfMemory == VtOutOfMemory ⇒ no From needed
> ```
> The reference `Allocator.Error!void` is a single OOM; `VtOutOfMemory` is the
> faithful concrete type for that one propagation.

**Why `&state::State` and not `*mut state::State`:** `update_frame` never mutates
a `State` *field* — it only (a) locks `state.mutex` (`lock(&self)`), and (b)
reads the interior raw pointers `state.terminal` / `state.inspector` / values
`state.preedit` / `state.mouse` and mutates **through** the `*mut Terminal`
(a disjoint allocation). All mutation of terminal data flows through the stored
`*mut Terminal`, which carries its own mutability — the exact Zig-`*T`→`&`
convention this crate already uses (`render_cell_map(render_state: &RenderState)`,
`kitty_requires_update(t: &Terminal)`). `&State` also carries no aliasing with
`&mut self` (State is not part of Renderer). This is the cleanest faithful shape.
(If the implementer hits any borrow friction — e.g. a future State field mutation
— fall back to `state: *mut state::State` with `#[allow(clippy::not_unsafe_ptr_arg_deref)]`;
the interior-pointer walk in §C is identical either way. `&State` is the recommendation.)

**Return type:** Zig `Allocator.Error!void`. Every fallible sub-call except one is
`catch`-logged inline (`render_cell_map`, `link_cells`, `rebuild_overlay`,
`update_highlights_flattened`, `rebuild_cells`, `overlay_update`) → caught, not
propagated. The **only** propagated `try` is `self.terminal_state.update(...)`
(render.rs:300 → `Result<(), pagelist::OutOfMemory>`) and the synchronized-output
early `return;`. So `update_frame -> Result<(), VtOutOfMemory>` (the `pagelist`
type aliased — see the COMPILE-BLOCKER box above; **not** the bare
`atlas::OutOfMemory` in scope), propagating that same `OutOfMemory` via `?` (Zig
`Preedit.clone` OOM collapses — rustty `Preedit::clone` is infallible,
state.rs:135).

### B.3 — the one required cross-cutting glue: `Mouse: Copy`

The `Critical` snapshot does `critical.mouse = state.mouse` (:1275). `state::Mouse`
currently derives only `Default` (state.rs:70). Add `#[derive(Clone, Copy, Default)]`
to `Mouse` — a faithful POD glue (Zig `Mouse` is trivially copyable; `Coordinate`
is Copy, `Mods` is Copy+Default, so the derive is sound). This is the sole edit
outside `generic.rs`.

### B.4 — how the 3c test constructs `State`

```rust
let mut mutex = rustty_zigstd::thread::Mutex::new();
let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
{ let mut s = t.vt_stream(); s.next_slice(b"Ab"); s.deinit(); }  // drive some content
let st = state::State {
    mutex: &mut mutex,                 // *mut Mutex
    terminal: &mut t,                  // *mut Terminal
    inspector: None,                   // Inspector uninhabited ⇒ always None (§C.7)
    preedit: None,                     // or Some(state::Preedit{codepoints}) for the preedit test
    mouse: state::Mouse::default(),    // point:None, mods:default
};
let mut r = test_renderer(10, 3)?;     // §F harness (3b's test_renderer, reused)
r.update_frame(&st, /*cursor_blink_visible=*/ true)?;
```

`Inspector` is the uninhabited `enum {}` placeholder (state.rs:27), so
`st.inspector` can only ever be `None` — the overlay-features block (§C.7) is
statically the empty-features path.

---

## C. `update_frame` CRITICAL-SECTION ALIASING (decision 3)

`update_frame` introduces **NO new** "long-lived raw alias INTO a self field,
crossed by `&mut` to that SAME field" hazard (the memory
`shaper-hook-sb-clean-interleave` pattern). The §C.3 shaper-hook remedy is
**inherited, not re-introduced** — `update_frame` calls the already-fixed
`self.rebuild_cells(...)` and does **not** re-drive the shaper run loop. The only
discipline it needs is the faithful **Zig re-walk**: re-deref
`state.terminal.screens.active.pages` from the raw pointers on every use rather
than caching a reference across a `&mut`. **No mirihar-harness extension is
required** (see §C.6 for the proof of absence).

### C.1 — the RenderState/Terminal-walk map (every `state.*` / `self.*` access)

Let `t: *mut Terminal = state.terminal;` (Copy of the stored raw ptr; re-read
`state.terminal` fresh — it never changes, but keep the Zig shape). `state:
&State`. Each row is one Zig access → its faithful Rust deref + the aliasing note.

| # | Zig (`:line`) | Rust deref | kind | aliasing note |
|---|---|---|---|---|
| 1 | `state.mutex.lock()` (:1173) | `unsafe { (*state.mutex).lock() }` | mutate mutex | `lock(&self)`; `&State` ok. `unlock` in the `defer`/scope-guard (§C.4). |
| 2 | `state.terminal.modes.get(.synchronized_output)` (:1177) | `unsafe { (*t).modes.get(Mode::SynchronizedOutput) }` | `&` read → early `return Ok(())` | modes.get(&self); fresh reborrow. |
| 3 | `…screens.active.pages.getBottomRight(.screen)` (:1188) | `let active: *mut Screen = unsafe { (*t).screens.active };`  `let br = unsafe { (*active).pages.get_bottom_right(point::Tag::Screen) };` | `&` read → `Option<Pin>` | `active` is a **stored `*mut Screen`** (a separate heap alloc, ScreenSet.active, screen_set.rs:50), not a reborrow of `*t`. `Pin` is Copy (node:`*mut Node`, y,x, garbage) → owned, no live borrow. |
| 4 | `self.last_bottom_node == @intFromPtr(br.node) and self.last_bottom_y == br.y` (:1191) | `self.last_bottom_node == Some(br.node as usize) && self.last_bottom_y == br.y` | self read | `last_bottom_node: Option<usize>` (:604) — `@intFromPtr` → `br.node as usize`, wrapped in `Some`. |
| 5 | `self.last_bottom_node = @intFromPtr(br.node); self.last_bottom_y = br.y;` (:1195) | `self.last_bottom_node = Some(br.node as usize); self.last_bottom_y = br.y;` | narrow self write | field-level `&mut` on `self.last_bottom_*`; disjoint from `*t`. |
| 6 | `state.terminal.scrollViewport(.bottom)` (:1199) | `unsafe { (*t).scroll_viewport(ScrollViewport::Bottom) }` | `&mut Terminal` | terminal.rs:2686 `scroll_viewport(&mut self)`. **Fresh** `&mut *t`; the `active`/`br` borrows from #3 are already dropped (Pin owned). |
| 7 | `try self.terminal_state.update(self.alloc, state.terminal)` (:1203) | `self.terminal_state.update(self.alloc, unsafe { &mut *t })?` | `&mut self.terminal_state` + `&mut *t` | **disjoint allocations** (terminal_state is a Renderer field; `*t` is separate heap). `self.alloc = Global` (Copy ZST). render.rs:300. |
| 8 | `if self.terminal_state.dirty != .false { state.terminal.flags.search_viewport_dirty = true; }` (:1207) | `if self.terminal_state.dirty != Dirty::False { unsafe { (*t).flags.search_viewport_dirty = true; } }` | self read + `&mut *t` write | sequential; fresh reborrows. |
| 9 | `const scrollbar = …screens.active.pages.scrollbar()` (:1216) | `let active: *mut Screen = unsafe { (*t).screens.active };`  `let scrollbar = unsafe { (*active).pages.scrollbar() };` | `&mut PageList` | **re-read `active` fresh** (do NOT reuse #3's `active` across the #7 `&mut *t`). `scrollbar(&mut self)` grow.rs:137. `active`'s pointee (Screen) is a disjoint alloc, so even a cached `active` would be sound, but re-walk = faithful to Zig. |
| 10 | `const preedit = state.preedit orelse null; try p.clone(arena)` (:1219) | `let preedit: Option<state::Preedit> = state.preedit.as_ref().map(|p| p.clone());` | value read + clone | `Preedit::clone` infallible (state.rs:135), owns `Vec` (Global). |
| 11 | `if self.images.kittyRequiresUpdate(state.terminal) { self.draw_mutex.lock(); … self.images.kittyUpdate(self.alloc, state.terminal, cell_size) }` (:1231) | `if self.images.kitty_requires_update(unsafe { &*t }) { self.draw_mutex.lock(); … self.images.kitty_update(self.alloc, unsafe { &*t }, CellSize{width,height}); self.draw_mutex.unlock(); }` | `&self.images`→bool, then `&mut self.images` + `&*t` | disjoint fields/allocs. **`kitty_update` is a DEFERRED `unimplemented!()` seam** (image.rs:459) — reachable but NOT hit by the golden (no Kitty imagery ⇒ `kitty_requires_update` = false); §C.7. `draw_mutex.lock/unlock` faithfully paired. |
| 12 | `const links = osc8: { vp = state.mouse.point orelse .empty; if !mods.equal(ctrlOrSuper) .empty; self.terminal_state.linkCells(arena, vp) catch .empty }` (:1248) | `let links: CellSet = match state.mouse.point { None => CellSet::new(), Some(vp) => if !state.mouse.mods.equal(input::ctrl_or_super(Mods::default())) { CellSet::new() } else { self.terminal_state.link_cells(Global, vp).unwrap_or_else(|_| { /* log.warn */ CellSet::new() }) } };` | `&self.terminal_state` | `link_cells(&self,…)` render.rs:906. `ctrlOrSuper` → the ported input helper (verify name; `inputpkg.ctrlOrSuper`). |
| 13 | `const overlay_features = overlay: { insp = state.inspector orelse &.{}; insp.rendererInfo().overlayFeatures(arena) catch &.{} }` (:1265) | `let overlay_features: Vec<overlay::Feature> = match state.inspector { None => Vec::new(), Some(_insp) => unimplemented!("inspector.rendererInfo().overlayFeatures — deferred (rustty-inspector stub); verbatim above") };` | value read | **DEFERRED seam**, statically the `None` arm (Inspector uninhabited); §C.7. |
| 14 | `break :critical .{ links, mouse, preedit, scrollbar, overlay_features }` (:1273) | build the `Critical` local (§C.5) | — | mutex unlocks at scope end (§C.4). |

Post-critical (draw-mutex + no-more-errors region):

| # | Zig (`:line`) | Rust | aliasing note |
|---|---|---|---|
| 15 | `self.config.links.renderCellMap(arena, &critical.links, &self.terminal_state, state.mouse.point, state.mouse.mods)` (:1284) | `let _ = self.config.links.render_cell_map(Global, &mut critical.links, &self.terminal_state, state.mouse.point, state.mouse.mods); // catch→log.warn` | `&mut self.config.links` + `&mut critical.links` (local) + `&self.terminal_state` — **disjoint self fields**, NLL allows. link.rs:154. |
| 16 | highlight-clear loop over `row_data.items(.highlights/.dirty)` (:1299) | `for row in self.terminal_state.row_data.iter_mut() { if !row.highlights.is_empty() { row.highlights.clear(); row.dirty = true; any_dirty = true; } }` | `&mut self.terminal_state.row_data`. |
| 17 | `self.terminal_state.updateHighlightsFlattened(self.alloc, @intFromEnum(HighlightTag.search_match_selected), &.{m.match})` (:1318) | `if let Some(m) = &self.search_selected_match { let _ = self.terminal_state.update_highlights_flattened(self.alloc, HighlightTag::SearchMatchSelected as u8, core::slice::from_ref(&m.r#match)); }` | `&self.search_selected_match` (m) held across `&mut self.terminal_state` — **disjoint fields**, allowed. `&.{m.match}` → `core::slice::from_ref(&m.r#match)` (no clone). |
| 18 | `self.terminal_state.updateHighlightsFlattened(self.alloc, @intFromEnum(HighlightTag.search_match), m.matches)` (:1329) | `if let Some(m) = &self.search_matches { let _ = self.terminal_state.update_highlights_flattened(self.alloc, HighlightTag::SearchMatch as u8, &m.matches); }` | same disjoint-field pattern; `m.matches: Vec<Flattened>` → `&m.matches`. |
| 19 | `defer self.terminal_state.dirty = .false;` (:1345) | end-of-fn `self.terminal_state.dirty = Dirty::False;` after all uses (or a drop-guard); see §C.4. |
| 20 | `self.rebuildOverlay(critical.overlay_features) catch log` (:1349) | `let _ = self.rebuild_overlay(&critical.overlay_features); // →log.warn` | already ported (generic.rs:2815); takes `&[overlay::Feature]`. |
| 21 | draw-mutex block: `self.rebuildCells(critical.preedit, renderer.cursorStyle(&self.terminal_state, {…}), &critical.links) catch log` (:1364) | `self.draw_mutex.lock(); let cs = cursor::style(&self.terminal_state, cursor::StyleOptions{ preedit: critical.preedit.is_some(), focused: self.focused, blink_visible: cursor_blink_visible }); self.rebuild_cells(critical.preedit, cs, &critical.links); … self.draw_mutex.unlock();` | **inherits** the 3b §C.3 raw-ptr remedy internally. `cursor::style` = the ported `renderer.cursorStyle` (cursor.rs:57). `critical.preedit` **moved** into `rebuild_cells`, but `critical.preedit != null` is read FIRST for `cursor::style` → compute `is_some()` before the move. |
| 22 | scrollbar cache: `if !self.scrollbar.eql(critical.scrollbar) { self.scrollbar = critical.scrollbar; self.scrollbar_dirty = true; }` (:1383) | verbatim; `Scrollbar::eql(self, other)` grow.rs:718, Copy. |
| 23 | `self.uniforms.bg_color = .{ colors.background.{r,g,b}, @intFromFloat(@round(background_opacity*255)) }` (:1389) | `self.uniforms.bg_color = [ self.terminal_state.colors.background.r, .g, .b, (self.config.background_opacity * 255.0).round() as u8 ];` | `background_opacity: f64` (:161); `@intFromFloat(@round(..))` → `.round() as u8`. |
| 24 | macOS glass: `switch (self.config.background_blur) { .macos-glass-* => bg_color[3] = 0, else => {} }` (:1399) | `match self.config.background_blur { BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => self.uniforms.bg_color[3] = 0, _ => {} }` | already macOS-gated (whole module). **Verify** the `BackgroundBlur` variant names in render-types match `macos-glass-regular`/`macos-glass-clear`. |
| 25 | `self.images.overlayUpdate(self.alloc, self.overlay) catch log` (:1410) | `let _ = self.images.overlay_update(self.alloc, self.overlay.as_ref()); // →log.warn` | image.rs:264 `overlay_update(&mut self, alloc, Option<&Overlay>)`; `&mut self.images` + `&self.overlay` disjoint fields. |
| 26 | `self.updateCustomShaderUniformsFromState()` (:1418) | `self.update_custom_shader_uniforms_from_state();` | early-returns unless `has_custom_shaders` (§E.3). |
| 27 | `self.font_shaper.endFrame()` (:1423, OUTSIDE draw mutex) | `self.font_shaper.end_frame();` | coretext.rs:492 `end_frame(&mut self)`; harfbuzz `&self`. Placed **after** the draw-mutex block. |

### C.1a — executable logic OUTSIDE the row-table (R1 — the table is NOT complete on its own)

The §C.1 tables cover the critical-section + draw-mutex accesses, but `update_frame`
has body logic **before** the mutex and **inside** the highlight block that the
tables omit. R1 requires every one of these (all support exists in rustty):

- **Terminal-state reset block (:1143–1148)** — ported verbatim ahead of the
  `Critical` block, INCLUDING the doc comment ("We fully deinit and reset the
  terminal state every so often… Frame count is ~12 minutes at 120Hz."):
  ```rust
  let max_terminal_state_frame_count = 100_000;
  if self.terminal_state_frame_count >= max_terminal_state_frame_count {   // field: generic.rs:709
      self.terminal_state.deinit(self.alloc);                             // render.rs:292
      self.terminal_state = RenderState::empty();                         // render.rs:256 (`.empty`)
  }
  self.terminal_state_frame_count += 1;
  ```
- **Search-highlight guard + reset (:1294–1296)** — the OUTER guard on §C.1 row
  #16, NOT just the inner loop:
  ```rust
  if self.search_matches_dirty || self.terminal_state.dirty != Dirty::False {   // field: generic.rs:616
      self.search_matches_dirty = false;
      // … row-#16 highlight-clear loop, then the any_dirty→partial block below,
      //   then rows #17/#18 (updateHighlightsFlattened) — all INSIDE this guard.
  }
  ```
- **`any_dirty → partial` promotion (:1311–1313)** — inside the guard, after the
  clear loop, before rows #17/#18:
  ```rust
  if any_dirty && self.terminal_state.dirty == Dirty::False {
      self.terminal_state.dirty = Dirty::Partial;
  }
  // NOTE: The order below matters. Highlights added earlier will take priority.  (ref comment :1315-1316)
  ```
- **Commented-out timing blocks (comment fidelity)** — port verbatim as Rust `//`
  comments: the `updateFrame` timing block (:1128–1136, `// const start =
  std.time.Instant.now() … log.warn("[updateFrame time] …")`) and the critical-area
  timing block (:1166–1171, `// const start = try std.time.Instant.now() … "[updateFrame
  critical time] …"`). G3b does not gate this crate, so these are adversary-only.
- **Arena vestige (:1151)** — the `var arena = ArenaAllocator.init(self.alloc)` +
  `defer arena.deinit()` — see §D.6 (recommend omit, pass `Global`, PORT NOTE).

### C.2 — raw-ptr discipline summary (what MUST be raw)

Three pointer walks, each **re-dereffed fresh** at each use (never cached across
an interleaving `&mut`), matching the Zig `state.terminal.…` re-walk:

1. `state: &State` — shared; every field read is `state.field` / `(*state.mutex)`.
2. `t = state.terminal: *mut Terminal` — a *stored* raw pointer (Copy); `&*t` /
   `&mut *t` freshly per access. Disjoint from `self`.
3. `active = (*t).screens.active: *mut Screen` — a *stored* raw pointer to a
   **separate allocation**; re-read from `(*t)` for the scroll-block (#3) and
   again for the scrollbar (#9). Do **not** carry it across #7's `&mut *t`.

No `*mut Renderer self_raw`, no raw alias into `self.terminal_state` /
`self.font_shaper` is created by `update_frame` itself.

### C.3 — inherited remedies (NOT re-introduced)

`rebuild_cells` (§C.3 of 3b) already carries its Copy-hoist + raw-`*const
RenderState` + raw-self `add_cursor` remedy internally; `update_frame` calls it as
`self.rebuild_cells(…)` (whole-`&mut self` receiver — fine, it is a single call,
not interleaved with a live raw alias into a field). Likewise `rebuild_overlay`
carries its own disjoint-field `*const RenderState` detach (generic.rs:2889). No
new remedy needed.

### C.4 — mutex/dirty scoping (Zig `defer` → Rust)

- `state.mutex` lock/unlock (:1173/1174): the Zig `defer state.mutex.unlock()`
  fires on **every** exit of the critical block — the synchronized-output early
  `return`, the **`try self.terminal_state.update(...)` propagation (:1203, the
  SOLE propagating `?`, INSIDE the lock)**, and normal fall-through. **A
  hand-rolled `Drop` scope-guard IS available** (the earlier design claim that a
  drop-guard is "not available" was wrong — `std::sync::MutexGuard` doesn't apply
  to the guard-less `rustty_zigstd::thread::Mutex`, but a *hand-rolled* `Drop` over
  `&Mutex` is the correct faithful `defer` model) and is the recommended form:
  ```rust
  struct Unlock<'a>(&'a rustty_zigstd::thread::Mutex);
  impl Drop for Unlock<'_> { fn drop(&mut self) { self.0.unlock(); } }
  // in the critical block:
  unsafe { (*state.mutex).lock(); }
  let _mutex_guard = Unlock(unsafe { &*state.mutex });   // unlocks on ALL exits, incl. the `?` at :1203
  ```
  This is sound and conflict-free: the guard borrows the Mutex behind
  `state.mutex` — a **separate allocation, disjoint from `self`** — so it does NOT
  clash with the `&mut self.terminal_state` / `&mut *t` uses inside the block.
  **Do NOT use the "closure returning `Critical`" form** the earlier draft
  suggested: a closure cannot `?`-return `VtOutOfMemory` out of the *outer*
  `update_frame`, so the `terminal_state.update(...)?` propagation cannot live in
  it. (If explicit unlocks are used instead of the guard, an `unlock()` MUST
  precede **both** the sync-output early `return Ok(())` **and** the
  `terminal_state.update(...)?` propagation — missing the latter is the latent OOM
  deadlock.)
- `draw_mutex` (:1234/1360): two separate `lock()`…`unlock()` pairs (the kitty
  block #11 and the rebuild block #21). **These blocks contain NO `?`** (every
  sub-call is `catch`-logged/infallible), so explicit balanced
  `lock()`/`unlock()` at the block boundaries is faithful and sufficient — no leak
  path. **Caveat:** `draw_mutex` is a `self` field (generic.rs:574), so an
  `Unlock(&self.draw_mutex)` guard would hold a shared borrow of `self` across the
  `&mut self` calls in the block (`rebuild_cells`,
  `update_custom_shader_uniforms_from_state`) → **E0502**. If defer-symmetry is
  wanted here, the guard must hold a raw `*const Mutex` (not `&self.draw_mutex`) to
  avoid the borrow; otherwise just pair `lock()`/`unlock()` explicitly.
- `defer self.terminal_state.dirty = .false;` (:1345) + `errdefer comptime
  unreachable;` (:1342): after :1342 the reference is infallible; the Rust port
  runs the remaining body straight-line and sets `self.terminal_state.dirty =
  Dirty::False;` as the **last** statement before `font_shaper.end_frame()` — but
  note the reference `defer` runs *after* `endFrame`. Since nothing after :1345
  reads `dirty` except `…FromState` (:1418, which needs `dirty != .false`!) —
  **ordering matters**: the `defer` fires at function scope exit, i.e. AFTER
  `updateCustomShaderUniformsFromState` (:1418) and `endFrame` (:1423). So
  `…FromState` sees the still-`!= .false` dirty (that is exactly why it gates on
  `dirty`), and the reset happens last. **Faithful Rust:** set
  `self.terminal_state.dirty = Dirty::False;` as the final statement of
  `update_frame` (after `end_frame()`), NOT before `…FromState`. A `defer`-style
  drop-guard is the safest way to guarantee it also fires on the (caught) paths;
  a plain final assignment is fine because the post-:1342 region is infallible.

### C.5 — the `Critical` local struct

```rust
// Zig anonymous struct hoisted to a local (fn-scoped) type.
struct Critical {
    links: CellSet,
    #[allow(dead_code)]              // §B.2: write-only in the reference (see below)
    mouse: state::Mouse,
    preedit: Option<state::Preedit>,
    scrollbar: Scrollbar,
    overlay_features: Vec<overlay::Feature>,
}
```

`Critical.mouse` is **written** (`:1275 .mouse = state.mouse`) but **never read**
after the block in the reference (post-block code reads `state.mouse.point/.mods`
directly, :1289/1290). Mirror the field (R1 — it is in the reference `Critical`)
with `#[allow(dead_code)]` + a PORT NOTE ("write-only in the reference; the
regex-link path reads `state.mouse` directly, not `critical.mouse`").

### C.6 — WHY no mirihar-harness extension

The `shaper-hook-sb-clean-interleave` / `rust-mut-args-strongly-protected`
hazard is specifically: *a long-lived raw alias INTO a struct field, live across
`&mut` access to that SAME field/allocation.* `update_frame`:

- creates **no** `*const/*mut` alias into `self.terminal_state`, `self.font_shaper`,
  `self.cells`, or any self field that outlives a `&mut` to it. Every self access
  is a direct `self.field` (retagging a disjoint location) or a single method call.
- the raw pointers it *does* deref (`state.terminal`, `screens.active`) point at
  **allocations disjoint from `self`** and disjoint from each other; re-dereffing
  a stored raw pointer to a separate allocation after a `&mut` to a *different*
  allocation is not a use-after-retag.
- the two interleave-prone helpers (`rebuild_cells`, `rebuild_row`) are already
  proven on `mirihar-chunk3b` and are invoked as plain receiver calls.

**Therefore: no new borrow shape to prove; no `mirihar` candidate to add.**
Guard condition for the adversary: if the implementer deviates from the §C.2
fresh-re-walk (e.g. caches `let active = …` once and reuses it across the #7
`&mut *t`, or introduces a `*const RenderState`/`*mut Renderer` alias held across
the highlight loop), that WOULD be a new pattern → it must be proven on an
extended `mirihar-chunk3b` (add a `updateframe_*` candidate) under BOTH
`-Zmiri-stacked-borrows` and `-Zmiri-tree-borrows`, per the memory doctrine —
**do not hand-reason SB.** The recommended straight port needs none of that.

### C.7 — deferred seams reachable from `update_frame` (R5, verbatim-preserved, not executed)

| seam | why unhit | R1/R5 handling |
|---|---|---|
| `images.kitty_update` (#11, image.rs:459 `unimplemented!()`) | guarded by `kitty_requires_update` = false for non-Kitty content (the golden renders no Kitty imagery) | call it faithfully; body already a deferred R5 seam with verbatim Zig. |
| inspector `overlayFeatures` (#13) | `state.inspector` uninhabited ⇒ always `None` ⇒ empty-features arm | `match … { None => Vec::new(), Some(_) => unimplemented!("…verbatim above") }` — R1 keeps the reference logic as the unreachable arm. |
| `overlay.apply_features` semantic-prompt render leaf (via `rebuild_overlay`) | already a 3b deferred R5 seam (`overlay.rs`), only reached with an inspector feature (unhit) | inherited from 3b; unchanged. |

None of these is on the golden path (`derived_config_for_test`, no inspector, no
Kitty), so `update_frame` runs end-to-end without tripping an `unimplemented!()`.

---

## D. SIGNATURE BLOCKS (concrete Rust, all in `impl Renderer`, `mod macos_impl`)

```rust
// ── orchestrator ────────────────────────────────────────────────────────────
/// Update the frame data.
// Zig: pub fn updateFrame(self, state: *renderer.State, cursor_blink_visible: bool) Allocator.Error!void
// WIDENED to `pub` (dead-code-exempt entry point; also the crate's frame driver).
// requires: `use rustty_vt::pagelist::OutOfMemory as VtOutOfMemory;` in mod macos_impl
//           (bare `OutOfMemory` is atlas::OutOfMemory — see §B.2 COMPILE-BLOCKER box)
pub fn update_frame(
    &mut self,
    state: &state::State,
    cursor_blink_visible: bool,
) -> Result<(), VtOutOfMemory> { /* §C walk */ }

// ── uniforms: font grid ──────────────────────────────────────────────────────
/// Update uniforms that are based on the font grid. Caller must hold the draw mutex.
// Zig: fn updateFontGridUniforms(self) void   — pub for dead-code exemption (no 3c caller)
pub fn update_font_grid_uniforms(&mut self) {
    self.uniforms.cell_size = [
        self.grid_metrics.cell_width as f32,
        self.grid_metrics.cell_height as f32,
    ];
}

// ── uniforms: screen size ────────────────────────────────────────────────────
/// Update uniforms that are based on the screen size. Caller must hold the draw mutex.
// Zig: fn updateScreenSizeUniforms(self) void  — pub (no 3c caller)
pub fn update_screen_size_uniforms(&mut self) { /* §E.1 */ }

// ── uniforms: bg image vertex buffer (CPU) ───────────────────────────────────
/// Update the background image vertex buffer (CPU-side). Caller holds the draw mutex.
// Zig: fn updateBgImageBuffer(self) void  — pub (no 3c caller)
pub fn update_bg_image_buffer(&mut self) { /* §E.4 */ }

// ── custom-shader uniforms from terminal state ───────────────────────────────
/// Update custom shader uniforms that depend on terminal state.
// Zig: fn updateCustomShaderUniformsFromState(self) void  — pub (called by update_frame + directly tested)
pub fn update_custom_shader_uniforms_from_state(&mut self) { /* §E.3 */ }

// ── per-frame custom-shader uniforms ─────────────────────────────────────────
/// Update per-frame custom shader uniforms. Called once per frame (drawFrame half; no GPU calls).
// Zig: fn updateCustomShaderUniformsForFrame(self) !void  — Instant.now() infallible in Rust ⇒ ()
// pub (no 3c caller — its caller is drawFrame/Chunk 4; directly tested here).
pub fn update_custom_shader_uniforms_for_frame(&mut self) { /* §E.6 */ }
```

**Widening rationale (PORT NOTE form, per 3b §F / 3a):** `update_font_grid_uniforms`,
`update_screen_size_uniforms`, `update_bg_image_buffer`,
`update_custom_shader_uniforms_from_state`, `update_custom_shader_uniforms_for_frame`
are reference-`fn` (private). Widen each to `pub` "for dead-code exemption under
`-D warnings` — reachable from the crate root; reverts to reference-private
semantics when its real callers (`setScreenSize`/`setFontGrid`/`changeConfig`/
`drawFrame`) land — left pub, reverting is churn." No `#[allow(dead_code)]`.

**§D.6 — the arena vestige:** the reference opens `var arena =
ArenaAllocator.init(self.alloc)` (:1151) and threads `arena_alloc` into
`preedit.clone`, `link_cells`, `render_cell_map`, `overlayFeatures`. In rustty
every one of those owns its result as a `Global` `Vec`/`CellSet` freed by `Drop`
(the arena-collapse precedent — message.rs `SearchMatches`). **Recommend: omit the
explicit `ArenaAllocator`** and pass `Global` where the reference passes
`arena_alloc`, with a PORT NOTE citing the reference `arena` (nothing leaks — the
former arena tenants are now `Drop`-freed). (Alternatively keep a real
`ArenaAllocator` local for byte-faithfulness; the collapse is cleaner and matches
the crate.)

---

## E. THE UNIFORMS MATH (decision 4) — field-by-field, every field CONFIRMED

All target fields exist in the already-ported `shaders::Uniforms` (shaders.rs:38)
and `shadertoy::Uniforms` (shadertoy.rs:67); `self.size: size::Size` =
`rustty_render_types` (Size/ScreenSize/CellSize/Padding/GridSize confirmed);
`math::ortho2d` / `math::Mat` = `rustty_util::math` (math.rs:2/6).

> **REQUIRED IMPORTS — reconcile the full set (§E.1/§E.4/§E.6).** `mod macos_impl`
> today imports only `use crate::size::Size;` (generic.rs:419) and
> `use crate::shaders::{Atlas, BgImage, CellBg, CellText, CellTextBools, Uniforms,
> UniformsPaddingExtend};` (generic.rs:415) and `use rustty_render_types::WindowPaddingColor;`.
> The 3c bodies name several types unqualified that are **NOT yet in scope** — add:
> - `use crate::size::{ScreenSize, Padding, GridSize, CellSize};` — §E.1 type
>   annotations + `GridSize{…}`/`CellSize{…}` literals. (`crate::size` is
>   `pub use rustty_render_types as size`, lib.rs:87 — all four resolve.)
> - to the `crate::shaders` import: `BgImageInfo`, `BgImageInfoPosition`,
>   `BgImageInfoFit` (shaders.rs:240/256/276) — §E.4 names all three; only `BgImage`
>   is imported today.
> - the config enums §E.4/§C.1-#24 name: `BackgroundImagePosition`,
>   `BackgroundImageFit`, `BackgroundBlur` (render-types) — add a
>   `use rustty_render_types::{BackgroundImagePosition, BackgroundImageFit, BackgroundBlur};`
>   (or qualify at use-site). Verify each against the actual `DerivedConfig` field
>   types before finalizing.

### E.1 — `update_screen_size_uniforms` (:1935)

```rust
let terminal_size: ScreenSize = self.size.terminal();          // Size::terminal() → screen.sub_padding(padding), lib.rs:251
// Blank space around the grid.
let blank: Padding = self.size.screen                          // ScreenSize
    .blank_padding(                                            // lib.rs:460 (self, padding, grid, cell) → Padding
        self.size.padding,
        GridSize { columns: self.cells.size.columns, rows: self.cells.size.rows },
        CellSize { width: self.grid_metrics.cell_width, height: self.grid_metrics.cell_height },
    )
    .add(self.size.padding);                                   // Padding::add, lib.rs:580
self.uniforms.projection_matrix = math::ortho2d(              // Mat = [F32x4;4]
    -1.0 * self.size.padding.left as f32,
    (terminal_size.width + self.size.padding.right) as f32,
    (terminal_size.height + self.size.padding.bottom) as f32,
    -1.0 * self.size.padding.top as f32,
);
self.uniforms.grid_padding = [blank.top as f32, blank.right as f32, blank.bottom as f32, blank.left as f32];
self.uniforms.screen_size  = [self.size.screen.width as f32, self.size.screen.height as f32];
```

- `projection_matrix: math::Mat` ✔ (shaders.rs:41)
- `grid_padding: [f32;4]` ✔ (order top,right,bottom,left — reference comment)
- `screen_size: [f32;2]` ✔
- `self.cells.size` = `GridSize {columns, rows}` ✔ (Contents.size)

### E.2 — `update_font_grid_uniforms` (:1112) — see §D. `cell_size: [f32;2]` ✔.

### E.3 — `update_custom_shader_uniforms_from_state` (:2007)

Guards: `if !self.has_custom_shaders { return; }` then `if self.terminal_state.dirty
== Dirty::False { return; }`. Writes into `self.custom_shader_uniforms:
shadertoy::Uniforms` from `&self.terminal_state.colors` + `self.config.*`:

```rust
// 256-color palette
for (i, color) in colors.palette.iter().enumerate() {
    self.custom_shader_uniforms.palette[i] = [color.r as f32/255.0, color.g as f32/255.0, color.b as f32/255.0, 1.0];
}
self.custom_shader_uniforms.background_color = [bg.r as f32/255.0, …, 1.0];
self.custom_shader_uniforms.foreground_color = [fg.r as f32/255.0, …, 1.0];
// `colors.cursor: Option<RGB>` — a GENUINE `?RGB` (:2047), NOT a union ⇒ plain `if let Some(..)`:
if let Some(cur) = colors.cursor { self.custom_shader_uniforms.cursor_color = [cur.r as f32/255.0, …, 1.0]; }
// config-derived (Option<TerminalColor>): the reference does a RAW `.color` UNION
// FIELD ACCESS `cursor_text.color.r` / `selection_bg.color.r` / `selection_fg.color.r`
// (:2063/2073/2083) on `?TerminalColor`. Zig safety-checks `.color` as a TAG-ASSERT
// → it PANICS on the `cell-foreground`/`cell-background` specials (Config.zig:5549).
// A plain `if let Some(TerminalColor::Color(c))` would SILENTLY SKIP the specials
// (leave the uniform unchanged) — a THIRD behavior matching NEITHER the reference
// (panic) nor the 3b cursor-color resolution (which is a *different* code path that
// RESOLVES the specials to fg/bg). Faithful port = destructure the Option, then
// TAG-ASSERT with `match … _ => unreachable!()` (the 3b `unreachable!()` convention,
// generic.rs:1590/1680). See §G-RISK-1.
if let Some(v) = self.config.cursor_text {
    match v { TerminalColor::Color(c) => { …cursor_text = [c.r as f32/255.0, c.g.., c.b.., 1.0]; } _ => unreachable!() }
}
if let Some(v) = self.config.selection_background {
    match v { TerminalColor::Color(c) => { …selection_background_color = [c.r.., c.g.., c.b.., 1.0]; } _ => unreachable!() }
}
if let Some(v) = self.config.selection_foreground {
    match v { TerminalColor::Color(c) => { …selection_foreground_color = [c.r.., c.g.., c.b.., 1.0]; } _ => unreachable!() }
}
self.custom_shader_uniforms.cursor_visible = self.terminal_state.cursor.visible as i32;   // @intFromBool
let cursor_style = cursor::Style::from_terminal(self.terminal_state.cursor.visual_style); // renderer.CursorStyle.fromTerminal, cursor.rs:32
self.custom_shader_uniforms.previous_cursor_style = self.custom_shader_uniforms.current_cursor_style;
self.custom_shader_uniforms.current_cursor_style  = cursor_style as i32;                   // @intFromEnum
```

Confirmed shadertoy fields: `palette:[[f32;4];256]`, `background_color`,
`foreground_color`, `cursor_color`, `cursor_text`, `selection_background_color`,
`selection_foreground_color`, `cursor_visible:i32`, `current_cursor_style:i32`,
`previous_cursor_style:i32` — all present (shadertoy.rs:78–95). `colors.cursor:
Option<RGB>` (render.rs:119) ✔. `cursor.visible/visual_style` (render.rs) ✔.

### E.4 — `update_bg_image_buffer` (:1973)

```rust
self.bg_image_buffer = BgImage {
    opacity: self.config.bg_image_opacity,                     // f32 ✔ (:176)
    info: BgImageInfo {
        position: match self.config.bg_image_position {        // BackgroundImagePosition (render-types) → BgImageInfoPosition (shaders)
            BackgroundImagePosition::TopLeft      => BgImageInfoPosition::Tl,
            BackgroundImagePosition::TopCenter    => BgImageInfoPosition::Tc,
            BackgroundImagePosition::TopRight     => BgImageInfoPosition::Tr,
            BackgroundImagePosition::CenterLeft   => BgImageInfoPosition::Ml,
            BackgroundImagePosition::CenterCenter
          | BackgroundImagePosition::Center       => BgImageInfoPosition::Mc,   // ref `.@"center-center", .center => .mc`
            BackgroundImagePosition::CenterRight  => BgImageInfoPosition::Mr,
            BackgroundImagePosition::BottomLeft   => BgImageInfoPosition::Bl,
            BackgroundImagePosition::BottomCenter => BgImageInfoPosition::Bc,
            BackgroundImagePosition::BottomRight  => BgImageInfoPosition::Br,
        },
        fit: match self.config.bg_image_fit {                  // BackgroundImageFit → BgImageInfoFit
            BackgroundImageFit::Contain => BgImageInfoFit::Contain,
            BackgroundImageFit::Cover   => BgImageInfoFit::Cover,
            BackgroundImageFit::Stretch => BgImageInfoFit::Stretch,
            BackgroundImageFit::None    => BgImageInfoFit::None,
        },
        repeat: self.config.bg_image_repeat,                   // bool ✔ (:179)
    },
};
self.bg_image_buffer_modified = self.bg_image_buffer_modified.wrapping_add(1);  // `+%= 1`
```

Both enums' variants confirmed (render-types lib.rs:141/162; shaders.rs:256/276)
— all 10 position arms + 4 fit arms map 1:1, `CenterCenter|Center → Mc`.

### E.5 — the monomorphic Metal consts (`custom_shader_y_is_down` / `custom_shader_target`)

Both already exist as concrete `Metal` associated consts — no new metal.rs work:

- `Metal::CUSTOM_SHADER_Y_IS_DOWN: bool = true` (metal_impl.rs:145)
- `Metal::CUSTOM_SHADER_TARGET: shadertoy::Target = Target::Msl` (metal_impl.rs:142)
- `Metal::SWAP_CHAIN_COUNT: usize = 3` (metal_impl.rs:148)

`custom_shader_target` (ref :845) is used only in the **drawFrame/init-shaders**
path → **Chunk 4**, NOT 3c; `custom_shader_y_is_down` is used in `…ForFrame`
(:2157/2167). Monomorphic collapse: bind a module const
`const CUSTOM_SHADER_Y_IS_DOWN: bool = Metal::CUSTOM_SHADER_Y_IS_DOWN;` (the
`SWAP_CHAIN_COUNT` precedent, generic.rs:429) and keep **both** reference `if`
branches verbatim (`if !CUSTOM_SHADER_Y_IS_DOWN {…}` and `if
CUSTOM_SHADER_Y_IS_DOWN {…}`) — R1 branch fidelity; rustc does not warn on
`if !CONST`. (Do NOT hand-drop the dead branch — the const-collapse keeps both,
matching the comptime source.)

### E.6 — `update_custom_shader_uniforms_for_frame` (:2099)

`if !self.has_custom_shaders { return; }` then:

```rust
let now = std::time::Instant::now();                 // Zig `try std.time.Instant.now()` → infallible
// defer self.last_frame_time = Some(now);           // set at fn end (Zig defer)
// Mirror the reference `orelse t: { self.first_frame_time = now; break :t now; }` shape.
// (An `unwrap_or_else(|| { self.first_frame_time = Some(now); now })` also COMPILES under
//  edition-2021 disjoint closure captures — verified — since `Option<Instant>` is `Copy` so
//  the receiver read is a copy taken before the closure's `&mut self.first_frame_time`; the
//  `match` is preferred purely as a literal mirror of the Zig block, no reader ambiguity.)
let first_frame_time = match self.first_frame_time { Some(t) => t, None => { self.first_frame_time = Some(now); now } };
let last_frame_time  = self.last_frame_time.unwrap_or(now);
let u = &mut self.custom_shader_uniforms;
u.time       = now.duration_since(first_frame_time).as_nanos() as f32 / 1_000_000_000.0;   // /ns_per_s
u.time_delta = now.duration_since(last_frame_time ).as_nanos() as f32 / 1_000_000_000.0;
u.frame += 1;
let screen = self.size.screen; let padding = self.size.padding; let cell = self.size.cell;
u.resolution = [screen.width as f32, screen.height as f32, 1.0];
u.channel_resolution[0] = [screen.width as f32, screen.height as f32, 1.0, 0.0];
if let Some(cursor) = self.cells.get_cursor_glyph() {   // Contents::get_cursor_glyph → Option<CellText> (cell.rs:218)
    // pixel_x/pixel_y math (:2140-2207) using cursor.glyph_size/grid_pos/bearings/color[4]
    // the y-flip branches keyed on CUSTOM_SHADER_Y_IS_DOWN (§E.5, keep both branches)
    // then current/previous_cursor + current/previous_cursor_color + cursor_change_time
    //
    // IMPLEMENTER FLAG (block body deferred here): `cursor.grid_pos: [u16; 2]`
    // (shaders.rs:137) vs `cell.{width,height}: u32` / `padding.{left,top}: u32`
    // (render-types). The reference `cursor.grid_pos[0] * cell.width + padding.left`
    // is `u16 * u32` — a TYPE ERROR in Rust (no implicit widening). Cast to the wider
    // type BEFORE `as f32`, e.g. `(cursor.grid_pos[0] as u32 * cell.width + padding.left)
    // as f32`. Same for the `grid_pos[1] * cell.height + padding.top` line.
}
u.focus = self.focused as i32;                          // @intFromBool
if self.custom_shader_focused_changed && self.focused { u.time_focus = u.time; self.custom_shader_focused_changed = false; }
```

Confirmed shadertoy fields: `time,time_delta,frame,resolution:[f32;3],
channel_resolution:[[f32;4];4], current_cursor:[f32;4], previous_cursor,
current_cursor_color, previous_cursor_color, cursor_change_time, time_focus,
focus:i32` — all present. `first_frame_time/last_frame_time: Option<Instant>`
(generic.rs:637/642) ✔; `size.{screen,padding,cell}` ✔; `Contents::get_cursor_glyph`
returns `Option<CellText>` with `glyph_size/grid_pos/bearings/color` ✔ (shaders.rs
CellText). **Ordering note (Zig `defer self.last_frame_time = now;` :2109):**
`last_frame_time` is captured BEFORE the assignment, so read `self.last_frame_time`
into the local first, then set `self.last_frame_time = Some(now);` as the LAST
statement of the fn (mirror the `defer`).

> `std::meta.eql(new_cursor, uniforms.current_cursor)` (:2198) → `new_cursor ==
> u.current_cursor` (`[f32;4]` derives `PartialEq`); shadertoy `Uniforms` derives
> `Clone, Copy` (shadertoy.rs:66) — element comparisons are fine (no struct-wide
> `Eq` needed).

---

## F. DEAD-CODE (`-D warnings`) + THE 3c UNIT TEST (decision 5)

### F.1 — reachability / dead-code

- `update_frame` → **`pub`** (frame driver + entry point). Retires the 3b
  "real caller lands in 3c" notes on `rebuild_cells`/`rebuild_overlay` (they stay
  `pub`; now they also have a live in-crate caller).
- `update_font_grid_uniforms`, `update_screen_size_uniforms`,
  `update_bg_image_buffer`, `update_custom_shader_uniforms_from_state`,
  `update_custom_shader_uniforms_for_frame` → **`pub`** (§D widening).
  `…FromState` is *also* reachable via `update_frame`, but widen anyway so the
  direct test can call it.
- `Critical.mouse` → `#[allow(dead_code)]` (§C.5, faithful write-only mirror).
- No other `#[allow(dead_code)]`.

### F.2 — the tests (macOS/Metal-gated, Metal-skip like 3b)

Reuse the 3b harness verbatim: `opts(cols,rows)`, `derived_config_for_test()`,
`test_renderer(cols,rows) -> Option<Renderer>` (generic.rs:2926/2944/3006 — skips
`None` with no Metal device). Add:

**(1) `update_frame_fills_cells_uniforms_scrollbar`** — the headline test:
```rust
let mut r = match test_renderer(10, 3) { Some(r) => r, None => return };
let mut mutex = Mutex::new();
let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
{ let mut s = t.vt_stream(); s.next_slice(b"Ab"); s.deinit(); }
let st = state::State { mutex: &mut mutex, terminal: &mut t, inspector: None, preedit: None, mouse: state::Mouse::default() };
r.update_frame(&st, true).unwrap();
// Contents filled (same asserts as rebuild_cells_fills_bg_fg_cursor, now driven end-to-end):
let bg = r.terminal_state.colors.background;
assert_eq!(*r.cells.bg_cell(0,0), [bg.r,bg.g,bg.b,0]);
assert!(!r.cells.fg_rows.lists[1].is_empty());
let cur = r.cells.get_cursor_glyph().expect("cursor");
assert!(cur.bools.is_cursor_glyph);
assert_eq!(cur.grid_pos, [2,0]);
assert_eq!(r.uniforms.cursor_pos, [2,0]);                 // written by rebuild_cells inside update_frame
// bg_color uniform (§23): opacity 1.0 → alpha 255
assert_eq!(r.uniforms.bg_color, [bg.r,bg.g,bg.b,255]);
// scrollbar snapshot happened; on a small non-scrolling screen: cache updated & consistent
assert!(r.scrollbar.eql(unsafe { (*(*(&st).terminal)).screens.active … .pages.scrollbar() }) || r.scrollbar == Scrollbar::ZERO);
t.deinit(Global);
```
(scrollbar assert: simplest is to snapshot the expected `Scrollbar` from the same
`pages.scrollbar()` and assert `r.scrollbar.eql(expected)` + `r.scrollbar_dirty`
transitioned as designed; adapt to the actual small-screen value.)

**(2) `update_screen_size_uniforms_projection`** — direct: after `test_renderer`,
call `r.update_screen_size_uniforms()`; assert `r.uniforms.screen_size ==
[screen.w as f32, screen.h as f32]`, `grid_padding` (all-zero padding fixture ⇒
blank padding = balanced remainder), and a spot-check of `projection_matrix`
against `math::ortho2d(...)` recomputed from the fixture.

**(3) `update_font_grid_uniforms_cell_size`** — direct: assert
`r.uniforms.cell_size == [grid_metrics.cell_width as f32, cell_height as f32]`.

**(4) `update_bg_image_buffer_maps_config`** — set `r.config.bg_image_position =
CenterCenter; r.config.bg_image_fit = Cover; r.config.bg_image_repeat = true;`
call `r.update_bg_image_buffer()`; assert `r.bg_image_buffer.info.position ==
BgImageInfoPosition::Mc`, `.fit == Cover`, `.repeat == true`,
`bg_image_buffer_modified == 1`.

**(5) `update_custom_shader_uniforms_from_state_palette`** — set
`r.has_custom_shaders = true`; drive `r.terminal_state.update(Global, &mut t)`
(dirty=Full); call `r.update_custom_shader_uniforms_from_state()`; assert
`custom_shader_uniforms.background_color/foreground_color` = colors/255,
`palette[0]` matches, `cursor_visible`, `current_cursor_style`.

**(6) `update_custom_shader_uniforms_for_frame_timing`** — set
`r.has_custom_shaders = true`; call `r.update_custom_shader_uniforms_for_frame()`;
assert `custom_shader_uniforms.frame == 1`, `resolution == [screen.w, screen.h,
1.0]`, `channel_resolution[0] == [.., 0.0]`, `focus == 1`, and
`r.first_frame_time.is_some()` + `r.last_frame_time.is_some()`.

All six gate on `test_renderer` returning `Some` (skip cleanly without a Metal
device) — no Miri (Metal/CoreText FFI); the R6 doctrine's native-run exception
applies exactly as for the 3b test, and §C.6 establishes there is no new
SB-unsound structure to justify.

---

## G. OPEN RISKS

- **RISK-1 (fidelity, `…FromState`) — RESOLVED (was a real R1 violation):** the
  reference reads `cursor_text.color.r` / `selection_bg.color.r` /
  `selection_fg.color.r` on a `?Config.TerminalColor` (:2063/2073/2083) — a **raw
  Zig union `.color` field access**, which is **safety-checked as a TAG-ASSERT**
  (PANICS on the `cell-foreground`/`cell-background` specials; the union is
  `union(enum){ color: Color, cell-foreground, cell-background }`, Config.zig:5549;
  cursor-text :902, selection-fg/bg :707/708 all carry the specials). rustty's
  `TerminalColor` = `enum { Color(Color), CellForeground, CellBackground }`
  (generic.rs:97). The earlier `if let Some(TerminalColor::Color(c)) = self.config.X`
  shape **silently skips** the specials (leaves the uniform unchanged) — a THIRD
  behavior matching neither the reference (panic) nor the 3b precedent
  (generic.rs:1575/1649, cell.zig cursor-color resolution — a **different** path
  that genuinely RESOLVES the specials to fg/bg via `cursor_style.fg/.bg`, NOT a
  raw `.color` access). **Faithful port** (see §E.3): destructure the Option, then
  TAG-ASSERT — `if let Some(v) = self.config.X { match v { TerminalColor::Color(c)
  => …, _ => unreachable!() } }` (the 3b `unreachable!()` convention). Only the
  three `config.*` union accesses need this; `colors.cursor` (:2047) is a genuine
  `?RGB` and stays `if let Some(..)`.

- **RISK-2 (variant names) — VERIFIED, low:** `BackgroundBlur::MacosGlassRegular
  | MacosGlassClear` confirmed (render-types lib.rs:199); the `inputpkg.ctrlOrSuper(.{})`
  free-fn is `rustty_vt::input::key::ctrl_or_super(mods: Mods) -> Mods`
  (key.rs:1153), so §C-#12 is `!state.mouse.mods.equal(input::key::ctrl_or_super(Mods::default()))`.
  Adversary need only confirm the exact `use` path/`equal` method name.

- **RISK-3 (dirty ordering, §C.4):** the `defer self.terminal_state.dirty = .false`
  fires AFTER `…FromState` (:1418) and `endFrame` (:1423). `…FromState` *gates on*
  `dirty != .false`, so the reset MUST be the last thing. A plain final assignment
  is correct (post-:1342 region infallible), but a drop-guard is safer if the
  implementer keeps any early-return after :1342. Adversary: confirm `…FromState`
  still sees `dirty != False`.

- **RISK-4 (`state: &State` vs `*mut`):** recommended `&state::State`. If the
  guard-less `Mutex::lock`/`unlock` or a future State-field write forces it,
  switch to `*mut state::State` — the §C walk is identical (it already derefs raw
  interior pointers). Low risk; flagged for the implementer.

- **RISK-5 (mutex scoping — LATENT OOM DEADLOCK):** the guard-less mutex has no
  RAII drop, and `state.mutex` is held across **three** exits: the sync-output
  early `return` (:1179), the `try self.terminal_state.update(...)?` propagation
  (:1203 — the sole propagating `?`, INSIDE the lock), and normal fall-through.
  Unlocking only on the first + last (the earlier draft's plan) **leaks
  `state.mutex` on the OOM path → deadlock** (untestable: OOM never fires in the
  golden). §C.4 now prescribes a **hand-rolled `Drop` scope-guard over `&Mutex`**
  for `state.mutex` (sound — the mutex alloc is disjoint from `self`), which fires
  on all three exits; a closure returning `Critical` is explicitly rejected (it
  cannot `?`-return out of the outer fn). For the two `draw_mutex` pairs (#11,
  #21) explicit balanced `lock()`/`unlock()` suffices (no `?` inside); a
  `&self.draw_mutex` guard would E0502 against the block's `&mut self` calls.

- **RISK-6 (Metal-only, no Miri):** as with 3b, the whole module is
  macOS/Metal-gated so `update_frame` cannot be Miri-checked. §C.6 argues (by the
  disjoint-allocation structure) that no new SB/TB hazard exists; the adversary
  must re-verify the §C.2 fresh-re-walk discipline is actually followed in the
  implementation (a cached `active` across `&mut *t`, or any `*mut Renderer`/
  `*const RenderState` alias held across the highlight loop, would demand a
  `mirihar-chunk3b` `updateframe_*` candidate proven under both borrow models).

- **RISK-7 (`Contents.size` / `cells.get_cursor_glyph`):** `…ForFrame`'s cursor
  block and `update_screen_size_uniforms`'s `self.cells.size.{rows,columns}`
  assume `Contents.size: GridSize` and a populated cursor glyph. In test (6)
  `…ForFrame` runs with an empty `Contents` (no prior `rebuild_cells`) so
  `get_cursor_glyph()` is `None` and the cursor block is skipped — assert only the
  timing/resolution fields there, or run a `rebuild_cells` first to exercise the
  cursor branch.
