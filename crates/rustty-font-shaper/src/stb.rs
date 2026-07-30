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

#[cfg(test)]
mod tests {
    use super::stbir_resize_uint8;

    /// The deterministic ramp used by `.port/tools/stb-resize-oracle/oracle.c`, so a
    /// value produced here can be cross-checked against every toolchain in the D10
    /// matrix without re-deriving the input. KEEP IN SYNC with that harness.
    fn ramp_8x16() -> Vec<u8> {
        (0..16)
            .flat_map(|y| (0..8).map(move |x| ((x * 31 + y * 17) & 0xff) as u8))
            .collect()
    }

    /// Not a golden — a STRUCTURAL assertion that the C ran and interpolated:
    /// a 2x horizontal upsample of a strictly increasing row must be monotonic
    /// non-decreasing across that row, and must contain values that appear in NO
    /// source row (proof of interpolation rather than sample copying). Nearest
    /// neighbour would produce only source values and fails the second half.
    #[test]
    fn stbir_resize_uint8_upsamples_a_known_ramp() {
        let src = ramp_8x16();
        let mut dst = vec![0u8; 16 * 32];
        let rc =
            unsafe { stbir_resize_uint8(src.as_ptr(), 8, 16, 8, dst.as_mut_ptr(), 16, 32, 16, 1) };
        assert_eq!(
            1, rc,
            "stbir_resize_uint8 must succeed on a valid in-memory resize"
        );

        let row: Vec<u8> = dst[0..16].to_vec();
        assert!(
            row.windows(2).all(|w| w[0] <= w[1]),
            "a 2x upsample of a monotonic row must stay monotonic, got {row:?}"
        );
        let source_values: std::collections::HashSet<u8> = src.iter().copied().collect();
        assert!(
            dst.iter().any(|b| !source_values.contains(b)),
            "output contains only source sample values — the resampler did not interpolate"
        );
    }

    /// The 4-channel (BGRA) side of the ABI: `num_channels = 4` selects stb's
    /// interleaved-channel code path, with `input_stride_in_bytes = w * 4`. The
    /// resampler is defined to treat channels independently (`alpha_channel = -1`
    /// inside `stbir_resize_uint8` forces `ALPHA_USES_COLORSPACE |
    /// ALPHA_PREMULTIPLIED`, so no channel is special-cased), so a 4-channel resize
    /// must equal four separate 1-channel resizes of the deinterleaved planes,
    /// byte-for-byte. That pins the declaration's stride/`num_channels` argument
    /// order: swapping either produces cross-channel bleed and fails here.
    ///
    /// NOTE ON THE FAILURE ARM (measured, WS11): the `== 0` return that the freetype
    /// face maps to `Error::GlyphResizeFailed` (oracle `freetype.zig:649-652`) is NOT
    /// reachable from Rust with a valid buffer. `stb_image_resize.h` guards invalid
    /// parameters with `STBIR_ASSERT` (= `assert`), and `cc` does not define `NDEBUG`
    /// for our build (`nm -u` on `librustty_stb_image_resize.a` shows
    /// `___assert_rtn`), so `num_channels` out of range aborts, `output_h == 0` trips
    /// the ring-buffer assert, and `output_w == 0` is outright UB (SIGBUS) — none of
    /// them return 0. The only `return 0` reachable in practice is `STBIR_MALLOC`
    /// failure. Hence no negative-return test: it would be untrue.
    #[test]
    fn stbir_resize_uint8_treats_channels_independently() {
        // Four distinct 8x16 planes, interleaved BGRA-style.
        let planes: Vec<Vec<u8>> = (0..4u32)
            .map(|c| {
                (0..16u32)
                    .flat_map(|y| (0..8u32).map(move |x| ((x * 31 + y * 17 + c * 61) & 0xff) as u8))
                    .collect()
            })
            .collect();
        let mut interleaved = vec![0u8; 8 * 16 * 4];
        for i in 0..8 * 16 {
            for c in 0..4 {
                interleaved[i * 4 + c] = planes[c][i];
            }
        }

        let (ow, oh) = (16usize, 32usize);
        let mut dst4 = vec![0u8; ow * oh * 4];
        let rc = unsafe {
            stbir_resize_uint8(
                interleaved.as_ptr(),
                8,
                16,
                8 * 4,
                dst4.as_mut_ptr(),
                ow as i32,
                oh as i32,
                (ow * 4) as i32,
                4,
            )
        };
        assert_eq!(1, rc, "4-channel resize must succeed");

        for c in 0..4usize {
            let mut dst1 = vec![0u8; ow * oh];
            let rc = unsafe {
                stbir_resize_uint8(
                    planes[c].as_ptr(),
                    8,
                    16,
                    8,
                    dst1.as_mut_ptr(),
                    ow as i32,
                    oh as i32,
                    ow as i32,
                    1,
                )
            };
            assert_eq!(1, rc, "1-channel resize of plane {c} must succeed");
            let deinterleaved: Vec<u8> = (0..ow * oh).map(|i| dst4[i * 4 + c]).collect();
            assert_eq!(
                dst1, deinterleaved,
                "channel {c}: interleaved 4-channel resize must equal the 1-channel resize \
                 of the same plane"
            );
        }
    }
}
