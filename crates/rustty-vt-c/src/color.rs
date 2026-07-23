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

use rustty_vt::color::{Palette, RGB};

/// C: GhosttyColorRgb -> RusttyColorRgb (R2)
// PORT NOTE: `pub const C = extern struct { r: u8, g: u8, b: u8 };` from
// `color.zig`. `color::RGB` already has this exact `#[repr(C)]` layout, so the
// C-ABI type is the rustty-vt `RGB` itself; the alias keeps the reference name.
pub type RgbC = RGB;

/// C: the C-ABI palette projection.
// pub const PaletteC = [256]RGB.C;
// PORT NOTE: since `RGB.C` IS `color::RGB` (the alias `RgbC` above), the C-ABI
// palette type is byte-identical to the rustty-vt `color::Palette` (`[RGB; 256]`).
// The reference distinguishes `Palette`/`PaletteC` only because `RGB`/`RGB.C` are
// nominally distinct Zig types; here they coincide, so `PaletteC` is `Palette`.
pub type PaletteC = [RgbC; 256];

// pub fn paletteCval(palette: *const Palette) PaletteC {
//     var result: PaletteC = undefined;
//     for (&result, palette) |*dst, src| dst.* = src.cval();
//     return result;
// }
//
// PORT NOTE: `RGB.cval()` is the identity (`RGB` == `RGB.C` here), so the
// element-wise copy is a plain array copy. The reference takes a `*const Palette`
// and returns a `PaletteC` by value.
/// # Safety
/// `palette` must be a valid, readable pointer to a `Palette`.
pub unsafe fn palette_cval(palette: *const Palette) -> PaletteC {
    // SAFETY: `palette` is a valid readable `Palette` pointer (caller contract).
    // for (&result, palette) |*dst, src| dst.* = src.cval();
    unsafe { *palette }
}

// pub fn paletteZval(palette: *const PaletteC) Palette {
//     var result: Palette = undefined;
//     for (&result, palette) |*dst, src| dst.* = .fromC(src);
//     return result;
// }
//
// PORT NOTE: `RGB.fromC` is the identity (`RGB.C` == `RGB` here), so the
// element-wise copy is a plain array copy.
/// # Safety
/// `palette` must be a valid, readable pointer to a `PaletteC`.
pub unsafe fn palette_zval(palette: *const PaletteC) -> Palette {
    // SAFETY: `palette` is a valid readable `PaletteC` pointer (caller contract).
    // for (&result, palette) |*dst, src| dst.* = .fromC(src);
    unsafe { *palette }
}

/// PORT NOTE: `pub fn rgb_get(c: *const color.RGB.C, r: *u8, g: *u8, b: *u8)
/// callconv(lib.calling_conv) void` (oracle 20a1bfa5f: RGB inputs pass by
/// pointer for wasm/FFI friendliness). The C calling convention is the
/// `extern "C"` wrapper authored in `lib.rs` (`rustty_color_rgb_get`); the body
/// is a 1:1 port. Out-pointers are raw (`*mut u8`) per the raw-ptr discipline.
///
/// # Safety
/// `c` must be a valid readable `RgbC` pointer; `r`, `g`, and `b` must be
/// valid, writable pointers to `u8`.
pub unsafe fn rgb_get(c: *const RgbC, r: *mut u8, g: *mut u8, b: *mut u8) {
    // SAFETY: pointer validity is the caller's contract.
    unsafe {
        // r.* = c.r;
        *r = (*c).r;
        // g.* = c.g;
        *g = (*c).g;
        // b.* = c.b;
        *b = (*c).b;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test coverage for the by-pointer form (oracle 20a1bfa5f)
    #[test]
    fn rgb_get_by_pointer() {
        let c = RgbC { r: 1, g: 2, b: 3 };
        let (mut r, mut g, mut b) = (0u8, 0u8, 0u8);
        unsafe { rgb_get(&c as *const RgbC, &mut r, &mut g, &mut b) };
        assert_eq!((r, g, b), (1, 2, 3));
    }
}
