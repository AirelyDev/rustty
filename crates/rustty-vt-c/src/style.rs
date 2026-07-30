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
    // pub fn fromColor(c: style.Style.Color) Color
    pub fn from_color(c: style_pkg::Color) -> Color {
        // return switch (c) {
        //     .none => .{ .tag = .none, .value = .{ ._padding = 0 } },
        //     .palette => |idx| .{ .tag = .palette, .value = .{ .palette = idx } },
        //     .rgb => |rgb| .{ .tag = .rgb, .value = .{ .rgb = rgb.cval() } },
        // };
        // PORT NOTE (WS19 T5): `style_pkg::Color` is a byte-complete `{ tag, data }`
        // value rather than a Rust enum (it lives inside the page mapping, which is
        // compressed as raw bytes), so the reference's `switch (c)` is a switch on
        // `c.tag()` with tag-checked payload accessors.
        match c.tag() {
            style_pkg::ColorTag::None => Color {
                tag: ColorTag::None,
                value: ColorValue { _padding: 0 },
            },
            style_pkg::ColorTag::Palette => Color {
                tag: ColorTag::Palette,
                value: ColorValue {
                    palette: c.palette(),
                },
            },
            // PORT NOTE: `rgb.cval()` yields `color.RGB.C`; in rustty `RGB` already
            // IS that `#[repr(C)] { r, g, b: u8 }` C-ABI type, so `cval()` is the
            // identity on `RGB`.
            style_pkg::ColorTag::Rgb => Color {
                tag: ColorTag::Rgb,
                value: ColorValue { rgb: c.rgb() },
            },
        }
    }
}

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
    // pub fn fromStyle(s: style.Style) Style
    pub fn from_style(s: style_pkg::Style) -> Style {
        // return .{
        //     .fg_color = .fromColor(s.fg_color),
        //     .bg_color = .fromColor(s.bg_color),
        //     .underline_color = .fromColor(s.underline_color),
        //     .bold = s.flags.bold, .italic = s.flags.italic, .faint = s.flags.faint,
        //     .blink = s.flags.blink, .inverse = s.flags.inverse, .invisible = s.flags.invisible,
        //     .strikethrough = s.flags.strikethrough, .overline = s.flags.overline,
        //     .underline = @intFromEnum(s.flags.underline),
        // };
        Style {
            // size: usize = @sizeOf(Style),
            size: size_of::<Style>(),
            fg_color: Color::from_color(s.fg_color),
            bg_color: Color::from_color(s.bg_color),
            underline_color: Color::from_color(s.underline_color),
            bold: s.flags.bold,
            italic: s.flags.italic,
            faint: s.flags.faint,
            blink: s.flags.blink,
            inverse: s.flags.inverse,
            invisible: s.flags.invisible,
            strikethrough: s.flags.strikethrough,
            overline: s.flags.overline,
            underline: s.flags.underline as std::os::raw::c_int,
        }
    }
}

/// Returns the default style.
// pub fn default_style(result: *Style) callconv(lib.calling_conv) void
///
/// # Safety
/// `result` must be a valid, writable pointer to a `Style`.
pub unsafe fn default_style(result: *mut Style) {
    // result.* = .fromStyle(.{});
    // SAFETY: `result` validity is the caller's contract.
    unsafe { *result = Style::from_style(style_pkg::Style::default()) };
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
