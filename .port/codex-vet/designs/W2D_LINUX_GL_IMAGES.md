# W2D — Linux OpenGL Image Rendering Completion (F-005 / F-012 / F-020)

Wave-2 design for the three confirmed Linux image-pipeline parity gaps
(`.port/codex-vet/STATE.json` `.vet` of each id):

- **F-005 (P1)** — Kitty graphics are parsed/stored (vt layer, cross-platform) but never
  rendered on Linux: `linux_impl.rs` has no `images` field, no upload, no draw bands.
- **F-012 (P2)** — `background-image` config is accepted and its vertex buffer is synced,
  but there is no decode/upload/draw on Linux.
- **F-020 (P4)** — the inspector's renderer-overlay controls (producer live on Linux)
  have no OpenGL consumer: all 4 renderer-side stages are MILESTONE-SCOPE-CUT.

All three share one root: `crate::image` is macOS-only (it embeds
`crate::metal::texture::Texture`), so `generic/linux_impl.rs` cut every image-typed
field and every arm that needs one. The fix is the established monomorphic-twin move:
**copy-adapt the reviewed macOS twin over the `crate::opengl` leaf**, exactly as
`generic/linux_impl.rs` itself was produced from `macos_impl` and as the w1 F-013
custom-shaders fix does for its slice.

Oracle pins: ghostty `6ad1fe7d` (`GHOSTTY=/Users/gael/projects/ghostty`, READ-ONLY).
Rustty line refs below are at audit pin `dc054bb` == current `main d34cc15` for these
files; expect small drift after w1 lands (see §0.2).

Design ONLY. This document touches no `crates/`.

---

## 0. Ground truth and constraints

### 0.1 What already exists (verified by reading the tree)

| Piece | State | Evidence |
|---|---|---|
| GL `image`/`bg_image` pipelines compiled + linked, GLSL shaders embedded | **LANDED** | `opengl/shaders.rs:84-87` (image.v/f, bg_image.v/f), `:325-326` pipelines, `:378-403` init, vertex-layout asserts `:216-226` |
| GL `ImageTextureFormat` (gray/rgba/bgra) + `image_texture_options(format, srgb)` | **LANDED** | `opengl/opengl_impl.rs:684-702`, `:588-611`; oracle `OpenGL.zig:379-419` |
| GL `image_buffer_options` / `bg_image_buffer_options` | **LANDED** | `opengl/opengl_impl.rs:555-559`; oracle `OpenGL.zig:351-352` |
| GL leaf `Texture::init(opts, w, h, Option<&[u8]>)` / `replace_region` / manual `deinit` | **LANDED** | `opengl/texture.rs:107-188` |
| GL `Buffer::<T>::init_fill` / manual `deinit` | **LANDED** | `opengl/buffer.rs:157,200` |
| GL `RenderPass::step` executes immediately (bind→draw→unbind), `Step { uniforms, buffers, textures, samplers, draw }` | **LANDED** | `opengl/render_pass.rs:128-153`, `:73-85` |
| `linux_impl` `Renderer.bg_image_buffer` + `_modified` + `update_bg_image_buffer()` + FrameState `bg_image_buffer` + per-frame sync | **LANDED** | `linux_impl.rs:326-328, 483-485, 504, 1509-1535, 3596-3603` |
| `linux_impl` `Renderer.overlay: Option<Overlay>` field + deinit | **LANDED** | `linux_impl.rs:359, 951-953` |
| `DerivedConfig.bg_image/bg_image_opacity/_position/_fit/_repeat` (shared, ungated, `RendererConfig` accessor projection) | **LANDED** | `generic.rs:335-339, 470-527` (top-level, shared by both twins) |
| Inspector producer: `InspectorOverlay` trait (renderer-owned) + `impl` in inspector | **LANDED, cross-platform** | `state.rs:52` (trait), `crates/rustty-inspector/src/widgets/renderer.rs:294` (`impl InspectorOverlay for Info`), no `cfg(target_os)` gates |
| `Overlay` (CPU/z2d) module — all render leaves target-independent | **LANDED** | `overlay.rs`; only 3 macOS gates, all for the `image::Pending` type (`:51, :199, :818`) |
| macOS twin: the complete, test-green reference for every arm below | **LANDED** | `generic.rs` (`macos_impl`), `image.rs` |

So the entire remaining work is (a) an OpenGL twin of `crate::image`, and (b) un-cutting
the marked `MILESTONE-SCOPE-CUT` sites in `generic/linux_impl.rs` by copy-adapting the
macOS arms. No shader, pipeline, config, vt, or inspector work is needed.

### 0.2 w1 interaction (F-013 / F-014 / W8-03) — MUST land first

THREE w1 fixes overlap our files (STATE.json `.vet.fix_plan` of each; none landed at
time of writing — `git diff dc054bb..HEAD -- crates/rustty-renderer` is empty):

- **F-013 (custom shaders)** edits `linux_impl.rs` at `:3271` (uniform update in
  `update_frame`), `:3508-3512` (custom-shader-state setup/teardown), `:3550-3554`
  (per-frame uniform sync — **the same comment block that cuts
  `images.upload`/`upload_background_image`**), `:3668-3671` (base-pass attachment
  target — **adjacent to the bg_image draw arm** `:3674-3690`), and `:3742-3744`
  (post passes).
- **F-014 (scrollbar mailbox seam)** edits BOTH twins: it replaces the unit
  `SurfaceMailbox` field with a trait-seam `Option<Box<dyn ...>>` in `generic.rs`
  AND `generic/linux_impl.rs` — i.e. it churns the `Renderer` struct + `Options` +
  the `init` literal, the exact regions our WI-3.2/3.3 insert into, **and it drifts
  the macOS `generic.rs` line refs this doc uses as copy-sources**.
- **W8-03 (GL strict mode)** edits `opengl/gl/test_support.rs`,
  `opengl/opengl_impl.rs:750`, and the `linux_impl.rs` test-region skip arms
  (`:3970/:4021/:4102/:4167`) — directly adjacent to the test anchors (`:4185+`)
  our new tests cite.

Rules for the fix agents:

1. **Sequence AFTER w1 lands (all three above).** Re-read the landed diffs
   (`git log -p -- crates/rustty-renderer/src/generic/ crates/rustty-renderer/src/opengl/`)
   and re-anchor ALL `linux_impl.rs` line refs AND the macOS `generic.rs` refs
   (F-014 shifts struct/init/`update_frame` regions in both files); the cut-comment
   at `:3550-3554` will have been split by F-013 (its custom-shader third is
   replaced by real code; the kitty/bg-image two-thirds remain and are ours to
   replace). Anchor by the `MILESTONE-SCOPE-CUT` marker strings (unique), not line
   numbers (§8.1).
2. The base-pass attachment match (F-013's item 3) and the bg_image-vs-bg_color match
   (our F-012 arm, §4.4) are ADJACENT but disjoint statements — oracle order is:
   attachment select (`generic.zig:1592-1596`), then bg_image/bg_color step
   (`:1611-1628`). Keep that order.
3. W8-03 adds `RUSTTY_REQUIRE_GL=1` strict mode to the clean-skip arms in
   `test_support.rs`/`linux_impl.rs`. New live-GL tests added here MUST use the same
   helper/skip discipline so they are counted by that accounting (see §7).
4. **w2 siblings**: F-002/F-003 (wave-2, designs `W2D_WINDOWS_OS.md`/`W2D_FREEBSD.md`,
   written concurrently with this doc) also touch `lib.rs`, `generic.rs`,
   `generic/linux_impl.rs`, and `rustty-renderer/Cargo.toml`. Before executing WI-1,
   read those designs: if they widen the renderer's second cfg arm beyond
   `target_os = "linux"` (e.g. to include FreeBSD), the `image` dispatcher, the
   `lib.rs` gate, and the `overlay.rs` gates in §0.3 must use the SAME predicate as
   `generic.rs`'s dispatcher arm — the invariant is "`image`'s cfg == `generic`'s
   non-macOS arm cfg", not the literal `target_os = "linux"` spelled below.

### 0.3 cfg strategy (locked)

- `crate::image` becomes a **cfg twin pair**, dispatch identical to `generic.rs`:
  - `git mv src/image.rs src/image/macos_impl.rs` (zero content churn, history
    preserved).
  - New thin `src/image.rs` dispatcher:
    ```rust
    #[cfg(target_os = "macos")] mod macos_impl;
    #[cfg(target_os = "macos")] pub use macos_impl::*;
    #[cfg(target_os = "linux")] mod linux_impl;
    #[cfg(target_os = "linux")] pub use linux_impl::*;
    ```
  - New `src/image/linux_impl.rs` — full copy-adapt (see §2). Per the
    generic-twin doctrine, we duplicate wholesale rather than factoring a shared
    core: ghostty's `Image(GraphicsAPI)` is a comptime instantiation, and the repo's
    locked pattern (module header of `generic/linux_impl.rs`, F-013) is monomorphic
    twins with dual citations, not trait-generic abstraction.
- `lib.rs:139-140`: `#[cfg(target_os = "macos")] pub mod image;` →
  `#[cfg(any(target_os = "macos", target_os = "linux"))] pub mod image;`.
- `overlay.rs:51, :199, :818`: the three `#[cfg(target_os = "macos")]` gates (all
  exist ONLY because `image::Pending` was macOS-only — the port notes at `:195-199`
  say exactly this) become `#[cfg(any(target_os = "macos", target_os = "linux"))]`.
- Windows / FreeBSD / other targets: unaffected — `image` stays un-compiled there
  (same `any(macos, linux)` gate as `generic`'s consumers), `Renderer` remains the
  WebGL alias (`lib.rs:198`). Verified by the check-only lanes (§7).

### 0.4 GL-specific fidelity decisions (answering the assignment's format questions)

- **Pixel-format expansion is CPU-side and already ported.** Oracle
  `image.zig:854-876 convert()` swizzles gray/gray-alpha/rgb/bgr/bgra → RGBA before
  any GPU touch; `Image::upload` always creates the texture as
  `imageTextureOptions(.rgba, true)` (`image.zig:884-920`). The rustty macOS twin
  ports `convert` as pure-Rust per-pixel loops with the reference's
  PREMUL→PREMUL semantics (straight channel copy/expand, NO alpha multiply —
  `image.rs:1368-1502`, note at `:1394`). The GL twin keeps this code verbatim
  (it is API-free); only the `Texture::init` call swaps leaf.
- **`GL_UNPACK_ALIGNMENT`: no work needed, by oracle fidelity.** The oracle sets no
  pixel-store state anywhere in `src/renderer/` (grep `pixelStore|UNPACK` = 0 hits);
  it is safe because image uploads are always RGBA (4 bpp ⇒ every row is 4-byte
  aligned at the GL default alignment of 4). The GL twin's `upload` keeps the
  existing `assert(p.pixel_format == Rgba)`-by-construction (post-`prep_for_upload`)
  and adds a code comment recording this invariant. Do NOT add `pixel_store` calls.
- **sRGB**: `image_texture_options(ImageTextureFormat::Rgba, /*srgb=*/true)` →
  `internal_format = SRGBA`, faithful to `OpenGL.zig:398-419` and identical in shape
  to the macOS call (`image.rs:1531`).
- **Manual GL resource lifetimes.** Unlike Metal (ARC/`Retained`, textures freed by
  `Drop`), the GL leaves have manual `deinit` and NO `Drop` (locked doctrine,
  `generic/linux_impl.rs` module header). Consequences the GL image twin must port
  for real:
  - `Image::deinit` explicitly `texture.deinit()`s the `Ready`/`UnloadReady`/
    `Replace{texture,..}` arms (macOS lets the field drop).
  - `State::draw`'s per-placement transient vertex buffer gets an explicit
    `buf.deinit()` after `pass.step(..)` (the Zig `defer buf.deinit()`,
    `image.zig:169`); safe because GL `RenderPass::step` executes the draw
    immediately (`render_pass.rs:128-153`).
  - `Renderer::deinit` must free `images` + `bg_image` **while the owned GL context
    is current** — i.e. insert at `linux_impl.rs:977` (the `(821-823)` placeholder
    comment), before `deinit_shaders()`/`api` drop. Same ORDER-IS-LOAD-BEARING rule
    the header already documents for `swap_chain`.
- **Uniforms in the image draw steps**: the oracle passes NO uniforms in
  `images.draw` steps (`image.zig:170-178`) — on GL the `Global` UBO binding at
  index point 1 persists across program switches from the earlier bg/cell steps of
  the same pass. Copy faithfully (`uniforms: None`); do not "fix" by threading
  uniforms.
- **`State::draw` takes `pass: &mut RenderPass`** (GL `step` is `&mut self`, bumps
  `step_number` — `render_pass.rs:94-153`), vs Metal's `&RenderPass`. Same deviation
  `linux_impl.rs` already documents at `:3664-3667`.

---

## 1. Work item order

Per the assignment: **upload → draw → eviction → bgimage → overlay**, with the twin
module first since everything depends on it. Eviction is not a separate function —
it is the `is_unloading ⇒ deinit + remove` arm inside `State::upload`
(`image.zig:65-92`) plus unload marking from `kitty_update`; it lands with WI-2 and
gets its own test in WI-3.

| # | Item | Files | Findings |
|---|---|---|---|
| WI-1 | `crate::image` cfg-twin restructure (no behavior change) | `lib.rs`, `image.rs`→`image/macos_impl.rs`, new `image.rs` | all |
| WI-2 | `image/linux_impl.rs` — GL twin: state machine, upload+eviction, draw, kitty walk, overlay_update | `image/linux_impl.rs`, `overlay.rs` | F-005, F-020 |
| WI-3 | `generic/linux_impl.rs` — kitty wiring: fields, update_frame, upload site, 3 draw bands, deinit | `generic/linux_impl.rs` | F-005 |
| WI-4 | `generic/linux_impl.rs` — background image: prep/change_config/init/upload/draw arm | `generic/linux_impl.rs` | F-012 |
| WI-5 | `generic/linux_impl.rs` — inspector overlay consumer: 4 stages | `generic/linux_impl.rs` | F-020 |
| WI-6 | Ledger repair + header updates | `.port/phase-2/DEFERRALS.md`, `linux_impl.rs` headers | all (bookkeeping) |

Each of WI-3..5 is independently commit-able and independently verifiable; do not
merge them into one mega-commit (per-sub-batch gating doctrine).

---

## 2. WI-1 + WI-2 — the `crate::image` OpenGL twin

### 2.1 WI-1: restructure (mechanical)

1. `git mv crates/rustty-renderer/src/image.rs crates/rustty-renderer/src/image/macos_impl.rs`.
2. New `crates/rustty-renderer/src/image.rs` dispatcher (§0.3), with a module doc
   mirroring `generic.rs:7601-7612`'s twin-dispatch comment.
3. `lib.rs:139-140` gate → `any(macos, linux)`.
4. Verification: `cargo test -p rustty-renderer` on macOS host — **must be a
   no-diff-in-behavior refactor** (all image/golden tests green);
   `cargo check --target x86_64-pc-windows-msvc -p rustty-renderer` and
   `--target x86_64-unknown-freebsd` still green (module not compiled there).

### 2.2 WI-2: `image/linux_impl.rs`

Copy-adapt `image/macos_impl.rs` (1829 lines) — READ-ONLY reference — with the
API glue swapped. Every `fn` keeps dual citations (`image.rs::macos_impl` range +
`image.zig` range), same as `generic/linux_impl.rs` does. Import swap table:

| macOS twin (`image/macos_impl.rs:56-61`) | GL twin |
|---|---|
| `crate::metal::api::MTLPrimitiveType` | `crate::opengl::render_pass::Primitive` (as `linux_impl.rs:3686` uses) |
| `crate::metal::buffer::Buffer` | `crate::opengl::buffer::Buffer` |
| `crate::metal::metal_impl::{ImageTextureFormat, Metal}` | `crate::opengl::opengl_impl::{ImageTextureFormat, OpenGL}` |
| `crate::metal::pipeline::Pipeline` | `crate::opengl::pipeline::Pipeline` |
| `crate::metal::render_pass::{Draw, RenderPass, Step}` | `crate::opengl::render_pass::{Draw, RenderPass, Step}` |
| `crate::metal::texture::Texture` | `crate::opengl::texture::Texture` |

All other imports (vt kitty modules, `rustty_util`, `crate::shaders`,
`crate::size::CellSize`, `crate::overlay::Overlay`, allocator plumbing) are
unchanged. Item-by-item porting notes (oracle `image.zig`, macOS `image/macos_impl.rs`):

1. **Pure/unchanged verbatim** (API-free; copy, keep citations): `Placement` (:1069),
   `DrawPlacements` (:1096), `Id`/`z_less_than` (:1119-1160), `ImageMap`/`ImageEntry`
   (:1162), `Pending`/`PixelFormat`/`bpp` (:1217-1277), `Replace` (:1202),
   `WuffsError`/`UploadError` (:1617-1638), raw alloc helpers (:1640-1690),
   `convert`/`prep_for_upload` (:1368-1514), `mark_for_unload`/`mark_for_replace`/
   `is_pending`/`has_texture`/`is_unloading`/`get_pending`/`get_pending_pointer`
   (:1297-1366, :1556-1614), `State::empty`/`kitty_requires_update` (:96-113,
   :324-353), the four kitty terminal-walk fns `kitty_update`/`prep_kitty_placement`/
   `prep_kitty_virtual_placement`/`prep_image`/`prep_kitty_image` (:355-1067; oracle
   `image.zig:237-624`), `overlay_update` (:270-322; oracle `:183-235`).
2. **`Image` enum + `deinit` + `get_texture`/`into_texture`** (:1179-1200,
   :1280-1295, :1573-1594; oracle `:702-790, :939-952`): variant payloads become
   `opengl::texture::Texture`; `deinit` gains explicit `texture.deinit()` in the
   `Ready`/`UnloadReady`/`Replace`/`UnloadReplace` arms (§0.4). NOTE: pending-data
   frees stay exactly as the macOS twin (raw alloc, no Drop).
3. **`Image::upload`** (:1516-1550; oracle `:884-920`): swap
   `api: &Metal` → `api: &OpenGL`; `Texture::init(api.image_texture_options(
   ImageTextureFormat::Rgba, true), w, h, Some(p.data_slice()))` — GL leaf takes the
   same shape (`texture.rs:107`). On error → `UploadError::UploadFailed` (unchanged).
4. **`State::upload`** (:140-180; oracle `:65-92`): signature
   `(&mut self, alloc: A, api: &OpenGL) -> bool` — the macOS twin returns plain
   `bool` (`image.rs:140`), faithful to the oracle's `bool` (`image.zig:65-92`);
   keep that exact surface. The
   unloading arm (`img.deinit(alloc)` + map remove) is THE eviction path; with the
   GL `deinit` from item 2 this now really deletes GL textures.
5. **`State::draw`** (:182-258; oracle `:105-180`): `api: &OpenGL`,
   `pipeline: &Pipeline`, `pass: &mut RenderPass`; per-placement
   `Buffer::<shaders::Image>::init_fill(api.image_buffer_options(), &[...])`;
   `Step { pipeline, uniforms: None, buffers: &[Some(buf.buffer)], textures:
   &[Some(texture)], samplers: &[], draw: Draw { kind: Primitive::TriangleStrip,
   vertex_count: 4, instance_count: 1 } }` (GL buffers are `Copy` handles —
   `buf.buffer`, not `buf.raw()`; see `linux_impl.rs:3679-3733` precedent); explicit
   `buf.deinit()` after the step (§0.4).
6. **`State::deinit`** (:114-138; oracle `:49-57`): unchanged shape; per-image
   `deinit` now frees GL textures — document that callers must hold a current
   context.
7. **Tests in-file** (twin of `image/macos_impl.rs:1708+` smoke tests): the
   empty-map lifecycle test using `OpenGL`'s offscreen context via
   `test_support::gl_lock()` + clean-skip (`golden_renderer` pattern,
   `linux_impl.rs:4213-4220`), plus pure tests (no GL): `Pending::len`,
   `z_less_than`, `DrawPlacements` slicing on a hand-built `State`.

**Verification (WI-2):**
- macOS host: `cargo test -p rustty-renderer` (macOS twin untouched, stays green);
  `cargo check` cross-target lanes (§7.1).
- rvm: `cargo test -p rustty-renderer image::` — pure tests run everywhere;
  GL-context tests run under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe), clean-skip
  otherwise, strict under `RUSTTY_REQUIRE_GL=1` (§7.2).

---

## 3. WI-3 — Kitty wiring in `generic/linux_impl.rs` (F-005)

All sites carry `MILESTONE-SCOPE-CUT` markers today; each replacement is a
copy-adapt of the cited macOS lines with the §0.4 GL deltas. Oracle cites at pin.

| # | Site (linux_impl.rs @ dc054bb) | Replace with (macOS `generic.rs`) | Oracle (`generic.zig`) |
|---|---|---|---|
| 3.1 | Module header divergence #2 (`:25-31`) + struct-note (`:240-248`) | rewrite: `images`/`bg_image` now live | — |
| 3.2 | `Renderer` fields (struct at `:249`; insert beside `bg_image_buffer :326`) | `images: image::State`, `bg_image: Option<image::Image>`, `bg_image_changed: bool` (`generic.rs:1271-1282`) | `:178-189` |
| 3.3 | `init` literal (`:885-924`) | `images: image::State::empty(), bg_image: None, bg_image_changed: false` — copy the macOS twin's REAL init literal (`generic.rs:2048-2051`; test-init twin `:6275-6277`). NOTE: the oracle struct-decl default is `bg_image_changed: bool = true` (`generic.zig:189`), but that field is write-only vestigial in the oracle (only decl `:189` + a shadowing local in `changeConfig` `:1881`); the macOS twin sets `false` and relies on `init` calling `prep_background_image` directly (oracle `:799` / macOS `:2071`) — WI-4.1 restores that same call on Linux, so follow the macOS twin | `:178-194` |
| 3.4 | `deinit` placeholder comment (`:977`) | `self.images.deinit(self.alloc);` + `if let Some(img) = self.bg_image.take() { img.deinit(self.alloc) }` (`generic.rs:2160-2170`) — **before `deinit_shaders`, context current** (§0.4) | `:825-827` (deinitShaders `:829`) |
| 3.5 | `update_frame` kitty cut (`:3155-3156`) | `kitty_requires_update`/`draw_mutex.lock`/`kitty_update(alloc, &*t, CellSize{grid_metrics…})`/unlock (`generic.rs:4588-4608`) | `:1229-1248` |
| 3.6 | `draw_frame_raw` upload cut (`:3550-3554`, post-w1 re-anchored) | `(*self_ptr).images.upload((*self_ptr).alloc, &(*self_ptr).api)` ignore-err arm (`generic.rs:5670-5678`) — `upload_background_image` half lands in WI-4 | `:1548-1549` |
| 3.7 | below-bg band cut (`:3692`) | `images.draw(&api, &shaders.pipelines.image, &mut pass, DrawPlacements::KittyBelowBg)` (`generic.rs:5875-5882`) | `:1631-1637` |
| 3.8 | below-text band cut (`:3710`) | same, `KittyBelowText` (`generic.rs:5900-5908`) | `:1647-1653` |
| 3.9 | above-text half of cut (`:3735-3736`) | same, `KittyAboveText` (`generic.rs:5935-5943`) — overlay half is WI-5 | `:1673-1679` |

Raw-pointer discipline: the draw bands run inside the `self_ptr`/`frame` raw-root
region (`:3556-3558` note) — follow the macOS twin's `(*self_ptr).images.draw(...)`
shape verbatim; do NOT introduce `&mut self` reborrows across the pass
(rust-mut-args / LIFETIMES doctrine). `pass` is already `mut` on GL.

**New tests (in `linux_impl.rs` tests mod):**
- `kitty_image_pixel_path` — twin of macOS `generic.rs:7139` (offscreen APC-driven:
  transmit+place a 1×2 kitty image through `Terminal`'s APC stream after pinning
  `t.width_px`/`height_px` to the renderer's cell metrics, then `update_frame` +
  `draw_frame(true)`). BE HONEST about what the macOS twin asserts: it asserts
  STATE, not pixels — (a) `!r.images.kitty_placements.is_empty()` (`:7183-7186`),
  (b) the `Id::Kitty(1)` entry is pending-or-textured after `update_frame`
  (`:7189-7198`), (c) `storage.dirty` cleared (`:7200-7204`), (d) `has_texture()`
  after `draw_frame` (`:7208-7220`). Port those assertions verbatim. A pixel
  readback at the placement (`Target::read_rgba` + Y-flip per the golden) MAY be
  added as a Linux-only strengthening, but it must follow the structural-threshold
  philosophy (§8.2) and be commented as EXTRA (no macOS-twin analogue). Live-GL:
  gl_lock + clean-skip per the existing golden (`:4185-4260`).
- `kitty_eviction_removes_image` (new, GL — no macOS-twin analogue, comment it as
  such): after the pixel-path draw, send the APC delete-all (`\x1b_Ga=d\x1b\\`),
  `update_frame` (marks unload), `draw_frame` (upload loop evicts), assert
  `r.images.images.is_empty()`. (Covers upload→draw→**eviction**,
  oracle `image.zig:73-78`.)
- Pure (non-GL, runs on any Linux): placement-math assertions via
  `images.kitty_update` on a headless `Terminal` — assert `kitty_bg_end`/
  `kitty_text_end` splits and `Placement` fields for a known placement (mirrors what
  the macOS twin exercises inside `:7136` before the draw).

**Verification commands (WI-3):** §7.2 rvm loop; on macOS
`cargo test -p rustty-renderer` must stay green (twin untouched).

---

## 4. WI-4 — Background image (F-012)

| # | Site (linux_impl.rs) | Replace with (macOS `generic.rs`) | Oracle |
|---|---|---|---|
| 4.1 | `init` cut (`:934-935`) | `result.prep_background_image()?;` after `update_bg_image_buffer()` (`generic.rs:2070-2071`) | `:798-799` |
| 4.2 | `change_config` cut (`:1253-1254`) + the doc note `:1205-1212` | `bg_image_changed` Option-match compare (`generic.rs:2553-2558`) + `if bg_image_changed { prep_background_image() }` arm (`:2573-2588`) — keep the existing `bg_image_config_changed → update_bg_image_buffer` (`:1261-1266`) exactly where it is | `:1878-1897` |
| 4.3 | `draw_frame_raw` upload site (with 3.6) | `upload_background_image()?` (`generic.rs:5680`); port the fn itself from `generic.rs:5336-5397` (unload/replace/pending → `bg.upload(self.alloc, &self.api)`) | `:1551-1552`, fn `:1832-1895` |
| 4.4 | bg draw arm (`:3674-3690`, post-w1 re-anchored) | replace the unconditional `bg_color` step with the `match self.bg_image { Some(Ready(texture)) => bg_image step, Some(_) => {}, None => bg_color step }` (`generic.rs:5838-5870`), GL `Step` deltas: `uniforms: Some((*frame).uniforms.buffer)`, `buffers: &[Some((*frame).bg_image_buffer.buffer)]`, `Primitive::Triangle`, `vertex_count: 3` | `:1611-1628` |
| 4.5 | port `prep_background_image` itself | `generic.rs:2623-2755` (open path bytes → `FileType::detect`/`guess_from_extension` → `rustty_wuffs::{png,jpeg}::decode` → `Pending` → `mark_for_replace`/set `bg_image`); fully API-free — verbatim copy with citations | `:1746-1830` |

Nothing config-side is needed: `DerivedConfig.bg_image` and the `RendererConfig`
accessors are shared and already live on Linux (`generic.rs:335-339, 470-527`;
F-012 evidence "test ctor linux_impl.rs:3854"). `Error::ImageDecode`: **verified
ABSENT from the linux twin** — its `Error` enum (`linux_impl.rs:118-135`) has only
`GlFailed`/`OutOfMemory`/`Defunct`, and its doc comment (`:123-124`) explicitly
says "The `ImageDecode` variant … has no analogue here — bg-image is
milestone-cut". WI-4 MUST add the `ImageDecode` variant + the wuffs-error mapping
arms, twin of `generic.rs:951-983`, and rewrite that stale doc-comment sentence.

**New tests:**
- `golden_renders_background_image` — twin of macOS `generic.rs:6915` (`draw_center`
  helper: temp solid-color PNG encoded at test time, `config.bg_image =
  Some(Path::Required(...))`, `prep_background_image`, `update_bg_image_buffer`,
  draw, read back center pixel with/without bg; assert `Pending → Ready` transition
  as macOS `:6939-6966` does). Live-GL, clean-skip, Y-flip.
- Fit/tile/opacity coverage: the fit/position/repeat/opacity → `BgImageInfo` mapping
  is CPU-side (`update_bg_image_buffer`, already unit-tested on linux twin — macOS
  twin test `update_bg_image_buffer_maps_config` `generic.rs:7404-7416`; add the
  linux twin test if absent). GPU-side fit/tile sampling is the shader's job
  (`bg_image.v/f.glsl`, already ported); the golden asserts the Stretch case
  end-to-end like the macOS golden does. Do not invent per-mode pixel goldens the
  macOS twin doesn't have.

**Verification:** §7.2; plus macOS `cargo test -p rustty-renderer
golden_renders_background_image` stays green.

---

## 5. WI-5 — Inspector overlay consumer (F-020)

Producer (`rustty-inspector/src/widgets/renderer.rs:114-213` checkboxes,
`impl InspectorOverlay for Info` at `:294`) and the seam
(`state.rs:52` renderer-owned trait; `State.inspector: Option<*mut dyn
InspectorOverlay>`, shared) are already cross-platform. Wire the 4 renderer stages:

| # | Site (linux_impl.rs) | Replace with (macOS `generic.rs`) | Oracle |
|---|---|---|---|
| 5.1 | `Critical` struct (`:3058`) | add `overlay_features: Vec<crate::overlay::Feature>` (`generic.rs:4470`) | `:1269-1283` |
| 5.2 | features cut (`:3172-3173`) | `match state.inspector { None => Vec::new(), Some(insp) => unsafe { (*insp).overlay_features() } }` inside the mutex-guarded critical section + `overlay_features` in the `Critical` literal (`generic.rs:4630-4655`) | `:1269-1283` |
| 5.3 | rebuild cut (`:3229-3231`) | `let _ = self.rebuild_overlay(&critical.overlay_features);` (`generic.rs:4733`); port `rebuild_overlay` from `generic.rs:4317-4410` — pure CPU/z2d, API-free, verbatim | call `:1356-1365`, fn `:2230+` |
| 5.4 | overlay upload cut (`:3268-3269`) | `let _ = self.images.overlay_update(self.alloc, self.overlay.as_ref());` inside the draw-mutex block (`generic.rs:4796-4801`) | `:1416-1424` |
| 5.5 | overlay draw half of `:3735-3736` | `if self.overlay.is_some() { images.draw(..., &mut pass, DrawPlacements::Overlay) }` after the above-text band (`generic.rs:5945-5957`) | `:1681-1688` |

`overlay.rs` gate lift (§0.3) is a prerequisite (lands with WI-2): `pending_image`
returns the linux twin's `Pending` on Linux — identical POD shape, its existing
host tests (`overlay.rs:818+`) then run on Linux too.

**New tests:**
- `overlay_draws_on_features` (live-GL): a test-local
  `struct FakeInspector(Vec<Feature>)` implementing `InspectorOverlay`, passed via
  `state::State { inspector: Some(&mut fake as *mut state::Inspector), .. }`
  (`pub type Inspector = dyn InspectorOverlay`, `state.rs:71`; `State` ALSO has the
  separate opaque `inspector_ptr: Option<*mut ()>` field, `state.rs:128` — leave it
  `None`; the golden already sets both to `None`, `:4232-4233`); render a grid
  with a hyperlink/semantic-prompt feature enabled, assert the frame differs from
  the feature-off render and that `r.images.overlay_placements` is non-empty after
  `update_frame`.
- Pure: `rebuild_overlay_builds_and_clears` — twin of macOS `generic.rs:7270-7290`
  (needs a `Renderer`, hence golden_renderer ⇒ live-GL clean-skip on rvm).

---

## 6. WI-6 — Ledger + header repair (bookkeeping, all three findings)

1. `.port/phase-2/DEFERRALS.md`: the "GL image" entry that `linux_impl.rs:29`
   dangling-cites (ordered by L4 plan `:170`, never written — confirmed in all three
   `.vet`s) is now moot for kitty/bg-image/overlay: instead of adding it, write a
   **RESOLVED** row recording the whole GL-image deferral arc (deferred by L4 plan
   `docs/superpowers/plans/2026-07-17-L4-opengl-renderer.md:54-60,143-144,170`;
   resolved by this wave, cite the landing commits), so the ledger drift noted in
   GAP_ANALYSIS.md:22 closes rather than compounds. Also reconcile the R-d section
   conflation flagged in F-020 `.vet.notes` (DEFERRALS.md:545-598).
2. `generic/linux_impl.rs` module header: delete divergence #2 (`:25-31`) and the
   struct-note MINUS-list (`:246-248` clause); `image/macos_impl.rs` header: drop
   the "(GPU R-d, macOS-only)" claim; `lib.rs:135-138` (the comment block above the
   `pub mod image` gate claiming `image::State`'s draw path "is written against the
   `crate::metal` leaves") — rewrite for the twin pair; `overlay.rs` port notes at
   `:195-199` (the "lifts with L4" prophecy) — mark fulfilled.
3. Strike/refresh every now-stale `MILESTONE-SCOPE-CUT` comment the WIs replaced
   (grep `MILESTONE-SCOPE-CUT` in `linux_impl.rs` afterward). Known survivors that
   need REWRITING, not deletion: the two section headers `:3028` and `:3371`
   ("bg_color/cell_bg/cell_text only" — no longer true post-w1+w2) and `:3264`
   (macOS glass background — a compiled-out-on-Linux note, KEEP but retitle so it
   no longer greps as a scope cut). Target end-state: zero `MILESTONE-SCOPE-CUT`
   markers left in the update/draw path — verify and say so in the commit.

Verification: `.port/tools/gates/check_stale_comments.sh` +
`check_comment_coverage.py` if wired for these files; else grep audit in the PR.

---

## 7. Verification plan (per-step commands)

### 7.1 macOS host (every WI)

```sh
cd /Users/gael/projects/rustty
cargo test -p rustty-renderer                       # macOS twin must stay green (no-regression)
cargo check --target x86_64-pc-windows-msvc -p rustty-renderer
cargo check --target x86_64-unknown-freebsd  -p rustty-renderer
cargo check --workspace                             # placeholder-fill backstop (memory doctrine)
```

macOS-runnable shared-logic coverage: the pure pieces (placement math, format
selection, convert/premul, `Pending::len`, bg-image decode length invariant
`generic.rs:713`) are the SAME code in both twins (copy-adapt), and the macOS
twin's existing tests keep them pinned on the host; the Linux copies of those
pure tests (WI-2.7, WI-3 pure, WI-4 buffer-map) run on rvm without any GL.

### 7.2 Linux VM (WI-2..5) — the live-GL lane

```sh
rsync -a --delete --exclude target /Users/gael/projects/rustty/ rvm:~/rustty-vmwork/
ssh rvm 'cd ~/rustty-vmwork && cargo test -p rustty-renderer'                  # pure + clean-skip accounting
ssh rvm 'cd ~/rustty-vmwork && LIBGL_ALWAYS_SOFTWARE=1 RUSTTY_REQUIRE_GL=1 \
         cargo test -p rustty-renderer'                                        # live-GL strict (llvmpipe GL 4.5)
```

(Established pattern: `.port/tools/gates/ws19_portal_openuri_gate.sh`; llvmpipe
finding + strict-mode env from W8-03, landing in w1 — if `RUSTTY_REQUIRE_GL` is not
yet merged when this executes, run the second line without it and eyeball the
"skipping" lines = 0.)

**Live-GL-gated assertions** (clean-skip, counted by W8-03 accounting):
`kitty_image_pixel_path`, `kitty_eviction_removes_image`,
`golden_renders_background_image`, `overlay_draws_on_features`,
`rebuild_overlay_builds_and_clears`, image twin GL lifecycle smoke.
**Never-skipped assertions**: all pure tests in §7.1/WI lists, plus the existing
`golden_renders_cursor_fg_bg` must remain byte-stable (no regression from the new
draw arms when `images` empty / `bg_image` None / overlay None — the empty-state
render path is IDENTICAL to today's by construction: every new arm no-ops on empty
state, same as the oracle).

### 7.3 Optional hardening

A `w2_gl_images_gate.sh` under `.port/tools/gates/` wrapping §7.2 (rsync + both
test invocations + grep-zero-skips), modeled on `ws19_portal_openuri_gate.sh`, so
CI/agents can re-run the lane one-shot. Recommended but not blocking.

---

## 8. Risks / open points

1. **w1 churn**: all `:3xxx` anchors in `linux_impl.rs` shift once F-013/F-014/W8-03
   land, and F-014 also shifts the macOS `generic.rs` copy-source refs.
   Mitigation: WIs anchor to the `MILESTONE-SCOPE-CUT` marker text (unique strings),
   not line numbers; §0.2 rule 1.
6. **w2-internal collisions**: F-002/F-003 (sibling w2 designs) edit `lib.rs` and
   `generic/linux_impl.rs` in the same wave. Whoever lands second must rebase-merge,
   and the `image`-gate predicate must track `generic`'s dispatcher arm (§0.2
   rule 4). If the siblings run CONCURRENTLY with this work, coordinate the `lib.rs`
   edit through one agent.
2. **llvmpipe vs real-GPU pixel drift**: goldens use the structural-threshold
   philosophy (`linux_impl.rs:4197-4205`), never byte-diffs against Metal — new
   tests must follow it (assert regions/colors with tolerance, not exact bytes).
3. **GL context currency in `deinit` paths**: `Image::deinit` frees textures; any
   test that lets `State` outlive its `OffscreenContext` will UB/GL-error. The
   gl_lock + deinit-before-context-drop ordering (module header doctrine) is
   mandatory in every new test; review checklist item.
4. **Overlay `Pending` aliasing**: `overlay_update`'s borrowed-`Option<&Overlay>`
   ownership deviation (PORT NOTE at `image/macos_impl.rs:261-269`, fn at `:270`)
   must be kept IDENTICAL in the twin — do not "simplify" to by-value.
5. **`sync` vs draw of transient buffers**: GL `step` draws immediately, so the
   per-placement buffer create→step→deinit sequence is safe; if a future w
   batches steps, this breaks — noted in the twin's `State::draw` comment.
