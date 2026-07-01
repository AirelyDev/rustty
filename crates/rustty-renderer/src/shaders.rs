//! Renderer GPU vertex/uniform data structs.
//!
//! PORTS: the vertex-data struct definitions from ghostty
//! `src/renderer/opengl/shaders.zig` (equivalently `metal/shaders.zig`) — the
//! `CellText`, `CellBg`, and `Uniforms` structs that `cell.zig` fills, plus the
//! `CellText.Atlas` enum.
//!
//! PORT STATUS: vertex-data structs ported (headless). The GPU-backend
//! machinery in the reference `shaders.zig` — the `Pipeline`/`PipelineCollection`
//! /`PipelineDescription` types, the `Shaders` container with `init`/`deinit`,
//! the `Image`/`BgImage` image-pipeline structs, `initPostPipelines`/
//! `initPostPipeline`, and the comptime `loadShaderCode`/`processIncludes`
//! GLSL `@embedFile` include-processing — is the OpenGL/Metal draw path and is
//! DEFERRED to R-d/Phase 3 (the GPU-backend milestone). Only the CPU-side vertex
//! data structs that `cell.zig` assembles are needed by the headless R-a port.

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
