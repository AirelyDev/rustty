# GPU R-d Chunk 4 — the OFFSCREEN GOLDEN-IMAGE GATE (the R-d contract)

> **LANDED.** The golden is GREEN:
> `generic::macos_impl::tests::golden_renders_cursor_fg_bg` builds a deterministic
> terminal state ("Ab" + cursor), runs `update_frame` → `draw_frame(true)` through the
> real Metal encode/submit path into an offscreen IOSurface-backed `BGRA8Unorm` target,
> reads the pixels back, and asserts the render structure (bg/fg/cursor). The
> `draw_frame` raw-ptr aliasing discipline is proven clean under **both** Stacked and
> Tree Borrows by `.port/phase-2/mirihar-chunk3b` (`drawframe_shipped` clean;
> `drawframe_naive` demonstrates the SB UB it avoids). Determinism pins:
> `AlphaBlending::Native` ⇒ `BGRA8Unorm`, `colorspace = Srgb` ⇒ `use_display_p3 = false`,
> `offscreen_size == size.screen` ⇒ `size_changed == false`. The seam audit + the
> honest `PORT STATUS`/G3b decision are the Chunk-5 finale
> (`.port/phase-2/GPU_RD_CHUNK5_DESIGN.md`).

**Design doc. Design ONLY — touches NO `crates/`.** Architect deliverable for the
Chunk-4 workflow (`.port/phase-2/wf-gpu-rd-chunk4-golden.js`). Ports (from
`ghostty/src/renderer/generic.zig` + `metal/shaders.zig` + `metal/{Frame,Target,RenderPass}.zig`
+ `shaders/shaders.metal`) into `crates/rustty-renderer` (MONOMORPHIC `Renderer` over
`Metal`). The R-d contract: **render a deterministic terminal state OFFSCREEN
(Metal → IOSurface), read back pixels, assert the render.**

STATE going in: Chunks 0/1/2/3a/3b/3c are DONE + committed. The CPU side is complete
— `Renderer` struct + `SwapChain`/`FrameState` (3a), the whole `metal/*` backend
(0/1/2), the cell-fill (`rebuild_cells`/`rebuild_row`/`add_*` filling `Contents`, 3b),
`update_frame` + the uniforms math (3c). What REMAINS: `drawFrame` (GPU encode+submit),
the completion path, atlas→GPU sync, shader-pipeline init on the `Renderer`, the
**packed-bool byte-faithfulization**, and a `Renderer` that actually RENDERS offscreen.

> **CRITICAL GATE-COVERAGE NOTE (from the brief):** G3b comment-coverage SKIPS
> `rustty-renderer`, so `generic.rs`/`shaders.rs` R1 comment fidelity is NOT
> gate-enforced. The adversary + verify panel are the ONLY R1 backstop. Preserve
> EVERY reference line/comment verbatim by hand.

---

## Verified reference facts (read, not assumed)

**`metal/shaders.zig` (the MONOMORPHIC target — this is what the `.metal` shaders read):**

```zig
pub const Uniforms = extern struct {
    projection_matrix: math.Mat align(16),   // [16]f32 = 64B
    screen_size: [2]f32 align(8),
    cell_size:   [2]f32 align(8),
    grid_size:   [2]u16 align(4),
    grid_padding:[4]f32 align(16),
    padding_extend: PaddingExtend align(1),   // packed struct(u8)
    min_contrast: f32 align(4),
    cursor_pos:  [2]u16 align(4),
    cursor_color:[4]u8 align(4),
    bg_color:    [4]u8 align(4),
    bools: extern struct {                     // <-- extern, NOT packed: 4 separate bytes
        cursor_wide: bool align(1),
        use_display_p3: bool align(1),
        use_linear_blending: bool align(1),
        use_linear_correction: bool align(1) = false,
    },
    const PaddingExtend = packed struct(u8) { left,right,up,down: bool, _padding: u4 = 0 };
};
pub const CellText = extern struct {           // @sizeOf == 32 (reference test{})
    glyph_pos:  [2]u32 align(8),
    glyph_size: [2]u32 align(8),
    bearings:   [2]i16 align(4),
    grid_pos:   [2]u16 align(4),
    color:      [4]u8 align(4),
    atlas: Atlas align(1),                      // enum(u8)
    bools: packed struct(u8){ no_min_contrast,is_cursor_glyph: bool, _padding: u6 } align(1),
};
pub const CellBg = [4]u8;                       // already ABI (uchar4)
pub const BgImage = extern struct {
    opacity: f32 align(4),
    info: Info align(1),                        // packed struct(u8){ position:enum(u4), fit:enum(u2), repeat:bool, _padding:u1 }
};
```

**`shaders/shaders.metal` — the exact bit reads (this is the ground truth):**

```c
enum Padding : uint8_t { EXTEND_LEFT=1u, EXTEND_RIGHT=2u, EXTEND_UP=4u, EXTEND_DOWN=8u };
struct Uniforms { float4x4 projection_matrix; float2 screen_size; float2 cell_size;
  ushort2 grid_size; float4 grid_padding; uint8_t padding_extend; float min_contrast;
  ushort2 cursor_pos; uchar4 cursor_color; uchar4 bg_color;
  bool cursor_wide; bool use_display_p3; bool use_linear_blending; bool use_linear_correction; };
enum CellTextBools : uint8_t { NO_MIN_CONTRAST=1u, IS_CURSOR_GLYPH=2u };
enum BgImagePosition: uint8_t { BG_IMAGE_POSITION = 15u /*bits0-3*/, ... };
enum BgImageFit:      uint8_t { BG_IMAGE_FIT = 3u<<4 /*bits4-5*/, ... };
enum BgImageRepeat:   uint8_t { BG_IMAGE_REPEAT = 1u<<6 /*bit6*/ };
// cell_text_vertex: uint8_t bools [[attribute(6)]];   ((in.bools & IS_CURSOR_GLYPH)==0) ...
// cell_bg_fragment:  uniforms.padding_extend & EXTEND_LEFT ...
```

**Ported layout facts (already in the tree):**
* `metal/buffer.rs` — `Buffer<T: Copy>::{init, sync(&[T]), sync_from_array_lists(&[Vec<T>]), raw()}`;
  `sync` **memcpys `T`'s raw bytes** (so the buffer element type IS the GPU ABI).
* `metal/pipeline.rs` `auto_attribute` + `metal/shaders.rs` `VertexLayoutRef` tables:
  the CellText vertex-attribute offsets are `[0,8,16,20,24,28,29]`, stride 32; BgImage
  `[0,4]`, stride 8. `bools`/`info` are read as **`MTLVertexFormat::UChar` (1 byte)**.
* `metal/target.rs` `Target::read_bgra() -> Vec<u8>` — `w*h*4` BGRA bytes (honors
  `bytesPerRow`; call after `waitUntilCompleted`). The golden readback API. **Present.**
* `metal/frame.rs` — `Frame::begin(opts, renderer:*mut(), target:*mut Target, on_completed: CompletionFn)`;
  `Frame::complete(sync)`; the SYNC path (golden path) commits → `waitUntilCompleted`
  → `self.block.sync=true` → **invokes the `CompletionFn` fn-pointer directly** (no ObjC
  block). `CompletionContext{ renderer:*mut(), target:*mut Target, sync }` — RAW `*mut`.
* `image::State::{upload, draw}` — **real** (only the *kitty pixel* path
  `kitty_update`/`prep_kitty_image` is `unimplemented!()`, guarded — the golden renders
  no kitty imagery). `drawFrame` may call them unconditionally.
* `shadertoy::load_from_files` — **not found** (un-ported); `config.custom_shaders` is the
  `RepeatablePath` **placeholder ZST**. → `Renderer::init_shaders`' custom-shader loader
  is a deferred seam (§6).
* `math::Mat = [[f32;4];4]` (align **4**, not 16) → the ergonomic `Uniforms` in-memory
  layout does NOT match the Metal ABI → **serialization is mandatory** (§2).

---

## §1 SCOPE — what lands in Chunk 4 vs what stays deferred

| Method / item | Ref | Decision | Notes |
|---|---|---|---|
| `drawFrame(sync)` | `:1430` | **LAND** | the GPU encode+submit; the whole R-d contract |
| `frameCompleted(health)` | `:1724` | **LAND** | completion trampoline target; `&self` (see §5) |
| `syncAtlasTexture(atlas, tex)` | `:3358` | **LAND** | uploads font atlas → GPU texture; load-bearing for glyphs |
| `initShaders` (Renderer) | `:836` | **LAND** (loader deferred) | referenced by `drawFrame` reinit branch; custom-shader load → empty seam (§6) |
| `deinitShaders` | `:832` | **LAND** (as `pub fn`) | one-liner `self.shaders.deinit()`; sole caller is the deferred `deinit`, so `pub` to dodge dead-code `-D warnings` (§6.4; 3a §B.4 "make pub" precedent) |
| `hasAnimations` | `:1013` | **LAND** | `-> self.has_custom_shaders`; `needs_redraw` uses it |
| `uploadBackgroundImage` | `:1828` | **LAND** (thin) | fully guarded by `if let Some(bg)`; no-op with `bg_image=None`; called by `drawFrame` |
| **GPU-ABI serialization module** `metal::gpu_abi` | — | **LAND** (new) | `UniformsGpu`/`CellTextGpu`/`BgImageGpu` + `From` + drift guards + byte test (§2) |
| **The golden test** + harness | — | **LAND** (new) | offscreen render → IOSurface readback → structural pixel asserts (§3) |
| cross-crate accessors (§7) | — | **LAND** (new) | `SharedGrid::atlas_grayscale/atlas_color`, `Atlas::modified` |
| `Renderer::init(alloc, options)` | `:658` | **DEFER** (already `unimplemented!`) | needs `Metal::init` (live NSView/IOSurfaceLayer — windowing-host seam) + real `RepeatablePath`; golden constructs directly (§4) |
| `Renderer::deinit` | `:800` | **DEFER** (already `unimplemented!`) | golden lets field `Drop` run; not on the golden path |
| `prepBackgroundImage` | `:1747` | **DEFER** | called only by `init`/`changeConfig` (deferred); does file I/O + decode. R5 seam + verbatim body |
| `changeConfig` | `:1841` | **DEFER** | not on golden path; R5 seam |
| `present` (real layer path) | `Metal.zig:253` | **DEFER** (already stub) | offscreen `present` is a no-op; the completion path reads back, never presents |
| `loopEnter`/`surfaceInit`/`setScreenSize`/display-link | various | **DEFER** (already stubs) | windowing-host / apprt seams |

**Gate-green boundary.** After Chunk 4:
`cargo build -p rustty-renderer` + `cargo build --workspace` + `cargo clippy -p rustty-renderer --all-targets -- -D warnings`
compile (all of `drawFrame`'s call graph is reachable: it references `init_shaders`,
`sync_atlas_texture`, `has_animations`, `upload_background_image`, `frame_completed`, and
the `metal::gpu_abi` serializers — none are dead. **`deinit_shaders` is the ONE exception**:
its sole caller `Renderer::deinit` (ref `:825`) stays deferred and `drawFrame`'s reinit branch
calls `self.shaders.deinit()` DIRECTLY (ref `:1500`, NOT `self.deinitShaders()`), so it lands
as `pub fn` to survive `-D warnings` — per §6.4 / the 3a §B.4 "make pub, ZERO
`#[allow(dead_code)]`" precedent. Every other Chunk-4 method IS live: `draw_frame` via the
golden `#[test]` (clippy `--all-targets` compiles tests), `frame_completed` via the trampoline,
`sync_atlas_texture`/`upload_background_image` via `draw_frame`, `init_shaders` via the reinit
branch, `has_animations` via `needs_redraw`). `cargo test -p rustty-renderer` runs
the golden test which **renders + asserts** on a Metal host and **skips cleanly** (early
`return`) on a host with no Metal device (the `metal.rs` / `test_renderer` precedent).
`check_acyclic.sh` stays green (the `gpu_abi` module is renderer-local; the two accessor
additions are on crates the renderer already depends on). `check_test_counts.py` sees the
new golden test (+ the `gpu_abi` byte test) as additions.

---

## §2 PACKED-BOOL BYTE-FAITHFULNESS — the core deliverable

### §2.0 Decision: **serialize-on-buffer-sync into `#[repr(C)]` GPU-ABI POD types**

The ergonomic `crate::shaders::{Uniforms, CellText, BgImage}` (with their **named-bool**
sub-structs) stay **UNTOUCHED**. A new **renderer-local** module `crate::metal::gpu_abi`
defines the exact Metal-ABI POD types `UniformsGpu` / `CellTextGpu` / `BgImageGpu`, their
`From<&Ergonomic>` serializers (the bit-packing), and `const{}` drift guards + a byte
unit-test. The `FrameState` GPU buffers change element type to the ABI types; `drawFrame`
converts at the sync boundary.

**Why serialize-on-sync, not repr(C)-pack-in-place (justified byte-for-byte):**

1. **`Uniforms` forces a serializer regardless.** Its ABI needs (a) `grid_padding`
   16-aligned (`math::Mat` is align-4, so Rust `#[repr(C)]` places `grid_padding` at
   offset **84, not 96**) and (b) `padding_extend` as **1 byte** (the ergonomic
   `UniformsPaddingExtend` is 4 bool bytes). The in-memory ergonomic `Uniforms` simply is
   not the ABI. Given a serializer is unavoidable for `Uniforms`, using the **same
   mechanism** for `CellText`/`BgImage` is one auditable ABI module, not a mix.
2. **Zero churn to 3b/3c.** The 3b fill code writes `CellTextBools{ no_min_contrast,
   is_cursor_glyph }` struct-literals and the 3b/3c tests assert `cur.bools.is_cursor_glyph`
   (field access) and `r.uniforms.grid_padding` etc. Keeping the ergonomic structs verbatim
   means the golden test EXTENDS the 3b/3c fixture with **no regression** to landed code.
3. **It is the literal fulfillment of the existing PORT NOTEs.** `crate::shaders`' notes
   already say *"the exact GPU byte layout … is applied by the R-d/Phase 3 GPU backend when
   this is serialized into a uniform buffer"* and *"its `_padding` bits reconstructed by the
   backend serializer."* `metal::gpu_abi` **is** that backend serializer, in the layer the
   note named. The alternative (repr(C)-pack-in-place) would *rewrite* the ergonomic structs,
   contradicting their notes and rippling into 3b/3c.

**Considered alternative (rejected, documented for the adversary):** make
`CellTextBools`/`UniformsPaddingExtend` themselves 1-byte `#[repr(transparent)]` bit-fields
with accessors, so `CellText`'s memcpy is byte-correct with no conversion. It is *arguably*
a more faithful port of a Zig `packed struct(u8)` and eliminates the per-frame CellText
conversion — but it rewrites the ergonomic structs and forces `cur.bools.is_cursor_glyph →
.is_cursor_glyph()` + literal→ctor edits across the 3b fill code and both 3b/3c tests, and
it still cannot fix `Uniforms` in place (the align-16 + offset shifts). Net: more churn,
no `Uniforms` win. The per-frame CellText conversion (below) is the accepted cost.

> **⚠ METAL-vs-OpenGL DISCREPANCY — FLAG (must land in a PORT NOTE).** The current
> `crate::shaders.rs` PORT NOTEs describe `Bools`/`PaddingExtend` as **`packed
> struct(u32)`** — that is the **OpenGL** `opengl/shaders.zig` layout (`bools`/`padding_extend`
> are each a 4-byte u32). The MONOMORPHIC port targets **Metal**, whose `metal/shaders.zig`
> has `padding_extend: packed struct(u8)` (**1 byte**) and `bools: extern struct { 4× bool
> align(1) }` (**4 separate bytes, NOT a u32**). Chunk 4's serializer MUST target the
> **Metal** ABI (verified against `.metal`: `uint8_t padding_extend` + four inline `bool`s).
> Add a PORT NOTE in `gpu_abi` (and correct/annotate the misleading "u32" refs in
> `shaders.rs`) so a future reader is not misled into the OpenGL layout.

### §2.1 `UniformsGpu` — 144 bytes, align 16

```rust
// crate::metal::gpu_abi
/// The exact Metal-ABI byte image of `crate::shaders::Uniforms` (the `metal/shaders.zig`
/// `extern struct` / the `Uniforms` struct the .metal shaders read). Explicit padding
/// fields reproduce Zig's `align(16)`/`align(4)` gaps; `padding_extend` is the packed
/// `EXTEND_*` byte; the four `bools` are four separate `u8`s (Metal, NOT a u32).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct UniformsGpu {
    projection_matrix: [[f32; 4]; 4], // 0
    screen_size: [f32; 2],            // 64
    cell_size: [f32; 2],              // 72
    grid_size: [u16; 2],              // 80
    _pad0: [u8; 12],                  // 84  → grid_padding@96 (Zig grid_padding align(16))
    grid_padding: [f32; 4],           // 96
    padding_extend: u8,               // 112 (left|right<<1|up<<2|down<<3)
    _pad1: [u8; 3],                   // 113 → min_contrast@116 (Zig min_contrast align(4))
    min_contrast: f32,                // 116
    cursor_pos: [u16; 2],             // 120
    cursor_color: [u8; 4],            // 124
    bg_color: [u8; 4],                // 128
    cursor_wide: u8,                  // 132
    use_display_p3: u8,               // 133
    use_linear_blending: u8,          // 134
    use_linear_correction: u8,        // 135
    // align(16) ⇒ size 144 (tail pad 136..144)
}

impl From<&crate::shaders::Uniforms> for UniformsGpu {
    fn from(u: &crate::shaders::Uniforms) -> Self {
        let pe = &u.padding_extend; // UniformsPaddingExtend { left,right,up,down }
        UniformsGpu {
            projection_matrix: u.projection_matrix,
            screen_size: u.screen_size,
            cell_size: u.cell_size,
            grid_size: u.grid_size,
            _pad0: [0; 12],
            grid_padding: u.grid_padding,
            // .metal: EXTEND_LEFT=1, EXTEND_RIGHT=2, EXTEND_UP=4, EXTEND_DOWN=8
            padding_extend: (pe.left as u8)
                | ((pe.right as u8) << 1)
                | ((pe.up as u8) << 2)
                | ((pe.down as u8) << 3),
            _pad1: [0; 3],
            min_contrast: u.min_contrast,
            cursor_pos: u.cursor_pos,
            cursor_color: u.cursor_color,
            bg_color: u.bg_color,
            cursor_wide: u.bools.cursor_wide as u8,
            use_display_p3: u.bools.use_display_p3 as u8,
            use_linear_blending: u.bools.use_linear_blending as u8,
            use_linear_correction: u.bools.use_linear_correction as u8,
        }
    }
}
```

Drift guard (fails the build if any offset drifts from the Metal ABI):
```rust
const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    assert!(size_of::<UniformsGpu>() == 144);
    assert!(align_of::<UniformsGpu>() == 16);
    assert!(offset_of!(UniformsGpu, projection_matrix) == 0);
    assert!(offset_of!(UniformsGpu, screen_size) == 64);
    assert!(offset_of!(UniformsGpu, cell_size) == 72);
    assert!(offset_of!(UniformsGpu, grid_size) == 80);
    assert!(offset_of!(UniformsGpu, grid_padding) == 96);
    assert!(offset_of!(UniformsGpu, padding_extend) == 112);
    assert!(offset_of!(UniformsGpu, min_contrast) == 116);
    assert!(offset_of!(UniformsGpu, cursor_pos) == 120);
    assert!(offset_of!(UniformsGpu, cursor_color) == 124);
    assert!(offset_of!(UniformsGpu, bg_color) == 128);
    assert!(offset_of!(UniformsGpu, cursor_wide) == 132);
    assert!(offset_of!(UniformsGpu, use_linear_correction) == 135);
};
```
*(Bit order LSB-first matches Zig `packed struct(u8)` field order AND the `.metal`
`EXTEND_*` enum; the four bools are four bytes matching MSL's four inline `bool`s.)*

### §2.2 `CellTextGpu` — 32 bytes

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CellTextGpu {
    glyph_pos: [u32; 2],  // 0
    glyph_size: [u32; 2], // 8
    bearings: [i16; 2],   // 16
    grid_pos: [u16; 2],   // 20
    color: [u8; 4],       // 24
    atlas: u8,            // 28  (Atlas enum value 0=grayscale, 1=color)
    bools: u8,            // 29  (no_min_contrast | is_cursor_glyph<<1)
    // align 4 ⇒ size 32 (tail pad 30..32)
}

impl From<&crate::shaders::CellText> for CellTextGpu {
    fn from(c: &crate::shaders::CellText) -> Self {
        CellTextGpu {
            glyph_pos: c.glyph_pos,
            glyph_size: c.glyph_size,
            bearings: c.bearings,
            grid_pos: c.grid_pos,
            color: c.color,
            atlas: c.atlas as u8, // repr(u8): Grayscale=0, Color=1
            // .metal: NO_MIN_CONTRAST=1u (bit0), IS_CURSOR_GLYPH=2u (bit1)
            bools: (c.bools.no_min_contrast as u8) | ((c.bools.is_cursor_glyph as u8) << 1),
        }
    }
}
```
Drift guard: `size_of == 32`, offsets `[0,8,16,20,24,28,29]`. **These are identical to the
existing `VertexLayoutRef` for `CellText`** (offsets/stride unchanged) — the only ABI-fix
is the *content* of byte 29 (`is_cursor_glyph` now visible in bit 1; previously it lived at
offset 30 in the ergonomic 2-bool struct and was never read → the cursor bug OPEN RISK #1
from Chunk-1b).

**Re-pin, precisely (the buffer element type is now `CellTextGpu`):**
* Move `impl VertexLayout for CellText` → `impl VertexLayout for CellTextGpu` (`stride =
  size_of::<CellTextGpu>()`) and the byte-layout drift guards (`offset_of!(CellText, …)` →
  `offset_of!(CellTextGpu, …)`) so they pin what is *literally in the buffer*.
  `crate::shaders::CellText` no longer needs `VertexLayout`.
* **Update the `PipelineDescription` table + imports (else the crate will not compile).** In
  `metal/shaders.rs` the pipeline rows reference `<CellText as VertexLayout>::LAYOUT` (the
  `cell_text` row) and `<BgImage as VertexLayout>::LAYOUT` (the `bg_image` row); re-point them
  to `<CellTextGpu …>` / `<BgImageGpu …>` and update the `use crate::shaders::{BgImage,
  CellText, Image}` import to bring `CellTextGpu`/`BgImageGpu` in from `crate::metal::gpu_abi`
  (`Image` STAYS `crate::shaders::Image` — it is already ABI, no `ImageGpu`). Offsets are
  byte-identical (verified: CellText 32B `[0,8,16,20,24,28,29]`, BgImage 8B `[0,4]`), so the
  GPU vertex-fetch behavior is UNCHANGED.
* **Do NOT silently drop the reference `test{}` (R1).** The current `metal/shaders.rs` const
  guard carries `assert!(size_of::<CellText>() == 32) // un-defers the reference test{}` —
  that is the port of `metal/shaders.zig`'s lone `test { expectEqual(32, @sizeOf(CellText)) }`,
  standing on the ergonomic `crate::shaders::CellText`. When the *layout/offset* guards move to
  `CellTextGpu`, KEEP a standalone `assert!(size_of::<crate::shaders::CellText>() == 32)` on the
  ergonomic type so the reference test{} stays un-deferred. G3b does not gate this crate — this
  const guard is its ONLY enforcement; re-pinning must not lose it.

### §2.3 `BgImageGpu` — 8 bytes (defined for buffer-type consistency; NOT on golden path)

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BgImageGpu {
    opacity: f32, // 0
    info: u8,     // 4  (position | fit<<4 | repeat<<6)
    // align 4 ⇒ size 8
}
impl From<&crate::shaders::BgImage> for BgImageGpu {
    fn from(b: &crate::shaders::BgImage) -> Self {
        let i = &b.info; // BgImageInfo { position: enum(u4), fit: enum(u2), repeat }
        BgImageGpu {
            opacity: b.opacity,
            // .metal: BG_IMAGE_POSITION=15u (bits0-3), BG_IMAGE_FIT=3u<<4 (bits4-5), BG_IMAGE_REPEAT=1u<<6
            info: ((i.position as u8) & 0x0F)
                | (((i.fit as u8) & 0x03) << 4)
                | ((i.repeat as u8) << 6),
        }
    }
}
```
`BgImage`'s `bg_image_buffer` sync in `drawFrame` is guarded by
`bg_image_buffer_modified` (stays 0 with no bg image) — **never hit by the golden**. Defined
+ typed so `FrameState.bg_image_buffer: Buffer<BgImageGpu>` stays a real ABI buffer (no
lingering byte-unfaithful `Buffer<BgImage>`), but it is not golden-load-bearing. Its
`VertexLayout`/drift-guards re-pin to `BgImageGpu` alongside `CellTextGpu` (§2.2 — byte-identical
`[0,4]` offsets, `stride 8`), so the `bg_image` pipeline row + `metal/shaders.rs` still compile.

### §2.4 `CellBg` — no change

`CellBg = [u8;4]` is already the exact `uchar4` ABI. `frame.cells_bg` stays `Buffer<CellBg>`;
`drawFrame` syncs `&self.cells.bg_cells` directly (no serialization).

### §2.5 Byte unit test (in `gpu_abi`, runs everywhere — NOT Metal-gated)

A `#[test]` that builds an ergonomic `Uniforms`/`CellText` with known named-bool values,
converts, and asserts the raw bytes at the exact ABI offsets:
```rust
let c = CellText { bools: CellTextBools{ no_min_contrast:false, is_cursor_glyph:true }, .. };
let g = CellTextGpu::from(&c);
let raw: [u8;32] = unsafe { core::mem::transmute(g) };
assert_eq!(raw[29], 0b10);            // IS_CURSOR_GLYPH bit
// padding_extend {up:true} -> raw[112] == 0b0100 ; bools {use_display_p3:true} -> raw[133]==1
```
This makes the bit-packing checkable off a Metal host (the adversary's "compute the expected
byte" check), independent of the GPU golden.

### §2.6 `FrameState` buffer-type changes (3a code, owned by the draw path)

```rust
pub uniforms: Buffer<UniformsGpu>,   // was Buffer<Uniforms>
pub cells:    Buffer<CellTextGpu>,   // was Buffer<CellText>
pub cells_bg: Buffer<CellBg>,        // UNCHANGED
pub bg_image_buffer: Buffer<BgImageGpu>, // was Buffer<BgImage>
```
`FrameState::init` updates the three `Buffer::<T>::init(...)` type args accordingly.

---

## §3 THE GOLDEN GATE — deterministic render + assert

### §3.1 Assertion strategy: **STRUCTURAL pixel asserts** (recommended), not exact-PNG

**Reject exact-golden-PNG:** the output depends on the FreeType glyph raster (font
version), the Metal shader compiler (metallib vs runtime-`newLibraryWithSource` can differ
sub-LSB — see the `[F7]` note in `metal/shaders.rs`), and GPU rounding. An exact-image gate
would be flaky across hosts/toolchains and would fail for reasons unrelated to the port.

**Structural asserts** verify the *structure the port is responsible for* — the cursor is
the config red, the background is the config bg, a foreground glyph is present in row 0 —
with thresholds lenient enough to survive the sRGB→Display-P3 conversion + premultiply +
`BGRA8Unorm` storage rounding, yet non-vacuous (each assert can only pass if that GPU stage
actually ran). This is the robust choice and matches the brief's recommendation.

### §3.2 Determinism pins

* **`AlphaBlending::Native`** (the 3b/3c fixture) ⇒ `blending.is_linear() == false` ⇒ the
  target texture is **`BGRA8Unorm`** (NOT `_sRGB`). The readback bytes are then the shader's
  gamma-encoded output *directly* (no implicit sRGB re-encode on store), so the asserts read
  the colors the shader computed. `colorspace = Srgb` ⇒ `use_display_p3 = false` (the golden
  reads raw BGRA, which ignores the IOSurface colorspace tag — see the Target `set_color_space`
  deferred seam).
* **`offscreen_size == size.screen`** (the grid's pixel size `cols*cell_w × rows*cell_h`).
  Then `drawFrame`'s `surface_size == self.size.screen` ⇒ `size_changed == false` ⇒ no
  in-`drawFrame` resize/`update_screen_size_uniforms` surprise, projection is 1:1
  (terminal size == screen size == surface size), and cell (x,y) lands at pixel
  `[x*cell_w .. (x+1)*cell_w) × [y*cell_h .. (y+1)*cell_h)` — trivial pixel math.
  *(The 3b `test_renderer` hardcodes `init_offscreen(…, 256, 256)`; the golden harness
  instead builds the grid/metrics FIRST, computes `size.screen`, then calls
  `init_offscreen(blending, size.screen.width, size.screen.height)`.)*
* `focused=true`, `blink_visible=true` ⇒ a **block** cursor. After `b"Ab"` on a 10×3 grid
  the cursor sits at grid **(2,0)** (the empty cell after "Ab") — the block sprite fills that
  cell red (grayscale block mask, full alpha, × `cursor_color`). Verified by the 3b test
  (`cur.grid_pos == [2,0]`, `cur.color[0..3] == [0xFF,0,0]`, `uniforms.cursor_pos == [2,0]`).

### §3.3 Readback + which target

`drawFrame` gets its frame via `swap_chain.next_frame()` (which increments `frame_index`),
resizes that `FrameState.target` from 1×1 to `size.screen`, renders into it, and (offscreen)
`present` is a no-op — the pixels stay in the IOSurface. After `draw_frame(true)` returns:
```rust
let idx = r.swap_chain.frame_index as usize;
let px: Vec<u8> = r.swap_chain.frames[idx].target.read_bgra(); // w*h*4, [B,G,R,A] per px, tight rows
```
(all fields are `pub`; `read_bgra` already handles `bytesPerRow`).

### §3.4 The exact structural asserts

Let `w = size.screen.width`, `cw = cell_w`, `ch = cell_h`. Helper
`px_at(px,w,x,y) -> (b,g,r,a)` at byte `(y*w + x)*4`.

```rust
// (A) CURSOR CELL — solid red block at grid (2,0). Sample the cell interior center.
let (b,g,rr,_a) = px_at(&px, w, 2*cw + cw/2, ch/2);
assert!(rr > 128, "cursor cell not bright-red (R={rr})");
assert!(rr > g && rr > b, "cursor cell not red-dominant (R={rr} G={g} B={b})");

// (B) BACKGROUND — config bg is black (RGB 0,0,0), opacity 1.0 ⇒ bg_color = [0,0,0,255].
//     Sample an empty cell far from glyphs/cursor (grid (8,2), bottom row): cell_bg is
//     transparent there (alpha 0) ⇒ the opaque bg_color full-screen layer shows through.
//     The RGB check ALONE is vacuous here (config bg RGB == clear_color RGB == 0,0,0; and
//     bg_color_fragment premultiplies rgb by alpha, so it ALSO emits RGB 0 — see below), so
//     we ALSO assert the ALPHA: the bg_color step writes an opaque layer (a=255) whereas the
//     render-pass clear leaves a=0. read_bgra's 4th byte per pixel is the alpha channel.
let (bb,bg2,br,ba) = px_at(&px, w, 8*cw + cw/2, 2*ch + ch/2);
assert!(br < 96 && bg2 < 96 && bb < 96, "background not dark (R={br} G={bg2} B={bb})");
assert!(ba > 128, "bg_color step absent / bg alpha not opaque (A={ba})"); // non-vacuity vs the clear

// (C) FOREGROUND GLYPH present in row 0 — "A" at grid (0,0) is white. Scan the cell for a
//     bright mark (antialiased glyph ⇒ take the max luminance over the cell region).
let mut maxlum = 0u32;
for yy in 0..ch { for xx in 0..cw {
    let (b3,g3,r3,_) = px_at(&px, w, xx, yy);
    maxlum = maxlum.max((r3 as u32 + g3 as u32 + b3 as u32) / 3);
}}
assert!(maxlum > 128, "no fg glyph rendered in cell (0,0) (maxlum={maxlum})");
```
Non-vacuity: (A) can only pass if `cell_text` ran with `is_cursor_glyph` set + `cursor_color`
red reached the shader (proves the packed `bools` byte AND the uniform serialization). **(B)'s
RGB check is VACUOUS with this fixture and carries its weight through the ALPHA assert** (this
corrects the earlier claim that (B) "proves the bg_color full-screen step + the serialized
bg_color" via RGB): the config bg is RGB `(0,0,0)`, identical to the render-pass
`clear_color = {0,0,0,0}`, and `bg_color_fragment` returns `load_color(bg_color,…)` which
premultiplies `rgb *= a`, so it too emits RGB `(0,0,0)` — a dark pixel is produced *whether or
not the bg step ran or `bg_color` serialized correctly*. The alpha does distinguish them:
`bg_color_fragment` returns `a = bg_color.a/255 = 1.0` (opaque; `background_opacity = 1.0`)
while the clear leaves `a = 0`, so `ba > 128` can only pass if the full-screen `bg_color` step
actually ran AND `bg_color.a = 255` serialized into `UniformsGpu` (byte 131 = `bg_color` offset
128 + 3). *(Alternative for a stronger RGB proof: render the golden with a distinct **non-black**
background so RGB alone separates the bg step from the clear — but that mutates the 3b/3c
fixture; the alpha assert keeps the fixture intact, which is why it is preferred.)* (C) proves
the atlas sync + `cell_text` glyph sampling. Thresholds tolerate the P3 conversion / GPU rounding.

---

## §4 Renderer::init vs golden-constructs-directly — **construct directly**

**Decision: the golden constructs the `Renderer` field-by-field**, reusing/extending the 3b
`test_renderer` recipe. `Renderer::init` stays the `unimplemented!()` seam it already is.

**Justification.** `Renderer::init` (`:658`) does `GraphicsAPI.init(alloc, options)` =
`Metal::init(alloc, opts)`, which needs `opts.rt_surface.platform.nsview` (a **live
NSView**) + an `IOSurfaceLayer` attach — the windowing-host seam M-2 that `metal_impl.rs`
explicitly defers (`Metal::init` is `unimplemented!("windowing-host …")`; the offscreen entry
is `init_offscreen`). It also reads `options.config.custom_shaders.value.items.len` and
`options.surface_mailbox`, both of which are **deferred placeholders** (`RepeatablePath` ZST,
`SurfaceMailbox` ZST) — landing `init` would require un-deferring the config-parse +
apprt-mailbox seams. And acyclicity: a real `init` pulls `renderer::Options` which references
`apprt`/config surfaces (`renderer → config` is **forbidden by R4**). The 3b/3c `test_renderer`
already proves the field-by-field construction is sufficient (offscreen `Metal` +
`init_shaders(&[])` + `SwapChain::init` + embedded `SharedGrid`/`Shaper` +
`derived_config_for_test()`); the golden harness is `test_renderer` + a size tweak (§3.2) +
`draw_frame`. This keeps Chunk 4 inside the offscreen scope and R4-acyclic.

---

## §5 THE COMPLETION TRAMPOLINE — raw `*mut` discipline + a mirihar candidate

### §5.1 The trampoline body (authored in `macos_impl`; `frame.rs` already carries the shape)

```rust
// = Frame.zig `bufferCompleted` (the @fieldParentPtr / container_of re-entry).
unsafe fn metal_buffer_completed(
    ctx: &CompletionContext,
    buffer: &ProtocolObject<dyn MTLCommandBuffer>,
) {
    let status = buffer.status();
    let health = match status {
        MTLCommandBufferStatus::Error => crate::Health::Unhealthy,
        _ => crate::Health::Healthy,
    };
    // container_of: the type-erased renderer pointer back to *mut Renderer.
    let renderer = ctx.renderer as *mut Renderer;
    if health == crate::Health::Healthy {
        // offscreen present is a no-op stub; &mut api reborrow through the raw ptr.
        // R1 — the reference logs a present failure (Frame.zig :82-88):
        //   `block.renderer.api.present(block.target.*, block.sync) catch |err|
        //        log.err("Failed to present render target: err={}", .{err});`
        // present is a no-op offscreen (always Ok ⇒ no err to surface), but the error-log
        // site is preserved verbatim as this comment for fidelity; when the real layer-path
        // `present` un-defers, restore `if let Err(err) = … { log::error!(…) }`.
        let _ = (*renderer).api.present(&*ctx.target, ctx.sync);
    }
    (*renderer).frame_completed(health);
}
```
`drawFrame` installs it: `begin_frame(self_ptr as *mut (), addr_of_mut!((*frame).target), metal_buffer_completed)`.
`complete(true)` (golden path) commits → `waitUntilCompleted` → **invokes it directly**.

`frameCompleted`:
```rust
// Reference `pub fn frameCompleted(self: *Self, health)`. Ported as `&self`: every op it
// performs is &self-safe — `health.store` (AtomicHealth, &self), the surface_mailbox push
// (SurfaceMailbox is a no-op placeholder — deferred seam), and `swap_chain.release_frame`
// (&self, posts the semaphore). Narrowing *Self → &self is faithful (no mutation of a
// non-atomic field) AND reduces the trampoline reborrow to a SHARED reborrow (SB-cleaner).
pub fn frame_completed(&self, health: crate::Health) {
    if self.health.load() != health {
        self.health.store(health);
        // `self.surface_mailbox.push(.{ .renderer_health = health }, .{ .forever = {} });`
        //   — SurfaceMailbox is the deferred no-op placeholder (§ generic.rs seam); the real
        //     mailbox routing lands with the apprt tier. Ported as a no-op push (returns 0).
    }
    self.swap_chain.release_frame();
}
```
The `SurfaceMailbox` placeholder gains a no-op `push(_msg) -> usize { 0 }` so the two
reference push sites (`drawFrame`'s `scrollbar_dirty` defer at `:1454`, `frameCompleted`'s
`renderer_health` at `:1735`) port verbatim as harmless no-ops (returning 0 ⇒ the
`if (push(...) > 0)` branch is never taken offscreen — the golden observes no mailbox).

### §5.2 Raw-ptr discipline (NOT `&mut`) + **a mirihar candidate IS needed — FLAG**

Two raw-pointer-aliasing-self hazards arise in `drawFrame` that do **not** appear in the 3b
`add_cursor` pattern (which 3b already proved on `mirihar-chunk3b`):

* **(H1) `frame: *mut FrameState`** (a raw ptr into `self.swap_chain.frames[i]`, from
  `next_frame`) is re-deref'd **across `&mut self` method calls** (`self.init_shaders()`,
  `self.update_custom_shader_uniforms_for_frame()`, `self.images.upload(...)`,
  `self.sync_atlas_texture(...)`). Under Stacked Borrows, taking `&mut self` for those methods
  can pop the raw `frame` provenance; the later `(*frame).…` deref would then be UB.
* **(H2) the completion trampoline** derefs `self_ptr: *mut Renderer` (== `self`) to call
  `&mut (*self_ptr).api.present(...)` (and `&self frame_completed`) **while `drawFrame`'s
  `&mut self` is still live on the stack** (`complete()` runs inside `drawFrame`). A
  raw-alias-crossed-by-`&mut` — exactly the class the memory says to PROVE, not hand-reason.

**Prescription (do NOT hand-reason SB):** extend `.port/phase-2/mirihar-chunk3b/` with a
new argv-selected candidate `drawframe-completion` that models a `Renderer{ swap_chain:
Vec<Frame>, api: Api, other: X }`, and:
1. `let r: *mut Renderer = &mut renderer;`
2. `let frame: *mut Frame = &mut (*r).swap_chain[i];`
3. call a `&mut self` method on `r` (touching `other`/`api`) — then re-deref `(*frame).field`
   and `(*r).field` (models H1);
4. a nested "trampoline" `fn(ctx: *mut ())` that re-derives `&mut (*r).api` and `&(*r)` and
   is invoked while the outer raw ptrs are live (models H2).
5. **the mid-`drawFrame` SHARED `&self` reborrows (the render-pass mix — REQUIRED, per the
   adversary):** between creating the raw roots and firing the trampoline, take **shared**
   `&self` reborrows off the receiver — `let _ = &(*r).shaders; (*r).images.draw(&(*r).api, …)`
   — modeling the render-pass steps that read `self.shaders.pipelines.*`, `self.images`, and
   `self.api` (`begin_frame` is `&self`, verified `metal_impl.rs:620`) **interleaved with the
   live `self_ptr` (`r`) AND `frame` raw roots** (which are themselves re-read as
   `(*frame).uniforms`/`(*frame).target`). The candidate MUST reproduce this **mix** — shared
   reborrows + `&mut` reborrows + two raw roots — NOT just `&mut`-only interleaving. The real
   Stacked-Borrows profile is a shared-vs-raw mix, and the "single raw root, short reborrows"
   idiom must be validated against THAT profile before it is recorded in `MIRI_EXCEPTIONS.md`.

**Pass criterion:** the chosen structure must be clean under **both**
`-Zmiri-stacked-borrows` and `-Zmiri-tree-borrows` (the 3b bar). **Hypothesis to verify (not
assert):** drive *all* of `drawFrame`'s self-access through the single raw root `self_ptr`
with **short-lived `unsafe { &mut *self_ptr }` / `&mut *frame` reborrows** that never outlive
the frame/trampoline raw ptrs (the "single raw root, short reborrows" idiom), and create
`self_ptr`/`frame` **once at the top**. The mirihar run decides whether that is clean or
whether a variant (e.g. `addr_of_mut!` field roots, or reborrow-per-statement) is required.
If clean, record it in `MIRI_EXCEPTIONS.md` beside the 3b entry.

---

## §6 `drawFrame` port map + dead-code reachability + `init_shaders`

### §6.1 `draw_frame` signature + body order (faithful to `:1430`–`:1721`)

```rust
pub fn draw_frame(&mut self, sync: bool) -> Result<(), Error> { … }
```
Body, in reference order (raw-ptr discipline per §5.2):
1. `draw_mutex` lock (RAII guard = Zig `lock()`+`defer unlock()`).
2. `defer` scrollbar mailbox push (no-op placeholder, §5.1).
3. `self.api.draw_frame_start()` + `defer draw_frame_end()`.
4. `surface_size = self.api.surface_size()?`; early-return if `w==0 || h==0`.
5. `size_changed`, `needs_redraw = size_changed || self.cells_rebuilt || self.has_animations() || sync`.
   `if !needs_redraw { self.api.present_last_target()?; return Ok(()) }`. (Golden: `sync=true`
   ⇒ always redraws.) Then `self.cells_rebuilt = false`.
6. `frame = self.swap_chain.next_frame()?` → `*mut FrameState` (H1).
7. reinit-shaders branch: `if self.reinitialize_shaders { … self.shaders.deinit(); self.init_shaders()?; }`
   (golden: `false`, skipped — but references `init_shaders`/`deinit`, keeping them live).
8. `assert!(!self.shaders.defunct)`.
9. custom-shader-state (golden: `has_custom_shaders=false` ⇒ ensure `frame.custom_shader_state`
   is `None`).
10. `if size_changed { self.size.screen = …; self.update_screen_size_uniforms(); }` (golden: skipped).
11. target-resize `if frame.target.{width,height} != size.screen || …modified { (*frame).resize(&self.api,…)?; }`
    (golden: FIRST draw resizes 1×1 → `size.screen`).
12. `self.images.upload(self.alloc, &self.api)` (no-op, empty); `self.upload_background_image()?`
    (no-op, `bg_image=None`); `self.update_custom_shader_uniforms_for_frame()`.
13. **sync** (the serialization boundary, §2):
    * `(*frame).uniforms.sync(&[UniformsGpu::from(&self.uniforms)])?;`
    * `(*frame).cells_bg.sync(&self.cells.bg_cells)?;`
    * fg cells — see §6.2.
    * bg_image_buffer sync — guarded by `modified` (skipped, golden).
14. **atlas sync** (`texture:` blocks): under the font-grid shared lock, if
    `atlas.modified() > frame.grayscale_modified` then
    `self.sync_atlas_texture(atlas_grayscale, &mut (*frame).grayscale)?` (+ the color atlas).
    Load-bearing: without it the `cell_text` shader samples an empty texture → no glyphs.
15. `frame_ctx = self.api.begin_frame(self_ptr as *mut (), addr_of_mut!((*frame).target), metal_buffer_completed)?;`
    `defer frame_ctx.complete(sync);`
16. RenderPass (golden path — no bg image, no custom shaders):
    `bg_color` step → `images.draw(kitty_below_bg)` → `cell_bg` step →
    `images.draw(kitty_below_text)` → `cell_text` step (`instance_count = fg_count`,
    textures `[grayscale, color]`, buffers `[cells, cells_bg]`) → `images.draw(kitty_above_text)`
    → overlay draw (`overlay = None`, skipped). Each `images.draw` is a no-op (empty placements).
17. custom-shaders post-pass (golden: `frame.custom_shader_state == None`, skipped).

### §6.2 fg-cell serialization (the accepted per-frame conversion)

The reference is `const fg_count = try frame.cells.syncFromArrayLists(self.cells.fg_rows.lists);`
— a zero-copy memcpy of the `extern struct`s. Rust's `CellText` in-memory layout is not the
packed-`u8` ABI (byte 29), so we serialize:
```rust
// Fuses the reference `syncFromArrayLists` (row concatenation, order-preserving) with the
// CellText -> CellTextGpu ABI serialization. One transient Vec (NOT a GPU-buffer realloc —
// `sync` still reuses frame.cells).
let flat: Vec<CellTextGpu> = self
    .cells
    .fg_rows
    .lists
    .iter()
    .flat_map(|row| row.iter().map(CellTextGpu::from))
    .collect();
frame.cells.sync(&flat)?;               // via unsafe { &mut *frame }
let fg_count = flat.len();
```
> **Accepted deviation (OPEN RISK, §8):** this one transient `Vec` per frame deviates from
> the reference's "keep this around so we don't reallocate" intent. It is forced by the
> layout mismatch and is fine for the offscreen golden (correctness gate); the
> production/async high-frequency draw path is a deferred windowing-host concern. Documented
> with a PORT NOTE. (The rejected repr(C)-pack-in-place alternative, §2.0, would remove it.)

### §6.3 `syncAtlasTexture` (`:3358`)

```rust
fn sync_atlas_texture(&self, atlas: &rustty_font::atlas::Atlas, texture: &mut Texture)
    -> Result<(), Error>
{
    if atlas.size() as usize > texture.width {   // reference `atlas.size > texture.width`
        *texture = self.api.init_atlas_texture(atlas)?;  // drops old (Zig deinit + reinit)
    }
    texture.replace_region(0, 0, atlas.size() as usize, atlas.size() as usize, atlas.data());
    Ok(())
}
```
`Texture::{replace_region, width}` and `Atlas::{size, data}` are already `pub`; `Atlas::modified`
and `SharedGrid::atlas_grayscale/atlas_color` are the §7 additions.

### §6.4 `init_shaders` / `deinit_shaders` / `has_animations`

```rust
// PORT NOTE (dead-code / `-D warnings`, following GPU_RD_CHUNK3A_DESIGN §B.4's "make pub,
// ZERO #[allow(dead_code)]" precedent for deferred-seam internals): `deinit_shaders`'s SOLE
// reference is `Renderer::deinit` (ref generic.zig :825), which stays the `unimplemented!()`
// seam (§1). `drawFrame`'s reinit branch calls `self.shaders.deinit()` DIRECTLY (ref :1500),
// NOT `self.deinitShaders()` — so `deinit_shaders` has no live caller in Chunk 4, and a
// private `fn` would trip `-D warnings`. Widen it to `pub fn` (Zig-private → Rust-pub,
// documented per the 3a widening of the deferred SwapChain/FrameState internals) so it stays
// landed-but-uncalled until `Renderer::deinit` un-defers.
pub fn deinit_shaders(&mut self) { self.shaders.deinit(/* alloc dissolved */); }
fn has_animations(&self) -> bool { self.has_custom_shaders }

fn init_shaders(&mut self) -> Result<(), Error> {
    // Reference loads custom shaders via `shadertoy.loadFromFiles(arena, self.config.custom_shaders,
    // GraphicsAPI.custom_shader_target)` then `self.api.initShaders(alloc, custom_shaders)`.
    // `shadertoy::load_from_files` is un-ported AND `config.custom_shaders` is the
    // `RepeatablePath` placeholder ZST — so the custom-shader load is a DEFERRED seam that
    // yields the empty list (faithful to the reference's `catch |err| break :err &.{}` and to
    // the placeholder). The verbatim Zig loader body is preserved as a comment (R5).
    let custom_shaders: &[&CStr] = &[]; // DEFERRED: shadertoy::load_from_files (see PORT NOTE)
    self.shaders = self.api.init_shaders(custom_shaders)?;
    self.has_custom_shaders = !custom_shaders.is_empty();
    Ok(())
}
```
`init_shaders` is thus **live + runnable** (no panic seam), just with custom-shader loading
deferred-to-empty — consistent with the `RepeatablePath` placeholder. The golden never enters
the reinit branch, but `init_shaders` is compile-referenced by that branch (step 7) so it stays
non-dead. **`deinit_shaders`, by contrast, is referenced ONLY by the deferred `Renderer::deinit`**
(`drawFrame`'s reinit path deinits `self.shaders` directly, not via `deinit_shaders`), so it is
`pub fn` (above) to survive `-D warnings` — it is NOT kept live by any `drawFrame` reference.
`has_animations` is live via `draw_frame`'s `needs_redraw`.

### §6.5 The golden test (macOS + `#[cfg(test)]`, Metal-skip)

Add to the existing `#[cfg(target_os="macos")] mod macos_impl { … #[cfg(test)] mod tests }`,
beside `rebuild_cells_fills_bg_fg_cursor`. Reuse `derived_config_for_test()` and the
`test_renderer` font-grid recipe, but build metrics/size first and pass `size.screen` to
`init_offscreen` (§3.2). Then:
```rust
#[test]
fn golden_renders_cursor_fg_bg() {
    let mut r = match golden_renderer(10, 3) { Some(r) => r, None => return }; // Metal-skip
    let mut t = Terminal::init(Global, opts(10,3)).unwrap();
    { let mut s = t.vt_stream(); s.next_slice(b"Ab"); s.deinit(); }
    // drive the full pipeline through update_frame (3c) — fills cells + uniforms + atlas.
    let st = state::State { mutex: &mut Mutex::new(), terminal: &mut t, inspector: None,
                            preedit: None, mouse: state::Mouse::default() };
    r.update_frame(&st, true).unwrap();
    r.draw_frame(true).unwrap();                       // sync ⇒ waitUntilCompleted + trampoline
    let idx = r.swap_chain.frame_index as usize;
    let px = r.swap_chain.frames[idx].target.read_bgra();
    // ... the §3.4 structural asserts (cursor red / bg dark / fg glyph present) ...
    t.deinit(Global);
}
```
`golden_renderer` = `test_renderer` with the `init_offscreen(blending, screen_w, screen_h)`
size tweak. Metal-skip: `init_offscreen` returns `Err(NoMetalDevice)` on headless CI ⇒
`None` ⇒ `return` (clean skip, the `metal.rs` precedent).

---

## §7 Cross-crate accessor additions (small, faithful widenings)

`syncAtlasTexture`'s caller (`drawFrame`) needs `&self.font_grid.atlas_grayscale` +
`atlas.modified`, but rustty narrowed those to private (the reference Zig fields are public).
Add:
* `rustty-font-shaper` `SharedGrid`: `pub fn atlas_grayscale(&self) -> &Atlas<A>` and
  `pub fn atlas_color(&self) -> &Atlas<A>` (the `atlas_grayscale`/`atlas_color` fields are
  currently private). Mirrors the existing narrowing PORT NOTE in `metal_impl.rs`
  `init_atlas_texture` ("the Zig public fields are narrowed … the renderer reads them through
  the accessors").
* `rustty-font` `Atlas`: `pub fn modified(&self) -> usize { self.modified.load(Ordering::Relaxed) }`
  (the `.monotonic` load the reference does; the field + relaxed-load are already there).

Both are on crates `rustty-renderer` **already depends on** ⇒ `check_acyclic.sh` stays green.
Accessed via the `*mut SharedGrid` raw handle: `unsafe { &*self.font_grid }.atlas_grayscale()`,
under `unsafe { &*self.font_grid }.lock.read()` (the reference `lock.lockShared()`).

---

## §8 OPEN RISKS

1. **[HIGHEST] Packed-bool bit fidelity** — the whole point. Mitigated by: (a) the §2.5 byte
   unit-test (runs off a Metal host; asserts `raw[29]`, `raw[112]`, `raw[133]`, …), (b) the
   `const{}` offset drift guards, (c) the golden's cursor-red assert (a wrong `is_cursor_glyph`
   bit ⇒ the shader's `(in.bools & IS_CURSOR_GLYPH)` recolor logic misfires ⇒ visible pixel
   failure). The adversary must independently recompute each byte vs `.metal` + the Zig
   `packed struct(u8)`.
2. **[HIGH] Metal-vs-OpenGL ABI confusion** — the `shaders.rs` PORT NOTEs say
   `packed struct(u32)` (OpenGL). Chunk 4 targets the **Metal** ABI (`padding_extend` 1 byte,
   `bools` 4 bytes). If the porter follows the u32 notes, `UniformsGpu` is wrong. Mitigated by
   the §2.0 FLAG + a mandatory clarifying PORT NOTE in `gpu_abi`.
3. **[HIGH] The completion-trampoline + frame raw-ptr aliasing (H1/H2)** — requires a **new
   `mirihar-chunk3b` candidate** (§5.2) that models H1 + H2 **AND** the mid-`drawFrame` shared
   `&self` render-pass reborrows interleaved with the raw roots (the shared-vs-raw mix, not
   just `&mut` interleaving), proven clean under SB **and** TB. Do NOT hand-reason. A wrong
   structure is silent UB that the golden may not catch.
4. **[MED] Per-frame `Vec<CellTextGpu>` allocation** (§6.2) — accepted deviation from the
   reference's no-realloc intent; documented. The repr(C)-pack-in-place alternative (§2.0)
   removes it at the cost of 3b/3c churn — offered to the adversary as the tradeoff.
5. **[MED] Golden determinism** — pinned to `AlphaBlending::Native` (`BGRA8Unorm`, no store
   re-encode) + `colorspace=Srgb` + `offscreen_size==size.screen`. If a future change flips
   blending to linear, the target becomes `_sRGB` and the readback semantics shift (the
   asserts would need linear-space thresholds). The pin is load-bearing — note it in the test.
6. **[MED] `init_shaders` custom-shader loader deferred-to-empty** — because
   `shadertoy::load_from_files` is un-ported and `RepeatablePath` is a placeholder. Faithful
   (matches the reference `catch → &.{}` + the placeholders) but the porter must confirm
   `load_from_files` really is absent (grep) and preserve the verbatim Zig loader as a comment.
7. **[LOW] `SurfaceMailbox` no-op `push`** — returns 0, so `drawFrame`'s
   `if (push(...) > 0) scrollbar_dirty = false` never fires offscreen (harmless; the golden
   does not observe the mailbox). The real routing is the deferred apprt seam.
8. **[LOW] R1 comment coverage** — G3b does NOT gate this crate. Every ported line/comment of
   `drawFrame`/`frameCompleted`/`syncAtlasTexture`/`initShaders`/`deinitShaders` + the
   `shaders.rs` clarifying note must be preserved by hand; the adversary/verify are the only
   backstop.

---

## Appendix — signature summary (new/changed items)

```rust
// crate::metal::gpu_abi  (NEW module)
#[repr(C, align(16))] pub struct UniformsGpu { … }   // 144B
#[repr(C)]            pub struct CellTextGpu { … }    // 32B
#[repr(C)]            pub struct BgImageGpu  { … }    // 8B
impl From<&crate::shaders::Uniforms> for UniformsGpu { … }
impl From<&crate::shaders::CellText> for CellTextGpu { … }
impl From<&crate::shaders::BgImage>  for BgImageGpu  { … }
// + const{} drift guards + #[test] byte assertions

// crate::generic (macos_impl) — Renderer methods (NEW)
pub fn draw_frame(&mut self, sync: bool) -> Result<(), Error>;
pub fn frame_completed(&self, health: crate::Health);            // &self (faithful narrowing)
fn    sync_atlas_texture(&self, atlas: &rustty_font::atlas::Atlas, texture: &mut Texture) -> Result<(), Error>;
fn    init_shaders(&mut self) -> Result<(), Error>;              // loader deferred → empty
pub fn deinit_shaders(&mut self);                                // pub: sole caller Renderer::deinit is deferred (§6.4)
fn    has_animations(&self) -> bool;
fn    upload_background_image(&mut self) -> Result<(), Error>;    // thin, guarded
unsafe fn metal_buffer_completed(ctx: &CompletionContext, buffer: &ProtocolObject<dyn MTLCommandBuffer>);
impl SurfaceMailbox { pub fn push(&self, _msg: /*…*/) -> usize { 0 } }  // no-op placeholder

// FrameState (CHANGED buffer element types)
pub uniforms: Buffer<UniformsGpu>;
pub cells:    Buffer<CellTextGpu>;
pub bg_image_buffer: Buffer<BgImageGpu>;
// pub cells_bg: Buffer<CellBg>  (UNCHANGED — already ABI)

// rustty-font-shaper::SharedGrid (NEW accessors)
pub fn atlas_grayscale(&self) -> &Atlas<A>;
pub fn atlas_color(&self) -> &Atlas<A>;
// rustty-font::Atlas (NEW accessor)
pub fn modified(&self) -> usize;
```
