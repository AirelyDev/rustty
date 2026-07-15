//! Build script for rustty-font-shaper.
//!
//! Compiles the vendored `stb_image_resize` single-header library into a small
//! static object exposing `stbir_resize_uint8`. R3: this is the SAME C the
//! reference links (ghostty `src/stb/stb_image_resize.h`), copied into our own
//! `vendor/` tree and built with `cc` — no pure-Rust substitution. The freetype
//! face uses it to scale bitmap-strike glyphs (`FT_GLYPH_FORMAT_BITMAP`).

fn main() {
    // WASM DIVERGENCE (`wasm32`): the freestanding web_canvas backend does not
    // use the FreeType face and therefore never links `stbir_resize_uint8`; skip
    // the C compile (freestanding wasm has no C toolchain wired here). The `stb`
    // module + freetype face are cfg-gated off on wasm (see lib.rs / face/mod.rs).
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }

    println!("cargo:rerun-if-changed=vendor/stb_impl.c");
    println!("cargo:rerun-if-changed=vendor/stb_image_resize.h");

    cc::Build::new()
        .file("vendor/stb_impl.c")
        .include("vendor")
        // stb_image_resize.h is a public-domain single-header lib; silence its
        // (benign) warnings so they don't trip a -Werror-style CI.
        .warnings(false)
        .compile("rustty_stb_image_resize");
}
