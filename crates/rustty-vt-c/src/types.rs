//! Comptime-generated metadata describing the layout of all C API
//! extern structs for the current target.
//!
//! This is embedded in the binary as a const string and exposed via
//! `rustty_type_json` so that WASM (and other FFI) consumers can
//! build structs without hardcoding byte offsets.

// const std = @import("std");
// const lib = @import("../lib.zig");
// const color = @import("../color.zig");
// const mouse_event = @import("mouse_event.zig");
// const point = @import("../point.zig");
// const size_report = @import("size_report.zig");
//
// const terminal = @import("terminal.zig");
// const formatter = @import("formatter.zig");
// const selection = @import("selection.zig");
// const selection_gesture = @import("selection_gesture.zig");
// const render = @import("render.zig");
// const style_c = @import("style.zig");
// const mouse_encode = @import("mouse_encode.zig");
// const grid_ref = @import("grid_ref.zig");
//
// PORT NOTE (R3, comptime): the reference generates `json` at COMPTIME by
// reflecting over each extern struct (`@typeInfo`, `@offsetOf`, `@sizeOf`,
// `@alignOf`). Rust has no comptime type reflection, so the metadata ROWS are
// spelled out below, but everything else is the same comptime story: the rows
// are `const` (`core::mem::offset_of!` / `size_of` / `align_of` are all
// const-evaluable over the identical `#[repr(C)]` projections) and the JSON is
// serialized by `const fn`s into a `static` byte array with a NUL sentinel,
// exactly like the reference's `json: [:0]const u8`.
//
// This must NOT be a lazily-built runtime cache: `rustty_type_json` is a getter
// that WASM embedders call while holding `DataView`/`TypedArray` views over
// `wasmMemory.buffer` (see `include/rustty/vt/wasm.h`), and a first-call heap
// allocation grows the linear memory, DETACHING every one of those views. The
// reference cannot do that because its string is static data; neither can this.
//
// The produced JSON is byte-identical to the reference's (same struct order,
// same field order, same `std.json.Stringify` formatting). The per-field `type`
// strings reproduce the reference `typeName` map (bool/f32/f64/i*/u*/enum/
// struct/pointer/array/opaque) applied to each Zig field type.
use std::ffi::c_char;
use std::os::raw::c_int;

use core::mem::{align_of, offset_of, size_of};

// PORT NOTE: `terminal.DeviceAttributes` is `Effects.CDeviceAttributes` with the
// nested `Primary`/`Secondary`/`Tertiary`. The rustty C-ABI projections live in
// `terminal.rs`.
use crate::terminal::{CDeviceAttributes, CDeviceAttributesPrimary};
use crate::terminal::{CDeviceAttributesSecondary, CDeviceAttributesTertiary};

/// C: RusttySurfacePosition (R2)
// pub const SurfacePosition = extern struct { x: f64, y: f64 };
// PORT NOTE: the reference declares `SurfacePosition`/`Codepoints` in this
// module; the existing rustty port placed the identical `#[repr(C)]` layouts in
// `selection_gesture.rs`. Reuse them here under the reference names to avoid a
// duplicate definition (same byte layout).
pub use crate::selection_gesture::Codepoints;
pub use crate::selection_gesture::SurfacePosition;

/// A single field's layout metadata.
// PORT NOTE: `StructInfo.FieldInfo = struct { name, offset, size, type }`.
struct FieldInfo {
    name: &'static str,
    offset: usize,
    size: usize,
    ty: &'static str,
}

/// Meta information about a struct that we expose to ease writing
/// bindings in some languages, particularly WASM where we can't
/// easily share struct definitions and need to hardcode byte offsets.
// pub const StructInfo = struct { name, size, @"align", fields: []const FieldInfo };
struct StructInfo {
    // PORT NOTE: `name` (`@typeName(T)`) is recorded by the reference but never
    // emitted by `jsonStringify` (it only writes size/align/fields), so it is
    // omitted here.
    size: usize,
    align: usize,
    fields: &'static [FieldInfo],
}

impl StructInfo {
    // pub fn jsonStringify(self: *const StructInfo, jws) ... {
    //     beginObject; "size":size; "align":align; "fields": beginObject;
    //     for fields: "name": { "offset":O, "size":S, "type":T }; endObject; endObject;
    // }
    const fn json_stringify(&self, out: &mut [u8], len: &mut usize) {
        push_str(out, len, "{");
        // "size"
        push_str(out, len, "\"size\":");
        push_usize(out, len, self.size);
        // "align"
        push_str(out, len, ",\"align\":");
        push_usize(out, len, self.align);
        // "fields"
        push_str(out, len, ",\"fields\":{");
        let mut i = 0;
        while i < self.fields.len() {
            if i > 0 {
                push_str(out, len, ",");
            }
            let f = &self.fields[i];
            push_json_string(out, len, f.name);
            push_str(out, len, ":{\"offset\":");
            push_usize(out, len, f.offset);
            push_str(out, len, ",\"size\":");
            push_usize(out, len, f.size);
            push_str(out, len, ",\"type\":");
            push_json_string(out, len, f.ty);
            push_str(out, len, "}");
            i += 1;
        }
        push_str(out, len, "}}");
    }
}

// PORT NOTE: `std.json.Stringify` writes integers in decimal and escapes strings
// minimally; these helpers reproduce that for the small, ASCII-only key set used
// here. They are `const fn` over a caller-owned byte buffer because the whole
// document is built at COMPILE TIME (see the module PORT NOTE): no allocator,
// no `String`, nothing that could run on a first call.
const fn push_str(out: &mut [u8], len: &mut usize, s: &str) {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        out[*len] = b[i];
        *len += 1;
        i += 1;
    }
}

const fn push_usize(out: &mut [u8], len: &mut usize, v: usize) {
    // Decimal, no separators — `std.json.Stringify`'s integer format.
    let mut digits = [0u8; 20];
    let mut n = 0;
    let mut v = v;
    if v == 0 {
        digits[0] = b'0';
        n = 1;
    }
    while v > 0 {
        digits[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        out[*len] = digits[n];
        *len += 1;
    }
}

const fn push_json_string(out: &mut [u8], len: &mut usize, s: &str) {
    push_str(out, len, "\"");
    // The struct/field names here are all simple ASCII identifiers; no escaping
    // is required, matching the reference output.
    push_str(out, len, s);
    push_str(out, len, "\"");
}

/// `&str` equality usable in `const fn` (`==` on `str` is not const).
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

// PORT NOTE: reproduce the reference's target-aware integer type strings.
// `types.zig` derives an unsigned field's `"u32"`/`"u64"` label from its actual
// `@bitSizeOf` on the build target, so `usize` is `"u32"` on wasm32 (4 bytes) and
// `"u64"` on 64-bit (8 bytes). The rows below hardcode `"u64"` for `usize` (right
// on 64-bit); deriving the label from the real `size_of` narrows it correctly on
// wasm32 while leaving 64-bit output byte-identical. Non-integer labels
// (`enum`/`bool`/`opaque`/`struct`) and fixed-width ints pass through unchanged.
const fn resolve_ty(ty: &'static str, size: usize) -> &'static str {
    if str_eq(ty, "u8") || str_eq(ty, "u16") || str_eq(ty, "u32") || str_eq(ty, "u64") {
        match size {
            1 => "u8",
            2 => "u16",
            4 => "u32",
            8 => "u64",
            _ => ty,
        }
    } else {
        ty
    }
}

// PORT NOTE: build one `StructInfo` from explicit `(field-name, offset, size,
// type-string)` rows. The reference derives these via `@typeInfo`/`@offsetOf`/
// `@sizeOf`; here `offset_of!`/`size_of` of the identical `#[repr(C)]` types
// produce the same numbers and the type strings reproduce `typeName(field.type)`.
macro_rules! struct_info {
    ($T:ty, [ $( ($name:literal, $ft:ty, $field:tt, $tystr:literal) ),* $(,)? ]) => {
        StructInfo {
            size: size_of::<$T>(),
            align: align_of::<$T>(),
            fields: &[
                $(
                    FieldInfo {
                        name: $name,
                        offset: offset_of!($T, $field),
                        size: size_of::<$ft>(),
                        ty: resolve_ty($tystr, size_of::<$ft>()),
                    },
                )*
            ],
        }
    };
}

/// All C API structs and their Rustty C names.
// pub const structs: std.StaticStringMap(StructInfo) = ...;
//
// PORT NOTE: an ordered `(name, StructInfo)` list (the reference `StaticStringMap`
// preserves insertion order in `keys()`/`values()`), evaluated at compile time.
const STRUCTS: &[(&str, StructInfo)] = {
    use crate::color::RgbC;
    use crate::formatter::{ScreenOptionsExtra, TerminalOptions, TerminalOptionsExtra};
    use crate::grid_ref::CGridRef;
    use crate::mouse_encode::Size as MouseEncoderSize;
    use crate::mouse_event::Position as MousePosition;
    use crate::point::PointC;
    use crate::render::Colors;
    use crate::selection::{CSelection, SelectLineOptions};
    use crate::selection::{SelectWordBetweenOptions, SelectWordOptions};
    use crate::selection_gesture::{Behaviors, Geometry};
    use crate::size_report::Size as SizeReportSize;
    use crate::style::{Color as StyleColor, Style};
    use crate::terminal::{DesktopNotification, ProgressReport, ScrollViewport, TerminalScrollbar};
    use rustty_lib::types::{Buffer, String as LibString};
    use rustty_vt::point::Coordinate as PointCoordinate;

    &[
        // .{ "GhosttyBuffer", StructInfo.init(lib.Buffer) },
        (
            "RusttyBuffer",
            struct_info!(Buffer, [
                // ptr: ?[*]u8 -> optional -> "opaque"; cap: usize -> "u64"; len: usize -> "u64"
                ("ptr", *mut u8, ptr, "opaque"),
                ("cap", usize, cap, "u64"),
                ("len", usize, len, "u64"),
            ]),
        ),
        // .{ "GhosttyClipboardContent", StructInfo.init(terminal.ClipboardContent) },
        (
            "RusttyClipboardContent",
            struct_info!(
                crate::terminal::ClipboardContent,
                [
                    ("mime", LibString, mime, "struct"),
                    ("data", LibString, data, "struct"),
                ]
            ),
        ),
        // .{ "GhosttyClipboardWrite", StructInfo.init(terminal.ClipboardWrite) },
        (
            "RusttyClipboardWrite",
            struct_info!(
                crate::terminal::ClipboardWrite,
                [
                    ("size", usize, size, "u64"),
                    // location: clipboard.Location (enum(c_int)) -> "enum"
                    ("location", rustty_vt::clipboard::Location, location, "enum"),
                    ("contents", *const crate::terminal::ClipboardContent, contents, "opaque"),
                    ("contents_len", usize, contents_len, "u64"),
                ]
            ),
        ),
        // .{ "GhosttyCodepoints", StructInfo.init(Codepoints) },
        (
            "RusttyCodepoints",
            struct_info!(Codepoints, [
                // ptr: ?[*]const u32 -> optional -> "opaque"; len: usize -> "u64"
                ("ptr", *const u32, ptr, "opaque"),
                ("len", usize, len, "u64"),
            ]),
        ),
        // .{ "GhosttyColorPaletteMask", StructInfo.init(color_c.PaletteMask) },
        (
            "RusttyColorPaletteMask",
            struct_info!(
                crate::color::PaletteMask,
                [("bits", [u64; 4], bits, "array"),]
            ),
        ),
        // .{ "GhosttyColorRgb", StructInfo.init(color.RGB.C) },
        (
            "RusttyColorRgb",
            struct_info!(
                RgbC,
                [("r", u8, r, "u8"), ("g", u8, g, "u8"), ("b", u8, b, "u8"),]
            ),
        ),
        // .{ "GhosttyColorX11Entry", StructInfo.init(color_c.X11Entry) },
        (
            "RusttyColorX11Entry",
            struct_info!(
                crate::color::X11Entry,
                [
                    ("name", *const c_char, name, "opaque"),
                    ("color", RgbC, color, "struct"),
                ]
            ),
        ),
        // .{ "GhosttyDeviceAttributes", StructInfo.init(terminal.DeviceAttributes) },
        (
            "RusttyDeviceAttributes",
            struct_info!(
                CDeviceAttributes,
                [
                    ("primary", CDeviceAttributesPrimary, primary, "struct"),
                    ("secondary", CDeviceAttributesSecondary, secondary, "struct"),
                    ("tertiary", CDeviceAttributesTertiary, tertiary, "struct"),
                ]
            ),
        ),
        // .{ "GhosttyDeviceAttributesPrimary", StructInfo.init(terminal.DeviceAttributes.Primary) },
        (
            "RusttyDeviceAttributesPrimary",
            struct_info!(
                CDeviceAttributesPrimary,
                [
                    ("conformance_level", u16, conformance_level, "u16"),
                    // features: [64]u16 -> "array"
                    ("features", [u16; 64], features, "array"),
                    ("num_features", usize, num_features, "u64"),
                ]
            ),
        ),
        // .{ "GhosttyDeviceAttributesSecondary", StructInfo.init(terminal.DeviceAttributes.Secondary) },
        (
            "RusttyDeviceAttributesSecondary",
            struct_info!(
                CDeviceAttributesSecondary,
                [
                    ("device_type", u16, device_type, "u16"),
                    ("firmware_version", u16, firmware_version, "u16"),
                    ("rom_cartridge", u16, rom_cartridge, "u16"),
                ]
            ),
        ),
        // .{ "GhosttyDeviceAttributesTertiary", StructInfo.init(terminal.DeviceAttributes.Tertiary) },
        (
            "RusttyDeviceAttributesTertiary",
            struct_info!(
                CDeviceAttributesTertiary,
                [("unit_id", u32, unit_id, "u32"),]
            ),
        ),
        // .{ "GhosttyFormatterTerminalOptions", StructInfo.init(formatter.TerminalOptions) },
        (
            "RusttyFormatterTerminalOptions",
            struct_info!(TerminalOptions, [
                ("size", usize, size, "u64"),
                // emit: Format (enum(c_int)) -> "enum"
                ("emit", crate::selection::Format, emit, "enum"),
                ("unwrap", bool, unwrap, "bool"),
                ("trim", bool, trim, "bool"),
                ("extra", TerminalOptionsExtra, extra, "struct"),
                // selection: ?*const CSelection -> optional -> "opaque"
                ("selection", *const CSelection, selection, "opaque"),
            ]),
        ),
        // .{ "GhosttySelection", StructInfo.init(selection.CSelection) },
        (
            "RusttySelection",
            struct_info!(
                CSelection,
                [
                    ("size", usize, size, "u64"),
                    ("start", CGridRef, start, "struct"),
                    ("end", CGridRef, end, "struct"),
                    ("rectangle", bool, rectangle, "bool"),
                ]
            ),
        ),
        // .{ "GhosttyTerminalSelectWordOptions", StructInfo.init(selection.SelectWordOptions) },
        (
            "RusttyTerminalSelectWordOptions",
            struct_info!(SelectWordOptions, [
                ("size", usize, size, "u64"),
                ("ref", CGridRef, ref_, "struct"),
                // boundary_codepoints: ?[*]const u32 -> optional -> "opaque"
                ("boundary_codepoints", *const u32, boundary_codepoints, "opaque"),
                ("boundary_codepoints_len", usize, boundary_codepoints_len, "u64"),
            ]),
        ),
        // .{ "GhosttyTerminalSelectWordBetweenOptions",
        //    StructInfo.init(selection.SelectWordBetweenOptions) },
        (
            "RusttyTerminalSelectWordBetweenOptions",
            struct_info!(SelectWordBetweenOptions, [
                ("size", usize, size, "u64"),
                ("start", CGridRef, start, "struct"),
                ("end", CGridRef, end, "struct"),
                ("boundary_codepoints", *const u32, boundary_codepoints, "opaque"),
                ("boundary_codepoints_len", usize, boundary_codepoints_len, "u64"),
            ]),
        ),
        // .{ "GhosttyTerminalSelectLineOptions", StructInfo.init(selection.SelectLineOptions) },
        (
            "RusttyTerminalSelectLineOptions",
            struct_info!(SelectLineOptions, [
                ("size", usize, size, "u64"),
                ("ref", CGridRef, ref_, "struct"),
                ("whitespace", *const u32, whitespace, "opaque"),
                ("whitespace_len", usize, whitespace_len, "u64"),
                ("semantic_prompt_boundary", bool, semantic_prompt_boundary, "bool"),
            ]),
        ),
        // .{ "GhosttyFormatterTerminalExtra", StructInfo.init(formatter.TerminalOptions.Extra) },
        (
            "RusttyFormatterTerminalExtra",
            struct_info!(
                TerminalOptionsExtra,
                [
                    ("size", usize, size, "u64"),
                    ("palette", bool, palette, "bool"),
                    ("modes", bool, modes, "bool"),
                    ("scrolling_region", bool, scrolling_region, "bool"),
                    ("tabstops", bool, tabstops, "bool"),
                    ("pwd", bool, pwd, "bool"),
                    ("keyboard", bool, keyboard, "bool"),
                    ("screen", ScreenOptionsExtra, screen, "struct"),
                ]
            ),
        ),
        // .{ "GhosttyFormatterScreenExtra", StructInfo.init(formatter.ScreenOptions.Extra) },
        (
            "RusttyFormatterScreenExtra",
            struct_info!(
                ScreenOptionsExtra,
                [
                    ("size", usize, size, "u64"),
                    ("cursor", bool, cursor, "bool"),
                    ("style", bool, style, "bool"),
                    ("hyperlink", bool, hyperlink, "bool"),
                    ("protection", bool, protection, "bool"),
                    ("kitty_keyboard", bool, kitty_keyboard, "bool"),
                    ("charsets", bool, charsets, "bool"),
                ]
            ),
        ),
        // .{ "GhosttyGridRef", StructInfo.init(grid_ref.CGridRef) },
        (
            "RusttyGridRef",
            struct_info!(CGridRef, [
                ("size", usize, size, "u64"),
                // node: ?*PageList.List.Node -> optional -> "opaque"
                ("node", *mut rustty_vt::pagelist::Node, node, "opaque"),
                // x/y: size.CellCountInt (u16) -> "u16"
                ("x", u16, x, "u16"),
                ("y", u16, y, "u16"),
            ]),
        ),
        // .{ "GhosttyMouseEncoderSize", StructInfo.init(mouse_encode.Size) },
        (
            "RusttyMouseEncoderSize",
            struct_info!(
                MouseEncoderSize,
                [
                    ("size", usize, size, "u64"),
                    ("screen_width", u32, screen_width, "u32"),
                    ("screen_height", u32, screen_height, "u32"),
                    ("cell_width", u32, cell_width, "u32"),
                    ("cell_height", u32, cell_height, "u32"),
                    ("padding_top", u32, padding_top, "u32"),
                    ("padding_bottom", u32, padding_bottom, "u32"),
                    ("padding_right", u32, padding_right, "u32"),
                    ("padding_left", u32, padding_left, "u32"),
                ]
            ),
        ),
        // .{ "GhosttyMousePosition", StructInfo.init(mouse_event.Position) },
        (
            "RusttyMousePosition",
            struct_info!(MousePosition, [("x", f32, x, "f32"), ("y", f32, y, "f32"),]),
        ),
        // .{ "GhosttyPoint", StructInfo.init(point.Point.C) },
        (
            "RusttyPoint",
            struct_info!(
                PointC,
                [
                    // tag: Tag (enum) -> "enum"; value: CValue (union) -> "opaque"
                    ("tag", crate::point::TagC, tag, "enum"),
                    ("value", crate::point::PointCValue, value, "opaque"),
                ]
            ),
        ),
        // .{ "GhosttyPointCoordinate", StructInfo.init(point.Coordinate) },
        (
            "RusttyPointCoordinate",
            struct_info!(
                PointCoordinate,
                [("x", u16, x, "u16"), ("y", u32, y, "u32"),]
            ),
        ),
        // .{ "GhosttyRenderStateColors", StructInfo.init(render.Colors) },
        (
            "RusttyRenderStateColors",
            struct_info!(
                Colors,
                [
                    ("size", usize, size, "u64"),
                    ("background", RgbC, background, "struct"),
                    ("foreground", RgbC, foreground, "struct"),
                    ("cursor", RgbC, cursor, "struct"),
                    ("cursor_has_value", bool, cursor_has_value, "bool"),
                    // palette: [256]RGB.C -> "array"
                    ("palette", [RgbC; 256], palette, "array"),
                ]
            ),
        ),
        // .{ "GhosttySelectionGestureBehaviors", StructInfo.init(selection_gesture.Behaviors) },
        (
            "RusttySelectionGestureBehaviors",
            struct_info!(
                Behaviors,
                [
                    (
                        "single_click",
                        crate::selection_gesture::Behavior,
                        single_click,
                        "enum"
                    ),
                    (
                        "double_click",
                        crate::selection_gesture::Behavior,
                        double_click,
                        "enum"
                    ),
                    (
                        "triple_click",
                        crate::selection_gesture::Behavior,
                        triple_click,
                        "enum"
                    ),
                ]
            ),
        ),
        // .{ "GhosttySelectionGestureGeometry", StructInfo.init(selection_gesture.Geometry) },
        (
            "RusttySelectionGestureGeometry",
            struct_info!(
                Geometry,
                [
                    ("columns", u32, columns, "u32"),
                    ("cell_width", u32, cell_width, "u32"),
                    ("padding_left", u32, padding_left, "u32"),
                    ("screen_height", u32, screen_height, "u32"),
                ]
            ),
        ),
        // .{ "GhosttySizeReportSize", StructInfo.init(size_report.Size) },
        (
            "RusttySizeReportSize",
            struct_info!(
                SizeReportSize,
                [
                    // rows/columns: CellCountInt (u16) -> "u16"
                    ("rows", u16, rows, "u16"),
                    ("columns", u16, columns, "u16"),
                    ("cell_width", u32, cell_width, "u32"),
                    ("cell_height", u32, cell_height, "u32"),
                ]
            ),
        ),
        // .{ "GhosttyString", StructInfo.init(lib.String) },
        (
            "RusttyString",
            struct_info!(LibString, [
                // ptr: [*]const u8 (non-optional) -> "pointer"; len: usize -> "u64"
                ("ptr", *const u8, ptr, "pointer"),
                ("len", usize, len, "u64"),
            ]),
        ),
        // .{ "GhosttySurfacePosition", StructInfo.init(SurfacePosition) },
        (
            "RusttySurfacePosition",
            struct_info!(
                SurfacePosition,
                [("x", f64, x, "f64"), ("y", f64, y, "f64"),]
            ),
        ),
        // .{ "GhosttyStyle", StructInfo.init(style_c.Style) },
        (
            "RusttyStyle",
            struct_info!(
                Style,
                [
                    ("size", usize, size, "u64"),
                    ("fg_color", StyleColor, fg_color, "struct"),
                    ("bg_color", StyleColor, bg_color, "struct"),
                    ("underline_color", StyleColor, underline_color, "struct"),
                    ("bold", bool, bold, "bool"),
                    ("italic", bool, italic, "bool"),
                    ("faint", bool, faint, "bool"),
                    ("blink", bool, blink, "bool"),
                    ("inverse", bool, inverse, "bool"),
                    ("invisible", bool, invisible, "bool"),
                    ("strikethrough", bool, strikethrough, "bool"),
                    ("overline", bool, overline, "bool"),
                    // underline: c_int -> "i32"
                    ("underline", c_int, underline, "i32"),
                ]
            ),
        ),
        // .{ "GhosttyStyleColor", StructInfo.init(style_c.Color) },
        (
            "RusttyStyleColor",
            struct_info!(
                StyleColor,
                [
                    // tag: ColorTag (enum) -> "enum"; value: ColorValue (union) -> "opaque"
                    ("tag", crate::style::ColorTag, tag, "enum"),
                    ("value", crate::style::ColorValue, value, "opaque"),
                ]
            ),
        ),
        // (oracle 03d5fa268 REMOVED the `GhosttyTerminalOptions` row along with
        // the struct itself; `json_parses` asserts the name is absent.)
        // .{ "GhosttyTerminalDesktopNotification", StructInfo.init(terminal.DesktopNotification) },
        //   (oracle c3655ba25)
        (
            "RusttyTerminalDesktopNotification",
            struct_info!(
                DesktopNotification,
                [
                    ("size", usize, size, "u64"),
                    // title/body: RusttyString (ptr+len over BORROWED BYTES)
                    ("title", LibString, title, "struct"),
                    ("body", LibString, body, "struct"),
                ]
            ),
        ),
        // .{ "GhosttyTerminalProgressReport", StructInfo.init(terminal.ProgressReport) },
        //   (oracle 47d602c42; the shared enum is 628adaf30's)
        (
            "RusttyTerminalProgressReport",
            struct_info!(
                ProgressReport,
                [
                    ("size", usize, size, "u64"),
                    // state: ProgressState (enum(c_int)) -> "enum"
                    ("state", crate::terminal::ProgressState, state, "enum"),
                    // progress: int8_t, -1 == omitted
                    ("progress", i8, progress, "i8"),
                ]
            ),
        ),
        // .{ "GhosttyTerminalScrollbar", StructInfo.init(terminal.TerminalScrollbar) },
        //
        // PORT NOTE: `TerminalScrollbar` is `PageList.Scrollbar.C`, a plain
        // extern struct of three `u64`s — NOT a union — so it reflects field by
        // field exactly like every other registered struct.
        (
            "RusttyTerminalScrollbar",
            struct_info!(
                TerminalScrollbar,
                [
                    ("total", u64, total, "u64"),
                    ("offset", u64, offset, "u64"),
                    ("len", u64, len, "u64"),
                ]
            ),
        ),
        // .{ "GhosttyTerminalScrollViewport", StructInfo.init(terminal.ScrollViewport) },
        //
        // PORT NOTE: `ScrollViewport` is `lib.TaggedUnion(...).C`, the extern
        // struct `{ tag: Tag, value: CValue }`; `StructInfo.init` reflects those
        // TWO fields, mapping `Tag` (an enum) to `"enum"` and `CValue` (a union,
        // which `typeName` does not name) to `"opaque"`. Rustty holds the tag as
        // the ABI-identical `c_int` (D11, see terminal.rs) — that is a Rust
        // soundness narrowing, not an ABI or reflection change, so the emitted
        // label stays `"enum"` as the reference reports it.
        (
            "RusttyTerminalScrollViewport",
            struct_info!(
                ScrollViewport,
                [
                    ("tag", c_int, tag, "enum"),
                    (
                        "value",
                        crate::terminal::ScrollViewportCValue,
                        value,
                        "opaque"
                    ),
                ]
            ),
        ),
    ]
};

/// Scratch capacity for the counting pass. Only ever alive during const
/// evaluation; the shipped `static` is exactly `JSON_LEN + 1` bytes.
const JSON_SCRATCH: usize = 32 * 1024;

// fn jsonWriteAll(writer) ... {
//     beginObject; for (structs.keys(), structs.values()) |name, *info| {
//         objectField(name); info.jsonStringify(&jws);
//     } endObject;
// }
const fn json_write_all(out: &mut [u8], len: &mut usize) {
    push_str(out, len, "{");
    let mut i = 0;
    while i < STRUCTS.len() {
        if i > 0 {
            push_str(out, len, ",");
        }
        let (name, info) = &STRUCTS[i];
        push_json_string(out, len, name);
        push_str(out, len, ":");
        info.json_stringify(out, len);
        i += 1;
    }
    push_str(out, len, "}");
}

// PORT NOTE: the reference's two-pass comptime build —
//   var counter: std.Io.Writer.Discarding = .init(&.{});
//   jsonWriteAll(&counter.writer); var buf: [counter.count:0]u8 = undefined;
//   jsonWriteAll(&(std.Io.Writer.fixed(&buf)));
// Same shape here: pass 1 writes into an oversized const scratch buffer purely
// to learn `JSON_LEN`, pass 2 copies exactly that many bytes into the shipped
// `static` and appends the `[:0]` sentinel. Both passes run at compile time, so
// nothing but the final byte array reaches the binary.
const fn json_len() -> usize {
    // The scratch buffer is a const-eval LOCAL, so it never reaches the binary;
    // only the `usize` it yields does.
    let mut buf = [0u8; JSON_SCRATCH];
    let mut len = 0usize;
    json_write_all(&mut buf, &mut len);
    len
}

const JSON_LEN: usize = json_len();

const fn json_bytes() -> [u8; JSON_LEN + 1] {
    let mut out = [0u8; JSON_LEN + 1];
    let mut len = 0usize;
    json_write_all(&mut out, &mut len);
    // [:0] sentinel — `out` is one byte longer and already zeroed.
    out
}

/// The comptime-generated NUL-terminated JSON string of all structs.
// /// The comptime-generated JSON string of all structs.
// pub const json: [:0]const u8 = ...;
// PORT NOTE: a `static` (not a `const`) so the C API returns ONE stable address
// for the `[*:0]const u8`, and so no code runs — and no allocator is touched —
// on the first `rustty_type_json` call. See the module PORT NOTE for why a lazy
// cache is not an option here (WASM view detachment).
static JSON: [u8; JSON_LEN + 1] = json_bytes();

/// The NUL-terminated bytes behind `get_json`, for the tests below.
#[cfg(test)]
fn json_cstring() -> &'static [u8] {
    &JSON
}

// /// Returns a pointer to the comptime-generated JSON string describing
// /// the layout of all C API extern structs, and writes its length to `len`.
// /// Exported as `ghostty_type_json` for FFI consumers.
// pub fn get_json() callconv(lib.calling_conv) [*:0]const u8 {
//     return json.ptr;
// }
//
/// Returns a pointer to the comptime-generated JSON string describing the layout
/// of all C API extern structs, and writes its length to `len`.
/// Exported as `rustty_type_json` for FFI consumers.
pub fn get_json() -> *const c_char {
    // return json.ptr;
    JSON.as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference parses `json` with `std.json.parseFromSlice`; the
    // tests below parse the cached JSON (sans the trailing NUL) into a minimal
    // tree and assert the same facts. A tiny purpose-built parser is used (no
    // serde dependency in this leaf); it handles the object/number/string subset
    // this JSON emits.
    fn parsed_json() -> JsonValue {
        let bytes = json_cstring();
        // Drop the trailing NUL sentinel.
        let s = std::str::from_utf8(&bytes[..bytes.len() - 1]).unwrap();
        let mut p = Parser {
            b: s.as_bytes(),
            i: 0,
        };
        let v = p.parse_value();
        p.skip_ws();
        assert_eq!(p.i, p.b.len(), "trailing JSON content");
        v
    }

    // test "json parses" {
    #[test]
    fn json_parses() {
        let root = parsed_json();
        let root = root.as_object();

        // Verify we have all expected structs
        // try testing.expect(root.contains("GhosttyFormatterTerminalOptions"));
        assert!(root
            .iter()
            .any(|(k, _)| k == "RusttyFormatterTerminalOptions"));
        // WS5 T6: registration for the new color utility C types.
        assert!(root.iter().any(|(k, _)| k == "RusttyColorPaletteMask"));
        assert!(root.iter().any(|(k, _)| k == "RusttyColorX11Entry"));
        // WS5 T17: clipboard write C types.
        assert!(root.iter().any(|(k, _)| k == "RusttyClipboardContent"));
        assert!(root.iter().any(|(k, _)| k == "RusttyClipboardWrite"));
        let clip_content = get(root, "RusttyClipboardContent").as_object();
        let clip_content_fields = get(clip_content, "fields").as_object();
        assert!(clip_content_fields.iter().any(|(k, _)| k == "mime"));
        assert!(clip_content_fields.iter().any(|(k, _)| k == "data"));
        let clip_write = get(root, "RusttyClipboardWrite").as_object();
        let clip_write_fields = get(clip_write, "fields").as_object();
        assert!(clip_write_fields.iter().any(|(k, _)| k == "size"));
        assert!(clip_write_fields.iter().any(|(k, _)| k == "location"));
        assert!(clip_write_fields.iter().any(|(k, _)| k == "contents"));
        assert!(clip_write_fields.iter().any(|(k, _)| k == "contents_len"));

        // try testing.expect(!root.contains("GhosttyTerminalOptions"));
        //
        // The NEGATIVE assertion oracle 03d5fa268 replaced the field checks
        // with: the options struct left the ABI entirely, and a re-introduction
        // must fail here rather than pass silently.
        assert!(!root.iter().any(|(k, _)| k == "RusttyTerminalOptions"));
    }

    // test "struct sizes are non-zero" {
    #[test]
    fn struct_sizes_are_non_zero() {
        let root = parsed_json();
        let root = root.as_object();
        // var it = parsed.value.object.iterator();
        // while (it.next()) |entry| { const size = struct_info.get("size").?.integer;
        //     try std.testing.expect(size > 0); }
        for (_, v) in root {
            let struct_info = v.as_object();
            let size = get(struct_info, "size").as_int();
            assert!(size > 0);
        }
    }

    // Rust-only regression test (no oracle counterpart): the reflected FIELD
    // MAPS of the two scroll structs and the `typeName` label of every enum
    // field. The oracle registers `PageList.Scrollbar.C` (an extern struct of
    // three u64s) and `Terminal.ScrollViewport.C` (`extern struct { tag: Tag,
    // value: CValue }`), so `ghostty_type_json` reports named fields for both,
    // and `typeName` maps any enum — `clipboard.Location` included — to
    // "enum", never to an integer label. Verified field-for-field against
    // `ghostty-vt.wasm`'s own `ghostty_type_json` output.
    #[test]
    fn scroll_struct_field_maps_and_enum_labels() {
        use crate::terminal::{ScrollViewport, TerminalScrollbar};

        let root = parsed_json();
        let root = root.as_object();

        // GhosttyTerminalScrollbar: {"total","offset","len"}, all u64.
        let sb = get(root, "RusttyTerminalScrollbar").as_object();
        let sb_fields = get(sb, "fields").as_object();
        assert_eq!(3, sb_fields.len());
        let expected: [(&str, usize); 3] = [
            ("total", offset_of!(TerminalScrollbar, total)),
            ("offset", offset_of!(TerminalScrollbar, offset)),
            ("len", offset_of!(TerminalScrollbar, len)),
        ];
        for (i, (name, offset)) in expected.iter().enumerate() {
            assert_eq!(*name, sb_fields[i].0, "scrollbar field {i}");
            let f = sb_fields[i].1.as_object();
            assert_eq!(*offset as i64, get(f, "offset").as_int());
            assert_eq!(size_of::<u64>() as i64, get(f, "size").as_int());
            assert_eq!("u64", get(f, "type").as_str());
        }

        // GhosttyTerminalScrollViewport: {"tag" (enum), "value" (opaque)} —
        // consumers must be able to find the tag at offset 0 and the payload
        // at its own offset, which a single whole-struct "value" field hides.
        let sv = get(root, "RusttyTerminalScrollViewport").as_object();
        let sv_fields = get(sv, "fields").as_object();
        assert_eq!(2, sv_fields.len());
        assert_eq!("tag", sv_fields[0].0);
        let tag = sv_fields[0].1.as_object();
        assert_eq!(
            offset_of!(ScrollViewport, tag) as i64,
            get(tag, "offset").as_int()
        );
        assert_eq!(size_of::<c_int>() as i64, get(tag, "size").as_int());
        assert_eq!("enum", get(tag, "type").as_str());
        assert_eq!("value", sv_fields[1].0);
        let value = sv_fields[1].1.as_object();
        assert_eq!(
            offset_of!(ScrollViewport, value) as i64,
            get(value, "offset").as_int()
        );
        assert_eq!(
            size_of::<crate::terminal::ScrollViewportCValue>() as i64,
            get(value, "size").as_int()
        );
        assert_eq!("opaque", get(value, "type").as_str());

        // Every enum-typed field is labelled "enum" (`typeName`), never by the
        // integer that carries it.
        let clip_write = get(root, "RusttyClipboardWrite").as_object();
        let clip_write_fields = get(clip_write, "fields").as_object();
        let location = get(clip_write_fields, "location").as_object();
        assert_eq!("enum", get(location, "type").as_str());
        let progress = get(root, "RusttyTerminalProgressReport").as_object();
        let progress_fields = get(progress, "fields").as_object();
        assert_eq!(
            "enum",
            get(get(progress_fields, "state").as_object(), "type").as_str()
        );
    }

    // Rust-only regression test (no oracle counterpart, because the oracle
    // cannot express the bug): the JSON must be STATIC data built at compile
    // time, exactly like the reference's `pub const json: [:0]const u8`. A
    // lazily built runtime cache allocates on the first `rustty_type_json`
    // call, which under WASM grows the linear memory and DETACHES every
    // `DataView`/`TypedArray` the embedder holds over `wasmMemory.buffer` —
    // the very views `include/rustty/vt/wasm.h` tells embedders to keep.
    // The constant IS the subject: `LEN` is a `const` binding of `JSON_LEN`
    // (the length of the `static`'s array type), so the assertion is what pins
    // the document to non-empty comptime data — and it guards the `LEN - 1`
    // index below.
    #[allow(clippy::assertions_on_constants)]
    #[test]
    fn json_is_comptime_static_data() {
        // The document's length is a CONST — it is the length of the `static`'s
        // array type — which is only possible if the whole document is built
        // during const evaluation rather than on the first call.
        const LEN: usize = JSON_LEN;
        assert!(LEN > 0);
        assert_eq!(b'{', JSON[0]);
        assert_eq!(b'}', JSON[LEN - 1]);

        // The `[:0]` sentinel lives in the same static allocation.
        assert_eq!(LEN + 1, JSON.len());
        assert_eq!(0u8, JSON[LEN]);

        // `get_json` hands out the address of that static — no per-call and no
        // lazily allocated buffer can be involved.
        assert!(std::ptr::eq(get_json() as *const u8, JSON.as_ptr()));
        assert_eq!(get_json(), get_json());
    }

    // --- tiny JSON value + parser (test-only) ---
    fn get<'a>(obj: &'a [(String, JsonValue)], key: &str) -> &'a JsonValue {
        obj.iter().find(|(k, _)| k == key).map(|(_, v)| v).unwrap()
    }

    enum JsonValue {
        Object(Vec<(String, JsonValue)>),
        Int(i64),
        Str(String),
    }

    impl JsonValue {
        fn as_object(&self) -> &[(String, JsonValue)] {
            match self {
                JsonValue::Object(o) => o,
                _ => panic!("not an object"),
            }
        }
        fn as_int(&self) -> i64 {
            match self {
                JsonValue::Int(n) => *n,
                _ => panic!("not an int"),
            }
        }
        fn as_str(&self) -> &str {
            match self {
                JsonValue::Str(s) => s,
                _ => panic!("not a string"),
            }
        }
    }

    struct Parser<'a> {
        b: &'a [u8],
        i: usize,
    }

    impl Parser<'_> {
        fn skip_ws(&mut self) {
            while self.i < self.b.len() && (self.b[self.i] as char).is_whitespace() {
                self.i += 1;
            }
        }
        fn parse_value(&mut self) -> JsonValue {
            self.skip_ws();
            match self.b[self.i] {
                b'{' => self.parse_object(),
                b'"' => JsonValue::Str(self.parse_string()),
                _ => self.parse_int(),
            }
        }
        fn parse_object(&mut self) -> JsonValue {
            let mut out = Vec::new();
            self.i += 1; // {
            self.skip_ws();
            if self.b[self.i] == b'}' {
                self.i += 1;
                return JsonValue::Object(out);
            }
            loop {
                self.skip_ws();
                let key = self.parse_string();
                self.skip_ws();
                assert_eq!(self.b[self.i], b':');
                self.i += 1;
                let val = self.parse_value();
                out.push((key, val));
                self.skip_ws();
                match self.b[self.i] {
                    b',' => {
                        self.i += 1;
                    }
                    b'}' => {
                        self.i += 1;
                        break;
                    }
                    c => panic!("unexpected {}", c as char),
                }
            }
            JsonValue::Object(out)
        }
        fn parse_string(&mut self) -> String {
            assert_eq!(self.b[self.i], b'"');
            self.i += 1;
            let start = self.i;
            while self.b[self.i] != b'"' {
                self.i += 1;
            }
            let s = std::str::from_utf8(&self.b[start..self.i])
                .unwrap()
                .to_string();
            self.i += 1; // closing "
            s
        }
        fn parse_int(&mut self) -> JsonValue {
            let start = self.i;
            if self.b[self.i] == b'-' {
                self.i += 1;
            }
            while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
                self.i += 1;
            }
            let n: i64 = std::str::from_utf8(&self.b[start..self.i])
                .unwrap()
                .parse()
                .unwrap();
            JsonValue::Int(n)
        }
    }
}
