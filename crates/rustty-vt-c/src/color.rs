//! rustty-vt-c color
//!
//! PORTS: ghostty src/terminal/c/color.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const lib = @import("../lib.zig");
//   const color = @import("../color.zig");
// `color.RGB.C` is the C-ABI projection of `color.RGB`
// (`extern struct { r: u8, g: u8, b: u8 }`). In rustty `color::RGB` is ALREADY
// `#[repr(C)]` with `r/g/b: u8` (byte-identical to the reference `RGB.C`), so the
// C-ABI type IS that struct. We alias it here under the C name `RGB_C` so the
// FFI surface and `types.rs` registry can name it; per R3 the rustty-vt enum/type
// is the source of truth and vt-c is a pure FFI projection.

use rustty_vt::color::RGB;

/// C: GhosttyColorRgb -> RusttyColorRgb (R2)
// PORT NOTE: `pub const C = extern struct { r: u8, g: u8, b: u8 };` from
// `color.zig`. `color::RGB` already has this exact `#[repr(C)]` layout, so the
// C-ABI type is the rustty-vt `RGB` itself; the alias keeps the reference name.
pub type RgbC = RGB;

/// PORT NOTE: `pub fn rgb_get(c: color.RGB.C, r: *u8, g: *u8, b: *u8)
/// callconv(lib.calling_conv) void`. The C calling convention is the
/// `extern "C"` wrapper authored in `lib.rs` (`rustty_color_rgb_get`); the body
/// is a 1:1 port. Out-pointers are raw (`*mut u8`) per the raw-ptr discipline.
///
/// # Safety
/// `r`, `g`, and `b` must be valid, writable pointers to `u8`.
pub unsafe fn rgb_get(c: RgbC, r: *mut u8, g: *mut u8, b: *mut u8) {
    // SAFETY: `r`/`g`/`b` are valid writable `u8` pointers (caller contract).
    unsafe {
        // r.* = c.r;
        *r = c.r;
        // g.* = c.g;
        *g = c.g;
        // b.* = c.b;
        *b = c.b;
    }
}
