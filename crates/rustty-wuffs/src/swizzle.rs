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
    rustty_wuffs_pixel_format__is_interleaved, rustty_wuffs_status__message,
    wuffs_base__pixel_format, wuffs_base__pixel_swizzler, wuffs_base__pixel_swizzler__prepare,
    wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice, wuffs_base__slice_u8,
    WUFFS_BASE__PIXEL_FORMAT__BGR, WUFFS_BASE__PIXEL_FORMAT__BGRA_PREMUL,
    WUFFS_BASE__PIXEL_FORMAT__RGB, WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL,
    WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL, WUFFS_BASE__PIXEL_FORMAT__Y,
    WUFFS_BASE__PIXEL_FORMAT__YA_NONPREMUL,
};

// const log = std.log.scoped(.wuffs_swizzler);
//
// PORT NOTE: the reference's scope tag, carried through the `log` facade as
// `target: LOG_SCOPE` (DECISION-logging.md §6r4).
const LOG_SCOPE: &str = "wuffs_swizzler";

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
//     // Wuffs doesn't support YA_PREMUL as a swizzle source. The nonpremul
//     // pair produces the same bytes (r=g=b=y, a=a, no alpha math), which
//     // is what we want: alpha semantics are preserved as-is, matching the
//     // other conversions here.
//     return swizzle(alloc, src, c.WUFFS_BASE__PIXEL_FORMAT__YA_NONPREMUL, c.WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL);
// }
#[allow(non_snake_case)]
pub fn gaToRgba<A: Allocator + Copy>(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error> {
    // Wuffs doesn't support YA_PREMUL as a swizzle source. The nonpremul
    // pair produces the same bytes (r=g=b=y, a=a, no alpha math), which
    // is what we want: alpha semantics are preserved as-is, matching the
    // other conversions here.
    swizzle(
        alloc,
        src,
        WUFFS_BASE__PIXEL_FORMAT__YA_NONPREMUL,
        WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL,
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

// pub fn rgbaSrcOver(dst: []u8, src: []const u8) void {
//
// PORT NOTE: the reference mutates `dst: []u8` in place — `&mut [u8]` (the
// unique-borrow analog; no allocator is involved, unlike the `swizzle` path).
/// Composite `src` over `dst` in place. Both are straight
/// (non-premultiplied) alpha RGBA of the same length. A transparent
/// destination pixel takes the source pixel exactly; wuffs composites
/// everything else in 16-bit integer space.
// PARITY: `% 4 == 0` kept verbatim (see the `swizzle` PARITY note below).
#[allow(non_snake_case)]
#[allow(clippy::manual_is_multiple_of)]
pub fn rgbaSrcOver(dst: &mut [u8], src: &[u8]) {
    //     assert(dst.len == src.len);
    //     assert(dst.len % 4 == 0);
    assert!(dst.len() == src.len());
    assert!(dst.len() % 4 == 0);

    //     var swizzler: c.wuffs_base__pixel_swizzler = undefined;
    // SAFETY: `undefined` -> a zeroed POD; `prepare` fully initializes it before
    // `swizzle_interleaved_from_slice` reads it (the `swizzle` precedent below).
    let mut swizzler: wuffs_base__pixel_swizzler = unsafe { core::mem::zeroed() };
    //     const status = c.wuffs_base__pixel_swizzler__prepare(
    //         &swizzler,
    //         c.wuffs_base__make_pixel_format(c.WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL),
    //         c.wuffs_base__empty_slice_u8(),
    //         c.wuffs_base__make_pixel_format(c.WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL),
    //         c.wuffs_base__empty_slice_u8(),
    //         c.WUFFS_BASE__PIXEL_BLEND__SRC_OVER,
    //     );
    //
    // PORT NOTE: `make_pixel_format` / `empty_slice_u8` are `static inline`s,
    // constructed by hand; `WUFFS_BASE__PIXEL_BLEND__SRC_OVER` is
    // `#define ((wuffs_base__pixel_blend)1)` (a function-style cast macro bindgen
    // can't bind) — the literal `1` — the same three PORT NOTEs `swizzle` below
    // already documents for its `SRC` (`0`) prepare call.
    //
    // SAFETY: `swizzler` is valid (zeroed) receiver storage; both formats are
    // valid direct RGBA_NONPREMUL; the empty palette slices carry no pointer to
    // dereference.
    let status = unsafe {
        wuffs_base__pixel_swizzler__prepare(
            &mut swizzler,
            wuffs_base__pixel_format {
                repr: WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL,
            },
            wuffs_base__slice_u8 {
                ptr: core::ptr::null_mut(),
                len: 0,
            },
            wuffs_base__pixel_format {
                repr: WUFFS_BASE__PIXEL_FORMAT__RGBA_NONPREMUL,
            },
            wuffs_base__slice_u8 {
                ptr: core::ptr::null_mut(),
                len: 0,
            },
            1, // c.WUFFS_BASE__PIXEL_BLEND__SRC_OVER
        )
    };
    //     // This format pair and blend mode is a supported swizzle, so
    //     // preparation can only fail on a programming error.
    //     assert(c.wuffs_base__status__is_ok(&status));
    //
    // PORT NOTE: `status__is_ok` is a `static inline` — a Wuffs status is OK iff
    // its `repr` string pointer is NULL (read directly, the error.rs `check`
    // precedent).
    // This format pair and blend mode is a supported swizzle, so
    // preparation can only fail on a programming error.
    assert!(status.repr.is_null());

    //     _ = c.wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice(
    //         &swizzler,
    //         c.wuffs_base__make_slice_u8(dst.ptr, dst.len),
    //         c.wuffs_base__empty_slice_u8(),
    //         c.wuffs_base__make_slice_u8(@constCast(src.ptr), src.len),
    //     );
    // }
    //
    // PORT NOTE: `@constCast(src.ptr)` — the C struct field is a plain
    // `uint8_t*`; the swizzler treats the SOURCE slice read-only (the
    // `src_slice` PORT NOTE in `swizzle` below).
    //
    // SAFETY: `swizzler` was `prepare`d immediately above (sequential, no
    // overlapping borrow — no SB/TB hazard); `dst`/`src` are live slices of
    // equal length (asserted). The C call writes at most `dst.len` bytes
    // through the dst pointer and only reads through the src pointer — the
    // ASan-proven contract `swizzle` below documents.
    let _ = unsafe {
        wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice(
            &swizzler,
            wuffs_base__slice_u8 {
                ptr: dst.as_mut_ptr(),
                len: dst.len(),
            },
            wuffs_base__slice_u8 {
                ptr: core::ptr::null_mut(),
                len: 0,
            },
            wuffs_base__slice_u8 {
                ptr: src.as_ptr() as *mut u8,
                len: src.len(),
            },
        )
    };
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
        // if (!c.wuffs_base__status__is_ok(&status)) {
        //
        // PORT NOTE (S38-F2): `swizzle` does NOT call `error.zig`'s `check` — the
        // reference inlines its own `is_ok`/`message`/`log.warn` sequence here with
        // a DIFFERENT message (the bare `"{s}"` status text, not `check`'s
        // `"decode err={s}"`) under its own `.wuffs_swizzler` scope. Reusing `check`
        // here changed both the text and the scope; the reference's own sequence is
        // reproduced instead. A Wuffs `status` is OK iff its `repr` pointer is NULL.
        if !status.repr.is_null() {
            // const e = c.wuffs_base__status__message(&status);
            //
            // SAFETY: `status.repr` is non-null (checked above);
            // `rustty_wuffs_status__message` returns a 'static C string owned by
            // the Wuffs runtime.
            let e = unsafe { rustty_wuffs_status__message(&status) };
            // PORT NOTE: the message is a Wuffs-owned C string; render it lossily
            // for the human-facing warning only (never fed back onto a
            // byte-faithful path).
            // SAFETY: `e` is a non-null 'static NUL-terminated C string.
            let e = unsafe { core::ffi::CStr::from_ptr(e) };
            // log.warn("{s}", .{e});
            log::warn!(target: LOG_SCOPE, "{}", e.to_string_lossy());
            // return error.WuffsError;
            return Err(Error::WuffsError);
        }
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

    // ---- task-14e-2 / S38-F2 -------------------------------------------------

    // PORT NOTE (S38-F2): `swizzle.zig:172-176` does NOT delegate to
    // `error.zig`'s `check`. It inlines its own failure arm with a DIFFERENT
    // message (`log.warn("{s}", .{e})` — the bare Wuffs status text) under its
    // own `.wuffs_swizzler` scope. The port had reused `check`, which renders
    // `decode err={s}` under the CALLER's scope, so both the text and the scope
    // diverged. These two tests pin both halves.
    //
    // The failure is driven by a real `wuffs_base__pixel_swizzler__prepare`
    // rejection rather than a synthesised status: every source format the module
    // uses fails to prepare against a `YA_NONPREMUL` destination (established by
    // sweeping the seven formats this file names; the six other destinations all
    // prepare successfully).

    /// The restored site emits the reference's message under the reference's
    /// scope, and nothing else.
    #[test]
    fn swizzle_prepare_failure_warns_with_the_reference_text_and_scope() {
        let cap = crate::log_capture::start();
        let src = [1u8; 24];
        let err = swizzle(
            Global,
            &src,
            WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL,
            WUFFS_BASE__PIXEL_FORMAT__YA_NONPREMUL,
        )
        .map(|_| ())
        .unwrap_err();
        let messages = cap.messages();
        drop(cap);

        // Positive control: the arm really ran (the reference returns
        // `error.WuffsError` from the same block that logs).
        assert_eq!(Error::WuffsError, err);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "wuffs_swizzler".to_string(),
                "base: unsupported pixel swizzler option".to_string(),
            )],
            messages
        );
    }

    /// The absence half: a SUCCESSFUL swizzle emits nothing. Guards against the
    /// "diagnostic on a success path" defect, and asserts positively that it
    /// reached past the `prepare` the silence is about (the destination buffer
    /// is the swizzled output, which only exists if `prepare` AND the swizzle
    /// itself ran).
    #[test]
    fn a_successful_swizzle_is_silent() {
        let cap = crate::log_capture::start();
        let rgba = gaToRgba(Global, &[7, 100, 8, 200]).unwrap();
        let messages = cap.messages();
        drop(cap);

        assert_eq!(
            &[7u8, 7, 7, 100, 8, 8, 8, 200][..],
            rgba.as_slice(),
            "the swizzle did not run, so this test never reached the code whose \
             silence it asserts"
        );
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }

    // test "gaToRgba" {
    #[test]
    fn ga_to_rgba() {
        //     const rgba = try gaToRgba(std.testing.allocator, &.{ 7, 100, 8, 200 });
        //     defer std.testing.allocator.free(rgba);
        //
        // PORT NOTE: the reference `defer free` is `RawBuf`'s `Drop`.
        let rgba = gaToRgba(Global, &[7, 100, 8, 200]).unwrap();

        //     try std.testing.expectEqualSlices(u8, &.{
        //         7, 7, 7, 100,
        //         8, 8, 8, 200,
        //     }, rgba);
        // }
        assert_eq!(
            &[
                7, 7, 7, 100, //
                8, 8, 8, 200, //
            ],
            rgba.as_slice()
        );
    }

    // test "rgbaSrcOver" {
    #[test]
    fn rgba_src_over() {
        // 50% white over opaque black, opaque over anything, transparent
        // source over anything, and anything over a transparent
        // destination (exact source passthrough).
        //     var dst = [_]u8{ ... };
        let mut dst = [
            0u8, 0, 0, 255, //
            10, 20, 30, 40, //
            10, 20, 30, 40, //
            0, 0, 0, 0, //
        ];
        //     rgbaSrcOver(&dst, &.{ ... });
        rgbaSrcOver(
            &mut dst,
            &[
                255, 255, 255, 128, //
                100, 110, 120, 255, //
                100, 110, 120, 0, //
                200, 100, 50, 128, //
            ],
        );
        //     try std.testing.expectEqualSlices(u8, &.{ ... }, &dst);
        // }
        assert_eq!(
            &[
                128, 128, 128, 255, //
                100, 110, 120, 255, //
                10, 20, 30, 40, //
                200, 100, 50, 128, //
            ],
            &dst
        );
    }

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

    // RETIRED TEST (`ga_to_rgba_always_unsupported`, R5): the pre-`86db30785`
    // KEY FINDING test asserted `gaToRgba` was UNCONDITIONALLY
    // `Err(Error::WuffsError)` (the old `YA_PREMUL` source has no case in
    // `prepare`'s src-format switch — an upstream bug this port documented
    // as-is). Upstream FIXED it (`86db30785`: route through the
    // `YA_NONPREMUL` -> `RGBA_NONPREMUL` pair, verified present in the
    // vendored alpha.9 `prepare__ya_nonpremul`), so that test is REPLACED by
    // the oracle's own `gaToRgba` expectation above — fixing, not weakening.
}
