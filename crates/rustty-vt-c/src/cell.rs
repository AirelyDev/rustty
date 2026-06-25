//! rustty-vt-c cell
//!
//! PORTS: ghostty src/terminal/c/cell.zig
//!
//! PORT STATUS: FILLED (cluster Stage B v2). Types are byte-exact and the
//! `get`/`get_multi`/`getTyped` bodies + all `test {}` blocks are ported. The
//! `#[repr(C)]` enums carry their reference discriminants so the C ABI is
//! anchored.

use crate::result::Result;
use std::ffi::c_void;

// const page = @import("../page.zig");
// const Cell = page.Cell;
// const color = @import("../color.zig");
// const style_c = @import("style.zig");
use crate::color::RgbC;
use rustty_vt::page::Cell;

/// C: GhosttyCell
// pub const CCell = u64;
pub type CCell = u64;

/// C: GhosttyCellContentTag
// pub const ContentTag = enum(c_int) { codepoint = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentTag {
    Codepoint = 0,
    CodepointGrapheme = 1,
    BgColorPalette = 2,
    BgColorRgb = 3,
}

/// C: GhosttyCellWide
// pub const Wide = enum(c_int) { narrow = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wide {
    Narrow = 0,
    Wide = 1,
    SpacerTail = 2,
    SpacerHead = 3,
}

/// C: GhosttyCellSemanticContent
// pub const SemanticContent = enum(c_int) { output = 0, input = 1, prompt = 2 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticContent {
    Output = 0,
    Input = 1,
    Prompt = 2,
}

/// C: GhosttyCellData
// pub const CellData = enum(c_int) { invalid = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellData {
    Invalid = 0,

    /// The codepoint of the cell (0 if empty or bg-color-only).
    /// Output type: uint32_t * (stored as u21, zero-extended)
    Codepoint = 1,

    /// The content tag describing what kind of content is in the cell.
    /// Output type: GhosttyCellContentTag *
    ContentTag = 2,

    /// The wide property of the cell.
    /// Output type: GhosttyCellWide *
    Wide = 3,

    /// Whether the cell has text to render.
    /// Output type: bool *
    HasText = 4,

    /// Whether the cell has styling (non-default style).
    /// Output type: bool *
    HasStyling = 5,

    /// The style ID for the cell (for use with style lookups).
    /// Output type: uint16_t *
    StyleId = 6,

    /// Whether the cell has a hyperlink.
    /// Output type: bool *
    HasHyperlink = 7,

    /// Whether the cell is protected.
    /// Output type: bool *
    Protected = 8,

    /// The semantic content type of the cell (from OSC 133).
    /// Output type: GhosttyCellSemanticContent *
    SemanticContent = 9,

    /// The palette index for the cell's background color.
    /// Only valid when content_tag is bg_color_palette.
    /// Output type: GhosttyColorPaletteIndex *
    ColorPalette = 10,

    /// The RGB value for the cell's background color.
    /// Only valid when content_tag is bg_color_rgb.
    /// Output type: GhosttyColorRgb *
    ColorRgb = 11,
}

// Output type expected for querying the data of the given kind.
// PORT NOTE: `pub fn OutType(comptime self: CellData) type` selects the output
// type per data kind:
//     .invalid => void,
//     .codepoint => u32,
//     .content_tag => ContentTag,
//     .wide => Wide,
//     .has_text, .has_styling, .has_hyperlink, .protected => bool,
//     .style_id => u16,
//     .semantic_content => SemanticContent,
//     .color_palette => u8,
//     .color_rgb => color.RGB.C,
// In Rust the `getTyped` comptime dispatch collapses into a runtime match; the
// OutType mapping is preserved as the per-arm pointer cast in `get_typed`.

/// PORT NOTE: `pub fn get(cell_, data, out) callconv(...) Result`.
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get(cell_: CCell, data: CellData, out: *mut c_void) -> Result {
    // PORT NOTE: the runtime-safety `intToEnum(CellData, @intFromEnum(data))`
    // guard on the raw `c_int` crossing the C boundary lives in the `extern "C"`
    // wrapper in `lib.rs` (`rustty_cell_get`); by the time `get` is reached the
    // `data` enum is already validated.

    // return switch (data) {
    //     .invalid => .invalid_value,
    //     inline else => |comptime_data| getTyped(cell_, comptime_data, @ptrCast(@alignCast(out))),
    // };
    match data {
        CellData::Invalid => Result::InvalidValue,
        // SAFETY: `out` validity/type is the caller's contract per `data`.
        _ => unsafe { get_typed(cell_, data, out) },
    }
}

/// PORT NOTE: `pub fn get_multi(cell_, count, keys, values, out_written) Result`.
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
pub unsafe fn get_multi(
    cell_: CCell,
    count: usize,
    keys: *const CellData,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // const k = keys orelse return .invalid_value;
    // const v = values orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    if values.is_null() {
        return Result::InvalidValue;
    }

    // for (0..count) |i| {
    //     const result = get(cell_, k[i], v[i]);
    //     if (result != .success) {
    //         if (out_written) |w| w.* = i;
    //         return result;
    //     }
    // }
    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // SAFETY: `value` validity/type is the caller's contract.
        let result = unsafe { get(cell_, key, value) };
        if result != Result::Success {
            if !out_written.is_null() {
                // SAFETY: caller-provided out pointer, checked non-null.
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    // if (out_written) |w| w.* = count;
    if !out_written.is_null() {
        // SAFETY: caller-provided out pointer, checked non-null.
        unsafe { *out_written = count };
    }
    // return .success;
    Result::Success
}

/// PORT NOTE: `fn getTyped(cell_, comptime data, out: *data.OutType()) Result`.
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
unsafe fn get_typed(cell_: CCell, data: CellData, out: *mut c_void) -> Result {
    // const cell: Cell = @bitCast(cell_);
    let cell = Cell::from_bits(cell_);
    match data {
        // .invalid => return .invalid_value,
        CellData::Invalid => return Result::InvalidValue,
        // .codepoint => out.* = @intCast(cell.codepoint()),
        CellData::Codepoint => {
            // SAFETY: `out` points to a writable `u32` per OutType.
            // PORT NOTE: `@intCast(cell.codepoint())` to u32; `codepoint()` is
            // already `u21` (= `u32` in rustty), so the cast is the identity.
            unsafe { *(out as *mut u32) = cell.codepoint() };
        }
        // .content_tag => out.* = @enumFromInt(@intFromEnum(cell.content_tag)),
        CellData::ContentTag => {
            let v = match cell.content_tag() {
                rustty_vt::page::ContentTag::Codepoint => ContentTag::Codepoint,
                rustty_vt::page::ContentTag::CodepointGrapheme => ContentTag::CodepointGrapheme,
                rustty_vt::page::ContentTag::BgColorPalette => ContentTag::BgColorPalette,
                rustty_vt::page::ContentTag::BgColorRgb => ContentTag::BgColorRgb,
            };
            // SAFETY: `out` points to a writable `ContentTag` per OutType.
            unsafe { *(out as *mut ContentTag) = v };
        }
        // .wide => out.* = @enumFromInt(@intFromEnum(cell.wide)),
        CellData::Wide => {
            let v = match cell.wide() {
                rustty_vt::page::Wide::Narrow => Wide::Narrow,
                rustty_vt::page::Wide::Wide => Wide::Wide,
                rustty_vt::page::Wide::SpacerTail => Wide::SpacerTail,
                rustty_vt::page::Wide::SpacerHead => Wide::SpacerHead,
            };
            // SAFETY: `out` points to a writable `Wide` per OutType.
            unsafe { *(out as *mut Wide) = v };
        }
        // .has_text => out.* = cell.hasText(),
        CellData::HasText => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = cell.has_text() };
        }
        // .has_styling => out.* = cell.hasStyling(),
        CellData::HasStyling => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = cell.has_styling() };
        }
        // .style_id => out.* = cell.style_id,
        CellData::StyleId => {
            // SAFETY: `out` points to a writable `u16` per OutType.
            unsafe { *(out as *mut u16) = cell.style_id() };
        }
        // .has_hyperlink => out.* = cell.hyperlink,
        CellData::HasHyperlink => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = cell.hyperlink() };
        }
        // .protected => out.* = cell.protected,
        CellData::Protected => {
            // SAFETY: `out` points to a writable `bool` per OutType.
            unsafe { *(out as *mut bool) = cell.protected() };
        }
        // .semantic_content => out.* = @enumFromInt(@intFromEnum(cell.semantic_content)),
        CellData::SemanticContent => {
            let v = match cell.semantic_content() {
                rustty_vt::page::SemanticContent::Output => SemanticContent::Output,
                rustty_vt::page::SemanticContent::Input => SemanticContent::Input,
                rustty_vt::page::SemanticContent::Prompt => SemanticContent::Prompt,
            };
            // SAFETY: `out` points to a writable `SemanticContent` per OutType.
            unsafe { *(out as *mut SemanticContent) = v };
        }
        // .color_palette => out.* = cell.content.color_palette,
        CellData::ColorPalette => {
            // SAFETY: `out` points to a writable `u8` per OutType.
            unsafe { *(out as *mut u8) = cell.content_color_palette() };
        }
        // .color_rgb => { const rgb = cell.content.color_rgb; out.* = .{ .r = rgb.r, .g = rgb.g, .b = rgb.b }; },
        CellData::ColorRgb => {
            let rgb = cell.content_color_rgb();
            // SAFETY: `out` points to a writable `color.RGB.C` per OutType.
            unsafe {
                *(out as *mut RgbC) = RgbC {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                };
            }
        }
    }

    // return .success;
    Result::Success
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::c_void;

    // test "get codepoint"
    #[test]
    fn get_codepoint() {
        // const cell: CCell = @bitCast(Cell.init('A'));
        let cell: CCell = Cell::init(b'A' as u32).to_bits();
        let mut cp: u32 = 0;
        // try testing.expectEqual(Result.success, get(cell, .codepoint, @ptrCast(&cp)));
        assert_eq!(Result::Success, unsafe {
            get(
                cell,
                CellData::Codepoint,
                &mut cp as *mut u32 as *mut c_void,
            )
        });
        // try testing.expectEqual(@as(u32, 'A'), cp);
        assert_eq!(b'A' as u32, cp);
    }

    // test "get has_text"
    #[test]
    fn get_has_text() {
        let cell: CCell = Cell::init(b'A' as u32).to_bits();
        let mut has: bool = false;
        assert_eq!(Result::Success, unsafe {
            get(
                cell,
                CellData::HasText,
                &mut has as *mut bool as *mut c_void,
            )
        });
        // try testing.expect(has);
        assert!(has);
    }

    // test "get empty cell"
    #[test]
    fn get_empty_cell() {
        // const cell: CCell = @bitCast(Cell.init(0));
        let cell: CCell = Cell::init(0u32).to_bits();
        let mut has: bool = true;
        assert_eq!(Result::Success, unsafe {
            get(
                cell,
                CellData::HasText,
                &mut has as *mut bool as *mut c_void,
            )
        });
        // try testing.expect(!has);
        assert!(!has);
    }

    // test "get wide"
    #[test]
    fn get_wide() {
        // var zig_cell = Cell.init('A'); zig_cell.wide = .wide;
        let mut zig_cell = Cell::init(b'A' as u32);
        zig_cell.set_wide(rustty_vt::page::Wide::Wide);
        // const cell: CCell = @bitCast(zig_cell);
        let cell: CCell = zig_cell.to_bits();
        let mut w: Wide = Wide::Narrow;
        assert_eq!(Result::Success, unsafe {
            get(cell, CellData::Wide, &mut w as *mut Wide as *mut c_void)
        });
        // try testing.expectEqual(Wide.wide, w);
        assert_eq!(Wide::Wide, w);
    }

    // test "get_multi success"
    #[test]
    fn get_multi_success() {
        // const cell: CCell = @bitCast(Cell.init('B'));
        let cell: CCell = Cell::init(b'B' as u32).to_bits();
        let mut cp: u32 = 0;
        let mut has_text: bool = false;
        let mut written: usize = 0;

        // const keys = [_]CellData{ .codepoint, .has_text };
        let keys = [CellData::Codepoint, CellData::HasText];
        // var values = [_]?*anyopaque{ @ptrCast(&cp), @ptrCast(&has_text) };
        let mut values = [
            &mut cp as *mut u32 as *mut c_void,
            &mut has_text as *mut bool as *mut c_void,
        ];
        // try testing.expectEqual(Result.success, get_multi(cell, keys.len, &keys, &values, &written));
        assert_eq!(Result::Success, unsafe {
            get_multi(
                cell,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        // try testing.expectEqual(keys.len, written);
        assert_eq!(keys.len(), written);
        // try testing.expectEqual(@as(u32, 'B'), cp);
        assert_eq!(b'B' as u32, cp);
        // try testing.expect(has_text);
        assert!(has_text);
    }

    // test "get_multi error sets out_written"
    #[test]
    fn get_multi_error_sets_out_written() {
        // const cell: CCell = @bitCast(Cell.init('C'));
        let cell: CCell = Cell::init(b'C' as u32).to_bits();
        let mut cp: u32 = 0;
        let mut written: usize = 99;

        // const keys = [_]CellData{ .codepoint, .invalid };
        let keys = [CellData::Codepoint, CellData::Invalid];
        // var values = [_]?*anyopaque{ @ptrCast(&cp), @ptrCast(&cp) };
        let cp_ptr = &mut cp as *mut u32 as *mut c_void;
        let mut values = [cp_ptr, cp_ptr];
        // try testing.expectEqual(Result.invalid_value, get_multi(cell, keys.len, &keys, &values, &written));
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                cell,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                &mut written,
            )
        });
        // try testing.expectEqual(1, written);
        assert_eq!(1, written);
        // try testing.expectEqual(@as(u32, 'C'), cp);
        assert_eq!(b'C' as u32, cp);
    }

    // test "get_multi null keys returns invalid_value"
    #[test]
    fn get_multi_null_keys_returns_invalid_value() {
        let cell: CCell = Cell::init(b'A' as u32).to_bits();
        let mut cp: u32 = 0;
        // var values = [_]?*anyopaque{@ptrCast(&cp)};
        let mut values = [&mut cp as *mut u32 as *mut c_void];
        // try testing.expectEqual(Result.invalid_value, get_multi(cell, 1, null, &values, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                cell,
                1,
                std::ptr::null(),
                values.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        });
    }

    // test "get_multi null values returns invalid_value"
    #[test]
    fn get_multi_null_values_returns_invalid_value() {
        let cell: CCell = Cell::init(b'A' as u32).to_bits();
        // const keys = [_]CellData{.codepoint};
        let keys = [CellData::Codepoint];
        // try testing.expectEqual(Result.invalid_value, get_multi(cell, 1, &keys, null, null));
        assert_eq!(Result::InvalidValue, unsafe {
            get_multi(
                cell,
                1,
                keys.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        });
    }
}
