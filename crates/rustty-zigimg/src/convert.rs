//! Pixel-format conversion — a faithful mirror of the TWO arms of `zigimg`
//! `src/PixelFormatConverter.zig` that `libvaxis` can reach.
//!
//! **CORRECTION TO THE WS12 PLAN.** The plan (§Option Z-C) described this as
//! "rgba32→rgb24 drops every 4th byte; rgb24→rgba32 inserts `0xFF`". The second half is
//! right; **the first half is wrong** and was corrected against the reference before
//! porting. `PixelFormatConverter.zig:460` dispatches
//! `RgbaColorToRgbColor(.rgba32, .rgb24).convert`, whose per-pixel body is
//! `rgbaToRgb` (`:663`):
//!
//! ```zig
//! fn rgbaToRgb(comptime T: type, rgba: anytype) T {
//!     return T.from.color(rgba.to.premultipliedAlpha());
//! }
//! ```
//!
//! so the alpha channel is **PREMULTIPLIED INTO** R/G/B, not discarded. For a fully
//! opaque image (`a == 255`) the two are indistinguishable, which is exactly why a lazy
//! test would never catch the difference — so `premultiply_is_not_an_alpha_drop` below
//! pins a translucent pixel.
//!
//! The arithmetic, from `color.zig`:
//! - `toF32(x)` for `u8` is `x / 255.0` (`ScaleValue(f32)`, `color.zig:85+`);
//! - `scaleRed(v: f32)` for `u8` is
//!   `@intFromFloat(clamp(@round(v * 255.0), 0.0, 255.0))` (`color.zig:66-70`).
//!
//! Composed, `premultipliedAlpha` (`color.zig:480-505`) then `from.color`
//! (`color.zig:158-168`, a u8→u8 identity scale) gives, per channel:
//!
//! ```text
//! out = round( (c / 255) * (a / 255) * 255 )     in f32
//! ```
//!
//! reproduced verbatim below in `f32` — NOT as an integer approximation such as
//! `(c * a + 127) / 255`, which differs from `f32` rounding on some inputs. Fidelity
//! here is cheap; guessing is not.

/// `zigimg` `PixelFormatConverter.zig:460` — `rgba32` → `rgb24`.
///
/// Per pixel: premultiply alpha into R/G/B (see the module header), then keep R,G,B.
/// The alpha channel itself is dropped only *after* being folded into the colour, which
/// is what "the reference does not composite against a background" means: a pixel with
/// `a == 0` becomes black, not the backdrop.
pub fn rgba32_to_rgb24(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() / 4 * 3);
    for px in pixels.chunks_exact(4) {
        let a = f32::from(px[3]) / 255.0;
        out.push(premultiply_channel(px[0], a));
        out.push(premultiply_channel(px[1], a));
        out.push(premultiply_channel(px[2], a));
    }
    out
}

/// One channel of `color.zig`'s `premultipliedAlpha` followed by the u8→u8
/// `from.color` scale: `round((c/255) * alpha * 255)`, clamped to `0..=255`.
#[inline]
fn premultiply_channel(c: u8, alpha: f32) -> u8 {
    let v = (f32::from(c) / 255.0) * alpha;
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// `zigimg` `PixelFormatConverter.zig:433` — `rgb24` → `rgba32`.
///
/// `rgbToRgba` (`:659`) is `T.from.color(rgb)`; the source has no `a` field, so
/// `from.color` (`color.zig:158-168`) leaves `res.a` at the `RgbaColor(u8)` field default,
/// which is `std.math.maxInt(u8)` = **255** (`color.zig:591-595`). Hence the inserted
/// `0xFF`: it is the struct default, not an arbitrary choice.
pub fn rgb24_to_rgba32(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() / 3 * 4);
    for px in pixels.chunks_exact(3) {
        out.push(px[0]);
        out.push(px[1]);
        out.push(px[2]);
        out.push(0xFF);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb24_to_rgba32_inserts_the_opaque_struct_default() {
        assert_eq!(
            rgb24_to_rgba32(&[1, 2, 3, 4, 5, 6]),
            vec![1, 2, 3, 0xFF, 4, 5, 6, 0xFF]
        );
    }

    #[test]
    fn rgba32_to_rgb24_is_identity_on_opaque_pixels() {
        // a == 255 => premultiply by 1.0 => the colour survives unchanged. This is the
        // ONLY case a naive "drop every 4th byte" port would also get right.
        assert_eq!(
            rgba32_to_rgb24(&[10, 20, 30, 255, 40, 50, 60, 255]),
            vec![10, 20, 30, 40, 50, 60]
        );
    }

    /// THE NEGATIVE CONTROL for the plan's error. A "drop every 4th byte" implementation
    /// passes every opaque test above and fails this one.
    #[test]
    fn premultiply_is_not_an_alpha_drop() {
        // Half-transparent white: a = 128 => alpha = 128/255 = 0.50196…
        //   round(255/255 * 0.50196… * 255) = round(128.0) = 128
        assert_eq!(rgba32_to_rgb24(&[255, 255, 255, 128]), vec![128, 128, 128]);
        // Fully transparent: every channel collapses to 0 (NOT preserved, and NOT
        // composited against any backdrop).
        assert_eq!(rgba32_to_rgb24(&[200, 100, 50, 0]), vec![0, 0, 0]);
        // A mid case with distinct channels, so a transposition would also be caught:
        //   alpha = 64/255 = 0.25098…
        //   r: round(200/255 * 0.25098… * 255) = round(50.196…) = 50
        //   g: round(100/255 * 0.25098… * 255) = round(25.098…) = 25
        //   b: round( 50/255 * 0.25098… * 255) = round(12.549…) = 13
        assert_eq!(rgba32_to_rgb24(&[200, 100, 50, 64]), vec![50, 25, 13]);
    }

    #[test]
    fn round_trip_rgb24_through_rgba32_is_lossless() {
        // rgb24 -> rgba32 sets a = 255, and premultiplying by 1.0 is the identity, so
        // the round trip must be exact. (It would NOT be if either arm were wrong.)
        let src: Vec<u8> = (0u8..=254).collect();
        let src = &src[..(src.len() / 3) * 3];
        assert_eq!(rgba32_to_rgb24(&rgb24_to_rgba32(src)), src);
    }

    #[test]
    fn conversions_produce_the_right_lengths() {
        assert_eq!(rgba32_to_rgb24(&[0; 4 * 7]).len(), 3 * 7);
        assert_eq!(rgb24_to_rgba32(&[0; 3 * 7]).len(), 4 * 7);
    }
}
