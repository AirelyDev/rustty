//! Thin, faithful FFI wrappers around `harfbuzz-sys` used by the FreeType face.
//!
//! PORT NOTE (glue): the reference imports the `harfbuzz` package
//! (`pkg/harfbuzz/{font,freetype,...}.zig`). That package is its own port target
//! with its own test contract; until it is carved, this module provides the
//! MINIMAL surface `face/freetype.zig` uses: `harfbuzz.Font` (with `destroy`) and
//! `harfbuzz.freetype.createFont` (the `hb_ft_font_create_referenced` bridge).
//! R1: additive `unsafe` glue over C FFI — nothing from the ported file is lost.

use harfbuzz_sys as c;

/// HarfBuzz error.
// PORT: stands in for `pkg/harfbuzz/errors.zig`'s `Error.HarfbuzzFailed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarfbuzzFailed;

/// A HarfBuzz font object.
// PORT: `pkg/harfbuzz/font.zig`'s `Font`.
#[derive(Clone, Copy, Debug)]
pub struct Font {
    pub handle: *mut c::hb_font_t,
}

impl Font {
    /// Decreases the reference count on the given font object, destroying it at
    /// zero and freeing all memory.
    // PORT: `Font.destroy`.
    pub fn destroy(self) {
        unsafe {
            c::hb_font_destroy(self.handle);
        }
    }

    // PORT: `pkg/harfbuzz/font.zig`'s `Font.setScale`.
    // pub fn setScale(self: *Font, x: u32, y: u32) void {
    //     c.hb_font_set_scale(self.handle, @intCast(x), @intCast(y));
    // }
    pub fn set_scale(self, x: u32, y: u32) {
        unsafe {
            c::hb_font_set_scale(self.handle, x as i32, y as i32);
        }
    }
}

/// Creates an `hb_font_t` font object from the specified `CTFontRef`.
///
// PORT: `pkg/harfbuzz/coretext.zig`'s `createFont` (the `hb_coretext_font_create`
// bridge, enabled by `HAVE_CORETEXT` in the vendored HarfBuzz build). `ct_font`
// is a type-erased `CTFontRef` (the caller passes its `objc2` `CTFont` pointer);
// it is cast to the bindgen `CTFontRef` (same underlying `CTFontRef`), mirroring
// the reference's custom extern that avoids a ptrcast mismatch.
#[cfg(target_os = "macos")]
pub fn create_font_coretext(ct_font: *const core::ffi::c_void) -> Result<Font, HarfbuzzFailed> {
    let handle = unsafe { c::hb_coretext_font_create(ct_font as c::CTFontRef) };
    if handle.is_null() {
        Err(HarfbuzzFailed)
    } else {
        Ok(Font { handle })
    }
}

/// The starting cluster of a global feature (inclusive).
// PORT: `harfbuzz.c.HB_FEATURE_GLOBAL_START`.
pub const HB_FEATURE_GLOBAL_START: u32 = c::HB_FEATURE_GLOBAL_START;

/// The ending cluster of a global feature (exclusive).
// PORT: `harfbuzz.c.HB_FEATURE_GLOBAL_END`. HarfBuzz defines this as
// `((unsigned int) -1)`; bindgen cannot const-evaluate that macro so it is
// reproduced here as `u32::MAX` (the identical bit pattern).
pub const HB_FEATURE_GLOBAL_END: u32 = u32::MAX;

/// Represents an OpenType feature setting passed to `shape`.
// PORT: `pkg/harfbuzz/common.zig`'s `Feature` (`extern struct`). The bindgen
// `hb_feature_t` has the identical layout (`tag`/`value`/`start`/`end`), so it
// is re-exported directly rather than re-declared.
pub type Feature = c::hb_feature_t;

/// The `hb_glyph_info_t` result of shaping (glyph index + source cluster).
// PORT: `pkg/harfbuzz/common.zig`'s `GlyphInfo` (`extern struct`). The shaper
// reads `.codepoint` (glyph index after shaping) and `.cluster`.
pub type GlyphInfo = c::hb_glyph_info_t;

/// The `hb_glyph_position_t` result of shaping (advances + offsets).
// PORT: `pkg/harfbuzz/common.zig`'s `GlyphPosition` (`extern struct`). The
// shaper reads `.x_advance`/`.y_advance`/`.x_offset`/`.y_offset`.
pub type GlyphPosition = c::hb_glyph_position_t;

/// The type of buffer contents.
// PORT: `pkg/harfbuzz/buffer.zig`'s `ContentType` (`enum(u2)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentType {
    /// Initial value for new buffer.
    #[allow(dead_code)]
    Invalid,
    /// The buffer contains input characters (before shaping).
    Unicode,
    /// The buffer contains output glyphs (after shaping).
    #[allow(dead_code)]
    Glyphs,
}

impl ContentType {
    fn to_c(self) -> c::hb_buffer_content_type_t {
        match self {
            ContentType::Invalid => c::hb_buffer_content_type_t_HB_BUFFER_CONTENT_TYPE_INVALID,
            ContentType::Unicode => c::hb_buffer_content_type_t_HB_BUFFER_CONTENT_TYPE_UNICODE,
            ContentType::Glyphs => c::hb_buffer_content_type_t_HB_BUFFER_CONTENT_TYPE_GLYPHS,
        }
    }
}

/// HarfBuzz clustering behavior options.
// PORT: `pkg/harfbuzz/buffer.zig`'s `ClusterLevel` (`enum(u2)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterLevel {
    /// Non-base characters merged into the preceding base cluster (default).
    #[allow(dead_code)]
    MonotoneGraphemes,
    /// Non-base characters get their own cluster; output stays monotone.
    #[allow(dead_code)]
    MonotoneCharacters,
    /// Non-base characters get their own cluster; not forced monotone.
    Characters,
    /// Grapheme clustering without forcing monotone order.
    #[allow(dead_code)]
    Graphemes,
}

impl ClusterLevel {
    fn to_c(self) -> c::hb_buffer_cluster_level_t {
        match self {
            ClusterLevel::MonotoneGraphemes => {
                c::hb_buffer_cluster_level_t_HB_BUFFER_CLUSTER_LEVEL_MONOTONE_GRAPHEMES
            }
            ClusterLevel::MonotoneCharacters => {
                c::hb_buffer_cluster_level_t_HB_BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS
            }
            ClusterLevel::Characters => {
                c::hb_buffer_cluster_level_t_HB_BUFFER_CLUSTER_LEVEL_CHARACTERS
            }
            ClusterLevel::Graphemes => {
                c::hb_buffer_cluster_level_t_HB_BUFFER_CLUSTER_LEVEL_GRAPHEMES
            }
        }
    }
}

/// The text flow direction of a buffer.
// PORT: `pkg/harfbuzz/common.zig`'s `Direction` (`enum(u3)`). Only the variants
// the shaper touches are exercised; the rest round out the enum for fidelity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    #[allow(dead_code)]
    Invalid,
    Ltr,
    #[allow(dead_code)]
    Rtl,
    #[allow(dead_code)]
    Ttb,
    #[allow(dead_code)]
    Btt,
}

impl Direction {
    fn to_c(self) -> c::hb_direction_t {
        match self {
            Direction::Invalid => c::hb_direction_t_HB_DIRECTION_INVALID,
            Direction::Ltr => c::hb_direction_t_HB_DIRECTION_LTR,
            Direction::Rtl => c::hb_direction_t_HB_DIRECTION_RTL,
            Direction::Ttb => c::hb_direction_t_HB_DIRECTION_TTB,
            Direction::Btt => c::hb_direction_t_HB_DIRECTION_BTT,
        }
    }
}

/// Buffers serve a dual role in HarfBuzz; before shaping, they hold the input
/// characters passed to `shape`, and after shaping they hold the output glyphs.
// PORT: `pkg/harfbuzz/buffer.zig`'s `Buffer` (only the surface the shaper uses).
#[derive(Clone, Copy, Debug)]
pub struct Buffer {
    pub handle: *mut c::hb_buffer_t,
}

impl Buffer {
    /// Creates a new `hb_buffer_t` with all properties at defaults.
    // pub fn create() Error!Buffer { ... }
    pub fn create() -> Result<Buffer, HarfbuzzFailed> {
        let handle = unsafe { c::hb_buffer_create() };
        if handle.is_null() {
            Err(HarfbuzzFailed)
        } else {
            Ok(Buffer { handle })
        }
    }

    /// Deallocate the buffer. Decreases its reference count by one; at zero the
    /// buffer and all associated resources are freed.
    // pub fn destroy(self: *Buffer) void { c.hb_buffer_destroy(self.handle); }
    pub fn destroy(self) {
        unsafe {
            c::hb_buffer_destroy(self.handle);
        }
    }

    /// Resets the buffer to its initial status.
    // pub fn reset(self: Buffer) void { c.hb_buffer_reset(self.handle); }
    pub fn reset(self) {
        unsafe {
            c::hb_buffer_reset(self.handle);
        }
    }

    /// Returns the number of items in the buffer.
    // pub fn getLength(self: Buffer) u32 { return c.hb_buffer_get_length(self.handle); }
    pub fn get_length(self) -> u32 {
        unsafe { c::hb_buffer_get_length(self.handle) }
    }

    /// Sets the type of buffer contents.
    // pub fn setContentType(self, ct) void { c.hb_buffer_set_content_type(...); }
    pub fn set_content_type(self, ct: ContentType) {
        unsafe {
            c::hb_buffer_set_content_type(self.handle, ct.to_c());
        }
    }

    /// Appends a character with Unicode value `cp` and cluster value `cluster`.
    // pub fn add(self, cp: u32, cluster: u32) void { c.hb_buffer_add(...); }
    pub fn add(self, cp: u32, cluster: u32) {
        unsafe {
            c::hb_buffer_add(self.handle, cp, cluster);
        }
    }

    /// Set the text flow direction of the buffer.
    // pub fn setDirection(self, dir) void { c.hb_buffer_set_direction(...); }
    pub fn set_direction(self, dir: Direction) {
        unsafe {
            c::hb_buffer_set_direction(self.handle, dir.to_c());
        }
    }

    /// Sets unset buffer segment properties from the buffer's Unicode contents.
    // pub fn guessSegmentProperties(self) void { c.hb_buffer_guess_segment_properties(...); }
    pub fn guess_segment_properties(self) {
        unsafe {
            c::hb_buffer_guess_segment_properties(self.handle);
        }
    }

    /// Sets the cluster level of a buffer.
    // pub fn setClusterLevel(self, level) void { c.hb_buffer_set_cluster_level(...); }
    pub fn set_cluster_level(self, level: ClusterLevel) {
        unsafe {
            c::hb_buffer_set_cluster_level(self.handle, level.to_c());
        }
    }

    /// Returns the buffer glyph information array. Valid as long as buffer
    /// contents are not modified.
    // pub fn getGlyphInfos(self: Buffer) []GlyphInfo { ... }
    pub fn get_glyph_infos(self) -> &'static [GlyphInfo] {
        let mut length: u32 = 0;
        let ptr = unsafe { c::hb_buffer_get_glyph_infos(self.handle, &mut length) };
        if ptr.is_null() {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(ptr, length as usize) }
    }

    /// Returns the buffer glyph position array, or `None` (e.g. within a buffer
    /// message callback). Valid as long as buffer contents are not modified.
    // pub fn getGlyphPositions(self: Buffer) ?[]GlyphPosition { ... }
    pub fn get_glyph_positions(self) -> Option<&'static [GlyphPosition]> {
        let mut length: u32 = 0;
        let ptr = unsafe { c::hb_buffer_get_glyph_positions(self.handle, &mut length) };
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { core::slice::from_raw_parts(ptr, length as usize) })
        }
    }
}

/// Shapes `buf` using `font`, turning its Unicode content into positioned
/// glyphs. If `features` is non-empty it controls the applied features.
// PORT: `pkg/harfbuzz/shape.zig`'s `shape`.
// pub fn shape(font: Font, buf: Buffer, features: ?[]const Feature) void { ... }
pub fn shape(font: Font, buf: Buffer, features: Option<&[Feature]>) {
    // const hb_feats: [*c]const c.hb_feature_t = feats: {
    //     if (features) |fs| if (fs.len > 0) break :feats @ptrCast(fs.ptr);
    //     break :feats null;
    // };
    let hb_feats: *const c::hb_feature_t = match features {
        Some(fs) if !fs.is_empty() => fs.as_ptr(),
        _ => core::ptr::null(),
    };

    unsafe {
        c::hb_shape(
            font.handle,
            buf.handle,
            hb_feats,
            match features {
                Some(f) => f.len() as core::ffi::c_uint,
                None => 0,
            },
        );
    }
}

/// Creates an `hb_font_t` font object from the specified `FT_Face`.
///
/// Calls `FT_Reference_Face` on `ft_face`, ensuring it remains alive as long as
/// the resulting font object does, and `FT_Done_Face` when the font is destroyed.
// PORT: `pkg/harfbuzz/freetype.zig`'s `createFont` (via the custom extern
// `hb_ft_font_create_referenced` so the correct FreeType struct is used).
pub fn create_font(face: freetype_sys::FT_Face) -> Result<Font, HarfbuzzFailed> {
    // `harfbuzz-sys` binds its own `FT_Face` type; cast the `freetype-sys`
    // pointer to it (same underlying C `FT_Face`), mirroring the reference's
    // custom-extern trick that avoids a ptrcast mismatch.
    let handle = unsafe { c::hb_ft_font_create_referenced(face.cast()) };
    if handle.is_null() {
        Err(HarfbuzzFailed)
    } else {
        Ok(Font { handle })
    }
}
