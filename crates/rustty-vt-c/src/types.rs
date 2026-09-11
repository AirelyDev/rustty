//! Comptime-generated ABI metadata for the public librustty-vt C types.
//!
//! The manifest is embedded in the library and returned by
//! `rustty_type_json`. It is intended for FFI consumers that cannot use the
//! C headers directly, most notably WebAssembly hosts. Its format is defined
//! by `types.schema.json`.
//
// const std = @import("std");
// const builtin = @import("builtin");
// const build_options = @import("terminal_options");
// const lib = @import("../lib.zig");
// const c_abi = @import("../../lib/c_abi.zig");
// ... (module imports; the Rust counterparts are the `use` items below)
//
// PORT NOTE (R3, comptime): the reference generates `json` at COMPTIME by
// reflecting over each registered type (`@typeInfo`, `@offsetOf`, `@sizeOf`,
// `@alignOf`, `@intFromEnum`). Rust has no comptime type reflection, so the
// metadata ROWS are spelled out below, but everything else is the same
// comptime story: the rows are `const` (`core::mem::offset_of!` / `size_of` /
// `align_of` are all const-evaluable over the identical `#[repr(C)]`
// projections, and every enum VALUE is the Rust enum cast `E::Variant as
// i64`, so a renamed/removed variant is a compile error) and the JSON is
// serialized by `const fn`s into a `static` byte array with a NUL sentinel,
// exactly like the reference's `json: [:0]const u8`.
//
// This must NOT be a lazily-built runtime cache: `rustty_type_json` is a getter
// that WASM embedders call while holding `DataView`/`TypedArray` views over
// `wasmMemory.buffer` (see `include/rustty/vt/wasm.h`), and a first-call heap
// allocation grows the linear memory, DETACHING every one of those views. The
// reference cannot do that because its string is static data; neither can this.
//
// The produced JSON is byte-identical to the reference's (same declaration
// order, same field order, same `std.json.Stringify` formatting), modulo the
// R2 renames (`Ghostty*` -> `Rustty*`, `GHOSTTY_*` -> `RUSTTY_*`) and the
// rustty `library_version`/`commit` identity. Verified against the oracle's
// own `ghostty_type_json` output (libghostty-vt.dylib built at the pin).
//
// ---------------------------------------------------------------------------
// DEFERRED ROWS: NONE. Bundle T landed the registry machinery and every row
// whose C type existed then; Bundles R/M landed their rows in P4-W5 T2/T3;
// Bundle S (snapshot/io) landed the last five (GhosttyReader, GhosttyWriter,
// GhosttySnapshotDecoderData, GhosttySnapshotDecoderOption,
// GhosttySnapshotDecoder opaque) plus the Result IO_ERROR=-5 /
// LIMIT_EXCEEDED=-6 values and the TerminalOption 31 / TerminalData 36+38
// rows. The registry now matches the oracle's `type_decls` EXACTLY (the
// check-vt-struct-layouts.mjs parity arm binds it with an empty residual
// list).
// ---------------------------------------------------------------------------
use std::ffi::c_char;
use std::os::raw::c_int;

use core::mem::{align_of, offset_of, size_of};

// PORT NOTE: `terminal.DeviceAttributes` is `Effects.CDeviceAttributes` with the
// nested `Primary`/`Secondary`/`Tertiary`. The rustty C-ABI projections live in
// `terminal.rs`.
use crate::terminal::{CDeviceAttributes, CDeviceAttributesPrimary};
use crate::terminal::{CDeviceAttributesSecondary, CDeviceAttributesTertiary};
// const search = @import("search.zig");
use crate::search::{SearchData, SearchOption, SearchScroll, SearchStatus};
use crate::selection::CSelectionBuffer;

use rustty_lib::packed::{Encoding, PackedLayout, PackedTaggedUnionLayout};

/// C: RusttySurfacePosition (R2)
// pub const SurfacePosition = extern struct { x: f64, y: f64 };
// PORT NOTE: the reference declares `SurfacePosition`/`Codepoints` in this
// module; the existing rustty port placed the identical `#[repr(C)]` layouts in
// `selection_gesture.rs`. Reuse them here under the reference names to avoid a
// duplicate definition (same byte layout).
pub use crate::selection_gesture::Codepoints;
pub use crate::selection_gesture::SurfacePosition;

// const TypeDecl = struct { name, T, kind, prefix, sentinel_suffix,
//     alias_type, tagged_union, union_field_names_T, ... };
//
// PORT NOTE: the reference `TypeDecl` couples each row to a Zig `type` and
// reflects everything else. The Rust model splits per kind (the reflection
// outputs become explicit row data); `initStruct`/`initTaggedStruct`/
// `initUnion`/`initEnum`/`initEnumSentinel`/`initPacked`/`initAlias`/
// `initOpaque` map onto the `TypeDecl::*` constructors below.

/// How a field's type is described (`Json.writeFieldType`).
///
/// PORT NOTE: the reference derives this from `@typeInfo(T)` + the
/// `publicTypeName` / `fieldTypeOverride` / `writePointerTypeNamed` maps; the
/// Rust rows spell the RESOLVED label, with the overridden sites commented at
/// their rows.
#[derive(Clone, Copy)]
enum Ty {
    /// `"type": "<name>"` — a builtin label or a registered type name.
    Named(&'static str),
    /// `.array => "type":"array","elem":E,"count":N`.
    Array(&'static str, usize),
    /// `.pointer/.optional => "type":"pointer","elem":E,"const":B[,"nullable":true]`.
    Pointer(&'static str, bool, bool),
}

/// One reflected struct/union field row (`Json.writeField`).
struct Field {
    name: &'static str,
    offset: usize,
    size: usize,
    ty: Ty,
    /// `decl.tagged_union` metadata, present ONLY on the tagged value field:
    /// `(tag field name, tag enum public name, cFieldRename arms)`.
    tagged: Option<TaggedMeta>,
}

struct TaggedMeta {
    tag_field: &'static str,
    /// `publicTypeName(Tag)` — arm keys render through
    /// `writeEnumObjectField(tag_enum, ...)` exactly like enum values.
    tag_enum: &'static str,
    /// `(zig tag name, cFieldRename result)` per tag, in declaration order.
    arms: &'static [(&'static str, Option<&'static str>)],
}

/// Enum value rows (`inline for (@typeInfo(decl.T).@"enum".fields)`).
enum EnumValues {
    /// `(zig field name, @intFromEnum value)` rows; the value side is the Rust
    /// enum cast so the manifest can never drift from the enum itself.
    Rows(&'static [(&'static str, i64)]),
    /// PORT NOTE: `input_key.Key` has 176 fields; its reflected name table
    /// already exists as `rustty_vt::input::key::TAG_NAMES` (the `@tagName`
    /// model), so the writer iterates that single source of truth directly.
    KeyTagNames,
}

enum TypeDecl {
    Struct {
        name: &'static str,
        size: usize,
        align: usize,
        fields: &'static [Field],
    },
    Union {
        name: &'static str,
        size: usize,
        align: usize,
        fields: &'static [Field],
    },
    Enum {
        name: &'static str,
        prefix: &'static str,
        sentinel_suffix: &'static str,
        values: EnumValues,
    },
    Packed {
        name: &'static str,
        layout: &'static PackedLayout,
    },
    Alias {
        name: &'static str,
        size: usize,
        align: usize,
        alias_type: &'static str,
    },
    Opaque {
        name: &'static str,
    },
}

impl TypeDecl {
    const fn name(&self) -> &'static str {
        match self {
            TypeDecl::Struct { name, .. }
            | TypeDecl::Union { name, .. }
            | TypeDecl::Enum { name, .. }
            | TypeDecl::Packed { name, .. }
            | TypeDecl::Alias { name, .. }
            | TypeDecl::Opaque { name } => name,
        }
    }
}

// PORT NOTE: `publicTypeName`/`intName` over `usize`/`isize`: the reference
// derives `"u64"`/`"u32"` from the target's actual `@bitSizeOf(usize)`, so the
// labels stay correct on wasm32.
const USIZE_TY: &str = if size_of::<usize>() == 8 {
    "u64"
} else {
    "u32"
};
const ISIZE_TY: &str = if size_of::<isize>() == 8 {
    "i64"
} else {
    "i32"
};

/// Shorthand row constructors (see `Ty`).
const fn n(name: &'static str) -> Ty {
    Ty::Named(name)
}
const fn usize_ty() -> Ty {
    Ty::Named(USIZE_TY)
}

macro_rules! fields {
    ($T:ty, [ $( ($name:literal, $ft:ty, $field:tt, $ty:expr) ),* $(,)? ]) => {
        &[
            $(
                Field {
                    name: $name,
                    offset: offset_of!($T, $field),
                    size: size_of::<$ft>(),
                    ty: $ty,
                    tagged: None,
                },
            )*
        ]
    };
}

macro_rules! struct_decl {
    ($cname:literal, $T:ty, $fields:expr) => {
        TypeDecl::Struct {
            name: $cname,
            size: size_of::<$T>(),
            align: align_of::<$T>(),
            fields: $fields,
        }
    };
}

macro_rules! enum_values {
    ($E:ty, [ $( ($zig:literal, $v:ident) ),* $(,)? ]) => {
        EnumValues::Rows(&[ $( ($zig, <$E>::$v as i64) ),* ])
    };
}

// PORT NOTE: for rustty's OPEN `enum(c_int)` models (`pub struct X(pub i32)`
// with declared consts — the Zig `enum(c_int) { ..., _ }` shape) the declared
// fields are the consts; `@intFromEnum` is the `.0` projection.
macro_rules! enum_const_values {
    ($E:ty, [ $( ($zig:literal, $v:ident) ),* $(,)? ]) => {
        EnumValues::Rows(&[ $( ($zig, <$E>::$v.0 as i64) ),* ])
    };
}

macro_rules! enum_decl {
    ($cname:literal, $prefix:literal, $values:expr) => {
        TypeDecl::Enum {
            name: $cname,
            prefix: $prefix,
            sentinel_suffix: "MAX_VALUE",
            values: $values,
        }
    };
    ($cname:literal, $prefix:literal, $sentinel:literal, $values:expr) => {
        TypeDecl::Enum {
            name: $cname,
            prefix: $prefix,
            sentinel_suffix: $sentinel,
            values: $values,
        }
    };
}

/// `taggedArm` for `GhosttyStyleColor` (`arm_source == .fields`): an arm is
/// the tag name itself iff `CValue` has a same-named field of non-zero size
/// (`none` has none; `palette`/`rgb` do).
const STYLE_COLOR_ARMS: &[(&str, Option<&str>)] = &[
    ("none", None),
    ("palette", Some("palette")),
    ("rgb", Some("rgb")),
];

/// Public C types. Names and enum prefixes are intentionally explicit because
/// Zig identifiers are not the C API contract.
// const type_decls = [_]TypeDecl{ ... };
static DECLS: &[TypeDecl] = {
    use crate::color::RgbC;
    use crate::formatter::{ScreenOptionsExtra, TerminalOptions, TerminalOptionsExtra};
    use crate::grid_ref::CGridRef;
    use crate::kitty_graphics::PlacementRenderInfo;
    use crate::mouse_encode::Size as MouseEncoderSize;
    use crate::mouse_event::Position as MousePosition;
    use crate::point::{PointC, PointCValue};
    use crate::render::Colors;
    use crate::selection::{CSelection, FormatOptions, SelectLineOptions};
    use crate::selection::{SelectWordBetweenOptions, SelectWordOptions};
    use crate::selection_gesture::{Behaviors, Geometry};
    use crate::sgr::{AttributeC, CValue as SgrCValue, UnknownC};
    use crate::size_report::Size as SizeReportSize;
    use crate::style::{Color as StyleColor, ColorValue as StyleColorValue, Style};
    use crate::sys::Image as SysImage;
    use crate::terminal::{DesktopNotification, ProgressReport, TerminalScrollbar};
    use crate::terminal::{ScrollViewport, ScrollViewportCValue};
    use rustty_lib::allocator::{Allocator, VTable};
    use rustty_lib::types::{Buffer, String as LibString};
    use rustty_vt::point::Coordinate as PointCoordinate;

    &[
        // .initStruct("GhosttyAllocator", lib.alloc.Allocator),
        struct_decl!(
            "RusttyAllocator",
            Allocator,
            fields!(Allocator, [
                // ctx: *anyopaque -> non-null pointer to opaque
                ("ctx", *mut std::ffi::c_void, ctx, Ty::Pointer("opaque", false, false)),
                ("vtable", *const VTable, vtable, Ty::Pointer("RusttyAllocatorVtable", true, false)),
            ])
        ),
        // .initStruct("GhosttyAllocatorVtable", lib.alloc.VTable),
        struct_decl!(
            "RusttyAllocatorVtable",
            VTable,
            fields!(
                VTable,
                [
                    // *const fn (...) -> pointer to function
                    ("alloc", usize, alloc, Ty::Pointer("function", true, false)),
                    (
                        "resize",
                        usize,
                        resize,
                        Ty::Pointer("function", true, false)
                    ),
                    ("remap", usize, remap, Ty::Pointer("function", true, false)),
                    ("free", usize, free, Ty::Pointer("function", true, false)),
                ]
            )
        ),
        // .initStruct("GhosttyBuffer", lib.Buffer),
        struct_decl!(
            "RusttyBuffer",
            Buffer,
            fields!(Buffer, [
                // ptr: ?[*]u8 -> optional pointer -> nullable
                ("ptr", *mut u8, ptr, Ty::Pointer("u8", false, true)),
                ("cap", usize, cap, usize_ty()),
                ("len", usize, len, usize_ty()),
            ])
        ),
        // .initStruct("GhosttyCellsView", cell.CellsView),
        struct_decl!(
            "RusttyCellsView",
            crate::cell::CellsView,
            fields!(crate::cell::CellsView, [
                // writePointerTypeNamed(T, true, "GhosttyCell"): the borrowed
                // cell values are `RusttyCell` (`u64`) — named, const, nullable.
                ("ptr", *const u64, ptr, Ty::Pointer("RusttyCell", true, true)),
                ("len", usize, len, usize_ty()),
            ])
        ),
        // .initStruct("GhosttyClipboardContent", terminal.ClipboardContent),
        struct_decl!(
            "RusttyClipboardContent",
            crate::terminal::ClipboardContent,
            fields!(
                crate::terminal::ClipboardContent,
                [
                    ("mime", LibString, mime, n("RusttyString")),
                    ("data", LibString, data, n("RusttyString")),
                ]
            )
        ),
        // .initStruct("GhosttyClipboardRead", terminal.ClipboardRead),
        struct_decl!(
            "RusttyClipboardRead",
            crate::terminal::ClipboardRead,
            fields!(crate::terminal::ClipboardRead, [
                ("size", usize, size, usize_ty()),
                ("location", rustty_vt::clipboard::Location, location, n("RusttyClipboardLocation")),
                ("mimes", *const LibString, mimes, Ty::Pointer("RusttyString", true, true)),
                ("mimes_len", usize, mimes_len, usize_ty()),
                ("list", bool, list, n("bool")),
                ("name", LibString, name, n("RusttyString")),
                ("granted", bool, granted, n("bool")),
                ("can_remember", bool, can_remember, n("bool")),
                ("ctx", *const std::ffi::c_void, ctx, Ty::Pointer("opaque", true, false)),
                ("reply", crate::terminal::ClipboardReadReplyFn, reply, Ty::Pointer("function", true, false)),
            ])
        ),
        // .initStruct("GhosttyClipboardReadReply", terminal.ClipboardReadReply),
        struct_decl!(
            "RusttyClipboardReadReply",
            crate::terminal::ClipboardReadReply,
            fields!(crate::terminal::ClipboardReadReply, [
                ("size", usize, size, usize_ty()),
                ("result", rustty_vt::clipboard::ReadStatus, result, n("RusttyClipboardReadResult")),
                ("contents", *const crate::terminal::ClipboardContent, contents, Ty::Pointer("RusttyClipboardContent", true, true)),
                ("contents_len", usize, contents_len, usize_ty()),
                ("available", *const LibString, available, Ty::Pointer("RusttyString", true, true)),
                ("available_len", usize, available_len, usize_ty()),
                ("remember", bool, remember, n("bool")),
            ])
        ),
        // .initStruct("GhosttyClipboardWrite", terminal.ClipboardWrite),
        struct_decl!(
            "RusttyClipboardWrite",
            crate::terminal::ClipboardWrite,
            fields!(crate::terminal::ClipboardWrite, [
                ("size", usize, size, usize_ty()),
                ("location", rustty_vt::clipboard::Location, location, n("RusttyClipboardLocation")),
                ("contents", *const crate::terminal::ClipboardContent, contents, Ty::Pointer("RusttyClipboardContent", true, true)),
                ("contents_len", usize, contents_len, usize_ty()),
                ("name", LibString, name, n("RusttyString")),
                ("granted", bool, granted, n("bool")),
                ("can_remember", bool, can_remember, n("bool")),
                ("ctx", *const std::ffi::c_void, ctx, Ty::Pointer("opaque", true, false)),
                ("reply", crate::terminal::ClipboardWriteReplyFn, reply, Ty::Pointer("function", true, false)),
            ])
        ),
        // .initStruct("GhosttyClipboardWriteReply", terminal.ClipboardWriteReply),
        struct_decl!(
            "RusttyClipboardWriteReply",
            crate::terminal::ClipboardWriteReply,
            fields!(
                crate::terminal::ClipboardWriteReply,
                [
                    ("size", usize, size, usize_ty()),
                    (
                        "result",
                        rustty_vt::clipboard::WriteStatus,
                        result,
                        n("RusttyClipboardWriteResult")
                    ),
                    ("remember", bool, remember, n("bool")),
                ]
            )
        ),
        // .initStruct("GhosttyCodepoints", Codepoints),
        struct_decl!(
            "RusttyCodepoints",
            Codepoints,
            fields!(Codepoints, [
                ("ptr", *const u32, ptr, Ty::Pointer("u32", true, true)),
                ("len", usize, len, usize_ty()),
            ])
        ),
        // .initStruct("GhosttyColorPaletteMask", color_c.PaletteMask),
        struct_decl!(
            "RusttyColorPaletteMask",
            crate::color::PaletteMask,
            fields!(
                crate::color::PaletteMask,
                [("bits", [u64; 4], bits, Ty::Array("u64", 4)),]
            )
        ),
        // .initStruct("GhosttyColorRgb", color.RGB.C),
        struct_decl!(
            "RusttyColorRgb",
            RgbC,
            fields!(
                RgbC,
                [
                    ("r", u8, r, n("u8")),
                    ("g", u8, g, n("u8")),
                    ("b", u8, b, n("u8")),
                ]
            )
        ),
        // .initStruct("GhosttyColorX11Entry", color_c.X11Entry),
        struct_decl!(
            "RusttyColorX11Entry",
            crate::color::X11Entry,
            fields!(crate::color::X11Entry, [
                ("name", *const c_char, name, Ty::Pointer("u8", true, true)),
                ("color", RgbC, color, n("RusttyColorRgb")),
            ])
        ),
        // .initStruct("GhosttyDeviceAttributes", terminal.DeviceAttributes),
        struct_decl!(
            "RusttyDeviceAttributes",
            CDeviceAttributes,
            fields!(
                CDeviceAttributes,
                [
                    (
                        "primary",
                        CDeviceAttributesPrimary,
                        primary,
                        n("RusttyDeviceAttributesPrimary")
                    ),
                    (
                        "secondary",
                        CDeviceAttributesSecondary,
                        secondary,
                        n("RusttyDeviceAttributesSecondary")
                    ),
                    (
                        "tertiary",
                        CDeviceAttributesTertiary,
                        tertiary,
                        n("RusttyDeviceAttributesTertiary")
                    ),
                ]
            )
        ),
        // .initStruct("GhosttyDeviceAttributesPrimary", ...Primary),
        struct_decl!(
            "RusttyDeviceAttributesPrimary",
            CDeviceAttributesPrimary,
            fields!(
                CDeviceAttributesPrimary,
                [
                    ("conformance_level", u16, conformance_level, n("u16")),
                    ("features", [u16; 64], features, Ty::Array("u16", 64)),
                    ("num_features", usize, num_features, usize_ty()),
                ]
            )
        ),
        // .initStruct("GhosttyDeviceAttributesSecondary", ...Secondary),
        struct_decl!(
            "RusttyDeviceAttributesSecondary",
            CDeviceAttributesSecondary,
            fields!(
                CDeviceAttributesSecondary,
                [
                    ("device_type", u16, device_type, n("u16")),
                    ("firmware_version", u16, firmware_version, n("u16")),
                    ("rom_cartridge", u16, rom_cartridge, n("u16")),
                ]
            )
        ),
        // .initStruct("GhosttyDeviceAttributesTertiary", ...Tertiary),
        struct_decl!(
            "RusttyDeviceAttributesTertiary",
            CDeviceAttributesTertiary,
            fields!(
                CDeviceAttributesTertiary,
                [("unit_id", u32, unit_id, n("u32")),]
            )
        ),
        // .initStruct("GhosttyFormatterScreenExtra", formatter.ScreenOptions.Extra),
        struct_decl!(
            "RusttyFormatterScreenExtra",
            ScreenOptionsExtra,
            fields!(
                ScreenOptionsExtra,
                [
                    ("size", usize, size, usize_ty()),
                    ("cursor", bool, cursor, n("bool")),
                    ("style", bool, style, n("bool")),
                    ("hyperlink", bool, hyperlink, n("bool")),
                    ("protection", bool, protection, n("bool")),
                    ("kitty_keyboard", bool, kitty_keyboard, n("bool")),
                    ("charsets", bool, charsets, n("bool")),
                ]
            )
        ),
        // .initStruct("GhosttyFormatterTerminalExtra", formatter.TerminalOptions.Extra),
        struct_decl!(
            "RusttyFormatterTerminalExtra",
            TerminalOptionsExtra,
            fields!(
                TerminalOptionsExtra,
                [
                    ("size", usize, size, usize_ty()),
                    ("palette", bool, palette, n("bool")),
                    ("modes", bool, modes, n("bool")),
                    ("scrolling_region", bool, scrolling_region, n("bool")),
                    ("tabstops", bool, tabstops, n("bool")),
                    ("pwd", bool, pwd, n("bool")),
                    ("keyboard", bool, keyboard, n("bool")),
                    (
                        "screen",
                        ScreenOptionsExtra,
                        screen,
                        n("RusttyFormatterScreenExtra")
                    ),
                ]
            )
        ),
        // .initStruct("GhosttyFormatterTerminalOptions", formatter.TerminalOptions),
        struct_decl!(
            "RusttyFormatterTerminalOptions",
            TerminalOptions,
            fields!(TerminalOptions, [
                ("size", usize, size, usize_ty()),
                ("emit", crate::selection::Format, emit, n("RusttyFormatterFormat")),
                ("unwrap", bool, unwrap, n("bool")),
                ("trim", bool, trim, n("bool")),
                ("extra", TerminalOptionsExtra, extra, n("RusttyFormatterTerminalExtra")),
                ("selection", *const CSelection, selection, Ty::Pointer("RusttySelection", true, true)),
            ])
        ),
        // .initStruct("GhosttyGridRef", grid_ref.CGridRef),
        struct_decl!(
            "RusttyGridRef",
            CGridRef,
            fields!(CGridRef, [
                ("size", usize, size, usize_ty()),
                // writePointerTypeNamed(T, true, "opaque"): the node pointer's
                // pointee (PageList.List.Node) is not a public type.
                ("node", *mut rustty_vt::pagelist::Node, node, Ty::Pointer("opaque", false, true)),
                ("x", u16, x, n("u16")),
                ("y", u16, y, n("u16")),
            ])
        ),
        // .initStruct("GhosttyKittyGraphicsPlacementRenderInfo", kitty_graphics.PlacementRenderInfo),
        struct_decl!(
            "RusttyKittyGraphicsPlacementRenderInfo",
            PlacementRenderInfo,
            fields!(
                PlacementRenderInfo,
                [
                    ("size", usize, size, usize_ty()),
                    ("pixel_width", u32, pixel_width, n("u32")),
                    ("pixel_height", u32, pixel_height, n("u32")),
                    ("grid_cols", u32, grid_cols, n("u32")),
                    ("grid_rows", u32, grid_rows, n("u32")),
                    ("viewport_col", i32, viewport_col, n("i32")),
                    ("viewport_row", i32, viewport_row, n("i32")),
                    ("viewport_visible", bool, viewport_visible, n("bool")),
                    ("source_x", u32, source_x, n("u32")),
                    ("source_y", u32, source_y, n("u32")),
                    ("source_width", u32, source_width, n("u32")),
                    ("source_height", u32, source_height, n("u32")),
                ]
            )
        ),
        // .initStruct("GhosttyMimeReader", io.MimeReader),
        struct_decl!(
            "RusttyMimeReader",
            crate::io::MimeReader,
            fields!(crate::io::MimeReader, [
                // read: ?MimeReaderFn -> optional fn pointer -> nullable
                ("read", crate::io::MimeReaderFn, read, Ty::Pointer("function", true, true)),
                // userdata: ?*anyopaque -> optional pointer -> nullable
                ("userdata", *mut std::ffi::c_void, userdata, Ty::Pointer("opaque", false, true)),
            ])
        ),
        // .initStruct("GhosttyMouseEncoderSize", mouse_encode.Size),
        struct_decl!(
            "RusttyMouseEncoderSize",
            MouseEncoderSize,
            fields!(
                MouseEncoderSize,
                [
                    ("size", usize, size, usize_ty()),
                    ("screen_width", u32, screen_width, n("u32")),
                    ("screen_height", u32, screen_height, n("u32")),
                    ("cell_width", u32, cell_width, n("u32")),
                    ("cell_height", u32, cell_height, n("u32")),
                    ("padding_top", u32, padding_top, n("u32")),
                    ("padding_bottom", u32, padding_bottom, n("u32")),
                    ("padding_right", u32, padding_right, n("u32")),
                    ("padding_left", u32, padding_left, n("u32")),
                ]
            )
        ),
        // .initStruct("GhosttyMousePosition", mouse_event.Position),
        struct_decl!(
            "RusttyMousePosition",
            MousePosition,
            fields!(
                MousePosition,
                [("x", f32, x, n("f32")), ("y", f32, y, n("f32")),]
            )
        ),
        // .initStruct("GhosttyPaste", paste.Request),
        struct_decl!(
            "RusttyPaste",
            crate::paste::Request,
            fields!(crate::paste::Request, [
                ("size", usize, size, usize_ty()),
                // D11: `location`/`source` are held as raw `c_int`s in the Rust
                // struct (inbound C memory); the manifest reports the C-facing
                // enum names, exactly like the reference field types.
                ("location", rustty_vt::clipboard::Location, location, n("RusttyClipboardLocation")),
                ("source", crate::paste::Source, source, n("RusttyPasteSource")),
                ("mimes", *const LibString, mimes, Ty::Pointer("RusttyString", true, true)),
                ("mimes_len", usize, mimes_len, usize_ty()),
                ("reader", crate::io::MimeReader, reader, n("RusttyMimeReader")),
                ("allow_unsafe", bool, allow_unsafe, n("bool")),
            ])
        ),
        // .initTaggedStruct("GhosttyPoint", point.Point.C, "tag", "value", .generated),
        TypeDecl::Struct {
            name: "RusttyPoint",
            size: size_of::<PointC>(),
            align: align_of::<PointC>(),
            fields: &[
                Field {
                    name: "tag",
                    offset: offset_of!(PointC, tag),
                    size: size_of::<c_int>(),
                    ty: Ty::Named("RusttyPointTag"),
                    tagged: None,
                },
                Field {
                    name: "value",
                    offset: offset_of!(PointC, value),
                    size: size_of::<PointCValue>(),
                    // fieldTypeOverride("GhosttyPoint", "value")
                    ty: Ty::Named("RusttyPointValue"),
                    tagged: Some(TaggedMeta {
                        tag_field: "tag",
                        tag_enum: "RusttyPointTag",
                        arms: crate::point::C_FIELD_ARMS,
                    }),
                },
            ],
        },
        // .initStruct("GhosttyPointCoordinate", point.Coordinate),
        struct_decl!(
            "RusttyPointCoordinate",
            PointCoordinate,
            fields!(
                PointCoordinate,
                [("x", u16, x, n("u16")), ("y", u32, y, n("u32")),]
            )
        ),
        // .initUnion("GhosttyPointValue", point.Point.CValue, point.Point.C),
        //
        // PORT NOTE: `writeMappedUnionFields` — tag fields mapped through
        // `cFieldRename` and DEDUPED (all four tags rename to `coordinate`),
        // plus `_padding`; union field offsets are all 0.
        TypeDecl::Union {
            name: "RusttyPointValue",
            size: size_of::<PointCValue>(),
            align: align_of::<PointCValue>(),
            fields: &[
                Field {
                    name: "coordinate",
                    offset: 0,
                    size: size_of::<PointCoordinate>(),
                    ty: Ty::Named("RusttyPointCoordinate"),
                    tagged: None,
                },
                Field {
                    name: "_padding",
                    offset: 0,
                    size: size_of::<[u64; 2]>(),
                    ty: Ty::Array("u64", 2),
                    tagged: None,
                },
            ],
        },
        // .initStruct("GhosttyReader", io.Reader),
        struct_decl!(
            "RusttyReader",
            crate::io::Reader,
            fields!(
                crate::io::Reader,
                [
                    // *const fn (...) -> pointer to function
                    (
                        "read",
                        Option<crate::io::ReaderFn>,
                        read,
                        Ty::Pointer("function", true, true)
                    ),
                    (
                        "userdata",
                        *mut std::ffi::c_void,
                        userdata,
                        Ty::Pointer("opaque", false, true)
                    ),
                ]
            )
        ),
        // .initStruct("GhosttyRenderStateColors", render.Colors),
        struct_decl!(
            "RusttyRenderStateColors",
            Colors,
            fields!(
                Colors,
                [
                    ("size", usize, size, usize_ty()),
                    ("background", RgbC, background, n("RusttyColorRgb")),
                    ("foreground", RgbC, foreground, n("RusttyColorRgb")),
                    ("cursor", RgbC, cursor, n("RusttyColorRgb")),
                    ("cursor_has_value", bool, cursor_has_value, n("bool")),
                    (
                        "palette",
                        [RgbC; 256],
                        palette,
                        Ty::Array("RusttyColorRgb", 256)
                    ),
                ]
            )
        ),
        // .initStruct("GhosttyRenderStateCursor", render.Cursor),
        struct_decl!(
            "RusttyRenderStateCursor",
            crate::render::Cursor,
            fields!(
                crate::render::Cursor,
                [
                    ("size", usize, size, usize_ty()),
                    ("viewport_has_value", bool, viewport_has_value, n("bool")),
                    ("viewport_x", u16, viewport_x, n("u16")),
                    ("viewport_y", u16, viewport_y, n("u16")),
                    ("wide_tail", bool, wide_tail, n("bool")),
                    ("visible", bool, visible, n("bool")),
                    ("blinking", bool, blinking, n("bool")),
                    ("password_input", bool, password_input, n("bool")),
                    (
                        "visual_style",
                        crate::render::CursorVisualStyle,
                        visual_style,
                        n("RusttyRenderStateCursorVisualStyle")
                    ),
                ]
            )
        ),
        // .initStruct("GhosttyRenderStateRowSelection", render.RowSelection),
        struct_decl!(
            "RusttyRenderStateRowSelection",
            crate::render::RowSelection,
            fields!(
                crate::render::RowSelection,
                [
                    ("size", usize, size, usize_ty()),
                    ("start_x", u16, start_x, n("u16")),
                    ("end_x", u16, end_x, n("u16")),
                ]
            )
        ),
        // .initStruct("GhosttySelection", selection.CSelection),
        struct_decl!(
            "RusttySelection",
            CSelection,
            fields!(
                CSelection,
                [
                    ("size", usize, size, usize_ty()),
                    ("start", CGridRef, start, n("RusttyGridRef")),
                    ("end", CGridRef, end, n("RusttyGridRef")),
                    ("rectangle", bool, rectangle, n("bool")),
                ]
            )
        ),
        // .initStruct("GhosttySelectionBuffer", selection.CSelectionBuffer),
        struct_decl!(
            "RusttySelectionBuffer",
            CSelectionBuffer,
            fields!(
                CSelectionBuffer,
                [
                    // ptr: ?[*]CSelection -> optional pointer -> nullable
                    ("ptr", *mut CSelection, ptr, Ty::Pointer("RusttySelection", false, true)),
                    ("cap", usize, cap, usize_ty()),
                    ("len", usize, len, usize_ty()),
                ]
            )
        ),
        // .initStruct("GhosttySelectionGestureBehaviors", selection_gesture.Behaviors),
        struct_decl!(
            "RusttySelectionGestureBehaviors",
            Behaviors,
            fields!(
                Behaviors,
                [
                    (
                        "single_click",
                        crate::selection_gesture::Behavior,
                        single_click,
                        n("RusttySelectionGestureBehavior")
                    ),
                    (
                        "double_click",
                        crate::selection_gesture::Behavior,
                        double_click,
                        n("RusttySelectionGestureBehavior")
                    ),
                    (
                        "triple_click",
                        crate::selection_gesture::Behavior,
                        triple_click,
                        n("RusttySelectionGestureBehavior")
                    ),
                ]
            )
        ),
        // .initStruct("GhosttySelectionGestureGeometry", selection_gesture.Geometry),
        struct_decl!(
            "RusttySelectionGestureGeometry",
            Geometry,
            fields!(
                Geometry,
                [
                    ("columns", u32, columns, n("u32")),
                    ("cell_width", u32, cell_width, n("u32")),
                    ("padding_left", u32, padding_left, n("u32")),
                    ("screen_height", u32, screen_height, n("u32")),
                ]
            )
        ),
        // .initTaggedStruct("GhosttySgrAttribute", sgr.Attribute.C, "tag", "value", .generated),
        TypeDecl::Struct {
            name: "RusttySgrAttribute",
            size: size_of::<AttributeC>(),
            align: align_of::<AttributeC>(),
            fields: &[
                Field {
                    name: "tag",
                    offset: offset_of!(AttributeC, tag),
                    size: size_of::<c_int>(),
                    ty: Ty::Named("RusttySgrAttributeTag"),
                    tagged: None,
                },
                Field {
                    name: "value",
                    offset: offset_of!(AttributeC, value),
                    size: size_of::<SgrCValue>(),
                    // fieldTypeOverride("GhosttySgrAttribute", "value")
                    ty: Ty::Named("RusttySgrAttributeValue"),
                    tagged: Some(TaggedMeta {
                        tag_field: "tag",
                        tag_enum: "RusttySgrAttributeTag",
                        arms: crate::sgr::C_FIELD_ARMS,
                    }),
                },
            ],
        },
        // .initStruct("GhosttySgrUnknown", sgr.Attribute.Unknown.C),
        struct_decl!(
            "RusttySgrUnknown",
            UnknownC,
            fields!(UnknownC, [
                ("full_ptr", *const u16, full_ptr, Ty::Pointer("u16", true, false)),
                ("full_len", usize, full_len, usize_ty()),
                ("partial_ptr", *const u16, partial_ptr, Ty::Pointer("u16", true, false)),
                ("partial_len", usize, partial_len, usize_ty()),
            ])
        ),
        // .initUnion("GhosttySgrAttributeValue", sgr.Attribute.CValue, sgr.Attribute.C),
        //
        // PORT NOTE: `writeMappedUnionFields` over `Attribute.C.cFieldRename`
        // (see `sgr::C_FIELD_ARMS`): payload-less tags drop out, the 7
        // digit-leading names arrive renamed, plus `_padding`. The
        // `fieldTypeOverride` rows: `underline` -> `RusttySgrUnderline`, the
        // `_8`/`_256`-suffixed fields -> `RusttyColorPaletteIndex`.
        TypeDecl::Union {
            name: "RusttySgrAttributeValue",
            size: size_of::<SgrCValue>(),
            align: align_of::<SgrCValue>(),
            fields: &[
                Field {
                    name: "unknown",
                    offset: 0,
                    size: size_of::<UnknownC>(),
                    ty: Ty::Named("RusttySgrUnknown"),
                    tagged: None,
                },
                Field {
                    name: "underline",
                    offset: 0,
                    size: size_of::<c_int>(),
                    ty: Ty::Named("RusttySgrUnderline"),
                    tagged: None,
                },
                Field {
                    name: "underline_color",
                    offset: 0,
                    size: 3,
                    ty: Ty::Named("RusttyColorRgb"),
                    tagged: None,
                },
                Field {
                    name: "underline_color_256",
                    offset: 0,
                    size: 1,
                    ty: Ty::Named("RusttyColorPaletteIndex"),
                    tagged: None,
                },
                Field {
                    name: "direct_color_fg",
                    offset: 0,
                    size: 3,
                    ty: Ty::Named("RusttyColorRgb"),
                    tagged: None,
                },
                Field {
                    name: "direct_color_bg",
                    offset: 0,
                    size: 3,
                    ty: Ty::Named("RusttyColorRgb"),
                    tagged: None,
                },
                Field {
                    name: "bg_8",
                    offset: 0,
                    size: 1,
                    ty: Ty::Named("RusttyColorPaletteIndex"),
                    tagged: None,
                },
                Field {
                    name: "fg_8",
                    offset: 0,
                    size: 1,
                    ty: Ty::Named("RusttyColorPaletteIndex"),
                    tagged: None,
                },
                Field {
                    name: "bright_bg_8",
                    offset: 0,
                    size: 1,
                    ty: Ty::Named("RusttyColorPaletteIndex"),
                    tagged: None,
                },
                Field {
                    name: "bright_fg_8",
                    offset: 0,
                    size: 1,
                    ty: Ty::Named("RusttyColorPaletteIndex"),
                    tagged: None,
                },
                Field {
                    name: "bg_256",
                    offset: 0,
                    size: 1,
                    ty: Ty::Named("RusttyColorPaletteIndex"),
                    tagged: None,
                },
                Field {
                    name: "fg_256",
                    offset: 0,
                    size: 1,
                    ty: Ty::Named("RusttyColorPaletteIndex"),
                    tagged: None,
                },
                Field {
                    name: "_padding",
                    offset: 0,
                    size: size_of::<[u64; 8]>(),
                    ty: Ty::Array("u64", 8),
                    tagged: None,
                },
            ],
        },
        // .initStruct("GhosttySizeReportSize", size_report.Size),
        struct_decl!(
            "RusttySizeReportSize",
            SizeReportSize,
            fields!(
                SizeReportSize,
                [
                    ("rows", u16, rows, n("u16")),
                    ("columns", u16, columns, n("u16")),
                    ("cell_width", u32, cell_width, n("u32")),
                    ("cell_height", u32, cell_height, n("u32")),
                ]
            )
        ),
        // .initStruct("GhosttyString", lib.String),
        struct_decl!(
            "RusttyString",
            LibString,
            fields!(LibString, [
                // ptr: [*]const u8 (non-optional) -> no "nullable"
                ("ptr", *const u8, ptr, Ty::Pointer("u8", true, false)),
                ("len", usize, len, usize_ty()),
            ])
        ),
        // .initStruct("GhosttySurfacePosition", SurfacePosition),
        struct_decl!(
            "RusttySurfacePosition",
            SurfacePosition,
            fields!(
                SurfacePosition,
                [("x", f64, x, n("f64")), ("y", f64, y, n("f64")),]
            )
        ),
        // .initStruct("GhosttyStyle", style.Style),
        struct_decl!(
            "RusttyStyle",
            Style,
            fields!(
                Style,
                [
                    ("size", usize, size, usize_ty()),
                    ("fg_color", StyleColor, fg_color, n("RusttyStyleColor")),
                    ("bg_color", StyleColor, bg_color, n("RusttyStyleColor")),
                    (
                        "underline_color",
                        StyleColor,
                        underline_color,
                        n("RusttyStyleColor")
                    ),
                    ("bold", bool, bold, n("bool")),
                    ("italic", bool, italic, n("bool")),
                    ("faint", bool, faint, n("bool")),
                    ("blink", bool, blink, n("bool")),
                    ("inverse", bool, inverse, n("bool")),
                    ("invisible", bool, invisible, n("bool")),
                    ("strikethrough", bool, strikethrough, n("bool")),
                    ("overline", bool, overline, n("bool")),
                    // underline: c_int -> "i32"
                    ("underline", c_int, underline, n("i32")),
                ]
            )
        ),
        // .initTaggedStruct("GhosttyStyleColor", style.Color, "tag", "value", .fields),
        TypeDecl::Struct {
            name: "RusttyStyleColor",
            size: size_of::<StyleColor>(),
            align: align_of::<StyleColor>(),
            fields: &[
                Field {
                    name: "tag",
                    offset: offset_of!(StyleColor, tag),
                    size: size_of::<crate::style::ColorTag>(),
                    ty: Ty::Named("RusttyStyleColorTag"),
                    tagged: None,
                },
                Field {
                    name: "value",
                    offset: offset_of!(StyleColor, value),
                    size: size_of::<StyleColorValue>(),
                    // fieldTypeOverride("GhosttyStyleColor", "value")
                    ty: Ty::Named("RusttyStyleColorValue"),
                    tagged: Some(TaggedMeta {
                        tag_field: "tag",
                        tag_enum: "RusttyStyleColorTag",
                        arms: STYLE_COLOR_ARMS,
                    }),
                },
            ],
        },
        // .initUnion("GhosttyStyleColorValue", style.ColorValue, null),
        //
        // PORT NOTE: `union_field_names_T == null` -> the union's own fields
        // (`palette`, `rgb`, `_padding`), offsets all 0. `fieldTypeOverride`:
        // `palette` -> `RusttyColorPaletteIndex`.
        TypeDecl::Union {
            name: "RusttyStyleColorValue",
            size: size_of::<StyleColorValue>(),
            align: align_of::<StyleColorValue>(),
            fields: &[
                Field {
                    name: "palette",
                    offset: 0,
                    size: 1,
                    ty: Ty::Named("RusttyColorPaletteIndex"),
                    tagged: None,
                },
                Field {
                    name: "rgb",
                    offset: 0,
                    size: 3,
                    ty: Ty::Named("RusttyColorRgb"),
                    tagged: None,
                },
                Field {
                    name: "_padding",
                    offset: 0,
                    size: size_of::<u64>(),
                    ty: Ty::Named("u64"),
                    tagged: None,
                },
            ],
        },
        // .initStruct("GhosttySysImage", sys.Image),
        struct_decl!(
            "RusttySysImage",
            SysImage,
            fields!(SysImage, [
                ("width", u32, width, n("u32")),
                ("height", u32, height, n("u32")),
                ("data", *mut u8, data, Ty::Pointer("u8", false, true)),
                ("data_len", usize, data_len, usize_ty()),
            ])
        ),
        // .initStruct("GhosttyTerminalDesktopNotification", terminal.DesktopNotification),
        struct_decl!(
            "RusttyTerminalDesktopNotification",
            DesktopNotification,
            fields!(
                DesktopNotification,
                [
                    ("size", usize, size, usize_ty()),
                    ("title", LibString, title, n("RusttyString")),
                    ("body", LibString, body, n("RusttyString")),
                ]
            )
        ),
        // .initStruct("GhosttyTerminalModeConfig", terminal.ModeConfig),
        struct_decl!(
            "RusttyTerminalModeConfig",
            crate::terminal::ModeConfig,
            fields!(
                crate::terminal::ModeConfig,
                [
                    // fieldTypeOverride("GhosttyTerminalModeConfig", "mode") -> "GhosttyMode"
                    (
                        "mode",
                        rustty_vt::modes::ModeTagBacking,
                        mode,
                        n("RusttyMode")
                    ),
                    ("value", bool, value, n("bool")),
                ]
            )
        ),
        // .initStruct("GhosttyTerminalProgressReport", terminal.ProgressReport),
        struct_decl!(
            "RusttyTerminalProgressReport",
            ProgressReport,
            fields!(
                ProgressReport,
                [
                    ("size", usize, size, usize_ty()),
                    (
                        "state",
                        crate::terminal::ProgressState,
                        state,
                        n("RusttyTerminalProgressState")
                    ),
                    ("progress", i8, progress, n("i8")),
                ]
            )
        ),
        // .initStruct("GhosttyTerminalScrollbar", terminal.TerminalScrollbar),
        struct_decl!(
            "RusttyTerminalScrollbar",
            TerminalScrollbar,
            fields!(
                TerminalScrollbar,
                [
                    ("total", u64, total, n("u64")),
                    ("offset", u64, offset, n("u64")),
                    ("len", u64, len, n("u64")),
                ]
            )
        ),
        // .initTaggedStruct("GhosttyTerminalScrollViewport", terminal.ScrollViewport, "tag", "value", .generated),
        TypeDecl::Struct {
            name: "RusttyTerminalScrollViewport",
            size: size_of::<ScrollViewport>(),
            align: align_of::<ScrollViewport>(),
            fields: &[
                Field {
                    name: "tag",
                    offset: offset_of!(ScrollViewport, tag),
                    size: size_of::<c_int>(),
                    ty: Ty::Named("RusttyTerminalScrollViewportTag"),
                    tagged: None,
                },
                Field {
                    name: "value",
                    offset: offset_of!(ScrollViewport, value),
                    size: size_of::<ScrollViewportCValue>(),
                    // fieldTypeOverride("GhosttyTerminalScrollViewport", "value")
                    ty: Ty::Named("RusttyTerminalScrollViewportValue"),
                    tagged: Some(TaggedMeta {
                        tag_field: "tag",
                        tag_enum: "RusttyTerminalScrollViewportTag",
                        arms: crate::terminal::SCROLL_VIEWPORT_C_FIELD_ARMS,
                    }),
                },
            ],
        },
        // .initUnion("GhosttyTerminalScrollViewportValue",
        //     terminal.ZigTerminal.ScrollViewport.CValue, ...C),
        TypeDecl::Union {
            name: "RusttyTerminalScrollViewportValue",
            size: size_of::<ScrollViewportCValue>(),
            align: align_of::<ScrollViewportCValue>(),
            fields: &[
                Field {
                    name: "delta",
                    offset: 0,
                    size: size_of::<isize>(),
                    ty: Ty::Named(ISIZE_TY),
                    tagged: None,
                },
                Field {
                    name: "row",
                    offset: 0,
                    size: size_of::<usize>(),
                    ty: Ty::Named(USIZE_TY),
                    tagged: None,
                },
                Field {
                    name: "_padding",
                    offset: 0,
                    size: size_of::<[u64; 2]>(),
                    ty: Ty::Array("u64", 2),
                    tagged: None,
                },
            ],
        },
        // .initStruct("GhosttyTerminalSelectLineOptions", selection.SelectLineOptions),
        struct_decl!(
            "RusttyTerminalSelectLineOptions",
            SelectLineOptions,
            fields!(SelectLineOptions, [
                ("size", usize, size, usize_ty()),
                ("ref", CGridRef, ref_, n("RusttyGridRef")),
                ("whitespace", *const u32, whitespace, Ty::Pointer("u32", true, true)),
                ("whitespace_len", usize, whitespace_len, usize_ty()),
                ("semantic_prompt_boundary", bool, semantic_prompt_boundary, n("bool")),
            ])
        ),
        // .initStruct("GhosttyTerminalSelectWordBetweenOptions", selection.SelectWordBetweenOptions),
        struct_decl!(
            "RusttyTerminalSelectWordBetweenOptions",
            SelectWordBetweenOptions,
            fields!(SelectWordBetweenOptions, [
                ("size", usize, size, usize_ty()),
                ("start", CGridRef, start, n("RusttyGridRef")),
                ("end", CGridRef, end, n("RusttyGridRef")),
                ("boundary_codepoints", *const u32, boundary_codepoints, Ty::Pointer("u32", true, true)),
                ("boundary_codepoints_len", usize, boundary_codepoints_len, usize_ty()),
            ])
        ),
        // .initStruct("GhosttyTerminalSelectWordOptions", selection.SelectWordOptions),
        struct_decl!(
            "RusttyTerminalSelectWordOptions",
            SelectWordOptions,
            fields!(SelectWordOptions, [
                ("size", usize, size, usize_ty()),
                ("ref", CGridRef, ref_, n("RusttyGridRef")),
                ("boundary_codepoints", *const u32, boundary_codepoints, Ty::Pointer("u32", true, true)),
                ("boundary_codepoints_len", usize, boundary_codepoints_len, usize_ty()),
            ])
        ),
        // .initStruct("GhosttyTerminalSelectionFormatOptions", selection.FormatOptions),
        struct_decl!(
            "RusttyTerminalSelectionFormatOptions",
            FormatOptions,
            fields!(FormatOptions, [
                ("size", usize, size, usize_ty()),
                ("emit", c_int, emit, n("RusttyFormatterFormat")),
                ("unwrap", bool, unwrap, n("bool")),
                ("trim", bool, trim, n("bool")),
                ("selection", *const CSelection, selection, Ty::Pointer("RusttySelection", true, true)),
            ])
        ),
        // .initTaggedStruct("GhosttyTerminalUnknownSequence", terminal.UnknownSequence.C, "tag", "value", .generated),
        TypeDecl::Struct {
            name: "RusttyTerminalUnknownSequence",
            size: size_of::<crate::terminal::UnknownSequence>(),
            align: align_of::<crate::terminal::UnknownSequence>(),
            fields: &[
                Field {
                    name: "tag",
                    offset: offset_of!(crate::terminal::UnknownSequence, tag),
                    size: size_of::<c_int>(),
                    ty: Ty::Named("RusttyTerminalUnknownSequenceTag"),
                    tagged: None,
                },
                Field {
                    name: "value",
                    offset: offset_of!(crate::terminal::UnknownSequence, value),
                    size: size_of::<crate::terminal::UnknownSequenceValue>(),
                    // fieldTypeOverride("GhosttyTerminalUnknownSequence", "value")
                    ty: Ty::Named("RusttyTerminalUnknownSequenceValue"),
                    tagged: Some(TaggedMeta {
                        tag_field: "tag",
                        tag_enum: "RusttyTerminalUnknownSequenceTag",
                        arms: crate::terminal::UNKNOWN_SEQUENCE_C_FIELD_ARMS,
                    }),
                },
            ],
        },
        // .initStruct("GhosttyTerminalUnknownStringSequence", terminal.UnknownStringSequence),
        struct_decl!(
            "RusttyTerminalUnknownStringSequence",
            crate::terminal::UnknownStringSequence,
            fields!(
                crate::terminal::UnknownStringSequence,
                [
                    ("truncated", bool, truncated, n("bool")),
                    ("content", LibString, content, n("RusttyString")),
                ]
            )
        ),
        // .initUnion(
        //     "GhosttyTerminalUnknownSequenceValue",
        //     terminal.UnknownSequence.CValue,
        //     terminal.UnknownSequence.C,
        // ),
        TypeDecl::Union {
            name: "RusttyTerminalUnknownSequenceValue",
            size: size_of::<crate::terminal::UnknownSequenceValue>(),
            align: align_of::<crate::terminal::UnknownSequenceValue>(),
            fields: &[
                Field {
                    name: "apc",
                    offset: 0,
                    size: size_of::<crate::terminal::UnknownStringSequence>(),
                    ty: Ty::Named("RusttyTerminalUnknownStringSequence"),
                    tagged: None,
                },
                Field {
                    name: "_padding",
                    offset: 0,
                    size: size_of::<[u64; 16]>(),
                    ty: Ty::Array("u64", 16),
                    tagged: None,
                },
            ],
        },
        // .initStruct("GhosttyWriter", io.Writer),
        struct_decl!(
            "RusttyWriter",
            crate::io::Writer,
            fields!(
                crate::io::Writer,
                [
                    // *const fn (...) -> pointer to function
                    (
                        "write",
                        Option<crate::io::WriterFn>,
                        write,
                        Ty::Pointer("function", true, true)
                    ),
                    (
                        "userdata",
                        *mut std::ffi::c_void,
                        userdata,
                        Ty::Pointer("opaque", false, true)
                    ),
                ]
            )
        ),
        //
        // ------------------------------------------------------------------
        // Enums.
        // ------------------------------------------------------------------
        // .initEnumSentinel("GhosttyResult", result.Result, "GHOSTTY_", "RESULT_MAX_VALUE"),
        enum_decl!(
            "RusttyResult",
            "RUSTTY_",
            "RESULT_MAX_VALUE",
            enum_values!(
                crate::result::Result,
                [
                    ("success", Success),
                    ("out_of_memory", OutOfMemory),
                    ("invalid_value", InvalidValue),
                    ("out_of_space", OutOfSpace),
                    ("no_value", NoValue),
                    ("io_error", IoError),
                    ("limit_exceeded", LimitExceeded),
                    ("rejected", Rejected),
                ]
            )
        ),
        // .initEnum("GhosttyBuildInfo", build_info.BuildInfo, "GHOSTTY_BUILD_INFO_"),
        enum_decl!(
            "RusttyBuildInfo",
            "RUSTTY_BUILD_INFO_",
            enum_values!(
                crate::build_info::BuildInfo,
                [
                    ("invalid", Invalid),
                    ("simd", Simd),
                    ("kitty_graphics", KittyGraphics),
                    ("tmux_control_mode", TmuxControlMode),
                    ("optimize", Optimize),
                    ("version_string", VersionString),
                    ("version_major", VersionMajor),
                    ("version_minor", VersionMinor),
                    ("version_patch", VersionPatch),
                    ("version_pre", VersionPre),
                    ("version_build", VersionBuild),
                ]
            )
        ),
        // .initEnumSentinel("GhosttyOptimizeMode", build_info.OptimizeMode, "GHOSTTY_OPTIMIZE_", "MODE_MAX_VALUE"),
        enum_decl!(
            "RusttyOptimizeMode",
            "RUSTTY_OPTIMIZE_",
            "MODE_MAX_VALUE",
            enum_values!(
                crate::build_info::OptimizeMode,
                [
                    ("debug", Debug),
                    ("release_safe", ReleaseSafe),
                    ("release_small", ReleaseSmall),
                    ("release_fast", ReleaseFast),
                ]
            )
        ),
        // .initEnumSentinel("GhosttyCellContentTag", cell.ContentTag, "GHOSTTY_CELL_CONTENT_", "TAG_MAX_VALUE"),
        enum_decl!(
            "RusttyCellContentTag",
            "RUSTTY_CELL_CONTENT_",
            "TAG_MAX_VALUE",
            enum_values!(
                crate::cell::ContentTag,
                [
                    ("codepoint", Codepoint),
                    ("codepoint_grapheme", CodepointGrapheme),
                    ("bg_color_palette", BgColorPalette),
                    ("bg_color_rgb", BgColorRgb),
                ]
            )
        ),
        // .initEnum("GhosttyCellData", cell.CellData, "GHOSTTY_CELL_DATA_"),
        enum_decl!(
            "RusttyCellData",
            "RUSTTY_CELL_DATA_",
            enum_values!(
                crate::cell::CellData,
                [
                    ("invalid", Invalid),
                    ("codepoint", Codepoint),
                    ("content_tag", ContentTag),
                    ("wide", Wide),
                    ("has_text", HasText),
                    ("has_styling", HasStyling),
                    ("style_id", StyleId),
                    ("has_hyperlink", HasHyperlink),
                    ("protected", Protected),
                    ("semantic_content", SemanticContent),
                    ("color_palette", ColorPalette),
                    ("color_rgb", ColorRgb),
                ]
            )
        ),
        // .initEnum("GhosttyCellSemanticContent", cell.SemanticContent, "GHOSTTY_CELL_SEMANTIC_"),
        enum_decl!(
            "RusttyCellSemanticContent",
            "RUSTTY_CELL_SEMANTIC_",
            enum_values!(
                crate::cell::SemanticContent,
                [("output", Output), ("input", Input), ("prompt", Prompt),]
            )
        ),
        // .initEnum("GhosttyCellWide", cell.Wide, "GHOSTTY_CELL_WIDE_"),
        enum_decl!(
            "RusttyCellWide",
            "RUSTTY_CELL_WIDE_",
            enum_values!(
                crate::cell::Wide,
                [
                    ("narrow", Narrow),
                    ("wide", Wide),
                    ("spacer_tail", SpacerTail),
                    ("spacer_head", SpacerHead),
                ]
            )
        ),
        // .initEnum("GhosttyClipboardLocation", clipboard.Location, "GHOSTTY_CLIPBOARD_LOCATION_"),
        enum_decl!(
            "RusttyClipboardLocation",
            "RUSTTY_CLIPBOARD_LOCATION_",
            enum_values!(
                rustty_vt::clipboard::Location,
                [
                    ("standard", Standard),
                    ("selection", Selection),
                    ("primary", Primary),
                ]
            )
        ),
        // .initEnum("GhosttyClipboardReadResult", clipboard.Read.Status, "GHOSTTY_CLIPBOARD_READ_RESULT_"),
        // PORT NOTE (e03475c0c): the reference type is `clipboard.Read.Status`
        // (flattened to `ReadStatus` — an open newtype, hence const rows).
        enum_decl!(
            "RusttyClipboardReadResult",
            "RUSTTY_CLIPBOARD_READ_RESULT_",
            enum_const_values!(
                rustty_vt::clipboard::ReadStatus,
                [
                    ("success", Success),
                    ("denied", Denied),
                    ("unsupported", Unsupported),
                    ("busy", Busy),
                    ("io_error", IoError),
                ]
            )
        ),
        // .initEnum("GhosttyClipboardWriteResult", clipboard.Write.Status, "GHOSTTY_CLIPBOARD_WRITE_RESULT_"),
        // PORT NOTE (7ae9b1113): the reference type is now `clipboard.Write.Status`
        // (flattened to `WriteStatus`); the C name is unchanged.
        enum_decl!(
            "RusttyClipboardWriteResult",
            "RUSTTY_CLIPBOARD_WRITE_RESULT_",
            enum_const_values!(
                rustty_vt::clipboard::WriteStatus,
                [
                    ("success", Success),
                    ("denied", Denied),
                    ("unsupported", Unsupported),
                    ("busy", Busy),
                    ("invalid_data", InvalidData),
                    ("io_error", IoError),
                ]
            )
        ),
        // .initEnum("GhosttyColorScheme", device_status.ColorScheme, "GHOSTTY_COLOR_SCHEME_"),
        enum_decl!(
            "RusttyColorScheme",
            "RUSTTY_COLOR_SCHEME_",
            enum_values!(
                rustty_vt::device_status::ColorScheme,
                [("light", Light), ("dark", Dark),]
            )
        ),
        // .initEnum("GhosttyFocusEvent", focus_pkg.Event, "GHOSTTY_FOCUS_"),
        enum_decl!(
            "RusttyFocusEvent",
            "RUSTTY_FOCUS_",
            enum_values!(
                rustty_vt::focus::Event,
                [("gained", Gained), ("lost", Lost),]
            )
        ),
        // .initEnum("GhosttyFormatterFormat", formatter_pkg.Format, "GHOSTTY_FORMATTER_FORMAT_"),
        enum_decl!(
            "RusttyFormatterFormat",
            "RUSTTY_FORMATTER_FORMAT_",
            enum_values!(
                rustty_vt::formatter::Format,
                [("plain", Plain), ("vt", Vt), ("html", Html),]
            )
        ),
        // .initEnum("GhosttyKey", input_key.Key, "GHOSTTY_KEY_"),
        enum_decl!("RusttyKey", "RUSTTY_KEY_", EnumValues::KeyTagNames),
        // .initEnum("GhosttyKeyAction", input_key.Action, "GHOSTTY_KEY_ACTION_"),
        enum_decl!(
            "RusttyKeyAction",
            "RUSTTY_KEY_ACTION_",
            enum_values!(
                rustty_vt::input::key::Action,
                [("release", Release), ("press", Press), ("repeat", Repeat),]
            )
        ),
        // .initEnum("GhosttyKeyEncoderOption", key_encode.Option, "GHOSTTY_KEY_ENCODER_OPT_"),
        enum_decl!(
            "RusttyKeyEncoderOption",
            "RUSTTY_KEY_ENCODER_OPT_",
            enum_values!(
                crate::key_encode::Option_,
                [
                    ("cursor_key_application", CursorKeyApplication),
                    ("keypad_key_application", KeypadKeyApplication),
                    ("ignore_keypad_with_numlock", IgnoreKeypadWithNumlock),
                    ("alt_esc_prefix", AltEscPrefix),
                    ("modify_other_keys_state_2", ModifyOtherKeysState2),
                    ("kitty_flags", KittyFlags),
                    ("macos_option_as_alt", MacosOptionAsAlt),
                    ("backarrow_key_mode", BackarrowKeyMode),
                ]
            )
        ),
        // .initEnum("GhosttyKittyGraphicsData", kitty_graphics.Data, "GHOSTTY_KITTY_GRAPHICS_DATA_"),
        enum_decl!(
            "RusttyKittyGraphicsData",
            "RUSTTY_KITTY_GRAPHICS_DATA_",
            enum_values!(
                crate::kitty_graphics::Data,
                [
                    ("invalid", Invalid),
                    ("placement_iterator", PlacementIterator),
                    ("generation", Generation),
                ]
            )
        ),
        // .initEnum("GhosttyKittyGraphicsImageData", kitty_graphics.ImageData, "GHOSTTY_KITTY_IMAGE_DATA_"),
        enum_decl!(
            "RusttyKittyGraphicsImageData",
            "RUSTTY_KITTY_IMAGE_DATA_",
            enum_values!(
                crate::kitty_graphics::ImageData,
                [
                    ("invalid", Invalid),
                    ("id", Id),
                    ("number", Number),
                    ("width", Width),
                    ("height", Height),
                    ("format", Format),
                    ("compression", Compression),
                    ("data_ptr", DataPtr),
                    ("data_len", DataLen),
                    ("generation", Generation),
                ]
            )
        ),
        // .initEnum("GhosttyKittyGraphicsPlacementData", kitty_graphics.PlacementData, "GHOSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_"),
        enum_decl!(
            "RusttyKittyGraphicsPlacementData",
            "RUSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_",
            enum_values!(
                crate::kitty_graphics::PlacementData,
                [
                    ("invalid", Invalid),
                    ("image_id", ImageId),
                    ("placement_id", PlacementId),
                    ("is_virtual", IsVirtual),
                    ("x_offset", XOffset),
                    ("y_offset", YOffset),
                    ("source_x", SourceX),
                    ("source_y", SourceY),
                    ("source_width", SourceWidth),
                    ("source_height", SourceHeight),
                    ("columns", Columns),
                    ("rows", Rows),
                    ("z", Z),
                ]
            )
        ),
        // .initEnum("GhosttyKittyGraphicsPlacementIteratorOption", ..., "GHOSTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_"),
        enum_decl!(
            "RusttyKittyGraphicsPlacementIteratorOption",
            "RUSTTY_KITTY_GRAPHICS_PLACEMENT_ITERATOR_OPTION_",
            enum_values!(
                crate::kitty_graphics::PlacementIteratorOption,
                [("layer", Layer),]
            )
        ),
        // .initEnum("GhosttyKittyImageCompression", kitty_graphics.ImageCompression, "GHOSTTY_KITTY_IMAGE_COMPRESSION_"),
        enum_decl!(
            "RusttyKittyImageCompression",
            "RUSTTY_KITTY_IMAGE_COMPRESSION_",
            enum_values!(
                crate::kitty_graphics::ImageCompression,
                [("none", None), ("zlib_deflate", ZlibDeflate),]
            )
        ),
        // .initEnum("GhosttyKittyImageFormat", kitty_graphics.ImageFormat, "GHOSTTY_KITTY_IMAGE_FORMAT_"),
        enum_decl!(
            "RusttyKittyImageFormat",
            "RUSTTY_KITTY_IMAGE_FORMAT_",
            enum_values!(
                crate::kitty_graphics::ImageFormat,
                [
                    ("rgb", Rgb),
                    ("rgba", Rgba),
                    ("png", Png),
                    ("gray_alpha", GrayAlpha),
                    ("gray", Gray),
                ]
            )
        ),
        // .initEnum("GhosttyKittyPlacementLayer", kitty_graphics.PlacementLayer, "GHOSTTY_KITTY_PLACEMENT_LAYER_"),
        enum_decl!(
            "RusttyKittyPlacementLayer",
            "RUSTTY_KITTY_PLACEMENT_LAYER_",
            enum_values!(
                crate::kitty_graphics::PlacementLayer,
                [
                    ("all", All),
                    ("below_bg", BelowBg),
                    ("below_text", BelowText),
                    ("above_text", AboveText),
                ]
            )
        ),
        // .initEnum("GhosttyModeReportState", modes_pkg.Report.State, "GHOSTTY_MODE_REPORT_"),
        enum_decl!(
            "RusttyModeReportState",
            "RUSTTY_MODE_REPORT_",
            enum_values!(
                rustty_vt::modes::State,
                [
                    ("not_recognized", NotRecognized),
                    ("set", Set),
                    ("reset", Reset),
                    ("permanently_set", PermanentlySet),
                    ("permanently_reset", PermanentlyReset),
                ]
            )
        ),
        // .initEnum("GhosttyMouseAction", input_mouse.Action, "GHOSTTY_MOUSE_ACTION_"),
        enum_decl!(
            "RusttyMouseAction",
            "RUSTTY_MOUSE_ACTION_",
            enum_values!(
                rustty_vt::input::mouse::Action,
                [("press", Press), ("release", Release), ("motion", Motion),]
            )
        ),
        // .initEnum("GhosttyMouseButton", input_mouse.Button, "GHOSTTY_MOUSE_BUTTON_"),
        enum_decl!(
            "RusttyMouseButton",
            "RUSTTY_MOUSE_BUTTON_",
            enum_values!(
                rustty_vt::input::mouse::Button,
                [
                    ("unknown", Unknown),
                    ("left", Left),
                    ("right", Right),
                    ("middle", Middle),
                    ("four", Four),
                    ("five", Five),
                    ("six", Six),
                    ("seven", Seven),
                    ("eight", Eight),
                    ("nine", Nine),
                    ("ten", Ten),
                    ("eleven", Eleven),
                ]
            )
        ),
        // .initEnum("GhosttyMouseEncoderOption", mouse_encode.Option, "GHOSTTY_MOUSE_ENCODER_OPT_"),
        enum_decl!(
            "RusttyMouseEncoderOption",
            "RUSTTY_MOUSE_ENCODER_OPT_",
            enum_values!(
                crate::mouse_encode::Option_,
                [
                    ("event", Event),
                    ("format", Format),
                    ("size", Size),
                    ("any_button_pressed", AnyButtonPressed),
                    ("track_last_cell", TrackLastCell),
                ]
            )
        ),
        // .initEnum("GhosttyMouseFormat", mouse_pkg.Format, "GHOSTTY_MOUSE_FORMAT_"),
        enum_decl!(
            "RusttyMouseFormat",
            "RUSTTY_MOUSE_FORMAT_",
            enum_values!(
                rustty_vt::mouse::Format,
                [
                    ("x10", X10),
                    ("utf8", Utf8),
                    ("sgr", Sgr),
                    ("urxvt", Urxvt),
                    ("sgr_pixels", SgrPixels),
                ]
            )
        ),
        // .initEnum("GhosttyMouseTrackingMode", mouse_pkg.Event, "GHOSTTY_MOUSE_TRACKING_"),
        enum_decl!(
            "RusttyMouseTrackingMode",
            "RUSTTY_MOUSE_TRACKING_",
            enum_values!(
                rustty_vt::mouse::Event,
                [
                    ("none", None),
                    ("x10", X10),
                    ("normal", Normal),
                    ("button", Button),
                    ("any", Any),
                ]
            )
        ),
        // .initEnum("GhosttyOptionAsAlt", input_config.OptionAsAlt, "GHOSTTY_OPTION_AS_ALT_"),
        enum_decl!(
            "RusttyOptionAsAlt",
            "RUSTTY_OPTION_AS_ALT_",
            enum_values!(
                rustty_vt::input::config::OptionAsAlt,
                [
                    ("false", r#false),
                    ("true", r#true),
                    ("left", left),
                    ("right", right),
                ]
            )
        ),
        // .initEnum("GhosttyOscCommandData", osc.CommandData, "GHOSTTY_OSC_DATA_"),
        enum_decl!(
            "RusttyOscCommandData",
            "RUSTTY_OSC_DATA_",
            enum_values!(
                crate::osc::CommandData,
                [
                    ("invalid", Invalid),
                    ("change_window_title_str", ChangeWindowTitleStr),
                ]
            )
        ),
        // .initEnumSentinel("GhosttyOscCommandType", osc.CommandType, "GHOSTTY_OSC_COMMAND_", "TYPE_MAX_VALUE"),
        enum_decl!(
            "RusttyOscCommandType",
            "RUSTTY_OSC_COMMAND_",
            "TYPE_MAX_VALUE",
            enum_values!(
                rustty_vt::osc::CommandKey,
                [
                    ("invalid", Invalid),
                    ("change_window_title", ChangeWindowTitle),
                    ("change_window_icon", ChangeWindowIcon),
                    ("semantic_prompt", SemanticPrompt),
                    ("clipboard_contents", ClipboardContents),
                    ("report_pwd", ReportPwd),
                    ("mouse_shape", MouseShape),
                    ("color_operation", ColorOperation),
                    ("kitty_color_protocol", KittyColorProtocol),
                    ("show_desktop_notification", ShowDesktopNotification),
                    ("hyperlink_start", HyperlinkStart),
                    ("hyperlink_end", HyperlinkEnd),
                    ("conemu_sleep", ConemuSleep),
                    ("conemu_show_message_box", ConemuShowMessageBox),
                    ("conemu_change_tab_title", ConemuChangeTabTitle),
                    ("conemu_progress_report", ConemuProgressReport),
                    ("conemu_wait_input", ConemuWaitInput),
                    ("conemu_guimacro", ConemuGuimacro),
                    ("conemu_run_process", ConemuRunProcess),
                    (
                        "conemu_output_environment_variable",
                        ConemuOutputEnvironmentVariable
                    ),
                    ("conemu_xterm_emulation", ConemuXtermEmulation),
                    ("conemu_comment", ConemuComment),
                    ("kitty_text_sizing", KittyTextSizing),
                    ("kitty_clipboard_protocol", KittyClipboardProtocol),
                    ("kitty_dnd_protocol", KittyDndProtocol),
                    ("context_signal", ContextSignal),
                    ("kitty_desktop_notification", KittyDesktopNotification),
                ]
            )
        ),
        // .initEnum("GhosttyPasteSource", paste.Source, "GHOSTTY_PASTE_SOURCE_"),
        enum_decl!(
            "RusttyPasteSource",
            "RUSTTY_PASTE_SOURCE_",
            enum_values!(
                crate::paste::Source,
                [("clipboard", Clipboard), ("text", Text),]
            )
        ),
        // .initEnum("GhosttyPointTag", point.Tag, "GHOSTTY_POINT_TAG_"),
        enum_decl!(
            "RusttyPointTag",
            "RUSTTY_POINT_TAG_",
            enum_values!(
                crate::point::TagC,
                [
                    ("active", Active),
                    ("viewport", Viewport),
                    ("screen", Screen),
                    ("history", History),
                ]
            )
        ),
        // .initEnum("GhosttyRenderStateCursorVisualStyle", render.CursorVisualStyle, "GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_"),
        enum_decl!(
            "RusttyRenderStateCursorVisualStyle",
            "RUSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_",
            enum_values!(
                crate::render::CursorVisualStyle,
                [
                    ("bar", Bar),
                    ("block", Block),
                    ("underline", Underline),
                    ("block_hollow", BlockHollow),
                ]
            )
        ),
        // .initEnum("GhosttyRenderStateData", render.Data, "GHOSTTY_RENDER_STATE_DATA_"),
        enum_decl!(
            "RusttyRenderStateData",
            "RUSTTY_RENDER_STATE_DATA_",
            enum_values!(
                crate::render::Data,
                [
                    ("invalid", Invalid),
                    ("cols", Cols),
                    ("rows", Rows),
                    ("dirty", Dirty),
                    ("row_iterator", RowIterator),
                    ("color_background", ColorBackground),
                    ("color_foreground", ColorForeground),
                    ("color_cursor", ColorCursor),
                    ("color_cursor_has_value", ColorCursorHasValue),
                    ("color_palette", ColorPalette),
                    ("cursor_visual_style", CursorVisualStyle),
                    ("cursor_visible", CursorVisible),
                    ("cursor_blinking", CursorBlinking),
                    ("cursor_password_input", CursorPasswordInput),
                    ("cursor_viewport_has_value", CursorViewportHasValue),
                    ("cursor_viewport_x", CursorViewportX),
                    ("cursor_viewport_y", CursorViewportY),
                    ("cursor_viewport_wide_tail", CursorViewportWideTail),
                    ("cursor", Cursor),
                    ("colors", Colors),
                ]
            )
        ),
        // .initEnum("GhosttyRenderStateDirty", render.Dirty, "GHOSTTY_RENDER_STATE_DIRTY_"),
        enum_decl!(
            "RusttyRenderStateDirty",
            "RUSTTY_RENDER_STATE_DIRTY_",
            enum_values!(
                crate::render::Dirty,
                [("false", False), ("partial", Partial), ("full", Full),]
            )
        ),
        // .initEnum("GhosttyRenderStateOption", render.SetOption, "GHOSTTY_RENDER_STATE_OPTION_"),
        enum_decl!(
            "RusttyRenderStateOption",
            "RUSTTY_RENDER_STATE_OPTION_",
            enum_values!(crate::render::SetOption, [("dirty", Dirty)])
        ),
        // .initEnum("GhosttyRenderStateRowCellsData", render.RowCellsData, "GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_"),
        enum_decl!(
            "RusttyRenderStateRowCellsData",
            "RUSTTY_RENDER_STATE_ROW_CELLS_DATA_",
            enum_values!(
                crate::render::RowCellsData,
                [
                    ("invalid", Invalid),
                    ("raw", Raw),
                    ("style", Style),
                    ("graphemes_len", GraphemesLen),
                    ("graphemes_buf", GraphemesBuf),
                    ("bg_color", BgColor),
                    ("fg_color", FgColor),
                    ("selected", Selected),
                    ("has_styling", HasStyling),
                    ("graphemes_utf8", GraphemesUtf8),
                ]
            )
        ),
        // .initEnum("GhosttyRenderStateRowData", render.RowData, "GHOSTTY_RENDER_STATE_ROW_DATA_"),
        enum_decl!(
            "RusttyRenderStateRowData",
            "RUSTTY_RENDER_STATE_ROW_DATA_",
            enum_values!(
                crate::render::RowData,
                [
                    ("invalid", Invalid),
                    ("dirty", Dirty),
                    ("raw", Raw),
                    ("cells", Cells),
                    ("selection", Selection),
                    ("cells_raw", CellsRaw),
                ]
            )
        ),
        // .initEnum("GhosttyRenderStateRowOption", render.RowOption, "GHOSTTY_RENDER_STATE_ROW_OPTION_"),
        enum_decl!(
            "RusttyRenderStateRowOption",
            "RUSTTY_RENDER_STATE_ROW_OPTION_",
            enum_values!(crate::render::RowOption, [("dirty", Dirty)])
        ),
        // .initEnum("GhosttyRowData", row.RowData, "GHOSTTY_ROW_DATA_"),
        enum_decl!(
            "RusttyRowData",
            "RUSTTY_ROW_DATA_",
            enum_values!(
                crate::row::RowData,
                [
                    ("invalid", Invalid),
                    ("wrap", Wrap),
                    ("wrap_continuation", WrapContinuation),
                    ("grapheme", Grapheme),
                    ("styled", Styled),
                    ("hyperlink", Hyperlink),
                    ("semantic_prompt", SemanticPrompt),
                    ("kitty_virtual_placeholder", KittyVirtualPlaceholder),
                    ("dirty", Dirty),
                ]
            )
        ),
        // .initEnum("GhosttyRowSemanticPrompt", row.SemanticPrompt, "GHOSTTY_ROW_SEMANTIC_"),
        enum_decl!(
            "RusttyRowSemanticPrompt",
            "RUSTTY_ROW_SEMANTIC_",
            enum_values!(
                crate::row::SemanticPrompt,
                [
                    ("none", None),
                    ("prompt", Prompt),
                    ("prompt_continuation", PromptContinuation),
                ]
            )
        ),
        // .initEnum("GhosttySearchData", search.Data, "GHOSTTY_SEARCH_DATA_"),
        enum_decl!(
            "RusttySearchData",
            "RUSTTY_SEARCH_DATA_",
            enum_values!(
                SearchData,
                [
                    ("status", Status),
                    ("needle", Needle),
                    ("total_matches", TotalMatches),
                    ("selected_index", SelectedIndex),
                    ("selected_match", SelectedMatch),
                    ("matches", Matches),
                    ("viewport_matches", ViewportMatches),
                    ("select_scroll", SelectScroll),
                ]
            )
        ),
        // .initEnum("GhosttySearchOption", search.Option, "GHOSTTY_SEARCH_OPT_"),
        enum_decl!(
            "RusttySearchOption",
            "RUSTTY_SEARCH_OPT_",
            enum_values!(
                SearchOption,
                [
                    ("needle", Needle),
                    ("select_next", SelectNext),
                    ("select_prev", SelectPrev),
                    ("select_scroll", SelectScroll),
                ]
            )
        ),
        // .initEnum("GhosttySearchScroll", search.Scroll, "GHOSTTY_SEARCH_SCROLL_"),
        enum_decl!(
            "RusttySearchScroll",
            "RUSTTY_SEARCH_SCROLL_",
            enum_values!(SearchScroll, [("if_needed", IfNeeded), ("none", None)])
        ),
        // .initEnum("GhosttySearchStatus", search.Status, "GHOSTTY_SEARCH_STATUS_"),
        enum_decl!(
            "RusttySearchStatus",
            "RUSTTY_SEARCH_STATUS_",
            enum_values!(
                SearchStatus,
                [
                    ("running", Running),
                    ("feed_required", FeedRequired),
                    ("complete", Complete),
                ]
            )
        ),
        // .initEnum("GhosttySelectionAdjust", Selection.Adjustment, "GHOSTTY_SELECTION_ADJUST_"),
        enum_decl!(
            "RusttySelectionAdjust",
            "RUSTTY_SELECTION_ADJUST_",
            enum_values!(
                crate::selection::Adjustment,
                [
                    ("left", Left),
                    ("right", Right),
                    ("up", Up),
                    ("down", Down),
                    ("home", Home),
                    ("end", End),
                    ("page_up", PageUp),
                    ("page_down", PageDown),
                    ("beginning_of_line", BeginningOfLine),
                    ("end_of_line", EndOfLine),
                ]
            )
        ),
        // .initEnum("GhosttySelectionGestureAutoscroll", selection_gesture.Autoscroll, "GHOSTTY_SELECTION_GESTURE_AUTOSCROLL_"),
        enum_decl!(
            "RusttySelectionGestureAutoscroll",
            "RUSTTY_SELECTION_GESTURE_AUTOSCROLL_",
            enum_values!(
                crate::selection_gesture::Autoscroll,
                [("none", None), ("up", Up), ("down", Down),]
            )
        ),
        // .initEnum("GhosttySelectionGestureBehavior", selection_gesture.Behavior, "GHOSTTY_SELECTION_GESTURE_BEHAVIOR_"),
        enum_decl!(
            "RusttySelectionGestureBehavior",
            "RUSTTY_SELECTION_GESTURE_BEHAVIOR_",
            enum_values!(
                crate::selection_gesture::Behavior,
                [
                    ("cell", Cell),
                    ("word", Word),
                    ("line", Line),
                    ("output", Output),
                ]
            )
        ),
        // .initEnum("GhosttySelectionGestureData", selection_gesture.Data, "GHOSTTY_SELECTION_GESTURE_DATA_"),
        enum_decl!(
            "RusttySelectionGestureData",
            "RUSTTY_SELECTION_GESTURE_DATA_",
            enum_values!(
                crate::selection_gesture::Data,
                [
                    ("click_count", ClickCount),
                    ("dragged", Dragged),
                    ("autoscroll", Autoscroll),
                    ("behavior", Behavior),
                    ("anchor", Anchor),
                ]
            )
        ),
        // .initEnum("GhosttySelectionGestureEventOption", selection_gesture.EventOption, "GHOSTTY_SELECTION_GESTURE_EVENT_OPT_"),
        enum_decl!(
            "RusttySelectionGestureEventOption",
            "RUSTTY_SELECTION_GESTURE_EVENT_OPT_",
            enum_values!(
                crate::selection_gesture::EventOption,
                [
                    ("ref", Ref),
                    ("position", Position),
                    ("repeat_distance", RepeatDistance),
                    ("time_ns", TimeNs),
                    ("repeat_interval_ns", RepeatIntervalNs),
                    ("word_boundary_codepoints", WordBoundaryCodepoints),
                    ("behaviors", Behaviors),
                    ("rectangle", Rectangle),
                    ("geometry", Geometry),
                    ("viewport", Viewport),
                ]
            )
        ),
        // .initEnum("GhosttySelectionGestureEventType", selection_gesture.EventType, "GHOSTTY_SELECTION_GESTURE_EVENT_TYPE_"),
        enum_decl!(
            "RusttySelectionGestureEventType",
            "RUSTTY_SELECTION_GESTURE_EVENT_TYPE_",
            enum_values!(
                crate::selection_gesture::EventType,
                [
                    ("press", Press),
                    ("release", Release),
                    ("drag", Drag),
                    ("autoscroll_tick", AutoscrollTick),
                    ("deep_press", DeepPress),
                ]
            )
        ),
        // .initEnum("GhosttySelectionOrder", Selection.Order, "GHOSTTY_SELECTION_ORDER_"),
        enum_decl!(
            "RusttySelectionOrder",
            "RUSTTY_SELECTION_ORDER_",
            enum_values!(
                crate::selection::Order,
                [
                    ("forward", Forward),
                    ("reverse", Reverse),
                    ("mirrored_forward", MirroredForward),
                    ("mirrored_reverse", MirroredReverse),
                ]
            )
        ),
        // .initEnum("GhosttySgrAttributeTag", sgr.Attribute.Tag, "GHOSTTY_SGR_ATTR_"),
        enum_decl!(
            "RusttySgrAttributeTag",
            "RUSTTY_SGR_ATTR_",
            enum_values!(
                crate::sgr::Tag,
                [
                    ("unset", Unset),
                    ("unknown", Unknown),
                    ("bold", Bold),
                    ("reset_bold", ResetBold),
                    ("italic", Italic),
                    ("reset_italic", ResetItalic),
                    ("faint", Faint),
                    ("underline", Underline),
                    ("underline_color", UnderlineColor),
                    ("256_underline_color", Color256Underline),
                    ("reset_underline_color", ResetUnderlineColor),
                    ("overline", Overline),
                    ("reset_overline", ResetOverline),
                    ("blink", Blink),
                    ("reset_blink", ResetBlink),
                    ("inverse", Inverse),
                    ("reset_inverse", ResetInverse),
                    ("invisible", Invisible),
                    ("reset_invisible", ResetInvisible),
                    ("strikethrough", Strikethrough),
                    ("reset_strikethrough", ResetStrikethrough),
                    ("direct_color_fg", DirectColorFg),
                    ("direct_color_bg", DirectColorBg),
                    ("8_bg", Color8Bg),
                    ("8_fg", Color8Fg),
                    ("reset_fg", ResetFg),
                    ("reset_bg", ResetBg),
                    ("8_bright_bg", Color8BrightBg),
                    ("8_bright_fg", Color8BrightFg),
                    ("256_bg", Color256Bg),
                    ("256_fg", Color256Fg),
                ]
            )
        ),
        // .initEnum("GhosttySgrUnderline", sgr.Attribute.Underline, "GHOSTTY_SGR_UNDERLINE_"),
        enum_decl!(
            "RusttySgrUnderline",
            "RUSTTY_SGR_UNDERLINE_",
            enum_values!(
                rustty_vt::sgr::Underline,
                [
                    ("none", None),
                    ("single", Single),
                    ("double", Double),
                    ("curly", Curly),
                    ("dotted", Dotted),
                    ("dashed", Dashed),
                ]
            )
        ),
        // .initEnumSentinel("GhosttySizeReportStyle", size_report.Style, "GHOSTTY_SIZE_REPORT_", "STYLE_MAX_VALUE"),
        enum_decl!(
            "RusttySizeReportStyle",
            "RUSTTY_SIZE_REPORT_",
            "STYLE_MAX_VALUE",
            enum_values!(
                crate::size_report::SizeReportStyle,
                [
                    ("mode_2048", Mode2048),
                    ("csi_14_t", Csi14T),
                    ("csi_16_t", Csi16T),
                    ("csi_18_t", Csi18T),
                ]
            )
        ),
        // .initEnum("GhosttySnapshotDecoderData", snapshot.DecoderData, "GHOSTTY_SNAPSHOT_DECODER_DATA_"),
        enum_decl!(
            "RusttySnapshotDecoderData",
            "RUSTTY_SNAPSHOT_DECODER_DATA_",
            enum_values!(
                crate::snapshot::DecoderData,
                [
                    ("invalid", Invalid),
                    ("max_continuation_bytes", MaxContinuationBytes),
                    ("source_offset", SourceOffset),
                    ("history_rows_primary", HistoryRowsPrimary),
                    ("history_rows_alternate", HistoryRowsAlternate),
                    ("progress_screen", ProgressScreen),
                    ("progress_rows", ProgressRows),
                    ("progress_remaining", ProgressRemaining),
                    ("retain_continuation", RetainContinuation),
                ]
            )
        ),
        // .initEnum("GhosttySnapshotDecoderOption", snapshot.DecoderOption, "GHOSTTY_SNAPSHOT_DECODER_OPT_"),
        enum_decl!(
            "RusttySnapshotDecoderOption",
            "RUSTTY_SNAPSHOT_DECODER_OPT_",
            enum_values!(
                crate::snapshot::DecoderOption,
                [
                    ("max_continuation_bytes", MaxContinuationBytes),
                    ("retain_continuation", RetainContinuation),
                ]
            )
        ),
        // .initEnumSentinel("GhosttyStyleColorTag", style.ColorTag, "GHOSTTY_STYLE_COLOR_", "TAG_MAX_VALUE"),
        enum_decl!(
            "RusttyStyleColorTag",
            "RUSTTY_STYLE_COLOR_",
            "TAG_MAX_VALUE",
            enum_values!(
                crate::style::ColorTag,
                [("none", None), ("palette", Palette), ("rgb", Rgb),]
            )
        ),
        // .initEnum("GhosttySysLogLevel", sys.LogLevel, "GHOSTTY_SYS_LOG_LEVEL_"),
        enum_decl!(
            "RusttySysLogLevel",
            "RUSTTY_SYS_LOG_LEVEL_",
            enum_values!(
                crate::sys::LogLevel,
                [
                    ("error", Error),
                    ("warning", Warning),
                    ("info", Info),
                    ("debug", Debug),
                ]
            )
        ),
        // .initEnum("GhosttySysOption", sys.Option, "GHOSTTY_SYS_OPT_"),
        enum_decl!(
            "RusttySysOption",
            "RUSTTY_SYS_OPT_",
            enum_values!(
                crate::sys::Option_,
                [
                    ("userdata", Userdata),
                    ("decode_png", DecodePng),
                    ("log", Log),
                    ("random_secure", RandomSecure),
                ]
            )
        ),
        // .initEnum("GhosttyTerminalCompressionMode", terminal.CompressionMode, "GHOSTTY_TERMINAL_COMPRESSION_MODE_"),
        enum_decl!(
            "RusttyTerminalCompressionMode",
            "RUSTTY_TERMINAL_COMPRESSION_MODE_",
            enum_values!(
                crate::terminal::CompressionMode,
                [("incremental", Incremental), ("full", Full),]
            )
        ),
        // .initEnum("GhosttyTerminalCompressionResult", terminal.CompressionResult, "GHOSTTY_TERMINAL_COMPRESSION_RESULT_"),
        enum_decl!(
            "RusttyTerminalCompressionResult",
            "RUSTTY_TERMINAL_COMPRESSION_RESULT_",
            enum_values!(
                crate::terminal::CompressionResult,
                [
                    ("unsupported", Unsupported),
                    ("pending", Pending),
                    ("complete", Complete),
                ]
            )
        ),
        // .initEnum("GhosttyTerminalCursorStyle", terminal.TerminalCursorStyle, "GHOSTTY_TERMINAL_CURSOR_STYLE_"),
        enum_decl!(
            "RusttyTerminalCursorStyle",
            "RUSTTY_TERMINAL_CURSOR_STYLE_",
            enum_values!(
                crate::terminal::TerminalCursorStyle,
                [
                    ("bar", Bar),
                    ("block", Block),
                    ("underline", Underline),
                    ("block_hollow", BlockHollow),
                ]
            )
        ),
        // .initEnum("GhosttyTerminalData", terminal.TerminalData, "GHOSTTY_TERMINAL_DATA_"),
        enum_decl!(
            "RusttyTerminalData",
            "RUSTTY_TERMINAL_DATA_",
            enum_values!(
                crate::terminal::TerminalData,
                [
                    ("invalid", Invalid),
                    ("cols", Cols),
                    ("rows", Rows),
                    ("cursor_x", CursorX),
                    ("cursor_y", CursorY),
                    ("cursor_pending_wrap", CursorPendingWrap),
                    ("active_screen", ActiveScreen),
                    ("cursor_visible", CursorVisible),
                    ("kitty_keyboard_flags", KittyKeyboardFlags),
                    ("scrollbar", Scrollbar),
                    ("cursor_style", CursorStyle),
                    ("mouse_tracking", MouseTracking),
                    ("title", Title),
                    ("pwd", Pwd),
                    ("total_rows", TotalRows),
                    ("scrollback_rows", ScrollbackRows),
                    ("width_px", WidthPx),
                    ("height_px", HeightPx),
                    ("color_foreground", ColorForeground),
                    ("color_background", ColorBackground),
                    ("color_cursor", ColorCursor),
                    ("color_palette", ColorPalette),
                    ("color_foreground_default", ColorForegroundDefault),
                    ("color_background_default", ColorBackgroundDefault),
                    ("color_cursor_default", ColorCursorDefault),
                    ("color_palette_default", ColorPaletteDefault),
                    ("kitty_image_storage_limit", KittyImageStorageLimit),
                    ("kitty_image_medium_file", KittyImageMediumFile),
                    ("kitty_image_medium_temp_file", KittyImageMediumTempFile),
                    ("kitty_image_medium_shared_mem", KittyImageMediumSharedMem),
                    ("kitty_graphics", KittyGraphics),
                    ("selection", Selection),
                    ("viewport_active", ViewportActive),
                    ("vt_processing_error", VtProcessingError),
                    ("scrollback_max_bytes", ScrollbackMaxBytes),
                    ("scrollback_max_lines", ScrollbackMaxLines),
                    ("continuation_max_bytes", ContinuationMaxBytes),
                    ("mode", Mode),
                    ("vt_ground", VtGround),
                    ("cursor_at_prompt", CursorAtPrompt),
                    ("clipboard_write_max_bytes", ClipboardWriteMaxBytes),
                ]
            )
        ),
        // .initEnum("GhosttyTerminalOption", terminal.Option, "GHOSTTY_TERMINAL_OPT_"),
        enum_decl!(
            "RusttyTerminalOption",
            "RUSTTY_TERMINAL_OPT_",
            enum_values!(
                crate::terminal::Option_,
                [
                    ("userdata", Userdata),
                    ("write_pty", WritePty),
                    ("bell", Bell),
                    ("enquiry", Enquiry),
                    ("xtversion", Xtversion),
                    ("title_changed", TitleChanged),
                    // writeEnumObjectField: "size_cb" renders as "SIZE".
                    ("size_cb", SizeCb),
                    ("color_scheme", ColorScheme),
                    ("device_attributes", DeviceAttributes),
                    ("title", Title),
                    ("pwd", Pwd),
                    ("color_foreground", ColorForeground),
                    ("color_background", ColorBackground),
                    ("color_cursor", ColorCursor),
                    ("color_palette", ColorPalette),
                    ("kitty_image_storage_limit", KittyImageStorageLimit),
                    ("kitty_image_medium_file", KittyImageMediumFile),
                    ("kitty_image_medium_temp_file", KittyImageMediumTempFile),
                    ("kitty_image_medium_shared_mem", KittyImageMediumSharedMem),
                    ("apc_max_bytes", ApcMaxBytes),
                    ("apc_max_bytes_kitty", ApcMaxBytesKitty),
                    ("selection", Selection),
                    ("default_cursor_style", DefaultCursorStyle),
                    ("default_cursor_blink", DefaultCursorBlink),
                    ("glyph_protocol", GlyphProtocol),
                    ("pwd_changed", PwdChanged),
                    ("clipboard_write", ClipboardWrite),
                    ("scrollback_max_bytes", ScrollbackMaxBytes),
                    ("scrollback_max_lines", ScrollbackMaxLines),
                    ("desktop_notification", DesktopNotification),
                    ("progress_report", ProgressReport),
                    ("continuation_max_bytes", ContinuationMaxBytes),
                    ("title_report", TitleReport),
                    ("mode_default", ModeDefault),
                    ("mode", Mode),
                    ("unknown_sequence", UnknownSequence),
                    ("unknown_max_bytes", UnknownMaxBytes),
                    ("terminfo_name", TerminfoName),
                    ("clipboard_read", ClipboardRead),
                    ("clipboard_write_max_bytes", ClipboardWriteMaxBytes),
                ]
            )
        ),
        // .initEnum("GhosttyTerminalProgressState", terminal.ProgressState, "GHOSTTY_TERMINAL_PROGRESS_STATE_"),
        enum_decl!(
            "RusttyTerminalProgressState",
            "RUSTTY_TERMINAL_PROGRESS_STATE_",
            enum_values!(
                crate::terminal::ProgressState,
                [
                    ("remove", Remove),
                    ("set", Set),
                    ("error", Error),
                    ("indeterminate", Indeterminate),
                    ("pause", Pause),
                ]
            )
        ),
        // .initEnum("GhosttyTerminalScreen", terminal.TerminalScreen, "GHOSTTY_TERMINAL_SCREEN_"),
        enum_decl!(
            "RusttyTerminalScreen",
            "RUSTTY_TERMINAL_SCREEN_",
            enum_values!(
                crate::terminal::TerminalScreen,
                [("primary", Primary), ("alternate", Alternate),]
            )
        ),
        // .initEnum("GhosttyTerminalScrollViewportTag", ..., "GHOSTTY_SCROLL_VIEWPORT_"),
        enum_decl!(
            "RusttyTerminalScrollViewportTag",
            "RUSTTY_SCROLL_VIEWPORT_",
            enum_values!(
                crate::terminal::ScrollViewportTag,
                [
                    ("top", Top),
                    ("bottom", Bottom),
                    ("delta", Delta),
                    ("row", Row),
                ]
            )
        ),
        // .initEnum("GhosttyTerminalUnknownSequenceTag", terminal.UnknownSequence.Tag, "GHOSTTY_TERMINAL_UNKNOWN_SEQUENCE_"),
        enum_decl!(
            "RusttyTerminalUnknownSequenceTag",
            "RUSTTY_TERMINAL_UNKNOWN_SEQUENCE_",
            enum_values!(crate::terminal::UnknownSequenceTag, [("apc", Apc)])
        ),
        // .initPacked("GhosttyCell", page.Cell.CLayout),
        TypeDecl::Packed {
            name: "RusttyCell",
            layout: &rustty_vt::page::cell_c_layout::CLAYOUT,
        },
        // .initAlias("GhosttyColorPaletteIndex", u8, "u8"),
        TypeDecl::Alias {
            name: "RusttyColorPaletteIndex",
            size: size_of::<u8>(),
            align: align_of::<u8>(),
            alias_type: "u8",
        },
        // .initAlias("GhosttyKittyKeyFlags", u8, "u8"),
        TypeDecl::Alias {
            name: "RusttyKittyKeyFlags",
            size: size_of::<u8>(),
            align: align_of::<u8>(),
            alias_type: "u8",
        },
        // .initAlias("GhosttyMode", u16, "u16"),
        TypeDecl::Alias {
            name: "RusttyMode",
            size: size_of::<u16>(),
            align: align_of::<u16>(),
            alias_type: "u16",
        },
        // .initAlias("GhosttyMods", u16, "u16"),
        TypeDecl::Alias {
            name: "RusttyMods",
            size: size_of::<u16>(),
            align: align_of::<u16>(),
            alias_type: "u16",
        },
        // .initAlias("GhosttyRow", u64, "u64"),
        TypeDecl::Alias {
            name: "RusttyRow",
            size: size_of::<u64>(),
            align: align_of::<u64>(),
            alias_type: "u64",
        },
        // .initAlias("GhosttyStyleId", u16, "u16"),
        TypeDecl::Alias {
            name: "RusttyStyleId",
            size: size_of::<u16>(),
            align: align_of::<u16>(),
            alias_type: "u16",
        },
        // .initOpaque(...) x19.
        TypeDecl::Opaque {
            name: "RusttyFormatter",
        },
        TypeDecl::Opaque {
            name: "RusttyKeyEncoder",
        },
        TypeDecl::Opaque {
            name: "RusttyKeyEvent",
        },
        TypeDecl::Opaque {
            name: "RusttyKittyGraphics",
        },
        TypeDecl::Opaque {
            name: "RusttyKittyGraphicsImage",
        },
        TypeDecl::Opaque {
            name: "RusttyKittyGraphicsPlacementIterator",
        },
        TypeDecl::Opaque {
            name: "RusttyMouseEncoder",
        },
        TypeDecl::Opaque {
            name: "RusttyMouseEvent",
        },
        TypeDecl::Opaque {
            name: "RusttyOscCommand",
        },
        TypeDecl::Opaque {
            name: "RusttyOscParser",
        },
        TypeDecl::Opaque {
            name: "RusttyRenderState",
        },
        TypeDecl::Opaque {
            name: "RusttyRenderStateRowCells",
        },
        TypeDecl::Opaque {
            name: "RusttyRenderStateRowIterator",
        },
        // .initOpaque("GhosttySearch"),
        TypeDecl::Opaque {
            name: "RusttySearch",
        },
        TypeDecl::Opaque {
            name: "RusttySelectionGesture",
        },
        TypeDecl::Opaque {
            name: "RusttySelectionGestureEvent",
        },
        TypeDecl::Opaque {
            name: "RusttySgrParser",
        },
        // .initOpaque("GhosttySnapshotDecoder"),
        TypeDecl::Opaque {
            name: "RusttySnapshotDecoder",
        },
        TypeDecl::Opaque {
            name: "RusttyTerminal",
        },
        TypeDecl::Opaque {
            name: "RusttyTrackedGridRef",
        },
    ]
};

// comptime {
//     @setEvalBranchQuota(100_000);
//     for (type_decls, 0..) |decl, i| {
//         for (type_decls[0..i]) |previous| {
//             if (std.mem.eql(u8, decl.name, previous.name))
//                 @compileError("duplicate public ABI type: " ++ decl.name);
//         }
//     }
// }
const fn assert_no_duplicate_decls() {
    let mut i = 0;
    while i < DECLS.len() {
        let mut j = 0;
        while j < i {
            if str_eq(DECLS[i].name(), DECLS[j].name()) {
                panic!("duplicate public ABI type");
            }
            j += 1;
        }
        i += 1;
    }
}
const _: () = assert_no_duplicate_decls();

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

const fn push_i64(out: &mut [u8], len: &mut usize, v: i64) {
    if v < 0 {
        push_str(out, len, "-");
        push_usize(out, len, v.unsigned_abs() as usize);
    } else {
        push_usize(out, len, v as usize);
    }
}

const fn push_json_string(out: &mut [u8], len: &mut usize, s: &str) {
    push_str(out, len, "\"");
    // The struct/field names here are all simple ASCII identifiers; no escaping
    // is required, matching the reference output.
    push_str(out, len, s);
    push_str(out, len, "\"");
}

const fn push_bool(out: &mut [u8], len: &mut usize, v: bool) {
    push_str(out, len, if v { "true" } else { "false" });
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

const fn starts_with(s: &str, prefix: &str) -> bool {
    let (s, p) = (s.as_bytes(), prefix.as_bytes());
    if s.len() < p.len() {
        return false;
    }
    let mut i = 0;
    while i < p.len() {
        if s[i] != p[i] {
            return false;
        }
        i += 1;
    }
    true
}

// fn writeUpperObjectField(comptime name: []const u8, jws) ... {
//     var upper: [name.len]u8 = undefined;
//     for (name, 0..) |c, i| upper[i] = std.ascii.toUpper(c);
//     try jws.objectField(&upper);
// }
/// Writes `"NAME":` from `name[start..]`, ASCII-uppercased.
const fn push_upper_object_field(out: &mut [u8], len: &mut usize, name: &str, start: usize) {
    push_str(out, len, "\"");
    let b = name.as_bytes();
    let mut i = start;
    while i < b.len() {
        out[*len] = b[i].to_ascii_uppercase();
        *len += 1;
        i += 1;
    }
    push_str(out, len, "\":");
}

// fn writeEnumObjectField(comptime enum_name, comptime zig_name, jws) ...
const fn push_enum_object_field(out: &mut [u8], len: &mut usize, enum_name: &str, zig_name: &str) {
    // if (std.mem.eql(u8, enum_name, "GhosttyKey") and
    //     std.mem.startsWith(u8, zig_name, "key_"))
    //     return writeUpperObjectField(zig_name["key_".len..], jws);
    if str_eq(enum_name, "RusttyKey") && starts_with(zig_name, "key_") {
        push_upper_object_field(out, len, zig_name, "key_".len());
        return;
    }
    if str_eq(enum_name, "RusttySgrAttributeTag") {
        if str_eq(zig_name, "256_underline_color") {
            push_str(out, len, "\"UNDERLINE_COLOR_256\":");
            return;
        }
        if str_eq(zig_name, "8_bg") {
            push_str(out, len, "\"BG_8\":");
            return;
        }
        if str_eq(zig_name, "8_fg") {
            push_str(out, len, "\"FG_8\":");
            return;
        }
        if str_eq(zig_name, "8_bright_bg") {
            push_str(out, len, "\"BRIGHT_BG_8\":");
            return;
        }
        if str_eq(zig_name, "8_bright_fg") {
            push_str(out, len, "\"BRIGHT_FG_8\":");
            return;
        }
        if str_eq(zig_name, "256_bg") {
            push_str(out, len, "\"BG_256\":");
            return;
        }
        if str_eq(zig_name, "256_fg") {
            push_str(out, len, "\"FG_256\":");
            return;
        }
    }
    if str_eq(enum_name, "RusttyTerminalOption") && str_eq(zig_name, "size_cb") {
        push_str(out, len, "\"SIZE\":");
        return;
    }
    push_upper_object_field(out, len, zig_name, 0);
}

// fn isBuiltinType(name: []const u8) bool { ... }
// PORT NOTE: only the "manifest named references resolve" test consumes this
// (as in the reference, where lazy comptime analysis has the same effect).
#[cfg_attr(not(test), allow(dead_code))]
fn is_builtin_type(name: &str) -> bool {
    let builtins = [
        "array", "bool", "f32", "f64", "function", "i8", "i16", "i32", "i64", "opaque", "pointer",
        "u8", "u16", "u32", "u64", "void",
    ];
    if builtins.contains(&name) {
        return true;
    }
    let b = name.as_bytes();
    if b.len() > 1 && (b[0] == b'i' || b[0] == b'u') && (b'1'..=b'9').contains(&b[1]) {
        return b[2..].iter().all(|c| c.is_ascii_digit());
    }
    false
}

// fn writeSizeAlign(comptime T: type, jws) ... {
//     "size": @sizeOf(T); "align": @alignOf(T);
// }
const fn write_size_align(out: &mut [u8], len: &mut usize, size: usize, align: usize) {
    push_str(out, len, "\"size\":");
    push_usize(out, len, size);
    push_str(out, len, ",\"align\":");
    push_usize(out, len, align);
}

// fn writeFieldType / writePointerType / writePointerTypeNamed — the resolved
// labels live in each row's `Ty`; this writes the reference's exact key set.
const fn write_field_type(out: &mut [u8], len: &mut usize, ty: &Ty) {
    match ty {
        Ty::Named(name) => {
            push_str(out, len, "\"type\":");
            push_json_string(out, len, name);
        }
        Ty::Array(elem, count) => {
            push_str(out, len, "\"type\":\"array\",\"elem\":");
            push_json_string(out, len, elem);
            push_str(out, len, ",\"count\":");
            push_usize(out, len, *count);
        }
        Ty::Pointer(elem, is_const, nullable) => {
            push_str(out, len, "\"type\":\"pointer\",\"elem\":");
            push_json_string(out, len, elem);
            push_str(out, len, ",\"const\":");
            push_bool(out, len, *is_const);
            if *nullable {
                push_str(out, len, ",\"nullable\":true");
            }
        }
    }
}

// fn writeField(comptime decl, comptime name, comptime T, comptime offset, jws)
const fn write_field(out: &mut [u8], len: &mut usize, field: &Field) {
    push_json_string(out, len, field.name);
    push_str(out, len, ":{\"offset\":");
    push_usize(out, len, field.offset);
    push_str(out, len, ",\"size\":");
    push_usize(out, len, field.size);
    push_str(out, len, ",");
    write_field_type(out, len, &field.ty);
    // if (decl.tagged_union) |tagged| { ... "tag": tag_field; writeTaggedArms }
    if let Some(tagged) = &field.tagged {
        push_str(out, len, ",\"tag\":");
        push_json_string(out, len, tagged.tag_field);
        // fn writeTaggedArms: arm keys render through writeEnumObjectField
        // with the TAG enum's public name; values are the cFieldRename
        // results (null when the tag has no value field).
        push_str(out, len, ",\"arms\":{");
        let mut i = 0;
        while i < tagged.arms.len() {
            if i > 0 {
                push_str(out, len, ",");
            }
            let (zig_name, rename) = tagged.arms[i];
            push_enum_object_field(out, len, tagged.tag_enum, zig_name);
            match rename {
                Some(arm) => push_json_string(out, len, arm),
                None => push_str(out, len, "null"),
            }
            i += 1;
        }
        push_str(out, len, "}");
    }
    push_str(out, len, "}");
}

// fn writePackedBits(comptime Layout: type, jws) ...
const fn write_packed_bits(out: &mut [u8], len: &mut usize, layout: &PackedLayout) {
    let mut first = true;
    let mut i = 0;
    while i < layout.fields.len() {
        // const name = comptime Layout.fieldName(field_tag) orelse continue;
        let name = match layout.field_name(i) {
            Some(name) => name,
            None => {
                i += 1;
                continue;
            }
        };
        let field = &layout.fields[i];
        let options = layout.field_options(i);

        if !first {
            push_str(out, len, ",");
        }
        first = false;
        push_json_string(out, len, name);
        push_str(out, len, ":{\"lsb\":");
        push_usize(out, len, field.bit_offset);
        push_str(out, len, ",\"width\":");
        push_usize(out, len, field.bit_width);

        match options.encoding {
            // .scalar => "type": options.type_name orelse publicTypeName(field.type)
            Encoding::Scalar => {
                push_str(out, len, ",\"type\":");
                push_json_string(
                    out,
                    len,
                    match options.type_name {
                        Some(name) => name,
                        None => field.zig_type_name,
                    },
                );
            }
            Encoding::Packed(nested) => {
                push_str(out, len, ",\"kind\":\"packed\",\"bits\":{");
                write_packed_bits(out, len, nested);
                push_str(out, len, "}");
            }
            Encoding::TaggedUnion(union_layout) => {
                write_packed_tagged_union(out, len, layout, union_layout);
            }
        }
        push_str(out, len, "}");
        i += 1;
    }
}

// fn writePackedTaggedUnion(comptime Layout, comptime UnionLayout, jws) ...
const fn write_packed_tagged_union(
    out: &mut [u8],
    len: &mut usize,
    layout: &PackedLayout,
    union_layout: &PackedTaggedUnionLayout,
) {
    push_str(out, len, ",\"kind\":\"union\",\"tag\":");
    // Layout.fieldName(UnionLayout.tag_field).?
    let tag_index = layout.field_index(union_layout.tag_field);
    push_json_string(
        out,
        len,
        match layout.field_name(tag_index) {
            Some(name) => name,
            None => panic!("packed tagged union tag field is omitted"),
        },
    );
    push_str(out, len, ",\"arms\":{");

    // const tag_type_name = tag_options.type_name orelse publicTypeName(UnionLayout.Tag);
    let tag_options = layout.field_options(tag_index);
    let tag_type_name = match tag_options.type_name {
        Some(name) => name,
        None => layout.fields[tag_index].zig_type_name,
    };
    let mut i = 0;
    while i < union_layout.arms.len() {
        if i > 0 {
            push_str(out, len, ",");
        }
        let arm = &union_layout.arms[i];
        push_enum_object_field(out, len, tag_type_name, arm.tag_name);
        match arm.source {
            None => push_str(out, len, "null"),
            Some((_, arm_layout)) => write_packed_arm(out, len, arm_layout),
        }
        i += 1;
    }
    push_str(out, len, "}");
}

// fn writePackedArm(comptime Layout: type, jws) ...
const fn write_packed_arm(out: &mut [u8], len: &mut usize, layout: &PackedLayout) {
    push_str(out, len, "{\"kind\":\"packed\",\"width\":");
    // @bitSizeOf(Layout.Zig)
    push_usize(out, len, layout.bit_size);
    push_str(out, len, ",\"bits\":{");
    write_packed_bits(out, len, layout);
    push_str(out, len, "}}");
}

// fn writePackedType(comptime Layout: type, jws) ...
const fn write_packed_type(out: &mut [u8], len: &mut usize, layout: &PackedLayout) {
    // writeSizeAlign(Layout.Zig)
    write_size_align(out, len, layout.size, layout.align);
    push_str(out, len, ",\"underlying\":");
    // publicTypeName(Layout.Backing)
    push_json_string(out, len, layout.backing);
    push_str(out, len, ",\"bits\":{");
    write_packed_bits(out, len, layout);
    push_str(out, len, "}");
}

// fn writeType(comptime decl: TypeDecl, jws) ...
const fn write_type(out: &mut [u8], len: &mut usize, decl: &TypeDecl) {
    push_str(out, len, "{\"kind\":");
    push_json_string(
        out,
        len,
        match decl {
            TypeDecl::Struct { .. } => "struct",
            TypeDecl::Union { .. } => "union",
            TypeDecl::Enum { .. } => "enum",
            TypeDecl::Packed { .. } => "packed",
            TypeDecl::Alias { .. } => "alias",
            TypeDecl::Opaque { .. } => "opaque",
        },
    );
    push_str(out, len, ",");

    match decl {
        TypeDecl::Struct {
            size,
            align,
            fields,
            ..
        }
        | TypeDecl::Union {
            size,
            align,
            fields,
            ..
        } => {
            write_size_align(out, len, *size, *align);
            push_str(out, len, ",\"fields\":{");
            let mut i = 0;
            while i < fields.len() {
                if i > 0 {
                    push_str(out, len, ",");
                }
                write_field(out, len, &fields[i]);
                i += 1;
            }
            push_str(out, len, "}");
        }
        TypeDecl::Enum {
            name,
            prefix,
            sentinel_suffix,
            values,
        } => {
            // writeSizeAlign(c_int); "underlying":"i32"
            write_size_align(out, len, size_of::<c_int>(), align_of::<c_int>());
            push_str(out, len, ",\"underlying\":\"i32\",\"prefix\":");
            push_json_string(out, len, prefix);
            push_str(out, len, ",\"values\":{");
            match values {
                EnumValues::Rows(rows) => {
                    let mut i = 0;
                    while i < rows.len() {
                        if i > 0 {
                            push_str(out, len, ",");
                        }
                        let (zig_name, value) = rows[i];
                        push_enum_object_field(out, len, name, zig_name);
                        push_i64(out, len, value);
                        i += 1;
                    }
                }
                EnumValues::KeyTagNames => {
                    let rows = rustty_vt::input::key::TAG_NAMES;
                    let mut i = 0;
                    while i < rows.len() {
                        if i > 0 {
                            push_str(out, len, ",");
                        }
                        let (key, zig_name) = rows[i];
                        push_enum_object_field(out, len, name, zig_name);
                        push_i64(out, len, key as i64);
                        i += 1;
                    }
                }
            }
            // jws.objectField(decl.sentinel_suffix); jws.write(maxInt(c_int));
            push_str(out, len, ",");
            push_json_string(out, len, sentinel_suffix);
            push_str(out, len, ":");
            push_i64(out, len, c_int::MAX as i64);
            push_str(out, len, "}");
        }
        TypeDecl::Packed { layout, .. } => write_packed_type(out, len, layout),
        TypeDecl::Alias {
            size,
            align,
            alias_type,
            ..
        } => {
            write_size_align(out, len, *size, *align);
            push_str(out, len, ",\"type\":");
            push_json_string(out, len, alias_type);
        }
        TypeDecl::Opaque { .. } => {
            // writeSizeAlign(*anyopaque)
            write_size_align(
                out,
                len,
                size_of::<*const std::ffi::c_void>(),
                align_of::<*const std::ffi::c_void>(),
            );
        }
    }
    push_str(out, len, "}");
}

// PORT NOTE: `@tagName(builtin.target.cpu.arch/os.tag/abi)` — the Zig target
// tag names, mapped from the Rust cfg values (they coincide for the
// arch names; wasm32-unknown-unknown's os is Zig `.freestanding`, and the
// empty Rust `target_env` is Zig abi `.none`).
const ABI_TARGET: &str = if cfg!(target_arch = "aarch64") {
    "aarch64"
} else if cfg!(target_arch = "x86_64") {
    "x86_64"
} else if cfg!(target_arch = "wasm32") {
    "wasm32"
} else {
    "unknown"
};
const ABI_OS: &str = if cfg!(target_os = "macos") {
    "macos"
} else if cfg!(target_os = "linux") {
    "linux"
} else if cfg!(target_os = "windows") {
    "windows"
} else if cfg!(target_arch = "wasm32") {
    "freestanding"
} else {
    "unknown"
};
const ABI_ENVIRONMENT: &str = if cfg!(target_env = "gnu") {
    "gnu"
} else if cfg!(target_env = "musl") {
    "musl"
} else if cfg!(target_env = "msvc") {
    "msvc"
} else {
    "none"
};
const ABI_ENDIAN: &str = if cfg!(target_endian = "little") {
    "little"
} else {
    "big"
};

// const Json = struct { fn writeAll(writer) ... }
const fn json_write_all(out: &mut [u8], len: &mut usize) {
    push_str(out, len, "{\"schema\":1,\"abi\":{\"target\":");
    push_json_string(out, len, ABI_TARGET);
    push_str(out, len, ",\"os\":");
    push_json_string(out, len, ABI_OS);
    push_str(out, len, ",\"environment\":");
    push_json_string(out, len, ABI_ENVIRONMENT);
    push_str(out, len, ",\"pointer_size\":");
    push_usize(out, len, size_of::<*const std::ffi::c_void>());
    push_str(out, len, ",\"usize_size\":");
    push_usize(out, len, size_of::<usize>());
    push_str(out, len, ",\"max_alignment\":");
    push_usize(out, len, rustty_lib::c_abi::MAX_ALIGNMENT as usize);
    push_str(out, len, ",\"endian\":");
    push_json_string(out, len, ABI_ENDIAN);
    push_str(out, len, "},\"library_version\":");
    // try jws.write(build_options.version_string);
    push_json_string(out, len, rustty_vt::options::VERSION_STRING);
    push_str(out, len, ",\"commit\":");
    // if (build_options.version_build) |commit| write(commit) else write(null);
    match rustty_vt::options::VERSION_BUILD {
        Some(commit) => push_json_string(out, len, commit),
        None => push_str(out, len, "null"),
    }
    // "dirty": null
    push_str(out, len, ",\"dirty\":null,\"types\":{");
    let mut i = 0;
    while i < DECLS.len() {
        if i > 0 {
            push_str(out, len, ",");
        }
        push_json_string(out, len, DECLS[i].name());
        push_str(out, len, ":");
        write_type(out, len, &DECLS[i]);
        i += 1;
    }
    push_str(out, len, "}}");
}

/// Scratch capacity for the counting pass. Only ever alive during const
/// evaluation; the shipped `static` is exactly `JSON_LEN + 1` bytes.
const JSON_SCRATCH: usize = 128 * 1024;

// PORT NOTE: the reference's two-pass comptime build —
//   var counter: std.Io.Writer.Discarding = .init(&.{});
//   Json.writeAll(&counter.writer); var buf: [counter.count:0]u8 = undefined;
//   Json.writeAll(&writer);
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

/// The comptime-generated NUL-terminated JSON string of the manifest.
// pub const json: [:0]const u8 = json: { ... };
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

// pub fn get_json() callconv(lib.calling_conv) [*:0]const u8 {
//     return json.ptr;
// }
pub fn get_json() -> *const c_char {
    // return json.ptr;
    JSON.as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference parses `json` with `std.json.parseFromSlice`;
    // the tests below parse the static document (sans the trailing NUL) into a
    // minimal tree and assert the same facts. A tiny purpose-built parser is
    // used (no serde dependency in this leaf); it handles the object/number/
    // string/null subset this JSON emits.
    pub(super) fn parsed_json() -> JsonValue {
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

    pub(super) fn manifest_types(root: &JsonValue) -> &[(String, JsonValue)] {
        get(root.as_object(), "types").as_object()
    }

    // test "manifest parses and is versioned" {
    #[test]
    fn manifest_parses_and_is_versioned() {
        let root = parsed_json();
        let root_obj = root.as_object();
        // try std.testing.expectEqual(@as(i64, 1), root.get("schema").?.integer);
        assert_eq!(1i64, get(root_obj, "schema").as_int());
        // try std.testing.expect(root.contains("abi"));
        assert!(contains(root_obj, "abi"));
        // try std.testing.expectEqual(@as(i64, c_abi.max_alignment),
        //     root.get("abi").?.object.get("max_alignment").?.integer);
        assert_eq!(
            rustty_lib::c_abi::MAX_ALIGNMENT as i64,
            get(get(root_obj, "abi").as_object(), "max_alignment").as_int()
        );
        // try std.testing.expect(root.contains("library_version"));
        assert!(contains(root_obj, "library_version"));
        let types = manifest_types(&root);
        // try std.testing.expectEqual(type_decls.len, manifest_types.count());
        assert_eq!(DECLS.len(), types.len());
        // inline for (type_decls) |decl| expect(manifest_types.contains(decl.name));
        for decl in DECLS {
            assert!(contains(types, decl.name()), "missing {}", decl.name());
        }

        // const cursor_fields = manifest_types.get("GhosttyRenderStateCursor").?.object
        //     .get("fields").?.object;
        let cursor_fields =
            get(get(types, "RusttyRenderStateCursor").as_object(), "fields").as_object();
        // try std.testing.expect(cursor_fields.contains("size")); ...
        assert!(contains(cursor_fields, "size"));
        assert!(contains(cursor_fields, "viewport_has_value"));
        assert!(contains(cursor_fields, "viewport_x"));
        assert!(contains(cursor_fields, "viewport_y"));
        assert!(contains(cursor_fields, "wide_tail"));
        assert!(contains(cursor_fields, "visible"));
        assert!(contains(cursor_fields, "blinking"));
        assert!(contains(cursor_fields, "password_input"));
        assert!(contains(cursor_fields, "visual_style"));
    }

    // test "manifest describes enums, arrays, and tagged unions" {
    #[test]
    fn manifest_describes_enums_arrays_and_tagged_unions() {
        let root = parsed_json();
        let types = manifest_types(&root);

        // const data = manifest_types.get("GhosttyRenderStateData").?.object;
        let data = get(types, "RusttyRenderStateData").as_object();
        // try std.testing.expectEqualStrings("enum", data.get("kind").?.string);
        assert_eq!("enum", get(data, "kind").as_str());
        // try std.testing.expectEqual(@as(i64, @intFromEnum(render.Data.dirty)),
        //     data.get("values").?.object.get("DIRTY").?.integer);
        assert_eq!(
            crate::render::Data::Dirty as i64,
            get(get(data, "values").as_object(), "DIRTY").as_int()
        );

        // const colors = manifest_types.get("GhosttyRenderStateColors").?.object;
        let colors = get(types, "RusttyRenderStateColors").as_object();
        let palette = get(get(colors, "fields").as_object(), "palette").as_object();
        // try std.testing.expectEqualStrings("array", palette.get("type").?.string);
        assert_eq!("array", get(palette, "type").as_str());
        // try std.testing.expectEqualStrings("GhosttyColorRgb", palette.get("elem").?.string);
        assert_eq!("RusttyColorRgb", get(palette, "elem").as_str());
        // try std.testing.expectEqual(@as(i64, 256), palette.get("count").?.integer);
        assert_eq!(256i64, get(palette, "count").as_int());

        // const style_color = manifest_types.get("GhosttyStyleColor").?.object;
        let style_color = get(types, "RusttyStyleColor").as_object();
        let value = get(get(style_color, "fields").as_object(), "value").as_object();
        // try std.testing.expectEqualStrings("GhosttyStyleColorValue", value.get("type").?.string);
        assert_eq!("RusttyStyleColorValue", get(value, "type").as_str());
        // try std.testing.expectEqualStrings("tag", value.get("tag").?.string);
        assert_eq!("tag", get(value, "tag").as_str());
        // try std.testing.expectEqualStrings("palette",
        //     value.get("arms").?.object.get("PALETTE").?.string);
        assert_eq!(
            "palette",
            get(get(value, "arms").as_object(), "PALETTE").as_str()
        );
        // try std.testing.expect(value.get("arms").?.object.get("NONE").? == .null);
        assert!(get(get(value, "arms").as_object(), "NONE").is_null());
    }

    // test "manifest describes the complete packed cell layout" {
    #[test]
    fn manifest_describes_the_complete_packed_cell_layout() {
        use rustty_vt::page::cell_c_layout::{CLAYOUT, CONTENT, CONTENT_CODEPOINT};
        use rustty_vt::page::cell_c_layout::{CONTENT_COLOR_PALETTE, CONTENT_COLOR_RGB};

        let root = parsed_json();
        let types = manifest_types(&root);
        let descriptor = get(types, "RusttyCell").as_object();

        // try std.testing.expectEqualStrings("packed", descriptor.get("kind").?.string);
        assert_eq!("packed", get(descriptor, "kind").as_str());
        // try std.testing.expectEqual(@as(i64, @sizeOf(page.Cell.CLayout.Zig)),
        //     descriptor.get("size").?.integer);
        assert_eq!(
            size_of::<rustty_vt::page::Cell>() as i64,
            get(descriptor, "size").as_int()
        );
        // try std.testing.expectEqualStrings("u64", descriptor.get("underlying").?.string);
        assert_eq!("u64", get(descriptor, "underlying").as_str());

        // inline for (@typeInfo(page.Cell.CLayout.Zig).@"struct".fields) |field| ...
        let bits = get(descriptor, "bits").as_object();
        for (i, field) in CLAYOUT.fields.iter().enumerate() {
            if let Some(public_name) = CLAYOUT.field_name(i) {
                let bit = get(bits, public_name).as_object();
                // expectEqual(@bitOffsetOf(...), bit.get("lsb").?.integer);
                assert_eq!(field.bit_offset as i64, get(bit, "lsb").as_int());
                // expectEqual(@bitSizeOf(field.type), bit.get("width").?.integer);
                assert_eq!(field.bit_width as i64, get(bit, "width").as_int());
            } else {
                // try std.testing.expect(!bits.contains(field.name));
                assert!(!contains(bits, field.zig_name));
            }
        }

        // const content = bits.get("content").?.object;
        let content = get(bits, "content").as_object();
        assert_eq!("union", get(content, "kind").as_str());
        assert_eq!("content_tag", get(content, "tag").as_str());
        let arms = get(content, "arms").as_object();

        // const Content = @FieldType(page.Cell.CLayout.Zig, "content"); ...
        let codepoint = get(arms, "CODEPOINT").as_object();
        let grapheme = get(arms, "CODEPOINT_GRAPHEME").as_object();
        expect_packed_arm_field(codepoint, "codepoint", &CONTENT_CODEPOINT, "data", "u21");
        expect_packed_arm_field(grapheme, "codepoint", &CONTENT_CODEPOINT, "data", "u21");
        expect_packed_arm_field(
            get(arms, "BG_COLOR_PALETTE").as_object(),
            "index",
            &CONTENT_COLOR_PALETTE,
            "data",
            "RusttyColorPaletteIndex",
        );

        // const rgb = arms.get("BG_COLOR_RGB").?.object;
        let rgb = get(arms, "BG_COLOR_RGB").as_object();
        // inline for (@typeInfo(Rgb).@"struct".fields) |field|
        //     try expectPackedArmField(rgb, field.name, Rgb, field.name, "u8");
        for (i, field) in CONTENT_COLOR_RGB.fields.iter().enumerate() {
            let _ = i;
            expect_packed_arm_field(
                rgb,
                field.zig_name,
                &CONTENT_COLOR_RGB,
                field.zig_name,
                "u8",
            );
        }
        // (CONTENT is referenced so the descriptor identity is proven used.)
        assert_eq!("content", CONTENT.union_field);
    }

    // test "manifest packed cell layouts decode real values" {
    #[test]
    fn manifest_packed_cell_layouts_decode_real_values() {
        use rustty_vt::page::{Cell, CellRgb, ContentTag};

        let root = parsed_json();
        let types = manifest_types(&root);
        let cell_bits = get(get(types, "RusttyCell").as_object(), "bits").as_object();

        // var codepoint = page.Cell.CLayout.Zig.init('A');
        let mut codepoint = Cell::init('A' as u32);
        expect_decoded_cell_content(
            types,
            cell_bits,
            codepoint.to_bits(),
            "codepoint",
            'A' as u64,
        );

        // codepoint.content_tag = .codepoint_grapheme;
        codepoint.set_content_tag(ContentTag::CodepointGrapheme);
        expect_decoded_cell_content(
            types,
            cell_bits,
            codepoint.to_bits(),
            "codepoint",
            'A' as u64,
        );

        // var palette: page.Cell.CLayout.Zig = @bitCast(@as(u64, 0));
        // palette.content_tag = .bg_color_palette;
        // palette.content = .{ .color_palette = .{ .data = 173 } };
        let mut palette = Cell::from_bits(0);
        palette.set_content_tag(ContentTag::BgColorPalette);
        palette.set_content_color_palette(173);
        expect_decoded_cell_content(types, cell_bits, palette.to_bits(), "index", 173);

        // var rgb: page.Cell.CLayout.Zig = @bitCast(@as(u64, 0));
        // rgb.content_tag = .bg_color_rgb;
        // rgb.content = .{ .color_rgb = .{ .r = 0x12, .g = 0x34, .b = 0x56 } };
        let mut rgb = Cell::from_bits(0);
        rgb.set_content_tag(ContentTag::BgColorRgb);
        rgb.set_content_color_rgb(CellRgb {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        });
        let rgb_arm = active_cell_content_arm(types, cell_bits, rgb.to_bits());
        let content = extract_manifest_bits(rgb.to_bits(), get(cell_bits, "content").as_object());
        assert_eq!(
            0x12u64,
            extract_manifest_bits(
                content,
                get(get(rgb_arm, "bits").as_object(), "r").as_object()
            )
        );
        assert_eq!(
            0x34u64,
            extract_manifest_bits(
                content,
                get(get(rgb_arm, "bits").as_object(), "g").as_object()
            )
        );
        assert_eq!(
            0x56u64,
            extract_manifest_bits(
                content,
                get(get(rgb_arm, "bits").as_object(), "b").as_object()
            )
        );
    }

    // test "manifest uses public enum names" {
    #[test]
    fn manifest_uses_public_enum_names() {
        let root = parsed_json();
        let types = manifest_types(&root);

        let cell_content =
            get(get(types, "RusttyCellContentTag").as_object(), "values").as_object();
        // try std.testing.expect(cell_content.contains("TAG_MAX_VALUE"));
        assert!(contains(cell_content, "TAG_MAX_VALUE"));
        // try std.testing.expect(!cell_content.contains("MAX_VALUE"));
        assert!(!contains(cell_content, "MAX_VALUE"));

        let sgr_values = get(get(types, "RusttySgrAttributeTag").as_object(), "values").as_object();
        // try std.testing.expectEqual(@as(i64, 9), sgr_values.get("UNDERLINE_COLOR_256").?.integer);
        assert_eq!(9i64, get(sgr_values, "UNDERLINE_COLOR_256").as_int());
        // try std.testing.expectEqual(@as(i64, 23), sgr_values.get("BG_8").?.integer);
        assert_eq!(23i64, get(sgr_values, "BG_8").as_int());
        // try std.testing.expect(!sgr_values.contains("256_UNDERLINE_COLOR"));
        assert!(!contains(sgr_values, "256_UNDERLINE_COLOR"));

        let sgr_arms = get(
            get(
                get(get(types, "RusttySgrAttribute").as_object(), "fields").as_object(),
                "value",
            )
            .as_object(),
            "arms",
        )
        .as_object();
        // try std.testing.expectEqualStrings("underline_color_256",
        //     sgr_arms.get("UNDERLINE_COLOR_256").?.string);
        assert_eq!(
            "underline_color_256",
            get(sgr_arms, "UNDERLINE_COLOR_256").as_str()
        );
        // try std.testing.expectEqualStrings("bg_8", sgr_arms.get("BG_8").?.string);
        assert_eq!("bg_8", get(sgr_arms, "BG_8").as_str());

        let terminal_values =
            get(get(types, "RusttyTerminalOption").as_object(), "values").as_object();
        // try std.testing.expectEqual(@as(i64, 6), terminal_values.get("SIZE").?.integer);
        assert_eq!(6i64, get(terminal_values, "SIZE").as_int());
        // try std.testing.expect(!terminal_values.contains("SIZE_CB"));
        assert!(!contains(terminal_values, "SIZE_CB"));

        let osc_values = get(get(types, "RusttyOscCommandType").as_object(), "values").as_object();
        // try std.testing.expectEqual(@as(i64, 22), osc_values.get("KITTY_TEXT_SIZING").?.integer);
        assert_eq!(22i64, get(osc_values, "KITTY_TEXT_SIZING").as_int());
        assert_eq!(23i64, get(osc_values, "KITTY_CLIPBOARD_PROTOCOL").as_int());
        assert_eq!(24i64, get(osc_values, "KITTY_DND_PROTOCOL").as_int());
        assert_eq!(25i64, get(osc_values, "CONTEXT_SIGNAL").as_int());
        // try std.testing.expectEqual(@as(i64, 26), osc_values.get("KITTY_DESKTOP_NOTIFICATION").?.integer);
        assert_eq!(
            26i64,
            get(osc_values, "KITTY_DESKTOP_NOTIFICATION").as_int()
        );
        // try std.testing.expect(osc_values.contains("TYPE_MAX_VALUE"));
        assert!(contains(osc_values, "TYPE_MAX_VALUE"));
        // try std.testing.expect(!osc_values.contains("MAX_VALUE"));
        assert!(!contains(osc_values, "MAX_VALUE"));

        let key_values = get(get(types, "RusttyKey").as_object(), "values").as_object();
        // try std.testing.expect(key_values.contains("A"));
        assert!(contains(key_values, "A"));
        // try std.testing.expect(!key_values.contains("KEY_A"));
        assert!(!contains(key_values, "KEY_A"));

        let mode_values =
            get(get(types, "RusttyModeReportState").as_object(), "values").as_object();
        // try std.testing.expectEqual(@as(i64, 0), mode_values.get("NOT_RECOGNIZED").?.integer);
        assert_eq!(0i64, get(mode_values, "NOT_RECOGNIZED").as_int());
        // try std.testing.expectEqual(@as(i64, 4), mode_values.get("PERMANENTLY_RESET").?.integer);
        assert_eq!(4i64, get(mode_values, "PERMANENTLY_RESET").as_int());
    }

    // test "manifest named references resolve" {
    #[test]
    fn manifest_named_references_resolve() {
        let root = parsed_json();
        let types = manifest_types(&root);

        // var type_iterator = manifest_types.iterator();
        for (_, descriptor_value) in types {
            let descriptor = descriptor_value.as_object();
            // if (descriptor.get("bits")) |bits_value| ...
            if let Some(bits_value) = find(descriptor, "bits") {
                expect_manifest_bits_valid(
                    types,
                    bits_value.as_object(),
                    (get(descriptor, "size").as_int() * 8) as usize,
                );
            }
            let fields_value = match find(descriptor, "fields") {
                Some(v) => v,
                None => continue,
            };
            for (_, field_value) in fields_value.as_object() {
                let field = field_value.as_object();
                let field_type = get(field, "type").as_str();
                // if (!Json.isBuiltinType(field_type))
                //     try std.testing.expect(manifest_types.contains(field_type));
                if !is_builtin_type(field_type) {
                    assert!(contains(types, field_type), "unresolved {field_type}");
                }

                if let Some(elem_value) = find(field, "elem") {
                    let elem = elem_value.as_str();
                    if !is_builtin_type(elem) {
                        assert!(contains(types, elem), "unresolved elem {elem}");
                    }
                }

                if let Some(arms_value) = find(field, "arms") {
                    // const union_descriptor = manifest_types.get(field_type).?.object;
                    let union_descriptor = get(types, field_type).as_object();
                    let union_fields = get(union_descriptor, "fields").as_object();
                    let tag_field_name = get(field, "tag").as_str();
                    let tag_type_name = get(
                        get(fields_value.as_object(), tag_field_name).as_object(),
                        "type",
                    )
                    .as_str();
                    let tag_values =
                        get(get(types, tag_type_name).as_object(), "values").as_object();
                    for (arm_key, arm_value) in arms_value.as_object() {
                        // try std.testing.expect(tag_values.contains(arm_entry.key_ptr.*));
                        assert!(contains(tag_values, arm_key));
                        if arm_value.is_null() {
                            continue;
                        }
                        // try std.testing.expect(union_fields.contains(...string));
                        assert!(contains(union_fields, arm_value.as_str()));
                    }
                }
            }
        }
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

    // fn expectPackedArmField(arm, manifest_name, comptime T, comptime
    //     zig_name, expected_type) !void { ... }
    fn expect_packed_arm_field(
        arm: &[(String, JsonValue)],
        manifest_name: &str,
        layout: &rustty_lib::packed::PackedLayout,
        zig_name: &str,
        expected_type: &str,
    ) {
        // try std.testing.expectEqualStrings("packed", arm.get("kind").?.string);
        assert_eq!("packed", get(arm, "kind").as_str());
        // try std.testing.expectEqual(@as(i64, @bitSizeOf(T)), arm.get("width").?.integer);
        assert_eq!(layout.bit_size as i64, get(arm, "width").as_int());
        let bit = get(get(arm, "bits").as_object(), manifest_name).as_object();
        let index = layout.field_index(zig_name);
        // try std.testing.expectEqual(@as(i64, @bitOffsetOf(T, zig_name)), bit.get("lsb").?.integer);
        assert_eq!(layout.bit_offset(index) as i64, get(bit, "lsb").as_int());
        // try std.testing.expectEqual(@as(i64, @bitSizeOf(@FieldType(T, zig_name))), ...);
        assert_eq!(layout.bit_width(index) as i64, get(bit, "width").as_int());
        // try std.testing.expectEqualStrings(expected_type, bit.get("type").?.string);
        assert_eq!(expected_type, get(bit, "type").as_str());
    }

    // fn extractManifestBits(value: u64, bit: std.json.ObjectMap) u64 { ... }
    fn extract_manifest_bits(value: u64, bit: &[(String, JsonValue)]) -> u64 {
        let lsb = get(bit, "lsb").as_int() as u32;
        let width = get(bit, "width").as_int() as u32;
        let mask = if width == 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        (value >> lsb) & mask
    }

    // fn activeCellContentArm(manifest_types, cell_bits, raw: u64) ... { ... }
    fn active_cell_content_arm<'a>(
        types: &'a [(String, JsonValue)],
        cell_bits: &'a [(String, JsonValue)],
        raw: u64,
    ) -> &'a [(String, JsonValue)] {
        let content_tag = get(cell_bits, "content_tag").as_object();
        let value = extract_manifest_bits(raw, content_tag);
        let enum_values = get(
            get(types, get(content_tag, "type").as_str()).as_object(),
            "values",
        )
        .as_object();
        for (key, entry) in enum_values {
            if entry.as_int() == value as i64 {
                return get(
                    get(get(cell_bits, "content").as_object(), "arms").as_object(),
                    key,
                )
                .as_object();
            }
        }
        unreachable!()
    }

    // fn expectDecodedCellContent(manifest_types, cell_bits, raw, field_name,
    //     expected) !void { ... }
    fn expect_decoded_cell_content(
        types: &[(String, JsonValue)],
        cell_bits: &[(String, JsonValue)],
        raw: u64,
        field_name: &str,
        expected: u64,
    ) {
        let arm = active_cell_content_arm(types, cell_bits, raw);
        let content = extract_manifest_bits(raw, get(cell_bits, "content").as_object());
        assert_eq!(
            expected,
            extract_manifest_bits(
                content,
                get(get(arm, "bits").as_object(), field_name).as_object()
            )
        );
    }

    // fn expectManifestBitsValid(manifest_types, bits, container_width) !void
    fn expect_manifest_bits_valid(
        types: &[(String, JsonValue)],
        bits: &[(String, JsonValue)],
        container_width: usize,
    ) {
        for (_, bit_value) in bits {
            let bit = bit_value.as_object();
            let lsb = get(bit, "lsb").as_int() as usize;
            let width = get(bit, "width").as_int() as usize;
            // try std.testing.expect(lsb + width <= container_width);
            assert!(lsb + width <= container_width);

            if let Some(type_value) = find(bit, "type") {
                let type_name = type_value.as_str();
                assert!(is_builtin_type(type_name) || contains(types, type_name));
                continue;
            }

            let kind = get(bit, "kind").as_str();
            if kind == "packed" {
                expect_manifest_bits_valid(types, get(bit, "bits").as_object(), width);
                continue;
            }

            // try std.testing.expectEqualStrings("union", kind);
            assert_eq!("union", kind);
            let tag_name = get(bit, "tag").as_str();
            let tag = get(bits, tag_name).as_object();
            let enum_values =
                get(get(types, get(tag, "type").as_str()).as_object(), "values").as_object();
            for (arm_key, arm_value) in get(bit, "arms").as_object() {
                assert!(contains(enum_values, arm_key));
                if arm_value.is_null() {
                    continue;
                }
                let arm = arm_value.as_object();
                let arm_width = get(arm, "width").as_int() as usize;
                // try std.testing.expect(arm_width <= width);
                assert!(arm_width <= width);
                expect_manifest_bits_valid(types, get(arm, "bits").as_object(), arm_width);
            }
        }
    }

    // --- tiny JSON value + parser (test-only) ---
    fn find<'a>(obj: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
        obj.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub(super) fn get<'a>(obj: &'a [(String, JsonValue)], key: &str) -> &'a JsonValue {
        match find(obj, key) {
            Some(v) => v,
            None => panic!("missing key {key}"),
        }
    }

    fn contains(obj: &[(String, JsonValue)], key: &str) -> bool {
        find(obj, key).is_some()
    }

    pub(super) enum JsonValue {
        Object(Vec<(String, JsonValue)>),
        Int(i64),
        Str(String),
        Null,
    }

    impl JsonValue {
        pub(super) fn as_object(&self) -> &[(String, JsonValue)] {
            match self {
                JsonValue::Object(o) => o,
                _ => panic!("not an object"),
            }
        }
        pub(super) fn as_int(&self) -> i64 {
            match self {
                JsonValue::Int(n) => *n,
                _ => panic!("not an int"),
            }
        }
        pub(super) fn as_str(&self) -> &str {
            match self {
                JsonValue::Str(s) => s,
                _ => panic!("not a string"),
            }
        }
        fn is_null(&self) -> bool {
            matches!(self, JsonValue::Null)
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
                b'n' => {
                    assert_eq!(&self.b[self.i..self.i + 4], b"null");
                    self.i += 4;
                    JsonValue::Null
                }
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
            // "true"/"false" only appear as pointer `const` values.
            if self.i == start {
                if self.b.len() - self.i >= 4 && &self.b[self.i..self.i + 4] == b"true" {
                    self.i += 4;
                    return JsonValue::Int(1);
                }
                if self.b.len() - self.i >= 5 && &self.b[self.i..self.i + 5] == b"false" {
                    self.i += 5;
                    return JsonValue::Int(0);
                }
                panic!("unexpected JSON token");
            }
            let n: i64 = std::str::from_utf8(&self.b[start..self.i])
                .unwrap()
                .parse()
                .unwrap();
            JsonValue::Int(n)
        }
    }
}

/// The manifest's enum tables are HAND-LISTED; the reference's are REFLECTED.
///
/// PORT NOTE: no reference analogue, and that is the whole point. The oracle
/// writes `.initEnum("GhosttyTerminalOption", terminal.Option, "GHOSTTY_…")`
/// and `inline for (@typeInfo(decl.T).@"enum".fields)` enumerates the Zig enum,
/// so the oracle's manifest CANNOT omit a variant. Rust has no such reflection,
/// so [`DECLS`] carries a written-out `enum_values!` list per enum — and four of
/// those lists had silently fallen behind their own Rust enum (`S182-F1`..`F4`:
/// `RUSTTY_REJECTED`, `RUSTTY_SYS_OPT_RANDOM_SECURE`,
/// `RUSTTY_TERMINAL_OPT_CLIPBOARD_READ`/`_CLIPBOARD_WRITE_MAX_BYTES`,
/// `RUSTTY_TERMINAL_DATA_CLIPBOARD_WRITE_MAX_BYTES`).
///
/// Nothing caught them: `G-ABISCHEMA` validates the manifest's SHAPE against a
/// JSON Schema and has no idea which variants should exist, and
/// `check-vt-enum-discriminants.mjs` compares the HEADER to the Rust enum, never
/// to the manifest. This module is the missing third edge — manifest vs the
/// shipped header — so the next omission fails a test instead of shipping.
#[cfg(test)]
mod manifest_header_sync_tests {
    use super::tests::{get, manifest_types, parsed_json};
    use rustty_lib::c_header::parse_header_enum;
    use std::collections::BTreeMap;

    /// Manifest enums with no counterpart in the curated header family. Each
    /// entry must say why. A stale entry (one that HAS acquired a header enum)
    /// is a FAIL — a rotting allowlist is how a check stops binding.
    const UNDECLARED_IN_HEADERS: &[(&str, &str)] = &[];

    /// Read every curated header's text.
    fn header_text() -> String {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../include/rustty");
        let mut out = String::new();
        let mut stack = vec![std::path::PathBuf::from(dir)];
        // `include/rustty/vt.h` sits beside the `rustty/` directory's contents.
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).expect("curated header dir") {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().and_then(|e| e.to_str()) == Some("h") {
                    out.push_str(&std::fs::read_to_string(&p).unwrap());
                    out.push('\n');
                }
            }
        }
        // The sentinel enumerator is `<PREFIX>_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`;
        // substitute the macro so the strict parser sees an integer.
        out.replace("RUSTTY_ENUM_MAX_VALUE", "2147483647")
    }

    /// The manifest's enum declarations as `name -> (prefix, {FULL_C_NAME: value})`,
    /// with the `*_MAX_VALUE` sentinel row dropped (it is the manifest's own
    /// bookkeeping, emitted from `sentinel_suffix`, not an enum field).
    #[allow(clippy::type_complexity)]
    fn manifest_enums() -> Vec<(String, BTreeMap<String, i64>)> {
        let root = parsed_json();
        let types = manifest_types(&root);
        let mut out = Vec::new();
        for (name, v) in types {
            let obj = v.as_object();
            if get(obj, "kind").as_str() != "enum" {
                continue;
            }
            let prefix = get(obj, "prefix").as_str();
            let mut rows = BTreeMap::new();
            for (k, val) in get(obj, "values").as_object().iter() {
                let value = val.as_int();
                if value == i64::from(std::ffi::c_int::MAX) && k.ends_with("MAX_VALUE") {
                    continue;
                }
                rows.insert(format!("{}{}", prefix, k), value);
            }
            out.push((name.clone(), rows));
        }
        out
    }

    /// Every manifest enum declares EXACTLY the enumerators the shipped header
    /// declares, with the same values.
    #[test]
    fn manifest_enums_match_the_shipped_headers() {
        let src = header_text();
        let enums = manifest_enums();
        assert!(
            enums.len() >= 20,
            "found only {} manifest enums — the walk is not finding them",
            enums.len()
        );

        let mut checked = 0usize;
        let mut failures: Vec<String> = Vec::new();
        for (name, rows) in &enums {
            let Ok(header) = parse_header_enum(&src, name) else {
                if !UNDECLARED_IN_HEADERS.iter().any(|(n, _)| n == name) {
                    failures.push(format!(
                        "{}: manifest declares this enum but no curated header does, \
                         and it is not in UNDECLARED_IN_HEADERS",
                        name
                    ));
                }
                continue;
            };
            checked += 1;
            let header: BTreeMap<String, i64> = header
                .into_iter()
                .filter(|(n, _)| !n.ends_with("_MAX_VALUE"))
                .collect();
            for (n, v) in &header {
                match rows.get(n) {
                    None => failures.push(format!(
                        "{}: header declares {} = {} and the manifest OMITS it",
                        name, n, v
                    )),
                    Some(mv) if mv != v => failures.push(format!(
                        "{}: {} is {} in the header but {} in the manifest",
                        name, n, v, mv
                    )),
                    Some(_) => {}
                }
            }
            for (n, v) in rows {
                if !header.contains_key(n) {
                    failures.push(format!(
                        "{}: the manifest declares {} = {} and no header does",
                        name, n, v
                    ));
                }
            }
        }

        // Stale-allowlist rot.
        for (n, _) in UNDECLARED_IN_HEADERS {
            if parse_header_enum(&src, n).is_ok() {
                failures.push(format!(
                    "{}: listed in UNDECLARED_IN_HEADERS but the headers DO declare it now. \
                     Delete the row.",
                    n
                ));
            }
        }

        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
        assert!(
            checked >= 20,
            "only {} manifest enums were actually compared against a header",
            checked
        );
    }
}
