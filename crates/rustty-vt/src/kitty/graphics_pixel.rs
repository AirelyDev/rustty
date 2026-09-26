//! Pixel-buffer operations for the Kitty graphics protocol: format
//! conversion to RGBA, background fill, and rectangle composition.
//! These are the primitives behind animation frame loading (a=f) and
//! frame composition (a=c). See graphics_animation.rs for the
//! animation model built on top of them.
//!
//! rustty-vt kitty/graphics_pixel
//!
//! PORTS: ghostty src/terminal/kitty/graphics_pixel.zig (the `322d7ae78`
//! wuffs-backed END STATE; the 3-day z2d/hand-rolled intermediate from
//! `d8b920e50` is range-diff superseded and was never ported)
//!
//! PORT STATUS: ported
//
// PORT NOTE (wuffs edge): upstream wires wuffs into the vt module whenever
// Kitty graphics is enabled (`322d7ae78` GhosttyZig.zig), including the
// standalone lib build. rustty's wuffs binding rides the OPTIONAL `wuffs`
// cargo feature (W1 decision; G-VTCLIB pins the isolated `-p rustty-vt-c`
// graph wuffs-free for the decode_png default). A kitty-graphics build
// without wuffs is therefore expressible here but not upstream; for that
// configuration the swizzle leaves below fall back to byte-exact scalar
// mirrors of the vendored wuffs kernels (wuffs-v0.4.c: the trivial
// swizzle expansions and `composite_nonpremul_nonpremul_u32_axxx`), pinned
// equivalent by `wuffs_and_scalar_src_over_agree` under `--features wuffs`.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const wuffs = @import("wuffs");
// const command = @import("graphics_command.zig");
use crate::kitty::graphics_command as command;
use crate::kitty::graphics_storage::OutOfMemory;

/// Convert pixel data in the given format to a freshly allocated RGBA
/// buffer. The caller owns the result; the input is not freed.
///
/// The input length must be a multiple of the format's bytes per
/// pixel; image loading validates data length against the image
/// dimensions before storing it.
//
// PORT NOTE: `Allocator.Error![]u8` is `Result<Vec<u8>, OutOfMemory>`, and
// every allocation below is a `try_reserve_exact` so a real allocation failure
// returns `Err` as the reference's does.
// CORRECTION (parity S190-F2): this was infallible (`Vec::with_capacity`,
// `to_vec`), which made the exec callers' ENOMEM arms and
// `ImageStorage::convert_image_to_rgba`'s `try` unreachable.
pub fn rgba_from_format(format: command::Format, data: &[u8]) -> Result<Vec<u8>, OutOfMemory> {
    // PORT NOTE: port-local test fail point standing in for a failing
    // allocator (see `RGBA_FROM_FORMAT_TW`).
    #[cfg(test)]
    RGBA_FROM_FORMAT_TW.with(|tw| tw.borrow_mut().check(RgbaFromFormatFp::Alloc))?;

    #[cfg(feature = "wuffs")]
    {
        let result = match format {
            // .rgba => return try alloc.dupe(u8, data),
            command::Format::Rgba => return try_dupe(data),
            command::Format::Rgb => rustty_wuffs::swizzle::rgbToRgba(GLOBAL, data),
            command::Format::Gray => rustty_wuffs::swizzle::gToRgba(GLOBAL, data),
            command::Format::GrayAlpha => rustty_wuffs::swizzle::gaToRgba(GLOBAL, data),

            // PNG is decoded to RGBA during image loading.
            command::Format::Png => unreachable!(),
        };

        // return result catch |err| switch (err) {
        //     error.OutOfMemory => error.OutOfMemory,
        //     // These are fixed, supported swizzles; wuffs cannot fail to
        //     // prepare them and nothing else in the conversion errors.
        //     error.WuffsError, error.Overflow => unreachable,
        // };
        // PORT NOTE: the RawBuf is copied into an owning Vec; PERF(port):
        // extra copy vs the reference's direct `[]u8` handoff.
        match result {
            Ok(buf) => try_dupe(buf.as_slice()),
            Err(rustty_wuffs::Error::OutOfMemory) => Err(OutOfMemory),
            Err(rustty_wuffs::Error::WuffsError | rustty_wuffs::Error::Overflow) => unreachable!(),
        }
    }

    #[cfg(not(feature = "wuffs"))]
    {
        // Byte-exact scalar mirrors of the wuffs swizzles (see the module
        // PORT NOTE): RGB/G expand with alpha forced opaque, GA replicates
        // the gray channel and passes alpha through — exactly the vendored
        // kernels' outputs, pinned by the `rgba conversion` test vectors in
        // both cfgs.
        match format {
            command::Format::Rgba => try_dupe(data), // try alloc.dupe(u8, data)
            command::Format::Rgb => {
                let pixels = data.len() / 3;
                let mut result = try_with_capacity(pixels * 4)?;
                for px in data.chunks_exact(3) {
                    result.extend_from_slice(&[px[0], px[1], px[2], 255]);
                }
                Ok(result)
            }
            command::Format::Gray => {
                let mut result = try_with_capacity(data.len() * 4)?;
                for &v in data {
                    result.extend_from_slice(&[v, v, v, 255]);
                }
                Ok(result)
            }
            command::Format::GrayAlpha => {
                let pixels = data.len() / 2;
                let mut result = try_with_capacity(pixels * 4)?;
                for px in data.chunks_exact(2) {
                    result.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
                }
                Ok(result)
            }

            // PNG is decoded to RGBA during image loading.
            command::Format::Png => unreachable!(),
        }
    }
}

/// A `Vec` with room for exactly `n` bytes, or `OutOfMemory`.
fn try_with_capacity(n: usize) -> Result<Vec<u8>, OutOfMemory> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| OutOfMemory)?;
    Ok(v)
}

/// `alloc.dupe(u8, data)`.
fn try_dupe(data: &[u8]) -> Result<Vec<u8>, OutOfMemory> {
    let mut v = try_with_capacity(data.len())?;
    v.extend_from_slice(data);
    Ok(v)
}

// PORT NOTE (parity S190-F2): the reference reaches its OutOfMemory arms with a
// failing allocator; the port's kitty tier allocates on the global heap, so a
// `cfg(test)` fail point at the allocation stands in, the same stand-in
// `screen.rs`'s `CURSOR_COPY_TW` uses.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum RgbaFromFormatFp {
    Alloc,
}

#[cfg(test)]
thread_local! {
    pub(crate) static RGBA_FROM_FORMAT_TW: core::cell::RefCell<
        rustty_util::tripwire::Module<RgbaFromFormatFp, OutOfMemory>,
    > = core::cell::RefCell::new(rustty_util::tripwire::module());
}

/// The allocator handed to the wuffs swizzles (the reference passes the
/// caller's `Allocator`; rustty-vt's kitty tier is Global-allocated).
#[cfg(feature = "wuffs")]
const GLOBAL: crate::alloc::Global = crate::alloc::Global;

/// Fill an RGBA buffer with the given background color.
pub fn fill_background(data: &mut [u8], bg: command::Background) {
    // const raw: u32 = @bitCast(bg);
    let raw: u32 = bg.to_int();
    if raw == 0 {
        // @memset(data, 0);
        data.fill(0);
        return;
    }
    let mut i: usize = 0;
    while i + 4 <= data.len() {
        data[i] = bg.r;
        data[i + 1] = bg.g;
        data[i + 2] = bg.b;
        data[i + 3] = bg.a;
        i += 4;
    }
}

/// Compose the `src` rectangle (src_width x src_height RGBA pixels)
/// onto the `dst` canvas (dst_width x dst_height RGBA pixels) with
/// its top-left corner at (x, y). Portions of the rectangle outside
/// the canvas are silently clipped, matching Kitty's a=f behavior.
#[allow(clippy::too_many_arguments)]
pub fn compose_rect(
    dst: &mut [u8],
    dst_width: u32,
    dst_height: u32,
    src: &[u8],
    src_width: u32,
    src_height: u32,
    x: u32,
    y: u32,
    mode: command::CompositionMode,
) {
    if x >= dst_width || y >= dst_height {
        return;
    }
    let width: usize = core::cmp::min(src_width, dst_width - x) as usize;
    let height: usize = core::cmp::min(src_height, dst_height - y) as usize;
    for row in 0..height {
        let dst_off = ((y as usize + row) * dst_width as usize + x as usize) * 4;
        let src_off = row * src_width as usize * 4;
        compose_row(
            &mut dst[dst_off..][..width * 4],
            &src[src_off..][..width * 4],
            mode,
        );
    }
}

/// Compose a width x height rectangle between two full-size RGBA
/// canvases sharing the same `canvas_width` stride, reading from
/// (src_x, src_y) in `src` and writing at (dst_x, dst_y) in `dst`.
/// The caller must have validated that both rectangles are within
/// bounds; a=c reports out-of-bounds rectangles as errors rather
/// than clipping.
//
// PORT NOTE: the reference's single `composeCanvasRect` tolerates `dst` and
// `src` aliasing the SAME canvas (Zig slices carry no exclusivity); Rust
// cannot express that with `&mut [u8]`/`&[u8]`, so the port splits it into
// this two-canvas entry point and `compose_canvas_rect_in_place` below for
// the same-frame case (a=c with r == c). Same math, same row order.
#[allow(clippy::too_many_arguments)]
pub fn compose_canvas_rect(
    dst: &mut [u8],
    src: &[u8],
    canvas_width: u32,
    width: u32,
    height: u32,
    src_x: u32,
    src_y: u32,
    dst_x: u32,
    dst_y: u32,
    mode: command::CompositionMode,
) {
    for row in 0..height as usize {
        let dst_off = ((dst_y as usize + row) * canvas_width as usize + dst_x as usize) * 4;
        let src_off = ((src_y as usize + row) * canvas_width as usize + src_x as usize) * 4;
        compose_row(
            &mut dst[dst_off..][..width as usize * 4],
            &src[src_off..][..width as usize * 4],
            mode,
        );
    }
}

/// `compose_canvas_rect`, but with the source and destination rectangles in
/// ONE canvas (the reference's aliasing call shape). The caller must have
/// validated the rectangles disjoint (a=c rejects overlapping rectangles on
/// the same frame), which guarantees every per-row source range is disjoint
/// from every destination range, so the split borrows below always succeed.
#[allow(clippy::too_many_arguments)]
pub fn compose_canvas_rect_in_place(
    canvas: &mut [u8],
    canvas_width: u32,
    width: u32,
    height: u32,
    src_x: u32,
    src_y: u32,
    dst_x: u32,
    dst_y: u32,
    mode: command::CompositionMode,
) {
    let row_len = width as usize * 4;
    for row in 0..height as usize {
        let dst_off = ((dst_y as usize + row) * canvas_width as usize + dst_x as usize) * 4;
        let src_off = ((src_y as usize + row) * canvas_width as usize + src_x as usize) * 4;
        // Disjoint 2D rectangles ⇒ the two byte ranges never overlap, so one
        // range lies entirely before the other; split at the later offset.
        debug_assert!(src_off + row_len <= dst_off || dst_off + row_len <= src_off);
        if src_off < dst_off {
            let (head, tail) = canvas.split_at_mut(dst_off);
            compose_row(&mut tail[..row_len], &head[src_off..][..row_len], mode);
        } else {
            let (head, tail) = canvas.split_at_mut(src_off);
            compose_row(&mut head[dst_off..][..row_len], &tail[..row_len], mode);
        }
    }
}

fn compose_row(dst: &mut [u8], src: &[u8], mode: command::CompositionMode) {
    match mode {
        // .overwrite => @memcpy(dst, src),
        command::CompositionMode::Overwrite => dst.copy_from_slice(src),
        // .alpha_blend => wuffs.swizzle.rgbaSrcOver(dst, src),
        command::CompositionMode::AlphaBlend => rgba_src_over(dst, src),
    }
}

/// `wuffs.swizzle.rgbaSrcOver(dst, src)`: composite `src` over `dst` in
/// place. Both are straight (non-premultiplied) alpha RGBA of the same
/// length. A transparent destination pixel takes the source pixel exactly;
/// wuffs composites everything else in 16-bit integer space.
#[cfg(feature = "wuffs")]
fn rgba_src_over(dst: &mut [u8], src: &[u8]) {
    rustty_wuffs::swizzle::rgbaSrcOver(dst, src);
}

/// Byte-exact scalar mirror of the wuffs src-over kernel for builds without
/// the wuffs binding (see the module PORT NOTE): per pixel this is the
/// vendored `wuffs_private_impl__composite_nonpremul_nonpremul_u32_axxx`
/// (wuffs-v0.4.c), pinned against the real kernel by the
/// `wuffs_and_scalar_src_over_agree` differential KAT.
// PARITY: `% 4 == 0` kept verbatim (the rustty-wuffs swizzle.rs precedent for
// `rgbaSrcOver`'s own asserts).
#[allow(clippy::manual_is_multiple_of)]
#[cfg(not(feature = "wuffs"))]
fn rgba_src_over(dst: &mut [u8], src: &[u8]) {
    assert!(dst.len() == src.len());
    assert!(dst.len() % 4 == 0);
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let out =
            composite_nonpremul_nonpremul_axxx([d[0], d[1], d[2], d[3]], [s[0], s[1], s[2], s[3]]);
        d.copy_from_slice(&out);
    }
}

/// One pixel of the wuffs nonpremul src-over composite, channel-symmetric
/// over the three color lanes with alpha in lane 3 (the RGBA byte order).
#[cfg(any(not(feature = "wuffs"), test))]
fn composite_nonpremul_nonpremul_axxx(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    // Extract 16-bit color components.
    //
    // If the destination is transparent then SRC_OVER is equivalent to SRC:
    // just return src. This isn't just an optimization (skipping the rest of
    // the function's computation). It also preserves the nonpremul
    // distinction between e.g. transparent red and transparent blue that
    // would otherwise be lost by converting from nonpremul to premul and
    // back. (Comment from the vendored wuffs kernel.)
    let mut da: u32 = 0x101 * dst[3] as u32;
    if da == 0 {
        return src;
    }
    let sa: u32 = 0x101 * src[3] as u32;

    // Calculate the inverse of the src-alpha: how much of the dst to keep.
    let ia: u32 = 0xFFFF - sa;

    let mut out = [0u8; 4];
    for c in 0..3 {
        let dc: u32 = 0x101 * dst[c] as u32;
        let sc: u32 = 0x101 * src[c] as u32;
        // Convert dst from nonpremul to premul.
        let dc_premul = (dc * da) / 0xFFFF;
        // Composite src (nonpremul) over dst (premul).
        let mut composed = ((sc * sa) + (dc_premul * ia)) / 0xFFFF;
        // Convert dst from premul to nonpremul.
        let out_a = sa + ((da * ia) / 0xFFFF);
        if out_a != 0 {
            composed = (composed * 0xFFFF) / out_a;
        }
        // Convert from 16-bit color to 8-bit color.
        out[c] = (composed >> 8) as u8;
    }
    da = sa + ((da * ia) / 0xFFFF);
    out[3] = (da >> 8) as u8;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "rgba conversion"
    #[test]
    fn rgba_conversion() {
        {
            let rgb = [1u8, 2, 3, 4, 5, 6];
            let result = rgba_from_format(command::Format::Rgb, &rgb).unwrap();
            assert_eq!(&[1, 2, 3, 255, 4, 5, 6, 255], result.as_slice());
        }

        {
            let gray = [7u8, 8];
            let result = rgba_from_format(command::Format::Gray, &gray).unwrap();
            assert_eq!(&[7, 7, 7, 255, 8, 8, 8, 255], result.as_slice());
        }

        {
            let ga = [7u8, 100, 8, 200];
            let result = rgba_from_format(command::Format::GrayAlpha, &ga).unwrap();
            assert_eq!(&[7, 7, 7, 100, 8, 8, 8, 200], result.as_slice());
        }

        {
            let rgba = [1u8, 2, 3, 4];
            let result = rgba_from_format(command::Format::Rgba, &rgba).unwrap();
            assert_eq!(&rgba, result.as_slice());
        }
    }

    // test "fill background"
    #[test]
    fn fill_background_test() {
        let mut buf = [0u8; 8];
        fill_background(
            &mut buf,
            command::Background {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
        );
        assert_eq!(&[1, 2, 3, 4, 1, 2, 3, 4], &buf);

        fill_background(&mut buf, command::Background::default());
        assert_eq!(&[0u8; 8], &buf);
    }

    // test "compose rect overwrite with clipping"
    #[test]
    fn compose_rect_overwrite_with_clipping() {
        // 2x2 canvas, compose a 2x1 rect at (1, 1): only the first pixel
        // of the rect fits, the rest clips off the right edge.
        let mut dst = [0u8; 16];
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8];
        compose_rect(
            &mut dst,
            2,
            2,
            &src,
            2,
            1,
            1,
            1,
            command::CompositionMode::Overwrite,
        );

        let mut expect = [0u8; 16];
        expect[12] = 1;
        expect[13] = 2;
        expect[14] = 3;
        expect[15] = 4;
        assert_eq!(&expect, &dst);
    }

    // test "compose rect entirely out of bounds"
    #[test]
    fn compose_rect_entirely_out_of_bounds() {
        let mut dst = [9u8; 16];
        let src = [1u8; 4];
        compose_rect(
            &mut dst,
            2,
            2,
            &src,
            1,
            1,
            2,
            0,
            command::CompositionMode::Overwrite,
        );
        compose_rect(
            &mut dst,
            2,
            2,
            &src,
            1,
            1,
            0,
            2,
            command::CompositionMode::Overwrite,
        );
        assert_eq!(&[9u8; 16], &dst);
    }

    // test "alpha blend source-over semantics"
    #[test]
    fn alpha_blend_source_over_semantics() {
        // Opaque source overwrites exactly.
        {
            let mut dst = [10u8, 20, 30, 40];
            compose_rect(
                &mut dst,
                1,
                1,
                &[100, 110, 120, 255],
                1,
                1,
                0,
                0,
                command::CompositionMode::AlphaBlend,
            );
            assert_eq!(&[100, 110, 120, 255], &dst);
        }

        // Fully transparent source leaves the destination untouched
        // (like Kitty; see the module doc on rounding).
        {
            let mut dst = [10u8, 20, 30, 40];
            compose_rect(
                &mut dst,
                1,
                1,
                &[100, 110, 120, 0],
                1,
                1,
                0,
                0,
                command::CompositionMode::AlphaBlend,
            );
            assert_eq!(&[10, 20, 30, 40], &dst);
        }

        // 50% source over opaque destination.
        {
            let mut dst = [0u8, 0, 0, 255];
            compose_rect(
                &mut dst,
                1,
                1,
                &[255, 255, 255, 128],
                1,
                1,
                0,
                0,
                command::CompositionMode::AlphaBlend,
            );
            assert_eq!(255u8, dst[3]);
            assert_eq!(128u8, dst[0]);
        }

        // Blending over a transparent destination yields the source
        // exactly, matching Kitty: wuffs passes the source through
        // untouched when the destination is transparent.
        {
            let mut dst = [0u8, 0, 0, 0];
            compose_rect(
                &mut dst,
                1,
                1,
                &[200, 100, 50, 128],
                1,
                1,
                0,
                0,
                command::CompositionMode::AlphaBlend,
            );
            assert_eq!(&[200, 100, 50, 128], &dst);
        }
    }

    // test "compose canvas rect"
    #[test]
    fn compose_canvas_rect_test() {
        // 3x1 canvas: copy pixel at x=0 onto x=2.
        //
        // PORT NOTE: the reference calls `composeCanvasRect(&canvas, &canvas,
        // …)` — the aliasing shape — which the port spells
        // `compose_canvas_rect_in_place` (see its PORT NOTE).
        let mut canvas = [1u8, 2, 3, 4, 0, 0, 0, 0, 9, 9, 9, 9];
        compose_canvas_rect_in_place(
            &mut canvas,
            3,
            1,
            1,
            0,
            0,
            2,
            0,
            command::CompositionMode::Overwrite,
        );
        assert_eq!(&[1, 2, 3, 4, 0, 0, 0, 0, 1, 2, 3, 4], &canvas);
    }

    // Differential KAT (rustty addition; see the module PORT NOTE): the
    // scalar mirror of the wuffs src-over kernel must agree with the real
    // FFI kernel byte-for-byte. Runs only when both arms are available.
    #[cfg(feature = "wuffs")]
    #[test]
    fn wuffs_and_scalar_src_over_agree() {
        // Sweep alpha lanes exhaustively-ish and color lanes coarsely; every
        // (dst, src) pair must match the FFI kernel exactly.
        let lanes: [u8; 6] = [0, 1, 127, 128, 254, 255];
        for &da in &lanes {
            for &sa in &lanes {
                for &dc in &lanes {
                    for &sc in &lanes {
                        let dst = [dc, dc.wrapping_add(1), dc / 2, da];
                        let src = [sc, sc / 3, sc.wrapping_mul(2), sa];
                        let scalar = composite_nonpremul_nonpremul_axxx(dst, src);
                        let mut ffi = dst;
                        rustty_wuffs::swizzle::rgbaSrcOver(&mut ffi, &src);
                        assert_eq!(scalar, ffi, "dst={dst:?} src={src:?}");
                    }
                }
            }
        }
    }
}
