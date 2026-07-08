//! PORTS: ghostty pkg/wuffs/src/swizzle.zig — pixel-format swizzlers.
//!
//! PORT STATUS: R5 seam (deferred). swizzle is NOT on the background-image decode
//! path (`decode_frame` + `set_from_slice` with `RGBA_NONPREMUL` performs the
//! format conversion internally), so per RENDERER_BG_IMAGE_DESIGN §2.2 it is
//! ported as `unimplemented!()` seams with the verbatim reference body preserved.
//! The Kitty image path uses the renderer-local `image::convert` (also a seam)
//! rather than these; a caller (a future non-RGBA image ingest) lands the bodies.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Allocator;
// const assert = std.debug.assert;
// const c = @import("c.zig").c;
// const Error = @import("error.zig").Error;
use crate::{Error, RawBuf};

// const log = std.log.scoped(.wuffs_swizzler);

// pub fn gToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__Y, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn gToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    let _ = (alloc, src);
    unimplemented!(
        "wuffs.swizzle.gToRgba — R5 seam (not on the bg-image decode path; \
         RENDERER_BG_IMAGE_DESIGN §2.2). Verbatim body: return swizzle(alloc, src, \
         WUFFS_BASE__PIXEL_FORMAT__Y, WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);"
    )
}

// pub fn gaToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__YA_PREMUL, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn gaToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    let _ = (alloc, src);
    unimplemented!(
        "wuffs.swizzle.gaToRgba — R5 seam. Verbatim body: return swizzle(alloc, \
         src, WUFFS_BASE__PIXEL_FORMAT__YA_PREMUL, WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);"
    )
}

// pub fn rgbToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__RGB, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn rgbToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    let _ = (alloc, src);
    unimplemented!(
        "wuffs.swizzle.rgbToRgba — R5 seam. Verbatim body: return swizzle(alloc, \
         src, WUFFS_BASE__PIXEL_FORMAT__RGB, WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);"
    )
}

// pub fn bgrToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__BGR, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn bgrToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    let _ = (alloc, src);
    unimplemented!(
        "wuffs.swizzle.bgrToRgba — R5 seam. Verbatim body: return swizzle(alloc, \
         src, WUFFS_BASE__PIXEL_FORMAT__BGR, WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);"
    )
}

// pub fn bgraToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__BGRA_PREMUL, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn bgraToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    let _ = (alloc, src);
    unimplemented!(
        "wuffs.swizzle.bgraToRgba — R5 seam. Verbatim body: return swizzle(alloc, \
         src, WUFFS_BASE__PIXEL_FORMAT__BGRA_PREMUL, WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);"
    )
}

// fn swizzle(
//     alloc: Allocator,
//     src: []const u8,
//     comptime src_pixel_format: u32,
//     comptime dst_pixel_format: u32,
// ) Error![]u8 {
//     const src_slice = c.wuffs_base__make_slice_u8(@constCast(src.ptr), src.len);
//     const dst_fmt = c.wuffs_base__make_pixel_format(dst_pixel_format);
//     assert(c.wuffs_base__pixel_format__is_direct(&dst_fmt));
//     assert(c.wuffs_base__pixel_format__is_interleaved(&dst_fmt));
//     assert(c.wuffs_base__pixel_format__bits_per_pixel(&dst_fmt) % 8 == 0);
//     const dst_size = c.wuffs_base__pixel_format__bits_per_pixel(&dst_fmt) / 8;
//     const src_fmt = c.wuffs_base__make_pixel_format(src_pixel_format);
//     assert(c.wuffs_base__pixel_format__is_direct(&src_fmt));
//     assert(c.wuffs_base__pixel_format__is_interleaved(&src_fmt));
//     assert(c.wuffs_base__pixel_format__bits_per_pixel(&src_fmt) % 8 == 0);
//     const src_size = c.wuffs_base__pixel_format__bits_per_pixel(&src_fmt) / 8;
//     assert(src.len % src_size == 0);
//     const dst = try alloc.alloc(u8, src.len * dst_size / src_size);
//     errdefer alloc.free(dst);
//     const dst_slice = c.wuffs_base__make_slice_u8(dst.ptr, dst.len);
//     var swizzler: c.wuffs_base__pixel_swizzler = undefined;
//     {
//         const status = c.wuffs_base__pixel_swizzler__prepare(
//             &swizzler, dst_fmt, c.wuffs_base__empty_slice_u8(),
//             src_fmt, c.wuffs_base__empty_slice_u8(), c.WUFFS_BASE__PIXEL_BLEND__SRC);
//         if (!c.wuffs_base__status__is_ok(&status)) {
//             const e = c.wuffs_base__status__message(&status);
//             log.warn("{s}", .{e});
//             return error.WuffsError;
//         }
//     }
//     {
//         _ = c.wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice(
//             &swizzler, dst_slice, c.wuffs_base__empty_slice_u8(), src_slice);
//     }
//     return dst;
// }
//
// R5 SEAM: the private `swizzle` body is preserved verbatim above; it lands with
// the first `*ToRgba` caller (a non-RGBA image ingest path).
