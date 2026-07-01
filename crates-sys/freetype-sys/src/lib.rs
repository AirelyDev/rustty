//! freetype-sys — FFI bindings to the vendored FreeType 2.13.2 font rasterizer
//! (`links = "freetype"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: copies the SAME C library the reference uses (FreeType 2.13.2) and
//! builds it standalone (`cc` + `bindgen`; see `build.rs`). No pure-Rust
//! substitution during parity phases. The FFI is bindgen-generated from
//! `wrapper.h` — the same public FreeType headers the reference's
//! `pkg/freetype/c.zig` @cImports (`ft2build.h`, `FT_FREETYPE_H`,
//! `FT_TRUETYPE_TABLES_H`, `ftmm.h`, `ftoutln.h`, `ftsnames.h`, `ttnameid.h`,
//! `ftbitmap.h`, `ftbbox.h`).

// Force-link the vendored system zlib. FreeType is built with
// `FT_CONFIG_OPTION_SYSTEM_ZLIB=1`, so `src/gzip/ftgzip.c` references the zlib
// inflate API (`inflate`, `inflateInit2_`, `inflateEnd`, `inflateReset`).
// Without an explicit reference to the `zlib-sys` rlib, rustc prunes it — and
// with it the `cargo:rustc-link-lib=static=z` directive — leaving those symbols
// undefined at link time. `extern crate` keeps the archive on the link line
// (mirrors the reference's `lib.linkLibrary(zlib_dep)`).
extern crate zlib_sys;

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
    use std::ptr;

    /// Small self-contained TrueType font vendored for the KAT (Geist Mono,
    /// SIL Open Font License). Embedded so the smoke test needs no filesystem.
    const TEST_FONT: &[u8] = include_bytes!("../res/GeistMono-Regular.ttf");

    /// R3 known-answer test: prove the vendored FreeType 2.13.2 links and runs.
    ///
    /// Mirrors the reference's smoke path (Library.init → initMemoryFace →
    /// deinit; see pkg/freetype/Library.zig + face.zig "loading memory font"):
    ///   * `FT_Init_FreeType` succeeds and reports version 2.13.2 (the exact
    ///     vendored release — a fixed known answer);
    ///   * `FT_New_Memory_Face` parses the embedded TrueType font;
    ///   * the parsed face exposes the font's known metrics (`num_glyphs`,
    ///     `units_per_EM`) — fixed for this exact font file;
    ///   * `FT_Done_Face` / `FT_Done_FreeType` tear down cleanly.
    #[test]
    fn kat_freetype_2_13_2_memory_face() {
        unsafe {
            // Initialize the library.
            let mut lib: FT_Library = ptr::null_mut();
            let err = FT_Init_FreeType(&mut lib);
            assert_eq!(err, 0, "FT_Init_FreeType failed: {err}");
            assert!(!lib.is_null());

            // Known-answer: the vendored release is exactly FreeType 2.13.2.
            let (mut major, mut minor, mut patch) = (0, 0, 0);
            FT_Library_Version(lib, &mut major, &mut minor, &mut patch);
            assert_eq!((major, minor, patch), (2, 13, 2), "FreeType version");

            // Load the embedded TrueType font from memory (index 0).
            let mut face: FT_Face = ptr::null_mut();
            let err = FT_New_Memory_Face(
                lib,
                TEST_FONT.as_ptr(),
                TEST_FONT.len() as FT_Long,
                0,
                &mut face,
            );
            assert_eq!(err, 0, "FT_New_Memory_Face failed: {err}");
            assert!(!face.is_null());

            // Known metrics of this exact font file.
            let rec = &*face;
            assert_eq!(rec.num_glyphs, 825, "num_glyphs");
            assert_eq!(rec.units_per_EM, 1000, "units_per_EM");
            // Scalable TrueType outlines expose the OUTLINE face flag.
            assert_ne!(
                rec.face_flags & (FT_FACE_FLAG_SCALABLE as FT_Long),
                0,
                "expected a scalable (outline) face"
            );

            // Tear down.
            assert_eq!(FT_Done_Face(face), 0);
            assert_eq!(FT_Done_FreeType(lib), 0);
        }
    }
}
