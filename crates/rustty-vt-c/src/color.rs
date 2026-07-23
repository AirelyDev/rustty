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

use std::ffi::c_char;
use std::sync::OnceLock;

use crate::result::Result;
use rustty_vt::color::{Palette, RGB};
use rustty_vt::x11_color;

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

/// C: GhosttyColorPaletteMask -> RusttyColorPaletteMask (R2)
// pub const PaletteMask = extern struct { bits: [4]u64, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteMask {
    pub bits: [u64; 4],
}

impl PaletteMask {
    /// Convert to the Zig PaletteMask (std.StaticBitSet(256)) analog.
    pub fn to_zig(&self) -> rustty_vt::color::PaletteMask {
        let mut result = rustty_vt::color::PaletteMask::init_empty();
        for i in 0..256usize {
            if ((self.bits[i >> 6] >> (i & 63)) & 1) != 0 {
                result.set(i);
            }
        }
        result
    }
}

/// C: GhosttyColorX11Entry -> RusttyColorX11Entry (R2)
// pub const X11Entry = extern struct { name: ?[*:0]const u8, color: color.RGB.C };
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct X11Entry {
    /// Null-terminated color name; null marks the end of the table.
    pub name: *const c_char,
    pub color: RgbC,
}

// SAFETY: the table is immutable static data (pointers into leaked
// NUL-terminated strings); sharing across threads is sound.
unsafe impl Sync for X11Entry {}
unsafe impl Send for X11Entry {}

// PORT NOTE: the reference builds `x11_entries` at COMPTIME
// (`[x11_color.entries.len + 1]X11Entry`, null-name terminated). Rust has no
// comptime rgb.txt parse; the same table is built once at runtime and leaked
// (name storage lives for the process lifetime, satisfying the header's
// "static table" contract and the oracle's pointer-stability test).
fn x11_entries() -> &'static [X11Entry] {
    static ENTRIES: OnceLock<Vec<X11Entry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let src = x11_color::entries();
        let mut result = Vec::with_capacity(src.len() + 1);
        for e in src {
            let name: &'static std::ffi::CStr =
                Box::leak(std::ffi::CString::new(e.name).unwrap().into_boxed_c_str());
            result.push(X11Entry {
                name: name.as_ptr(),
                color: e.color,
            });
        }
        result.push(X11Entry {
            name: std::ptr::null(),
            color: RGB { r: 0, g: 0, b: 0 },
        });
        result
    })
}

/// # Safety
/// `name_`, if non-null, must be readable for `len` bytes; `out` writable.
pub unsafe fn parse_x11(name_: *const u8, len: usize, out: *mut RgbC) -> Result {
    // const name = (name_ orelse return .invalid_value)[0..len];
    if name_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: caller contract.
    let name = unsafe { std::slice::from_raw_parts(name_, len) };
    // const trimmed = std.mem.trim(u8, name, " \t");
    let trimmed = trim_space_tab(name);
    // const rgb = x11_color.map.get(trimmed) orelse return .invalid_value;
    let Ok(trimmed) = std::str::from_utf8(trimmed) else {
        return Result::InvalidValue;
    };
    let Some(rgb) = x11_color::map.get(trimmed) else {
        return Result::InvalidValue;
    };
    // SAFETY: caller contract.
    unsafe { *out = rgb };
    Result::Success
}

/// # Safety
/// `value_`, if non-null, readable for `len` bytes; `out` writable.
pub unsafe fn parse(value_: *const u8, len: usize, out: *mut RgbC) -> Result {
    if value_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: caller contract.
    let value = unsafe { std::slice::from_raw_parts(value_, len) };
    // const rgb = color.RGB.parse(value) catch return .invalid_value;
    let Ok(value_str) = std::str::from_utf8(value) else {
        return Result::InvalidValue;
    };
    match RGB::parse(value_str) {
        Ok(rgb) => {
            // SAFETY: caller contract.
            unsafe { *out = rgb };
            Result::Success
        }
        Err(_) => Result::InvalidValue,
    }
}

/// # Safety
/// `out` must be a valid writable `PaletteC` pointer.
pub unsafe fn palette_default(out: *mut PaletteC) {
    // out.* = color.paletteCval(&color.default);
    // SAFETY: caller contract (RGB == RGB.C here, plain copy).
    unsafe { *out = rustty_vt::color::DEFAULT };
}

/// Generate a 256-color palette. The output may alias the base input.
/// # Safety
/// `bg`/`fg`/`out` valid; `base_`/`skip_` valid or null.
pub unsafe fn palette_generate(
    base_: *const PaletteC,
    skip_: *const PaletteMask,
    bg: *const RgbC,
    fg: *const RgbC,
    harmonious: bool,
    out: *mut PaletteC,
) {
    // SAFETY: caller contract. `base` is COPIED before `out` is written, so
    // out==base aliasing is sound (the oracle documents in-place use).
    unsafe {
        let base: rustty_vt::color::Palette = if base_.is_null() {
            rustty_vt::color::DEFAULT
        } else {
            *base_
        };
        let skip = if skip_.is_null() {
            rustty_vt::color::PaletteMask::init_empty()
        } else {
            (*skip_).to_zig()
        };
        let result = rustty_vt::color::generate_256_color(base, skip, *bg, *fg, harmonious);
        *out = result;
    }
}

pub fn x11_names() -> *const X11Entry {
    x11_entries().as_ptr()
}

pub fn x11_name_count() -> usize {
    x11_color::entries().len()
}

/// # Safety
/// `value_` readable for `len` (or null); `out_index`/`out_rgb` writable.
pub unsafe fn parse_palette_entry(
    value_: *const u8,
    len: usize,
    out_index: *mut u8,
    out_rgb: *mut RgbC,
) -> Result {
    if value_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: caller contract.
    let value = unsafe { std::slice::from_raw_parts(value_, len) };
    // const entry = color.parsePaletteEntry(value) catch return .invalid_value;
    match rustty_vt::color::parse_palette_entry(value) {
        Ok(entry) => {
            // SAFETY: caller contract.
            unsafe {
                *out_index = entry.index;
                *out_rgb = entry.color;
            }
            Result::Success
        }
        Err(_) => Result::InvalidValue,
    }
}

/// # Safety
/// `c` must be a valid readable `RgbC` pointer.
pub unsafe fn luminance(c: *const RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { (*c).luminance() }
}

/// # Safety
/// `c` must be a valid readable `RgbC` pointer.
pub unsafe fn perceived_luminance(c: *const RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { (*c).perceived_luminance() }
}

/// # Safety
/// `a`/`b` must be valid readable `RgbC` pointers.
pub unsafe fn contrast(a: *const RgbC, b: *const RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { (*a).contrast(*b) }
}

fn trim_space_tab(s: &[u8]) -> &[u8] {
    let start = s
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&b| b != b' ' && b != b'\t')
        .map_or(start, |i| i + 1);
    &s[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vt::color::{self, RGB};

    fn expect_rgb(expected: RGB, actual: RgbC) {
        assert_eq!(expected, actual);
    }

    fn expect_palette_c(expected: &PaletteC, actual: &PaletteC) {
        for (e, a) in expected.iter().zip(actual.iter()) {
            expect_rgb(*e, *a);
        }
    }

    fn generate_palette(
        base: Option<&PaletteC>,
        skip: Option<&PaletteMask>,
        bg: RGB,
        fg: RGB,
        harmonious: bool,
    ) -> PaletteC {
        let mut out: PaletteC = [RGB { r: 0, g: 0, b: 0 }; 256];
        unsafe {
            palette_generate(
                base.map_or(std::ptr::null(), |b| b as *const PaletteC),
                skip.map_or(std::ptr::null(), |s| s as *const PaletteMask),
                &bg as *const RgbC,
                &fg as *const RgbC,
                harmonious,
                &mut out,
            )
        };
        out
    }

    fn set_mask_bit(mask: &mut PaletteMask, idx: usize) {
        mask.bits[idx >> 6] |= 1u64 << (idx & 63);
    }

    // test coverage for the by-pointer form (oracle 20a1bfa5f)
    #[test]
    fn rgb_get_by_pointer() {
        let c = RgbC { r: 1, g: 2, b: 3 };
        let (mut r, mut g, mut b) = (0u8, 0u8, 0u8);
        unsafe { rgb_get(&c as *const RgbC, &mut r, &mut g, &mut b) };
        assert_eq!((r, g, b), (1, 2, 3));
    }

    // test "color: parse_x11 valid"
    #[test]
    fn parse_x11_valid() {
        let cases: &[(&[u8], RGB)] = &[
            (
                b"white",
                RGB {
                    r: 255,
                    g: 255,
                    b: 255,
                },
            ),
            (
                b"medium spring green",
                RGB {
                    r: 0,
                    g: 250,
                    b: 154,
                },
            ),
            (
                b"ForestGreen",
                RGB {
                    r: 34,
                    g: 139,
                    b: 34,
                },
            ),
            (
                b"FoReStGReen",
                RGB {
                    r: 34,
                    g: 139,
                    b: 34,
                },
            ),
            (
                b" Forest Green ",
                RGB {
                    r: 34,
                    g: 139,
                    b: 34,
                },
            ),
            (b"\tblack\t", RGB { r: 0, g: 0, b: 0 }),
        ];
        for (input, expected) in cases {
            let mut out = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::Success, unsafe {
                parse_x11(input.as_ptr(), input.len(), &mut out)
            });
            expect_rgb(*expected, out);
        }
    }

    // test "color: parse_x11 invalid"
    #[test]
    fn parse_x11_invalid() {
        for case in [&b"nosuchcolor"[..], b"", b"   ", b"#ffffff"] {
            let mut out = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::InvalidValue, unsafe {
                parse_x11(case.as_ptr(), case.len(), &mut out)
            });
        }
        let mut out = RGB { r: 0, g: 0, b: 0 };
        assert_eq!(Result::InvalidValue, unsafe {
            parse_x11(std::ptr::null(), 0, &mut out)
        });
    }

    // test "color: parse valid"
    #[test]
    fn parse_valid() {
        let cases: &[(&[u8], RGB)] = &[
            (b"black", RGB { r: 0, g: 0, b: 0 }),
            (
                b"#AABBCC",
                RGB {
                    r: 170,
                    g: 187,
                    b: 204,
                },
            ),
            (b"0A0B0C", RGB { r: 10, g: 11, b: 12 }),
            (
                b"FFF",
                RGB {
                    r: 255,
                    g: 255,
                    b: 255,
                },
            ),
            (b"#345", RGB { r: 51, g: 68, b: 85 }),
            (b"rgb:1/2/3", RGB { r: 17, g: 34, b: 51 }),
            (b"  black ", RGB { r: 0, g: 0, b: 0 }),
            (
                b" #AABBCC   ",
                RGB {
                    r: 170,
                    g: 187,
                    b: 204,
                },
            ),
        ];
        for (input, expected) in cases {
            let mut out = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::Success, unsafe {
                parse(input.as_ptr(), input.len(), &mut out)
            });
            expect_rgb(*expected, out);
        }
    }

    // test "color: parse invalid"
    #[test]
    fn parse_invalid() {
        for case in [&b""[..], b"notacolor", b"#12345"] {
            let mut out = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::InvalidValue, unsafe {
                parse(case.as_ptr(), case.len(), &mut out)
            });
        }
        let mut out = RGB { r: 0, g: 0, b: 0 };
        assert_eq!(Result::InvalidValue, unsafe {
            parse(std::ptr::null(), 0, &mut out)
        });
    }

    // test "color: parse_palette_entry valid"
    #[test]
    fn parse_palette_entry_valid() {
        let cases: &[(&[u8], u8, RGB)] = &[
            (
                b"0=#AABBCC",
                0,
                RGB {
                    r: 170,
                    g: 187,
                    b: 204,
                },
            ),
            (
                b"0xF=#ABCDEF",
                15,
                RGB {
                    r: 171,
                    g: 205,
                    b: 239,
                },
            ),
            (
                b"0b1=#014589",
                1,
                RGB {
                    r: 1,
                    g: 69,
                    b: 137,
                },
            ),
            (
                b"0o7=#234567",
                7,
                RGB {
                    r: 35,
                    g: 69,
                    b: 103,
                },
            ),
            (
                b" 1= #DDEEFF    ",
                1,
                RGB {
                    r: 221,
                    g: 238,
                    b: 255,
                },
            ),
            (b"1=black", 1, RGB { r: 0, g: 0, b: 0 }),
        ];
        for (input, index, expected) in cases {
            let mut out_index: u8 = 0;
            let mut out_rgb = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::Success, unsafe {
                parse_palette_entry(input.as_ptr(), input.len(), &mut out_index, &mut out_rgb)
            });
            assert_eq!(*index, out_index);
            expect_rgb(*expected, out_rgb);
        }
    }

    // test "color: parse_palette_entry invalid"
    #[test]
    fn parse_palette_entry_invalid() {
        for case in [&b"256=#AABBCC"[..], b"a", b"", b"1=notacolor"] {
            let mut out_index: u8 = 0;
            let mut out_rgb = RGB { r: 0, g: 0, b: 0 };
            assert_eq!(Result::InvalidValue, unsafe {
                parse_palette_entry(case.as_ptr(), case.len(), &mut out_index, &mut out_rgb)
            });
        }
        let mut out_index: u8 = 0;
        let mut out_rgb = RGB { r: 0, g: 0, b: 0 };
        assert_eq!(Result::InvalidValue, unsafe {
            parse_palette_entry(std::ptr::null(), 0, &mut out_index, &mut out_rgb)
        });
    }

    // test "color: palette_default"
    #[test]
    fn palette_default_spot_values() {
        let mut out: PaletteC = [RGB { r: 0, g: 0, b: 0 }; 256];
        unsafe { palette_default(&mut out) };

        let expected = color::DEFAULT;
        expect_palette_c(&expected, &out);

        expect_rgb(
            RGB {
                r: 0x1D,
                g: 0x1F,
                b: 0x21,
            },
            out[0],
        );
        expect_rgb(RGB { r: 0, g: 0, b: 0 }, out[16]);
        expect_rgb(
            RGB {
                r: 255,
                g: 255,
                b: 255,
            },
            out[231],
        );
        expect_rgb(RGB { r: 8, g: 8, b: 8 }, out[232]);
        expect_rgb(
            RGB {
                r: 238,
                g: 238,
                b: 238,
            },
            out[255],
        );
    }

    // test "color: palette_generate base16 preserved"
    #[test]
    fn palette_generate_base16_preserved() {
        let base = color::DEFAULT;
        let palette = generate_palette(
            Some(&base),
            None,
            RGB { r: 0, g: 0, b: 0 },
            RGB {
                r: 255,
                g: 255,
                b: 255,
            },
            false,
        );
        for i in 0..16 {
            assert_eq!(base[i], palette[i]);
        }
    }

    // test "color: palette_generate cube corners"
    #[test]
    fn palette_generate_cube_corners() {
        let bg = RGB { r: 0, g: 0, b: 0 };
        let fg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let palette = generate_palette(None, None, bg, fg, false);
        expect_rgb(bg, palette[16]);
        expect_rgb(fg, palette[231]);
    }

    // test "color: palette_generate light theme harmonious=false"
    #[test]
    fn palette_generate_light_theme_harmonious_false() {
        let white = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let black = RGB { r: 0, g: 0, b: 0 };

        let normal = generate_palette(None, None, white, black, false);
        expect_rgb(black, normal[16]);
        expect_rgb(white, normal[231]);

        let harmonious = generate_palette(None, None, white, black, true);
        expect_rgb(white, harmonious[16]);
        expect_rgb(black, harmonious[231]);
    }

    // test "color: palette_generate grayscale monotonic"
    #[test]
    fn palette_generate_grayscale_monotonic() {
        let palette = generate_palette(
            None,
            None,
            RGB { r: 0, g: 0, b: 0 },
            RGB {
                r: 255,
                g: 255,
                b: 255,
            },
            false,
        );
        let mut prev_lum = 0.0f64;
        for c in palette.iter().take(256).skip(232) {
            let lum = c.luminance();
            assert!(lum >= prev_lum);
            prev_lum = lum;
        }
    }

    // test "color: palette_generate skip mask"
    #[test]
    fn palette_generate_skip_mask() {
        let base = color::DEFAULT;
        let mut skip = PaletteMask { bits: [0; 4] };
        set_mask_bit(&mut skip, 20);
        set_mask_bit(&mut skip, 100);
        set_mask_bit(&mut skip, 240);

        let palette = generate_palette(
            Some(&base),
            Some(&skip),
            RGB { r: 0, g: 0, b: 0 },
            RGB {
                r: 255,
                g: 255,
                b: 255,
            },
            false,
        );

        assert_eq!(base[20], palette[20]);
        assert_eq!(base[100], palette[100]);
        assert_eq!(base[240], palette[240]);
        assert_ne!(palette[21], base[21]);
    }

    // test "color: palette_generate dark harmonious no-op"
    #[test]
    fn palette_generate_dark_harmonious_no_op() {
        let bg = RGB { r: 0, g: 0, b: 0 };
        let fg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let normal = generate_palette(None, None, bg, fg, false);
        let harmonious = generate_palette(None, None, bg, fg, true);
        for i in 16..256 {
            assert_eq!(normal[i], harmonious[i]);
        }
    }

    // test "color: palette_generate light harmonious ramp"
    #[test]
    fn palette_generate_light_harmonious_ramp() {
        let bg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let fg = RGB { r: 0, g: 0, b: 0 };

        {
            let palette = generate_palette(None, None, bg, fg, false);
            let mut prev_lum = 0.0f64;
            for c in palette.iter().take(256).skip(232) {
                let lum = c.luminance();
                assert!(lum >= prev_lum);
                prev_lum = lum;
            }
        }

        {
            let palette = generate_palette(None, None, bg, fg, true);
            let mut prev_lum = 1.0f64;
            for c in palette.iter().take(256).skip(232) {
                let lum = c.luminance();
                assert!(lum <= prev_lum);
                prev_lum = lum;
            }
        }
    }

    // test "color: palette_generate NULL base uses default"
    #[test]
    fn palette_generate_null_base_uses_default() {
        let base = color::DEFAULT;
        let bg = RGB {
            r: 5,
            g: 10,
            b: 15,
        };
        let fg = RGB {
            r: 245,
            g: 250,
            b: 255,
        };

        let null_base = generate_palette(None, None, bg, fg, false);
        let explicit = generate_palette(Some(&base), None, bg, fg, false);
        expect_palette_c(&explicit, &null_base);
    }

    // test "color: palette_generate NULL skip"
    #[test]
    fn palette_generate_null_skip() {
        let base = color::DEFAULT;
        let bg = RGB {
            r: 5,
            g: 10,
            b: 15,
        };
        let fg = RGB {
            r: 245,
            g: 250,
            b: 255,
        };
        let skip = PaletteMask { bits: [0; 4] };

        let null_skip = generate_palette(Some(&base), None, bg, fg, false);
        let explicit = generate_palette(Some(&base), Some(&skip), bg, fg, false);
        expect_palette_c(&explicit, &null_skip);
    }

    // test "color: palette_generate matches generate256Color"
    #[test]
    fn palette_generate_matches_generate_256_color() {
        let base_c = color::DEFAULT;
        let bg = RGB {
            r: 16,
            g: 32,
            b: 48,
        };
        let fg = RGB {
            r: 240,
            g: 224,
            b: 208,
        };
        let mut skip_c = PaletteMask { bits: [0; 4] };
        set_mask_bit(&mut skip_c, 20);
        set_mask_bit(&mut skip_c, 100);
        set_mask_bit(&mut skip_c, 240);

        let actual = generate_palette(Some(&base_c), Some(&skip_c), bg, fg, true);
        let expected = rustty_vt::color::generate_256_color(base_c, skip_c.to_zig(), bg, fg, true);
        expect_palette_c(&expected, &actual);
    }

    // test "color: palette_generate in-place"
    #[test]
    fn palette_generate_in_place() {
        let mut base = color::DEFAULT;
        let bg = RGB { r: 0, g: 0, b: 0 };
        let fg = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let expected = generate_palette(Some(&base), None, bg, fg, false);

        unsafe {
            let base_ptr: *mut PaletteC = &mut base;
            palette_generate(
                base_ptr,
                std::ptr::null(),
                &bg as *const RgbC,
                &fg as *const RgbC,
                false,
                base_ptr,
            )
        };
        expect_palette_c(&expected, &base);
    }

    // test "color: luminance"
    #[test]
    fn luminance_matches_rgb_method() {
        let black = RGB { r: 0, g: 0, b: 0 };
        let white = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        assert!((0.0 - unsafe { luminance(&black) }).abs() < 0.000001);
        assert!((1.0 - unsafe { luminance(&white) }).abs() < 0.000001);

        let samples = [
            RGB {
                r: 40,
                g: 44,
                b: 52,
            },
            RGB {
                r: 171,
                g: 205,
                b: 239,
            },
            RGB {
                r: 12,
                g: 34,
                b: 56,
            },
        ];
        for sample in samples {
            assert!((sample.luminance() - unsafe { luminance(&sample) }).abs() < 0.000001);
        }
    }

    // test "color: perceived_luminance"
    #[test]
    fn perceived_luminance_matches_rgb_method() {
        let black = RGB { r: 0, g: 0, b: 0 };
        let white = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let dark = RGB {
            r: 0x28,
            g: 0x2C,
            b: 0x34,
        };
        assert!((0.0 - unsafe { perceived_luminance(&black) }).abs() < 0.000001);
        assert!((1.0 - unsafe { perceived_luminance(&white) }).abs() < 0.000001);
        assert!(unsafe { perceived_luminance(&dark) } < 0.5);
        assert!(unsafe { perceived_luminance(&white) } > 0.5);

        let samples = [
            dark,
            RGB {
                r: 171,
                g: 205,
                b: 239,
            },
            RGB {
                r: 12,
                g: 34,
                b: 56,
            },
        ];
        for sample in samples {
            assert!(
                (sample.perceived_luminance() - unsafe { perceived_luminance(&sample) }).abs()
                    < 0.000001
            );
        }
    }

    // test "color: contrast"
    #[test]
    fn contrast_matches_rgb_method() {
        let black = RGB { r: 0, g: 0, b: 0 };
        let white = RGB {
            r: 255,
            g: 255,
            b: 255,
        };
        let gray = RGB {
            r: 128,
            g: 128,
            b: 128,
        };

        assert!((21.0 - unsafe { contrast(&white, &black) }).abs() < 0.000001);
        assert!(
            (unsafe { contrast(&black, &white) } - unsafe { contrast(&white, &black) }).abs()
                < 0.000001
        );
        assert!((1.0 - unsafe { contrast(&gray, &gray) }).abs() < 0.000001);
    }

    // test "color: x11_names"
    #[test]
    fn x11_names_walk() {
        let names = x11_names();
        let count = x11_name_count();
        assert_eq!(rustty_vt::x11_color::entries().len(), count);
        assert!(count > 700);
        let mut walked = 0usize;
        unsafe {
            while !(*names.add(walked)).name.is_null() {
                let name = std::ffi::CStr::from_ptr((*names.add(walked)).name)
                    .to_str()
                    .unwrap();
                let expected = rustty_vt::x11_color::map.get(name).unwrap();
                expect_rgb(expected, (*names.add(walked)).color);
                walked += 1;
            }
            assert_eq!(count, walked);
            assert!((*names.add(count)).name.is_null());
        }
    }

    // test "color: x11_names static"
    #[test]
    fn x11_names_static() {
        assert_eq!(x11_names() as usize, x11_names() as usize);
    }
}
