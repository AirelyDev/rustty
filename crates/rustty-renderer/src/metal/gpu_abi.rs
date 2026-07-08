//! The exact Metal-ABI byte images of the ergonomic vertex/uniform structs.
//!
//! PORTS: nothing new from ghostty — this is the **backend serializer** the
//! `crate::shaders` PORT NOTEs named. Those notes say the exact GPU byte layout
//! "is applied by the R-d/Phase 3 GPU backend when this is serialized into a
//! uniform buffer" and that the packed structs' "`_padding` bits [are]
//! reconstructed by the backend serializer." `metal::gpu_abi` IS that serializer
//! (GPU_RD_CHUNK4_DESIGN §2): it defines the `#[repr(C)]` POD types whose byte
//! image the `.metal` shaders read, the `From<&Ergonomic>` bit-packing, `const{}`
//! offset drift guards, and a byte unit-test. The ergonomic
//! `crate::shaders::{Uniforms, CellText, BgImage}` (with their named-bool
//! sub-structs) stay UNTOUCHED; `FrameState`'s GPU buffers hold these ABI types and
//! `drawFrame` converts at the `sync` boundary.
//!
//! PORT NOTE (⚠ METAL-vs-OpenGL ABI — the discrepancy the design flagged, §2.0):
//! the `crate::shaders.rs` PORT NOTEs describe `Bools`/`PaddingExtend` as `packed
//! struct(u32)` — that is the **OpenGL** (`opengl/shaders.zig`) layout, where
//! `bools`/`padding_extend` are each a 4-byte `u32`. The MONOMORPHIC port targets
//! **Metal**, whose `metal/shaders.zig` has `padding_extend: packed struct(u8)` (a
//! SINGLE byte) and `bools: extern struct { 4× bool align(1) }` (FOUR separate
//! bytes, NOT a `u32`). This serializer targets the **Metal** ABI, verified against
//! `src/shaders/shaders.metal`: `uint8_t padding_extend` (bits `EXTEND_LEFT=1`,
//! `EXTEND_RIGHT=2`, `EXTEND_UP=4`, `EXTEND_DOWN=8`) + four inline `bool`s
//! (`cursor_wide`/`use_display_p3`/`use_linear_blending`/`use_linear_correction`);
//! `CellText.bools: uint8_t` (`NO_MIN_CONTRAST=1`, `IS_CURSOR_GLYPH=2`);
//! `BgImage.info: uint8_t` (`BgImagePosition` bits 0-3, `BgImageFit` bits 4-5,
//! `BgImageRepeat` bit 6). Do NOT follow the misleading "u32" notes.

use crate::shaders::{BgImage, CellText, Uniforms};

/// The exact Metal-ABI byte image of `crate::shaders::Uniforms` (the
/// `metal/shaders.zig` `extern struct` / the `Uniforms` struct the `.metal`
/// shaders read). Explicit padding fields reproduce Zig's `align(16)`/`align(4)`
/// gaps; `padding_extend` is the packed `EXTEND_*` byte; the four `bools` are four
/// separate `u8`s (Metal, NOT a u32). 144 bytes, align 16.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct UniformsGpu {
    pub(crate) projection_matrix: [[f32; 4]; 4], // 0
    pub(crate) screen_size: [f32; 2],            // 64
    pub(crate) cell_size: [f32; 2],              // 72
    pub(crate) grid_size: [u16; 2],              // 80
    pub(crate) _pad0: [u8; 12], // 84  → grid_padding@96 (Zig grid_padding align(16))
    pub(crate) grid_padding: [f32; 4], // 96
    pub(crate) padding_extend: u8, // 112 (left|right<<1|up<<2|down<<3)
    pub(crate) _pad1: [u8; 3],  // 113 → min_contrast@116 (Zig min_contrast align(4))
    pub(crate) min_contrast: f32, // 116
    pub(crate) cursor_pos: [u16; 2], // 120
    pub(crate) cursor_color: [u8; 4], // 124
    pub(crate) bg_color: [u8; 4], // 128
    pub(crate) cursor_wide: u8, // 132
    pub(crate) use_display_p3: u8, // 133
    pub(crate) use_linear_blending: u8, // 134
    pub(crate) use_linear_correction: u8, // 135
                                // align(16) ⇒ size 144 (tail pad 136..144)
}

impl From<&Uniforms> for UniformsGpu {
    fn from(u: &Uniforms) -> Self {
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

// Drift guard: fails the build if any offset drifts from the Metal ABI.
// (Bit order LSB-first matches Zig `packed struct(u8)` field order AND the `.metal`
//  `EXTEND_*` enum; the four bools are four bytes matching MSL's four inline `bool`s.)
const _: () = {
    assert!(core::mem::size_of::<UniformsGpu>() == 144);
    assert!(core::mem::align_of::<UniformsGpu>() == 16);
    assert!(core::mem::offset_of!(UniformsGpu, projection_matrix) == 0);
    assert!(core::mem::offset_of!(UniformsGpu, screen_size) == 64);
    assert!(core::mem::offset_of!(UniformsGpu, cell_size) == 72);
    assert!(core::mem::offset_of!(UniformsGpu, grid_size) == 80);
    assert!(core::mem::offset_of!(UniformsGpu, grid_padding) == 96);
    assert!(core::mem::offset_of!(UniformsGpu, padding_extend) == 112);
    assert!(core::mem::offset_of!(UniformsGpu, min_contrast) == 116);
    assert!(core::mem::offset_of!(UniformsGpu, cursor_pos) == 120);
    assert!(core::mem::offset_of!(UniformsGpu, cursor_color) == 124);
    assert!(core::mem::offset_of!(UniformsGpu, bg_color) == 128);
    assert!(core::mem::offset_of!(UniformsGpu, cursor_wide) == 132);
    assert!(core::mem::offset_of!(UniformsGpu, use_linear_correction) == 135);
};

/// The exact Metal-ABI byte image of `crate::shaders::CellText` (the per-instance
/// `cell_text` vertex the `.metal` shaders read). 32 bytes. The `atlas`/`bools`
/// bytes are the packed enum/bit values (§2.2).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CellTextGpu {
    pub(crate) glyph_pos: [u32; 2],  // 0
    pub(crate) glyph_size: [u32; 2], // 8
    pub(crate) bearings: [i16; 2],   // 16
    pub(crate) grid_pos: [u16; 2],   // 20
    pub(crate) color: [u8; 4],       // 24
    pub(crate) atlas: u8,            // 28  (Atlas enum value 0=grayscale, 1=color)
    pub(crate) bools: u8,            // 29  (no_min_contrast | is_cursor_glyph<<1)
                                     // align 4 ⇒ size 32 (tail pad 30..32)
}

impl From<&CellText> for CellTextGpu {
    fn from(c: &CellText) -> Self {
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

// Drift guard. These offsets/stride are IDENTICAL to the pre-Chunk-4
// `VertexLayoutRef` for `CellText` (`[0,8,16,20,24,28,29]`, stride 32) — the only
// ABI-fix is the *content* of byte 29 (`is_cursor_glyph` now visible in bit 1;
// previously it lived at offset 30 in the ergonomic 2-bool struct and was never
// read → OPEN RISK #1 from Chunk-1b).
const _: () = {
    assert!(core::mem::size_of::<CellTextGpu>() == 32);
    assert!(core::mem::offset_of!(CellTextGpu, glyph_pos) == 0);
    assert!(core::mem::offset_of!(CellTextGpu, glyph_size) == 8);
    assert!(core::mem::offset_of!(CellTextGpu, bearings) == 16);
    assert!(core::mem::offset_of!(CellTextGpu, grid_pos) == 20);
    assert!(core::mem::offset_of!(CellTextGpu, color) == 24);
    assert!(core::mem::offset_of!(CellTextGpu, atlas) == 28);
    assert!(core::mem::offset_of!(CellTextGpu, bools) == 29);
};

/// The exact Metal-ABI byte image of `crate::shaders::BgImage`. 8 bytes. Defined
/// for buffer-type consistency (`FrameState.bg_image_buffer: Buffer<BgImageGpu>`
/// stays a real ABI buffer); NOT on the golden path — its `sync` in `drawFrame` is
/// guarded by `bg_image_buffer_modified` and never hit with `bg_image = None`
/// (§2.3).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BgImageGpu {
    pub(crate) opacity: f32, // 0
    pub(crate) info: u8,     // 4  (position | fit<<4 | repeat<<6)
                             // align 4 ⇒ size 8
}

impl From<&BgImage> for BgImageGpu {
    fn from(b: &BgImage) -> Self {
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

const _: () = {
    assert!(core::mem::size_of::<BgImageGpu>() == 8);
    assert!(core::mem::offset_of!(BgImageGpu, opacity) == 0);
    assert!(core::mem::offset_of!(BgImageGpu, info) == 4);
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shaders::{
        Atlas, BgImageInfo, BgImageInfoFit, BgImageInfoPosition, CellText, CellTextBools, Uniforms,
        UniformsBools, UniformsPaddingExtend,
    };

    // Read byte `i` of a `#[repr(C)]` POD WITHOUT materializing the struct's tail
    // padding (which `mem::transmute` would read as uninit). `&*(ptr as *const
    // [u8; N])` forms a byte view; indexing reads only the requested (initialized)
    // offset.
    fn byte<T: Copy, const N: usize>(v: &T, i: usize) -> u8 {
        // SAFETY: `T` is a `#[repr(C)]` POD of size `N`; we read a single
        // initialized byte at offset `i < N` (never the tail padding).
        let bytes = unsafe { &*(v as *const T as *const [u8; N]) };
        bytes[i]
    }

    // §2.5: build ergonomic structs with known named-bool values, convert, and
    // assert the raw bytes at the exact ABI offsets. Runs off a Metal host (pure
    // byte math) — the adversary's "compute the expected byte" check, independent
    // of the GPU golden.
    #[test]
    fn cell_text_bools_pack_to_byte_29() {
        let c = CellText {
            bools: CellTextBools {
                no_min_contrast: false,
                is_cursor_glyph: true,
            },
            atlas: Atlas::Color,
            color: [0x11, 0x22, 0x33, 0x44],
            grid_pos: [2, 0],
            ..Default::default()
        };
        let g = CellTextGpu::from(&c);
        assert_eq!(byte::<_, 32>(&g, 29), 0b10); // IS_CURSOR_GLYPH bit (bit 1)
        assert_eq!(byte::<_, 32>(&g, 28), 1); // atlas: Color = 1
                                              // color bytes are ABI-identical (uchar4 @ 24)
        assert_eq!(byte::<_, 32>(&g, 24), 0x11);
        assert_eq!(byte::<_, 32>(&g, 27), 0x44);

        // no_min_contrast bit0 only
        let c2 = CellText {
            bools: CellTextBools {
                no_min_contrast: true,
                is_cursor_glyph: false,
            },
            ..Default::default()
        };
        assert_eq!(byte::<_, 32>(&CellTextGpu::from(&c2), 29), 0b01); // NO_MIN_CONTRAST

        // both bits
        let c3 = CellText {
            bools: CellTextBools {
                no_min_contrast: true,
                is_cursor_glyph: true,
            },
            ..Default::default()
        };
        assert_eq!(byte::<_, 32>(&CellTextGpu::from(&c3), 29), 0b11);
    }

    #[test]
    fn uniforms_padding_extend_and_bools_pack() {
        // Base zeroed ergonomic Uniforms (all POD; zero = all-false bools).
        let mut u: Uniforms = unsafe { core::mem::zeroed() };

        // padding_extend {up:true} -> byte[112] == EXTEND_UP == 0b0100 == 4
        u.padding_extend = UniformsPaddingExtend {
            left: false,
            right: false,
            up: true,
            down: false,
        };
        assert_eq!(byte::<_, 144>(&UniformsGpu::from(&u), 112), 0b0100);

        // all four -> 1|2|4|8 == 15
        u.padding_extend = UniformsPaddingExtend {
            left: true,
            right: true,
            up: true,
            down: true,
        };
        assert_eq!(byte::<_, 144>(&UniformsGpu::from(&u), 112), 0x0F);

        // left|down -> 1|8 == 9
        u.padding_extend = UniformsPaddingExtend {
            left: true,
            right: false,
            up: false,
            down: true,
        };
        assert_eq!(byte::<_, 144>(&UniformsGpu::from(&u), 112), 0b1001);
        u.padding_extend = UniformsPaddingExtend::default();

        // bools {use_display_p3:true} -> byte[133] == 1 (the four bools are four bytes)
        u.bools = UniformsBools {
            cursor_wide: false,
            use_display_p3: true,
            use_linear_blending: false,
            use_linear_correction: false,
        };
        let g = UniformsGpu::from(&u);
        assert_eq!(byte::<_, 144>(&g, 132), 0); // cursor_wide
        assert_eq!(byte::<_, 144>(&g, 133), 1); // use_display_p3
        assert_eq!(byte::<_, 144>(&g, 134), 0); // use_linear_blending
        assert_eq!(byte::<_, 144>(&g, 135), 0); // use_linear_correction

        // bg_color at offset 128..132; a[131] is the alpha the golden's non-vacuity
        // assert depends on (bg_color offset 128 + 3).
        u.bg_color = [0x00, 0x00, 0x00, 0xFF];
        let g2 = UniformsGpu::from(&u);
        assert_eq!(byte::<_, 144>(&g2, 128), 0x00);
        assert_eq!(byte::<_, 144>(&g2, 131), 0xFF); // bg alpha
                                                    // cursor_color at 124..128
        u.cursor_color = [0xFF, 0x00, 0x00, 0xFF];
        assert_eq!(byte::<_, 144>(&UniformsGpu::from(&u), 124), 0xFF);
    }

    #[test]
    fn bg_image_info_packs_to_byte_4() {
        let b = BgImage {
            opacity: 1.0,
            info: BgImageInfo {
                position: BgImageInfoPosition::Mc, // 4 (bits 0-3)
                fit: BgImageInfoFit::Cover,        // 1 (bits 4-5)
                repeat: true,                      // bit 6
            },
        };
        let g = BgImageGpu::from(&b);
        // 4 | (1<<4) | (1<<6) == 0x04 | 0x10 | 0x40 == 0x54
        assert_eq!(byte::<_, 8>(&g, 4), 0x54);
    }
}
