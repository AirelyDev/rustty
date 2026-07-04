# GPU R-d CHUNK 2 — implementable design

**Deliverable:** the exact Rust shape of (A) the `AlphaBlending` config→renderer cycle break, and
(B) the monomorphic offscreen port of `src/renderer/Metal.zig` into `rustty-renderer`.

**Prereqs already landed:** the whole `metal/*` leaf tier (api/buffer/texture/sampler/pipeline/
render_pass/target/frame/shaders — Chunks 0/1a/1b, committed). Decision fixed by
`GPU_RD_TRAIT_DESIGN.md`: port `generic.zig`/Metal **MONOMORPHICALLY** (concrete `Metal` leaf
types, no `GraphicsApi` trait hierarchy).

**Reference:** `/Users/gael/projects/ghostty/src/renderer/Metal.zig` (458 loc, read in full).
**Rules:** `PORT_RULES.md` — R1 (100% fidelity: every line/comment/`test{}`; comments only
retargeted to rustty names), R3 (objc2 replaces pkg/objc+pkg/macos, same Metal.framework), R4
(acyclic, `check_acyclic.sh`), R5 (never weaken a gate).

> **Design only. Do NOT touch `crates/`.** This doc is the spec Chunk 2 implements.

---

## EXECUTIVE SUMMARY (read first, ~12 lines)

1. **Break chosen:** HOIST `AlphaBlending { Native, Linear, LinearCorrected }` + `is_linear()`
   from `rustty-config/value_types.rs` to the `rustty-render-types` leaf. Both config and renderer
   already depend on render-types → no new edge, R4 holds. This is the **exact `PaddingBalance`
   precedent** (`WindowPaddingBalance = pub use rustty_render_types::PaddingBalance`).
2. **config re-exports:** `pub use rustty_render_types::AlphaBlending;` in `value_types.rs` — every
   use-site (`config_struct.rs` field/TypeId/defaults/Key, `deep_equal`, `c_get`) keeps working.
3. **`string_to_enum` resolution:** you cannot add inherent methods (`bare_enum_names!`) to a
   foreign type, so — exactly as `window_padding_balance_{string_to_enum,to_name,to_name_z}` —
   replace the macro with three **config-local free functions** over the Zig variant literals
   (`native`/`linear`/`linear-corrected`) and switch the three call-sites to the free-fn macro arms
   (`field_enum!(f, fn)`, `f_enum!(f, fn)`, `g_enum_fn!(f, fn_z)`). `DeepEqual` moves to render-types
   (orphan rule), like `PaddingBalance`.
4. **Parity gates stay green with ZERO gate edits:** the distinctive `alpha-blending` field
   doc-comments stay at the field in `config_struct.rs`; the only enum-attached ref comment
   (`/// See alpha-blending`, 2 words) is sub-distinctive and G3b-ignored; the Rust-only
   `alpha_blending_is_linear` test **stays in `value_types.rs`** (maps to `Config.zig`, count
   unchanged). Do NOT move the test to render-types (would misattribute vs `renderer_size` need=6).
5. **Offscreen Metal:** the real `Metal::init(alloc, opts)` needs a live `nsview` + `IOSurfaceLayer`
   → it is a `unimplemented!("windowing-host")` seam (verbatim Zig body preserved as a comment). A
   new **`Metal::init_offscreen(blending, width, height)`** does `chooseDevice → newCommandQueue →
   default_storage_mode → max_texture_size` with **no layer / no view**, storing a fixed
   `offscreen_size` that `surface_size()` returns (replacing `layer.bounds`).
6. **Compile boundary:** Chunk 2 does NOT need `renderer::Options` (offscreen ctor takes `blending`
   + size directly) nor `apprt`/`IOSurfaceLayer`. `cargo build -p rustty-renderer` + a smoke test
   (`init_offscreen → init_shaders → init_target → begin_frame`, no panic) is the gate. All objc2
   surface (`MTLCopyAllDevices`, `isHeadless/isRemovable/isLowPower/hasUnifiedMemory`,
   `supportsFamily`, `objc_autoreleasePoolPush/Pop`) is confirmed present at the pinned versions.

---

# PART A — break the config→renderer cycle for `AlphaBlending`

## A.0 Why this is a prerequisite

`Metal.zig` reads `config.blending` and branches on `blending.isLinear()` to pick
`bgra8unorm_srgb` vs `bgra8unorm` (`initShaders`/`initTarget`/`textureOptions`). So the ported
`Metal` struct must carry a `blending` field and call `is_linear()`. But `AlphaBlending` lives in
`rustty-config/src/value_types.rs:597`, and **`rustty-config` depends on `rustty-renderer`**
(`Cargo.toml:48`, the `Message::ChangeConfig` back-edge). If the renderer named
`rustty_config::…::AlphaBlending` the graph would cycle (R4 / G7). Fixing this is CHUNK 2's Part A.

## A.1 The chosen break — HOIST to `rustty-render-types` (the `PaddingBalance` precedent)

`rustty-render-types` is a clean leaf that **both** config (`Cargo.toml:43`) and renderer
(`Cargo.toml:26`) already depend on, and it already depends on `rustty-datastruct`
(for `DeepEqual`). Hoisting `AlphaBlending` there:

- introduces **no new crate edge** → `check_acyclic.sh` unaffected (R4 ✓);
- lets the renderer name `rustty_render_types::AlphaBlending` with no config edge;
- is the **exact** move already done for `PaddingBalance` (`value_types.rs:91`:
  `pub use rustty_render_types::PaddingBalance as WindowPaddingBalance;`) and for `Size`/`Health`
  (the A1 hoist precedent in `.port` memory).

### A.1.1 ADD to `rustty-render-types/src/lib.rs`

Carry the enum **with its port-mirror comments and `is_linear` body verbatim** (R1), plus the
`DeepEqual` impl (orphan rule — must live in the type's home crate, exactly like `PaddingBalance`
at `lib.rs:47`):

```rust
// pub const AlphaBlending = enum { native, linear, "linear-corrected"; pub fn isLinear };
//
// PORT NOTE (A1 hoist): `AlphaBlending` is DEFINED in ghostty `config/Config.zig`
// (`pub const AlphaBlending`), but the renderer (`Metal.zig` initShaders/initTarget/
// textureOptions) branches on `blending.isLinear()` to pick the sRGB pixel format.
// Because `rustty-config` depends on `rustty-renderer` (the Message::ChangeConfig
// back-edge), the renderer cannot name a config type. So the enum + `is_linear` are
// hoisted to this leaf (depended on by BOTH config and renderer) — the same hoist as
// `PaddingBalance`/`Size`/`Health`. `rustty-config` re-exports it so its public path
// (`value_types::AlphaBlending`) and every use-site keep working. Its config-facing
// PARSE/FORMAT (`string_to_enum`/`to_name`) stays a CONFIG concern (free fns there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaBlending {
    Native,
    Linear,
    LinearCorrected,
}

impl AlphaBlending {
    // pub fn isLinear(self: AlphaBlending) bool
    pub fn is_linear(self) -> bool {
        // return switch (self) {
        //     .native => false,
        //     .linear, .@"linear-corrected" => true,
        // };
        match self {
            AlphaBlending::Native => false,
            AlphaBlending::Linear | AlphaBlending::LinearCorrected => true,
        }
    }
}

// `AlphaBlending` is used as the `alpha-blending` config field type. The reference's
// `Config.changed` resolves its `deepEqual` via the `.@"enum"` arm (`old == new`). The
// orphan rule requires this `DeepEqual` impl to live in `AlphaBlending`'s home crate
// (rustty-render-types), mirroring `PaddingBalance` above.
impl rustty_datastruct::comparison::DeepEqual for AlphaBlending {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}
```

## A.2 The `string_to_enum` subtlety — resolved with config-local free functions

`AlphaBlending`'s `string_to_enum`/`to_name`/`to_name_z` are macro-generated **inherent** methods
via `bare_enum_names!(AlphaBlending {...})` (`value_types.rs:1059`). **You cannot add inherent
methods to a foreign (render-types) type.** Two options were weighed:

- **(i)** move the enum+`is_linear` to render-types and keep PARSE/FORMAT as **config-local free
  functions** (a config-local *function* over a foreign type is legal — no orphan issue). ✅ CHOSEN.
- **(ii)** move the string-parsing too, into render-types. ✗ REJECTED — string↔enum config parsing
  is a *config* concern (`std.meta.stringToEnum`/`@tagName` are driven by `parseIntoField`/
  `formatEntry`, both config machinery); render-types is the wrong layer, and it would drag the
  `name_z_array`/C-string plumbing into a pure geometry leaf.

Option (i) is **already the established pattern** in this codebase: `WindowPaddingBalance` (also a
render-types hoist) is parsed/formatted by three free functions in `config_struct.rs`
(`window_padding_balance_{string_to_enum,to_name,to_name_z}`, lines 8643/8698/8748) plus the
`f_enum!(field, fn)` / `field_enum!(field, fn)` / `g_enum_fn!(field, fn_z)` macro arms. The three
foreign-enum macro arms already exist and are exercised.

### A.2.1 ADD three free fns to `config_struct.rs` (beside the `window_padding_balance_*` trio)

Place next to the existing foreign-enum maps (`~8643` parse, `~8698` name, `~8748` name_z). Note
`linear-corrected` is **hyphenated** (the Zig field-name literal), unlike PaddingBalance's
non-hyphenated variants:

```rust
/// `std.meta.stringToEnum(config.AlphaBlending, input)`.
// pub const AlphaBlending = enum { native, linear, "linear-corrected" };
fn alpha_blending_string_to_enum(input: &[u8]) -> Option<AlphaBlending> {
    match input {
        b"native" => Some(AlphaBlending::Native),
        b"linear" => Some(AlphaBlending::Linear),
        b"linear-corrected" => Some(AlphaBlending::LinearCorrected),
        _ => None,
    }
}

/// `@tagName(config.AlphaBlending)`.
// pub const AlphaBlending = enum { native, linear, "linear-corrected" };
fn alpha_blending_to_name(v: AlphaBlending) -> &'static [u8] {
    match v {
        AlphaBlending::Native => b"native",
        AlphaBlending::Linear => b"linear",
        AlphaBlending::LinearCorrected => b"linear-corrected",
    }
}

/// `@tagName(config.AlphaBlending)`, NUL-terminated for the C API.
pub(crate) fn alpha_blending_to_name_z(v: AlphaBlending) -> *const std::os::raw::c_char {
    match v {
        AlphaBlending::Native => c"native".as_ptr(),
        AlphaBlending::Linear => c"linear".as_ptr(),
        AlphaBlending::LinearCorrected => c"linear-corrected".as_ptr(),
    }
}
```

> `AlphaBlending` resolves here via `config_struct.rs:87` `use crate::value_types::*;` (the
> re-export). `pub(crate)` on `_to_name_z` matches `window_padding_balance_to_name_z` so `c_get.rs`
> can import it.

## A.3 Every call-site edit (exhaustive)

| # | File:line (current) | Current | Change to | Why |
|---|---|---|---|---|
| 1 | `render-types/lib.rs` | — | **ADD** enum + `is_linear` + `DeepEqual` (A.1.1) | new home |
| 2 | `config/value_types.rs:595–615` | `pub enum AlphaBlending {...}` + `impl … is_linear` | **REPLACE** with re-export (A.3.1) | hoisted |
| 3 | `config/value_types.rs:1058–1064` | `bare_enum_names!(AlphaBlending {...})` | **DELETE** (can't impl inherent on foreign) | moved to free fns |
| 4 | `config/value_types.rs:8077–8085` | `#[test] fn alpha_blending_is_linear` | **KEEP unchanged** (uses re-export) | gate: count/provenance |
| 5 | `config/config_struct.rs` (~8650/8704/8756) | — | **ADD** the 3 free fns (A.2.1) | parse/format/c-get |
| 6 | `config/config_struct.rs:9137` | `field_enum!(alpha_blending, AlphaBlending::string_to_enum)` | `field_enum!(alpha_blending, alpha_blending_string_to_enum)` | PARSE |
| 7 | `config/config_struct.rs:8167` | `f_enum!(alpha_blending)` | `f_enum!(alpha_blending, alpha_blending_to_name)` | FORMAT |
| 8 | `config/c_get.rs:25–26` | import list | **ADD** `alpha_blending_to_name_z` | c-get import |
| 9 | `config/c_get.rs:246` | `g_enum!(alpha_blending)` | `g_enum_fn!(alpha_blending, alpha_blending_to_name_z)` | C API `@tagName` ptr |
| 10 | `config/deep_equal.rs:100` | `AlphaBlending,` in `deep_equal_via_eq!(…)` | **DELETE** the line (impl now in render-types) | orphan rule |
| 11 | `config/config_struct.rs:597` | `Key::AlphaBlending => TypeId::of::<AlphaBlending>()` | **NO CHANGE** (re-export resolves; `TypeId::of` works on the foreign `'static` type) | — |
| 12 | `config/config_struct.rs:4669–4671` | `AlphaBlending::Native` / `::LinearCorrected` defaults | **NO CHANGE** (variants resolve via re-export) | — |
| 13 | `config/config_struct.rs:137` | `AlphaBlending,` in the `Key` enum | **NO CHANGE** — this is a `Key` *variant name*, unrelated to the type | — |

### A.3.1 The re-export that replaces the definition (`value_types.rs:595–615`)

```rust
// pub const AlphaBlending = enum { native, linear, "linear-corrected"; pub fn isLinear };
//
// PORT NOTE (A1 hoist): DEFINED in ghostty `config/Config.zig`, but the renderer branches
// on `blending.isLinear()` to select the sRGB pixel format (Metal.zig). Because rustty-config
// depends on rustty-renderer, the renderer cannot name a config type; so the enum + `is_linear`
// + `DeepEqual` are hoisted to the rustty-render-types leaf (both config and renderer depend on
// it) — the `PaddingBalance`/`Size`/`Health` precedent. Re-exported here so `value_types::
// AlphaBlending`, the field/TypeId/defaults/Key use-sites, and the `alpha_blending_is_linear`
// test all keep resolving. The config-facing PARSE/FORMAT stays here as free fns in
// `config_struct.rs` (`alpha_blending_{string_to_enum,to_name,to_name_z}`), mirroring
// `window_padding_balance_*` (a config-local free fn over a foreign type is legal).
pub use rustty_render_types::AlphaBlending;
```

## A.4 Parity-gate handling (G1 test counts, G3 comment fidelity, G3b comment coverage)

The subtlety the task calls out: `AlphaBlending`'s def maps to a **`Config.zig`** file in the
gate map (`config_struct` entry: `Config.zig → [config_struct.rs, value_types.rs]`,
`check_test_counts.py:439`), because in ghostty `AlphaBlending` is defined *inside* `Config.zig`
(unlike `PaddingBalance`, defined in `renderer/size.zig`). Precise handling, verified against the
gate sources:

- **G1 / `check_test_counts.py` (config_struct, need=113):** the count is `#[test]` lines in
  `config_struct.rs` + `value_types.rs`, must be ≥113. **Keep `alpha_blending_is_linear` in
  `value_types.rs`** (item #4) — it references the re-exported `AlphaBlending`, compiles fine, and
  keeps the config count unchanged. **Do NOT move it to render-types** (`renderer_size` maps
  `size.zig → render-types/lib.rs` need=6; an extra test there would misattribute a `Config.zig`
  test to `size.zig`). No `check_test_counts.py` edit. `AlphaBlending` has **no reference `test{}`**
  (documented at `value_types.rs:8077`), so there is no reference test to relocate.

- **G3b / `check_comment_coverage.py` (crate-scoped):** it checks every *distinctive* (≥5
  word-tokens) `Config.zig` comment has a fuzzy match **somewhere** in `rustty-config`'s source.
  The distinctive `alpha-blending` comments are the **field doc block** (`Config.zig:378–403`:
  "What color space to use…", the `native`/`linear`/`linear-corrected` bullet descriptions) — those
  live at the **field** in `config_struct.rs:1108–1131` and **stay in config** (the field is not
  hoisted, only the enum type is). The only *enum-attached* reference comment is
  `Config.zig:9596 /// See alpha-blending` — **2 words, below the 5-token distinctive threshold →
  G3b ignores it.** Therefore hoisting the enum drops **zero** distinctive config comment. G3b for
  rustty-config stays green with no edit; G3b for render-types only *gains* covered-by-extra
  comments (adding port comments never fails G3b).

- **G3 / `check_comments_fidelity.py` (aggregate per-crate count):** config's ported comment total
  drops by the ~4 port-mirror lines that move with the enum (`// pub const AlphaBlending…`,
  `// pub fn isLinear…`, `// return switch…`), but the re-export block (A.3.1) re-adds a
  multi-line PORT NOTE in their place, and config's `rust_total` remains far above its
  `Config.zig` `ref_total` (large margin, same as post-`PaddingBalance`). render-types gains the
  mirror comments (only helps). No G3 edit, no weakening (R5 ✓).

- **What exactly STAYS in `value_types.rs`:** the re-export line **with** the `// pub const
  AlphaBlending = enum { … }` Zig-origin mirror comment + the A1-hoist PORT NOTE (A.3.1), and the
  unchanged `alpha_blending_is_linear` test (item #4). The `is_linear` body + its `// return
  switch…` comments move to render-types (A.1.1).

- **No gate-map change** in `check_test_counts.py`/`check_comment_coverage.py`. `AlphaBlending`'s
  provenance stays `Config.zig` (test + field comments in config); render-types merely *holds the
  definition*, exactly as it holds `PaddingBalance`'s definition while `size.zig`'s balance tests
  count it.

---

# PART B — port `Metal.zig` MONOMORPHICALLY (offscreen scope)

New file **`crates/rustty-renderer/src/metal/metal_impl.rs`**, wired into `metal.rs` with
`pub mod metal_impl;` + `pub use metal_impl::Metal;`. (Provenance: ports `src/renderer/Metal.zig`;
the crate-root `pub type Renderer = GenericRenderer<Metal>` alias is Chunk 3.)

Header comment mirrors `Metal.zig:1` `//! Graphics API wrapper for Metal.` and lists the
R3/objc2 substitutions (`pkg/objc`→`objc2`, `pkg/macos.graphics`→deferred, `IOSurfaceLayer`→
windowing-host seam).

## B.0 Imports (the compile surface)

```rust
use std::ffi::CStr;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSArray;                       // MTLCopyAllDevices return
use objc2_metal::{MTLDevice, MTLGPUFamily};          // protocol methods + family consts

use rustty_render_types::AlphaBlending;              // ← Part A hoist (no config edge)

use super::api;                                      // MTLResourceOptions/StorageMode/... (metal/api.rs)
use super::buffer::BufferOptions;
use super::frame::{self, CompletionFn, Frame};
use super::sampler::{Sampler, SamplerOptions};
use super::shaders::{self, Shaders};
use super::target::{self, Target};
use super::texture::{self, Texture, TextureOptions};
// rustty_font::Atlas for init_atlas_texture (rustty-renderer already deps rustty-font).
```

## B.1 Consts + the `Metal` struct (Chunk-2 offscreen shape)

`swap_chain_count`/`custom_shader_target`/`custom_shader_y_is_down` are `pub const` decls on the
Zig `Metal`; monomorphically they become associated consts (Chunk 3's `SwapChain`/custom-shader
paths read them). `SurfaceSize` names the anonymous struct `surfaceSize` returns.

```rust
/// Names the anonymous `struct { width: u32, height: u32 }` that `surfaceSize` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

/// Metal-local error. Ports `Metal.zig`'s `error{NoMetalDevice}` (the only Metal-*originated*
/// error; the resource constructors return their own leaf `Error`s, matching Zig's per-call
/// inferred unions). The aggregate `RendererError` that `generic.zig` maps leaf errors into is a
/// CHUNK-3 concern (`GPU_RD_TRAIT_DESIGN.md` §A shared vocabulary), not introduced here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `Metal.zig` `error{NoMetalDevice}` — `chooseDevice` found no usable MTLDevice.
    NoMetalDevice,
}

/// Graphics API wrapper for Metal. Ports `Metal.zig`'s `Metal = @This()` struct.
pub struct Metal {
    // DEFERRED (windowing-host seam M-1): `layer: IOSurfaceLayer,`
    //   `metal/IOSurfaceLayer.zig` is a windowing-host leaf (CALayer attach / display callback /
    //   setSurface present) not ported in the offscreen scope. The field is omitted from the
    //   Chunk-2 struct; it (and `Metal::init`) land with the windowing-host tier. Offscreen
    //   `surface_size` reads `offscreen_size` (below) and `present` is a stub — neither touches a
    //   layer. The verbatim Zig field decl is preserved in this comment (R1/R5).

    /// MTLDevice
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    /// MTLCommandQueue
    pub queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,

    /// Alpha blending mode.
    // Metal.zig: `blending: configpkg.Config.AlphaBlending`. Now the Part-A hoisted
    // `rustty_render_types::AlphaBlending` (no config edge). Drives sRGB pixel-format selection
    // in init_shaders/init_target/texture_options via `blending.is_linear()`.
    pub blending: AlphaBlending,

    /// The default storage mode to use for resources created with our device.
    ///
    /// This is based on whether the device is a discrete GPU or not, since
    /// discrete GPUs do not have unified memory and therefore do not support
    /// the "shared" storage mode, instead we have to use the "managed" mode.
    pub default_storage_mode: api::StorageMode,

    /// The maximum 2D texture width and height supported by the device.
    pub max_texture_size: u32,

    /// We start an AutoreleasePool before `drawFrame` and end it afterwards.
    // Metal.zig: `autorelease_pool: ?*objc.AutoreleasePool = null`. `objc.AutoreleasePool` wraps
    // `objc_autoreleasePoolPush()`/`Pop()`; objc2 exposes those same two runtime fns as
    // `objc2::ffi::objc_autoreleasePoolPush() -> *mut c_void` / `objc_autoreleasePoolPop(ptr)`
    // (confirmed public, objc2 0.6.4). The token is the pushed pool pointer.
    autorelease_pool: Option<*mut core::ffi::c_void>,

    /// OFFSCREEN seam M-3: the fixed surface size (replaces `layer.bounds` in `surfaceSize`).
    /// Set by `init_offscreen`; returned (clamped to `max_texture_size`) by `surface_size`.
    offscreen_size: SurfaceSize,
}

impl Metal {
    // Metal.zig: `pub const custom_shader_target: shadertoy.Target = .msl;`
    pub const CUSTOM_SHADER_TARGET: crate::shadertoy::Target = crate::shadertoy::Target::Msl;
    // The fragCoord for Metal shaders is +Y = down.
    pub const CUSTOM_SHADER_Y_IS_DOWN: bool = true;
    /// Triple buffering.
    pub const SWAP_CHAIN_COUNT: usize = 3;
}
```

> **`self: Metal` by-value → `&self`:** `Metal.zig`'s option factories take `self: Metal` (a struct
> copy that shares ObjC refs with no retain). `Metal` holds `Retained` (not `Copy`) in Rust, so
> every read-only method takes `&self` and `.clone()`s the device into the owned-device option
> structs (a refcount bump on a short-lived Options — behavior-equivalent). `*const Metal` →
> `&self`, `*Metal` → `&mut self`. Note as a PORT NOTE in the file.

## B.2 Live offscreen construction (`chooseDevice` + `init_offscreen`)

`chooseDevice` is fully offscreen-portable. `MTLCopyAllDevices` is re-exported bare
(`api.rs:297`, [ADV-FIX F1]) returning `Retained<NSArray<ProtocolObject<dyn MTLDevice>>>`. The Zig
`devices.iterate()` maps to indexed `count()`/`objectAtIndex()` (both available under the current
`NSArray` feature, each returning `Retained<ProtocolObject<dyn MTLDevice>>` — **no Cargo change**;
the `.iter()` form needs the `NSEnumerator` feature, see COMPILE BOUNDARY):

```rust
// fn chooseDevice() error{NoMetalDevice}!objc.Object
fn choose_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>, Error> {
    let mut chosen_device: Option<Retained<ProtocolObject<dyn MTLDevice>>> = None;

    // .macos arm (this module is `#[cfg(target_os = "macos")]`). The Zig `.ios` arm
    // (`MTLCreateSystemDefaultDevice`) and `else => @compileError` are preserved as comments:
    //   .ios  => chosen_device = MTLCreateSystemDefaultDevice();
    //   else  => @compileError("unsupported target for Metal");
    let devices = api::MTLCopyAllDevices();          // Retained<NSArray<...>>
    let n = devices.count();
    for i in 0..n {
        let device = devices.objectAtIndex(i);       // Retained<ProtocolObject<dyn MTLDevice>>
        // We want a GPU that's connected to a display.
        if device.isHeadless() {
            continue;
        }
        chosen_device = Some(device.clone());
        // If the user has an eGPU plugged in, they probably want to use it. Otherwise,
        // integrated GPUs are better for battery life and thermals.
        if device.isRemovable() || device.isLowPower() {
            break;
        }
    }

    // Zig `orelse return error.NoMetalDevice; return device.retain();` — the `Retained` already
    // owns the +1 (objc2 ARC), so returning it *is* the `retain()`; no explicit retain needed.
    chosen_device.ok_or(Error::NoMetalDevice)
}

/// Determines the maximum 2D texture size supported by the device.
/// We need to clamp our frame size to this if it's larger.
// fn queryMaxTextureSize(device: objc.Object) u32
fn query_max_texture_size(device: &ProtocolObject<dyn MTLDevice>) -> u32 {
    // https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf
    if device.supportsFamily(MTLGPUFamily::Apple10) {
        return 32768;
    }
    if device.supportsFamily(MTLGPUFamily::Apple3) {
        return 16384;
    }
    8192
}

impl Metal {
    /// OFFSCREEN constructor (CHUNK-2 golden/smoke entry point). Runs the offscreen-portable
    /// half of `Metal.zig`'s `init`: chooseDevice + newCommandQueue + device metadata, WITHOUT
    /// the NSView/IOSurfaceLayer half (windowing-host seam M-2). Takes `blending` + the fixed
    /// offscreen size directly, so CHUNK 2 needs neither `renderer::Options` nor `apprt`.
    pub fn init_offscreen(
        blending: AlphaBlending,
        width: u32,
        height: u32,
    ) -> Result<Metal, Error> {
        // Choose our MTLDevice and create a MTLCommandQueue for that device.
        let device = choose_device()?;
        // errdefer device.release() — ARC via Retained Drop on the early return below.
        let queue = device.newCommandQueue().ok_or(Error::NoMetalDevice)?;
        // (Zig `newCommandQueue` cannot fail in practice; the `?*anyopaque` null maps to None.)

        // Grab metadata about the device.
        //   .ios => .shared;  else => hasUnifiedMemory ? .shared : .managed
        let default_storage_mode = if device.hasUnifiedMemory() {
            api::StorageMode::Shared
        } else {
            api::StorageMode::Managed
        };
        let max_texture_size = query_max_texture_size(&device);
        // log.debug("device properties default_storage_mode={} max_texture_size={}", ...)

        Ok(Metal {
            device,
            queue,
            blending,
            default_storage_mode,
            max_texture_size,
            autorelease_pool: None,
            offscreen_size: SurfaceSize { width, height },
        })
    }

    /// DEFERRED (windowing-host seam M-2): the real `Metal.zig` `init(alloc, opts)`.
    ///
    /// Needs `opts.rt_surface.platform.nsview` (a live NSView) + `IOSurfaceLayer` + the
    /// wantsLayer/addSublayer/clipsToBounds/contentsScale/needsDisplayOnBoundsChange attach.
    /// None of that is offscreen-portable; it lands with the windowing-host tier. The verbatim
    /// Zig body (Metal.zig:64–156) is preserved here as a block comment (R1/R5).
    // pub fn init(alloc: Allocator, opts: rendererpkg.Options) !Metal { … view attach … }
    pub fn init(/* alloc, opts */) -> Result<Metal, Error> {
        unimplemented!("windowing-host: Metal::init needs a live NSView + IOSurfaceLayer (offscreen path is init_offscreen)")
    }
}
```

## B.3 `deinit` (Drop), the AutoreleasePool, and the deferred hooks

```rust
impl Drop for Metal {
    // pub fn deinit(self: *Metal) void { self.queue.release(); self.device.release();
    //                                    self.layer.release(); }
    fn drop(&mut self) {
        // ARC: `queue`/`device` (Retained) release automatically on drop; the `layer` release is
        // part of seam M-1. Release order is immaterial under ARC. If a pool is still pushed here
        // it would leak — but drawFrameStart/End are balanced (matching Zig, whose deinit also
        // does not pop the pool). No explicit body required.
    }
}

impl Metal {
    /// Actions taken before doing anything in `drawFrame`. Right now we use this to start an
    /// AutoreleasePool.
    // pub fn drawFrameStart(self: *Metal) void
    pub fn draw_frame_start(&mut self) {
        rustty_util::quirks::inlineAssert(self.autorelease_pool.is_none()); // Metal.zig: assert(...==null)
        // Zig `self.autorelease_pool = .init()` == objc_autoreleasePoolPush().
        self.autorelease_pool = Some(unsafe { objc2::ffi::objc_autoreleasePoolPush() });
    }

    /// Actions taken after `drawFrame` is done. Right now we use this to end our AutoreleasePool.
    // pub fn drawFrameEnd(self: *Metal) void
    pub fn draw_frame_end(&mut self) {
        rustty_util::quirks::inlineAssert(self.autorelease_pool.is_some()); // Metal.zig: assert(...!=null)
        // Zig `self.autorelease_pool.?.deinit(); self.autorelease_pool = null;` == pop.
        let pool = self.autorelease_pool.take().unwrap();
        unsafe { objc2::ffi::objc_autoreleasePoolPop(pool) };
    }

    /// DEFERRED (windowing-host seam M-4). Metal is the ONLY backend that declares `loopEnter`
    /// (the other 7 `@hasDecl` hooks are no-ops); it installs the layer display callback.
    // pub fn loopEnter(self: *Metal) void {
    //     const renderer: *align(1) Renderer = @fieldParentPtr("api", self);
    //     self.layer.setDisplayCallback(@ptrCast(&displayCallback), @ptrCast(renderer)); }
    // fn displayCallback(renderer: *Renderer) align(8) void {
    //     renderer.drawFrame(true) catch |err| { log.warn(...) }; }
    pub fn loop_enter(&mut self) {
        unimplemented!("windowing-host: Metal::loop_enter installs the IOSurfaceLayer display callback")
    }
}
```

> The other 7 apprt/thread hooks (`surfaceInit`/`finalizeSurfaceInit`/`threadEnter`/`threadExit`/
> `loopExit`/`displayRealized`/`displayUnrealized`) are **not declared on `Metal.zig`** → in the
> monomorphic port they simply do not exist as methods; `generic.zig`'s `@hasDecl` probes for them
> are a Chunk-3 concern (they resolve to the no-op default at the call site).

## B.4 Resource-option factories (§C — six buffer aliases + texture/sampler/image)

`Metal.zig`'s six `pub const xBufferOptions = bufferOptions;` cannot be method aliases in Rust
(`GPU_RD_TRAIT_DESIGN.md` §C: "Rust can't alias methods by `pub const`") → **six one-line
forwarders** around one private body. All owned-device options `.clone()` the device.

```rust
impl Metal {
    /// Returns the options to use when constructing buffers.
    // pub inline fn bufferOptions(self: Metal) bufferpkg.Options
    fn buffer_options(&self) -> BufferOptions {
        BufferOptions {
            device: self.device.clone(),
            resource_options: api::MTLResourceOptions {
                // Indicate that the CPU writes to this resource but never reads it.
                cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                storage_mode: self.default_storage_mode,
                hazard_tracking_mode: api::HazardTrackingMode::Default, // Zig field-default `.default`
            },
        }
    }
    // The six Zig `pub const … = bufferOptions;` aliases → six forwarders (PORT NOTE: no method
    // aliasing in Rust). Names MUST match what generic.zig/image.zig call.
    pub fn instance_buffer_options(&self) -> BufferOptions { self.buffer_options() }
    pub fn uniform_buffer_options(&self) -> BufferOptions { self.buffer_options() }
    pub fn fg_buffer_options(&self) -> BufferOptions { self.buffer_options() }
    pub fn bg_buffer_options(&self) -> BufferOptions { self.buffer_options() }
    pub fn image_buffer_options(&self) -> BufferOptions { self.buffer_options() }
    pub fn bg_image_buffer_options(&self) -> BufferOptions { self.buffer_options() }

    /// Returns the options to use when constructing textures.
    // pub inline fn textureOptions(self: Metal) Texture.Options
    pub fn texture_options(&self) -> TextureOptions {
        TextureOptions {
            device: self.device.clone(),
            // Using an `*_srgb` pixel format makes Metal gamma encode the pixels written to it
            // *after* blending, which means we get linear alpha blending rather than
            // gamma-incorrect blending.
            pixel_format: if self.blending.is_linear() {
                api::MTLPixelFormat::BGRA8Unorm_sRGB
            } else {
                api::MTLPixelFormat::BGRA8Unorm
            },
            resource_options: api::MTLResourceOptions {
                cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                storage_mode: self.default_storage_mode,
                hazard_tracking_mode: api::HazardTrackingMode::Default,
            },
            usage: api::MTLTextureUsage {
                // textureOptions is currently only used for custom shaders, which require both the
                // shader read (chaining) and render target (final output) usage. Disabling either
                // leads to metal validation errors in Xcode.
                shader_read: true,
                render_target: true,
                ..api::MTLTextureUsage::default()
            },
        }
    }

    // pub inline fn samplerOptions(self: Metal) Sampler.Options
    pub fn sampler_options(&self) -> SamplerOptions {
        SamplerOptions {
            device: self.device.clone(),
            // These parameters match Shadertoy behaviors.
            min_filter: api::MTLSamplerMinMagFilter::Linear,
            mag_filter: api::MTLSamplerMinMagFilter::Linear,
            s_address_mode: api::MTLSamplerAddressMode::ClampToEdge,
            t_address_mode: api::MTLSamplerAddressMode::ClampToEdge,
        }
    }
}
```

### B.4.1 `ImageTextureFormat` + `image_texture_options`

The Metal-local enum with `toPixelFormat`. Monomorphically it lives in `metal::` (the image.zig
port — a R-d prerequisite — will call `metal::Metal::image_texture_options(.rgba, true)`).

```rust
/// Pixel format for image texture options.
// pub const ImageTextureFormat = enum { gray, rgba, bgra };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageTextureFormat {
    /// 1 byte per pixel grayscale.
    Gray,
    /// 4 bytes per pixel RGBA.
    Rgba,
    /// 4 bytes per pixel BGRA.
    Bgra,
}

impl ImageTextureFormat {
    // fn toPixelFormat(self, srgb: bool) mtl.MTLPixelFormat
    fn to_pixel_format(self, srgb: bool) -> api::MTLPixelFormat {
        use api::MTLPixelFormat as F;
        match self {
            ImageTextureFormat::Gray => if srgb { F::R8Unorm_sRGB } else { F::R8Unorm },
            ImageTextureFormat::Rgba => if srgb { F::RGBA8Unorm_sRGB } else { F::RGBA8Unorm },
            ImageTextureFormat::Bgra => if srgb { F::BGRA8Unorm_sRGB } else { F::BGRA8Unorm },
        }
    }
}

impl Metal {
    /// Returns the options to use when constructing textures for images.
    // pub inline fn imageTextureOptions(self, format, srgb) Texture.Options
    pub fn image_texture_options(&self, format: ImageTextureFormat, srgb: bool) -> TextureOptions {
        TextureOptions {
            device: self.device.clone(),
            pixel_format: format.to_pixel_format(srgb),
            resource_options: api::MTLResourceOptions {
                cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                storage_mode: self.default_storage_mode,
                hazard_tracking_mode: api::HazardTrackingMode::Default,
            },
            usage: api::MTLTextureUsage {
                // We only need to read from this texture from a shader.
                shader_read: true,
                ..api::MTLTextureUsage::default()
            },
        }
    }
}
```

## B.5 Resource constructors driven through the api

```rust
impl Metal {
    /// Initializes a Texture suitable for the provided font atlas.
    // pub fn initAtlasTexture(self: *const Metal, atlas: *const font.Atlas) Texture.Error!Texture
    pub fn init_atlas_texture(
        &self,
        atlas: &rustty_font::Atlas,
    ) -> Result<Texture, texture::Error> {
        let pixel_format = match atlas.format {
            rustty_font::atlas::Format::Grayscale => api::MTLPixelFormat::R8Unorm,
            rustty_font::atlas::Format::Bgra => api::MTLPixelFormat::BGRA8Unorm_sRGB,
            // Zig `else => @panic("unsupported atlas format for Metal texture")` — catches `Bgr`.
            _ => panic!("unsupported atlas format for Metal texture"),
        };
        Texture::init(
            TextureOptions {
                device: self.device.clone(),
                pixel_format,
                resource_options: api::MTLResourceOptions {
                    cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                    storage_mode: self.default_storage_mode,
                    hazard_tracking_mode: api::HazardTrackingMode::Default,
                },
                usage: api::MTLTextureUsage {
                    // We only need to read from this texture from a shader.
                    shader_read: true,
                    ..api::MTLTextureUsage::default()
                },
            },
            atlas.size,
            atlas.size,
            None,
        )
    }

    /// Initialize a new render target which can be presented by this API.
    // pub fn initTarget(self: *const Metal, width: usize, height: usize) !Target
    pub fn init_target(&self, width: usize, height: usize) -> Result<Target, target::Error> {
        Target::init(target::Options {
            device: &self.device, // deref-coerces &Retained<..> → &ProtocolObject<..> (struct-field site)
            // Using an `*_srgb` pixel format makes Metal gamma encode the pixels written to it
            // *after* blending, which means we get linear alpha blending rather than
            // gamma-incorrect blending.
            pixel_format: if self.blending.is_linear() {
                api::MTLPixelFormat::BGRA8Unorm_sRGB
            } else {
                api::MTLPixelFormat::BGRA8Unorm
            },
            storage_mode: self.default_storage_mode,
            width,
            height,
        })
    }

    /// (initShaders) uses `blending.is_linear()` → sRGB pixel format for the shader pipelines.
    // pub fn initShaders(self: *const Metal, alloc, custom_shaders: []const [:0]const u8) !Shaders
    //
    // PORT NOTE: the reference `alloc` param is ELIDED — the already-landed `Shaders::init` leaf
    // (`metal/shaders.rs`) does not thread an Allocator (objc2/Vec-managed), matching how the leaf
    // was ported in Chunk 1b. `custom_shaders: []const [:0]const u8` → `&[&CStr]`.
    pub fn init_shaders(&self, custom_shaders: &[&CStr]) -> Result<Shaders, shaders::Error> {
        Shaders::init(
            &self.device,
            custom_shaders,
            // Using an `*_srgb` pixel format makes Metal gamma encode the pixels written to it
            // *after* blending, which means we get linear alpha blending rather than
            // gamma-incorrect blending.
            if self.blending.is_linear() {
                api::MTLPixelFormat::BGRA8Unorm_sRGB
            } else {
                api::MTLPixelFormat::BGRA8Unorm
            },
        )
    }
}
```

## B.6 Frame lifecycle (`surface_size`, `begin_frame`, `present`, `present_last_target`)

```rust
impl Metal {
    /// Get the current size of the runtime surface.
    // pub fn surfaceSize(self: *const Metal) !struct { width: u32, height: u32 }
    //
    // OFFSCREEN seam M-3: the reference reads `layer.bounds * contentsScale`. Offscreen there is
    // no layer, so we return the fixed `offscreen_size` set at construction — still clamped to
    // `max_texture_size` (the reference's clamp: we can't create a screen texture larger). The
    // deferred layer-bounds path (needs `pkg/macos.graphics.Rect`) is the windowing-host seam;
    // the verbatim Zig body (Metal.zig:216–232) is preserved as a comment.
    pub fn surface_size(&self) -> Result<SurfaceSize, Error> {
        Ok(SurfaceSize {
            width: self.offscreen_size.width.min(self.max_texture_size),
            height: self.offscreen_size.height.min(self.max_texture_size),
        })
    }

    /// Begin a frame.
    // pub inline fn beginFrame(self: *const Metal, renderer: *Renderer, target: *Target) !Frame
    //
    // PORT NOTE: the reference `renderer: *Renderer` (= `GenericRenderer(Metal)`) is not ported in
    // Chunk 2; `Frame::begin` (landed 1b) takes the type-erased `renderer: *mut ()` PLUS an
    // `on_completed: CompletionFn` (the type-erased `&bufferCompleted`, which Chunk-3 `drawFrame`
    // supplies). So `begin_frame` forwards both. Raw `*mut` (NOT `&mut`) per the container_of /
    // mut-args-strongly-protected aliasing model ([FIX-2]).
    pub fn begin_frame(
        &self,
        renderer: *mut (),
        target: *mut Target,
        on_completed: CompletionFn,
    ) -> Result<Frame, frame::Error> {
        Frame::begin(frame::Options { queue: &self.queue }, renderer, target, on_completed)
    }

    /// Present the provided target.
    // pub inline fn present(self: *Metal, target: Target, sync: bool) !void
    //
    // OFFSCREEN seam M-5: the reference calls `layer.setSurfaceSync(target.surface)` /
    // `layer.setSurface(...)`. Offscreen has no layer; the IOSurface is read back via
    // `Target::read_bgra` (landed 1b) instead, so `present` is a no-op stub offscreen. The Chunk-3
    // offscreen completion path reads back rather than presenting, so this is never exercised in
    // the offscreen golden. Verbatim Zig body preserved in the comment.
    pub fn present(&mut self, _target: &Target, _sync: bool) -> Result<(), Error> {
        // if (sync) self.layer.setSurfaceSync(target.surface)
        // else      try self.layer.setSurface(target.surface);
        Ok(())
    }

    /// Present the last presented target again. (noop for Metal)
    // pub inline fn presentLastTarget(self: *Metal) !void { _ = self; }
    pub fn present_last_target(&self) -> Result<(), Error> {
        Ok(())
    }
}
```

## B.7 Deferred-seam inventory (each an R5-preserved `unimplemented!("windowing-host")` or stub)

| Seam | Item | Zig source | Chunk-2 disposition |
|---|---|---|---|
| M-1 | `layer: IOSurfaceLayer` field | `Metal.zig:41` | field **omitted**, verbatim decl in comment (offscreen never reads a layer) |
| M-2 | `init(alloc, opts)` NSView-attach half | `Metal.zig:64–156` | `unimplemented!("windowing-host")`, verbatim body in comment; offscreen ctor = `init_offscreen` |
| M-3 | `surfaceSize` layer-bounds read | `Metal.zig:216–232` | returns fixed `offscreen_size` clamped; verbatim body in comment |
| M-4 | `loopEnter` + `displayCallback` | `Metal.zig:164–176` | `unimplemented!("windowing-host")`, verbatim bodies in comment |
| M-5 | `present` layer setSurface | `Metal.zig:253–259` | no-op stub (readback via `Target::read_bgra`); verbatim body in comment |

Everything else in `Metal.zig` (chooseDevice, newCommandQueue, default_storage_mode,
queryMaxTextureSize, the 3 consts, all option factories + 6 forwarders, ImageTextureFormat +
toPixelFormat, initAtlasTexture, initTarget, initShaders, drawFrameStart/End, beginFrame,
presentLastTarget) is **ported live** in Part B.

## B.8 The Chunk-2 smoke test (gate: `cargo test -p rustty-renderer`)

Mirror the landed `metal.rs` `draw_plumbing_smoke`, but drive through `Metal` (not the raw leaves).
Skips cleanly on a host with no Metal device (headless CI). PORT NOTE: rustty-only, not a ghostty
parity test (there is no reference `test{}` over `Metal.zig`).

```rust
#[test]
fn metal_offscreen_construct_and_begin_frame() {
    // Headless CI may lack a Metal device.
    let metal = match Metal::init_offscreen(AlphaBlending::Native, 4, 4) {
        Ok(m) => m,
        Err(Error::NoMetalDevice) => { eprintln!("no Metal device; skipping"); return; }
    };
    assert_eq!(metal.default_storage_mode, api::StorageMode::Shared); // Apple Silicon
    assert!(metal.max_texture_size >= 8192);
    assert_eq!(metal.surface_size().unwrap(), SurfaceSize { width: 4, height: 4 });

    // initShaders(load pipelines) → initTarget → beginFrame → trivial pass → complete(sync).
    let shaders = metal.init_shaders(&[]).expect("init_shaders");
    assert!(shaders.post_pipelines.is_empty());
    let mut target = metal.init_target(4, 4).expect("init_target");

    unsafe fn noop(_c: &frame::CompletionContext, _b: &ProtocolObject<dyn MTLCommandBuffer>) {}
    let tp: *mut Target = &mut target;
    let mut frame = metal.begin_frame(core::ptr::null_mut(), tp, noop).expect("begin_frame");
    // (clear pass identical to draw_plumbing_smoke) … frame.complete(true);
    let px = target.read_bgra();
    assert_eq!(px.len(), 4 * 4 * 4);
}
```

---

## COMPILE BOUNDARY

**Part A (config / render-types):**
- `render-types` gains `AlphaBlending` + `is_linear` + `DeepEqual` (uses its existing
  `rustty-datastruct` dep). No new edge.
- `config` re-exports it and swaps 3 macro call-sites to free fns. No new edge. `check_acyclic.sh`
  unchanged; **Part A is a strict prerequisite of Part B** (Metal's `blending` field type).

**Part B (renderer):**
- New file `metal/metal_impl.rs` behind the existing `#[cfg(target_os = "macos")] pub mod metal;`.
- Names only: `crate::metal::api::*`, the landed leaves (`buffer/texture/sampler/target/frame/
  shaders`), `rustty_render_types::AlphaBlending`, `rustty_font::Atlas`, `crate::shadertoy::Target`,
  and `objc2`/`objc2-metal`/`objc2-foundation`. **All present at pinned versions** (objc2 0.6.4:
  `ffi::objc_autoreleasePoolPush/Pop`; objc2-metal 0.3.2: `MTLCopyAllDevices`, `MTLDevice::{
  isHeadless,isRemovable,isLowPower,hasUnifiedMemory,newCommandQueue,supportsFamily}`,
  `MTLGPUFamily::{Apple10=1010,Apple3=1003}`, `MTLPixelFormat::{BGRA8Unorm,BGRA8Unorm_sRGB,
  RGBA8Unorm,RGBA8Unorm_sRGB,R8Unorm,R8Unorm_sRGB}`; objc2-foundation 0.3.2: `NSArray::{count,
  objectAtIndex}` under the already-enabled `NSArray` feature).
- **Chunk 2 does NOT need `renderer::Options`** — `init_offscreen(blending, w, h)` takes the two
  needed values directly (the view-related fields of `Options` are seam M-2). Nor `apprt`, nor
  `IOSurfaceLayer`. `RendererError` unification is Chunk-3 (leaf/Metal errors used directly now).
- **Cargo:** zero required edits (indexed `count()`/`objectAtIndex()` avoids the `NSEnumerator`
  feature). *Optional*: add `"NSEnumerator"` to `objc2-foundation` features to use `devices.iter()`
  for a more literal `iterate()` mapping — a one-line, behavior-identical choice.
- **Gate:** `cargo build -p rustty-renderer` green + `metal_offscreen_construct_and_begin_frame`
  passes (or skips cleanly headless). This unblocks Chunk 3 (`generic.zig` `SwapChain`/`drawFrame`)
  and Chunk 4 (offscreen golden), which drive `Metal` via its `pub` fields + `init_*` methods.

---

## OPEN RISKS

1. **`NSArray` iteration Item type.** `objectAtIndex(i)` returns `Retained<ProtocolObject<dyn
   MTLDevice>>` (confirmed) — the `choose_device` loop above is correct with **no** Cargo change.
   The `.iter()` alternative yields the same `Retained` but needs the `NSEnumerator` feature.
   Low risk; both paths verified in objc2-foundation 0.3.2 source.

2. **`self.device.clone()` refcount churn in option factories.** Zig copies `Metal` by value into
   the factories (sharing ObjC refs with no retain); Rust `&self` + `Retained::clone` bumps the
   refcount per Options build. Options are short-lived (constructed then consumed by a leaf `init`
   that stores its own `Retained`), so this is behavior-equivalent — flag for the Goal-2 churn pass
   only. (Same class as `GPU_RD_TRAIT_DESIGN.md` OPEN RISK #1.)

3. **AutoreleasePool via raw `objc2::ffi`.** objc2's *preferred* idiom is the scoped
   `autoreleasepool(|_| …)` closure, which cannot span the two calls `drawFrameStart`/`drawFrameEnd`.
   The field-stored `objc_autoreleasePoolPush/Pop` pair is the faithful map of Zig's
   `objc.AutoreleasePool` (which wraps those exact runtime fns) and is `unsafe`. Balance
   (push in start, pop in end) is asserted via `quirks::inlineAssert`, matching Zig. Not exercised until
   Chunk-3 `drawFrame`; validate under Miri/ASan is N/A (FFI) — covered by the Chunk-3 draw smoke.

4. **`begin_frame` raw-pointer aliasing.** `*mut ()` renderer + `*mut Target` stashed in the Frame
   completion context (the container_of trampoline). The block holds `*mut Renderer` while the
   renderer is mid-`drawFrame` — must stay raw `*mut` (not `&mut`), per `mut-args-strongly-
   protected` / [FIX-2]. Chunk 2 only passes a **null** renderer + no-op completion fn, so the
   aliasing is not exercised until Chunk 3; the null-ptr smoke is safe.

5. **`init_shaders` `alloc` elision.** The reference `initShaders(alloc, …)` param is dropped
   because the landed `Shaders::init` leaf takes no Allocator. If a later chunk reintroduces
   Allocator threading into the renderer (`GenericRenderer.alloc`), re-add `_alloc` for signature
   fidelity; today it is elided consistently with the leaf. No behavior lost.

6. **`ImageTextureFormat` placement.** Put in `metal::` (monomorphic; the image.zig port — a R-d
   prerequisite not yet landed — will call `Metal::image_texture_options`). Confirm the exact home
   when image.zig is ported (`GPU_RD_TRAIT_DESIGN.md` OPEN RISK #5). It compiles standalone in
   Chunk 2 regardless.

7. **iOS arms.** `Metal.zig`'s `.ios` branches (default_storage_mode `.shared`, chooseDevice
   `MTLCreateSystemDefaultDevice`, the sublayer attach) and the `else => @compileError` are
   preserved as verbatim comments but are unreachable — the module is `#[cfg(target_os = "macos")]`,
   which is exactly how ghostty's comptime `switch(builtin.os.tag)` selects the macOS arm. Not a
   loss (R1: dead comptime arms preserved as comments).
