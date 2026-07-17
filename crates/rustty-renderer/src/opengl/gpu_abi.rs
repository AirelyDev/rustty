//! The exact OpenGL-ABI byte images of the vertex/uniform structs.
//!
//! PORTS: ghostty `src/renderer/opengl/shaders.zig:163-300` — the `Uniforms`,
//! `CellText`, `CellBg`, `Image` and `BgImage` structs that the GLSL shaders read.
//! These are the OpenGL twins of `crate::metal::gpu_abi`, ported **FRESH** from the
//! OpenGL reference (NOT copied from `metal::gpu_abi`) because the two graphics
//! APIs' byte layouts genuinely differ — the plan's Global-Constraints note flags
//! this explicitly.
//!
//! PORT NOTE (⚠ OpenGL-vs-Metal ABI — the discrepancy the design flagged):
//! `opengl/shaders.zig` declares `Uniforms.bools` and `Uniforms.padding_extend` as
//! `packed struct(u32)` — each a **4-byte** `u32` (`shaders.zig:184-230`). The
//! **Metal** twin (`metal/shaders.zig`) instead uses `padding_extend: packed
//! struct(u8)` (a single byte) and `bools: extern struct { 4× bool align(1) }`
//! (four separate bytes). So this module targets the OpenGL layout: `padding_extend`
//! is a 4-byte `u32` at offset 112 (`left` bit0, `right` bit1, `up` bit2, `down`
//! bit3) and `bools` is a 4-byte `u32` at offset 132 (`cursor_wide` bit0,
//! `use_display_p3` bit1, `use_linear_blending` bit2, `use_linear_correction`
//! bit3). The Uniforms total size (144) coincides with Metal's, but the byte image
//! of the bool region differs — do NOT copy `metal::gpu_abi`.
//!
//! PORT NOTE (explicit padding): Zig's `extern struct` per-field `align(...)` gaps
//! (`projection_matrix`/`grid_padding` `align(16)`) are reproduced with explicit
//! `_pad*` fields plus `#[repr(C, align(16))]`. `const{}` drift guards pin every
//! offset/size to the reference `@sizeOf`/`@offsetOf`, and a runtime
//! `gpu_abi_struct_sizes` test mirrors the reference's lone
//! `test { expectEqual(32, @sizeOf(CellText)) }` (`shaders.zig:252-256`).

/// PORTS: `opengl/shaders.zig:260` — `pub const CellBg = [4]u8;`.
pub type CellBg = [u8; 4];

/// PORTS: `opengl/shaders.zig:163-231` — the `Uniforms` `extern struct`. 144 bytes,
/// align 16. `padding_extend`/`bools` are 4-byte `u32`s (the OpenGL packing).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct Uniforms {
    /// The projection matrix (`math.Mat` = `[4]F32x4` = 64 bytes, `align(16)`).
    pub projection_matrix: [[f32; 4]; 4], // 0
    /// Size of the screen (render target) in pixels.
    pub screen_size: [f32; 2], // 64
    /// Size of a single cell in pixels, unscaled.
    pub cell_size: [f32; 2], // 72
    /// Size of the grid in columns and rows.
    pub grid_size: [u16; 2], // 80
    /// Padding to `grid_padding`'s `align(16)` offset.
    pub _pad0: [u8; 12], // 84
    /// Padding around the terminal grid in pixels: top, right, bottom, left.
    pub grid_padding: [f32; 4], // 96
    /// `packed struct(u32)`: left bit0, right bit1, up bit2, down bit3.
    pub padding_extend: u32, // 112
    /// The minimum contrast ratio for text (WCAG 2.0).
    pub min_contrast: f32, // 116
    /// The cursor position.
    pub cursor_pos: [u16; 2], // 120
    /// The cursor color.
    pub cursor_color: [u8; 4], // 124
    /// The background color for the whole surface.
    pub bg_color: [u8; 4], // 128
    /// `packed struct(u32)`: cursor_wide bit0, use_display_p3 bit1,
    /// use_linear_blending bit2, use_linear_correction bit3.
    pub bools: u32, // 132
                    // align(16) ⇒ size 144 (tail pad 136..144)
}

/// PORTS: `opengl/shaders.zig:234-257` — a single per-instance `cell_text` vertex.
/// 32 bytes. `atlas`/`bools` are the packed enum/bit bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CellText {
    pub glyph_pos: [u32; 2],  // 0
    pub glyph_size: [u32; 2], // 8
    pub bearings: [i16; 2],   // 16
    pub grid_pos: [u16; 2],   // 20
    pub color: [u8; 4],       // 24
    /// `Atlas` `enum(u8)`: grayscale=0, color=1.
    pub atlas: u8, // 28
    /// `packed struct(u8)`: no_min_contrast bit0, is_cursor_glyph bit1.
    pub bools: u8, // 29
                              // align 4 ⇒ size 32 (tail pad 30..32)
}

/// PORTS: `opengl/shaders.zig:263-268` — a single `image` placement vertex. 48
/// bytes, align 16 (`source_rect: [4]f32 align(16)` bumps the struct alignment,
/// which the **Metal** twin's `Image` does NOT — hence a distinct byte image).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct Image {
    pub grid_pos: [f32; 2],    // 0
    pub cell_offset: [f32; 2], // 8
    pub source_rect: [f32; 4], // 16
    pub dest_size: [f32; 2],   // 32
                               // align(16) ⇒ size 48 (tail pad 40..48)
}

/// PORTS: `opengl/shaders.zig:271-300` — a single `bg_image` vertex. 8 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BgImage {
    pub opacity: f32, // 0
    /// `Info` `packed struct(u8)`: position bits0-3, fit bits4-5, repeat bit6.
    pub info: u8, // 4
                      // align 4 ⇒ size 8 (tail pad 5..8)
}

// ---------------------------------------------------------------------------
// Ergonomic (`crate::shaders`) -> OpenGL-ABI serializers.
//
// PARALLEL: the OpenGL twins of `metal::gpu_abi`'s `From` impls
// (metal/gpu_abi.rs:58-177). The FIELD copies are identical; the BIT PACKING is
// the discrepancy the design flagged — `Uniforms.padding_extend`/`Uniforms.bools`
// are 4-byte `u32` bitfields here (`opengl/shaders.zig:200-231` `packed
// struct(u32)`), where the Metal twin uses a 1-byte `padding_extend` and four
// separate bool bytes. Bit ORDER is LSB-first, matching the Zig `packed struct`
// field declaration order in BOTH APIs (verified against `opengl/shaders.zig:
// 200-231`: `cursor_wide,use_display_p3,use_linear_blending,use_linear_correction`
// and `left,right,up,down`). `CellText.bools`/`BgImage.info` are single bytes,
// byte-identical to the Metal twin.
// ---------------------------------------------------------------------------

impl From<&crate::shaders::Uniforms> for Uniforms {
    fn from(u: &crate::shaders::Uniforms) -> Self {
        let pe = &u.padding_extend; // UniformsPaddingExtend { left,right,up,down }
        let b = &u.bools; // UniformsBools { cursor_wide, use_display_p3, .. }
        Uniforms {
            projection_matrix: u.projection_matrix,
            screen_size: u.screen_size,
            cell_size: u.cell_size,
            grid_size: u.grid_size,
            _pad0: [0; 12],
            grid_padding: u.grid_padding,
            // packed struct(u32): left bit0, right bit1, up bit2, down bit3.
            padding_extend: (pe.left as u32)
                | ((pe.right as u32) << 1)
                | ((pe.up as u32) << 2)
                | ((pe.down as u32) << 3),
            min_contrast: u.min_contrast,
            cursor_pos: u.cursor_pos,
            cursor_color: u.cursor_color,
            bg_color: u.bg_color,
            // packed struct(u32): cursor_wide bit0, use_display_p3 bit1,
            // use_linear_blending bit2, use_linear_correction bit3.
            bools: (b.cursor_wide as u32)
                | ((b.use_display_p3 as u32) << 1)
                | ((b.use_linear_blending as u32) << 2)
                | ((b.use_linear_correction as u32) << 3),
        }
    }
}

impl From<&crate::shaders::CellText> for CellText {
    fn from(c: &crate::shaders::CellText) -> Self {
        CellText {
            glyph_pos: c.glyph_pos,
            glyph_size: c.glyph_size,
            bearings: c.bearings,
            grid_pos: c.grid_pos,
            color: c.color,
            atlas: c.atlas as u8, // repr(u8): Grayscale=0, Color=1
            // packed struct(u8): no_min_contrast bit0, is_cursor_glyph bit1.
            bools: (c.bools.no_min_contrast as u8) | ((c.bools.is_cursor_glyph as u8) << 1),
        }
    }
}

impl From<&crate::shaders::BgImage> for BgImage {
    fn from(b: &crate::shaders::BgImage) -> Self {
        let i = &b.info; // BgImageInfo { position: enum(u4), fit: enum(u2), repeat }
        BgImage {
            opacity: b.opacity,
            // packed struct(u8): position bits0-3, fit bits4-5, repeat bit6.
            info: ((i.position as u8) & 0x0F)
                | (((i.fit as u8) & 0x03) << 4)
                | ((i.repeat as u8) << 6),
        }
    }
}

// Compile-time drift guards: fail the build if any offset/size drifts from the
// OpenGL reference `@offsetOf`/`@sizeOf`.
const _: () = {
    // Uniforms (shaders.zig:163-231).
    assert!(core::mem::size_of::<Uniforms>() == 144);
    assert!(core::mem::align_of::<Uniforms>() == 16);
    assert!(core::mem::offset_of!(Uniforms, projection_matrix) == 0);
    assert!(core::mem::offset_of!(Uniforms, screen_size) == 64);
    assert!(core::mem::offset_of!(Uniforms, cell_size) == 72);
    assert!(core::mem::offset_of!(Uniforms, grid_size) == 80);
    assert!(core::mem::offset_of!(Uniforms, grid_padding) == 96);
    assert!(core::mem::offset_of!(Uniforms, padding_extend) == 112);
    assert!(core::mem::offset_of!(Uniforms, min_contrast) == 116);
    assert!(core::mem::offset_of!(Uniforms, cursor_pos) == 120);
    assert!(core::mem::offset_of!(Uniforms, cursor_color) == 124);
    assert!(core::mem::offset_of!(Uniforms, bg_color) == 128);
    assert!(core::mem::offset_of!(Uniforms, bools) == 132);

    // CellText (shaders.zig:234-257).
    assert!(core::mem::size_of::<CellText>() == 32);
    assert!(core::mem::offset_of!(CellText, glyph_pos) == 0);
    assert!(core::mem::offset_of!(CellText, glyph_size) == 8);
    assert!(core::mem::offset_of!(CellText, bearings) == 16);
    assert!(core::mem::offset_of!(CellText, grid_pos) == 20);
    assert!(core::mem::offset_of!(CellText, color) == 24);
    assert!(core::mem::offset_of!(CellText, atlas) == 28);
    assert!(core::mem::offset_of!(CellText, bools) == 29);

    // CellBg (shaders.zig:260).
    assert!(core::mem::size_of::<CellBg>() == 4);

    // Image (shaders.zig:263-268).
    assert!(core::mem::size_of::<Image>() == 48);
    assert!(core::mem::align_of::<Image>() == 16);
    assert!(core::mem::offset_of!(Image, grid_pos) == 0);
    assert!(core::mem::offset_of!(Image, cell_offset) == 8);
    assert!(core::mem::offset_of!(Image, source_rect) == 16);
    assert!(core::mem::offset_of!(Image, dest_size) == 32);

    // BgImage (shaders.zig:271-300).
    assert!(core::mem::size_of::<BgImage>() == 8);
    assert!(core::mem::offset_of!(BgImage, opacity) == 0);
    assert!(core::mem::offset_of!(BgImage, info) == 4);
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the reference's lone `test { expectEqual(32, @sizeOf(CellText)); }`
    /// (`opengl/shaders.zig:252-256`), extended to all five ABI structs. The
    /// `const{}` guards above already enforce this at build time (strictly
    /// stronger); this runtime test is the brief-mandated per-struct `size_of`
    /// assert and documents the expected sizes in one place.
    #[test]
    fn gpu_abi_struct_sizes() {
        assert_eq!(core::mem::size_of::<Uniforms>(), 144);
        assert_eq!(core::mem::size_of::<CellText>(), 32);
        assert_eq!(core::mem::size_of::<CellBg>(), 4);
        assert_eq!(core::mem::size_of::<Image>(), 48);
        assert_eq!(core::mem::size_of::<BgImage>(), 8);
    }
}
