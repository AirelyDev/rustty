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
