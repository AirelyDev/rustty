# R-d — Metal GPU Draw Path Blueprint

**Adversarially reviewed: ACCEPT-WITH-FIXES (2026-07-03) — folded in below** (`[ADV-FIX n]`).
The two make-or-break risks were CONFIRMED FEASIBLE: objc2-metal 0.3.2 covers the full mapped API
(all MTLDevice/CommandQueue/Buffer/Texture/RenderPipelineState/RenderCommandEncoder/RenderPass/
Library methods verified present), and headless offscreen render into an IOSurface-backed
MTLTexture + readback works with no window/WindowServer. Fixes:
- **[F10/F11, SCOPE — biggest] `generic.zig` has 3 unported PREREQUISITES the draft under-counted:**
  (1) `renderer/image.zig` (Kitty-graphics image rendering — generic.zig uploads image textures);
  (2) `renderer/Overlay.zig` (the renderer overlay); (3) a **counting `Semaphore`** —
  `SwapChain.frame_sema: std.Thread.Semaphore` for triple-buffer backpressure — which does NOT
  exist anywhere in the port (grep-confirmed). Add all three to scope: port a small `Semaphore`
  leaf into rustty-zigstd FIRST (before the SwapChain chunk), and scope image.zig + Overlay.zig as
  R-d prerequisites (or the offscreen golden path's minimal subset of them).
- **[F1, DROP] there is NO `MTLCopyAllDevices` gap** — objc2-metal exposes it directly
  (device.rs:17, macOS-cfg'd). Remove the `declare_extern` workaround; `chooseDevice` ports with
  `objc2_metal::MTLCopyAllDevices()` + `isHeadless/isRemovable/isLowPower` (all present).
- **[F3] `api.zig` is NOT a thin alias:** objc2's `MTLResourceOptions`/`MTLTextureUsage` are
  BITFLAGS, vs ghostty's packed struct-of-fields — compose the bits via the typed setters
  (`setResourceOptions`/`setStorageMode`/`setUsage`/`setPixelFormat`). Real translation, not aliasing.
- **[F5, GATE-RELIABILITY] a skip-on-nil golden `#[test]` is VACUOUS on a Metal-less CI runner** (it
  silently passes → R-d can regress undetected). MUST pin a known-Metal runner (self-hosted mac /
  Apple-silicon GH Actions) and make that job REQUIRED / fail-if-skipped. `MTLCreateSystemDefaultDevice()`
  returns `Option` (skip-on-nil is correct for local dev, but not a substitute for a real gate).
- **[F6, HONESTY] the real `present` IS reached** (`drawFrame` → `Frame.complete(sync)` →
  `bufferCompleted` → `present` → `layer.setSurfaceSync`, generic.zig:1590 / Frame.zig:126). The
  offscreen golden STUBS `present` to a no-op + readback and STUBS `surfaceSize()` to a fixed size —
  state these as explicit DEFERRALS, not "the entire draw path runs offscreen unchanged."
- **[F7] do NOT claim the two shader-load paths give the same golden.** The offline `xcrun metal`
  compiler and the runtime `newLibraryWithSource` compiler can differ in float-op ordering (sub-LSB).
  Pin ONE shader-load path to generate AND compare the reference; the other is compile-portability only.
- **[F8, DETERMINISM] `use_display_p3` is driven by `config.colorspace` (generic.zig:739),
  INDEPENDENT of the attachment** — the golden config must pin `colorspace != display-p3` AND
  `min_contrast <= 1.0` AND non-linear blending to zero out the shader's non-deterministic branches.
  A GPU golden needs a small ±1-LSB tolerance (weaker than the CPU-exact sprite precedent — document
  it; a per-channel tolerance could mask a systematic 1-LSB regression, acceptable for a GPU gate).
- **[F12] `Target` IOSurface init needs building a `CFDictionary`** (IOSurfaceWidth/Height/
  PixelFormat/BytesPerElement) via objc2-io-surface `IOSurfaceCreate` — more than "done"; for
  offscreen the DisplayP3 colorspace-set can be skipped (readback is raw bytes). Deps for offscreen:
  objc2-metal + objc2-io-surface + objc2-core-graphics (NOT quartz-core — that's the deferred
  on-screen CAMetalLayer half).

Scope: port `src/renderer/generic.zig` (3374 loc, backend-agnostic core) + `Metal.zig`
(458) + `metal/{api,buffer,Texture,Sampler,Pipeline,RenderPass,Frame,Target,shaders,
IOSurfaceLayer}.zig` + `shaders/shaders.metal` into `rustty-renderer`. macOS/Metal only
(OpenGL.zig = Linux, deferred). This tier is **golden-image gated**, not test-count gated
(heavy-tier-strategy: "GPU/GTK accept no unit-gate; compile + golden-image / live-session
diff"). The blueprint IS the quality gate.

FFI path (established, R3-consistent): the crates.io `objc2-*` framework crates REPLACE
ghostty `pkg/macos` + `pkg/objc`, exactly as `rustty-font-shaper` already replaced them for
CoreText. **Verified**: `objc2` 0.6.4, `objc2-foundation` 0.3.2, `objc2-metal` 0.3.2,
`objc2-io-surface`, `objc2-core-graphics`, `objc2-core-foundation`, `block2`, `dispatch2`
are ALL already in `Cargo.lock` (transitive via the font-shaper CoreText path). The only new
first-order dep R-d must ADD is `objc2-metal` (promote to direct) and, for the deferred
on-screen half only, `objc2-quartz-core` (NOT yet in the lock — CAMetalLayer/CALayer).

---

## TL;DR — the four decisions

1. **Land R-d as "the offscreen Metal draw path + an offscreen golden-image gate."**
   On-screen presentation (CAMetalLayer on a live NSView) is deferred to the windowing-host
   milestone. Offscreen (render a frame to an IOSurface-backed `MTLTexture`, read back BGRA,
   diff a committed PNG) is achievable NOW and proves the renderer emits correct pixels.
2. **Shader load: build.rs `xcrun metal`/`metallib` → `include_bytes!` → `newLibraryWithData`**
   (the byte-faithful mirror of ghostty's `@embedFile("ghostty_metallib")`). Documented
   fallback: `newLibraryWithSource` from `include_str!(shaders.metal)` (identical source, no
   toolchain) — this is *also* ghostty's own custom-shader path, so R3-faithful either way.
3. **Port `generic.zig` whole (both its CPU-agnostic fill loop AND its GPU draw calls);
   drive the golden-image test by calling `drawFrame(sync=true)` directly.** Do NOT port
   `renderer.Thread`'s event loop or the CAMetalLayer `displayCallback` in R-d — the
   offscreen gate needs neither.
4. **Un-defer exactly one unit test**: `size_of::<CellText>() == 32` (the lone `test{}` in
   the R-d reference scope, currently preserved as a comment in `shaders.rs`). Everything
   else in R-d is verified by the golden image.

---

## §1 Achievable-now vs deferred (the framing)

On-screen requires a `CAMetalLayer`/layer-hosting `NSView` supplied by the apprt host — which
does not exist until the windowing milestone. But **IOSurface-backed offscreen rendering needs
no window**: `MTLCreateSystemDefaultDevice()` returns a usable device on a headless/CI mac,
`Target.zig` already renders into an `IOSurface`-backed `MTLTexture` (not a drawable), and
`drawFrame` writes into `frame.target` — a `Target`, never the layer. The layer is touched in
exactly two places (`api.surfaceSize` reads `layer.bounds`; `api.present` sets `layer.contents`).
Stub those two and the entire draw path runs offscreen.

Honest per-file split:

| File | Offscreen-portable NOW | Needs live layer (defer) |
|---|---|---|
| `metal/api.zig` | ✅ all (enums/structs → mostly REPLACED by objc2-metal native types) | — |
| `metal/buffer.zig` (`Buffer<T>`) | ✅ all | — |
| `metal/Texture.zig` | ✅ all | — |
| `metal/Sampler.zig` | ✅ all | — |
| `metal/Pipeline.zig` (+ `autoAttribute`) | ✅ all | — |
| `metal/RenderPass.zig` | ✅ all | — |
| `metal/Target.zig` (IOSurface `MTLTexture`) | ✅ all | — |
| `metal/shaders.zig` (library + pipelines) | ✅ all | — |
| `metal/Frame.zig` | ✅ command buffer + `addCompletedHandler` + `frameCompleted` | `bufferCompleted`→`api.present` is a no-op offscreen (readback replaces present) |
| `Metal.zig` | ✅ device/queue select, `*BufferOptions`, `textureOptions`, `initTarget`, `initShaders`, `beginFrame`, `initAtlasTexture` | `layer` field, `init()` NSView attach (`wantsLayer`/`addSublayer`/`clipsToBounds`), `surfaceSize` (layer.bounds→fixed test size), `present`/`presentLastTarget`, `loopEnter`/`displayCallback` |
| `metal/IOSurfaceLayer.zig` | — (the IOSurface *itself* lives in Target; only the CALayer wrapper is here) | ✅ **fully deferred**: CALayer subclass, `display` method, ivars, `setSurface` main-thread dispatch |
| `generic.zig` | ✅ ~95%: init/deinit, SwapChain/FrameState, `updateFrame` (CPU), `rebuildCells`/`rebuildRow`/`addGlyph`/`addCursor`/`addUnderline`/…, uniforms math, `drawFrame` (renders into `frame.target`) | `DisplayLink`/CVDisplayLink block (`display_link`, `hasVsync`, `setMacOSDisplayID`, `displayLinkCallback`), `loopEnter`/`loopExit`, `surfaceSize`-from-layer coupling |

Deferred surface is small and self-contained: **IOSurfaceLayer.zig in full, the CVDisplayLink
block, and the `~10` layer-attach/present lines in `Metal.zig`/`generic.zig`.** Represent each
with the established inhabited-placeholder / `unimplemented!("windowing-host milestone")` seam
(precedent: `state::Inspector`, `thread::DerivedConfig`), R5-preserving the verbatim body.

---

## §2 Metal FFI mapping (`metal/*.zig` → objc2)

All method names verified present in `objc2-metal` 0.3.2's generated bindings. objc2 renames
ObjC selectors to snake_case with `_` at each colon (e.g. `newBufferWithLength:options:` →
`newBufferWithLength_options`). Objects are `Retained<ProtocolObject<dyn MTLThing>>`; ARC is
automatic (drop = release), so ghostty's manual `retain`/`release`/`errdefer …release()` mostly
**dissolves** into `Retained` ownership (note this per-file as the PORT NOTE).

| ghostty wrapper | objc2 type(s) / method(s) |
|---|---|
| `Metal.device` / `queue` | `ProtocolObject<dyn MTLDevice>` via `MTLCreateSystemDefaultDevice()`; `device.newCommandQueue()` → `dyn MTLCommandQueue` |
| `Metal.chooseDevice` macOS iter | ⚠ **GAP**: bare `MTLCopyAllDevices()` is NOT exposed (only `MTLCopyAllDevicesWithObserver`). See gaps below. |
| device metadata | `device.hasUnifiedMemory()`, `.isHeadless()`, `.isLowPower()`, `.isRemovable()`, `.supportsFamily(MTLGPUFamily::Apple10)` — all present |
| `buffer.zig` `Buffer(T)` | `device.newBufferWithLength_options(len, opts)`, `newBufferWithBytes_length_options(ptr,len,opts)`; `buf.contents()` (`NonNull<c_void>`), `buf.length()`, `buf.didModifyRange(NSRange)` |
| `Texture.zig` | `device.newTextureWithDescriptor(&desc)`; `tex.replaceRegion_mipmapLevel_withBytes_bytesPerRow(...)`; `MTLTextureDescriptor`, `MTLRegion`, `MTLTextureUsage` (bitflags), `MTLPixelFormat` |
| `Target.zig` (IOSurface tex) | `device.newTextureWithDescriptor_iosurface_plane(&desc, &IOSurfaceRef, 0)` (objc2-metal); IOSurface via `objc2-io-surface`; `CGColorSpace` DisplayP3 via `objc2-core-graphics` |
| `Sampler.zig` | `device.newSamplerStateWithDescriptor(&desc)`; `MTLSamplerDescriptor`, `MTLSamplerMinMagFilter`, `MTLSamplerAddressMode` |
| `Pipeline.zig` | `MTLRenderPipelineDescriptor`, `MTLVertexDescriptor` (+ `attributes`/`layouts` indexed subscript), `lib.newFunctionWithName(&NSString)`, `device.newRenderPipelineStateWithDescriptor_error(&desc)` (returns `Result<…, NSError>` — ghostty's `checkError` becomes idiomatic `?`) |
| `Pipeline.autoAttribute` | comptime field→`MTLVertexFormat` reflection. Rust has no comptime struct reflection → **hand-write** an explicit per-struct attribute table (`CellText`, `Image`, `BgImage`) via a small `trait VertexLayout { fn attributes() -> &'static [(MTLVertexFormat, usize)] }` on each vertex struct. Faithful to the *resulting* descriptor; document the comptime→table transform. |
| `RenderPass.zig` | `MTLRenderPassDescriptor::renderPassDescriptor()`, `colorAttachments`; `cmdbuf.renderCommandEncoderWithDescriptor(&desc)` → `dyn MTLRenderCommandEncoder`; `setRenderPipelineState`, `setVertexBuffer_offset_atIndex`, `setFragmentBuffer_offset_atIndex`, `setVertexTexture_atIndex`, `setFragmentTexture_atIndex`, `setFragmentSamplerState_atIndex`, `drawPrimitives_vertexStart_vertexCount_instanceCount`, `endEncoding` — all present |
| `Frame.zig` | `queue.commandBuffer()`; `buf.addCompletedHandler(block)` via `block2::RcBlock`; `buf.status()` (`MTLCommandBufferStatus`), `buf.commit()`, `buf.waitUntilCompleted()` |
| `shaders.zig` library | `device.newLibraryWithData_error(&DispatchData)` (needs `dispatch2` feature, **present**) for the metallib; `newLibraryWithSource_options_error(&NSString, opts)` for custom post-shaders |
| `api.zig` enums | Mostly **REPLACED** by objc2-metal's own `MTLPixelFormat`/`MTLResourceOptions`/`MTLLoadAction`/`MTLStoreAction`/`MTLBlend*`/`MTLPrimitiveType`/`MTLGPUFamily`/`MTLClearColor`/`MTLViewport`/`MTLRegion`/`MTLSize`/`MTLOrigin`. Port `api.zig` as a **thin re-export/alias module** mapping ghostty's enum names to objc2's (R3: the values are identical Apple constants). |

### Genuine objc2 gaps (flag + workaround)

- **`MTLCopyAllDevices()` (bare)** — absent from objc2-metal 0.3.2 (only
  `MTLCopyAllDevicesWithObserver`). Ghostty's `chooseDevice` macOS branch iterates all devices
  skipping headless. **Workaround for R-d**: the offscreen gate wants *any* device, so use
  `MTLCreateSystemDefaultDevice()` (present) — and keep the full `MTLCopyAllDevices` device-
  selection loop as a `declare_extern`'d `extern "C" { fn MTLCopyAllDevices() -> *mut NSArray }`
  (one line, links against Metal.framework) so the reference logic is preserved verbatim. The
  iteration then uses objc2 `NSArray` + the `isHeadless`/`isLowPower`/`isRemovable` getters.
- **`CAMetalLayer`/CALayer subclass** — needs `objc2-quartz-core` (not in lock) + objc2's
  `define_class!` for the IOSurfaceLayer subclass (ivars + `display`/`actionForKey:` override).
  **Deferred with the whole on-screen half.**
- **`replaceRegion:mipmapLevel:withBytes:bytesPerRow:`** — present but marked `unsafe` (raw
  pointer). Fine; wrap in the `Texture::replaceRegion` PORT.
- **Completion block** — `addCompletedHandler:` takes an ObjC block; use `block2::RcBlock`
  (present). The block captures `renderer` + `target` raw pointers exactly as ghostty's
  `CompletionBlock.Context` does (mut-args-strongly-protected memory model: use raw `*mut`).

Everything else maps 1:1. No other gaps.

---

## §3 Shader pipeline

`shaders.metal` (853 loc MSL) defines the entry points the pipelines bind:
`full_screen_vertex`, `bg_color_fragment`, `cell_bg_fragment`, `cell_text_vertex`/`_fragment`,
`image_vertex`/`_fragment`, `bg_image_vertex`/`_fragment`. Ghostty compiles them to a
`.metallib` at build time (`MetallibStep.zig`: two `xcrun` calls —
`xcrun -sdk macosx metal -o out.ir -c shaders.metal` then
`xcrun -sdk macosx metallib -o out.metallib out.ir`), `@embedFile`s the result as
`ghostty_metallib`, and loads it at runtime with `newLibraryWithData:error:` (wrapping the bytes
in `dispatch.Data`). Custom shadertoy post-shaders use `newLibraryWithSource:options:error:` at
runtime (from the R-b glslang→spirv-cross→MSL output — already ported in `shadertoy.rs`).

**Recommendation: (a) build.rs metallib**, mirroring ghostty exactly.
- `crates/rustty-renderer/build.rs` (macOS only): shell the same two `xcrun` commands over
  `src/shaders/shaders.metal` into `$OUT_DIR/rustty.metallib`; `include_bytes!(concat!(env!("OUT_DIR"), …))`.
- Runtime: wrap the bytes in a `dispatch2::DispatchData` and call
  `device.newLibraryWithData_error(&data)` — byte-faithful to `initLibrary`.
- Copy `shaders.metal` into `crates/rustty-renderer/src/shaders/` (R3: verbatim, it is source
  we own the way ghostty owns it — the glsl shadertoy files are already vendored there).

**Documented fallback (b)**: if the Metal toolchain is unavailable (bare Command-Line-Tools
without the Metal compiler component — a real CI risk), `newLibraryWithSource_options_error`
from `include_str!(shaders.metal)`. Same source, same pixels, no build tooling. This is *also*
ghostty's own code path (it uses `newLibraryWithSource` for every custom shader), so it is R3-
faithful, not a shortcut. Gate the build.rs on `xcrun -sdk macosx -f metal` succeeding and set a
cfg so the runtime picks the embedded metallib when present, else source-compiles. The golden
image is identical under both because the MSL is identical.

Custom post-pipelines (`initPostPipeline`) port straight to `newLibraryWithSource` — no metallib
needed there; they were already fed by `shadertoy.rs`.

---

## §4 `generic.zig` — backend-agnostic vs GPU-specific

`generic.zig` is a single comptime `Renderer(GraphicsAPI)`. In Rust this becomes
`Renderer<A: GraphicsApi>` (a `trait GraphicsApi` with associated `Buffer<T>`/`Texture`/`Target`/
`Sampler`/`RenderPass`/`Shaders` types), and `Metal.zig` becomes the sole `impl GraphicsApi for
Metal`. The comptime crate-root selector (`switch(build_config.renderer)`) becomes a cfg alias
`pub type Renderer = GenericRenderer<Metal>` on macOS.

**Already CPU-ported (R-a leaves that `generic.zig` consumes)** — do NOT re-port:
- `cell.rs`: `Contents` (`bg_cells: Vec<CellBg>`, `fg_rows: ArrayListCollection<CellText>`),
  `add`, `bg_cell`, `resize`, `clear`, `set_cursor`, classification helpers.
- `shaders.rs`: `Uniforms`, `CellText`, `CellBg`, `Atlas`, `CellTextBools` (`#[repr(C)]`).
- `cursor.rs`: renderer `Style`/`style()`. `state.rs`: `State`/`Preedit`. `link.rs`.
  `shadertoy.rs`: custom-shader transpile + `Uniforms`.

**Backend-agnostic CPU code IN `generic.zig` (new in R-d, 0 tests, no GPU calls)** — fills the
already-ported `Contents`/`Uniforms`:
- `updateFrame` (the critical-section extract + terminal `RenderState` walk),
- `rebuildCells` → `rebuildRow` → `addGlyph`/`addCursor`/`addPreeditCell`/`addUnderline`/
  `addOverline`/`addStrikethrough`, `rebuildOverlay`,
- uniforms math: `updateScreenSizeUniforms`, `updateFontGridUniforms`,
  `updateCustomShaderUniformsFromState`/`…ForFrame` (projection matrix, grid/cell size,
  padding-extend, cursor pos/color).
These are the bulk of the 3374 loc and are the highest-value faithful port (they decide the
pixels). They are exercised end-to-end by the golden image.

**GPU-specific (drives objc2 through the `GraphicsApi` trait)**:
- `SwapChain` + `FrameState` (triple-buffered `Buffer`/`Texture`/`Target` + semaphore),
- `drawFrame(sync)`: `nextFrame` → sync uniforms/cells/atlas textures into GPU buffers
  (`frame.uniforms.sync`, `frame.cells_bg.sync`, `frame.cells.syncFromArrayLists(fg_rows.lists)`,
  `syncAtlasTexture`) → `api.beginFrame` → one `RenderPass` with the ordered `step`s
  (bg_color/bg_image → kitty-below → cell_bg → kitty-below-text → **cell_text** (instanced,
  `instance_count = fg_count`, `triangle_strip`, 4 verts) → kitty-above → overlay) → optional
  custom-shader post passes → `frame_ctx.complete(sync)`,
- `frameCompleted`, `changeConfig`, `setScreenSize`, image upload (`self.images.upload`/`draw`).

The `drawFrame` control flow (needs_redraw gating, size-change, `reinitialize_shaders`,
custom-shader-state attach/detach) is CPU logic wrapping the GPU calls — port whole.

`draw_mutex: std.Thread.Mutex` → guard-less `rustty_zigstd::thread::Mutex` (same core-tier
mutex-model decision `state.rs` already uses); `frame_sema: std.Thread.Semaphore` →
a faithful counting semaphore (rustty-zigstd or `std::sync` Condvar+count — verify a
zigstd Semaphore exists, else port one leaf-side).

---

## §5 `renderer.Thread` event loop

`thread.rs` today ports ONLY the `Mailbox` type + `DerivedConfig` placeholder; the event loop
(xev `Loop`/`Async`/`Timer`, `threadMain`/`drainMailbox`/`renderCallback`, the draw/cursor
timers) was deferred jointly with D6-xev and this GPU tier. xev is now real
(`rustty-xev` kqueue). But **R-d does not need it**: the loop's job is to *call*
`updateFrame`+`drawFrame` on a cadence and present to the layer. The offscreen golden-image test
calls `updateFrame(state, blink)` then `drawFrame(sync=true)` **directly** — synchronous, no
loop, no layer, `waitUntilCompleted` gives a finished frame to read back.

**Recommendation**: land in R-d only the `drawFrame`/`updateFrame`/`frameCompleted` surface and
the `Frame.complete(sync=true)` synchronous path. **Defer** the `renderer.Thread` event loop AND
the async present half (`addCompletedHandler` async branch, `displayCallback`, CVDisplayLink)
to the windowing-host milestone — they only matter once there is a live layer to present into
and a real frame cadence to drive. This keeps R-d's new surface tight and fully gated by the
offscreen image. (Port the async branch's code as R5-preserved `unimplemented!` seam so no
reference lines are lost.)

---

## §6 The golden-image gate (the R-d contract)

Precedent (verified, `rustty-font/src/sprite/face.rs::test_diff_atlas`): the sprite tier decodes
a checked-in reference PNG with the pure-Rust `png` dev-dependency and asserts the rendered
buffer is **pixel-exact** against it (`res/U+XXXX...-WxH+T.png`, 8-bit grayscale = alpha
coverage). R-d follows this exact model, for RGBA.

**Design** (`#[cfg(target_os = "macos")] #[test] fn rd_golden_frame()`):
1. Build a fully-determined renderer input in-test (no live terminal needed):
   - fixed grid, e.g. **10×3 cells**, fixed `cell_size` from a fixed `Metrics`, fixed
     `screen_size` (e.g. 90×63 px) → fixed `Uniforms` (projection, padding, colors).
   - fixed content: a handful of `CellText` (a short ASCII word) + a few `CellBg` + a block
     cursor, pushed straight into a `Contents` via the ported `Contents::add`/`bg_cell`/
     `set_cursor` (so the test does NOT depend on `rebuildCells` — it can run before or
     independent of the full terminal walk). A second, richer variant CAN drive the real
     `updateFrame`+`rebuildCells` from a constructed `Terminal`/`RenderState` once those land.
   - a fixed font atlas: rasterize the embedded `JetBrainsMono-Regular` at a fixed px_per_em via
     the ported `rustty-font-shaper` `SharedGrid`, or (maximally deterministic) a tiny synthetic
     grayscale atlas with known glyph rects. Prefer the real atlas but **pin the font bytes,
     px_per_em, and hinting** so the coverage is reproducible across machines.
2. `MTLCreateSystemDefaultDevice()` → build `Metal` with a **fixed** `surfaceSize` (bypass the
   layer), `initTarget(w,h)` (IOSurface-backed), `initShaders`.
3. `updateFrame`(or hand-built `Contents`) → `drawFrame(sync=true)` → `waitUntilCompleted`.
4. Read back: lock the IOSurface (`IOSurfaceLock`), copy `w*h*4` bytes from
   `IOSurfaceGetBaseAddress` honoring `bytesPerRow` (objc2-io-surface), unlock. (Alternative:
   `MTLTexture.getBytes` — but IOSurface base-address is simplest and avoids a blit.)
5. Compare BGRA against `crates/rustty-renderer/res/rd_golden_10x3.png` (committed, decoded with
   the `png` dev-dep). Pixel-exact where possible; if GPU rasterization proves 1-LSB nondeter-
   ministic across GPU families, allow a **tolerance of ≤1/255 per channel** and assert it in
   the message (document the tolerance as the reference sprite test is exact but text AA on the
   GPU may differ by a ULP across Apple vs discrete GPUs).

**Determinism controls** (call these out in the test): fixed pixel format
(`bgra8unorm`, **non-sRGB / `blending=false`-equivalent** to avoid the srgb-encode branch and
DisplayP3 conversion diverging by GPU), `min_contrast` fixed, cursor color fixed, atlas pinned,
no custom shaders, `sync=true`. Pin `blending` to the gamma-incorrect (non-linear) path so the
`*_srgb` attachment + P3 colorspace variance is out of the golden image (or, if we want to test
the srgb path, commit two references). Prefer **grayscale-atlas monochrome text on solid bg** for
the first golden — it removes emoji/color-atlas and P3 from the equation.

**Reference generation**: generate once by running the test with a "write mode" env var that
dumps the readback as PNG (pure-Rust `png` encoder, dev-dep), **visually inspect** it (open the
PNG, confirm it shows the expected word + cursor + bg), then commit. Document in a header comment
that the PNG was human-verified on <date/GPU>. Re-generation requires the same review.

**How the gate runs**: a plain `#[cfg(target_os="macos")] #[test]`, so `cargo test -p
rustty-renderer` on any mac (incl. headless CI with a GPU) runs it; on non-macOS it compiles-out.
Add it to `ci.sh` behind an `is-macos && has-metal-device` guard so Linux CI stays green. This
`#[test]` + the `size_of::<CellText>()==32` test ARE the R-d test surface that replaces the
per-module test-count contract for this tier (record in DEFERRALS.md that `generic.zig`/`Metal`/
`metal/*` carry 0 reference `test{}` and are golden-gated; the one `metal/shaders.zig` CellText
test is un-deferred here).

---

## §7 Crate placement + deps + acyclicity

Everything lands in **`rustty-renderer`** (already the renderer leaf; it is BELOW apprt/core/
config — only they depend on it). New `Cargo.toml` deps (all macOS-gated
`[target.'cfg(target_os = "macos")'.dependencies]`):

```
objc2               # already transitive → promote to direct
objc2-foundation    # NSString/NSArray/NSError/NSRange
objc2-metal         # features: MTLDevice MTLCommandQueue MTLBuffer MTLTexture MTLLibrary
                    #   MTLRenderPipeline MTLRenderCommandEncoder MTLRenderPass MTLSampler
                    #   MTLPixelFormat MTLResourceOptions MTLVertexDescriptor dispatch2
objc2-io-surface    # IOSurfaceRef (Target)
objc2-core-graphics # CGColorSpace DisplayP3 (Target)
block2              # addCompletedHandler block
dispatch2           # DispatchData for newLibraryWithData (+ deferred main-queue dispatch)
# objc2-quartz-core → ADD ONLY with the deferred on-screen half (CAMetalLayer/CALayer)
[build-dependencies] (none required; build.rs shells xcrun)
[dev-dependencies]  png   # golden-image decode/encode (same as rustty-font sprite gate)
```

**Acyclicity**: all new deps are external crates.io — no internal edge. `rustty-renderer`
already deps `rustty-font-shaper` (which already deps `objc2-core-text`/`-core-graphics`), so
the objc2 family is established below the renderer. No new inter-crate edge, so
`check_acyclic.sh` is unaffected. `rustty-renderer` is already in `check_leaks.sh`'s `FFI_CRATES`
(oniguruma/glslang/spirv-cross) → the added C-FFI (Metal.framework) needs no G6 change beyond
confirming it's still listed. Confirm `check_test_counts.py` treats `generic.zig`/`Metal.zig`/
`metal/*` as golden-gated (0 `test{}` mapped) and `metal/shaders.zig` → `shaders.rs` count now
includes the un-deferred CellText test.

---

## §8 Chunked execution plan + risks

> **STATUS: Chunks 0-4 DONE + committed; Chunk 5 (this finale) DONE.** The offscreen
> R-d slice RENDERS and is golden-gated (`golden_renders_cursor_fg_bg`). Chunk 5
> audited the seams (**18** live `unimplemented!()` call sites — the earlier "~27"
> estimate predated the Chunk-2/3/4 fills — **all deferred, zero reachable-bug**),
> retained an honest `PORT STATUS: in progress` (see below), and updated the docs.
> Full audit: `.port/phase-2/GPU_RD_CHUNK5_DESIGN.md`.

**Chunk 0 — trait + api aliases + build.rs (compile-only).** `trait GraphicsApi` (assoc types),
`api.rs` re-export/alias of objc2-metal enums to ghostty names, `build.rs` metallib (+ source
fallback), copy `shaders.metal`. Gate: `cargo build -p rustty-renderer` on macOS.

**Chunk 1 — `metal/*` leaves (offscreen set).** `buffer.rs`, `Texture.rs`, `Sampler.rs`,
`Pipeline.rs` (+ hand-written vertex-attribute tables), `RenderPass.rs`, `Target.rs` (IOSurface),
`shaders.rs`-backend (library+pipelines), `Frame.rs`. Un-defer `size_of::<CellText>()==32`.
Gate: compiles; a scratch test creates device/queue/buffer/texture/pipeline without panic.

**Chunk 2 — `Metal.zig` impl (offscreen).** device/queue select (`MTLCreateSystemDefaultDevice`
+ preserved `MTLCopyAllDevices` extern), options fns, `initTarget`/`initShaders`/`beginFrame`/
`initAtlasTexture`; **fixed `surfaceSize`**, present = IOSurface readback, layer/loop = seam.

**Chunk 3 — `generic.zig` CPU half.** SwapChain/FrameState, `updateFrame`, `rebuildCells`/
`rebuildRow`/`addGlyph`/`addCursor`/… , uniforms math. No GPU output yet; unit-checkable by
inspecting `Contents`/`Uniforms` against a constructed terminal (optional dev asserts).

**Chunk 4 — `generic.zig` `drawFrame` + golden image.** Wire the render pass steps; build the
deterministic in-test frame; readback + PNG diff; generate + human-verify + commit the reference
PNG. Gate: `rd_golden_frame` green. **This is the R-d gate.**

**Chunk 5 — seams + docs. DONE.** Audited every live `unimplemented!()` in
`rustty-renderer/src` (**18** call sites: `generic.rs` 4, `metal/*` 3, `image.rs` 5,
`overlay.rs` 4, `message.rs` 1, `thread.rs` 1) and PROVED none is reachable from the
tested offscreen path → **all 18 legitimately DEFERRED** to five named Goal-2 tiers,
each with a verbatim R5 body, **zero reachable-bug**. Deferred bodies (IOSurfaceLayer,
CVDisplayLink, async present, `renderer.Thread` loop) stay `unimplemented!(...)`.
Updated the `lib.rs` header (the sole, behavior-preserving code touch), DEFERRALS.md
(new R-d subsection: golden-gated modules, un-deferred CellText==32 test, the 5
deferred tiers), and the Chunk-4 doc. **PORT STATUS/G3b decision:** kept an honest
`in progress` (the crate is not wholly ported) — G3b comment-coverage stays SKIPPED;
R1 backstop = adversary + golden (heavy-tier doctrine). Flipping to `done` purely to
activate the gate was rejected as dishonest. See `GPU_RD_CHUNK5_DESIGN.md`.

### Risks

- **CI Metal toolchain absence** (`xcrun metal` needs the Metal compiler component, not just
  CLT). Mitigated by the `newLibraryWithSource` fallback (§3) — same pixels, no toolchain.
- **Headless Metal device**: `MTLCreateSystemDefaultDevice()` works on a mac with any GPU incl.
  headless CI runners; it returns null on a truly GPU-less box → the test must `skip` (not fail)
  when no device. No display/window needed (IOSurface offscreen).
  **CORRECTED 2026-09-09 (task-14c-1):** this bullet previously said that skip was "guarded in
  `ci.sh`". It was not — `grep -in metal .port/tools/ci.sh` returned nothing, so risk **F5**
  above (a skip-on-nil golden is VACUOUS on a Metal-less runner) had no mitigation at all and a
  green macOS leg proved nothing about live-Metal execution. F5's mitigation is now IMPLEMENTED,
  and not in `ci.sh`: `crates/rustty-renderer/src/metal/test_support.rs` provides
  `RUSTTY_REQUIRE_METAL=1` (`metal_required` / `require_metal_or_skip`, a direct sibling of the
  `RUSTTY_REQUIRE_GL` precedent in `opengl/gl/test_support.rs`), EVERY "no Metal device" skip arm
  in the crate calls the pivot immediately before printing its skip line, and
  `.github/workflows/ci.yml` exports the variable on the macOS leg. Default (unset) behaviour is
  unchanged: local dev without a GPU still skips cleanly. `ci.sh` stays the check SEQUENCE only;
  the strict-mode switch is environment, matching how the GL lane already does it.
- **Golden nondeterminism across GPU families** (Apple silicon vs Intel/discrete text AA, sRGB/
  P3 conversion). Mitigated by pinning to the non-sRGB grayscale-text path for the first golden
  and allowing ≤1-LSB tolerance; pin the CI runner GPU class if exact. Commit-time human review
  is the backstop against a wrong reference.
- **`autoAttribute` comptime loss**: Rust can't reflect struct fields at comptime → hand-written
  attribute tables risk drift from the struct. Mitigate with a `const` assert that the table's
  last `(format, offset)` + size equals `size_of::<Vertex>()` (and the CellText==32 test).
- **objc2 `MTLCopyAllDevices` gap** — one declared extern; low risk, offscreen uses
  `CreateSystemDefaultDevice` anyway.
- **Offscreen ≠ on-screen honesty** *(MET — the golden is now the evidence)*: the golden test
  `golden_renders_cursor_fg_bg` proves the *renderer emits correct pixels into an IOSurface
  texture*; it does NOT prove CAMetalLayer presentation, vsync, or resize. Those remain
  explicitly the windowing-host milestone (the deferred `Metal::{init,loop_enter}` / async
  present / display-link seams). Standing caveat retained: "R-d green" means correct offscreen
  pixels, NOT "shows a window."
- **Semaphore/mutex fidelity**: `frame_sema`/`draw_mutex` must match the Zig blocking semantics
  (triple-buffer backpressure). Verify a zigstd `Semaphore` exists; if not, port one leaf-side
  before Chunk 3.
