//! PORTS: ghostty pkg/wuffs/src/jpeg.zig — the safe JPEG `decode`.
//!
//! PORT STATUS: ported (every line/comment/test{}, R1).
//!
//! PORT NOTE: this is `png.zig`'s body VERBATIM bar the `png`->`jpeg` decoder
//! symbols (the reference is two duplicated files; kept duplicated for fidelity).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Allocator;
// const c = @import("c.zig").c;
use wuffs_sys::{
    rustty_wuffs_pixel_buffer__set_from_slice, rustty_wuffs_pixel_config__height,
    rustty_wuffs_pixel_config__set, rustty_wuffs_pixel_config__width, sizeof__wuffs_jpeg__decoder,
    wuffs_base__color_u32_argb_premul, wuffs_base__image_config, wuffs_base__io_buffer,
    wuffs_base__io_buffer_meta, wuffs_base__pixel_buffer, wuffs_base__slice_u8,
    wuffs_jpeg__decoder, wuffs_jpeg__decoder__decode_frame,
    wuffs_jpeg__decoder__decode_image_config, wuffs_jpeg__decoder__initialize,
    wuffs_jpeg__decoder__workbuf_len, WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL,
    WUFFS_BASE__PIXEL_SUBSAMPLING__NONE, WUFFS_VERSION,
};
// const Error = @import("error.zig").Error;
// const check = @import("error.zig").check;
use crate::error::{check, Error};
// const ImageData = @import("main.zig").ImageData;
// const maximum_image_size = @import("main.zig").maximum_image_size;
use crate::{ImageData, RawBuf, Scratch, MAXIMUM_IMAGE_SIZE};
// const mul = std.math.mul;

// const log = std.log.scoped(.wuffs_jpeg);

/// Decode a JPEG image.
// pub fn decode(alloc: Allocator, data: []const u8) Error!ImageData {
pub fn decode<A: Allocator + Copy>(alloc: A, data: &[u8]) -> Result<ImageData<A>, Error> {
    // Work around some weirdness in WUFFS/Zig, there are some structs that
    // are defined as "extern" by the Zig compiler which means that Zig won't
    // allocate them on the stack at compile time. WUFFS has functions for
    // dynamically allocating these structs but they use the C malloc/free. This
    // gets around that by using the Zig allocator to allocate enough memory for
    // the struct and then casts it to the appropriate pointer.

    // const decoder_buf = try alloc.alloc(u8, c.sizeof__wuffs_jpeg__decoder());
    // defer alloc.free(decoder_buf);
    //
    // PORT NOTE (u64-aligned backing — the wuffs-sys KAT precedent).
    // SAFETY: FFI call to a pure size accessor (no receiver).
    let decoder_size = unsafe { sizeof__wuffs_jpeg__decoder() };
    let decoder_buf = Scratch::alloc(alloc, decoder_size, 8)?;

    // const decoder: ?*c.wuffs_jpeg__decoder = @ptrCast(decoder_buf);
    let decoder = decoder_buf.ptr() as *mut wuffs_jpeg__decoder;
    {
        // const status = c.wuffs_jpeg__decoder__initialize(decoder, c.sizeof__wuffs_jpeg__decoder(), c.WUFFS_VERSION, 0);
        // SAFETY: `decoder` points at `decoder_size` zeroed, 8-aligned bytes.
        let status = unsafe {
            wuffs_jpeg__decoder__initialize(decoder, decoder_size, WUFFS_VERSION as u64, 0)
        };
        // try check(log, &status);
        check(&status)?;
    }

    // var source_buffer: c.wuffs_base__io_buffer = .{
    //     .data = .{ .ptr = @ptrCast(@constCast(data.ptr)), .len = data.len },
    //     .meta = .{ .wi = data.len, .ri = 0, .pos = 0, .closed = true },
    // };
    let mut source_buffer = wuffs_base__io_buffer {
        data: wuffs_base__slice_u8 {
            ptr: data.as_ptr() as *mut u8,
            len: data.len(),
        },
        meta: wuffs_base__io_buffer_meta {
            wi: data.len(),
            ri: 0,
            pos: 0,
            closed: true,
        },
    };

    // var image_config: c.wuffs_base__image_config = undefined;
    // SAFETY: `undefined` -> zeroed POD; `decode_image_config` fully initializes it.
    let mut image_config: wuffs_base__image_config = unsafe { core::mem::zeroed() };
    {
        // const status = c.wuffs_jpeg__decoder__decode_image_config(decoder, &image_config, &source_buffer);
        // SAFETY: `decoder` is initialized; the out-config + io-buffer pointers are valid.
        let status = unsafe {
            wuffs_jpeg__decoder__decode_image_config(decoder, &mut image_config, &mut source_buffer)
        };
        // try check(log, &status);
        check(&status)?;
    }

    // const width = c.wuffs_base__pixel_config__width(&image_config.pixcfg);
    // const height = c.wuffs_base__pixel_config__height(&image_config.pixcfg);
    // SAFETY: `image_config` is initialized; the shims are pure accessors over `pixcfg`.
    let width = unsafe { rustty_wuffs_pixel_config__width(&image_config.pixcfg) };
    let height = unsafe { rustty_wuffs_pixel_config__height(&image_config.pixcfg) };

    // c.wuffs_base__pixel_config__set(&image_config.pixcfg, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL, c.WUFFS_BASE__PIXEL_SUBSAMPLING__NONE, width, height);
    // SAFETY: `pixcfg` is a valid, initialized pixel config; the args are plain u32.
    unsafe {
        rustty_wuffs_pixel_config__set(
            &mut image_config.pixcfg,
            WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL,
            WUFFS_BASE__PIXEL_SUBSAMPLING__NONE,
            width,
            height,
        );
    }

    // const size: usize = try mul(usize, try mul(usize, width, height), @sizeOf(c.wuffs_base__color_u32_argb_premul));
    let size: usize = (width as usize)
        .checked_mul(height as usize)
        .ok_or(Error::Overflow)?
        .checked_mul(core::mem::size_of::<wuffs_base__color_u32_argb_premul>())
        .ok_or(Error::Overflow)?;

    // if (size > maximum_image_size) {
    //     log.warn("image size {d} is larger than the maximum allowed ({d})", .{ size, maximum_image_size });
    //     return error.Overflow;
    // }
    if size > MAXIMUM_IMAGE_SIZE {
        eprintln!(
            "warning(wuffs_jpeg): image size {size} is larger than the maximum allowed ({MAXIMUM_IMAGE_SIZE})"
        );
        return Err(Error::Overflow);
    }

    // const destination = try alloc.alloc(u8, size);
    // errdefer alloc.free(destination);
    //
    // PORT NOTE: the `errdefer` is subsumed by `RawBuf`'s `Drop` (freed on an early
    // `?`, moved into the returned `ImageData` on success).
    let destination = RawBuf::alloc(alloc, size)?;

    // temporary buffer for intermediate processing of image
    // const work_buffer = try alloc.alloc(u8,
    //     // The type of this is a u64 on all systems but our allocator
    //     // uses a usize which is a u32 on 32-bit systems.
    //     std.math.cast(usize, c.wuffs_jpeg__decoder__workbuf_len(decoder).max_incl) orelse return error.OutOfMemory,
    // );
    // defer alloc.free(work_buffer);
    // SAFETY: `decoder` is initialized; `workbuf_len` is a pure accessor.
    let workbuf_max_incl = unsafe { wuffs_jpeg__decoder__workbuf_len(decoder) }.max_incl;
    let work_len = usize::try_from(workbuf_max_incl).map_err(|_| Error::OutOfMemory)?;
    let work_buffer = Scratch::alloc(alloc, work_len, 1)?;

    // const work_slice = c.wuffs_base__make_slice_u8(work_buffer.ptr, work_buffer.len);
    let work_slice = wuffs_base__slice_u8 {
        ptr: work_buffer.ptr(),
        len: work_buffer.len(),
    };

    // var pixel_buffer: c.wuffs_base__pixel_buffer = undefined;
    // SAFETY: `undefined` -> zeroed POD; `set_from_slice` memsets + initializes it.
    let mut pixel_buffer: wuffs_base__pixel_buffer = unsafe { core::mem::zeroed() };
    {
        // const status = c.wuffs_base__pixel_buffer__set_from_slice(&pixel_buffer, &image_config.pixcfg, c.wuffs_base__make_slice_u8(destination.ptr, destination.len));
        // SAFETY: `pixel_buffer`/`pixcfg` are valid; the destination slice covers `size` owned bytes.
        let status = unsafe {
            rustty_wuffs_pixel_buffer__set_from_slice(
                &mut pixel_buffer,
                &image_config.pixcfg,
                wuffs_base__slice_u8 {
                    ptr: destination.ptr(),
                    len: destination.len(),
                },
            )
        };
        // try check(log, &status);
        check(&status)?;
    }

    {
        // const status = c.wuffs_jpeg__decoder__decode_frame(decoder, &pixel_buffer, &source_buffer, c.WUFFS_BASE__PIXEL_BLEND__SRC, work_slice, null);
        //
        // PORT NOTE: `WUFFS_BASE__PIXEL_BLEND__SRC` is `#define ((wuffs_base__pixel_blend)0)`
        // (a function-style macro bindgen omits) — the literal `0` (the blend enum is a u8).
        // SAFETY: all pointers are valid + initialized; `work_slice` covers the work buffer.
        let status = unsafe {
            wuffs_jpeg__decoder__decode_frame(
                decoder,
                &mut pixel_buffer,
                &mut source_buffer,
                0, // c.WUFFS_BASE__PIXEL_BLEND__SRC
                work_slice,
                core::ptr::null_mut(),
            )
        };
        // try check(log, &status);
        check(&status)?;
    }

    // return .{ .width = width, .height = height, .data = destination };
    Ok(ImageData {
        width,
        height,
        data: destination,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;

    // test "jpeg_decode_000000" {
    //     const data = try decode(std.testing.allocator, @embedFile("1x1#000000.jpg"));
    //     defer std.testing.allocator.free(data.data);
    //     try std.testing.expectEqual(1, data.width);
    //     try std.testing.expectEqual(1, data.height);
    //     try std.testing.expectEqualSlices(u8, &.{ 0, 0, 0, 255 }, data.data);
    // }
    //
    // PORT NOTE (F5): the reference `1x1#000000.jpg` @embedFile is ABSENT from the
    // checkout; a minimal 1×1 solid-black baseline JPEG fixture stands in (a black
    // pixel decodes exactly: YCbCr(0,128,128) -> RGB(0,0,0)).
    #[test]
    fn jpeg_decode_000000() {
        let data = decode(Global, include_bytes!("../fixtures/1x1_000000.jpg")).unwrap();
        assert_eq!(1, data.width);
        assert_eq!(1, data.height);
        assert_eq!(&[0, 0, 0, 255], data.bytes());
    }

    // test "jpeg_decode_FFFFFF" { ... expectEqualSlices(&.{255,255,255,255}) }
    //
    // PORT NOTE (F5): a white pixel decodes exactly: YCbCr(255,128,128) -> RGB(255,255,255).
    #[test]
    fn jpeg_decode_ffffff() {
        let data = decode(Global, include_bytes!("../fixtures/1x1_FFFFFF.jpg")).unwrap();
        assert_eq!(1, data.width);
        assert_eq!(1, data.height);
        assert_eq!(&[255, 255, 255, 255], data.bytes());
    }

    // test "jpeg: too big" {
    //     const data = decode(std.testing.allocator, @embedFile("too_big.jpg"));
    //     try std.testing.expectError(error.Overflow, data);
    // }
    //
    // PORT NOTE (F5): the 12.5 MB `too_big.jpg` blob is replaced by the minimal
    // black fixture with its SOF0 dimensions patched to 65535×65535 (⇒
    // 65535·65535·4 ≈ 17 GiB > 4 GiB), read by `decode_image_config` BEFORE any
    // frame decode ⇒ the `> maximum_image_size` guard fires.
    #[test]
    fn jpeg_too_big() {
        let data = decode(Global, include_bytes!("../fixtures/too_big.jpg"));
        assert_eq!(Err(Error::Overflow), data.map(|_| ()));
    }
}
