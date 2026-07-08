//! Renderer GPU vertex/uniform data structs.
//!
//! PORTS: the vertex-data struct definitions from ghostty
//! `src/renderer/opengl/shaders.zig` (equivalently `metal/shaders.zig`) — the
//! `CellText`, `CellBg`, and `Uniforms` structs that `cell.zig` fills, plus the
//! `CellText.Atlas` enum.
//!
//! PORT STATUS: vertex-data structs ported (headless), plus the `Image`/
//! `BgImage` image-pipeline vertex structs (GPU R-d Chunk 0 — the pixel-
//! deciding per-placement/bg-image POD vertices, ported from `metal/shaders.zig`
//! beside `CellText`). The remaining GPU-backend machinery in the reference
//! `shaders.zig` — the `Pipeline`/`PipelineCollection`/`PipelineDescription`
//! types, the `Shaders` container with `init`/`deinit`, `initPostPipelines`/
//! `initPostPipeline`, and the comptime `loadShaderCode`/`processIncludes`
//! GLSL `@embedFile` include-processing — is the OpenGL/Metal draw path and is
//! DEFERRED to R-d/Phase 3 (the GPU-backend milestone). Only the CPU-side vertex
//! data structs that `cell.zig` assembles are needed by the headless R-a port.

// ⚠ METAL-vs-OpenGL ABI CLARIFICATION (GPU_RD_CHUNK4_DESIGN §2.0 FLAG). The PORT
// NOTEs below describe the `Bools`/`PaddingExtend`/`Info` packed sub-fields as
// `packed struct(u32)`. That is the **OpenGL** (`opengl/shaders.zig`) layout, where
// `bools`/`padding_extend` are each a 4-byte `u32`. The MONOMORPHIC port targets
// **Metal**, whose `metal/shaders.zig` (and the `.metal` shaders that read it) has
// `padding_extend: packed struct(u8)` (a SINGLE byte) and `bools: extern struct {
// 4× bool align(1) }` (FOUR separate bytes, NOT a u32); `CellText.bools` is a
// `packed struct(u8)`. The exact Metal byte image these structs serialize to is
// applied by the backend serializer `crate::metal::gpu_abi` (`UniformsGpu` /
// `CellTextGpu` / `BgImageGpu` + their `From` impls + drift guards + byte test) —
// see that module's PORT NOTE. The "u32" phrasing in the notes below is retained as
// the OpenGL-source description; the MONOMORPHIC (Metal) byte layout is the u8/
// 4-separate-bytes form in `gpu_abi`. Do NOT read the "u32" notes as the Metal ABI.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const assert = @import("../../quirks.zig").inlineAssert;
// const math = @import("../../math.zig");
use rustty_util::math;

/// The uniforms that are passed to our shaders.
//
// PORT NOTE: the reference is an `extern struct` whose fields carry explicit
// `align(...)` annotations tuned for the GPU uniform-buffer layout, and whose
// `bools`/`padding_extend` sub-fields are `packed struct(u32)` bitfields. The
// exact GPU byte layout (alignment + bit packing) is applied by the R-d/Phase 3
// GPU backend when this is serialized into a uniform buffer; this headless port
// preserves every field, its order, and its element types with `#[repr(C)]`, and
// models the two `packed struct(u32)` types as named-bool structs (their
// `_padding` bits are reconstructed by the backend serializer). No field or
// semantic is lost.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Uniforms {
    /// The projection matrix for turning world coordinates to normalized.
    /// This is calculated based on the size of the screen.
    pub projection_matrix: math::Mat,

    /// Size of the screen (render target) in pixels.
    pub screen_size: [f32; 2],

    /// Size of a single cell in pixels, unscaled.
    pub cell_size: [f32; 2],

    /// Size of the grid in columns and rows.
    pub grid_size: [u16; 2],

    /// The padding around the terminal grid in pixels. In order:
    /// top, right, bottom, left.
    pub grid_padding: [f32; 4],

    /// Bit mask defining which directions to
    /// extend cell colors in to the padding.
    /// Order, LSB first: left, right, up, down
    pub padding_extend: UniformsPaddingExtend,

    /// The minimum contrast ratio for text. The contrast ratio is calculated
    /// according to the WCAG 2.0 spec.
    pub min_contrast: f32,

    /// The cursor position and color.
    pub cursor_pos: [u16; 2],
    pub cursor_color: [u8; 4],

    /// The background color for the whole surface.
    pub bg_color: [u8; 4],

    /// Various booleans, in a packed struct for space efficiency.
    pub bools: UniformsBools,
}

// const Bools = packed struct(u32) { ... };
//
// PORT NOTE: hoisted out of `Uniforms` to a crate-level `UniformsBools` (Rust has
// no in-struct nested type declaration usable in field position). Reference is a
// `packed struct(u32)`; see the `Uniforms` PORT NOTE for the deferred packing.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct UniformsBools {
    /// Whether the cursor is 2 cells wide.
    pub cursor_wide: bool,

    /// Indicates that colors provided to the shader are already in
    /// the P3 color space, so they don't need to be converted from
    /// sRGB.
    pub use_display_p3: bool,

    /// Indicates that the color attachments for the shaders have
    /// an `*_srgb` pixel format, which means the shaders need to
    /// output linear RGB colors rather than gamma encoded colors,
    /// since blending will be performed in linear space and then
    /// Metal itself will re-encode the colors for storage.
    pub use_linear_blending: bool,

    /// Enables a weight correction step that makes text rendered
    /// with linear alpha blending have a similar apparent weight
    /// (thickness) to gamma-incorrect blending.
    pub use_linear_correction: bool,
    // _padding: u28 = 0,
}

// const PaddingExtend = packed struct(u32) { ... };
//
// PORT NOTE: hoisted to a crate-level `UniformsPaddingExtend`; see the `Uniforms`
// PORT NOTE for the deferred packed(u32) layout.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct UniformsPaddingExtend {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    // _padding: u28 = 0,
}

/// This is a single parameter for the terminal cell shader.
//
// PORT NOTE: the reference is an `extern struct` with per-field `align(...)` for
// the GPU per-instance vertex layout, and an inline `bools: packed struct(u8)`.
// As with `Uniforms`, the exact vertex byte layout is a GPU-backend concern
// deferred to R-d/Phase 3; this headless port preserves every field/type/default
// with `#[repr(C)]` and models the packed bool byte as the named-bool
// `CellTextBools` (its `_padding` bits reconstructed by the backend serializer).
// The reference defaults are reproduced by `Default` on the defaulted fields;
// `grid_pos`, `color`, and `atlas` have no default in the reference and must be
// supplied by the caller.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellText {
    pub glyph_pos: [u32; 2],
    pub glyph_size: [u32; 2],
    pub bearings: [i16; 2],
    pub grid_pos: [u16; 2],
    pub color: [u8; 4],
    pub atlas: Atlas,
    pub bools: CellTextBools,
}

/// The atlas backing a `CellText` glyph.
// pub const Atlas = enum(u8) { grayscale = 0, color = 1 };
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Atlas {
    Grayscale = 0,
    Color = 1,
}

// bools: packed struct(u8) { no_min_contrast, is_cursor_glyph, _padding: u6 }
//
// PORT NOTE: hoisted to `CellTextBools`; the reference `packed struct(u8)` byte
// packing is a deferred GPU-serialization concern (see `CellText` PORT NOTE).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CellTextBools {
    pub no_min_contrast: bool,
    pub is_cursor_glyph: bool,
    // _padding: u6 = 0,
}

// PORT NOTE: `CellText` provides `Default` for the defaulted-in-reference fields
// so callers can write `CellText { grid_pos, color, atlas, ..Default::default() }`
// mirroring the reference struct-literal that omits the defaulted fields. The
// non-defaulted `grid_pos`/`color`/`atlas` get placeholder values here that a
// caller is expected to override (the reference has no default for them).
impl Default for CellText {
    fn default() -> Self {
        CellText {
            glyph_pos: [0, 0],
            glyph_size: [0, 0],
            bearings: [0, 0],
            grid_pos: [0, 0],
            color: [0, 0, 0, 0],
            atlas: Atlas::Grayscale,
            bools: CellTextBools::default(),
        }
    }
}

// PORT NOTE (deferred): the reference keeps a commented-out size test with this
// struct. It is preserved verbatim as a comment; the `@sizeOf(CellText) == 32`
// invariant belongs to the GPU vertex layout applied by the R-d/Phase 3 backend.
//
//     test {
//         // Minimizing the size of this struct is important,
//         // so we test it in order to be aware of any changes.
//         try std.testing.expectEqual(32, @sizeOf(CellText));
//     }

/// This is a single parameter for the cell bg shader.
// pub const CellBg = [4]u8;
pub type CellBg = [u8; 4];

/// Single parameter for the image shader. See shader for field details.
//
// PORT NOTE: ported from `metal/shaders.zig`'s `Image` (the image-pipeline
// per-placement vertex struct — R-d/Phase 3 GPU-backend prerequisite). The
// reference is an `extern struct` of `f32` fields with no `align(...)`
// overrides (natural 4-byte float alignment); `#[repr(C)]` reproduces the
// vertex layout. The `f32` fields preclude deriving `Eq` (unlike `CellText`).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Image {
    pub grid_pos: [f32; 2],
    pub cell_offset: [f32; 2],
    pub source_rect: [f32; 4],
    pub dest_size: [f32; 2],
}

/// Single parameter for the bg image shader.
//
// PORT NOTE: ported from `metal/shaders.zig`'s `BgImage`. As with `CellText`,
// the reference is an `extern struct` with per-field `align(...)` (`opacity:
// f32 align(4)`, `info: Info align(1)`) and an inline `info: packed struct(u8)`.
// The exact GPU vertex byte layout (the `align`s + the bit packing of `Info`
// into a single byte) is applied by the R-d/Phase 3 GPU backend; this headless
// port preserves every field/type with `#[repr(C)]` and models the packed
// `Info` byte as the named `BgImageInfo` (its `_padding` bit reconstructed by
// the backend serializer). `opacity: f32` precludes deriving `Eq`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BgImage {
    pub opacity: f32,      // align(4) in the reference
    pub info: BgImageInfo, // align(1) in the reference
}

// pub const Info = packed struct(u8) {
//     position: Position, fit: Fit, repeat: bool, _padding: u1 = 0,
// };
//
// PORT NOTE: hoisted to `BgImageInfo` (Rust has no in-struct type declaration
// usable in field position — the `CellTextBools`/`Atlas` hoist precedent). The
// reference `packed struct(u8)` byte packing (position:u4, fit:u2, repeat:u1,
// _padding:u1) is a deferred GPU-serialization concern (see the `BgImage` PORT
// NOTE); this preserves the fields faithfully, the `_padding` bit reconstructed
// by the backend serializer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BgImageInfo {
    pub position: BgImageInfoPosition,
    pub fit: BgImageInfoFit,
    pub repeat: bool,
    // _padding: u1 = 0,
}

/// The `BgImage.Info.Position` nested enum.
// pub const Position = enum(u4) {
//     tl = 0, tc = 1, tr = 2, ml = 3, mc = 4, mr = 5, bl = 6, bc = 7, br = 8,
// };
//
// PORT NOTE: hoisted like `Atlas`; `enum(u4)` -> `#[repr(u8)]` (Rust has no
// `u4`; the 4-bit width is part of the deferred packed-`Info` layout).
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BgImageInfoPosition {
    Tl = 0,
    Tc = 1,
    Tr = 2,
    Ml = 3,
    Mc = 4,
    Mr = 5,
    Bl = 6,
    Bc = 7,
    Br = 8,
}

/// The `BgImage.Info.Fit` nested enum.
// pub const Fit = enum(u2) {
//     contain = 0, cover = 1, stretch = 2, none = 3,
// };
//
// PORT NOTE: hoisted like `Atlas`; `enum(u2)` -> `#[repr(u8)]`.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BgImageInfoFit {
    Contain = 0,
    Cover = 1,
    Stretch = 2,
    None = 3,
}
