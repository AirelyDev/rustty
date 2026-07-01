//! harfbuzz-sys — FFI bindings to the vendored HarfBuzz 11.0.0 text shaping
//! engine (`links = "harfbuzz"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: copies the SAME C library the reference uses (HarfBuzz 11.0.0) and
//! builds it standalone (`cc` in C++17 mode + `bindgen`; see `build.rs`). No
//! pure-Rust substitution during parity phases. The FFI is bindgen-generated
//! from `wrapper.h` — the same public HarfBuzz headers the reference's
//! `pkg/harfbuzz/c.zig` @cImports (`hb.h` and, because the vendored build
//! enables FreeType, `hb-ft.h`; CoreText is off so `hb-coretext.h` is omitted).

// Force-link the vendored FreeType. HarfBuzz is built with `-DHAVE_FREETYPE=1`,
// so `src/hb-ft.cc` references the FreeType API (`FT_Get_Char_Index`,
// `FT_Load_Glyph`, `FT_Reference_Face`, `FT_Done_Face`, …). Without an explicit
// reference to the `freetype-sys` rlib, rustc prunes it — and with it the
// `cargo:rustc-link-lib=static=freetype` directive — leaving those symbols
// undefined at link time. `extern crate` keeps the archive on the link line
// (mirrors the reference's `lib.linkLibrary(freetype)`).
extern crate freetype_sys;

// The bindgen-generated module carries C naming conventions and machine-shaped
// code; suppress the whole surface (this is generated, not hand-authored — the
// clippy/rustc-lint gate applies to our code, not the generated bindings).
#[allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    dead_code,
    rustdoc::all
)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

#[cfg(test)]
mod tests {
    use super::*;
    use freetype_sys::{
        FT_Done_Face, FT_Done_FreeType, FT_Init_FreeType, FT_Library, FT_Long, FT_New_Memory_Face,
        FT_Set_Pixel_Sizes,
    };
    use std::ptr;

    /// Small self-contained TrueType font vendored for the KAT (Geist Mono,
    /// SIL Open Font License). Embedded so the smoke test needs no filesystem.
    /// Geist Mono is a MONOSPACED font — a property the KAT leans on as a
    /// stable known answer (every shaped Latin glyph gets an identical advance).
    const TEST_FONT: &[u8] = include_bytes!("../res/GeistMono-Regular.ttf");

    /// R3 known-answer test: prove the vendored HarfBuzz 11.0.0 links and runs.
    ///
    /// Mirrors the reference's shaping path (pkg/harfbuzz: `freetype.createFont`
    /// → `Buffer` add-utf8 + set direction/script/language → `shape` →
    /// inspect `GlyphPosition`s). It:
    ///   * reports version 11.0.0 (the exact vendored release — a fixed known
    ///     answer) via `hb_version` / `hb_version_atleast`;
    ///   * builds an `hb_font_t` from a bundled FreeType face
    ///     (`hb_ft_font_create_referenced` — the FreeType↔HarfBuzz bridge that
    ///     `-DHAVE_FREETYPE=1` compiles in);
    ///   * shapes the ASCII string "hello" with LTR / Latin / "en" set
    ///     explicitly (as the reference's Buffer helpers do);
    ///   * asserts the known glyph count (5, one glyph per ASCII letter — no
    ///     ligatures) and known advances (every glyph resolves to a non-zero
    ///     glyph id, and — because Geist Mono is monospaced — every glyph gets
    ///     the SAME positive `x_advance` and a zero `y_advance`).
    #[test]
    fn kat_harfbuzz_11_0_0_shape_hello() {
        unsafe {
            // Known-answer: the vendored release is exactly HarfBuzz 11.0.0.
            let (mut major, mut minor, mut micro) = (0u32, 0u32, 0u32);
            hb_version(&mut major, &mut minor, &mut micro);
            assert_eq!((major, minor, micro), (11, 0, 0), "HarfBuzz version");
            assert_ne!(
                hb_version_atleast(1, 0, 0),
                0,
                "hb_version_atleast(1,0,0) must hold"
            );

            // Load the embedded TrueType font from memory (index 0) via
            // FreeType (the reference builds its hb_font from an FT_Face).
            let mut ft_lib: FT_Library = ptr::null_mut();
            assert_eq!(FT_Init_FreeType(&mut ft_lib), 0, "FT_Init_FreeType");
            assert!(!ft_lib.is_null());

            let mut ft_face = ptr::null_mut();
            let err = FT_New_Memory_Face(
                ft_lib,
                TEST_FONT.as_ptr(),
                TEST_FONT.len() as FT_Long,
                0,
                &mut ft_face,
            );
            assert_eq!(err, 0, "FT_New_Memory_Face failed: {err}");
            assert!(!ft_face.is_null());

            // Establish a pixel size on the face. `hb_ft_font_create_referenced`
            // derives the HarfBuzz font scale from the FreeType face's active
            // size; without a size set the scale (and therefore every advance)
            // is zero. The reference sets the face size before shaping the same
            // way (pkg/font's FreeType face sizing).
            assert_eq!(FT_Set_Pixel_Sizes(ft_face, 0, 32), 0, "FT_Set_Pixel_Sizes");

            // Bridge the FreeType face into a HarfBuzz font. `.cast()` bridges
            // freetype-sys's `FT_Face` to the identical `FT_Face` type bindgen
            // regenerated inside harfbuzz-sys (same C type, distinct Rust type —
            // the reference sidesteps this with custom extern fns; a raw-pointer
            // cast is the faithful Rust equivalent).
            let hb_font = hb_ft_font_create_referenced(ft_face.cast());
            assert!(!hb_font.is_null(), "hb_ft_font_create_referenced");

            // Build and populate the shaping buffer.
            let buf = hb_buffer_create();
            assert_ne!(hb_buffer_allocation_successful(buf), 0, "buffer alloc");

            let text = b"hello";
            hb_buffer_add_utf8(
                buf,
                text.as_ptr().cast(),
                text.len() as ::std::os::raw::c_int,
                0,
                text.len() as ::std::os::raw::c_int,
            );

            // Set direction / script / language explicitly (as the reference's
            // Buffer helpers do), rather than relying on segment-property
            // guessing.
            hb_buffer_set_direction(buf, hb_direction_t_HB_DIRECTION_LTR);
            hb_buffer_set_script(buf, hb_script_t_HB_SCRIPT_LATIN);
            let lang = hb_language_from_string(c"en".as_ptr(), -1);
            hb_buffer_set_language(buf, lang);

            // Shape (no features — the reference passes NULL when features is
            // empty).
            hb_shape(hb_font, buf, ptr::null(), 0);

            // Known glyph count: one glyph per ASCII letter, no ligatures.
            let len = hb_buffer_get_length(buf);
            assert_eq!(len, 5, "expected 5 shaped glyphs for \"hello\"");

            // Inspect glyph ids and advances.
            let mut n_info = 0u32;
            let infos = hb_buffer_get_glyph_infos(buf, &mut n_info);
            let mut n_pos = 0u32;
            let positions = hb_buffer_get_glyph_positions(buf, &mut n_pos);
            assert_eq!(n_info, 5);
            assert_eq!(n_pos, 5);
            assert!(!infos.is_null() && !positions.is_null());

            let infos = std::slice::from_raw_parts(infos, n_info as usize);
            let positions = std::slice::from_raw_parts(positions, n_pos as usize);

            // Every letter resolves to a real glyph (non-zero id => found in
            // the font, no .notdef fallback).
            for gi in infos {
                assert_ne!(gi.codepoint, 0, "glyph id 0 (.notdef) — letter unmapped");
            }

            // Known advances: Geist Mono is monospaced, so every glyph gets the
            // SAME positive horizontal advance and a zero vertical advance.
            let adv = positions[0].x_advance;
            assert!(adv > 0, "first glyph x_advance must be positive, got {adv}");
            for p in positions {
                assert_eq!(p.x_advance, adv, "monospace: all x_advances equal");
                assert_eq!(p.y_advance, 0, "horizontal shaping: y_advance == 0");
            }

            // Tear down (hb_font owns the referenced FT_Face; we still own our
            // own FT_Face handle + library).
            hb_buffer_destroy(buf);
            hb_font_destroy(hb_font);
            assert_eq!(FT_Done_Face(ft_face), 0);
            assert_eq!(FT_Done_FreeType(ft_lib), 0);
        }
    }
}
