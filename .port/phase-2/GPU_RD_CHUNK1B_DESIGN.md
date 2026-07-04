# GPU R-d — CHUNK 1b design: the Metal draw-plumbing leaves

**Architect deliverable.** Concrete, implementable design for the five remaining
`metal/*` draw-plumbing leaves — `Pipeline`, `RenderPass`, `Target`, `Frame`,
`shaders` (backend container) — plus the three genuinely-uncertain FFI spots
(IOSurface offscreen, the `block2` completion trampoline, the vertex-attribute
tables). Builds directly on the **landed Chunk-1a leaves**
(`metal/{api,buffer,texture,sampler}.rs`) and the **already-ported vertex structs**
(`crate::shaders::{CellText, CellBg, Image, BgImage, Uniforms}`).

Parents honored: `GPU_RD_TRAIT_DESIGN.md` (MONOMORPHIC port — §F; §A.3
Step/Attachment/Draw; §C Options; §G PipelineCollection/autoAttribute/Pipeline.init/
metallib) and `GPU_RD_BLUEPRINT.md` ([ADV-FIX F6/F7/F8/F12], §3 shader load).
Port rules: `PORT_RULES.md` (R1 fidelity; memory `rust-mut-args-strongly-protected`
= raw `*mut`, never `&mut`, for the re-entrant completion pointer).

**Ground truth (verified against the crate sources, do not re-verify):** every
objc2 symbol named in this doc was read out of
`~/.cargo/registry/.../{objc2-io-surface-0.3.2, objc2-core-foundation-0.3.2,
block2-0.6.2, dispatch2-0.3.1, objc2-metal-0.3.2}/src`. Exact selectors, signatures,
and constant names are quoted inline.

Sources being ported (read in full): `ghostty/src/renderer/metal/{Pipeline,
RenderPass,Target,Frame,shaders}.zig`.

---

## §0 — Executive summary (read first)

**HARD SPOT 1 — Target IOSurface offscreen ([F12]): RESOLVED.**
`macos.iosurface.IOSurface.init` maps to `objc2_io_surface::IOSurfaceRef::new(&CFDictionary)
-> Option<CFRetained<IOSurfaceRef>>` (the non-deprecated name for `IOSurfaceCreate`). The
4-key property dictionary is built with `objc2_core_foundation::{CFDictionary::new,
CFNumber::new, CFNumberType::IntType}` over the four `&'static CFString` externs
`kIOSurfaceWidth`/`kIOSurfaceHeight`/`kIOSurfacePixelFormat`/`kIOSurfaceBytesPerElement`.
Pixel format `'BGRA'` = `0x42475241` (kCVPixelFormatType_32BGRA), passed as an `i32`
CFNumber. The MTLTexture is created with
`device.newTextureWithDescriptor_iosurface_plane(&desc, &surface, 0)`. **Per [F12] the
DisplayP3 colorspace-set is SKIPPED offscreen** (raw-byte readback ignores IOSurface
colorspace metadata; it also matches the golden's `colorspace != display-p3` determinism
pin) → **`objc2-core-graphics` is NOT a Chunk-1b dep** (deferred with the on-screen half;
the `setColorSpace` body is R5-preserved as a seam). Readback:
`surface.lock(ReadOnly, null)` / `surface.base_address()` / `surface.bytes_per_row()` /
copy `w*h*4` BGRA / `surface.unlock(ReadOnly, null)` (all on `IOSurfaceRef`, `libc` feature).

**HARD SPOT 2 — Frame block2 completion trampoline ([F6], mut-args-strongly-protected):
RESOLVED.** The **sync path (the golden path) needs NO ObjC block**: Frame stashes a
`CompletionContext { renderer: *mut GenericRenderer, target: *mut Target, sync: bool }`
of **raw `*mut`** pointers plus a plain-Rust `CompletionFn` fn-pointer; `complete(true)`
does `buffer.commit()` → `buffer.waitUntilCompleted()` → sets `sync=true` → calls the
fn-pointer directly (the faithful analogue of Zig's `CompletionBlock.invoke`). Because
`GenericRenderer`/`frameCompleted`/`present` do not exist until Chunk 2/3, Frame is made
1b-compilable by a **forward-declared raw-pointer callback contract** (the renderer pointer
is type-erased to `*mut ()`; the concrete callback is installed by Chunk-3 `drawFrame`).
The **async path** (`buffer.addCompletedHandler(block)` via
`block2::RcBlock<dyn Fn(NonNull<ProtocolObject<dyn MTLCommandBuffer>>)>`) is the DEFERRED
windowing-host seam. `MTLCommandBuffer::{status,commit,waitUntilCompleted}` and
`MTLCommandQueue::commandBuffer` all confirmed present. Aliasing model + Miri/ASan callout
in §OPEN RISKS.

**HARD SPOT 3 — Pipeline vertex-attribute tables (autoAttribute → hand-written, §G):
RESOLVED.** Three instanced vertex structs get descriptors — `CellText`, `Image`, `BgImage`
(the `bg_color`/`cell_bg` and all `post_pipelines` use the full-screen-triangle vertex fn
with NO vertex input → `None`). Hand-written const tables below give the exact
`(MTLVertexFormat, offset)` per field, computed from the Zig `@offsetOf`; a
`const{}`-assert pins `size_of::<V>()` to the Zig `@sizeOf` (32/40/8) and each table offset
to `core::mem::offset_of!`. **DEPENDENCY (flagged):** the offsets are correct with the
current `crate::shaders` structs, but the packed-bool *bytes* (`CellTextBools`,
`BgImageInfo`) are not yet byte-faithful to the Zig `packed struct(u8)` the shader reads —
that is a Chunk-4 golden prerequisite, not a 1b blocker (see §OPEN RISKS #1).

**Cargo deps to ADD (Chunk 1b):**
- `objc2-metal` features `+= ["MTLLibrary", "dispatch2", "block2", "objc2-io-surface"]`.
- `objc2-io-surface = { version = "0.3", features = ["IOSurfaceRef","IOSurfaceTypes","objc2","objc2-core-foundation","libc"] }`.
- `objc2-core-foundation = { version = "0.3", features = ["CFBase","CFDictionary","CFNumber","CFString"] }`.
- `dispatch2 = { version = "0.3", features = ["block2"] }`.
- `block2 = { version = "0.6" }`.
- (Chunk 4, not 1b) `png` dev-dep for the golden; `objc2-core-graphics` DEFERRED.

**1b compile boundary:** all five leaves compile and link against Chunk-1a + `crate::shaders`
with **zero** reference to the not-yet-existing `GenericRenderer`/`Metal`/`initShaders`.
Frame's completion callback is a forward-declared fn-pointer contract (erased renderer ptr);
the concrete `frameCompleted`/`present` wiring lands in Chunk 3. `Shaders::init` needs a
`device` + `pixel_format` handed in by the caller (no `Metal` type dependency). Gate: `cargo
build -p rustty-renderer` on macOS + a scratch smoke test that builds a Pipeline/Shaders/Target/
Frame against `MTLCreateSystemDefaultDevice()` without panic. Un-defer `size_of::<CellText>()==32`.

---

## §1 — Module layout & the five leaves

Mirror the landed lowercase module names in `metal.rs`:

```rust
// crates/rustty-renderer/src/metal.rs  (ADD to the existing `pub mod {api,buffer,sampler,texture};`)
pub mod pipeline;      // ports metal/Pipeline.zig      (+ HARD SPOT 3 vertex tables)
pub mod render_pass;   // ports metal/RenderPass.zig     (§A.3 Step/Attachment/Draw)
pub mod target;        // ports metal/Target.zig         (+ HARD SPOT 1 IOSurface)
pub mod frame;         // ports metal/Frame.zig          (+ HARD SPOT 2 completion)
pub mod shaders;       // ports metal/shaders.zig backend (Shaders/PipelineCollection/library)
```

`metal::shaders` (the backend container) is DISTINCT from the crate-root `crate::shaders`
(the vertex/uniform POD structs, already ported). ghostty's single `metal/shaders.zig`
holds both; rustty already split the PODs to `crate::shaders`, so `metal::shaders` re-uses
`crate::shaders::{CellText, Image, BgImage}` for the attribute tables and owns only the
GPU-backend machinery (`Shaders`, `PipelineCollection`, `PipelineDescription`, library load).

Shared error type: each landed leaf declares its own `Error { MetalFailed }`. The five new
leaves follow suit (`pipeline::Error`, `shaders::Error`, `target::Error`, …), each
`#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Error { MetalFailed }`, exactly as
`buffer::Error`/`texture::Error`/`sampler::Error` do today. (A crate-wide `RendererError`
is a Chunk-3 concern; the leaves stay leaf-local, matching Chunk-1a.)

---

## §2 — HARD SPOT 3: `pipeline.rs` + the vertex-attribute tables

### 2.1 `Pipeline` + `Options` (ports `Pipeline.zig` 1–52, 137–152)

```rust
// metal/pipeline.rs
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBlendOperation, MTLDevice, MTLFunction, MTLLibrary, MTLPixelFormat,
    MTLRenderPipelineColorAttachmentDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLVertexDescriptor, MTLVertexStepFunction,
};
use crate::metal::api;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error { MetalFailed }

/// Describes a color attachment. (Pipeline.zig `Options.Attachment`.)
pub struct Attachment {
    pub pixel_format: api::MTLPixelFormat,
    pub blending_enabled: bool,      // Zig default = true; callers pass it explicitly
}

/// Options for initializing a render pipeline. (Pipeline.zig `Options`.)
pub struct Options<'a> {
    pub device: &'a ProtocolObject<dyn MTLDevice>,
    pub vertex_fn: &'a str,
    pub fragment_fn: &'a str,
    pub vertex_library: &'a ProtocolObject<dyn MTLLibrary>,
    pub fragment_library: &'a ProtocolObject<dyn MTLLibrary>,
    pub step_fn: api::MTLVertexStepFunction,   // Zig default `.per_vertex`
    pub attachments: &'a [Attachment],
}

/// MTLRenderPipelineState (Pipeline.zig top-level `state` field).
pub struct Pipeline {
    pub state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}
```

`Pipeline::init` takes the comptime `?type` `VertexAttributes` (Pipeline.zig:44) as a
runtime `Option<&'static [VertexAttribute]>` — the hand-written table from §2.2. Two forms:

```rust
impl Pipeline {
    /// Ports `Pipeline.init(comptime VertexAttributes: ?type, opts)`. The comptime
    /// `?type` becomes an `Option<&'static [VertexAttribute]>`: `Some(table)` builds
    /// the MTLVertexDescriptor from the table; `None` = no vertex input
    /// (full-screen-triangle pipelines + post-pipelines).
    pub fn init(
        vertex_attributes: Option<VertexLayoutRef>,
        opts: Options<'_>,
    ) -> Result<Self, Error> {
        // PORT NOTE (ARC): ghostty's alloc/init + `defer release()` → objc2's
        // `MTLRenderPipelineDescriptor::new()` owned by a scope-releasing Retained.
        let desc = MTLRenderPipelineDescriptor::new();

        // vertex + fragment functions: createWithBytes → NSString; newFunctionWithName:.
        let vtx_name = NSString::from_str(opts.vertex_fn);
        let func_vert = opts.vertex_library.newFunctionWithName(&vtx_name)
            .ok_or(Error::MetalFailed)?;          // ghostty force-unwraps `ptr.?`
        desc.setVertexFunction(Some(&func_vert));

        let frag_name = NSString::from_str(opts.fragment_fn);
        let func_frag = opts.fragment_library.newFunctionWithName(&frag_name)
            .ok_or(Error::MetalFailed)?;
        desc.setFragmentFunction(Some(&func_frag));

        // If we have vertex attributes, build + attach a vertex descriptor.
        if let Some(table) = vertex_attributes {
            let vertex_desc = MTLVertexDescriptor::new();
            auto_attribute(table, &vertex_desc);   // §2.2 (the hand-written autoAttribute)

            // layout[0]: stepFunction + stride.
            let layouts = vertex_desc.layouts();
            // SAFETY: index 0 is in range; objc2 marks the subscript unsafe.
            let layout = unsafe { layouts.objectAtIndexedSubscript(0) };
            layout.setStepFunction(opts.step_fn);
            // SAFETY: setStride is unsafe (stride invariants); the value is size_of::<V>().
            unsafe { layout.setStride(table.stride) };

            desc.setVertexDescriptor(Some(&vertex_desc));
        }

        // Color attachments (Pipeline.zig 114–135): premultiplied-alpha blending.
        let attachments = desc.colorAttachments();
        for (i, at) in opts.attachments.iter().enumerate() {
            // SAFETY: index `i` < attachments.len(); subscript is unsafe in objc2.
            let attachment = unsafe { attachments.objectAtIndexedSubscript(i) };
            attachment.setPixelFormat(at.pixel_format);
            attachment.setBlendingEnabled(at.blending_enabled);
            if at.blending_enabled {
                // "We always use premultiplied alpha blending for now."
                attachment.setRgbBlendOperation(MTLBlendOperation::Add);
                attachment.setAlphaBlendOperation(MTLBlendOperation::Add);
                attachment.setSourceRGBBlendFactor(MTLBlendFactor::One);
                attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
                attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
                attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
            }
        }

        // Make our state. `newRenderPipelineStateWithDescriptor:error:` → Result.
        // ghostty's checkError(err) → idiomatic `.map_err`.
        let state = opts.device
            .newRenderPipelineStateWithDescriptor_error(&desc)
            .map_err(|_ns_error| Error::MetalFailed)?;   // ghostty logs localizedDescription
        Ok(Self { state })
    }
    // deinit(): Zig `self.state.release()` → Retained Drop.
}
```

Verified objc2 signatures (all present in `objc2-metal` 0.3.2):
`MTLLibrary::newFunctionWithName(&self, &NSString) -> Option<Retained<ProtocolObject<dyn MTLFunction>>>`;
`MTLRenderPipelineDescriptor::{new, setVertexFunction(Option<&…>), setFragmentFunction, setVertexDescriptor(Option<&MTLVertexDescriptor>), colorAttachments() -> Retained<MTLRenderPipelineColorAttachmentDescriptorArray>}`;
the color-attachment setters `set{PixelFormat,BlendingEnabled,RgbBlendOperation,AlphaBlendOperation,SourceRGBBlendFactor,SourceAlphaBlendFactor,DestinationRGBBlendFactor,DestinationAlphaBlendFactor}`;
`MTLDevice::newRenderPipelineStateWithDescriptor_error(&self, &MTLRenderPipelineDescriptor) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>, Retained<NSError>>`.
`MTLBlendFactor::OneMinusSourceAlpha` and `MTLBlendOperation::Add` are the objc2 spellings of
ghostty's `.one_minus_source_alpha`/`.add`.

### 2.2 The hand-written `autoAttribute` (§G item 3/5, [F-vertex])

`Pipeline.zig:154 autoAttribute` uses comptime field reflection. Rust has none → a per-struct
**const table**. The `VertexLayout` trait + a `VertexAttribute` POD:

```rust
// metal/pipeline.rs
/// One attribute row: an MTLVertexFormat + its byte offset within the vertex struct.
/// (The `attr.setBufferIndex(0)` is a constant in autoAttribute, folded into the loop.)
#[derive(Clone, Copy)]
pub struct VertexAttribute {
    pub format: api::MTLVertexFormat,
    pub offset: usize,
}

/// A vertex struct's attribute table + its stride (== size_of::<V>()).
#[derive(Clone, Copy)]
pub struct VertexLayoutRef {
    pub attrs: &'static [VertexAttribute],
    pub stride: usize,
}

/// Ports Pipeline.zig `autoAttribute`: for each attribute, set format/offset/bufferIndex(0).
fn auto_attribute(table: VertexLayoutRef, vertex_desc: &MTLVertexDescriptor) {
    let attrs = vertex_desc.attributes();
    for (i, a) in table.attrs.iter().enumerate() {
        // SAFETY: index `i` < table.attrs.len(); subscript/setters are unsafe in objc2.
        let attr = unsafe { attrs.objectAtIndexedSubscript(i) };
        attr.setFormat(a.format);
        unsafe {
            attr.setOffset(a.offset);
            attr.setBufferIndex(0);
        }
    }
}
```

**The exact per-struct tables** — field → `MTLVertexFormat` (via the autoAttribute switch on
the field's backing/tag type) + `@offsetOf`. Offsets computed from the Zig `extern struct`
declarations (which the current `#[repr(C)]` Rust structs reproduce byte-for-byte in
*offset*; see §OPEN RISKS #1 for the *content* caveat).

**`CellText`** (Zig `@sizeOf` == **32**, stride 32):

| # | field | Zig type (→ backing) | MTLVertexFormat | offset |
|---|---|---|---|---|
| 0 | `glyph_pos`  | `[2]u32`        | `UInt2`  (37) | 0  |
| 1 | `glyph_size` | `[2]u32`        | `UInt2`  (37) | 8  |
| 2 | `bearings`   | `[2]i16`        | `Short2` (16) | 16 |
| 3 | `grid_pos`   | `[2]u16`        | `UShort2`(13) | 20 |
| 4 | `color`      | `[4]u8`         | `UChar4` (3)  | 24 |
| 5 | `atlas`      | `Atlas` enum(u8)→`u8` | `UChar`(45) | 28 |
| 6 | `bools`      | `packed(u8)`→`u8`     | `UChar`(45) | 29 |

**`Image`** (Zig `@sizeOf` == **40**, stride 40):

| # | field | Zig type | MTLVertexFormat | offset |
|---|---|---|---|---|
| 0 | `grid_pos`    | `[2]f32` | `Float2`(29) | 0  |
| 1 | `cell_offset` | `[2]f32` | `Float2`(29) | 8  |
| 2 | `source_rect` | `[4]f32` | `Float4`(31) | 16 |
| 3 | `dest_size`   | `[2]f32` | `Float2`(29) | 32 |

**`BgImage`** (Zig `@sizeOf` == **8**, stride 8):

| # | field | Zig type | MTLVertexFormat | offset |
|---|---|---|---|---|
| 0 | `opacity` | `f32`               | `Float`(28) | 0 |
| 1 | `info`    | `Info` packed(u8)→`u8` | `UChar`(45) | 4 |

All eight `MTLVertexFormat` constants verified present in
`objc2_metal::MTLVertexDescriptor` (UInt2=37, Short2=16, UShort2=13, UChar4=3, UChar=45,
Float=28, Float2=29, Float4=31 — identical Apple values).

The tables + a `VertexLayout` impl live in `metal::shaders` (beside the pipeline descs that
reference them) or `metal::pipeline`; recommend `metal::shaders` so `PipelineDescription`
selects the layout without a cross-module const cycle:

```rust
// metal/shaders.rs
use crate::metal::pipeline::{VertexAttribute as VA, VertexLayoutRef};
use crate::metal::api::MTLVertexFormat as F;
use crate::shaders::{BgImage, CellText, Image};

pub trait VertexLayout: Sized {
    const LAYOUT: VertexLayoutRef;
}

impl VertexLayout for CellText {
    const LAYOUT: VertexLayoutRef = VertexLayoutRef {
        attrs: &[
            VA { format: F::UInt2,   offset: 0 },
            VA { format: F::UInt2,   offset: 8 },
            VA { format: F::Short2,  offset: 16 },
            VA { format: F::UShort2, offset: 20 },
            VA { format: F::UChar4,  offset: 24 },
            VA { format: F::UChar,   offset: 28 },
            VA { format: F::UChar,   offset: 29 },
        ],
        stride: core::mem::size_of::<CellText>(),
    };
}
// … impl VertexLayout for Image (Float2/Float2/Float4/Float2 @ 0/8/16/32, stride 40)
// … impl VertexLayout for BgImage (Float/UChar @ 0/4, stride 8)
```

### 2.3 Drift guard (the [F-vertex] mitigation + un-deferred `CellText==32`)

A `const{}` block per struct asserts (a) size == the Zig `@sizeOf`, and (b) every table
offset equals `offset_of!` — so the hand-written table can never silently drift from the
Rust struct, and the Rust struct can never silently drift from the Zig `@sizeOf`:

```rust
// metal/shaders.rs — compile-time drift guards (a failed const panics at build time).
const _: () = {
    assert!(core::mem::size_of::<CellText>() == 32);   // un-defers metal/shaders.zig's test{}
    assert!(core::mem::offset_of!(CellText, glyph_pos)  == 0);
    assert!(core::mem::offset_of!(CellText, glyph_size) == 8);
    assert!(core::mem::offset_of!(CellText, bearings)   == 16);
    assert!(core::mem::offset_of!(CellText, grid_pos)   == 20);
    assert!(core::mem::offset_of!(CellText, color)      == 24);
    assert!(core::mem::offset_of!(CellText, atlas)      == 28);
    assert!(core::mem::offset_of!(CellText, bools)      == 29);
    assert!(core::mem::size_of::<Image>()   == 40);
    assert!(core::mem::size_of::<BgImage>() == 8);
    // (+ offset_of asserts for Image/BgImage)
};
```

`size_of`/`offset_of!` are `const`-stable (Rust ≥1.77), so `const{}` asserts work with no
extra crate features. The reference `metal/shaders.zig` `test { expectEqual(32, @sizeOf(CellText)) }`
is thereby **un-deferred** as a compile-time invariant (stronger than a runtime `#[test]`);
if the team prefers a literal `#[test]` for the inventory count, add a one-line
`#[test] fn cell_text_size() { assert_eq!(32, size_of::<CellText>()); }` — either satisfies R1.

> **Caveat (does NOT block 1b, blocks Chunk-4 golden):** the offsets above are correct, but the
> `atlas`/`bools`/`info` *bytes* are only shader-correct once `crate::shaders`'s packed-bool
> sub-structs (`CellTextBools`, `BgImageInfo`) serialize to a single `packed struct(u8)` byte
> (bit0/bit1…), which they do not yet (they are multi-byte non-`repr(C)` structs). See
> §OPEN RISKS #1.

---

## §3 — `render_pass.rs` (§A.3 Step/Attachment/Draw concrete, ports `RenderPass.zig`)

Monomorphic (§F): `Step`/`Attachment`/`Draw` are **concrete** structs — no `A::*` generics,
no `RawBuffer` associated type. Per §A.3 the fields are **borrows** (`&…`), not owned
`Retained` clones, to preserve Zig's zero-refcount by-value copy semantics (a `Retained`
clone would bump the refcount per step per frame). Zig's erased `[]const ?objc.Object`
buffer slice becomes `&[Option<&ProtocolObject<dyn MTLBuffer>>]` (already `T`-erased, exactly
what `buffer::Buffer::<T>::raw()` returns).

```rust
// metal/render_pass.rs
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLLoadAction, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLStoreAction, MTLTexture,
};
use crate::metal::api;
use crate::metal::pipeline::Pipeline;
use crate::metal::sampler::Sampler;
use crate::metal::target::Target;
use crate::metal::texture::Texture;

/// Where a color attachment renders to (RenderPass.zig `Attachment.target` union).
pub enum AttachmentTarget<'a> {
    Texture(&'a Texture),
    Target(&'a Target),
}

/// Describes a color attachment. (RenderPass.zig `Options.Attachment`.)
pub struct Attachment<'a> {
    pub target: AttachmentTarget<'a>,
    pub clear_color: Option<[f64; 4]>,   // Zig `?[4]f64 = null`
}

/// Describes the draw call for a step. (RenderPass.zig `Step.Draw`.)
pub struct Draw {
    pub kind: api::MTLPrimitiveType,   // Zig field name `type`; `kind` avoids the Rust kw
    pub vertex_count: usize,
    pub instance_count: usize,         // Zig default 1; set explicitly by callers
}

/// A step in a render pass. (RenderPass.zig `Step`.) Fields are borrows (§A.3).
pub struct Step<'a> {
    pub pipeline: &'a Pipeline,
    /// MTLBuffer (uniforms), bound at index 1.
    pub uniforms: Option<&'a ProtocolObject<dyn MTLBuffer>>,
    /// MTLBuffers: [0] = vertex@index0, rest start at index 2.
    pub buffers: &'a [Option<&'a ProtocolObject<dyn MTLBuffer>>],
    pub textures: &'a [Option<&'a Texture>],
    pub samplers: &'a [Option<&'a Sampler>],
    pub draw: Draw,
}

/// Options for beginning a render pass. (RenderPass.zig `Options`.)
pub struct Options<'a> {
    pub command_buffer: &'a ProtocolObject<dyn MTLCommandBuffer>,
    pub attachments: &'a [Attachment<'a>],
}

/// MTLRenderCommandEncoder (RenderPass.zig top-level `encoder` field).
pub struct RenderPass {
    pub encoder: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
}
```

`begin` / `step` / `complete`:

```rust
impl RenderPass {
    /// Ports RenderPass.begin.
    pub fn begin(opts: Options<'_>) -> Self {
        let desc = MTLRenderPassDescriptor::renderPassDescriptor();   // class method
        let attachments = desc.colorAttachments();
        for (i, at) in opts.attachments.iter().enumerate() {
            // SAFETY: `i` in range; subscript is unsafe in objc2.
            let a = unsafe { attachments.objectAtIndexedSubscript(i) };
            a.setLoadAction(if at.clear_color.is_some() {
                MTLLoadAction::Clear
            } else {
                MTLLoadAction::Load
            });
            a.setStoreAction(MTLStoreAction::Store);
            let tex: &ProtocolObject<dyn MTLTexture> = match at.target {
                AttachmentTarget::Texture(t) => &t.texture,
                AttachmentTarget::Target(t)  => &t.texture,   // Target's IOSurface-backed tex
            };
            a.setTexture(Some(tex));
            if let Some(c) = at.clear_color {
                a.setClearColor(api::MTLClearColor { red: c[0], green: c[1], blue: c[2], alpha: c[3] });
            }
        }
        // renderCommandEncoderWithDescriptor: → the encoder.
        let encoder = opts.command_buffer
            .renderCommandEncoderWithDescriptor(&desc)
            .expect("renderCommandEncoderWithDescriptor returned nil");  // Zig stores nil-less
        Self { encoder }
    }

    /// Ports RenderPass.step (the ordered encoder calls).
    pub fn step(&self, s: Step<'_>) {
        if s.draw.instance_count == 0 { return; }
        self.encoder.setRenderPipelineState(&s.pipeline.state);

        if !s.buffers.is_empty() {
            // index 0 = vertex buffer (both stages). SAFETY: objc2 setters unsafe.
            if let Some(buf) = s.buffers[0] {
                unsafe {
                    self.encoder.setVertexBuffer_offset_atIndex(Some(buf), 0, 0);
                    self.encoder.setFragmentBuffer_offset_atIndex(Some(buf), 0, 0);
                }
            }
            // rest from index 2 (index 1 reserved for uniforms).
            for (i, b) in s.buffers[1..].iter().enumerate() {
                if let Some(buf) = b {
                    let idx = i + 2;
                    unsafe {
                        self.encoder.setVertexBuffer_offset_atIndex(Some(buf), 0, idx);
                        self.encoder.setFragmentBuffer_offset_atIndex(Some(buf), 0, idx);
                    }
                }
            }
        }
        if let Some(buf) = s.uniforms {
            unsafe {
                self.encoder.setVertexBuffer_offset_atIndex(Some(buf), 0, 1);
                self.encoder.setFragmentBuffer_offset_atIndex(Some(buf), 0, 1);
            }
        }
        for (i, t) in s.textures.iter().enumerate() {
            if let Some(tex) = t {
                unsafe {
                    self.encoder.setVertexTexture_atIndex(Some(&tex.texture), i);
                    self.encoder.setFragmentTexture_atIndex(Some(&tex.texture), i);
                }
            }
        }
        for (i, samp) in s.samplers.iter().enumerate() {
            if let Some(sampler) = samp {
                unsafe { self.encoder.setFragmentSamplerState_atIndex(Some(&sampler.sampler), i); }
            }
        }
        unsafe {
            self.encoder.drawPrimitives_vertexStart_vertexCount_instanceCount(
                s.draw.kind, 0, s.draw.vertex_count, s.draw.instance_count);
        }
    }

    /// Ports RenderPass.complete: endEncoding.
    pub fn complete(&self) { self.encoder.endEncoding(); }
}
```

All encoder selectors verified present (`MTLRenderCommandEncoder` feature already enabled):
`setRenderPipelineState(&ProtocolObject<dyn MTLRenderPipelineState>)`,
`setVertexBuffer_offset_atIndex(Option<&…MTLBuffer>, NSUInteger, NSUInteger)` (unsafe),
`setFragmentBuffer_offset_atIndex`, `setVertexTexture_atIndex`, `setFragmentTexture_atIndex`,
`setFragmentSamplerState_atIndex`,
`drawPrimitives_vertexStart_vertexCount_instanceCount(MTLPrimitiveType, NSUInteger×3)` (unsafe),
`endEncoding`. Render-pass side: `MTLRenderPassDescriptor::renderPassDescriptor()`,
`.colorAttachments()`, `MTLRenderPassColorAttachmentDescriptor::{setLoadAction, setStoreAction,
setTexture(Option<&…>), setClearColor(MTLClearColor)}`,
`MTLCommandBuffer::renderCommandEncoderWithDescriptor(&MTLRenderPassDescriptor) ->
Option<Retained<…>>`.

**Lifetime note (§A.3, OPEN RISK #1 in the parent):** the borrow-threading through
`Step<'a>` is the one place that needs a real typecheck against the per-frame draw loop —
in particular `image.zig`'s `images.draw` builds a `Buffer<Image>` per placement and drops it
after the step, so the `&buf.raw()` borrow in `Step.buffers` must not outlive the step call.
That loop is Chunk-3/prerequisite code; the `Step<'a>` shape here is designed to accept a
borrow that lives only for the `pass.step(s)` call, which is sufficient. If borrow-threading
proves awkward there, the documented fallback is owned `Retained` clones (behavior-equivalent,
+1 retain/release per step — flag for Goal-2). This does not affect 1b compilation.

---

## §4 — HARD SPOT 1: `target.rs` (IOSurface offscreen, [F12])

### 4.1 `Target` + `Options` (ports `Target.zig`)

```rust
// metal/target.rs
use core::ffi::c_void;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::{CFDictionary, CFNumber, CFNumberType, CFRetained, CFString, CFType};
use objc2_io_surface::{
    IOSurfaceRef, IOSurfaceLockOptions,
    kIOSurfaceBytesPerElement, kIOSurfaceHeight, kIOSurfacePixelFormat, kIOSurfaceWidth,
};
use objc2_metal::{MTLDevice, MTLTexture, MTLTextureDescriptor};
use crate::metal::api;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error { MetalFailed }

/// Options for initializing a Target. (Target.zig `Options`.)
pub struct Options<'a> {
    pub device: &'a ProtocolObject<dyn MTLDevice>,
    pub width: usize,
    pub height: usize,
    pub pixel_format: api::MTLPixelFormat,      // golden pins BGRA8Unorm (non-sRGB)
    pub storage_mode: api::StorageMode,          // golden pins .Shared (Apple Silicon)
}

/// An IOSurface-backed MTLTexture render target. (Target.zig top-level fields.)
pub struct Target {
    /// The underlying IOSurface (owns the CF ref; releases on Drop).
    pub surface: CFRetained<IOSurfaceRef>,
    /// The underlying MTLTexture.
    pub texture: Retained<ProtocolObject<dyn MTLTexture>>,
    pub width: usize,
    pub height: usize,
}
```

### 4.2 `init` — the CFDictionary + `IOSurfaceCreate` + iosurface-backed texture

Maps ghostty's `IOSurface.init(.{width,height,pixel_format=.@"32BGRA",bytes_per_element=4,
colorspace=DisplayP3})` (`pkg/macos/iosurface/iosurface.zig`) — a 4-key CFDictionary of
`CFNumber(int)` values, then `IOSurfaceCreate`, then (skipped offscreen) `setColorSpace`.

```rust
impl Target {
    pub fn init(opts: Options<'_>) -> Result<Self, Error> {
        // ---- IOSurface property dictionary (ghostty foundation.Dictionary.create) ----
        // Values are CFNumber(.int) — ghostty uses `foundation.Number.create(.int, &c_int)`
        // → CFNumberType::IntType. Width/Height/BytesPerElement are the opts; PixelFormat is
        // the FourCC 'BGRA' == 0x42475241 (kCVPixelFormatType_32BGRA).
        const BGRA_FOURCC: i32 = 0x4247_5241; // 'B''G''R''A'
        let w = cf_number_int(opts.width as i32).ok_or(Error::MetalFailed)?;
        let h = cf_number_int(opts.height as i32).ok_or(Error::MetalFailed)?;
        let pf = cf_number_int(BGRA_FOURCC).ok_or(Error::MetalFailed)?;
        let bpe = cf_number_int(4).ok_or(Error::MetalFailed)?;

        // CFDictionaryCreate(keys, values, 4, kCFTypeDictionary{Key,Value}CallBacks).
        // Keys are the `&'static CFString` externs; values are the CFNumbers (as CFType).
        let keys: [*const c_void; 4] = [
            (kIOSurfaceWidth as *const CFString).cast(),
            (kIOSurfaceHeight as *const CFString).cast(),
            (kIOSurfacePixelFormat as *const CFString).cast(),
            (kIOSurfaceBytesPerElement as *const CFString).cast(),
        ];
        let vals: [*const c_void; 4] = [
            (&*w as *const CFNumber).cast(),
            (&*h as *const CFNumber).cast(),
            (&*pf as *const CFNumber).cast(),
            (&*bpe as *const CFNumber).cast(),
        ];
        // SAFETY: 4 valid keys/values; the CF type-callbacks retain/release CF objects.
        let props: CFRetained<CFDictionary> = unsafe {
            CFDictionary::new(
                None,
                keys.as_ptr().cast_mut(),
                vals.as_ptr().cast_mut(),
                4,
                core::ptr::addr_of!(objc2_core_foundation::kCFTypeDictionaryKeyCallBacks),
                core::ptr::addr_of!(objc2_core_foundation::kCFTypeDictionaryValueCallBacks),
            )
        }.ok_or(Error::MetalFailed)?;

        // SAFETY: `props` holds the correctly-typed IOSurface properties.
        let surface: CFRetained<IOSurfaceRef> =
            unsafe { IOSurfaceRef::new(&props) }.ok_or(Error::MetalFailed)?;

        // [F12] OFFSCREEN SCOPE: ghostty here calls surface.setColorSpace(DisplayP3)
        // (CGColorSpaceCreateWithName + CGColorSpaceCopyPropertyList + IOSurfaceSetValue).
        // The offscreen golden reads RAW BGRA bytes, which ignore the IOSurface colorspace
        // tag, and the golden determinism pin requires colorspace != display-p3 — so the
        // colorspace-set is SKIPPED here and R5-preserved as `set_color_space` (deferred
        // windowing-host seam; needs objc2-core-graphics, NOT a Chunk-1b dep).

        // ---- IOSurface-backed MTLTexture (Target.zig descriptor) ----
        let desc = MTLTextureDescriptor::new();
        // SAFETY: setWidth/setHeight unsafe (dimension invariants).
        unsafe { desc.setWidth(opts.width); desc.setHeight(opts.height); }
        desc.setPixelFormat(opts.pixel_format);
        // usage = { render_target: true }  (api::MTLTextureUsage → objc2 bitflags via resolve)
        desc.setUsage(api::MTLTextureUsage { render_target: true, ..Default::default() }.resolve());
        // resourceOptions = { cpu_cache_mode: write_combined, storage_mode: opts.storage_mode }
        desc.setResourceOptions(api::MTLResourceOptions {
            cpu_cache_mode: api::CPUCacheMode::WriteCombined,
            storage_mode: opts.storage_mode,
            hazard_tracking_mode: api::HazardTrackingMode::Default,
        }.resolve());

        // newTextureWithDescriptor:iosurface:plane: (plane 0).
        let texture = opts.device
            .newTextureWithDescriptor_iosurface_plane(&desc, &surface, 0)
            .ok_or(Error::MetalFailed)?;

        Ok(Self { surface, texture, width: opts.width, height: opts.height })
    }
    // deinit(): Zig `surface.deinit()` (setPurgeable+CFRelease) + `texture.release()` →
    //   CFRetained/Retained Drop. PORT NOTE: ghostty's setPurgeable(.empty) eager-unload is
    //   a Goal-2 nicety, not behavior; CFRetained's Drop CFReleases faithfully.
}

/// ghostty `foundation.Number.create(.int, &value)` → CFNumber(IntType). `value` is a c_int.
fn cf_number_int(value: i32) -> Option<CFRetained<CFNumber>> {
    // SAFETY: value_ptr points at a live i32 for the duration of the call; IntType matches.
    unsafe {
        CFNumber::new(None, CFNumberType::IntType, core::ptr::addr_of!(value).cast())
    }
}
```

Verified against the crate sources:
- `IOSurfaceRef::new(properties: &CFDictionary) -> Option<CFRetained<IOSurfaceRef>>` (unsafe;
  the recommended non-deprecated name for `IOSurfaceCreate`).
- `kIOSurfaceWidth`/`kIOSurfaceHeight`/`kIOSurfacePixelFormat`/`kIOSurfaceBytesPerElement`:
  `pub static … : &'static CFString` externs (feature `objc2-core-foundation`).
- `CFNumber::new(Option<&CFAllocator>, CFNumberType, *const c_void) -> Option<CFRetained<CFNumber>>`;
  `CFNumberType::IntType` == `kCFNumberIntType` (matches ghostty's `.int`).
- `CFDictionary::new(Option<&CFAllocator>, *mut *const c_void keys, *mut *const c_void values,
  CFIndex, *const CFDictionaryKeyCallBacks, *const CFDictionaryValueCallBacks) ->
  Option<CFRetained<CFDictionary>>`; `kCFTypeDictionary{Key,Value}CallBacks` are the extern
  callback statics (faithful to ghostty's `foundation.Dictionary.create`).
- `MTLDevice::newTextureWithDescriptor_iosurface_plane(&self, &MTLTextureDescriptor,
  &IOSurfaceRef, NSUInteger) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>` — gated on
  objc2-metal feature `objc2-io-surface`.

### 4.3 The readback path (the golden gate needs this — §6 of the blueprint)

New rustty test-support code (not a ghostty port); lives on `Target` (or a free fn) so the
Chunk-4 golden can pull `w*h*4` BGRA bytes out of the offscreen surface after
`waitUntilCompleted`:

```rust
impl Target {
    /// Copy the surface's `width*height*4` BGRA bytes into a `Vec<u8>`, honoring
    /// bytesPerRow (row stride may exceed width*4). Call AFTER the frame's command
    /// buffer has completed. (Golden-gate support; blueprint §6.)
    pub fn read_bgra(&self) -> Vec<u8> {
        let bpr = self.surface.bytes_per_row();          // IOSurfaceGetBytesPerRow
        let w4 = self.width * 4;
        let mut out = vec![0u8; self.width * self.height * 4];
        // SAFETY: ReadOnly lock; base_address is valid between lock/unlock; we read only
        // `w4` bytes per row for `height` rows, all within the surface allocation.
        unsafe {
            let _ = self.surface.lock(IOSurfaceLockOptions::ReadOnly, core::ptr::null_mut());
            let base = self.surface.base_address().as_ptr() as *const u8;   // IOSurfaceGetBaseAddress
            for row in 0..self.height {
                core::ptr::copy_nonoverlapping(
                    base.add(row * bpr), out.as_mut_ptr().add(row * w4), w4);
            }
            let _ = self.surface.unlock(IOSurfaceLockOptions::ReadOnly, core::ptr::null_mut());
        }
        out
    }
}
```

Verified: `IOSurfaceRef::{lock,unlock}(IOSurfaceLockOptions, *mut u32) -> libc::kern_return_t`
(unsafe; feature `libc`), `base_address() -> NonNull<c_void>` (`IOSurfaceGetBaseAddress`),
`bytes_per_row() -> usize` (`IOSurfaceGetBytesPerRow`).

PORT NOTE: ghostty's own `IOSurface.lock` helper passes options `0`; the readback here uses
`ReadOnly` (correct for a pure read — avoids invalidating the GPU copy). Both are valid;
`ReadOnly` is chosen because this is rustty-new golden-support code, not a ghostty port.
Coherence: the golden runner is pinned to Apple-Silicon ([F5]) with `.Shared` storage → the
IOSurface memory is CPU-coherent after `waitUntilCompleted` (no blit needed). On Intel/managed
storage a `synchronizeResource` blit would be required — out of the offscreen scope.

---

## §5 — HARD SPOT 2: `frame.rs` (completion trampoline, [F6], mut-args-strongly-protected)

### 5.1 The 1b-compilable shape: a forward-declared raw-pointer callback contract

`Frame.zig` names `Renderer = generic.Renderer(Metal)` and, in the completion block, derefs
the stashed `*Renderer` to call `renderer.api.present(target, sync)` + `renderer.frameCompleted(health)`.
Neither `GenericRenderer` nor `frameCompleted`/`present` exists until Chunk 2/3. To make Frame
**compile in 1b** while preserving the raw-pointer aliasing model exactly, the renderer pointer
is **type-erased to `*mut ()`** and the completion body is a **fn-pointer** the Chunk-3
`drawFrame` installs. This is the faithful analogue of Zig's `CompletionBlock` (itself a
type-erased ObjC block over a `Context`); the fn-pointer is minimal Rust-only glue that breaks
the 1b→Chunk-3 ordering and the Frame↔Renderer type edge (R1 "no-loss, not no-addition").

```rust
// metal/frame.rs
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandQueue};
use crate::metal::api;
use crate::metal::render_pass::{Attachment, Options as RpOptions, RenderPass};
use crate::metal::target::Target;

/// Health status handed back to the renderer. (renderer.zig `Health`, already a crate type.)
use crate::Health;   // { Healthy, Unhealthy } — hoisted crate-root enum

/// The completion context ghostty stashes as `CompletionBlock.Context`. Raw `*mut` per
/// `rust-mut-args-strongly-protected`: the completion trampoline re-enters the renderer
/// while it is live inside `drawFrame` (the @fieldParentPtr/container_of precedent); a
/// `&mut` here would be UB. `renderer` is type-erased (`*mut ()`) so Frame compiles in 1b —
/// Chunk-3 casts it back to `*mut GenericRenderer` inside the installed callback.
pub struct CompletionContext {
    pub renderer: *mut (),
    pub target: *mut Target,
    pub sync: bool,
}

/// The completion body. Chunk-3 supplies a concrete fn that casts `ctx.renderer` back to
/// `*mut GenericRenderer` and runs `renderer.api.present(*ctx.target, ctx.sync)` (on healthy)
/// + `renderer.frameCompleted(health)`. In 1b this is only stored/typed, never authored.
pub type CompletionFn =
    unsafe fn(ctx: &CompletionContext, buffer: &ProtocolObject<dyn MTLCommandBuffer>);

/// Options for beginning a frame. (Frame.zig `Options`.)
pub struct Options<'a> {
    pub queue: &'a ProtocolObject<dyn MTLCommandQueue>,
}

/// A frame in flight. (Frame.zig top-level `buffer` + `block` fields.)
pub struct Frame {
    /// MTLCommandBuffer.
    pub buffer: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    /// The stashed completion context (Frame.zig `block: CompletionBlock.Context`).
    pub block: CompletionContext,
    /// The completion body (installed by the caller; = Zig's `&bufferCompleted`).
    pub on_completed: CompletionFn,
}
```

### 5.2 `begin` / `renderPass` / `complete`

```rust
impl Frame {
    /// Ports Frame.begin. `renderer` is the type-erased `*mut GenericRenderer`; `on_completed`
    /// is the concrete completion fn (Chunk-3 supplies `metal_buffer_completed`). Raw `*mut`
    /// per the aliasing model.
    pub fn begin(
        opts: Options<'_>,
        renderer: *mut (),
        target: *mut Target,
        on_completed: CompletionFn,
    ) -> Result<Self, Error> {
        let buffer = opts.queue.commandBuffer().ok_or(Error::MetalFailed)?;
        // Frame.zig stashes { renderer, target, sync = false } for the (deferred) async path
        // and for the sync invoke-directly path.
        Ok(Self {
            buffer,
            block: CompletionContext { renderer, target, sync: false },
            on_completed,
        })
    }

    /// Ports Frame.renderPass: begin a RenderPass on this frame's command buffer.
    pub fn render_pass(&self, attachments: &[Attachment<'_>]) -> RenderPass {
        RenderPass::begin(RpOptions { attachments, command_buffer: &self.buffer })
    }

    /// Ports Frame.complete. sync=true is the golden path (invoke-directly, NO ObjC block);
    /// sync=false (async addCompletedHandler) is the DEFERRED windowing-host seam.
    pub fn complete(&mut self, sync: bool) {
        if !sync {
            // DEFERRED ([F6], §5 blueprint): the async present half. R5-preserved shape —
            //   let handler = self.make_completion_block();          // block2::RcBlock (§5.3)
            //   unsafe { self.buffer.addCompletedHandler(RcBlock::as_ptr(&handler)); }
            unimplemented!("windowing-host: async addCompletedHandler present");
        }
        self.buffer.commit();
        if sync {
            self.buffer.waitUntilCompleted();
            self.block.sync = true;
            // Zig `CompletionBlock.invoke(&self.block, .{buffer})` — a direct call. Here the
            // Rust fn-pointer IS the block body; no ObjC roundtrip needed on the sync path.
            // SAFETY: the renderer pointer is live for the duration of drawFrame; single-
            // threaded on the sync path (we just waited); raw *mut, never aliased as &mut.
            unsafe { (self.on_completed)(&self.block, &self.buffer); }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error { MetalFailed }
```

The Chunk-3 completion fn (shown here for context; **authored in Chunk 3, not 1b**) recovers
the concrete renderer and reproduces `Frame.zig:67 bufferCompleted` verbatim:

```rust
// (Chunk 3, in the Metal/Renderer module — NOT part of Chunk 1b)
unsafe fn metal_buffer_completed(ctx: &CompletionContext, buffer: &ProtocolObject<dyn MTLCommandBuffer>) {
    let status = buffer.status();
    let health = match status {
        MTLCommandBufferStatus::Error => Health::Unhealthy,
        _ => Health::Healthy,
    };
    let renderer = ctx.renderer as *mut GenericRenderer;   // container_of / @fieldParentPtr
    if health == Health::Healthy {
        // present the target (offscreen: a no-op + readback stub, [F6]).
        let _ = (*renderer).api.present(&*ctx.target, ctx.sync);
    }
    (*renderer).frame_completed(health);
}
```

`MTLCommandQueue::commandBuffer(&self) -> Option<Retained<ProtocolObject<dyn MTLCommandBuffer>>>`,
`MTLCommandBuffer::{commit(&self), waitUntilCompleted(&self), status(&self) -> MTLCommandBufferStatus}`
all confirmed present. `MTLCommandBufferStatus::Error` is the objc2 spelling of Zig's `.@"error"`.

### 5.3 The deferred async block (shape, R5-preserved)

For when the on-screen half lands, the block is a `block2::RcBlock` over the exact
`MTLCommandBufferHandler` closure type:

```rust
// MTLCommandBufferHandler = *mut block2::DynBlock<dyn Fn(NonNull<ProtocolObject<dyn MTLCommandBuffer>>)>
// (DynBlock<F> = Block<F>; RcBlock::as_ptr(&b) yields *mut Block<F> == the handler type.)
fn make_completion_block(&self) -> block2::RcBlock<dyn Fn(core::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>)> {
    let ctx = /* a Copy snapshot or shared raw ptrs of self.block */;
    let on_completed = self.on_completed;
    block2::RcBlock::new(move |buf: core::ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
        // SAFETY: buf is a valid MTLCommandBuffer for the callback's duration.
        unsafe { on_completed(&ctx, buf.as_ref()); }
    })
}
// unsafe { self.buffer.addCompletedHandler(block2::RcBlock::as_ptr(&handler)); }
```

Verified: `MTLCommandBuffer::addCompletedHandler(&self, MTLCommandBufferHandler)` (unsafe,
feature `block2`); `RcBlock::new(Closure) -> RcBlock<dyn Fn…>`; `RcBlock::as_ptr(&Self) ->
*mut Block<F>`; `MTLCommandBufferHandler = *mut DynBlock<dyn Fn(NonNull<ProtocolObject<dyn
MTLCommandBuffer>>)>` and `pub type DynBlock<F> = Block<F>`.

---

## §6 — `shaders.rs` (backend container, §G item 3; library load [F7]/§3)

### 6.1 `PipelineCollection` + `PipelineDescription` (§G item 3 — the `@Type` synthesis)

Zig synthesizes `PipelineCollection` from `pipeline_descs` via `@Type(.{.@"struct" = …})`
(named fields `bg_color`/`cell_bg`/`cell_text`/`image`/`bg_image`, each `Pipeline`). Rust has
no type synthesis → **hand-write** the struct; the `inline for (pipeline_descs)` init/deinit
loops become explicit per-field construction. Field names MUST match (generic.zig reads
`shaders.pipelines.cell_text` etc.).

```rust
// metal/shaders.rs
use crate::metal::pipeline::{Attachment, Options as PipelineOptions, Pipeline, VertexLayoutRef};

/// (§G item 3) The PipelineCollection @Type-synthesized in Zig, hand-written. Field names
/// are read by generic.zig (`shaders.pipelines.cell_text`), so they are load-bearing.
pub struct PipelineCollection {
    pub bg_color: Pipeline,
    pub cell_bg: Pipeline,
    pub cell_text: Pipeline,
    pub image: Pipeline,
    pub bg_image: Pipeline,
}

/// Comptime-known pipeline info (shaders.zig `PipelineDescription`). The comptime
/// `vertex_attributes: ?type` becomes `Option<VertexLayoutRef>` (§2.2).
pub struct PipelineDescription {
    pub vertex_attributes: Option<VertexLayoutRef>,
    pub vertex_fn: &'static str,
    pub fragment_fn: &'static str,
    pub step_fn: crate::metal::api::MTLVertexStepFunction,  // Zig default .per_vertex
    pub blending_enabled: bool,
}

impl PipelineDescription {
    /// Ports PipelineDescription.initPipeline.
    fn init_pipeline(
        &self,
        device: &ProtocolObject<dyn MTLDevice>,
        library: &ProtocolObject<dyn MTLLibrary>,
        pixel_format: api::MTLPixelFormat,
    ) -> Result<Pipeline, Error> {
        Pipeline::init(self.vertex_attributes, PipelineOptions {
            device,
            vertex_fn: self.vertex_fn,
            fragment_fn: self.fragment_fn,
            vertex_library: library,
            fragment_library: library,
            step_fn: self.step_fn,
            attachments: &[Attachment { pixel_format, blending_enabled: self.blending_enabled }],
        }).map_err(|_| Error::MetalFailed)
    }
}
```

The `pipeline_descs` comptime table → a module `const`. Each `cell_text`/`image`/`bg_image`
entry carries its `VertexLayoutRef` via `Some(<Struct as VertexLayout>::LAYOUT)`; `bg_color`
and `cell_bg` (full-screen-triangle vertex fn, no vertex input) carry `None`:

```rust
const PIPELINE_DESCS: &[(&str, PipelineDescription)] = &[
    ("bg_color",  PipelineDescription { vertex_attributes: None,
        vertex_fn: "full_screen_vertex", fragment_fn: "bg_color_fragment",
        step_fn: F::PerVertex, blending_enabled: false }),
    ("cell_bg",   PipelineDescription { vertex_attributes: None,
        vertex_fn: "full_screen_vertex", fragment_fn: "cell_bg_fragment",
        step_fn: F::PerVertex, blending_enabled: true }),
    ("cell_text", PipelineDescription { vertex_attributes: Some(<CellText as VertexLayout>::LAYOUT),
        vertex_fn: "cell_text_vertex", fragment_fn: "cell_text_fragment",
        step_fn: F::PerInstance, blending_enabled: true }),
    ("image",     PipelineDescription { vertex_attributes: Some(<Image as VertexLayout>::LAYOUT),
        vertex_fn: "image_vertex", fragment_fn: "image_fragment",
        step_fn: F::PerInstance, blending_enabled: true }),
    ("bg_image",  PipelineDescription { vertex_attributes: Some(<BgImage as VertexLayout>::LAYOUT),
        vertex_fn: "bg_image_vertex", fragment_fn: "bg_image_fragment",
        step_fn: F::PerInstance, blending_enabled: true }),
];   // F = api::MTLVertexStepFunction
```

PORT NOTE (§G item 2, IntFittingRange): the Zig `inline for (pipeline_descs, 0..)` errdefer/
init loops become explicit per-field assignment in `Shaders::init` (below); `PipelineCollection`
is built field-by-field rather than by name-indexed reflection.

### 6.2 `Shaders` + `init`/`deinit` (ports `shaders.zig` `Shaders`)

```rust
pub struct Shaders {
    pub library: Retained<ProtocolObject<dyn MTLLibrary>>,
    pub pipelines: PipelineCollection,
    pub post_pipelines: Vec<Pipeline>,   // Zig `[]const Pipeline` + alloc; Vec owns it
    pub defunct: bool,                    // double-free guard (read by generic.zig:1505)
}

impl Shaders {
    /// Ports Shaders.init. `post_shaders` = custom-shader SOURCE (not paths). `device` +
    /// `pixel_format` are handed in by the caller (Chunk-2 `Metal.initShaders`) — no `Metal`
    /// type dependency, so this compiles in 1b.
    pub fn init(
        device: &ProtocolObject<dyn MTLDevice>,
        post_shaders: &[&std::ffi::CStr],
        pixel_format: api::MTLPixelFormat,
    ) -> Result<Self, Error> {
        let library = init_library(device)?;                     // §6.3
        // Build each pipeline. (Zig errdefer deinit-on-failure dissolves into `?` + Retained
        // Drop: a failed later pipeline drops the earlier Retained pipelines automatically.)
        let pipelines = PipelineCollection {
            bg_color:  PIPELINE_DESCS[0].1.init_pipeline(device, &library, pixel_format)?,
            cell_bg:   PIPELINE_DESCS[1].1.init_pipeline(device, &library, pixel_format)?,
            cell_text: PIPELINE_DESCS[2].1.init_pipeline(device, &library, pixel_format)?,
            image:     PIPELINE_DESCS[3].1.init_pipeline(device, &library, pixel_format)?,
            bg_image:  PIPELINE_DESCS[4].1.init_pipeline(device, &library, pixel_format)?,
        };
        // Post-pipelines: on any error, log + fall back to empty (do not block Ghostty).
        let post_pipelines = init_post_pipelines(device, &library, post_shaders, pixel_format)
            .unwrap_or_else(|_err| {
                // log.warn("error initializing postprocess shaders err={}", .{err});
                Vec::new()
            });
        Ok(Self { library, pipelines, post_pipelines, defunct: false })
    }

    /// Ports Shaders.deinit (the double-free guard + per-field release). Zig takes an
    /// Allocator (frees post_pipelines); Rust owns via Vec/Retained Drop, but the `defunct`
    /// guard + explicit method are preserved for parity (generic.zig calls `shaders.deinit`).
    pub fn deinit(&mut self) {
        if self.defunct { return; }
        self.defunct = true;
        // Zig releases each pipeline + library + post_pipelines here; in Rust those are
        // Retained fields whose Drop releases them when `self` drops. The `defunct` flag
        // preserves the idempotent-deinit behavior (double-deinit is a no-op).
    }
}
```

PORT NOTE (ARC): the Zig `errdefer inline for … deinit()` unwinding on partial-init failure
dissolves into `?` early-return + `Retained` Drop — a failed pipeline drops the
already-constructed `Retained` pipelines automatically. The `defunct` bool is kept (generic.zig
asserts on it) even though `Drop` makes double-free structurally impossible.

### 6.3 Library load ([F7], §3 — pin ONE golden path)

Two paths, exactly as ghostty + the blueprint: embedded `.metallib` (via `newLibraryWithData`)
when `build.rs` set `rustty_metallib`, else `newLibraryWithSource` from `include_str!`. **[F7]:
pin the metallib path for generating AND comparing the golden**; the source path is
compile-portability only (the two compilers can differ sub-LSB).

```rust
// metal/shaders.rs
use dispatch2::DispatchData;

/// Ports Shaders.initLibrary. [F7]: the embedded-metallib path is the pinned golden path.
fn init_library(device: &ProtocolObject<dyn MTLLibrary>) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>, Error> {
    #[cfg(rustty_metallib)]
    {
        // ghostty: macos.dispatch.Data.create(@embedFile("ghostty_metallib"), …) →
        //   newLibraryWithData:error:. build.rs emits $OUT_DIR/rustty.metallib.
        const METALLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rustty.metallib"));
        let data = DispatchData::from_bytes(METALLIB);   // dispatch_data_t (copies bytes)
        device.newLibraryWithData_error(&data).map_err(|_ns| Error::MetalFailed)
    }
    #[cfg(not(rustty_metallib))]
    {
        // Fallback ([F7] compile-portability only): compile the SAME MSL at runtime.
        const SRC: &str = include_str!("../shaders/shaders.metal");
        let src = objc2_foundation::NSString::from_str(SRC);
        device.newLibraryWithSource_options_error(&src, None).map_err(|_ns| Error::MetalFailed)
    }
}
```

Verified: `MTLDevice::newLibraryWithData_error(&self, &DispatchData) -> Result<Retained<…>,
Retained<NSError>>` (features `MTLLibrary` + `dispatch2`);
`DispatchData::from_bytes(&[u8]) -> DispatchRetained<DispatchData>` (feature `block2`; derefs to
`&DispatchData`); `newLibraryWithSource_options_error(&NSString, Option<&MTLCompileOptions>) ->
Result<…>` (pass `None`).

### 6.4 `initPostPipelines` / `initPostPipeline` (custom post-shaders → `newLibraryWithSource`)

Ports `shaders.zig:364/402` straight — each custom shader compiles its own source library, then
a `Pipeline::init(None, …)` with `vertex_fn = "full_screen_vertex"`, `fragment_fn = "main0"`,
`vertex_library = library` (the main metallib), `fragment_library = post_library`, blending off.

```rust
fn init_post_pipelines(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    shaders: &[&std::ffi::CStr],
    pixel_format: api::MTLPixelFormat,
) -> Result<Vec<Pipeline>, Error> {
    if shaders.is_empty() { return Ok(Vec::new()); }        // Zig `return &.{}`
    let mut pipelines = Vec::with_capacity(shaders.len());   // Zig alloc.alloc(Pipeline, len)
    for source in shaders {
        pipelines.push(init_post_pipeline(device, library, source, pixel_format)?);
    }
    Ok(pipelines)   // Zig errdefer-on-partial dissolves into `?` + Vec/Retained Drop
}

fn init_post_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    data: &std::ffi::CStr,
    pixel_format: api::MTLPixelFormat,
) -> Result<Pipeline, Error> {
    let src = objc2_foundation::NSString::from_str(&data.to_string_lossy());
    let post_library = device.newLibraryWithSource_options_error(&src, None)
        .map_err(|_ns| Error::MetalFailed)?;
    Pipeline::init(None, PipelineOptions {
        device,
        vertex_fn: "full_screen_vertex",
        fragment_fn: "main0",
        vertex_library: library,
        fragment_library: &post_library,
        step_fn: api::MTLVertexStepFunction::PerVertex,
        attachments: &[Attachment { pixel_format, blending_enabled: false }],
    }).map_err(|_| Error::MetalFailed)
}
```

PORT NOTE: ghostty's `[:0]const u8` sentinel-terminated source → `&CStr` here (the shadertoy
transpile already produces owned NUL-terminated MSL). `NSString::from_str` copies the source;
the NUL sentinel is a Zig-ABI concern, not needed for `NSString`.

---

## §7 — The Chunk-1b compile boundary (what compiles in 1b vs needs Chunk 2/3)

**Compiles + links in Chunk 1b** (gate: `cargo build -p rustty-renderer` on macOS, `-D warnings`):

| Leaf | Depends only on | Status |
|---|---|---|
| `pipeline.rs` (+ VertexLayout tables) | Chunk-1a `api`, `crate::shaders::{CellText,Image,BgImage}`, objc2-metal | ✅ full |
| `render_pass.rs` (Step/Attachment/Draw) | `api`, `pipeline`, `texture`, `sampler`, `target`, objc2-metal | ✅ full |
| `target.rs` (IOSurface + readback) | `api`, objc2-io-surface, objc2-core-foundation | ✅ full |
| `frame.rs` (completion contract) | `api`, `render_pass`, `target`, `crate::Health`, objc2-metal | ✅ full (erased renderer ptr) |
| `shaders.rs` (backend container) | `api`, `pipeline`, `crate::shaders`, dispatch2, objc2-metal | ✅ full (device handed in) |

**Explicitly deferred to Chunk 2/3 (and NOT blocking 1b compile):**
- `frame.rs`: the **concrete completion fn** (`metal_buffer_completed` — casts `*mut ()` →
  `*mut GenericRenderer`, calls `frame_completed`/`api.present`) is Chunk 3. 1b stores it as a
  `CompletionFn` fn-pointer only. The **async `addCompletedHandler`** branch is `unimplemented!`
  (windowing-host seam).
- `target.rs`: `set_color_space` (DisplayP3, needs objc2-core-graphics) — deferred on-screen seam.
- `shaders.rs`: the CALLER of `Shaders::init` (Chunk-2 `Metal.initShaders`, which chooses
  `pixel_format` from `blending`/`colorspace`) — 1b's `Shaders::init` takes `device` +
  `pixel_format` as parameters, so it compiles without `Metal`.
- The `Step`/`Frame`/`RenderPass` **callers** (the `drawFrame` ordered-step loop) are Chunk 3.

**Un-deferred in 1b:** `size_of::<CellText>() == 32` (the lone reference `test{}` in scope) as a
`const{}` assert (§2.3).

**Scratch smoke test (the Chunk-1 gate, extend the landed `metal::tests`):** on
`MTLCreateSystemDefaultDevice()`, build the metallib `Shaders` (or the source fallback), a
`Target::init(4,4,BGRA8Unorm,Shared)`, a `Frame::begin` + a trivial `RenderPass` with one
`bg_color` step + `complete(sync=true)` (with a no-op `CompletionFn`), then `read_bgra()` and
assert the buffer is `4*4*4` bytes — proving the whole plumbing drives real Metal without panic.
Skips cleanly when no device (headless CI), like the landed smoke test.

---

## §8 — Cargo dependency changes (exact)

Add to `[target.'cfg(target_os = "macos")'.dependencies]`:

```toml
# objc2-metal: ADD the Chunk-1b feature set to the existing list —
#   MTLLibrary        -> newLibraryWithData/Source, newFunctionWithName, MTLFunction (shaders + pipeline)
#   dispatch2         -> newLibraryWithData_error (+ pulls dispatch2)
#   block2            -> addCompletedHandler / MTLCommandBufferHandler (async seam)
#   objc2-io-surface  -> newTextureWithDescriptor_iosurface_plane (+ pulls objc2-io-surface)
objc2-metal = { version = "0.3", default-features = false, features = [
    # … existing Chunk-0/1a features …
    "MTLLibrary", "dispatch2", "block2", "objc2-io-surface",
] }

# IOSurface create/lock/readback (Target). Default features already cover these; the explicit
# set keeps the objc2 per-feature style: IOSurfaceRef (create/lock/base_address/keys),
# IOSurfaceTypes (IOSurfaceLockOptions), objc2-core-foundation (CFString keys), libc (lock).
objc2-io-surface = { version = "0.3", features = [
    "IOSurfaceRef", "IOSurfaceTypes", "objc2", "objc2-core-foundation", "libc",
] }

# CoreFoundation: the IOSurface property dictionary (CFDictionary + CFNumber over CFString keys).
objc2-core-foundation = { version = "0.3", features = [
    "CFBase", "CFDictionary", "CFNumber", "CFString",
] }

# DispatchData for newLibraryWithData (metallib bytes → dispatch_data_t). from_bytes needs block2.
dispatch2 = { version = "0.3", features = ["block2"] }

# The addCompletedHandler completion block (RcBlock) — the deferred async present seam.
block2 = { version = "0.6" }
```

Add to `[dev-dependencies]` **in Chunk 4** (NOT 1b): `png` (golden decode/encode, same as the
`rustty-font` sprite gate). **NOT added in 1b (deferred on-screen half):** `objc2-core-graphics`
(the DisplayP3 colorspace-set), `objc2-quartz-core` (CAMetalLayer).

Acyclicity: all additions are external crates.io — no new inter-crate edge; `rustty-renderer` is
already below apprt/core/config and already an FFI crate for G6. `check_acyclic.sh` unaffected.

---

## §9 — OPEN RISKS (things a reviewer must scrutinize)

1. **[HIGHEST] Packed-bool byte-faithfulness — a Chunk-4 golden prerequisite, surfaced by the
   attribute tables.** The vertex tables (§2.2) have **correct offsets**, but the shader reads
   `atlas`+`bools` (CellText) / `info` (BgImage) as **single `packed struct(u8)` bytes**
   (bit0/bit1…). The current `crate::shaders` `CellTextBools`/`BgImageInfo` are **multi-byte
   non-`repr(C)`** structs (e.g. `CellTextBools` = two `bool` bytes at offsets 29,30). The
   attribute at offset 29 reads only byte 29 → `is_cursor_glyph` (bit1, byte 30) is **never seen
   by the GPU** → a golden run would render cursors wrong. `size_of::<CellText>() == 32` still
   holds (trailing padding absorbs the extra byte), so the §2.3 assert passes and this does **not**
   block 1b. **Action:** before the Chunk-4 golden, make `CellTextBools`/`BgImageInfo`/
   `UniformsBools`/`UniformsPaddingExtend` serialize to their `packed struct(u8/u32)` bytes
   (a `#[repr(transparent)] struct(u8)` newtype with bit accessors, or a serializer). This is the
   deferred "GPU-struct byte-faithfulness" the parent (§G, blueprint §4 risk) flagged. Scope it as
   a Chunk-3/4 prerequisite, tracked in DEFERRALS.md. (Also affects `Uniforms` alignment/packing —
   its `align(16)`/`packed(u32)` fields are not yet applied; same fix family.)

2. **Frame completion aliasing (mut-args-strongly-protected) — Miri/ASan required.** On the sync
   path the completion fn holds `*mut GenericRenderer` and re-enters the renderer (`present` +
   `frame_completed`) while the renderer is live inside `drawFrame`. Design uses raw `*mut`
   (never `&mut`) exactly as Zig's `@fieldParentPtr`/container_of, and the erased `*mut ()`
   defers the deref to Chunk 3. I have **not** proven the borrow model end-to-end (Chunk-3
   concern). Requires an ASan pass on the concrete impl (Miri can't run the Metal FFI; R6 routes
   FFI crates to ASan). The 1b `frame.rs` shape is UB-free on its own (it only stores/forwards
   the raw pointer); the risk lands with the Chunk-3 callback body.

3. **`Step<'a>` borrow-threading through the per-frame draw loop (parent OPEN RISK #1).** The
   zero-refcount borrow design is specified but **not compiled** against `image.zig`'s
   per-placement `Buffer<Image>` build/step/drop. If borrow lifetimes prove awkward there, fall
   back to owned `Retained` clones (+1 retain/release per step; behavior-equivalent). Does not
   affect 1b (the loop is Chunk 3).

4. **IOSurface readback coherence off Apple-Silicon.** `read_bgra` assumes `.Shared` storage +
   Apple-Silicon unified memory (the pinned golden runner, [F5]) → CPU-coherent after
   `waitUntilCompleted`. On Intel/`.Managed` a `synchronizeResource` blit would be needed. Out of
   the offscreen scope, but the golden runner pin ([F5]) must hold or the readback silently reads
   stale bytes.

5. **`kCFTypeDictionaryKeyCallBacks`/`ValueCallBacks` extern access.** `CFDictionary::new` wants
   `*const CFDictionaryKeyCallBacks`; the design takes `addr_of!` of the objc2 extern statics.
   Confirmed the statics exist (`objc2_core_foundation::generated::CFDictionary`). One unverified
   nit: whether they are re-exported at crate root vs need `objc2_core_foundation::` path — the
   implementer should confirm the exact import path (both are behind the `CFDictionary` feature).
   Low risk; a compile error is immediate and local.

6. **`Retained`/`CFRetained` return-type mismatch at the FFI seam.** `newTextureWithDescriptor_
   iosurface_plane` wants `iosurface: &IOSurfaceRef`; we hold `CFRetained<IOSurfaceRef>` which
   `Deref`s to `&IOSurfaceRef` — fine. `objc2-metal`'s `objc2-io-surface` feature must resolve
   the SAME `objc2-io-surface` version as our direct dep (Cargo unifies; both `0.3.2` per the
   lock). If a future bump splits them, the `&IOSurfaceRef` types would differ — pin the versions.

7. **`checkError` localizedDescription logging dropped.** ghostty's `Pipeline.checkError` /
   `shaders.checkError` log `nserror.localizedDescription` before returning `error.MetalFailed`.
   The design `.map_err(|_ns| Error::MetalFailed)` **discards** the NSError text. R1 wants the
   log preserved — but no logging facade is wired in the port yet (the landed leaves preserve
   `log.warn` sites as comments). **Action:** preserve the `checkError` log as a comment (matching
   `buffer.rs`'s treatment) OR thread the NSError into `Error` for a future log. Not a blocker;
   note for the completeness critic so the dropped `log.err` is accounted for.
