# R-d — the `GraphicsApi` abstraction: exact Rust shape of `GenericRenderer<A>`

**Sub-decision architect deliverable.** Parent: `GPU_RD_BLUEPRINT.md` (adversarially
reviewed, ACCEPT-WITH-FIXES). This doc pins the *exact* Rust shape of the abstraction that
`src/renderer/generic.zig`'s `pub fn Renderer(comptime GraphicsAPI: type) type` is generic
over, so it can be implemented verbatim. It is itself intended to be adversarially reviewed.

Scope note: R1 (PORT_RULES) requires 100% behavioral/comment/test fidelity. It does **not**
require preserving Zig comptime genericity *as Rust generics* — "genericity is a Zig comptime
convenience; behavior is the parity target." That single sentence is load-bearing for §F.

---

## DECISION — RESOLVED: port MONOMORPHICALLY (adversary-confirmed 2026-07-03)

This sub-blueprint was adversarially reviewed (independent re-derivation of the entire
`GraphicsAPI`-facing surface + a hunt for any downstream consumer/reference-`test{}`/second
backend that *requires* the generic seam). **Verdict: the monomorphic recommendation (§F) is
SOUND — adopt it.** The two hardest break attempts both held: (1) catalog completeness — no
`api` method/const/value-method is missing from §1; (2) no consumer forces the generic —
`apprt/`/`core/`/`config/` all use the concrete `renderer.Renderer` alias, there are **zero**
reference `test{}` blocks that instantiate the renderer with a mock/non-Metal API, WebGL
bypasses the generic, and OpenGL is Linux-deferred.

**This DEVIATES from the parent `GPU_RD_BLUEPRINT.md`'s stated mechanism** (a live
`trait GraphicsApi` hierarchy) — but not from any of its `[ADV-FIX n]` items; the trait-vs-mono
shape was a design detail stated in passing, not a reviewed decision. The deviation is
justified (byte-identical behavior; R1 explicitly licenses dropping the Rust-level genericity),
low-risk (avoids a GAT + `RawBuffer` hoist + `Step<'a,A>` lifetime threading + a const-generic
swap-chain workaround + `Retained` refcount reshaping — all Rust-only glue with no reference
analogue), and **reversible**: extract-trait is one of the safest refactors and lands cleanly
when OpenGL actually exists to validate the abstraction against the concrete Metal leaves that
§A was reverse-engineered from. Chunk-1 leaf shapes are **identical either way** (verified), so
even this decision is rework-free to revisit post-Chunk-1.

Folded fixes from the review:
- **[FIX-1] OPEN RISK #7 is the whole `DerivedConfig` cycle, not just `AlphaBlending`.** The
  config→renderer back-edge (rustty-config deps rustty-renderer) means the renderer cannot name
  ANY `rustty_config::Config.*` type, and `generic.zig`'s `DerivedConfig` (536-577) names ~10 of
  them (`TerminalColor`×7, `WindowPaddingColor`, `RepeatablePath`, `Path`,
  `BackgroundImagePosition`/`Fit`, `WindowColorspace`, `AlphaBlending`, `BackgroundBlur`,
  `FontShapingBreak`) + `init(*const Config)`. `AlphaBlending` currently lives in
  `rustty-config/src/value_types.rs:597`. Faithful fix (in-tree precedent): the renderer already
  breaks this exact cycle for `Message::ChangeConfig` by being **generic over `C`**
  (`thread.rs:67`, `message.rs:144` — concrete `C = rustty_config::Config` bound higher up), and
  `lib.rs:75-82` documents `DerivedConfig` as a deferred seam for precisely this. So port
  `DerivedConfig` generic over `C` (its fields become `C`-associated extracts) OR hoist the
  needed value types to `rustty-render-types` (the `size`/`Health` hoist precedent). **This is a
  CHUNK-3 prerequisite** (DerivedConfig is a `GenericRenderer` field, not touched by the
  monomorphic Chunk-0 scope of api.rs+build.rs+PODs), and it is IDENTICAL for both designs — it
  does not affect this decision. The golden gate pins `blending`/`colorspace`/`min_contrast`
  (all DerivedConfig fields), so it must be solved before Chunk 4, not skipped.
- **[FIX-2] §F.2 table wording bug:** the "monomorphic `begin_frame`" cell must read
  `*mut Renderer` (raw pointer), NOT `&mut Renderer`. The completion block stashes and later
  derefs the renderer pointer while `self` is live inside `drawFrame` — a `&mut` there is a UB
  trap (`rust-mut-args-strongly-protected`). Raw `*mut` is the faithful, UB-avoiding choice in
  BOTH designs (the `@fieldParentPtr`/container_of precedent). This trampoline is exercised by
  the golden gate (`Frame.complete(sync=true)` invokes the block directly), so it is **not**
  deferrable — only the async `addCompletedHandler` branch is.
- **[FIX-3] cosmetic:** Metal declares only `loopEnter` among the 8 `@hasDecl` hooks (§A.1's
  aside that it overrides `display_realized` is wrong; §1.4 lists all 8 correctly — the other 7
  are no-ops on Metal). And `pub const API = GraphicsAPI` (`generic.zig:85`, reached through by
  `image.zig`/`cell.zig`) vanishes monomorphically (leaves name concrete `crate::shaders`).

Everything below (§0–§G) stands as the implementation spec, read through the monomorphic lens
of §F (substitute `Metal`/`metal::*` for `A`/`A::*`, drop the `<A>` generics and the sub-traits).

---

## §0 — Executive recommendation (read first)

The blueprint's TL;DR mechanism is a full `trait GraphicsApi` hierarchy with a sub-trait per
associated type (`GpuBuffer`/`GpuTexture`/…) so that `GenericRenderer<A: GraphicsApi>` stays
generic. This doc specifies that hierarchy in full (§A–§E). **But my independent recommendation
(§F) is to port `generic.zig` MONOMORPHICALLY**: a concrete `Renderer` struct over the concrete
Metal leaf types, with the `GraphicsApi` genericity preserved *only* as a documented,
compile-checked Goal-2 seam (a thin marker trait + the crate-root `cfg` alias), **not** as the
generic parameter the renderer body is written against.

Reason in one paragraph: the Zig `GenericRenderer(GraphicsAPI)` is a **zero-cost comptime**
abstraction that the Zig compiler monomorphizes to exactly one type — `GenericRenderer(Metal)`
— in every build (the crate-root `switch(build_config.renderer)` picks one arm at comptime).
R-d has **exactly one** `impl` (Metal; OpenGL = Linux/deferred, WebGL = wasm/deferred). A
monomorphic Rust `Renderer` over concrete Metal types produces **byte-identical behavior** —
it *is* what the Zig compiler emits. Reproducing the abstraction as a Rust trait hierarchy is,
by contrast, **not** zero-cost in engineering terms: it forces (a) a GAT `type Buffer<T>`, (b)
sub-traits on every associated-type value so `frame.complete()`/`buffer.sync()`/`pass.step()`
resolve, (c) associated `Step`/`Attachment` types that must be publicly constructible from
generic code, (d) by-value movement of associated-type values through `[N]FrameState` arrays
and `&[Option<Texture>]` slices (Zig copies a pointer for free; Rust `Retained` clones bump a
refcount), and (e) a recursive `begin_frame(&mut GenericRenderer<Self>)` bound. That is a large
amount of Rust-only glue that reproduces **zero** additional behavior and multiplies the
adversarial-review surface — all to abstract over a set of size one. §F gives the full tradeoff.

Both designs are specified so the reviewer can override. Where they differ, the concrete Metal
leaf shapes (§C `Options`, §B `Buffer<T>`, the `Step`/`Attachment`/`Draw` structs) are
**identical** — only the top-level wiring (trait-generic vs concrete) changes, so the leaf work
of Chunk 1 is unaffected by the choice.

- **(A) trait family** — if the hierarchy is kept: `GraphicsApi` + seven value sub-traits
  `GpuBuffer<T>`/`GpuTexture`/`GpuTarget`/`GpuSampler`/`GpuPipeline`/`GpuFrame`/`GpuRenderPass`
  + `GpuShaders`, with `Shaders` the only "module" associated type (the vertex/uniform structs
  are concrete `crate::shaders::*`, already ported, backend-agnostic).
- **(B) `Buffer(T)`** — a concrete generic `struct Buffer<T: Copy>` in `metal/buffer.rs`
  (1:1 with Zig's `Buffer(comptime T)`). Trait path adds `type Buffer<T>: GpuBuffer<T>` (a GAT)
  pointing at it; monomorphic path uses it directly. Instantiated at 6 element types.
- **(E) Chunk-0 boundary** — with the monomorphic recommendation, Chunk 0 shrinks to `api.rs`
  enum aliases + `build.rs` + the concrete leaf struct *headers*; there is no free-standing
  trait file that must compile before the leaves exist. With the trait path, only `GraphicsApi`
  const/assoc-type declarations and the sub-trait *method signatures whose types are already
  concrete* compile in Chunk 0; every signature naming a not-yet-existing `metal/*` concrete
  type must wait for Chunk 1. Precise split in §E.

---

## §1 — Exhaustive catalog of the `GraphicsAPI`-facing surface

This is the raw material for §A–§G. Every reference was traced in `generic.zig` (grep of
`GraphicsAPI`, `self.api`, `api.`, and each associated-type value) plus `image.zig` (which
receives `&self.api` and instantiates `Buffer(shaders.Image)`).

### 1.1 Associated types read off `GraphicsAPI` (generic.zig:85–94, image.zig:9)

| Zig | kind | Rust mapping |
|---|---|---|
| `GraphicsAPI.Target` | GPU render target | assoc type `Target` / concrete `metal::Target` |
| `GraphicsAPI.Buffer` | **generic** `Buffer(comptime T)` | GAT `Buffer<T>` / concrete `metal::Buffer<T>` (§B) |
| `GraphicsAPI.Sampler` | sampler state | assoc type `Sampler` / concrete `metal::Sampler` |
| `GraphicsAPI.Texture` | GPU texture | assoc type `Texture` / concrete `metal::Texture` |
| `GraphicsAPI.RenderPass` | render pass | assoc type `RenderPass` / concrete `metal::RenderPass` |
| `GraphicsAPI.Frame` | frame ctx (via `beginFrame` ret / image.zig `*RenderPass`) | assoc type `Frame` / concrete `metal::Frame` |
| `GraphicsAPI.Pipeline` | pipeline (image.zig:107, `shaders.pipelines.*`) | assoc type `Pipeline` / concrete `metal::Pipeline` |
| `GraphicsAPI.shaders` | a **module** | see §1.2 |
| `GraphicsAPI.shaders.Shaders` | pipeline container | assoc type `Shaders` / concrete `metal::shaders::Shaders` |

### 1.2 The `shaders` "module" — split concrete vs backend-specific

`shaderpkg = GraphicsAPI.shaders` (generic.zig:93) is a Zig *namespace*, not a type. Rust has no
"associated module". Its members split cleanly:

- **Backend-AGNOSTIC vertex/uniform structs** — the *same* struct for Metal and OpenGL
  (ghostty defines equivalents in both `metal/shaders.zig` and `opengl/shaders.zig`; `cell.rs`
  already consumes `crate::shaders::{CellBg, CellText}`). Map to the **concrete, already-ported**
  `crate::shaders`:
  - `shaderpkg.Uniforms` → `crate::shaders::Uniforms` ✅ ported
  - `shaderpkg.CellText` → `crate::shaders::CellText` ✅ ported
  - `shaderpkg.CellBg` → `crate::shaders::CellBg` (= `[u8;4]`) ✅ ported
  - `shaderpkg.BgImage` → `crate::shaders::BgImage` ⚠ **NOT yet ported** — R-d prerequisite
  - `shaderpkg.Image` → `crate::shaders::Image` ⚠ **NOT yet ported** — R-d prerequisite
    (these two live in `metal/shaders.zig`; port them into `crate::shaders` beside CellText)
  - `shadertoy.Uniforms` (custom-shader uniforms) → `crate::shadertoy::Uniforms` ✅ ported
- **Backend-SPECIFIC** — the only member that becomes an associated type / concrete leaf:
  - `shaderpkg.Shaders` → `GraphicsApi::Shaders` / `metal::shaders::Shaders`

Consequence: `GenericRenderer`'s fields typed `shaderpkg.Uniforms`, `.CellBg`, etc. become
**concrete** `crate::shaders::*` in BOTH designs. Only `shaders: Shaders` and the pipelines
inside it are backend-typed.

### 1.3 Comptime consts read off `GraphicsAPI`

| Zig | value (Metal) | use site | Rust |
|---|---|---|---|
| `swap_chain_count` | `3` | `[buf_count]FrameState`, `frame_sema = .{.permits = buf_count}` (246,255) | `const SWAP_CHAIN_COUNT: usize` |
| `custom_shader_target` | `.msl` | arg to `shadertoy.loadFromFiles` (845) | `const CUSTOM_SHADER_TARGET: shadertoy::Target` |
| `custom_shader_y_is_down` | `true` | cursor Y-flip branch (2157, 2167) | `const CUSTOM_SHADER_Y_IS_DOWN: bool` |

### 1.4 Methods called on `self.api` (the GraphicsAPI instance) — the trait's own method set

Grouped by role. "opt" = guarded by `@hasDecl(GraphicsAPI, "…")` (a comptime capability probe;
see §G). Return types are the Zig return; error unions become `Result<_, RendererError>`.

**Construction / teardown**
- `GraphicsAPI.init(alloc, options) !GraphicsAPI` (662) — associated fn
- `api.deinit()` (663 errdefer, 827) — Rust `Drop`

**Resource *option* factories** (all return small POD structs; §C)
- `api.uniformBufferOptions() bufferpkg.Options` (347, 466)
- `api.fgBufferOptions()` (356) · `api.bgBufferOptions()` (358) · `api.bgImageBufferOptions()` (363)
- `api.imageBufferOptions()` (image.zig:139) · `instanceBufferOptions` (alias, unused in generic)
  — all six are `pub const … = bufferOptions;` aliases of one body in `Metal.zig`
- `api.textureOptions() Texture.Options` (472,479,511,518)
- `api.samplerOptions() Sampler.Options` (486)
- `api.imageTextureOptions(format: ImageTextureFormat, srgb: bool) Texture.Options` (image.zig:896)

**Resource constructors driven through the api**
- `api.initAtlasTexture(atlas: *const font.Atlas) !Texture` (372,378,3368)
- `api.initTarget(width, height) !Target` (394,428)
- `api.initShaders(alloc, custom_shaders: []const [:0]const u8) !Shaders` (853)

**Frame lifecycle**
- `api.drawFrameStart()` (1461) · `api.drawFrameEnd()` (1462) — AutoreleasePool push/pop in Metal
- `api.surfaceSize() !struct{width:u32,height:u32}` (1465)
- `api.presentLastTarget() !void` (1487)
- `api.beginFrame(renderer: *Renderer, target: *Target) !Frame` (1589) — **recursive**: names
  the *concrete* `GenericRenderer(Metal)` and stashes a raw `*Renderer` in a completion block
- `api.present(target, sync) !void` — reached via the completion block (`Frame.zig:82`), not
  from generic.zig directly, but part of the api's contract
- **field write** `self.api.blending = config.blending` (1905) — direct mutation of the Metal
  `blending: AlphaBlending` field; drives srgb pixel-format selection in initShaders/initTarget/
  textureOptions. Needs a setter or a public field (§A note, §E).

**Optional apprt/thread hooks (all `@hasDecl`-guarded)**
- `GraphicsAPI.surfaceInit(surface)` opt (867) — associated fn
- `api.finalizeSurfaceInit(surface)` opt (876) · `api.threadEnter(surface)` opt (884) ·
  `api.threadExit()` opt (892) · `api.loopEnter()` opt (900) · `api.loopExit()` opt (922) ·
  `api.displayRealized()` opt (941) · `api.displayUnrealized()` opt (970)
- In R-d scope these are **either no-ops or windowing-host seams** (Metal's `loopEnter`
  installs the display callback; deferred). Port as trait methods with default empty bodies
  (§A) or as concrete `Metal` methods some of which are `unimplemented!("windowing-host")`.

### 1.5 Methods/fields on **values of the associated types** (this is what forces sub-traits)

**`Buffer(T)`** — `frame.uniforms`, `frame.cells`, `frame.cells_bg`, `frame.bg_image_buffer`,
`state.uniforms` (custom shader), image.zig local `buf`:
- assoc fn `Buffer(T).init(opts, len) !Self` (347,356,358,363,466 …)
- assoc fn `Buffer(T).initFill(opts, data: []const T) !Self` (image.zig:138)
- `buf.deinit()` (many)
- `buf.sync(data: []const T) !void` (1559,1565,1695)
- `buf.syncFromArrayLists(lists: []const ArrayListUnmanaged(T)) !usize` (1561)
- **field** `buf.buffer` — the raw `MTLBuffer` (`objc.Object`), fed into `Step.uniforms` /
  `Step.buffers` (1615,1616,1625,1643,1660,1661,1711; image.zig:171)

**`Texture`** — `frame.grayscale`, `frame.color`, `state.front_texture`, `state.back_texture`,
image textures:
- assoc fn `Texture.init(opts, w, h, data: ?[]const u8) Error!Self` (471,478,510,517; image.zig:895)
- `tex.deinit()` (many)
- `tex.replaceRegion(x,y,w,h,data) !void` (3371)
- **field** `tex.width` (3363)
- `std.mem.swap(Texture, &front, &back)` (461) — **by-value move/swap**
- passed **by value** into `Step.textures: []const ?Texture` and RenderPass attachment
  `.target = .{ .texture = state.back_texture }` (1595,1613,1663,1664,1712)

**`Target`** — `frame.target`:
- created via `api.initTarget`
- `target.deinit()` (409,429)
- **fields** `frame.target.width`, `frame.target.height` (1537,1538)
- `&frame.target` → `api.beginFrame(self, &frame.target)`; `.target = frame.target` in attachment

**`Sampler`** — `state.sampler`:
- assoc fn `Sampler.init(opts) Error!Self` (486)
- `sampler.deinit()`
- passed **by value** into `Step.samplers: []const ?Sampler` (1713)

**`Frame`** (`frame_ctx` from `beginFrame`):
- `frame_ctx.complete(sync: bool)` (1590, in a `defer`)
- `frame_ctx.renderPass(attachments: []const RenderPass.Options.Attachment) RenderPass` (1593,1700)

**`RenderPass`** (`pass`):
- `pass.step(s: Step)` (1613,1622,1640,1656,1709; image.zig:169)
- `pass.complete()` (1600,1707)
- `&pass` passed to `images.draw(&api, pipeline, &pass, kind)`

**`Pipeline`** — read as values from the `Shaders` container:
- `self.shaders.pipelines.{bg_color,cell_bg,cell_text,image,bg_image}` (1614,1623,1641,1657,1677,…)
- `self.shaders.post_pipelines[i]` (1697,1710)
- passed **by value** into `Step.pipeline`

**`Shaders`** (`self.shaders`, field of the renderer):
- `shaders.deinit(alloc)` (833,983,1500)
- **field** `shaders.defunct` (1505 assert)
- **field** `shaders.pipelines` (the `PipelineCollection` struct: named `Pipeline` fields)
- **field** `shaders.post_pipelines` (`[]const Pipeline`)

**Constructed inline by generic.zig (so their shapes are part of the contract)**
- `RenderPass.Options.Attachment { target: union(enum){texture: Texture, target: Target},
  clear_color: ?[4]f64 }`
- `RenderPass.Step { pipeline: Pipeline, uniforms: ?<rawbuf>, buffers: []const ?<rawbuf>,
  textures: []const ?Texture, samplers: []const ?Sampler, draw: Draw }`
- `Draw { type: MTLPrimitiveType, vertex_count: usize, instance_count: usize = 1 }`

The `<rawbuf>` is `objc.Object` = the `MTLBuffer` handle from `buffer.buffer`. In Rust this is
`Retained<ProtocolObject<dyn MTLBuffer>>` (owned) or `&ProtocolObject<dyn MTLBuffer>` (borrowed).

---

## §A — Trait hierarchy (the blueprint path, fully specified)

If the reviewer keeps the generic hierarchy, this is the **complete** family. Nothing in §1.5
can be a "concrete subset": every associated-type value has methods called on it from generic
code, so each needs a bound. The one genuine simplification is §1.2 — the vertex/uniform
structs are concrete, so only `Shaders`/`Pipeline`/`Target`/`Frame`/`RenderPass`/`Texture`/
`Sampler`/`Buffer<T>` are abstract.

Shared vocabulary types (concrete, in `crate::…`): `RendererError` (the crate error enum, wraps
`MetalFailed`/`OutOfMemory`/…), `renderer::Options`, `shadertoy::Target`, `shadertoy::Uniforms`,
`crate::shaders::{Uniforms, CellText, CellBg, BgImage, Image, Atlas}`, `rustty_font::Atlas`,
`AlphaBlending` (= `rustty_config::…::AlphaBlending`), and the Metal enum aliases in
`metal::api` (`MTLPrimitiveType`, …). `SurfaceSize { width: u32, height: u32 }` is a concrete
struct (Metal returns an anonymous struct; name it).

### A.1 `GraphicsApi` — the top trait

```rust
/// Ports `generic.zig`'s comptime `GraphicsAPI` interface contract.
/// The renderer is written against `A: GraphicsApi`.
pub trait GraphicsApi: Sized {
    // ---- associated resource types (§1.1) ----
    type Target: GpuTarget;
    type Texture: GpuTexture;
    type Sampler: GpuSampler;
    type Pipeline: GpuPipeline;
    type Frame: GpuFrame<Self>;
    type RenderPass: GpuRenderPass<Self>;
    type Shaders: GpuShaders;

    /// Zig `Buffer(comptime T)`. A GAT pointing at the concrete generic
    /// `metal::Buffer<T>` (§B). `T: Copy` because sync() memcpys `[T]`.
    type Buffer<T: Copy>: GpuBuffer<T>;

    // ---- comptime consts (§1.3) ----
    const SWAP_CHAIN_COUNT: usize;
    const CUSTOM_SHADER_TARGET: shadertoy::Target;
    const CUSTOM_SHADER_Y_IS_DOWN: bool;

    // ---- construction (deinit = Drop) ----
    fn init(alloc: Allocator, options: renderer::Options) -> Result<Self, RendererError>;

    // ---- option factories (§C) ----
    fn uniform_buffer_options(&self) -> BufferOptions;
    fn instance_buffer_options(&self) -> BufferOptions;
    fn fg_buffer_options(&self) -> BufferOptions;
    fn bg_buffer_options(&self) -> BufferOptions;
    fn image_buffer_options(&self) -> BufferOptions;
    fn bg_image_buffer_options(&self) -> BufferOptions;
    fn texture_options(&self) -> TextureOptions;
    fn sampler_options(&self) -> SamplerOptions;
    fn image_texture_options(&self, format: ImageTextureFormat, srgb: bool) -> TextureOptions;

    // ---- resource constructors ----
    fn init_atlas_texture(&self, atlas: &rustty_font::Atlas)
        -> Result<Self::Texture, RendererError>;
    fn init_target(&self, width: usize, height: usize)
        -> Result<Self::Target, RendererError>;
    fn init_shaders(&self, alloc: Allocator, custom_shaders: &[&CStr])
        -> Result<Self::Shaders, RendererError>;

    // ---- frame lifecycle ----
    fn draw_frame_start(&mut self);
    fn draw_frame_end(&mut self);
    fn surface_size(&self) -> Result<SurfaceSize, RendererError>;
    fn present_last_target(&self) -> Result<(), RendererError>;
    /// `renderer` is a raw *mut to the concrete GenericRenderer<Self> — the
    /// completion block stashes it (container_of / @fieldParentPtr precedent).
    fn begin_frame(
        &self,
        renderer: *mut GenericRenderer<Self>,
        target: *mut Self::Target,
    ) -> Result<Self::Frame, RendererError>;
    fn present(&self, target: &Self::Target, sync: bool) -> Result<(), RendererError>;

    // ---- config field write (generic.zig:1905) ----
    fn set_blending(&mut self, blending: AlphaBlending);

    // ---- optional apprt/thread hooks (@hasDecl) — default no-ops ----
    fn surface_init(_surface: &mut apprt::Surface) -> Result<(), RendererError> { Ok(()) }
    fn finalize_surface_init(&self, _s: &mut apprt::Surface) -> Result<(), RendererError> { Ok(()) }
    fn thread_enter(&self, _s: &mut apprt::Surface) -> Result<(), RendererError> { Ok(()) }
    fn thread_exit(&self) {}
    fn loop_enter(&mut self) {}
    fn loop_exit(&mut self) {}
    fn display_realized(&mut self) {}
    fn display_unrealized(&mut self) {}
}
```

Notes:
- The `@hasDecl` probes in generic.zig become **defaulted trait methods** — a defaulted method
  is the faithful Rust analogue of "call it only if the impl declares it" (§G).
- `begin_frame`'s `*mut GenericRenderer<Self>` is the recursive bound. It is legal Rust
  (a trait method may name a generic struct parameterized by `Self`) but it couples the trait
  file to the renderer struct — flagged in Open Risks.
- `set_blending` replaces the raw `self.api.blending = …` field write. (Alternative: expose the
  field via `fn blending_mut(&mut self) -> &mut AlphaBlending`; a setter is cleaner.)

### A.2 Value sub-traits (§1.5)

```rust
/// Zig `metal/buffer.zig` `Buffer(T)`.
pub trait GpuBuffer<T: Copy>: Sized {
    /// The backend's raw GPU-buffer handle, fed into RenderPass steps
    /// (Metal: Retained<ProtocolObject<dyn MTLBuffer>>).
    type Raw;
    fn init(opts: BufferOptions, len: usize) -> Result<Self, RendererError>;
    fn init_fill(opts: BufferOptions, data: &[T]) -> Result<Self, RendererError>;
    fn sync(&mut self, data: &[T]) -> Result<(), RendererError>;
    fn sync_from_array_lists(&mut self, lists: &[Vec<T>]) -> Result<usize, RendererError>;
    /// The `buffer.buffer` field: borrow of the raw handle for a Step.
    fn raw(&self) -> &Self::Raw;
    // deinit = Drop
}

/// Zig `metal/Texture.zig`.
pub trait GpuTexture: Sized {
    fn init(opts: TextureOptions, width: usize, height: usize, data: Option<&[u8]>)
        -> Result<Self, RendererError>;
    fn replace_region(&self, x: usize, y: usize, width: usize, height: usize, data: &[u8])
        -> Result<(), RendererError>;
    fn width(&self) -> usize;
    // deinit = Drop; must be movable for std.mem.swap (Sized ✓)
}

/// Zig `metal/Target.zig`.
pub trait GpuTarget: Sized {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    // constructed via GraphicsApi::init_target; deinit = Drop
}

/// Zig `metal/Sampler.zig`.
pub trait GpuSampler: Sized {
    fn init(opts: SamplerOptions) -> Result<Self, RendererError>;
    // deinit = Drop
}

/// Zig `metal/Pipeline.zig` — a value carried into steps. No methods are
/// called on it from generic.zig; it is only moved/copied and read as a
/// Step field, so the bound is a marker (plus Drop for the Shaders owner).
pub trait GpuPipeline: Sized {}

/// Zig `metal/Frame.zig`. Generic over A so renderPass() names A::RenderPass.
pub trait GpuFrame<A: GraphicsApi + ?Sized>: Sized {
    fn render_pass(&self, attachments: &[Attachment<A>]) -> A::RenderPass;
    fn complete(&mut self, sync: bool);
}

/// Zig `metal/RenderPass.zig`.
pub trait GpuRenderPass<A: GraphicsApi + ?Sized>: Sized {
    fn step(&self, s: Step<A>);
    fn complete(&self);
}

/// Zig `metal/shaders.zig` `Shaders`.
pub trait GpuShaders: Sized {
    fn deinit(&mut self);              // takes alloc in Zig; Rust owns via Drop, kept for parity
    fn is_defunct(&self) -> bool;      // field `defunct`
    fn pipelines(&self) -> &PipelineCollection<Self::Pipeline>;
    fn post_pipelines(&self) -> &[Self::Pipeline];
    type Pipeline: GpuPipeline;
}
```

### A.3 The generic `Step` / `Attachment` / `Draw` — the hierarchy's sharpest cost

generic.zig **builds** these inline. To build them from code generic over `A`, they must be
generic structs whose fields name `A`'s associated types and the raw buffer handle. That means
`Step`/`Attachment` are **not** associated types on `GpuRenderPass` (that would make them
un-constructible generically) but *free generic structs* the trait method accepts:

```rust
pub struct Draw { pub kind: MTLPrimitiveType, pub vertex_count: usize, pub instance_count: usize }

pub enum AttachmentTarget<A: GraphicsApi + ?Sized> {
    Texture(A::Texture),
    Target(A::Target),
}
pub struct Attachment<A: GraphicsApi + ?Sized> {
    pub target: AttachmentTarget<A>,
    pub clear_color: Option<[f64; 4]>,
}

pub struct Step<'a, A: GraphicsApi + ?Sized> {
    pub pipeline: &'a A::Pipeline,
    pub uniforms: Option<&'a <A::Buffer<Uniforms> as GpuBuffer<Uniforms>>::Raw>,
    pub buffers: &'a [Option<&'a RawBufferOf<A>>],   // see note
    pub textures: &'a [Option<&'a A::Texture>],
    pub samplers: &'a [Option<&'a A::Sampler>],
    pub draw: Draw,
}
```

Note the `uniforms`/`buffers` element type: Zig's `Step.buffers` is `[]const ?objc.Object` — a
slice of *raw MTLBuffer handles* erased of `T`. Different `Buffer<T>` values feed the same
slice (`frame.cells.buffer` is `Buffer<CellText>::Raw`, `frame.cells_bg.buffer` is
`Buffer<CellBg>::Raw`). For that to typecheck generically, **all `A::Buffer<T>::Raw` must be the
same type** — i.e. `GpuBuffer<T>::Raw` must be a `T`-independent associated type of `A`, not of
`Buffer<T>`. So the raw handle has to be hoisted to `GraphicsApi::RawBuffer` and `GpuBuffer<T>`
constrained `type Raw = A::RawBuffer`. That is another cross-cutting associated type introduced
**purely** to model an erased Objective-C pointer that, in the concrete world, is just
`Retained<ProtocolObject<dyn MTLBuffer>>`. This is the single clearest illustration of the
trait tax; it is trivial monomorphically (§F) where `buffers: &[Option<&ProtocolObject<dyn
MTLBuffer>>]`.

The by-value-vs-borrow point: Zig copies `Texture`/`Sampler`/`Pipeline` *by value* into the
Step (a struct copy that shares the underlying retained ObjC object without a refcount bump).
Rust `Retained` is `Clone`-not-`Copy`; cloning bumps the refcount. To avoid semantic drift
(extra retains/releases per step, per frame) the Step fields above are **borrows** (`&'a …`),
which is the faithful zero-refcount analogue. The lifetimes thread through `render_pass`/`step`.

---

## §B — The `Buffer(comptime T)` mapping

**Decision: a concrete generic `struct Buffer<T: Copy>` in `metal/buffer.rs`** — the one place
in this whole port where a Rust generic maps 1:1 onto a Zig comptime generic with **zero
friction**, because `Buffer(T)` is a *leaf* value type (not a trait-object seam). This holds in
**both** designs:

- **Monomorphic (recommended):** the renderer names `metal::Buffer<Uniforms>` etc. directly.
- **Trait path:** `GraphicsApi::Buffer<T>` is a GAT aliased to it — `type Buffer<T: Copy> =
  metal::Buffer<T>` inside `impl GraphicsApi for Metal`, with `impl<T: Copy> GpuBuffer<T> for
  metal::Buffer<T>`.

```rust
// metal/buffer.rs — ports metal/buffer.zig `Buffer(comptime T)`
pub struct Buffer<T: Copy> {
    opts: BufferOptions,
    /// The underlying MTLBuffer.
    pub buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    /// Allocated length in *T*s (not bytes).
    len: usize,
    _marker: PhantomData<T>,
}

impl<T: Copy> Buffer<T> {
    pub fn init(opts: BufferOptions, len: usize) -> Result<Self, RendererError> { … }
    pub fn init_fill(opts: BufferOptions, data: &[T]) -> Result<Self, RendererError> { … }
    pub fn sync(&mut self, data: &[T]) -> Result<(), RendererError> { … }          // memcpy; needs T: Copy
    pub fn sync_from_array_lists(&mut self, lists: &[Vec<T>]) -> Result<usize, RendererError> { … }
    pub fn raw(&self) -> &ProtocolObject<dyn MTLBuffer> { &self.buffer }
}
// deinit(): Zig `release`; Rust Drop via Retained.
```

**Element types instantiated (6, enumerated):** `Buffer<crate::shaders::Uniforms>` (uniform
buffer), `Buffer<crate::shaders::CellBg>` (= `Buffer<[u8;4]>`, bg cells), `Buffer<crate::
shaders::CellText>` (fg cells), `Buffer<crate::shaders::BgImage>` (bg image vertex),
`Buffer<crate::shadertoy::Uniforms>` (custom-shader uniforms), `Buffer<crate::shaders::Image>`
(image.zig per-placement vertex). All six are `#[repr(C)] Copy` PODs (or a `[u8;4]` alias), so
`T: Copy` is satisfied and the `sync` memcpy is sound.

**Why not the alternatives:**
- *Associated type per concrete element* (`type UniformBuffer; type CellTextBuffer; …`): 6
  near-identical assoc types, discards the single Zig generic, and still needs a shared
  constructor surface. Rejected — strictly worse than a GAT and unfaithful to `Buffer(T)`.
- *GAT as the only mechanism, no concrete struct*: impossible — you still need one concrete
  type to instantiate; the GAT must point *somewhere*. The concrete `struct Buffer<T>` is that
  somewhere. The GAT is only the (optional) trait veneer.

**GAT/where-clause complications (trait path only):** GATs are stable (Rust ≥1.65). The only
wrinkle is the `T: Copy` bound must be declared on the GAT (`type Buffer<T: Copy>`) **and** it
must match `GpuBuffer<T: Copy>`; call sites `A::Buffer<CellText>` then require `CellText: Copy`
(true). `sync_from_array_lists` takes `&[Vec<T>]` — Zig's `[]const ArrayListUnmanaged(T)`; the
already-ported `ArrayListCollection<CellText>.lists` is a `Vec<Vec<CellText>>`, so `&fg_rows.
lists` is `&[Vec<CellText>]` and matches directly. No `for<'a>` HRTB is needed. Monomorphically,
none of this arises.

---

## §C — Options types (`bufferpkg.Options`, `Texture.Options`, `Sampler.Options`)

These are **plain POD structs owned by the metal leaves** (concrete in both designs). They are
*inputs* the api hands to the leaf constructors; they carry a `device` handle plus flags. They
do **not** need to be associated types — they never vary by backend within R-d, and even under
the trait path OpenGL would define its *own* leaf `Options` and its own `bufferOptions()` return
type, so the cleanest is: **each leaf owns its `Options`; the api's `*_options()` methods return
that leaf's concrete `Options`.** Under the trait, if OpenGL is ever added, the option types
become associated (`type BufferOptions; …`); for R-d they are concrete Metal structs. I keep
them concrete and name them `BufferOptions`/`TextureOptions`/`SamplerOptions` in `metal::`.

```rust
// metal/buffer.rs
pub struct BufferOptions {
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub resource_options: MTLResourceOptions,   // composed via typed setters (blueprint [F3])
}
// metal/texture.rs
pub struct TextureOptions {
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub pixel_format: MTLPixelFormat,
    pub resource_options: MTLResourceOptions,
    pub usage: MTLTextureUsage,
}
// metal/sampler.rs
pub struct SamplerOptions {
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub min_filter: MTLSamplerMinMagFilter,
    pub mag_filter: MTLSamplerMinMagFilter,
    pub s_address_mode: MTLSamplerAddressMode,
    pub t_address_mode: MTLSamplerAddressMode,
}
```

`Target::Options` (§ `metal/Target.zig`) is likewise a concrete `metal::target::Options
{ device, width, height, pixel_format, storage_mode }`, but it is **internal** — `initTarget`
builds it from the Metal struct's own fields and never crosses the trait boundary, so it is not
part of the api surface. Same for `Pipeline::Options` and `RenderPass::Options`.

`ImageTextureFormat` (Metal-local enum `{gray, rgba, bgra}` with `toPixelFormat(srgb)`): since
`imageTextureOptions(format, srgb)` is on the trait and image.zig calls it with `.rgba`, the
enum must be visible where image.zig's port lives. It is backend-agnostic in value (the three
logical formats) but its `toPixelFormat` mapping is Metal-specific. Map it to a concrete
`crate::ImageTextureFormat` enum (used by the ported image module) whose Metal `toPixelFormat`
lives in the Metal leaf. Monomorphically it can just live in `metal::`.

The `*_options()` methods flow: Metal's six `*BufferOptions` are `pub const … = bufferOptions;`
Zig aliases — one body. In Rust that is one private `fn buffer_options(&self) -> BufferOptions`
that the six trait methods each call (`fn uniform_buffer_options(&self){ self.buffer_options() }`),
preserving the six distinct names generic.zig/image.zig call. (Rust can't alias methods by
`pub const`; six one-line forwarders is the faithful, no-loss mapping — note it as a PORT NOTE.)

---

## §D — The `GenericRenderer<A>` struct skeleton

Field-by-field port of generic.zig's `Renderer` struct (lines 96–232). Backend-typed fields are
shown with the trait-generic form; the monomorphic form (§F) substitutes `Metal`/`metal::*` for
`A`/`A::*` and drops the `<A>`.

```rust
pub struct GenericRenderer<A: GraphicsApi> {
    pub alloc: Allocator,
    /// Held whenever drawFrame-touched state is read/modified.
    pub draw_mutex: rustty_zigstd::thread::Mutex<()>,   // guard-less, state.rs precedent
    pub config: DerivedConfig,
    pub surface_mailbox: apprt::surface::Mailbox,
    pub grid_metrics: rustty_font::Metrics,
    pub size: renderer::Size,                            // rustty_render_types::Size
    pub focused: bool,
    pub custom_shader_focused_changed: bool,
    pub scrollbar: terminal::Scrollbar,
    pub scrollbar_dirty: bool,
    pub last_bottom_node: Option<usize>,
    pub last_bottom_y: terminal::size::CellCountInt,
    pub search_matches: Option<renderer::Message::SearchMatches>,
    pub search_selected_match: Option<renderer::Message::SearchMatch>,
    pub search_matches_dirty: bool,

    pub cells: crate::cell::Contents,                   // ✅ ported (concrete)
    pub cells_rebuilt: bool,
    pub uniforms: crate::shaders::Uniforms,             // ✅ concrete (§1.2)
    pub custom_shader_uniforms: crate::shadertoy::Uniforms,  // ✅ concrete
    pub first_frame_time: Option<Instant>,
    pub last_frame_time: Option<Instant>,

    pub font_grid: *mut rustty_font::SharedGrid,        // Zig `*font.SharedGrid`
    pub font_shaper: rustty_font::Shaper,
    pub font_shaper_cache: rustty_font::ShaperCache,

    pub images: crate::image::State,                    // ⚠ image.zig — R-d prerequisite
    pub bg_image: Option<crate::image::Image>,          // ⚠ prerequisite
    pub bg_image_changed: bool,
    pub bg_image_buffer: crate::shaders::BgImage,       // ⚠ BgImage — prerequisite (§1.2)
    pub bg_image_buffer_modified: usize,

    /// Graphics API state.
    pub api: A,                                         // monomorphic: `Metal`
    pub display_link: Option<DisplayLink>,              // macOS seam; unimplemented!/None in R-d
    pub health: AtomicHealth,                           // std.atomic.Value(Health)

    pub swap_chain: SwapChain<A>,                       // monomorphic: SwapChain (no <A>)
    pub target_config_modified: usize,
    pub reinitialize_shaders: bool,
    pub has_custom_shaders: bool,
    pub shaders: A::Shaders,                            // monomorphic: metal::shaders::Shaders

    pub terminal_state: terminal::RenderState,
    pub terminal_state_frame_count: usize,
    pub overlay: Option<crate::Overlay>,                // ⚠ Overlay.zig — R-d prerequisite
}
```

Nested types (ported alongside):

```rust
struct SwapChain<A: GraphicsApi> {
    frames: [FrameState<A>; { A::SWAP_CHAIN_COUNT }],   // see note on const-generic array below
    frame_index: usize,                                  // IntFittingRange(0, buf_count)
    /// Triple-buffer backpressure. `= .{ .permits = buf_count }`.
    frame_sema: rustty_zigstd::thread::Semaphore,        // ✅ landed (with_permits(N))
    defunct: bool,
}

struct FrameState<A: GraphicsApi> {
    uniforms: A::Buffer<crate::shaders::Uniforms>,       // metal::Buffer<Uniforms>
    cells: A::Buffer<crate::shaders::CellText>,
    cells_bg: A::Buffer<crate::shaders::CellBg>,
    grayscale: A::Texture,  grayscale_modified: usize,
    color: A::Texture,      color_modified: usize,
    target: A::Target,      target_config_modified: usize,
    bg_image_buffer: A::Buffer<crate::shaders::BgImage>,
    bg_image_buffer_modified: usize,
    custom_shader_state: Option<CustomShaderState<A>>,
}

struct CustomShaderState<A: GraphicsApi> {
    front_texture: A::Texture,
    back_texture: A::Texture,
    sampler: A::Sampler,
    uniforms: A::Buffer<crate::shadertoy::Uniforms>,
}
```

Array-length note: `[FrameState; A::SWAP_CHAIN_COUNT]` needs `SWAP_CHAIN_COUNT` as a
const-generic in array position — not expressible from an associated const on a *type parameter*
without `generic_const_exprs` (unstable). Faithful workarounds: (1) **monomorphic** — the array
is `[FrameState; 3]` with a `const SWAP_CHAIN_COUNT: usize = 3` (trivial, recommended); (2)
trait path — make `GenericRenderer<A, const N: usize>` and set `N = Metal::SWAP_CHAIN_COUNT` at
the `type Renderer = …` alias, or store `frames: SmallVec<[FrameState<A>; 3]>`/`Vec` sized to
`A::SWAP_CHAIN_COUNT` at `init` (Zig's array is fixed but a length-3 `Vec` is behavior-identical;
document as a PORT NOTE). This is a real, if small, friction of the trait path.

Crate-root alias (ports the comptime `switch(build_config.renderer)`; see lib.rs:63 seam):

```rust
// monomorphic (recommended)
#[cfg(target_os = "macos")]
pub type Renderer = crate::metal::Renderer;   // = the concrete GenericRenderer-shaped struct

// trait path
#[cfg(target_os = "macos")]
pub type Renderer = GenericRenderer<crate::metal::Metal>;
```

`DerivedConfig` (generic.zig:536) is ported concretely (it is not backend-generic); it replaces
the `RendererDerivedConfig {}` uninhabited placeholder currently in lib.rs (line 82).

---

## §E — The Chunk-0 compile boundary

Chunk 0's gate is `cargo build -p rustty-renderer` with **only** (trait/alias defs + `api.rs`
enum aliases + `build.rs` + `shaders.metal` copy), before any `metal/*` concrete leaf bodies
exist. A trait *declaration* compiles without impls, **but a method signature that names a
concrete type which does not yet exist does not compile.** Precise split:

### E.1 Monomorphic recommendation — Chunk 0 is *smaller and safer*

There is **no free-standing generic trait** that must compile before the leaves. Chunk 0 = the
things with no dependency on unwritten bodies:
- `metal/api.rs` — the enum re-export/alias module (`MTLPixelFormat`/`MTLResourceOptions`/…),
  mapping objc2-metal types to ghostty names (blueprint §2). Compiles standalone. ✅
- `build.rs` — the metallib/source shader build. ✅
- `crate::shaders` additions (`BgImage`, `Image` structs — pure `#[repr(C)]`, no GPU). ✅
- The concrete leaf **struct headers** *may* be stubbed with fields + `unimplemented!()` method
  bodies if you want an early compile gate, but they aren't required to compile anything else.
- The `pub type Renderer = …` alias and the marker trait (§F) are **not** added until the
  concrete `Renderer` exists (Chunk 3–4).

So Chunk 0 monomorphic builds clean trivially: it is just `api.rs` + `build.rs` + POD structs.
No signature references a not-yet-written concrete type because nothing generic is declared yet.

### E.2 Trait path — exactly what is declarable in Chunk 0

If the hierarchy is added in Chunk 0 (as the blueprint's plan states), then only the parts whose
signatures name **already-concrete** types compile. Declarable in Chunk 0:
- `trait GraphicsApi`'s **associated type declarations** (`type Target: GpuTarget; …`) and
  **const declarations** — these name only *other traits* (also declared here), not concrete
  types. ✅
- The **empty/marker sub-traits** `GpuPipeline {}`, and the *method-less* parts of
  `GpuTarget`/`GpuSampler`/`GpuTexture` where the signature uses only concrete/std types
  (`fn width(&self) -> usize`, `fn replace_region(&self, …, data: &[u8]) -> Result<(),
  RendererError>`). ✅ — provided `RendererError`, `BufferOptions`, `TextureOptions`,
  `SamplerOptions`, `ImageTextureFormat`, `SurfaceSize`, `shadertoy::Target`, and the
  `crate::shaders::*` structs (including the new `BgImage`/`Image`) all exist in Chunk 0.
  → **Action: those concrete POD/error/option types must be authored in Chunk 0**, not Chunk 1,
  or the trait signatures that name them won't compile. They have no GPU dependency, so this is
  fine, but it is a Chunk-0 obligation to call out.
- `GraphicsApi` methods whose signatures name only the above concretes + `Self::*` associated
  types: `uniform_buffer_options`, `texture_options`, `sampler_options`,
  `image_texture_options`, `init_target` (`-> Result<Self::Target,_>`), `init_atlas_texture`
  (`-> Result<Self::Texture,_>` + `&rustty_font::Atlas`), `surface_size`, `set_blending`,
  `draw_frame_start/end`, the defaulted hooks. ✅ (they reference `Self::Target`/`Self::Texture`,
  which are trait-declared, **not** the concrete `metal::Target`.)

**Must wait for Chunk 1** (or need a Chunk-0 forward declaration):
- `begin_frame(&self, renderer: *mut GenericRenderer<Self>, …)` — names `GenericRenderer<Self>`.
  The `GenericRenderer` struct definition (§D) references `A::Buffer<…>`, `A::Texture`, etc.,
  which are trait-associated (OK), **but** the struct also embeds concrete
  `crate::cell::Contents`, `crate::image::State`, `crate::Overlay`, `rustty_font::*` — those must
  exist. `Contents`/`shaders` exist; **`image::State` and `Overlay` are R-d prerequisites** and
  must land (at least as headers) for `GenericRenderer<A>` to name them. So `begin_frame`'s
  signature (and the whole `GenericRenderer<A>` definition) is only declarable once those
  prerequisite headers exist. Put the prerequisite *headers* (image.zig/Overlay.zig struct
  shells) in Chunk 0 if `begin_frame` is to be declared there; otherwise defer `begin_frame`'s
  declaration.
- `init(alloc, options) -> Result<Self, _>` names `renderer::Options` — fine if `Options` is
  ported in Chunk 0 (it is a plain options struct; author it in Chunk 0).
- Nothing in the trait *bodies* is written in Chunk 0 (traits have no bodies except the defaulted
  hooks, which are empty and compile). The `impl GraphicsApi for Metal` and all leaf method
  bodies are Chunk 1–2.

**Bottom line for E:** monomorphic → Chunk 0 is just `api.rs` + `build.rs` + POD structs, always
green. Trait path → Chunk 0 must *also* author every concrete non-GPU vocabulary type
(`RendererError`, the three `*Options`, `ImageTextureFormat`, `SurfaceSize`, `Options`,
`crate::shaders::BgImage`/`Image`) **and** the `image::State`/`Overlay` prerequisite headers, or
the `GenericRenderer<A>` definition and `begin_frame` signature won't compile — a materially
larger Chunk 0 with more ways to go red. This asymmetry is itself an argument for §F.

---

## §F — Monomorphic alternative (honest evaluation + recommendation)

**Recommendation: port `generic.zig` MONOMORPHICALLY.** Keep the `GraphicsApi` *name* and the
crate-root `cfg` alias as a documented Goal-2 seam, but write the renderer body against concrete
Metal leaf types, not a generic `A`.

### F.1 What "monomorphic" means concretely
- `GenericRenderer<A>` → a concrete `Renderer` struct (§D with `A`→`Metal`, `A::Target`→
  `metal::Target`, `A::Buffer<T>`→`metal::Buffer<T>`, `A::Shaders`→`metal::shaders::Shaders`,
  …). Lives in `metal/renderer.rs` (or `generic.rs` kept as the module name for provenance).
- `metal::Buffer<T>` stays a **concrete generic** (§B) — the one faithful Rust generic.
- `Step`/`Attachment`/`Draw` are concrete structs with concrete fields (`buffers:
  &[Option<&ProtocolObject<dyn MTLBuffer>>]`, `textures: &[Option<&metal::Texture>]`, …). No
  `RawBuffer` associated type, no `AttachmentTarget<A>` generic enum.
- `frame.complete(sync)`, `buffer.sync(...)`, `pass.step(...)`, `texture.replace_region(...)`
  are **inherent methods** on the concrete leaves — no sub-traits needed.
- The genericity is preserved as: (1) the module-level provenance (`generic.rs` header comment
  documents that this is `GenericRenderer(Metal)` monomorphized), and (2) an **optional** thin
  marker `trait GraphicsApi` that `Metal` `impl`s carrying *only* the three consts + the assoc
  types, used by nothing in the hot path but standing as the compile-checked Goal-2 extension
  point. (Even this can be dropped; the header comment + the OpenGL blueprint suffice.)

### F.2 Tradeoffs

| Axis | Trait hierarchy (blueprint) | Monomorphic (recommended) |
|---|---|---|
| Behavioral fidelity (R1) | identical | **identical** — it is what Zig comptime emits |
| Comment/test fidelity (R1) | identical | identical (same bodies, same tests, same comments) |
| Rust-only glue volume | **high**: 8 traits, GAT, `RawBuffer` hoist, `Step<A>`/`Attachment<A>` generics, lifetime threading, const-generic `N` workaround | **low**: concrete structs + one concrete `Buffer<T>` |
| Adversarial-review surface | large (every bound + variance + lifetime is a place to be wrong) | small (concrete types; the reviewer checks behavior, not type plumbing) |
| Chunk-0 fragility | must author all vocabulary + prerequisite headers up front (§E.2) | minimal (§E.1) |
| `std.mem.swap(Texture,…)`, by-value Step fields | need `Sized` bounds + borrow reshaping to avoid refcount drift | direct: `std::mem::swap(&mut a, &mut b)`; borrow the leaves |
| `api.blending = …` field write | needs `set_blending` trait method | direct field write `self.api.blending = …` |
| `begin_frame(*mut Renderer<Self>)` recursion | recursive trait↔struct coupling | plain: `*mut Renderer` (concrete; raw ptr, NOT `&mut` — see [FIX-2]) |
| Goal-2 second backend (OpenGL) | trait already there | must generalize later (see F.3) |
| Faithfulness to Zig *mechanism* | closer to the comptime shape | equal to the *emitted* shape; the doc-comment hierarchy is preserved as comments |

### F.3 The only real cost of monomorphic, and why it's acceptable
The single thing the trait path buys is a ready-made slot for OpenGL/WebGL. But: (a) OpenGL is
Linux and **explicitly deferred** — when it lands it is a *large* tier of its own (`OpenGL.zig`
+ `opengl/*` + GLSL pipeline), at which point *introducing* the `GraphicsApi` trait and
generalizing `Renderer` is a mechanical, behavior-preserving refactor (extract-trait is one of
the safest refactors, and the concrete Metal leaves already have exactly the right method
shapes — §A was derived from them). (b) R1 is explicit that genericity is a comptime convenience,
not a parity target. (c) Doing the trait now means paying its full cost to abstract over a set of
**one**, and carrying that complexity through every subsequent R-d chunk and its adversarial
review — with a non-trivial chance of a subtle variance/lifetime/refcount bug that has *no*
analogue in the reference (a pure Rust-only defect). The asymmetry — certain complexity now vs.
a clean mechanical extraction later, exactly when a second impl actually exists to validate the
abstraction — favors monomorphic.

### F.4 Recommendation, stated plainly
Port monomorphically. Preserve the `GraphicsApi` interface **as documentation** (the generic.zig
doc-comment hierarchy diagram is R1-preserved verbatim in `generic.rs`) and as the crate-root
`cfg` alias. Optionally keep a minimal marker `trait GraphicsApi` (consts + assoc types only) as
a typed Goal-2 anchor, but do **not** write the renderer body against it. If the reviewer
prioritizes the Goal-2 seam over the complexity cost, §A–§E specify the full hierarchy and it is
implementable as written — the leaf work (Chunk 1) and behavior are identical either way, so the
decision can even be deferred to after Chunk 1 without rework.

---

## §G — Comptime constructs that don't map (beyond `autoAttribute`)

`autoAttribute` (Pipeline.zig:154) is already flagged by the blueprint (§2 [F-vertex]: hand-write
per-struct attribute tables via a `trait VertexLayout`/const table). The remaining GraphicsAPI-
facing comptime constructs and their hand-written Rust equivalents:

1. **`@hasDecl(GraphicsAPI, "loopEnter")` capability probes** (generic.zig:866,875,883,891,899,
   921,940,969). Zig calls the hook only if the type declares it. Rust equivalent: **defaulted
   trait methods** (§A.1) with empty/`Ok(())` bodies; `Metal` overrides the ones it needs
   (`loop_enter`, `display_realized`, …), the rest inherit the no-op default. Monomorphically:
   just concrete `Metal` methods, some `{}`/`unimplemented!("windowing-host")`. Faithful: the
   observable behavior ("hook runs iff implemented") is preserved.

2. **`SwapChain.buf_count = GraphicsAPI.swap_chain_count` as an array length** (246). Zig uses
   the comptime const to size `[buf_count]FrameState` and `IntFittingRange(0, buf_count)` for
   `frame_index`. Rust: monomorphic `const SWAP_CHAIN_COUNT: usize = 3` → `[FrameState; 3]` and
   `frame_index: usize` (the `IntFittingRange` micro-optimization — smallest int holding 0..=3 —
   has no behavioral effect; use `usize` with a PORT NOTE). Trait path: see §D array-length note
   (const-generic `N` or a length-`N` `Vec`).

3. **`PipelineCollection` via `@Type(.{.@"struct" = …})`** (metal/shaders.zig:78–95). Zig
   *generates* a struct whose fields are named by `pipeline_descs[i][0]` ("bg_color", "cell_bg",
   "cell_text", "image", "bg_image"), each `Pipeline`. Rust has no type-level struct synthesis:
   **hand-write** the struct:
   ```rust
   pub struct PipelineCollection<P = metal::Pipeline> {
       pub bg_color: P, pub cell_bg: P, pub cell_text: P, pub image: P, pub bg_image: P,
   }
   ```
   and the `inline for (pipeline_descs)` init/deinit loops (shaders.zig:131–144,176–178) become
   explicit per-field construction/drop. The `pipeline_descs` comptime table itself becomes a
   `const` array of a `PipelineDescription` struct (already POD in Zig); the `initPipeline`
   dispatch on `vertex_attributes: ?type` (a comptime `?type`) becomes the hand-written
   `VertexLayout` table selection from the `autoAttribute` item. Field names are read by
   generic.zig as `shaders.pipelines.cell_text` etc., so the field names MUST match exactly.

4. **`Buffer(comptime T)`** (metal/buffer.zig:21) — maps cleanly to `struct Buffer<T: Copy>`
   (§B); the *only* comptime-generic that is a true Rust generic, no hand-writing needed.

5. **`Pipeline.init(comptime VertexAttributes: ?type, opts)`** (Pipeline.zig:44) — the
   comptime `?type` selects whether to build a vertex descriptor. Rust: two code paths /
   `Option<&'static VertexLayout>` (or two methods `init` / `init_no_attrs`), driven by the
   hand-written table from item 3, not a type parameter.

None of these is on the pixel-deciding CPU path (they are pipeline/buffer *plumbing*); all have
exact, mechanical hand-written equivalents. No behavior is lost.

---

## OPEN RISKS (things I could not fully verify)

1. **`Retained` by-value vs Zig value-copy semantics.** Zig copies `Texture`/`Sampler`/
   `Pipeline` structs by value into `Step`, sharing the underlying retained ObjC object with **no
   refcount change** (manual ARC). Rust `Retained` clone bumps the refcount. I specified Step
   fields as **borrows** to preserve zero-refcount-churn, but I have not compiled it — the
   lifetime threading through `renderPass`→`step` (and through `images.draw` which builds a
   `Buffer` per placement and drops it after the step) needs a real typecheck. If borrows prove
   awkward inside the per-frame draw loop, the fallback is owned `Retained` clones (extra
   retain/release per step) — behavior-equivalent but with measurable churn; flag for Goal-2.
   (Monomorphic makes this easier to get right but does not eliminate it.)

2. **`std.mem.swap(Texture, &front, &back)`** (custom shader state). Maps to
   `std::mem::swap(&mut a, &mut b)` — trivially fine for concrete/`Sized` types in both designs;
   only noted because it forbids `?Sized` associated `Texture`.

3. **Const-generic swap-chain length under the trait path** — `[FrameState<A>; A::
   SWAP_CHAIN_COUNT]` is not expressible without `generic_const_exprs` (unstable). The `Vec`/
   const-generic-`N` workarounds (§D) are behavior-identical but I have not confirmed the
   preferred one compiles cleanly across the `SwapChain` methods (`nextFrame` indexing, `deinit`
   sema drain). Monomorphic sidesteps it entirely (`[_; 3]`).

4. **`begin_frame` recursive coupling** — `A::begin_frame(&mut GenericRenderer<Self>)` (trait) or
   `Metal::begin_frame(&mut Renderer)` (mono) creates a Frame that stashes a raw `*mut Renderer`
   and, on the sync path, calls back `renderer.frame_completed()` / `renderer.api.present()`.
   This is the `@fieldParentPtr`/container_of trampoline (memory `vtc-effects-trampoline-seam`,
   `intrusive-ptr` precedents). The raw-pointer aliasing (block holds `*mut Renderer` while the
   renderer is mid-`drawFrame`) must respect `mut-args-strongly-protected` — I have not proven
   the borrow model; use raw `*mut` (not `&mut`) exactly as Zig does, but this needs a Miri/ASan
   pass on the concrete impl.

5. **`imageTextureOptions`/`ImageTextureFormat` placement** — `image.zig` is an R-d prerequisite
   not yet ported; I inferred its api usage (`api.imageTextureOptions(.rgba, true)`,
   `Buffer(shaders.Image).initFill`) from the Zig source. Whether `ImageTextureFormat` lands in
   `crate::` or `metal::` depends on where the image module is placed relative to the trait —
   confirm when image.zig is ported.

6. **`renderer::Options` / `renderer::Message::SearchMatches` shapes** — referenced in `init`
   and the renderer struct but their full Rust ports are outside this sub-decision; I assumed
   they are (or will be) concrete crate types with no backend edge. Confirm `Options` is authored
   in Chunk 0 for the trait path (§E.2).

7. **`AlphaBlending` import path** — `self.api.blending` field / `set_blending` uses
   `configpkg.Config.AlphaBlending`. `rustty-config` already depends on `rustty-renderer`, so
   naming the config type from the renderer would cycle (G7). The blending enum may need to be a
   small renderer-local type or hoisted (as `Health`/`size` were). Verify the acyclic path before
   wiring `set_blending`/`initShaders`'s `blending.isLinear()`.
</content>
</invoke>
