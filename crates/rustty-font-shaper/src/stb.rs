//! Minimal FFI surface for the vendored `stb_image_resize` single-header lib.
//!
//! PORTS: ghostty `src/stb/main.zig` — but only `stbir_resize_uint8`, the one
//! symbol the freetype face uses (to scale bitmap-strike glyphs). The reference
//! `@cImport`s `stb_image.h` + `stb_image_resize.h` and re-exports
//! `stbi_load_from_memory` / `stbi_image_free` / `stbir_resize_uint8`; the first
//! two are used elsewhere (image decoding) and land with that port. R3: the C is
//! the SAME single-header lib, vendored under `vendor/` and built by `build.rs`.

use std::os::raw::{c_int, c_uchar};

unsafe extern "C" {
    /// `int stbir_resize_uint8(const unsigned char *input_pixels, int input_w,
    ///  int input_h, int input_stride_in_bytes, unsigned char *output_pixels,
    ///  int output_w, int output_h, int output_stride_in_bytes, int num_channels)`
    ///
    /// Returns 1 on success, 0 on failure.
    pub fn stbir_resize_uint8(
        input_pixels: *const c_uchar,
        input_w: c_int,
        input_h: c_int,
        input_stride_in_bytes: c_int,
        output_pixels: *mut c_uchar,
        output_w: c_int,
        output_h: c_int,
        output_stride_in_bytes: c_int,
        num_channels: c_int,
    ) -> c_int;
}
