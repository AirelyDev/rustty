//! This file contains the definitions of the Metal API that we use.
//!
//! Because the online Apple developer docs have recently (as of January 2025)
//! been changed to hide enum values, `Metal-cpp` has been used as a reference
//! source instead.
//!
//! Ref: https://developer.apple.com/metal/cpp/
//
// PORT NOTE (R3/objc2): ghostty hand-declares every Metal enum/extern-struct/
// extern-fn in `metal/api.zig` against Metal.framework. rustty replaces the
// ad-hoc `extern`/`enum` declarations with the `objc2-metal` crate's bindings
// to the SAME framework — the values are the identical Apple constants, so the
// port is a re-export of objc2's types under ghostty's names (mirroring how
// `rustty-font-shaper` replaced `pkg/macos` CoreText with `objc2-core-text`).
// objc2 models each Apple `NS_ENUM` as a newtype `struct Foo(pub NSUInteger)`
// with associated `const`s, and each `NS_OPTIONS` as a `bitflags` type; the
// numeric values match ghostty's `enum(c_ulong)`/`packed struct(c_ulong)`
// declarations one-for-one. The per-enum `/// https://developer.apple.com/...`
// doc-URL comments below are preserved verbatim from `api.zig` (R1).
//
// The two `packed struct(c_ulong)` bit-field types (`MTLResourceOptions` +
// `MTLTextureUsage`) do NOT have a 1:1 objc2 shape — objc2 exposes them as
// BITFLAGS, not ghostty's struct-of-fields ([ADV-FIX F3]). They are ported as
// ghostty-shaped structs (every field + comment preserved) with a `resolve()`
// that composes the objc2 bitflag value (R3: bit-identical to Apple's).

/// https://developer.apple.com/documentation/metal/mtlcommandbufferstatus?language=objc
//
// api.zig: `enum(c_ulong)` { not_enqueued=0, enqueued=1, committed=2,
// scheduled=3, completed=4, @"error"=5, _ }. objc2: associated consts
// `NotEnqueued=0 … Error=5` on `MTLCommandBufferStatus(pub NSUInteger)` (the
// trailing `_` = a non-exhaustive open enum, which the NSUInteger newtype
// models natively).
pub use objc2_metal::MTLCommandBufferStatus;

/// https://developer.apple.com/documentation/metal/mtlloadaction?language=objc
//
// api.zig: { dont_care=0, load=1, clear=2 }.
pub use objc2_metal::MTLLoadAction;

/// https://developer.apple.com/documentation/metal/mtlstoreaction?language=objc
//
// api.zig: { dont_care=0, store=1, multisample_resolve=2,
// store_and_multisample_resolve=3, unknown=4, custom_sample_depth_store=5 }.
pub use objc2_metal::MTLStoreAction;

/// https://developer.apple.com/documentation/metal/mtlresourceoptions?language=objc
//
// PORT NOTE [ADV-FIX F3]: `api.zig` models `MTLResourceOptions` as a
// `packed struct(c_ulong)` of bit-fields; objc2-metal exposes it as BITFLAGS
// (`MTLResourceOptions(pub NSUInteger)` with pre-shifted constants
// `CPUCacheMode…`/`StorageMode…`/`HazardTrackingMode…`). We keep ghostty's
// field-shaped struct so the `cpu_cache_mode`/`storage_mode`/
// `hazard_tracking_mode` layout + the nested `CPUCacheMode`/`StorageMode`/
// `HazardTrackingMode` enum semantics + comments are preserved verbatim, and
// compose the objc2 bitflag value via [`MTLResourceOptions::resolve`]. R3: the
// composed value is bit-identical to Apple's (objc2's constants are the same
// Apple constants, at the same CPUCacheMode/StorageMode/HazardTrackingMode bit
// shifts as ghostty's `packed struct` field order). ghostty's high-bit `_pad`
// (`@bitSizeOf(c_ulong) - 10` bits) is implicitly zero in the composed value.
//
// The nested `CPUCacheMode`/`StorageMode`/`HazardTrackingMode` enums are
// hoisted to module scope (Rust has no in-struct type decl usable in field
// position — the `UniformsBools`/`CellTextBools` hoist precedent in
// `shaders.rs`); they remain `MTLResourceOptions`'s logical members.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MTLResourceOptions {
    /// https://developer.apple.com/documentation/metal/mtlcpucachemode?language=objc
    pub cpu_cache_mode: CPUCacheMode,
    /// https://developer.apple.com/documentation/metal/mtlstoragemode?language=objc
    pub storage_mode: StorageMode,
    /// https://developer.apple.com/documentation/metal/mtlhazardtrackingmode?language=objc
    pub hazard_tracking_mode: HazardTrackingMode,
}

impl MTLResourceOptions {
    /// Construct with the given `storage_mode`, defaulting `cpu_cache_mode` and
    /// `hazard_tracking_mode` (mirrors `api.zig`, where `storage_mode` has no
    /// field default but the other two default to `.default`).
    pub const fn new(storage_mode: StorageMode) -> Self {
        Self {
            cpu_cache_mode: CPUCacheMode::Default,
            storage_mode,
            hazard_tracking_mode: HazardTrackingMode::Default,
        }
    }

    /// Compose the ghostty-shaped fields into objc2's `MTLResourceOptions`
    /// bitflag value ([ADV-FIX F3]). Each nested-enum selection maps to objc2's
    /// pre-shifted `MTLResource*` constant; OR-ing them reproduces ghostty's
    /// packed-struct bit layout exactly (R3).
    pub fn resolve(self) -> objc2_metal::MTLResourceOptions {
        use objc2_metal::MTLResourceOptions as O;
        let cpu = match self.cpu_cache_mode {
            CPUCacheMode::Default => O::CPUCacheModeDefaultCache,
            CPUCacheMode::WriteCombined => O::CPUCacheModeWriteCombined,
        };
        let storage = match self.storage_mode {
            StorageMode::Shared => O::StorageModeShared,
            StorageMode::Managed => O::StorageModeManaged,
            StorageMode::Private => O::StorageModePrivate,
            StorageMode::Memoryless => O::StorageModeMemoryless,
        };
        let hazard = match self.hazard_tracking_mode {
            HazardTrackingMode::Default => O::HazardTrackingModeDefault,
            HazardTrackingMode::Untracked => O::HazardTrackingModeUntracked,
            HazardTrackingMode::Tracked => O::HazardTrackingModeTracked,
        };
        O(cpu.0 | storage.0 | hazard.0)
    }
}

/// The `MTLResourceOptions.CPUCacheMode` nested enum (`enum(u4)`). Its bit
/// width lives in objc2's pre-shifted `CPUCacheMode…` bitflag constants, so the
/// Rust enum is a plain logical selector (see [`MTLResourceOptions::resolve`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CPUCacheMode {
    #[default]
    Default = 0,
    WriteCombined = 1,
}

/// The `MTLResourceOptions.StorageMode` nested enum (`enum(u4)`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StorageMode {
    Shared = 0,
    Managed = 1,
    Private = 2,
    Memoryless = 3,
}

/// The `MTLResourceOptions.HazardTrackingMode` nested enum (`enum(u2)`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HazardTrackingMode {
    #[default]
    Default = 0,
    Untracked = 1,
    Tracked = 2,
}

/// https://developer.apple.com/documentation/metal/mtlprimitivetype?language=objc
//
// api.zig: { point=0, line=1, line_strip=2, triangle=3, triangle_strip=4 }.
pub use objc2_metal::MTLPrimitiveType;

/// https://developer.apple.com/documentation/metal/mtlindextype?language=objc
//
// api.zig: { uint16=0, uint32=1 }.
pub use objc2_metal::MTLIndexType;

/// https://developer.apple.com/documentation/metal/mtlvertexformat?language=objc
//
// api.zig: the full `MTLVertexFormat` enum (invalid=0 … floatrgb9e5=55). objc2
// provides every variant at the identical value (e.g. `Float2=29`,
// `UChar4Normalized_BGRA=42`, `FloatRG11B10=54`).
pub use objc2_metal::MTLVertexFormat;

/// https://developer.apple.com/documentation/metal/mtlvertexstepfunction?language=objc
//
// api.zig: { constant=0, per_vertex=1, per_instance=2, per_patch=3,
// per_patch_control_point=4 }.
pub use objc2_metal::MTLVertexStepFunction;

/// https://developer.apple.com/documentation/metal/mtlpixelformat?language=objc
//
// api.zig: the full `MTLPixelFormat` enum (invalid=0 … x24_stencil8=262). objc2
// provides every variant at the identical Apple value (e.g. `BGRA8Unorm=80`,
// `RGBA8Unorm=70`, `Depth32Float=252`).
pub use objc2_metal::MTLPixelFormat;

/// https://developer.apple.com/documentation/metal/mtlpurgeablestate?language=objc
//
// api.zig: { keep_current=1, non_volatile=2, @"volatile"=3, empty=4 }.
pub use objc2_metal::MTLPurgeableState;

/// https://developer.apple.com/documentation/metal/mtlsamplerminmagfilter?language=objc
//
// api.zig: { nearest=0, linear=1 }.
pub use objc2_metal::MTLSamplerMinMagFilter;

/// https://developer.apple.com/documentation/metal/mtlsampleraddressmode?language=objc
//
// api.zig: { clamp_to_edge=0, mirror_clamp_to_edge=1, repeat=2, mirror_repeat=3,
// clamp_to_zero=4, clamp_to_border_color=5 }.
pub use objc2_metal::MTLSamplerAddressMode;

/// https://developer.apple.com/documentation/metal/mtlblendfactor?language=objc
//
// api.zig: { zero=0 … one_minus_source_1_alpha=18 }. objc2 spells the
// destination variants `Destination*` (ghostty: `dest_*`) and the dual-source
// variants `Source1*` (ghostty: `source_1_*`) — same values.
pub use objc2_metal::MTLBlendFactor;

/// https://developer.apple.com/documentation/metal/mtlblendoperation?language=objc
//
// api.zig: { add=0, subtract=1, reverse_subtract=2, min=3, max=4 }.
pub use objc2_metal::MTLBlendOperation;

/// https://developer.apple.com/documentation/metal/mtltextureusage?language=objc
//
// PORT NOTE [ADV-FIX F3]: `api.zig` models `MTLTextureUsage` as a
// `packed struct(c_ulong)` of `bool` bit-fields; objc2-metal exposes it as
// BITFLAGS (`MTLTextureUsage(pub NSUInteger)`: `ShaderRead=1`, `ShaderWrite=2`,
// `RenderTarget=4`, `PixelFormatView=0x10`, `ShaderAtomic=0x20`,
// `Unknown=0`). We keep ghostty's field-shaped struct — every field, its
// value comment, and the reserved-bit comments preserved verbatim — and compose
// the objc2 bitflag via [`MTLTextureUsage::resolve`]. R3: values are Apple-
// identical.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MTLTextureUsage {
    /// https://developer.apple.com/documentation/metal/mtltextureusage/shaderread?language=objc
    pub shader_read: bool, // TextureUsageShaderRead = 1,

    /// https://developer.apple.com/documentation/metal/mtltextureusage/shaderwrite?language=objc
    pub shader_write: bool, // TextureUsageShaderWrite = 2,

    /// https://developer.apple.com/documentation/metal/mtltextureusage/rendertarget?language=objc
    pub render_target: bool, // TextureUsageRenderTarget = 4,

    // _reserved: u1 = 0, // The enum skips from 4 to 16, 8 has no documented use.
    /// https://developer.apple.com/documentation/metal/mtltextureusage/pixelformatview?language=objc
    pub pixel_format_view: bool, // TextureUsagePixelFormatView = 16,

    /// https://developer.apple.com/documentation/metal/mtltextureusage/shaderatomic?language=objc
    pub shader_atomic: bool, // TextureUsageShaderAtomic = 32,
                             // __reserved: @Type(... @bitSizeOf(c_ulong) - 6 ...) = 0,
}

impl MTLTextureUsage {
    /// https://developer.apple.com/documentation/metal/mtltextureusage/unknown?language=objc
    pub const UNKNOWN: MTLTextureUsage = MTLTextureUsage {
        shader_read: false,
        shader_write: false,
        render_target: false,
        pixel_format_view: false,
        shader_atomic: false,
    }; // TextureUsageUnknown = 0,

    /// Compose the ghostty-shaped `bool` fields into objc2's `MTLTextureUsage`
    /// bitflag value ([ADV-FIX F3]); each set field OR-s in its objc2 constant.
    pub fn resolve(self) -> objc2_metal::MTLTextureUsage {
        use objc2_metal::MTLTextureUsage as U;
        let mut bits = U::Unknown.0;
        if self.shader_read {
            bits |= U::ShaderRead.0;
        }
        if self.shader_write {
            bits |= U::ShaderWrite.0;
        }
        if self.render_target {
            bits |= U::RenderTarget.0;
        }
        if self.pixel_format_view {
            bits |= U::PixelFormatView.0;
        }
        if self.shader_atomic {
            bits |= U::ShaderAtomic.0;
        }
        U(bits)
    }
}

/// https://developer.apple.com/documentation/metal/mtlbarrierscope?language=objc
//
// api.zig: { buffers=1, textures=2, render_targets=4 }. objc2 models it as
// bitflags `MTLBarrierScope` (Buffers=1<<0, Textures=1<<1, RenderTargets=1<<2)
// — same values.
pub use objc2_metal::MTLBarrierScope;

/// https://developer.apple.com/documentation/metal/mtlrenderstages?language=objc
//
// api.zig names the type `MTLRenderStage` (singular); objc2 names it
// `MTLRenderStages` (plural, matching the doc-URL) — same values
// (Vertex=1, Fragment=2, Tile=4, Object=8, Mesh=16). Re-exported under
// ghostty's singular name.
pub use objc2_metal::MTLRenderStages as MTLRenderStage;

/// https://developer.apple.com/documentation/metal/mtlgpufamily?language=objc
//
// api.zig: { apple1=1001 … apple10=1010, common1=3001 … common3=3003,
// metal3=5001, metal4=5002 }. objc2 provides every variant at the identical
// value (backed by `NSInteger`, matching ghostty's `enum(c_long)`).
pub use objc2_metal::MTLGPUFamily;

// PORT NOTE (extern structs): `api.zig` declares `MTLClearColor`/`MTLViewport`/
// `MTLRegion`/`MTLOrigin`/`MTLSize` as `extern struct`s (C ABI). objc2-metal
// defines the identical `#[repr(C)]` structs, re-exported here (R3: identical
// layout). objc2 names `MTLViewport`'s first two fields `originX`/`originY`
// where `api.zig` uses `x`/`y` — same `f64`/`f64` layout.
pub use objc2_metal::{MTLClearColor, MTLOrigin, MTLRegion, MTLSize, MTLViewport};

/// https://developer.apple.com/documentation/metal/1433367-mtlcopyalldevices
//
// api.zig: `pub extern "c" fn MTLCopyAllDevices() ?*anyopaque;`. objc2-metal
// exposes it directly ([ADV-FIX F1] — no `declare_extern` workaround needed),
// returning `Retained<NSArray<ProtocolObject<dyn MTLDevice>>>`.
pub use objc2_metal::MTLCopyAllDevices;

/// https://developer.apple.com/documentation/metal/1433401-mtlcreatesystemdefaultdevice
//
// api.zig: `pub extern "c" fn MTLCreateSystemDefaultDevice() ?*anyopaque;`.
// objc2-metal exposes it directly, returning
// `Option<Retained<ProtocolObject<dyn MTLDevice>>>` (the `?*anyopaque` null is
// the `None`).
pub use objc2_metal::MTLCreateSystemDefaultDevice;
