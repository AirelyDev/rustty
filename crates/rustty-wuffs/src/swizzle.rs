//! PORTS: ghostty pkg/wuffs/src/swizzle.zig — pixel-format swizzlers.
//!
//! PORT STATUS: ported (every line/comment, R1). Binds the vendored Wuffs
//! `wuffs_base__pixel_swizzler` API via the `wuffs-sys` FFI leaf. FFI-boundary
//! memory-safety (the raw `(ptr, len)` destination buffer the C swizzler writes
//! through) is proven under ASan, NOT Miri (Miri cannot execute foreign C calls)
//! — see `.port/tools/gates/sanitizers.sh rustty-wuffs swizzle:: address`.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Allocator;
// const assert = std.debug.assert;
// const c = @import("c.zig").c;
// const Error = @import("error.zig").Error;
use crate::{Error, RawBuf};
use wuffs_sys::{
    rustty_wuffs_pixel_format__bits_per_pixel, rustty_wuffs_pixel_format__is_direct,
    rustty_wuffs_pixel_format__is_interleaved, wuffs_base__pixel_format,
    wuffs_base__pixel_swizzler, wuffs_base__pixel_swizzler__prepare,
    wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice, wuffs_base__slice_u8,
    WUFFS_BASE__PIXEL_FORMAT__BGR, WUFFS_BASE__PIXEL_FORMAT__BGRA_PREMUL,
    WUFFS_BASE__PIXEL_FORMAT__RGB, WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL,
    WUFFS_BASE__PIXEL_FORMAT__Y, WUFFS_BASE__PIXEL_FORMAT__YA_PREMUL,
};

// const log = std.log.scoped(.wuffs_swizzler);
//
// PORT NOTE (`log.warn` folds its scope into the message — error.rs `check` +
// tripwire.rs precedent): `swizzle`'s prepare-failure branch below reuses
// `crate::error::check`, which already renders this warning uniformly.
use crate::error::check;

// pub fn gToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__Y, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn gToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    swizzle(
        alloc,
        src,
        WUFFS_BASE__PIXEL_FORMAT__Y,
        WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL,
    )
}

// pub fn gaToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__YA_PREMUL, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn gaToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    swizzle(
        alloc,
        src,
        WUFFS_BASE__PIXEL_FORMAT__YA_PREMUL,
        WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL,
    )
}

// pub fn rgbToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__RGB, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn rgbToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    swizzle(
        alloc,
        src,
        WUFFS_BASE__PIXEL_FORMAT__RGB,
        WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL,
    )
}

// pub fn bgrToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__BGR, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn bgrToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    swizzle(
        alloc,
        src,
        WUFFS_BASE__PIXEL_FORMAT__BGR,
        WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL,
    )
}

// pub fn bgraToRgba(alloc: Allocator, src: []const u8) Error![]u8 {
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__BGRA_PREMUL, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL);
// }
#[allow(non_snake_case)]
pub fn bgraToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    swizzle(
        alloc,
        src,
        WUFFS_BASE__PIXEL_FORMAT__BGRA_PREMUL,
        WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL,
    )
}

// fn swizzle(
//     alloc: Allocator,
//     src: []const u8,
//     comptime src_pixel_format: u32,
//     comptime dst_pixel_format: u32,
// ) Error![]u8 {
// PARITY: the reference's `% 8 == 0` / `% src_size == 0` divisibility tests are
// kept verbatim (the `.is_multiple_of` clippy rewrite would obscure the ported
// expression) — the ssh_cache.rs/surface.rs precedent.
#[allow(clippy::manual_is_multiple_of)]
fn swizzle<A: Allocator + Copy>(
    alloc: A,
    src: &[u8],
    src_pixel_format: u32,
    dst_pixel_format: u32,
) -> Result<RawBuf<A>, Error> {
    //     const src_slice = c.wuffs_base__make_slice_u8(@constCast(src.ptr), src.len);
    //
    // PORT NOTE: `make_slice_u8` is a `static inline` (no linkable symbol);
    // constructed here as the 2-field struct `{ ptr, len }` — the png.rs KAT
    // precedent. `@constCast(src.ptr)` is the Zig cast dropping `const`; the C
    // struct field is a plain `uint8_t*` regardless of the Rust `&[u8]` const-ness
    // (the swizzler treats the SOURCE slice read-only; see `is_interleaved` etc.
    // assertions below — no write ever lands through `src_slice`).
    let src_slice = wuffs_base__slice_u8 {
        ptr: src.as_ptr() as *mut u8,
        len: src.len(),
    };

    //     const dst_fmt = c.wuffs_base__make_pixel_format(dst_pixel_format);
    //
    // PORT NOTE: `make_pixel_format` is a `static inline` single-field wrapper
    // (`{ .repr = repr }`); constructed here by hand — the same technique as
    // `make_slice_u8` above.
    let dst_fmt = wuffs_base__pixel_format {
        repr: dst_pixel_format,
    };

    //     assert(c.wuffs_base__pixel_format__is_direct(&dst_fmt));
    //     assert(c.wuffs_base__pixel_format__is_interleaved(&dst_fmt));
    //     assert(c.wuffs_base__pixel_format__bits_per_pixel(&dst_fmt) % 8 == 0);
    //
    // SAFETY: `dst_fmt` is a valid, initialized `wuffs_base__pixel_format`; each
    // shim is a pure accessor over `repr` (see shim.h/shim.c).
    unsafe {
        assert!(rustty_wuffs_pixel_format__is_direct(&dst_fmt));
        assert!(rustty_wuffs_pixel_format__is_interleaved(&dst_fmt));
        assert!(rustty_wuffs_pixel_format__bits_per_pixel(&dst_fmt) % 8 == 0);
    }

    //     const dst_size = c.wuffs_base__pixel_format__bits_per_pixel(&dst_fmt) / 8;
    // SAFETY: see above.
    let dst_size = unsafe { rustty_wuffs_pixel_format__bits_per_pixel(&dst_fmt) } / 8;

    //     const src_fmt = c.wuffs_base__make_pixel_format(src_pixel_format);
    let src_fmt = wuffs_base__pixel_format {
        repr: src_pixel_format,
    };

    //     assert(c.wuffs_base__pixel_format__is_direct(&src_fmt));
    //     assert(c.wuffs_base__pixel_format__is_interleaved(&src_fmt));
    //     assert(c.wuffs_base__pixel_format__bits_per_pixel(&src_fmt) % 8 == 0);
    // SAFETY: see above.
    unsafe {
        assert!(rustty_wuffs_pixel_format__is_direct(&src_fmt));
        assert!(rustty_wuffs_pixel_format__is_interleaved(&src_fmt));
        assert!(rustty_wuffs_pixel_format__bits_per_pixel(&src_fmt) % 8 == 0);
    }

    //     const src_size = c.wuffs_base__pixel_format__bits_per_pixel(&src_fmt) / 8;
    // SAFETY: see above.
    let src_size = unsafe { rustty_wuffs_pixel_format__bits_per_pixel(&src_fmt) } / 8;

    //     assert(src.len % src_size == 0);
    assert!(src.len() % (src_size as usize) == 0);

    //     const dst = try alloc.alloc(u8, src.len * dst_size / src_size);
    //     errdefer alloc.free(dst);
    //
    // PORT NOTE: the reference `errdefer alloc.free(dst)` is subsumed by
    // `RawBuf`'s `Drop` (the png.rs precedent) — if any `?` below returns early,
    // `dst` is dropped (freed); on success it is returned to the caller.
    let dst = RawBuf::alloc(alloc, src.len() * (dst_size as usize) / (src_size as usize))?;

    //     const dst_slice = c.wuffs_base__make_slice_u8(dst.ptr, dst.len);
    let dst_slice = wuffs_base__slice_u8 {
        ptr: dst.ptr(),
        len: dst.len(),
    };

    //     var swizzler: c.wuffs_base__pixel_swizzler = undefined;
    // SAFETY: `undefined` -> a zeroed POD; `prepare` fully initializes it before
    // `swizzle_interleaved_from_slice` reads it.
    let mut swizzler: wuffs_base__pixel_swizzler = unsafe { core::mem::zeroed() };
    {
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
        //
        // PORT NOTE: `wuffs_base__empty_slice_u8()` is a `static inline` returning
        // `{ .ptr = NULL, .len = 0 }` — constructed here by hand (no palette is
        // used by any of the direct, non-indexed formats this module drives).
        // `WUFFS_BASE__PIXEL_BLEND__SRC` is `#define ((wuffs_base__pixel_blend)0)`
        // (a function-style cast macro bindgen can't bind) — the literal `0u8`,
        // same PORT NOTE png.rs's `decode_frame` blend arg already documents.
        //
        // SAFETY: `swizzler` is valid (zeroed) receiver storage; `dst_fmt`/
        // `src_fmt` are valid formats; the empty palette slices carry no pointer
        // to dereference.
        let status = unsafe {
            wuffs_base__pixel_swizzler__prepare(
                &mut swizzler,
                dst_fmt,
                wuffs_base__slice_u8 {
                    ptr: core::ptr::null_mut(),
                    len: 0,
                },
                src_fmt,
                wuffs_base__slice_u8 {
                    ptr: core::ptr::null_mut(),
                    len: 0,
                },
                0, // c.WUFFS_BASE__PIXEL_BLEND__SRC
            )
        };
        // try check(log, &status);
        //
        // PORT NOTE (DRY): reuses `crate::error::check` (the fold-the-log-scope
        // precedent png.rs/jpeg.rs already establish) rather than re-duplicating
        // the reference's inlined `is_ok`/`message`/`log.warn` sequence.
        check(&status)?;
    }
    {
        //     {
        //         _ = c.wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice(
        //             &swizzler, dst_slice, c.wuffs_base__empty_slice_u8(), src_slice);
        //     }
        //
        // SAFETY: `swizzler` was `prepare`d immediately above (sequential, no
        // overlapping borrow — no SB/TB hazard); `dst_slice` covers the freshly
        // allocated `dst` buffer sized `src.len() * dst_size / src_size` bytes;
        // `src_slice` covers `src` (read-only). The C call writes exactly
        // `dst_slice.len` bytes through `dst_slice.ptr` — proven not to overrun
        // under ASan (Miri cannot execute this foreign call).
        let _ = unsafe {
            wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice(
                &swizzler,
                dst_slice,
                wuffs_base__slice_u8 {
                    ptr: core::ptr::null_mut(),
                    len: 0,
                },
                src_slice,
            )
        };
    }

    //     return dst;
    // }
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;

    // The oracle (swizzle.zig) has no test{} blocks; these KATs are new,
    // derived by reading the vendored `wuffs-v0.4.c` swizzle_xxxx__y /
    // swizzle_bgrw__bgr / swizzle_bgrw__rgb / swizzle_swap_rgbx_bgrx bodies
    // directly (see the workstream spec) rather than guessed.

    /// gToRgba: gray replicated into R/G/B, A forced to 255 (opaque).
    #[test]
    fn g_to_rgba_known_answer() {
        let src = [0x11u8, 0x80, 0xFF];
        let dst = gToRgba(Global, &src).unwrap();
        assert_eq!(
            dst.as_slice(),
            &[
                0x11, 0x11, 0x11, 0xFF, //
                0x80, 0x80, 0x80, 0xFF, //
                0xFF, 0xFF, 0xFF, 0xFF, //
            ]
        );
    }

    /// rgbToRgba: identity channel copy, A forced to 255 (opaque).
    #[test]
    fn rgb_to_rgba_known_answer() {
        let src = [0x10u8, 0x20, 0x30, 0x40, 0x50, 0x60];
        let dst = rgbToRgba(Global, &src).unwrap();
        assert_eq!(
            dst.as_slice(),
            &[0x10, 0x20, 0x30, 0xFF, 0x40, 0x50, 0x60, 0xFF]
        );
    }

    /// bgrToRgba: R/B channel swap, A forced to 255 (opaque).
    #[test]
    fn bgr_to_rgba_known_answer() {
        let src = [0x10u8, 0x20, 0x30]; // B=0x10 G=0x20 R=0x30
        let dst = bgrToRgba(Global, &src).unwrap();
        assert_eq!(dst.as_slice(), &[0x30, 0x20, 0x10, 0xFF]);
    }

    /// bgraToRgba: R/B channel swap, A passed through unchanged (both sides
    /// are premultiplied-alpha direct formats, so this is a pure byte
    /// permutation with no scaling).
    #[test]
    fn bgra_premul_to_rgba_premul_known_answer() {
        let src = [0x10u8, 0x20, 0x30, 0x80]; // B=0x10 G=0x20 R=0x30 A=0x80
        let dst = bgraToRgba(Global, &src).unwrap();
        assert_eq!(dst.as_slice(), &[0x30, 0x20, 0x10, 0x80]);
    }

    /// The `assert(src.len % src_size == 0)` guard aborts on a misaligned
    /// (odd) input length for a 3-bytes-per-pixel source format.
    #[test]
    #[should_panic]
    fn rgb_to_rgba_odd_length_panics() {
        let src = [0x10u8, 0x20, 0x30, 0x40]; // 4 is not a multiple of 3
        let _ = rgbToRgba(Global, &src);
    }

    /// KEY FINDING (see workstream spec): `gaToRgba` swizzles FROM
    /// `WUFFS_BASE__PIXEL_FORMAT__YA_PREMUL` — but `wuffs_base__pixel_swizzler__prepare`'s
    /// src-format switch in the vendored C (wuffs-v0.4.c) has a case for
    /// `YA_NONPREMUL` but NOT `YA_PREMUL` (distinct repr values: 0x21000088 vs
    /// 0x22000088). `prepare` therefore ALWAYS returns a non-OK status for this
    /// src format, so `gaToRgba` is UNCONDITIONALLY `Err(Error::WuffsError)` for
    /// every input — ported AS-IS (R1 fidelity), not "fixed" to route through
    /// `YA_NONPREMUL`, even though that looks like an upstream ghostty dead-code
    /// path / bug. Do not silently "fix" this.
    #[test]
    fn ga_to_rgba_always_unsupported() {
        for src in [&[][..], &[0x11, 0xFF][..], &[0x00, 0x00, 0x80, 0x80][..]] {
            let result = gaToRgba(Global, src);
            assert_eq!(result.map(|_| ()), Err(Error::WuffsError));
        }
    }
}
