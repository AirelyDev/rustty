//! PORTS: ghostty pkg/wuffs/src/error.zig — the decode error set + `check`.
//!
//! PORT STATUS: ported (every line/comment, R1).

// const std = @import("std");
//
// const c = @import("c.zig").c;
use std::ffi::CStr;

use wuffs_sys::{rustty_wuffs_status__message, wuffs_base__status};

// pub const Error = std.mem.Allocator.Error || error{ WuffsError, Overflow };
//
// PORT NOTE: Zig's `std.mem.Allocator.Error` is `error{OutOfMemory}`; unioned
// with `error{ WuffsError, Overflow }` this is the three-variant set below. It is
// the error type of `png.decode`/`jpeg.decode` and (via `check`) `WuffsError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// `std.mem.Allocator.Error` — an allocation failed.
    OutOfMemory,
    /// A Wuffs C decode call reported a non-OK status.
    WuffsError,
    /// A checked size multiplication overflowed (`std.math.mul`) or the image
    /// exceeds `maximum_image_size`.
    Overflow,
}

// pub fn check(log: anytype, status: *const c.struct_wuffs_base__status__struct) error{WuffsError}!void {
//
// PORT NOTE: the reference threads the caller's scoped `log` (`.wuffs_png` /
// `.wuffs_jpeg`) as a comptime-`anytype` param so the warning is scoped. The
// facade's carrier for `std.log.scoped(.x)` is `target: "x"` (DECISION-logging.md
// §6r4), which is a runtime `&str`, so the parameter is threaded as the scope
// NAME — the caller passes its own `LOG_SCOPE`, and the warning is emitted under
// the reference's own scope rather than folded into the message text.
// A Wuffs `status` is OK iff its `repr` string pointer is NULL (the
// `static inline` `wuffs_base__status__is_ok`, which bindgen does not emit — read
// `repr` directly, exactly as the wuffs-sys KAT does).
pub fn check(log_scope: &str, status: &wuffs_base__status) -> Result<(), Error> {
    // if (!c.wuffs_base__status__is_ok(status)) {
    if !status.repr.is_null() {
        // const e = c.wuffs_base__status__message(status);
        //
        // SAFETY: `status.repr` is non-null (checked above); `rustty_wuffs_status__message`
        // (the static-inline shim over `wuffs_base__status__message`) returns a
        // 'static C string owned by the Wuffs runtime.
        let e = unsafe { rustty_wuffs_status__message(status) };
        // PORT NOTE: the message is a Wuffs-owned C string; render it lossily for
        // the human-facing warning only (never fed back onto a byte-faithful path).
        // SAFETY: `e` is a non-null 'static NUL-terminated C string (see above).
        let msg = unsafe { CStr::from_ptr(e) };
        // log.warn("decode err={s}", .{e});
        log::warn!(target: log_scope, "decode err={}", msg.to_string_lossy());
        // return error.WuffsError;
        return Err(Error::WuffsError);
    }
    Ok(())
}
