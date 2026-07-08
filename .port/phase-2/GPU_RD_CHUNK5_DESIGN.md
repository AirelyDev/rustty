# GPU R-d Chunk 5 — the FINALE: seams + docs + the PORT STATUS / G3b decision

**Design doc. Design ONLY — touches NO `crates/`.** Architect deliverable for the
Chunk-5 workflow (`.port/phase-2/wf-gpu-rd-chunk5-seams-docs.js`). This is the LAST
chunk of the GPU R-d offscreen arc. NO new heavy porting. Three deliverables:

1. **Seam audit** — classify every `unimplemented!()` in `rustty-renderer/src`, and
   PROVE none is reachable from a ported/tested path (a reachable one is a blocker bug).
2. **The honest PORT STATUS / G3b decision** — the crux. The renderer renders
   offscreen but has substantial deferred surface; flipping `lib.rs` to
   `PORT STATUS: done` would be dishonest and would prematurely activate G3b
   comment-coverage on the whole crate. Decide + justify.
3. **Docs update** — mark the offscreen R-d arc complete in the blueprint /
   DEFERRALS / lib.rs header, with a concise completion summary.

STATE going in: Chunks 0/1/2/3a/3b/3c/4 are DONE + committed. The renderer RENDERS —
`generic::macos_impl::tests::golden_renders_cursor_fg_bg` builds a deterministic
terminal state (`"Ab"` + cursor), runs `update_frame` → `draw_frame(true)` through the
real Metal encode/submit path into an offscreen IOSurface-backed `BGRA8Unorm` target,
reads the pixels back, and asserts the render structure (bg/fg/cursor pixels). The
raw-pointer aliasing discipline of `draw_frame` is proven clean under **both** Stacked
Borrows and Tree Borrows by the standalone `mirihar-chunk3b` harness (`drawframe_shipped`
candidate; the `drawframe_naive` candidate demonstrates the SB UB the discipline avoids).

---

## §1 SEAM AUDIT

### 1.1 Method — the exact set of live `unimplemented!()` call sites

`grep -rnE "^\s+unimplemented!\(" crates/rustty-renderer/src` plus the one inline
`=> unimplemented!(` arm (`generic.rs:3218`). Doc-comment *mentions* of the word
`unimplemented!` (e.g. `generic.rs:28`, `image.rs:7/17`, `overlay.rs:17`,
`thread.rs:48`, `metal_impl.rs:48`, `generic.rs:3794/5143`) are NOT call sites and are
excluded.

> **Count note (corrects the brief).** The brief estimated "~27 sites (7 in generic.rs,
> ~20 in metal/*)". The ACTUAL live call-site count is **18** — `generic.rs` = 4,
> `metal/*` = 3, the rest spread over `image.rs` (5), `overlay.rs` (4), `message.rs` (1),
> `thread.rs` (1). The brief's estimate appears to predate the Chunk-2/3/4 fills (many
> once-`unimplemented!()` metal leaf bodies are now real). The audit below is exhaustive
> against the current tree; the count discrepancy is a stale-estimate artifact, not a
> missing-seam gap.

### 1.2 The seam table

Classification legend:
- **DEFERRED → `<tier>`** = a legitimate R5 seam, verbatim Zig body preserved above it,
  NOT on the offscreen R-d path, with the named Goal-2 tier that will fill it and the
  reason it is not needed for the offscreen slice.
- **REACHABLE-BUG** = reachable from a ported/tested path (`draw_frame`/`update_frame`/
  `rebuild_cells` / the golden test). **None found.**

| # | Seam (file:line) | Reference method | Classification — tier + why off the offscreen path |
|---|---|---|---|
| 1 | `generic.rs:1314` `Renderer::init` | `generic.zig` `Renderer.init` | **DEFERRED → windowing-host / apprt-surface tier.** Needs `GraphicsAPI::init` over a **live NSView/apprt surface**, `font.Shaper.init`, `initShaders`, the uniform setup + `prepBackgroundImage`. The golden **constructs the `Renderer` struct field-by-field** (`golden_renderer`, `generic.rs:4917`) over `Metal::init_offscreen`, then calls `update_font_grid_uniforms`/`update_screen_size_uniforms` by hand — it never calls `Renderer::init`. Not on the offscreen path. |
| 2 | `generic.rs:1360` `Renderer::deinit` | `generic.zig` `Renderer.deinit` | **DEFERRED → windowing-host tier.** Tears down the full draw-path state via the not-yet-declared `deinitShaders` etc. **There is NO `impl Drop for Renderer`** (grep-confirmed) — dropping the golden's `r` runs each field's own `Drop` (`api`/`swap_chain`/`shaders`/`font_shaper`…), NOT `Renderer::deinit`. Never called on any ported path. |
| 3 | `generic.rs:276` `DerivedConfig::init<C>` | `generic.zig` `Renderer.DerivedConfig.init` | **DEFERRED → renderer↔config tier (G7).** Generic `C` binds `rustty_config::Config` only in core; a `renderer → config` edge cycles (config already deps renderer). The golden uses `derived_config_for_test()`, not `DerivedConfig::init`. Config-coupled, off-path. |
| 4 | `generic.rs:3218` inspector `overlayFeatures` | `generic.zig` `insp.rendererInfo().overlayFeatures` | **DEFERRED → rustty-inspector tier.** The seam field is `state.inspector: Option<*mut Inspector>` where `pub enum Inspector {}` is uninhabited (`state.rs:27,55`). Note: a raw pointer to an uninhabited type is still a perfectly constructible *value*, and the `Some(_insp)` pattern only *binds* (never derefs) it — so nothing here is statically uninhabited (the arm is NOT "unconstructible"). The airtight reason it is unreachable: **no ported path constructs a non-`None` inspector**, and the golden pins `inspector: None` (`generic.rs:5012`), so the `unimplemented!()` (`generic.rs:3218`) is never reached. Verbatim body kept as the unreached arm. |
| 5 | `metal/metal_impl.rs:334` `Metal::init` | `Metal.zig` `init` | **DEFERRED → windowing-host tier.** Needs a live NSView + `IOSurfaceLayer`. The offscreen path is the separate `Metal::init_offscreen` (used by the golden). Off-path. |
| 6 | `metal/metal_impl.rs:384` `Metal::loop_enter` | `Metal.zig` `loopEnter` | **DEFERRED → windowing-host / display-link tier.** Installs the `IOSurfaceLayer` display callback (Metal is the only backend that declares `loopEnter`). No windowed layer offscreen; never called. |
| 7 | `metal/frame.rs:198` async present | `Frame.zig` `complete` (the `!sync` half) | **DEFERRED → windowing-host tier.** The async `addCompletedHandler` present half. R5-preserved as the `block2::RcBlock` shape. The golden calls `draw_frame(true)` ⇒ `complete(sync=true)` ⇒ the **synchronous** branch (buffer commit + `waitUntilCompleted` + direct block invoke). The `!sync` branch is never taken on the tested path. |
| 8 | `message.rs:182` `Message::initChangeConfig<C>` | `message.zig` `initChangeConfig` | **DEFERRED → renderer↔config / Thread-event-loop tier (G7).** Config-coupled (same wall as #3). Consumed by the cross-thread `ChangeConfig` mailbox path, not by the offscreen render. Off-path. |
| 9 | `thread.rs:71` `Thread::DerivedConfig::init<C>` | `Thread.zig` `DerivedConfig.init` | **DEFERRED → renderer↔config / Thread-event-loop tier (G7).** Inhabited field-less placeholder; the `custom_shader_animation` config field + real body land with the config-side placement. Off-path. |
| 10 | `image.rs:465` `State::kitty_update` | `image.zig` `prepKittyImages`/update | **DEFERRED → Kitty-pixel tier.** The golden renders **no Kitty images**. `draw_frame` calls `self.images.upload(...)` over an **empty** `images` map (`image::State::empty()`), which iterates zero keys — `kitty_update`/`prep_*` are never entered. Off-path. |
| 11 | `image.rs:541` `prep_kitty_placement` | `image.zig` `prepKittyPlacement` | **DEFERRED → Kitty-pixel tier.** Called only from `kitty_update` (a comment-level caller, `#[allow(dead_code)]`). No Kitty images in the golden. Off-path. |
| 12 | `image.rs:610` `prep_kitty_virtual_placement` | `image.zig` `prepKittyVirtualPlacement` | **DEFERRED → Kitty-pixel tier.** Same as #11. Off-path. |
| 13 | `image.rs:736` `prep_kitty_image` | `image.zig` `prepKittyImage` | **DEFERRED → Kitty-pixel tier.** Called only from the Kitty seams. Off-path. |
| 14 | `image.rs:1059` `Image::convert` (wuffs swizzle) | `image.zig` `Image.convert` | **DEFERRED → Kitty-pixel / image-upload tier.** Reached only via `Image::prep_for_upload` for a **pending non-RGBA** image. The golden uploads no images at all (empty map + `bg_image = None`). Off-path. |
| 15 | `overlay.rs:270` `highlight_hyperlinks` | `Overlay.zig` `highlightHyperlinks` | **DEFERRED → overlay-render tier.** The golden sets `overlay: None`; the overlay render is never invoked. Needs a z2d AA/hairline `Context` + `RenderState` row/cell metadata. Off-path. |
| 16 | `overlay.rs:363` `highlight_semantic_prompts` | `Overlay.zig` `highlightSemanticPrompts` | **DEFERRED → overlay-render tier.** Same as #15. Off-path. |
| 17 | `overlay.rs:444` `highlight_grid_rect` | `Overlay.zig` `highlightGridRect` | **DEFERRED → overlay-render tier.** Called only from the `highlight_*` seams (`#[allow(dead_code)]`). Off-path. |
| 18 | `overlay.rs:508` `highlight_pixel_rect` | `Overlay.zig` `highlightPixelRect` | **DEFERRED → overlay-render tier.** Same as #17. Off-path. |

### 1.3 Reachability proof (the load-bearing part — no seam is a hidden bug)

The golden test's ported call chain is: `golden_renderer` (direct struct construction over
`Metal::init_offscreen` + `SwapChain::init` + `init_shaders`; **not** `Renderer::init`) →
`update_frame(&st, true)` → `rebuild_cells` → `draw_frame(true)`. The tested state is
pinned: `overlay = None`, `state.inspector = None` (uninhabited), `images = empty`,
`bg_image = None`, `has_custom_shaders = false`, `search_matches = None`, `sync = true`,
`size_changed = false`.

Walking `draw_frame` (`generic.rs:3966`+) against those pins, each branch that *could*
touch a seam is statically skipped:

- **`Renderer::init` / `deinit` (#1/#2)** — not called (direct construction; no `Drop`).
- **async present (#7)** — `sync = true` ⇒ the synchronous `complete` branch; the
  `!sync` block (`frame.rs:198`) is dead on this path.
- **`images.upload` → `convert`/`prep_kitty_*` (#10–#14)** — `image::State::empty()` ⇒
  `self.images.keys()` is empty ⇒ the upload loop body (and thus `convert`) never runs.
- **`upload_background_image` (bg path)** — `bg_image = None` ⇒ `upload_background_image`
  (`generic.rs:3895`) returns `Ok(())` immediately; `prep_background_image` (an
  `init`-only call) is never reached.
- **overlay `highlight_*` (#15–#18)** — `overlay = None` ⇒ the overlay render is skipped.
- **inspector `overlayFeatures` (#4)** — the golden pins `state.inspector = None`
  (`generic.rs:5012`) and no ported path constructs a non-`None` inspector, so the
  `Some(_insp)` arm (which only *binds*, never derefs, the `*mut Inspector` to the
  uninhabited `enum {}`) is never taken. The pin, not "unconstructibility", is the proof.
- **`DerivedConfig::init` / `Message::initChangeConfig` / `Thread::DerivedConfig::init`
  (#3/#8/#9)** — config-coupled, reached only from the cross-thread `ChangeConfig`
  mailbox / core config path, none of which the offscreen golden exercises.

**Conclusion: 0 REACHABLE-BUG seams. All 18 are legitimately deferred R5 seams**, each
with a named Goal-2 tier and a verbatim body preserved (R1/R5). No blocker to surface.

The deferred surface groups into **five named tiers**:
- **Windowing-host / apprt-surface** (#1, #2, #5, #6, #7): `Renderer::init`/`deinit`,
  `Metal::init`/`loop_enter`, async present, the display-link + real CAMetalLayer present
  path. (These are the `changeConfig`/`setScreenSize`/`setFontGrid` resize + config-swap
  and the windowed layer-present milestone.)
- **Renderer↔config / Thread-event-loop (G7)** (#3, #8, #9): config-coupled `init` bodies
  behind the renderer→config cycle wall.
- **rustty-inspector** (#4): inspector overlay features.
- **Kitty-pixel / image-upload** (#10–#14): the Kitty graphics + wuffs swizzle path.
- **Overlay-render** (#15–#18): the z2d debug-overlay rect draws.

Not represented by an `unimplemented!()` in this crate but still deferred surface (named
for the honest status text):
- **`shadertoy::load_from_files`** and **`load_from_file`** (the custom-shader file load,
  G7 renderer→config cycle) — these are NOT live Rust fns at all: they are **fully
  commented-out verbatim reference blocks** (`// pub fn loadFromFiles(` at
  `shadertoy.rs:132`, `// pub fn loadFromFile(` at `:163`). That is even *safer* than a
  seam macro — there is no callable body that could silently misbehave — so it is
  precisely "verbatim-preserved reference, commented out; no live fn", not a "plain
  deferred fn".
- **`prep_background_image`** — likewise NOT an independent live fn (`grep` finds no
  `fn prep_background_image`); it exists only as a reference call *inside* the deferred
  `Renderer::init` seam (`generic.rs:1308/1319`), so it is **subsumed by seam #1**, not a
  standalone deferred fn.
- the **OpenGL/Linux backend** (entirely absent — monomorphic Metal-only port), and
  **GTK** (a different crate/tier).

---

## §2 The PORT STATUS / G3b DECISION (the crux)

### 2.1 The mechanism

`check_comment_coverage.py` (G3b, `.port/tools/gates/check_comment_coverage.py:142-146`)
activates comment-coverage on a crate **iff** its `lib.rs` matches `PORT STATUS:\s*done`:

```python
done = any("PORT STATUS:" in ... and re.search(r"PORT STATUS:\s*done", ...)
           for f in rust_files if f.endswith("lib.rs"))
if not (args.strict or done or args.crate or scope_port is not None):
    continue        # crate SKIPPED
```

`rustty-renderer/src/lib.rs:7` currently says `PORT STATUS: in progress (R-a headless
slice)`, so the crate is **SKIPPED** by G3b today. Flipping it to `done` would turn on a
whole-crate fuzzy match of every ≥5-word reference comment against the port's comments.

### 2.2 The decision — **KEEP an honest "in progress"** (do NOT flip to `done`)

`PORT STATUS: done` is a **semantic claim that the crate is wholly ported**. It is not:
18 live seams across five named tiers plus the OpenGL backend, GTK, resize/config-swap,
the windowed present path, `shadertoy` file-load, and bg-image I/O are all deferred. The
renderer's **offscreen R-d slice is done**; the **crate is not**. Flipping the flag purely
to activate a gate would be gaming the honesty marker for gate-green — exactly what the
brief and PORT_RULES forbid ("Do NOT weaken honesty for gate-green"). **Recommendation:
keep `in progress`, with an updated, accurate status body (§2.4).**

This is consistent with the established heavy-tier doctrine (memory
`heavy-tier-strategy`: *"GPU/GTK accept no unit-gate; compile + golden-image / live-session
diff"*): the GPU tier's R1 backstop is the **adversary + verify panel + the golden gate**,
not G3b comment-coverage. Chunk 4's own design doc already states this explicitly ("G3b
comment-coverage SKIPS `rustty-renderer` … the adversary + verify panel are the ONLY R1
backstop. Preserve EVERY reference line/comment verbatim by hand"). Chunk 5 does not
change that contract; it records it honestly.

### 2.3 Alternatives considered (and why rejected)

- **Flip to `done` + make the whole crate comment-coverage-clean.** Rejected on honesty
  (the crate is not done) AND cost: it would force a real whole-crate coverage pass over
  `generic.rs` (5000+ loc) for a claim that is false. Honesty is the deciding factor; the
  cost merely confirms it.
- **A scoped `PORT STATUS: done` sub-marker** (e.g. per-module). Rejected: the gate keys
  off `lib.rs` only; there is no per-module honest-done marker, and inventing one to
  half-activate the gate re-introduces the same honesty smell at finer grain.

### 2.4 The AVAILABLE scoped mechanism (recommended as tooling, NOT as a status flip)

The gate already supports a **scope override** without touching the honesty flag:
`--port-glob` + `--ref-files` (`check_comment_coverage.py:131-136`) run the *identical*
coverage logic restricted to a chosen module ↔ reference pair, independent of
`PORT STATUS`. So the completed offscreen-slice files CAN be coverage-checked in isolation
— e.g.:

```
python3 .port/tools/gates/check_comment_coverage.py \
  --port-glob 'crates/rustty-renderer/src/{generic,shaders,metal/*}.rs' \
  --ref-files src/renderer/generic.zig,src/renderer/metal/shaders.zig,...
```

**Recommendation:** keep this as an *optional* on-demand assurance run over the done
slice (the adversary already covers R1), and note it in the docs. Do **not** wire it into
CI as a required gate this chunk — the offscreen slice's R1 gate is the golden test + the
adversary, and a scoped coverage run's ref-file mapping is itself review-sensitive.

### 2.5 The exact updated `lib.rs` header text (implementation spec for the port step)

Replace the `PORT STATUS: in progress (R-a headless slice)` block (`lib.rs:7`) and the
`DEFERRED:` block (`lib.rs:36-44`) with an accurate GPU-R-d-aware status. Content to land
(behavior-preserving — a comment edit only; keep the word `in progress`, never `done`):

```
//! PORT STATUS: in progress. Two slices are complete; substantial surface deferred.
//!
//! DONE — the R-a headless slice: cell, cursor, state, shaders, shadertoy (transpile),
//!   link, Health, message, thread (minimal cross-thread surface), size (A1 hoist).
//!
//! DONE — the GPU R-d OFFSCREEN slice (macOS/Metal, Chunks 0-4): the monomorphic
//!   `Renderer` over the concrete `Metal` backend RENDERS a terminal state offscreen.
//!   The whole `metal/*` backend (device/queue/buffer/texture/pipeline/render-pass/
//!   target/frame), `SwapChain`/`FrameState`, `update_frame`, the `rebuild_cells` CPU
//!   cell-fill, the uniforms math, and `draw_frame` (Metal encode+submit → IOSurface
//!   readback) are ported and gated by the golden test
//!   `generic::macos_impl::tests::golden_renders_cursor_fg_bg` ("Ab" + cursor, real
//!   pixel asserts). The `draw_frame` raw-ptr aliasing is proven clean under Stacked
//!   AND Tree Borrows by `.port/phase-2/mirihar-chunk3b`.
//!
//! DEFERRED (named Goal-2 tiers — R5 `unimplemented!()` seams, verbatim bodies kept):
//!   - windowing-host / apprt-surface: `Renderer::{init,deinit}` (live NSView),
//!     `Metal::{init,loop_enter}`, the async `Frame::complete` present half, the real
//!     CAMetalLayer/IOSurfaceLayer + display-link windowed present, and the
//!     resize/config-swap (`changeConfig`/`setScreenSize`/`setFontGrid`).
//!   - renderer<->config (G7 cycle): `Renderer.DerivedConfig::init`,
//!     `Message::initChangeConfig`, `Thread.DerivedConfig::init`,
//!     `shadertoy::load_from_files`, `prep_background_image` file I/O.
//!   - Kitty-pixel: `image` `kitty_update`/`prep_kitty_*`/`convert` (wuffs swizzle).
//!   - overlay-render: the z2d `Overlay` `highlight_*` rect draws.
//!   - rustty-inspector: `overlayFeatures` (uninhabited inspector placeholder).
//!   - OpenGL/WebGL backends (Linux/wasm) and GTK — out of the monomorphic Metal scope.
//!
//! G3b comment-coverage SKIPS this crate until `PORT STATUS: done`; it stays "in
//! progress" while the above is deferred (honest). R1 fidelity for the ported slices is
//! backstopped by the adversary + verify panel + the golden gate (heavy-tier doctrine),
//! and MAY be spot-checked with a scoped `check_comment_coverage.py --port-glob` run.
```

(Exact prose is the port step's to finalize; the load-bearing invariants are: keep
`in progress`, name the offscreen slice as DONE, enumerate the five deferred tiers, and
state the G3b/adversary contract.)

---

## §3 DOCS UPDATE — scope

Mark the offscreen R-d arc complete and record the honest status. Exact files/sections:

### 3.1 `crates/rustty-renderer/src/lib.rs` (header) — **primary**
Land the §2.5 status text. Behavior-preserving comment edit; the only *code* touch of
this chunk. No `#[cfg]`/`pub use`/struct changes.

### 3.2 `.port/phase-2/GPU_RD_BLUEPRINT.md`
- **§8 "Chunked execution plan"** — annotate **Chunk 0-4 as DONE + committed** and
  **Chunk 5** with its actual deliverable (this doc): "seams audited (18 sites, all
  deferred, zero reachable-bug), honest `in progress` retained, docs updated". Fold the
  brief's stale "~27 seams" estimate into "18 actual" if the blueprint repeats it.
- **Risks — "Offscreen ≠ on-screen honesty"** — confirm it is now MET: the golden proves
  the renderer emits correct pixels into an IOSurface texture; it does NOT prove
  CAMetalLayer presentation/vsync/resize (the windowing-host milestone). Keep as the
  standing caveat, now with the golden as evidence.

### 3.3 `.port/phase-2/GPU_RD_CHUNK4_DESIGN.md`
Add a short **"LANDED"** note at the top: golden green, the mirihar SB+TB proof, the
determinism pins (`AlphaBlending::Native` ⇒ `BGRA8Unorm`, `colorspace = Srgb` ⇒
`use_display_p3 = false`, `offscreen_size == size.screen` ⇒ `size_changed == false`).
Point forward to this Chunk-5 doc for the seam audit + status decision.

### 3.4 `.port/phase-2/DEFERRALS.md` — append under `## rustty-renderer R-b/R-c` (line 409)
Add a new subsection **`### rustty-renderer R-d — offscreen Metal draw path (Chunks 0-4) DONE; on-screen + config + Kitty + overlay DEFERRED`**:
- **DONE:** the monomorphic `Renderer<Metal>` renders offscreen; golden-gated
  (`golden_renders_cursor_fg_bg`); the `metal/*` backend + `SwapChain`/`FrameState` +
  `update_frame`/`rebuild_cells`/`draw_frame` + uniforms; the CellText==32 un-defer;
  mirihar SB+TB proof of the `draw_frame` raw-ptr discipline.
- **DEFERRED (the 5 tiers of §1.2):** windowing-host (`Renderer::{init,deinit}`,
  `Metal::{init,loop_enter}`, async present, display-link/CAMetalLayer, resize/config-swap),
  renderer↔config G7 (`DerivedConfig::init`×2, `initChangeConfig`, `shadertoy::load_from_files`,
  bg-image I/O), Kitty-pixel, overlay-render, rustty-inspector, and the OpenGL/WebGL/GTK
  backends. Each `unimplemented!()` keeps its verbatim body (R5).
- **G3b:** crate remains `PORT STATUS: in progress` ⇒ SKIPPED; R1 backstopped by
  adversary + golden; scoped `--port-glob` coverage available on the done slice.

### 3.5 `MEMORY.md` (auto-memory) — optional, if the port step records progress
A one-line index entry is reasonable (e.g. `gpu-rd-offscreen-done` — the offscreen Metal
draw path renders + is golden-gated; on-screen/config/Kitty/overlay deferred; renderer
stays `in progress` so G3b keeps skipping). Non-blocking; the port/commit step decides.

**Not to touch:** the other Chunk design docs (3a/3b/3c/1b/2/P1/TRAIT), `NEXT_PHASE_PLAN`,
and any `crates/` beyond the `lib.rs` header edit. No new gates, no `Cargo.toml` changes.

---

## §4 Summary for the port/verify step

- **Seam audit:** 18 live `unimplemented!()` call sites (`generic.rs` 4, `metal/*` 3,
  `image.rs` 5, `overlay.rs` 4, `message.rs` 1, `thread.rs` 1). **All 18 are legitimately
  DEFERRED** to five named Goal-2 tiers, each with a verbatim R5 body. **Zero
  REACHABLE-BUG** — proven by walking `draw_frame` against the golden's static pins
  (`overlay/inspector/images/bg_image = None`, empty images, `sync = true`,
  `size_changed = false`, direct struct construction with no `Drop`). No blocker to
  surface.
- **PORT STATUS:** keep **`in progress`** with the §2.5 accurate body (offscreen slice
  done; five deferred tiers + OpenGL/GTK enumerated). Do NOT flip to `done`.
- **G3b:** stays SKIPPED (honest). R1 backstop = adversary + golden (heavy-tier doctrine).
  Scoped `--port-glob` coverage documented as an optional on-demand check on the done slice.
- **Docs:** land the `lib.rs` header (the sole code touch, behavior-preserving) + annotate
  the blueprint §8/risks, Chunk-4 doc, and a new DEFERRALS.md R-d subsection.
