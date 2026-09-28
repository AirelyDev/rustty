//! rustty-vt-c style
//!
//! PORTS: ghostty src/terminal/c/style.zig
//!
//! PORT STATUS: FILLED (cluster Stage B v2). The extern struct/union/enum layouts
//! are byte-exact; `from_color`/`from_style`/`default_style`/`style_is_default`
//! bodies + all `test {}` blocks are ported.

use rustty_vt::color::RGB;
use rustty_vt::style as style_pkg;
use std::mem::size_of;

// const style = @import("../style.zig");
// const color = @import("../color.zig");
// const sgr = @import("../sgr.zig");

/// C: GhosttyStyleColorTag -> RusttyStyleColorTag (R2)
// pub const ColorTag = enum(c_int) { none = 0, palette = 1, rgb = 2 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTag {
    None = 0,
    Palette = 1,
    Rgb = 2,
}

/// C: GhosttyStyleColorValue -> RusttyStyleColorValue (R2)
// pub const ColorValue = extern union { palette: u8, rgb: color.RGB.C, _padding: u64 };
#[repr(C)]
#[derive(Clone, Copy)]
pub union ColorValue {
    pub palette: u8,
    pub rgb: RGB,
    pub _padding: u64,
}

/// C: GhosttyStyleColor -> RusttyStyleColor (R2)
// pub const Color = extern struct { tag: ColorTag, value: ColorValue };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Color {
    pub tag: ColorTag,
    pub value: ColorValue,
}

impl Color {
    // // fromColor converts the tag as a plain integer, which requires
    // // the internal and C tag values to line up.
    // comptime {
    //     const Tag = std.meta.Tag(style.Style.Color);
    //     for (@typeInfo(Tag).@"enum".fields) |f| {
    //         assert(f.value == @intFromEnum(@field(ColorTag, f.name)));
    //     }
    // }
    // (the Rust analog is the module-level `const` block below `Color`)

    // pub fn fromColor(c: style.Style.Color) Color
    pub fn from_color(c: style_pkg::Color) -> Color {
        // PORT NOTE (WS19 T5): `style_pkg::Color` is a byte-complete `{ tag, data }`
        // value rather than a Rust enum (it lives inside the page mapping, which is
        // compressed as raw bytes), so the reference's `switch (c)` is a switch on
        // `c.tag()` with tag-checked payload accessors.
        //
        // The value is built as a single integer rather than through
        // the per-variant union fields: the C union layout puts
        // .palette at byte 0 and .rgb at bytes 0-2, which is exactly
        // the internal payload representation, so each variant is the
        // payload zero-extended to the u64 union backing. This keeps
        // the conversion to two stores (tag + value) per color, which
        // matters for the per-cell style reads in the render state
        // API.
        let value: u64 = match c.tag() {
            style_pkg::ColorTag::None => 0,
            style_pkg::ColorTag::Palette => c.palette() as u64,
            // `@as(u24, @bitCast(rgb))` — the packed `RGB` layout is
            // r | g<<8 | b<<16 (see `PackedStyle`'s `data_from_color`).
            style_pkg::ColorTag::Rgb => {
                let rgb = c.rgb();
                (rgb.r as u64) | ((rgb.g as u64) << 8) | ((rgb.b as u64) << 16)
            }
        };
        Color {
            // .tag = @enumFromInt(@intFromEnum(std.meta.activeTag(c))),
            // SAFETY: the internal and C tag values line up 1:1 (const-asserted
            // below), and `c.tag()` only yields declared variants.
            tag: unsafe {
                std::mem::transmute::<std::os::raw::c_int, ColorTag>(c.tag() as std::os::raw::c_int)
            },
            value: ColorValue { _padding: value },
        }
    }
}

// fromColor converts the tag as a plain integer, which requires
// the internal and C tag values to line up.
const _: () = {
    assert!(style_pkg::ColorTag::None as i64 == ColorTag::None as i64);
    assert!(style_pkg::ColorTag::Palette as i64 == ColorTag::Palette as i64);
    assert!(style_pkg::ColorTag::Rgb as i64 == ColorTag::Rgb as i64);
};

/// C: GhosttyStyle -> RusttyStyle (R2)
// pub const Style = extern struct {
//     size: usize = @sizeOf(Style),
//     fg_color: Color, bg_color: Color, underline_color: Color,
//     bold: bool, italic: bool, faint: bool, blink: bool, inverse: bool,
//     invisible: bool, strikethrough: bool, overline: bool, underline: c_int,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Style {
    pub size: usize,
    pub fg_color: Color,
    pub bg_color: Color,
    pub underline_color: Color,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub underline: std::os::raw::c_int,
}

impl Style {
    /// The default (empty) style in C representation. C
    // pub const default: Style = .{ ... };
    pub const DEFAULT: Style = Style {
        size: size_of::<Style>(),
        fg_color: Color {
            tag: ColorTag::None,
            value: ColorValue { _padding: 0 },
        },
        bg_color: Color {
            tag: ColorTag::None,
            value: ColorValue { _padding: 0 },
        },
        underline_color: Color {
            tag: ColorTag::None,
            value: ColorValue { _padding: 0 },
        },
        bold: false,
        italic: false,
        faint: false,
        blink: false,
        inverse: false,
        invisible: false,
        strikethrough: false,
        overline: false,
        underline: 0,
    };

    // // fromStyle writes the eight bool fields as one 8-byte store,
    // // which requires them to be consecutive and to match the low
    // // eight flag bits in order.
    // comptime {
    //     const flag_names = [8][:0]const u8{
    //         "bold",    "italic",    "faint",         "blink",
    //         "inverse", "invisible", "strikethrough", "overline",
    //     };
    //     const base = @offsetOf(Style, "bold");
    //     for (flag_names, 0..) |name, i| {
    //         assert(@offsetOf(Style, name) == base + i);
    //         assert(@bitOffsetOf(@FieldType(style.Style, "flags"), name) == i);
    //     }
    // }
    // (the C-struct half is the module-level `const` block below; the internal
    // `@bitOffsetOf` half is anchored by `Flags::to_u16`, whose bit order —
    // bold..overline in bits 0..7 — IS rustty's definition of the packed
    // layout and is exercised by the vt `flags_eq_matches_packed_eq` test.)

    /// Write the C representation of the given style directly through
    /// the out pointer. The hot per-cell style read in the render
    /// state API uses this rather than `out.* = fromStyle(s)`: the
    /// by-value form materializes a stack temporary plus a
    /// @sizeOf(Style)-byte copy that LLVM does not elide.
    // pub fn write(s: style.Style, out: *Style) void { ... }
    pub fn write(s: style_pkg::Style, out: &mut Style) {
        // out.size = @sizeOf(Style);
        out.size = size_of::<Style>();
        // out.fg_color = .fromColor(s.fg_color);
        out.fg_color = Color::from_color(s.fg_color);
        // out.bg_color = .fromColor(s.bg_color);
        out.bg_color = Color::from_color(s.bg_color);
        // out.underline_color = .fromColor(s.underline_color);
        out.underline_color = Color::from_color(s.underline_color);

        // I know this looks scary, but this results in truly measurable
        // performance improvements due to how often styles are written
        // especially in render loops. We vectorize our 8-bit to 8-byte
        // write.
        //
        // We have to align(1) because we're ptrCasting a u8 pointer.
        // @as(*align(1) u64, @ptrCast(&out.bold)).* = bytes: { ... };
        let bytes: u64 = {
            // Spread the low eight flag bits (the eight bool fields, in
            // order) into one byte each and write them with a single
            // 8-byte store.
            // const flag_bits: u8 = @truncate(@as(u16, @bitCast(s.flags)));
            let flag_bits: u8 = s.flags.to_u16() as u8;
            // const masks: @Vector(8, u8) = .{ 1, 2, 4, 8, 16, 32, 64, 128 };
            // const hits = (@splat(flag_bits) & masks) == masks;
            // const bytes: @Vector(8, u8) = @select(u8, hits, @splat(1), @splat(0));
            let masks: [u8; 8] = [1, 2, 4, 8, 16, 32, 64, 128];
            let mut lanes = [0u8; 8];
            let mut i = 0;
            while i < 8 {
                lanes[i] = if flag_bits & masks[i] == masks[i] {
                    1
                } else {
                    0
                };
                i += 1;
            }
            u64::from_ne_bytes(lanes)
        };
        // SAFETY: the eight bool fields are consecutive starting at `bold`
        // (const-asserted below) with unique 0/1 representations, so the
        // unaligned 8-byte store writes exactly those fields. The pointer is
        // derived from the whole `Style`, not from `&mut out.bold`: a
        // one-byte field reference carries provenance for that byte only,
        // and an 8-byte store through it is out of bounds (Stacked Borrows;
        // task C-1, NEW-FINDINGS N393).
        unsafe {
            let base = out as *mut Style as *mut u8;
            (base.add(core::mem::offset_of!(Style, bold)) as *mut u64).write_unaligned(bytes)
        };

        // out.underline = @intFromEnum(s.flags.underline);
        out.underline = s.flags.underline as std::os::raw::c_int;
    }

    // pub fn fromStyle(s: style.Style) Style {
    //     var result: Style = undefined;
    //     write(s, &result);
    //     return result;
    // }
    //
    // PORT NOTE: `undefined` -> a `DEFAULT` seed (a `&mut` to uninit memory is
    // UB in Rust); `write` overwrites every field either way.
    pub fn from_style(s: style_pkg::Style) -> Style {
        let mut result: Style = Style::DEFAULT;
        Style::write(s, &mut result);
        result
    }
}

// fromStyle writes the eight bool fields as one 8-byte store, which requires
// them to be consecutive (and to match the low eight flag bits in order — the
// internal half of the reference's comptime block; see the note in `Style`).
const _: () = {
    use core::mem::offset_of;
    let base = offset_of!(Style, bold);
    assert!(offset_of!(Style, bold) == base);
    assert!(offset_of!(Style, italic) == base + 1);
    assert!(offset_of!(Style, faint) == base + 2);
    assert!(offset_of!(Style, blink) == base + 3);
    assert!(offset_of!(Style, inverse) == base + 4);
    assert!(offset_of!(Style, invisible) == base + 5);
    assert!(offset_of!(Style, strikethrough) == base + 6);
    assert!(offset_of!(Style, overline) == base + 7);
};

/// Returns the default style.
// pub fn default_style(result: *Style) callconv(lib.calling_conv) void
///
/// # Safety
/// `result` must be a valid, writable pointer to a `Style`.
pub unsafe fn default_style(result: *mut Style) {
    // result.* = .default;
    // SAFETY: `result` validity is the caller's contract.
    unsafe { *result = Style::DEFAULT };
    // assert(result.size == @sizeOf(Style));
    // SAFETY: `result` validity is the caller's contract.
    assert!(unsafe { (*result).size } == size_of::<Style>());
}

/// Returns true if the style is the default style.
// pub fn style_is_default(s: *const Style) callconv(lib.calling_conv) bool
///
/// # Safety
/// `s` must be a valid, readable pointer to a `Style`.
pub unsafe fn style_is_default(s: *const Style) -> bool {
    // assert(s.size == @sizeOf(Style));
    // SAFETY: `s` validity is the caller's contract.
    let s = unsafe { &*s };
    assert!(s.size == size_of::<Style>());
    // return s.fg_color.tag == .none and s.bg_color.tag == .none and
    //     s.underline_color.tag == .none and s.bold == false and ... and s.underline == 0;
    s.fg_color.tag == ColorTag::None
        && s.bg_color.tag == ColorTag::None
        && s.underline_color.tag == ColorTag::None
        && !s.bold
        && !s.italic
        && !s.faint
        && !s.blink
        && !s.inverse
        && !s.invisible
        && !s.strikethrough
        && !s.overline
        && s.underline == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_vt::color::RGB;
    use std::os::raw::c_int;

    // test "default style"
    #[test]
    fn default_style_test() {
        // var s: Style = undefined; default_style(&s);
        let mut s = std::mem::MaybeUninit::<Style>::uninit();
        unsafe { default_style(s.as_mut_ptr()) };
        let s = unsafe { s.assume_init() };
        // try testing.expect(style_is_default(&s));
        assert!(unsafe { style_is_default(&s) });
        // try testing.expectEqual(ColorTag.none, s.fg_color.tag);
        assert_eq!(ColorTag::None, s.fg_color.tag);
        assert_eq!(ColorTag::None, s.bg_color.tag);
        assert_eq!(ColorTag::None, s.underline_color.tag);
        // try testing.expect(!s.bold);
        assert!(!s.bold);
        assert!(!s.italic);
        // try testing.expectEqual(@as(c_int, 0), s.underline);
        assert_eq!(0 as c_int, s.underline);
    }

    // test "convert style with colors"
    #[test]
    fn convert_style_with_colors() {
        // const zig_style: style.Style = .{
        //     .fg_color = .{ .palette = 42 },
        //     .bg_color = .{ .rgb = .{ .r = 255, .g = 128, .b = 64 } },
        //     .underline_color = .none,
        //     .flags = .{ .bold = true, .underline = .curly },
        // };
        let zig_style = style_pkg::Style {
            fg_color: style_pkg::Color::Palette(42),
            bg_color: style_pkg::Color::Rgb(RGB {
                r: 255,
                g: 128,
                b: 64,
            }),
            underline_color: style_pkg::Color::None,
            flags: style_pkg::Flags {
                bold: true,
                underline: rustty_vt::sgr::Underline::Curly,
                ..Default::default()
            },
        };

        // const c_style: Style = .fromStyle(zig_style);
        let c_style = Style::from_style(zig_style);
        // try testing.expectEqual(ColorTag.palette, c_style.fg_color.tag);
        assert_eq!(ColorTag::Palette, c_style.fg_color.tag);
        // try testing.expectEqual(@as(u8, 42), c_style.fg_color.value.palette);
        assert_eq!(42u8, unsafe { c_style.fg_color.value.palette });
        // try testing.expectEqual(ColorTag.rgb, c_style.bg_color.tag);
        assert_eq!(ColorTag::Rgb, c_style.bg_color.tag);
        // try testing.expectEqual(@as(u8, 255), c_style.bg_color.value.rgb.r);
        assert_eq!(255u8, unsafe { c_style.bg_color.value.rgb.r });
        assert_eq!(128u8, unsafe { c_style.bg_color.value.rgb.g });
        assert_eq!(64u8, unsafe { c_style.bg_color.value.rgb.b });
        // try testing.expectEqual(ColorTag.none, c_style.underline_color.tag);
        assert_eq!(ColorTag::None, c_style.underline_color.tag);
        // try testing.expect(c_style.bold);
        assert!(c_style.bold);
        // try testing.expectEqual(@as(c_int, 3), c_style.underline);
        assert_eq!(3 as c_int, c_style.underline);
        // try testing.expect(!style_is_default(&c_style));
        assert!(!unsafe { style_is_default(&c_style) });
    }
}
