//! rustty-vt-c style
//!
//! PORTS: ghostty src/terminal/c/style.zig
//!
//! PORT STATUS: SKELETON (cluster Stage B). The extern struct/union/enum layouts
//! are byte-exact; `from_color`/`from_style`/`default_style`/`style_is_default`
//! bodies are `todo!()` placeholders filled in a later sub-batch.

use rustty_vt::color::RGB;
use rustty_vt::style as style_pkg;

// const style = @import("../style.zig");
// const color = @import("../color.zig");
// const sgr = @import("../sgr.zig");

/// C: GhosttyStyleColorTag
// pub const ColorTag = enum(c_int) { none = 0, palette = 1, rgb = 2 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorTag {
    None = 0,
    Palette = 1,
    Rgb = 2,
}

/// C: GhosttyStyleColorValue
// pub const ColorValue = extern union { palette: u8, rgb: color.RGB.C, _padding: u64 };
#[repr(C)]
#[derive(Clone, Copy)]
pub union ColorValue {
    pub palette: u8,
    pub rgb: RGB,
    pub _padding: u64,
}

/// C: GhosttyStyleColor
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
        let _ = c;
        todo!("vt-c style::Color::from_color")
    }
}

/// C: GhosttyStyle
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
        let _ = s;
        todo!("vt-c style::Style::from_style")
    }
}

/// Returns the default style.
// pub fn default_style(result: *Style) callconv(lib.calling_conv) void
///
/// # Safety
/// `result` must be a valid, writable pointer to a `Style`.
pub unsafe fn default_style(result: *mut Style) {
    let _ = result;
    todo!("vt-c style::default_style")
}

/// Returns true if the style is the default style.
// pub fn style_is_default(s: *const Style) callconv(lib.calling_conv) bool
///
/// # Safety
/// `s` must be a valid, readable pointer to a `Style`.
pub unsafe fn style_is_default(s: *const Style) -> bool {
    let _ = s;
    todo!("vt-c style::style_is_default")
}
