//! rustty-vt-c render
//!
//! PORTS: ghostty src/terminal/c/render.zig
//!
//! PORT STATUS: FILLED (Stage B v4). Every type, `pub fn`/helper body, and all
//! `test {}` blocks are ported assertion-for-assertion.
//
// PORT NOTE: the reference `RenderStateWrapper`/`RowIteratorWrapper`/
// `RowCellsWrapper` mirror the `std.MultiArrayList` struct-of-arrays layout of
// `renderpkg.RenderState`: the iterator caches per-field slices
// (`row_data.items(.raw)`, `.cells`, `.selection`, `.dirty`). rustty-vt's
// `RenderState` is the faithful `Vec<Row>` port (each `Row` owns its `raw`,
// `cells: Vec<Cell>`, `dirty`, `selection`); see render.rs's module PORT NOTE.
// So the rustty iterator wrappers cache a single raw slice pointer into
// `row_data` (and, for cells, into a `Row.cells`) plus the palette, and the
// reference's `it.raws[y]` / `it.cells[y]` / `it.selection[y]` / `it.dirty[y]`
// become `row_data[y].raw` / `.cells` / `.selection` / `.dirty`. This is a
// representation-only adaptation; no behavior, assertion, or comment is dropped.

use crate::result::Result;
use crate::row;
use crate::style as style_c;
use crate::terminal as terminal_c;
use std::ffi::c_void;
use std::os::raw::c_int;

// const std = @import("std");
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const lib = @import("../lib.zig");
// const CAllocator = lib.alloc.Allocator;
// const colorpkg = @import("../color.zig");
// const cursorpkg = @import("../cursor.zig");
// const page = @import("../page.zig");
// const size = @import("../size.zig");
// const Style = @import("../style.zig").Style;
// const terminal_c = @import("terminal.zig");
// const ZigTerminal = @import("../Terminal.zig");
// const renderpkg = @import("../render.zig");
// const Result = @import("result.zig").Result;
// const row = @import("row.zig");
// const style_c = @import("style.zig");
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_lib::types::Buffer;
use rustty_vt::color::{Palette, RGB};
use rustty_vt::cursor;
use rustty_vt::page;
use rustty_vt::render::{
    CellList, Dirty as RenderDirty, RenderState as ZigRenderState, Row as RenderRow,
};
use rustty_vt::size;
use rustty_vt::style::ColorTag;

// const log = std.log.scoped(.render_state_c);
//
// PORT NOTE (W34-B02): rustty's scoped-log sink is `sys::log_fn` (the port of
// `terminal/c/sys.zig`'s `logFn`), which is silent until a C caller installs a
// log callback via `rustty_sys_set(RUSTTY_SYS_OPT_LOG, ...)`. `LOG_SCOPE` is
// the `@tagName` of the reference's scope, which `log_fn` forwards to the
// callback as the scope text.
pub(crate) const LOG_SCOPE: &str = "render_state_c";

// const RenderStateWrapper = struct {
//     alloc: std.mem.Allocator,
//     state: renderpkg.RenderState = .empty,
// };
//
// PORT NOTE (W12-01, internal — NOT C-ABI): the wrapper STORAGE is allocated
// through the C-resolved allocator (`alloc.create`, see `new_`) and handed
// across the C ABI as a raw `*mut RenderStateWrapper`; `free` returns it via
// `alloc.destroy` — the caller allocator owns the lifecycle exactly like the
// reference. Tier-2 residual: the `renderpkg.RenderState` CONTENTS still grow
// through rustty-vt's own `allocator_api2::Global` (DEFERRALS.md
// "F-008/W12-01 Tier 2").
pub struct RenderStateWrapper {
    // PORT NOTE: the reference `alloc: std.mem.Allocator` is the C-resolved
    // allocator; it owns this wrapper's storage (used by `free`). Tier-2
    // residual: the `deinit`/`update` calls on `state` still use Global.
    pub alloc: Box<dyn rustty_lib::allocator::ZigAllocator>,
    pub state: ZigRenderState,
}

// /// The "before the first element" position for the iterator wrappers
// /// below. Represented as a sentinel index rather than an optional because
// /// optional codegens into something that is less efficient than this.
// const position_none = std.math.maxInt(usize);
/// The "before the first element" position for the iterator wrappers
/// below. Represented as a sentinel index rather than an optional because
/// optional codegens into something that is less efficient than this.
pub const POSITION_NONE: usize = usize::MAX;

// const RowIteratorWrapper = struct {
//     alloc: std.mem.Allocator,
//
//     /// The current index (also y value) into the row list, or
//     /// `position_none` if iteration hasn't started. Always validate
//     /// against `raws.len` before use.
//     y: usize,
//
//     /// These are the raw pointers into the render state data.
//     raws: []const page.Row,
//     cells: []const std.MultiArrayList(renderpkg.RenderState.Cell),
//     selection: []const ?[2]size.CellCountInt,
//     dirty: []bool,
//
//     /// The global dirty state from the render state that populated this
//     /// iterator. This has the same borrowed lifetime as the row slices.
//     state_dirty: *const Dirty,
//
//     /// The color palette from the render state, needed to resolve
//     /// palette-indexed background colors on cells.
//     palette: *const colorpkg.Palette,
// };
//
// PORT NOTE: see the module note — the per-field slices are replaced by a single
// raw slice pointer into `row_data` (`rows`), indexed by `y`.
pub struct RowIteratorWrapper {
    pub alloc: Box<dyn rustty_lib::allocator::ZigAllocator>,

    /// The current index (also y value) into the row list, or
    /// `POSITION_NONE` if iteration hasn't started. Always validate
    /// against `raws.len` before use.
    pub y: usize,

    /// These are the raw pointers into the render state data.
    // PORT NOTE: `raws`/`cells`/`selection`/`dirty` collapse to a raw `*mut
    // [Row]` (ptr+len) since rustty-vt's `RenderState` owns rows as `Vec<Row>`.
    // MUT provenance is required: the reference MultiArrayList slices are
    // writable and `row_set(.dirty)` writes through this pointer — a
    // `*const`-derived (SharedReadOnly) tag cast to `*mut` still forbids the
    // write under Stacked Borrows (Miri-caught in `render_row_get_set_dirty`).
    //
    // SB (SOUNDNESS, 0d37f2d34 interleave): this pointer carries the provenance
    // of the `Vec<Row>` BUFFER root (`Vec::as_mut_ptr`), NOT of a transient
    // `&mut RenderStateWrapper`. `rustty_render_state_clean` mutates the same
    // rows between `get(.row_iterator)` and `row_iterator_next_dirty` (the
    // "existing iterators observe cleaning" contract); transient `&mut` slices
    // formed inside `clean` are children of the same buffer root, so they die
    // without invalidating this alias. A wrapper-level `&mut` reborrow anywhere
    // in this module WOULD invalidate `state_dirty` below — which is why every
    // state access in this module goes through raw place projections off the
    // handle (the rust-mut-args-strongly-protected discipline).
    pub rows: *mut [RenderRow],

    /// The global dirty state from the render state that populated this
    /// iterator. This has the same borrowed lifetime as the row slices.
    // SB: raw projection off the wrapper base pointer (`addr_of!`), so it stays
    // valid across the transient field-level borrows the exports create.
    pub state_dirty: *const RenderDirty,

    /// The color palette from the render state, needed to resolve
    /// palette-indexed background colors on cells.
    pub palette: *const Palette,
}

// const RowCellsWrapper = struct {
//     alloc: std.mem.Allocator,
//
//     /// The current index (also x value) into the cell list, or
//     /// `position_none` if iteration hasn't started. Always validate
//     /// against `raws.len` before use.
//     x: usize,
//
//     raws: []const page.Cell,
//     graphemes: []const []const u21,
//     styles: []const Style,
//     selection: ?[2]size.CellCountInt,
//
//     /// The color palette, needed to resolve palette-indexed background colors.
//     palette: *const colorpkg.Palette,
// };
//
// PORT NOTE (reshape): the reference `raws`/`graphemes`/`styles` are the three
// SoA columns of the row's `MultiArrayList(Cell)`; rustty's `RenderState` stores
// them in a `CellList` (SoA), so this holds a single `*const CellList` and reads
// `(*cells).raw()[x]` / `.style()[x]` / `.grapheme(x)` — the same projections.
pub struct RowCellsWrapper {
    pub alloc: Box<dyn rustty_lib::allocator::ZigAllocator>,

    /// The current index (also x value) into the cell list, or
    /// `POSITION_NONE` if iteration hasn't started. Always validate
    /// against `raws.len` before use.
    pub x: usize,

    pub cells: *const CellList,
    pub selection: Option<[size::CellCountInt; 2]>,

    /// The color palette, needed to resolve palette-indexed background colors.
    pub palette: *const Palette,
}

/// C: GhosttyRenderState -> RusttyRenderState (R2)
// pub const RenderState = ?*RenderStateWrapper;
pub type RenderState = *mut RenderStateWrapper;

/// C: GhosttyRenderStateRowIterator -> RusttyRenderStateRowIterator (R2)
// pub const RowIterator = ?*RowIteratorWrapper;
pub type RowIterator = *mut RowIteratorWrapper;

/// C: GhosttyRenderStateRowCells -> RusttyRenderStateRowCells (R2)
// pub const RowCells = ?*RowCellsWrapper;
pub type RowCells = *mut RowCellsWrapper;

/// C: GhosttyRenderStateDirty -> RusttyRenderStateDirty (R2)
// pub const Dirty = renderpkg.RenderState.Dirty;
//
// PORT NOTE (W34-B03): the reference's `renderpkg.RenderState.Dirty` is
// `lib.Enum(lib.target, &.{ "false", "partial", "full" })`, whose C backing is
// `c_int` (`src/lib/enum.zig`: `.c => c_int`), so `getTyped`/`setTyped` move a
// full `int` through the caller's payload pointer, and the published header
// declares `RusttyRenderStateDirty` with `RUSTTY_ENUM_MAX_VALUE` (int storage).
// rustty's INTERNAL `render::Dirty` is a plain Rust enum (1 byte), so aliasing
// it here made `get(.dirty)` write ONE byte into the C caller's 4-byte slot
// (leaving the high bytes stale) and `set(.dirty)` read only the low byte. This
// is the C-facing projection — int-sized, like `CursorVisualStyle` below — and
// the internal enum is converted at the boundary.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dirty {
    False = 0,
    Partial = 1,
    Full = 2,
}

// The C-ABI storage width the published header declares (`include/rustty/vt/
// render.h`: `RUSTTY_RENDER_STATE_DIRTY_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`).
const _: () = assert!(std::mem::size_of::<Dirty>() == std::mem::size_of::<c_int>());

impl Dirty {
    /// Narrow an inbound C integer to a `Dirty`, mirroring `std.meta.intToEnum`;
    /// `None` outside the declared range.
    ///
    /// D11 (P3-W6 T2), see `kitty_graphics::Data::from_c_int`: a `#[repr(C)]`
    /// enum materialized from out-of-range C memory is UB in every build mode,
    /// so `set(.dirty)` range-checks the payload before converting.
    pub fn from_c_int(v: c_int) -> Option<Dirty> {
        match v {
            0 => Some(Dirty::False),
            1 => Some(Dirty::Partial),
            2 => Some(Dirty::Full),
            _ => None,
        }
    }
}

impl From<RenderDirty> for Dirty {
    fn from(d: RenderDirty) -> Dirty {
        match d {
            RenderDirty::False => Dirty::False,
            RenderDirty::Partial => Dirty::Partial,
            RenderDirty::Full => Dirty::Full,
        }
    }
}

impl From<Dirty> for RenderDirty {
    fn from(d: Dirty) -> RenderDirty {
        match d {
            Dirty::False => RenderDirty::False,
            Dirty::Partial => RenderDirty::Partial,
            Dirty::Full => RenderDirty::Full,
        }
    }
}

/// C: GhosttyRenderStateRowSelection -> RusttyRenderStateRowSelection (R2)
// pub const RowSelection = extern struct {
//     size: usize = @sizeOf(RowSelection),
//     start_x: u16 = 0,
//     end_x: u16 = 0,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RowSelection {
    pub size: usize,
    pub start_x: u16,
    pub end_x: u16,
}

impl Default for RowSelection {
    fn default() -> RowSelection {
        RowSelection {
            size: std::mem::size_of::<RowSelection>(),
            start_x: 0,
            end_x: 0,
        }
    }
}

/// C: GhosttyRenderStateCursorVisualStyle -> RusttyRenderStateCursorVisualStyle (R2)
// pub const CursorVisualStyle = enum(c_int) { bar = 0, block = 1, underline = 2, block_hollow = 3 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorVisualStyle {
    Bar = 0,
    Block = 1,
    Underline = 2,
    BlockHollow = 3,
}

impl CursorVisualStyle {
    // pub fn fromCursorStyle(s: cursorpkg.Style) CursorVisualStyle {
    //     return switch (s) {
    //         .bar => .bar,
    //         .block => .block,
    //         .underline => .underline,
    //         .block_hollow => .block_hollow,
    //     };
    // }
    pub fn from_cursor_style(s: cursor::Style) -> CursorVisualStyle {
        match s {
            cursor::Style::Bar => CursorVisualStyle::Bar,
            cursor::Style::Block => CursorVisualStyle::Block,
            cursor::Style::Underline => CursorVisualStyle::Underline,
            cursor::Style::BlockHollow => CursorVisualStyle::BlockHollow,
        }
    }
}

/// C: GhosttyRenderStateCursor -> RusttyRenderStateCursor (R2)
// pub const Cursor = extern struct {
//     size: usize = @sizeOf(Cursor),
//     viewport_has_value: bool,
//     viewport_x: u16,
//     viewport_y: u16,
//     wide_tail: bool,
//     visible: bool,
//     blinking: bool,
//     password_input: bool,
//     visual_style: CursorVisualStyle,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Cursor {
    pub size: usize,
    pub viewport_has_value: bool,
    pub viewport_x: u16,
    pub viewport_y: u16,
    pub wide_tail: bool,
    pub visible: bool,
    pub blinking: bool,
    pub password_input: bool,
    pub visual_style: CursorVisualStyle,
}

/// C: GhosttyRenderStateData -> RusttyRenderStateData (R2)
// pub const Data = enum(c_int) { invalid = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Data {
    Invalid = 0,
    Cols = 1,
    Rows = 2,
    Dirty = 3,
    RowIterator = 4,
    ColorBackground = 5,
    ColorForeground = 6,
    ColorCursor = 7,
    ColorCursorHasValue = 8,
    ColorPalette = 9,
    CursorVisualStyle = 10,
    CursorVisible = 11,
    CursorBlinking = 12,
    CursorPasswordInput = 13,
    CursorViewportHasValue = 14,
    CursorViewportX = 15,
    CursorViewportY = 16,
    CursorViewportWideTail = 17,
    Cursor = 18,
    Colors = 19,
}

// D11 (P3-W6 T2): proves reading this value out of C memory as a `c_int` leaves
// the ABI unchanged (the header declares `const RusttyRenderStateData *keys`).
const _: () = assert!(std::mem::size_of::<Data>() == std::mem::size_of::<c_int>());

impl Data {
    /// Narrow an inbound C integer to a `Data`, mirroring `std.meta.intToEnum`;
    /// `None` outside the declared range. `Invalid = 0` is IN range and keeps
    /// its `.invalid_value` disposition inside `get`.
    // PORT NOTE: the reference's guard is
    // `std.enums.fromInt(Data, @intFromEnum(data))`; `Data` is a contiguous
    // `#[repr(C)]` enum here, so the range check IS `fromInt`.
    pub fn from_c_int(v: c_int) -> Option<Data> {
        // contiguous 0..=19
        if (0..=19).contains(&v) {
            // SAFETY: `v` is a valid `Data` discriminant (range-checked above).
            Some(unsafe { std::mem::transmute::<c_int, Data>(v) })
        } else {
            None
        }
    }
}

// Output type expected for querying the data of the given kind.
// PORT NOTE: `pub fn OutType(comptime self: Data) type`:
//     .invalid => void,
//     .cols, .rows => size.CellCountInt,
//     .dirty => Dirty,
//     .row_iterator => RowIterator,
//     .color_background, .color_foreground, .color_cursor => colorpkg.RGB.C,
//     .color_cursor_has_value => bool,
//     .color_palette => colorpkg.PaletteC,
//     .cursor_visual_style => CursorVisualStyle,
//     .cursor_visible, .cursor_blinking, .cursor_password_input => bool,
//     .cursor_viewport_has_value, .cursor_viewport_wide_tail => bool,
//     .cursor_viewport_x, .cursor_viewport_y => size.CellCountInt,
//     .cursor => Cursor,
//     .colors => Colors,
// In Rust the OutType mapping is preserved as the per-arm pointer cast in
// `get_typed`.

/// C: GhosttyRenderStateOption -> RusttyRenderStateOption (R2)
// pub const SetOption = enum(c_int) { dirty = 0 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetOption {
    Dirty = 0,
}

// Input type expected for setting the option.
// PORT NOTE: `pub fn InType(comptime self: SetOption) type`: `.dirty => Dirty`.

/// C: GhosttyRenderStateColors -> RusttyRenderStateColors (R2)
// pub const Colors = extern struct {
//     size: usize = @sizeOf(Colors),
//     background: colorpkg.RGB.C,
//     foreground: colorpkg.RGB.C,
//     cursor: colorpkg.RGB.C,
//     cursor_has_value: bool,
//     palette: colorpkg.PaletteC,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Colors {
    pub size: usize,
    pub background: RGB,
    pub foreground: RGB,
    pub cursor: RGB,
    pub cursor_has_value: bool,
    pub palette: [RGB; 256],
}

// PORT NOTE: the `Colors` struct descriptor used by `struct_sized_field_fits`
// (the reference `lib.structSizedFieldFits(Colors, out_size, "<field>")`).
// `RGB` is 3 bytes / align 1, `bool` 1/1, `usize` 8/8, `[RGB; 256]` 768/1. The
// `#[repr(C)]` layout is reproduced exactly so `offset_of`/`field_size` match.
fn colors_struct_type() -> rustty_lib::r#struct::StructType {
    use rustty_lib::r#struct::{FieldInfo, Layout, StructType};
    StructType {
        layout: Layout::Extern,
        is_tuple: false,
        fields: vec![
            FieldInfo {
                name: "size",
                size: std::mem::size_of::<usize>(),
                natural_align: std::mem::align_of::<usize>(),
                alignment: std::mem::align_of::<usize>(),
            },
            FieldInfo {
                name: "background",
                size: std::mem::size_of::<RGB>(),
                natural_align: std::mem::align_of::<RGB>(),
                alignment: std::mem::align_of::<RGB>(),
            },
            FieldInfo {
                name: "foreground",
                size: std::mem::size_of::<RGB>(),
                natural_align: std::mem::align_of::<RGB>(),
                alignment: std::mem::align_of::<RGB>(),
            },
            FieldInfo {
                name: "cursor",
                size: std::mem::size_of::<RGB>(),
                natural_align: std::mem::align_of::<RGB>(),
                alignment: std::mem::align_of::<RGB>(),
            },
            FieldInfo {
                name: "cursor_has_value",
                size: std::mem::size_of::<bool>(),
                natural_align: std::mem::align_of::<bool>(),
                alignment: std::mem::align_of::<bool>(),
            },
            FieldInfo {
                name: "palette",
                size: std::mem::size_of::<[RGB; 256]>(),
                natural_align: std::mem::align_of::<[RGB; 256]>(),
                alignment: std::mem::align_of::<[RGB; 256]>(),
            },
        ],
    }
}

// PORT NOTE: the `Cursor` struct descriptor used by `struct_sized_field_fits`
// (the reference `lib.structSizedFieldFits(Cursor, out_size, "<field>")`).
// `bool` 1/1, `u16` 2/2, `usize` 8/8, `CursorVisualStyle` is `c_int`-sized. The
// `#[repr(C)]` layout is reproduced exactly so `offset_of`/`field_size` match.
fn cursor_struct_type() -> rustty_lib::r#struct::StructType {
    use rustty_lib::r#struct::{FieldInfo, Layout, StructType};
    StructType {
        layout: Layout::Extern,
        is_tuple: false,
        fields: vec![
            FieldInfo {
                name: "size",
                size: std::mem::size_of::<usize>(),
                natural_align: std::mem::align_of::<usize>(),
                alignment: std::mem::align_of::<usize>(),
            },
            FieldInfo {
                name: "viewport_has_value",
                size: std::mem::size_of::<bool>(),
                natural_align: std::mem::align_of::<bool>(),
                alignment: std::mem::align_of::<bool>(),
            },
            FieldInfo {
                name: "viewport_x",
                size: std::mem::size_of::<u16>(),
                natural_align: std::mem::align_of::<u16>(),
                alignment: std::mem::align_of::<u16>(),
            },
            FieldInfo {
                name: "viewport_y",
                size: std::mem::size_of::<u16>(),
                natural_align: std::mem::align_of::<u16>(),
                alignment: std::mem::align_of::<u16>(),
            },
            FieldInfo {
                name: "wide_tail",
                size: std::mem::size_of::<bool>(),
                natural_align: std::mem::align_of::<bool>(),
                alignment: std::mem::align_of::<bool>(),
            },
            FieldInfo {
                name: "visible",
                size: std::mem::size_of::<bool>(),
                natural_align: std::mem::align_of::<bool>(),
                alignment: std::mem::align_of::<bool>(),
            },
            FieldInfo {
                name: "blinking",
                size: std::mem::size_of::<bool>(),
                natural_align: std::mem::align_of::<bool>(),
                alignment: std::mem::align_of::<bool>(),
            },
            FieldInfo {
                name: "password_input",
                size: std::mem::size_of::<bool>(),
                natural_align: std::mem::align_of::<bool>(),
                alignment: std::mem::align_of::<bool>(),
            },
            FieldInfo {
                name: "visual_style",
                size: std::mem::size_of::<CursorVisualStyle>(),
                natural_align: std::mem::align_of::<CursorVisualStyle>(),
                alignment: std::mem::align_of::<CursorVisualStyle>(),
            },
        ],
    }
}

// pub fn new(alloc_: ?*const CAllocator, result: *RenderState) ... Result {
//     result.* = new_(alloc_) catch |err| {
//         result.* = null;
//         return switch (err) { error.OutOfMemory => .out_of_memory };
//     };
//     return .success;
// }
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be a
/// valid writable pointer to a `RenderState`.
pub unsafe fn new(alloc_: Option<&CAllocator>, result: *mut RenderState) -> Result {
    match new_(alloc_) {
        Ok(ptr) => {
            // SAFETY: `result` is a valid writable `RenderState` (caller contract).
            unsafe { *result = ptr };
            Result::Success
        }
        Err(OutOfMemory) => {
            // SAFETY: `result` is a valid writable `RenderState` (caller contract).
            unsafe { *result = std::ptr::null_mut() };
            Result::OutOfMemory
        }
    }
}

// PORT NOTE: the reference `error{OutOfMemory}` carried by `new_`'s return.
struct OutOfMemory;

// fn new_(alloc_: ?*const CAllocator) error{OutOfMemory}!*RenderStateWrapper {
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(RenderStateWrapper) catch return error.OutOfMemory;
//     ptr.* = .{ .alloc = alloc };
//     return ptr;
// }
//
// PORT NOTE (W12-01, supersedes the D3-lifecycle substitution): `alloc.create`
// goes through the C-resolved allocator, so the caller allocator OWNS the
// wrapper storage and a rejecting allocator yields a recoverable
// `error.OutOfMemory` exactly like the reference. `RenderState::empty()`
// supplies the `.state = .empty` field default.
fn new_(alloc_: Option<&CAllocator>) -> std::result::Result<RenderState, OutOfMemory> {
    let alloc = rustty_lib::allocator::default(alloc_);
    // const ptr = alloc.create(RenderStateWrapper) catch return error.OutOfMemory;
    let ptr: *mut RenderStateWrapper = match rustty_lib::allocator::create_raw(&*alloc) {
        Ok(ptr) => ptr,
        Err(_) => return Err(OutOfMemory),
    };
    // ptr.* = .{ .alloc = alloc };
    // SAFETY: `ptr` is valid uninitialized `RenderStateWrapper` storage.
    unsafe {
        std::ptr::write(
            ptr,
            RenderStateWrapper {
                alloc,
                state: ZigRenderState::empty(),
            },
        )
    };
    Ok(ptr)
}

// pub fn free(state_: RenderState) callconv(lib.calling_conv) void {
//     const state = state_ orelse return;
//     const alloc = state.alloc;
//     state.state.deinit(alloc);
//     alloc.destroy(state);
// }
/// # Safety
/// `state_`, if non-null, must be a handle returned by `new`.
pub unsafe fn free(state_: RenderState) {
    // const state = state_ orelse return;
    if state_.is_null() {
        return;
    }
    // PORT NOTE (W12-01): move the wrapper value out of the caller-allocator
    // storage, deinit its contents, return the STORAGE through the resolved
    // allocator (`alloc.destroy(state)` — the caller free callback fires),
    // then drop the value (which drops the `alloc` box itself last).
    // SAFETY: `state_` is a live `new`-returned wrapper (caller contract);
    // after the read the storage is treated as uninitialized and freed once.
    let mut state = unsafe { std::ptr::read(state_) };
    // const alloc = state.alloc;
    // state.state.deinit(alloc);
    // PORT NOTE (W12-01 Tier-2 residual): rustty-vt's `RenderState` allocates
    // internally through `Global`, so `deinit` takes `Global`, not the caller
    // allocator (see DEFERRALS.md "F-008/W12-01 Tier 2").
    let gpa = allocator_api2::alloc::Global;
    state.state.deinit(gpa);
    // alloc.destroy(state);
    // SAFETY: `state_` was created by `new_` via `create_raw` with this
    // same resolved allocator.
    unsafe { rustty_lib::allocator::destroy_raw(&*state.alloc, state_) };
    drop(state);
}

// pub fn update(state_: RenderState, terminal_: terminal_c.Terminal) ... Result {
//     const state = state_ orelse return .invalid_value;
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     state.state.update(state.alloc, t) catch return .out_of_memory;
//     return .success;
// }
/// # Safety
/// `state_`/`terminal_`, if non-null, must be live handles.
pub unsafe fn update(state_: RenderState, terminal_: terminal_c::Terminal) -> Result {
    // const state = state_ orelse return .invalid_value;
    if state_.is_null() {
        return Result::InvalidValue;
    }
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper; raw-ptr read of the sibling field.
    let t = unsafe { (*terminal_).terminal };

    // state.state.update(state.alloc, t) catch return .out_of_memory;
    let gpa = allocator_api2::alloc::Global;
    // SAFETY: `state_`/`t` are live (caller contract); `update` needs `&mut`
    // access to both. They are independent allocations, so the two `&mut`s do
    // not alias.
    let state = unsafe { &mut *state_ };
    let t_ref = unsafe { &mut *t };
    match state.state.update(gpa, t_ref) {
        Ok(()) => Result::Success,
        Err(_oom) => Result::OutOfMemory,
    }
}

// pub fn begin_update(
//     state_: RenderState,
//     terminal_: terminal_c.Terminal,
// ) callconv(lib.calling_conv) Result {
//     const state = state_ orelse return .invalid_value;
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//
//     state.state.beginUpdate(state.alloc, t) catch return .out_of_memory;
//     return .success;
// }
/// # Safety
/// `state_`/`terminal_`, if non-null, must be live handles.
pub unsafe fn begin_update(state_: RenderState, terminal_: terminal_c::Terminal) -> Result {
    if state_.is_null() {
        return Result::InvalidValue;
    }
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `terminal_` is a live wrapper; raw-ptr read of the sibling field.
    let t = unsafe { (*terminal_).terminal };

    let gpa = allocator_api2::alloc::Global;
    // SAFETY: `state_`/`t` are live (caller contract); they are independent
    // allocations, so the two `&mut`s do not alias.
    let state = unsafe { &mut *state_ };
    let t_ref = unsafe { &mut *t };
    match state.state.begin_update(gpa, t_ref) {
        Ok(()) => Result::Success,
        Err(_oom) => Result::OutOfMemory,
    }
}

// pub fn end_update(
//     state_: RenderState,
// ) callconv(lib.calling_conv) Result {
//     const state = state_ orelse return .invalid_value;
//     state.state.endUpdate();
//     return .success;
// }
/// # Safety
/// `state_`, if non-null, must be a live handle.
pub unsafe fn end_update(state_: RenderState) -> Result {
    if state_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `state_` is live (caller contract).
    let state = unsafe { &mut *state_ };
    state.state.end_update();
    Result::Success
}

// pub fn clean(
//     state_: RenderState,
// ) callconv(lib.calling_conv) Result {
//     const state = state_ orelse return .invalid_value;
//     state.state.clean();
//     return .success;
// }
/// # Safety
/// `state_`, if non-null, must be a live handle.
pub unsafe fn clean(state_: RenderState) -> Result {
    // const state = state_ orelse return .invalid_value;
    if state_.is_null() {
        return Result::InvalidValue;
    }
    // state.state.clean();
    // SB (SOUNDNESS): a NARROW field-level `&mut (*state_).state` autoref, not a
    // wrapper-level `&mut *state_` — a live row iterator holds raw aliases into
    // this state (`rows` with buffer-root provenance, `state_dirty` off the
    // wrapper base tag), and a whole-wrapper `&mut` reborrow would invalidate
    // them for the documented clean-then-`next_dirty` interleave.
    // SAFETY: `state_` is live (caller contract); no other reference into the
    // wrapper is live during this call.
    unsafe { (*state_).state.clean() };
    Result::Success
}

// pub fn get(state_: RenderState, data: Data, out: ?*anyopaque) ... Result {
//     const state = state_ orelse return .invalid_value;
//     return getDispatch(state, data, out);
// }
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get(state_: RenderState, data: Data, out: *mut c_void) -> Result {
    // const state = state_ orelse return .invalid_value;
    if state_.is_null() {
        return Result::InvalidValue;
    }
    // return getDispatch(state, data, out);
    // SAFETY: `out` validity/type is the caller's contract per `data`.
    unsafe { get_dispatch(state_, data, out) }
}

// PORT NOTE (W34-B02): the reference carries the runtime-safety guard inside
// `getDispatch`, so every caller reaches it — including `get_multi`, which
// calls `getDispatch` per array element. rustty's `get_dispatch` takes the
// already-narrowed `Data` (a `#[repr(C)]` enum cannot even hold an out-of-range
// tag), so the guard lives here, in the raw-`c_int` entry point shared by the
// `extern "C"` wrapper in `lib.rs` AND by `get_multi` — the same "every path
// goes through the guard" shape as the reference.
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get_checked(state_: RenderState, data: c_int, out: *mut c_void) -> Result {
    // const state = state_ orelse return .invalid_value;
    if state_.is_null() {
        return Result::InvalidValue;
    }
    // _ = std.enums.fromInt(Data, @intFromEnum(data)) orelse {
    //     log.warn("render_state_get invalid data value={d}", .{@intFromEnum(data)});
    //     return .invalid_value;
    // };
    let Some(d) = Data::from_c_int(data) else {
        crate::sys::log_fn(
            crate::sys::StdLevel::Warn,
            LOG_SCOPE,
            format_args!("render_state_get invalid data value={data}"),
        );
        return Result::InvalidValue;
    };
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { get_dispatch(state_, d, out) }
}

// inline fn getDispatch(
//     state: *RenderStateWrapper,
//     data: Data,
//     out: ?*anyopaque,
// ) Result {
//     if (comptime std.debug.runtime_safety) { ... }
//     const out_ptr = out orelse return .invalid_value;
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| getTyped(state, comptime_data, @ptrCast(@alignCast(out_ptr))),
//     };
// }
/// # Safety
/// `state_` must be non-null and live; `out` must point to writable storage of
/// the type required by `data`.
#[inline]
unsafe fn get_dispatch(state_: RenderState, data: Data, out: *mut c_void) -> Result {
    // const out_ptr = out orelse return .invalid_value;
    if out.is_null() {
        return Result::InvalidValue;
    }
    match data {
        // .invalid => .invalid_value,
        Data::Invalid => Result::InvalidValue,
        // SAFETY: `out` validity/type is the caller's contract per `data`.
        _ => unsafe { get_typed(state_, data, out) },
    }
}

// pub fn get_multi(state_, count, keys, values, out_written) ... Result {
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//
//     // Unwrap the handle once for the whole batch rather than per key.
//     // A null handle fails on the first key, matching the per-call
//     // behavior.
//     const state: ?*RenderStateWrapper = state_;
//
//     for (0..count) |i| {
//         const result = if (state) |s| getDispatch(s, k[i], v[i]) else Result.invalid_value;
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
// PORT NOTE (W34-B02): `keys` is C memory typed `const RusttyRenderStateData *`
// in the published header; each element is read as a raw `c_int` (ABI-identical
// — `Data` is `#[repr(C)]`, asserted int-sized above) and range-checked per
// element. Reading an element straight into a `Data` would materialize an
// out-of-range discriminant, which is UB in Rust in EVERY build mode.
pub unsafe fn get_multi(
    state_: RenderState,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // const k = keys orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    // const v = values orelse return .invalid_value;
    if values.is_null() {
        return Result::InvalidValue;
    }

    // Unwrap the handle once for the whole batch rather than per key.
    // A null handle fails on the first key, matching the per-call
    // behavior.
    let state: RenderState = state_;

    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // const result = if (state) |s| getDispatch(s, k[i], v[i]) else Result.invalid_value;
        //
        // PORT NOTE (W34-B02): the reference's `getDispatch` receives a
        // possibly-invalid Zig enum and guards it; rustty narrows the raw
        // `c_int` per element (with the same `log.warn`) before dispatching.
        let result = if !state.is_null() {
            let Some(d) = Data::from_c_int(key) else {
                crate::sys::log_fn(
                    crate::sys::StdLevel::Warn,
                    LOG_SCOPE,
                    format_args!("render_state_get invalid data value={key}"),
                );
                if !out_written.is_null() {
                    // SAFETY: caller-provided out pointer, checked non-null.
                    unsafe { *out_written = i };
                }
                return Result::InvalidValue;
            };
            // SAFETY: `out` validity/type is the caller's contract.
            unsafe { get_dispatch(state, d, value) }
        } else {
            Result::InvalidValue
        };
        if result != Result::Success {
            if !out_written.is_null() {
                // SAFETY: caller-provided out pointer, checked non-null.
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    if !out_written.is_null() {
        // SAFETY: caller-provided out pointer, checked non-null.
        unsafe { *out_written = count };
    }
    Result::Success
}

// fn getTyped(state: *RenderStateWrapper, comptime data: Data, out: *data.OutType()) Result {
//     switch (data) { ... }
//     return .success;
// }
//
// SB (SOUNDNESS): every arm reads through raw place projections off the handle
// (`(*state_).state...`) instead of a wrapper-level `&mut *state_` — a live row
// iterator holds raw aliases into this wrapper (`state_dirty` off the wrapper
// base tag), and a whole-wrapper `&mut` reborrow would invalidate them (the
// rust-mut-args-strongly-protected discipline).
/// # Safety
/// `state_` must be non-null and live; `out` must point to writable storage of
/// the type required by `data`.
unsafe fn get_typed(state_: RenderState, data: Data, out: *mut c_void) -> Result {
    match data {
        // .invalid => return .invalid_value,
        Data::Invalid => return Result::InvalidValue,
        // .cols => out.* = state.state.cols,
        Data::Cols => {
            // SAFETY: `out` points to a writable `size::CellCountInt`.
            unsafe { *(out as *mut size::CellCountInt) = (*state_).state.cols };
        }
        // .rows => out.* = state.state.rows,
        Data::Rows => {
            // SAFETY: `out` points to a writable `size::CellCountInt`.
            unsafe { *(out as *mut size::CellCountInt) = (*state_).state.rows };
        }
        // .dirty => out.* = state.state.dirty,
        Data::Dirty => {
            // SAFETY: `out` points to a writable `Dirty` — the int-sized C
            // projection (W34-B03), so the caller's full slot is written.
            unsafe { *(out as *mut Dirty) = Dirty::from((*state_).state.dirty) };
        }
        // .row_iterator => {
        //     const it = out.* orelse return .invalid_value;
        //     const row_data = state.state.row_data.slice();
        //     it.* = .{ .alloc = it.alloc, .y = position_none, .raws = ..., .cells = ...,
        //               .selection = ..., .dirty = ..., .state_dirty = &state.state.dirty,
        //               .palette = &state.state.colors.palette };
        // },
        Data::RowIterator => {
            // const it = out.* orelse return .invalid_value;
            // PORT NOTE: `out` points to a `RowIterator` (a `*mut
            // RowIteratorWrapper`); the reference dereferences it (`out.*`) to
            // get the iterator handle.
            // SAFETY: `out` points to a writable `RowIterator`.
            let it_ptr = unsafe { *(out as *mut RowIterator) };
            if it_ptr.is_null() {
                return Result::InvalidValue;
            }
            // SAFETY: `it_ptr` is a live `row_iterator_new`-returned wrapper.
            let it = unsafe { &mut *it_ptr };
            // const row_data = state.state.row_data.slice();
            // it.* = .{ .alloc = it.alloc, .y = position_none, .raws = row_data.items(.raw),
            //           ..., .state_dirty = &state.state.dirty, .palette = &state.state.colors.palette };
            it.y = POSITION_NONE;
            // MUT slice: `row_set(.dirty)` later writes through this pointer,
            // so it must carry write provenance. SB: derive it from the
            // `Vec<Row>` BUFFER root (`as_mut_ptr` through a transient
            // field-level `&mut`), so the alias survives `clean`'s later
            // transient reborrows of the same rows (see the field's PORT NOTE).
            let row_data: *mut Vec<RenderRow> =
                unsafe { core::ptr::addr_of_mut!((*state_).state.row_data) };
            // SAFETY: `row_data` is a live `Vec` field of the live wrapper; the
            // autorefs below are transient and die within this statement.
            it.rows = core::ptr::slice_from_raw_parts_mut(
                unsafe { (*row_data).as_mut_ptr() },
                unsafe { (*row_data).len() },
            );
            // SB: raw projections off the wrapper base tag (`addr_of!`), never
            // through an intermediate reference, so they stay valid across the
            // transient borrows other exports create.
            it.state_dirty = unsafe { core::ptr::addr_of!((*state_).state.dirty) };
            it.palette = unsafe { core::ptr::addr_of!((*state_).state.colors.palette) };
        }
        // .color_background => out.* = state.state.colors.background.cval(),
        Data::ColorBackground => {
            // SAFETY: `out` points to a writable `RGB`.
            unsafe { *(out as *mut RGB) = (*state_).state.colors.background };
        }
        // .color_foreground => out.* = state.state.colors.foreground.cval(),
        Data::ColorForeground => {
            // SAFETY: `out` points to a writable `RGB`.
            unsafe { *(out as *mut RGB) = (*state_).state.colors.foreground };
        }
        // .color_cursor => {
        //     const cursor = state.state.colors.cursor orelse return .invalid_value;
        //     out.* = cursor.cval();
        // },
        Data::ColorCursor => {
            let cursor = match unsafe { (*state_).state.colors.cursor } {
                Some(cursor) => cursor,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `RGB`.
            unsafe { *(out as *mut RGB) = cursor };
        }
        // .color_cursor_has_value => out.* = state.state.colors.cursor != null,
        Data::ColorCursorHasValue => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = (*state_).state.colors.cursor.is_some() };
        }
        // .color_palette => out.* = colorpkg.paletteCval(&state.state.colors.palette),
        Data::ColorPalette => {
            // SAFETY: `out` points to a writable `PaletteC` (`[RGB; 256]`); the
            // shared reborrow of the palette is transient.
            unsafe {
                *(out as *mut [RGB; 256]) =
                    crate::color::palette_cval(&(*state_).state.colors.palette)
            };
        }
        // .cursor_visual_style => out.* = CursorVisualStyle.fromCursorStyle(state.state.cursor.visual_style),
        Data::CursorVisualStyle => {
            // SAFETY: `out` points to a writable `CursorVisualStyle`.
            unsafe {
                *(out as *mut CursorVisualStyle) =
                    CursorVisualStyle::from_cursor_style((*state_).state.cursor.visual_style)
            };
        }
        // .cursor_visible => out.* = state.state.cursor.visible,
        Data::CursorVisible => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = (*state_).state.cursor.visible };
        }
        // .cursor_blinking => out.* = state.state.cursor.blinking,
        Data::CursorBlinking => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = (*state_).state.cursor.blinking };
        }
        // .cursor_password_input => out.* = state.state.cursor.password_input,
        Data::CursorPasswordInput => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = (*state_).state.cursor.password_input };
        }
        // .cursor_viewport_has_value => out.* = state.state.cursor.viewport != null,
        Data::CursorViewportHasValue => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = (*state_).state.cursor.viewport.is_some() };
        }
        // .cursor_viewport_x => { const vp = state.state.cursor.viewport orelse return .invalid_value; out.* = vp.x; },
        Data::CursorViewportX => {
            let vp = match unsafe { &(*state_).state.cursor.viewport } {
                Some(vp) => vp,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `size::CellCountInt`.
            unsafe { *(out as *mut size::CellCountInt) = vp.x };
        }
        // .cursor_viewport_y => { const vp = ... orelse return .invalid_value; out.* = vp.y; },
        Data::CursorViewportY => {
            let vp = match unsafe { &(*state_).state.cursor.viewport } {
                Some(vp) => vp,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `size::CellCountInt`.
            unsafe { *(out as *mut size::CellCountInt) = vp.y };
        }
        // .cursor_viewport_wide_tail => { const vp = ... orelse return .invalid_value; out.* = vp.wide_tail; },
        Data::CursorViewportWideTail => {
            let vp = match unsafe { &(*state_).state.cursor.viewport } {
                Some(vp) => vp,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = vp.wide_tail };
        }
        // .cursor => return writeCursor(state, out),
        Data::Cursor => {
            // SAFETY: `out` points to a writable `Cursor` (caller contract).
            return unsafe { write_cursor(state_, out as *mut Cursor) };
        }
        // .colors => return writeColors(state, out),
        Data::Colors => {
            // SAFETY: `out` points to a writable `Colors` (caller contract).
            return unsafe { write_colors(state_, out as *mut Colors) };
        }
    }

    Result::Success
}

// pub fn set(state_: RenderState, option: SetOption, value: ?*const anyopaque) ... Result {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(SetOption, ...) catch { log.warn(...); return .invalid_value; } }
//     return switch (option) {
//         inline else => |comptime_option| setTyped(state_, comptime_option, @ptrCast(@alignCast(value orelse return .invalid_value))),
//     };
// }
//
// PORT NOTE: the `intToEnum(SetOption, ...)` guard lives in the `extern "C"`
// wrapper in `lib.rs`.
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
pub unsafe fn set(state_: RenderState, option: SetOption, value: *const c_void) -> Result {
    // value orelse return .invalid_value
    if value.is_null() {
        return Result::InvalidValue;
    }
    match option {
        // SAFETY: `value` is non-null and of the type required by `option`.
        SetOption::Dirty => unsafe { set_typed(state_, option, value) },
    }
}

// fn setTyped(state_: RenderState, comptime option: SetOption, value: *const option.InType()) Result {
//     const state = state_ orelse return .invalid_value;
//     switch (option) { .dirty => state.state.dirty = value.* }
//     return .success;
// }
/// # Safety
/// `value` must point to a value of the type required by `option`.
unsafe fn set_typed(state_: RenderState, option: SetOption, value: *const c_void) -> Result {
    // const state = state_ orelse return .invalid_value;
    if state_.is_null() {
        return Result::InvalidValue;
    }
    match option {
        // .dirty => state.state.dirty = value.*,
        SetOption::Dirty => {
            // SAFETY: `value` points to a readable int-sized `Dirty` (W34-B03:
            // the C enum is `c_int`-backed, so the full slot is read — the old
            // low-byte read both truncated and transmuted unvalidated bytes).
            let raw = unsafe { *(value as *const c_int) };
            // PORT NOTE: the reference loads `value.*` as a `Dirty` directly; an
            // out-of-range payload is safety-checked illegal behavior in Zig
            // (and UB in ReleaseFast). Rust cannot materialize an invalid
            // `#[repr(C)]` enum at all, so the D11 range check applies here and
            // an undeclared value returns `.invalid_value` (the same
            // disposition the `get` side gives an undeclared selector).
            let Some(dirty) = Dirty::from_c_int(raw) else {
                return Result::InvalidValue;
            };
            // SB: raw place write off the handle — no wrapper-level `&mut`
            // (see `get_typed`'s SOUNDNESS note).
            // SAFETY: `state_` is a live wrapper (caller contract).
            unsafe { (*state_).state.dirty = dirty.into() };
        }
    }
    Result::Success
}

// fn writeCursor(state: *RenderStateWrapper, out_cursor: *Cursor) Result {
//     const out_size = out_cursor.size;
//     if (out_size < @sizeOf(usize)) return .invalid_value;
//     const cursor = &state.state.cursor;
//     if (lib.structSizedFieldFits(Cursor, out_size, "viewport_has_value")) {
//         out_cursor.viewport_has_value = cursor.viewport != null;
//     }
//     if (cursor.viewport) |viewport| {
//         if (lib.structSizedFieldFits(Cursor, out_size, "viewport_x")) out_cursor.viewport_x = viewport.x;
//         if (lib.structSizedFieldFits(Cursor, out_size, "viewport_y")) out_cursor.viewport_y = viewport.y;
//         if (lib.structSizedFieldFits(Cursor, out_size, "wide_tail")) out_cursor.wide_tail = viewport.wide_tail;
//     }
//     if (lib.structSizedFieldFits(Cursor, out_size, "visible")) out_cursor.visible = cursor.visible;
//     if (lib.structSizedFieldFits(Cursor, out_size, "blinking")) out_cursor.blinking = cursor.blinking;
//     if (lib.structSizedFieldFits(Cursor, out_size, "password_input")) out_cursor.password_input = cursor.password_input;
//     if (lib.structSizedFieldFits(Cursor, out_size, "visual_style")) {
//         out_cursor.visual_style = CursorVisualStyle.fromCursorStyle(cursor.visual_style);
//     }
//     return .success;
// }
/// # Safety
/// `state_` must be non-null and live; `out_cursor_` must be a valid writable
/// pointer to a `Cursor` (its readable prefix per the sized-struct contract).
unsafe fn write_cursor(state_: RenderState, out_cursor_: *mut Cursor) -> Result {
    // SAFETY: `out_cursor_` is a valid writable `Cursor` (caller contract).
    let out_cursor = unsafe { &mut *out_cursor_ };
    // const out_size = out_cursor.size;
    let out_size = out_cursor.size;
    // if (out_size < @sizeOf(usize)) return .invalid_value;
    if out_size < std::mem::size_of::<usize>() {
        return Result::InvalidValue;
    }

    // const cursor = &state.state.cursor;
    // SB: shared reborrow of the cursor sub-field only (transient), never a
    // wrapper-level `&mut` (see `get_typed`'s SOUNDNESS note).
    // SAFETY: `state_` is a live wrapper (caller contract).
    let cursor = unsafe { &(*state_).state.cursor };
    let cursor_ty = cursor_struct_type();
    // if (lib.structSizedFieldFits(Cursor, out_size, "viewport_has_value")) { ... }
    if rustty_lib::struct_sized_field_fits(&cursor_ty, out_size, "viewport_has_value") {
        out_cursor.viewport_has_value = cursor.viewport.is_some();
    }

    // if (cursor.viewport) |viewport| { ... }
    if let Some(viewport) = &cursor.viewport {
        if rustty_lib::struct_sized_field_fits(&cursor_ty, out_size, "viewport_x") {
            out_cursor.viewport_x = viewport.x;
        }

        if rustty_lib::struct_sized_field_fits(&cursor_ty, out_size, "viewport_y") {
            out_cursor.viewport_y = viewport.y;
        }

        if rustty_lib::struct_sized_field_fits(&cursor_ty, out_size, "wide_tail") {
            out_cursor.wide_tail = viewport.wide_tail;
        }
    }

    // if (lib.structSizedFieldFits(Cursor, out_size, "visible")) out_cursor.visible = cursor.visible;
    if rustty_lib::struct_sized_field_fits(&cursor_ty, out_size, "visible") {
        out_cursor.visible = cursor.visible;
    }

    // if (lib.structSizedFieldFits(Cursor, out_size, "blinking")) out_cursor.blinking = cursor.blinking;
    if rustty_lib::struct_sized_field_fits(&cursor_ty, out_size, "blinking") {
        out_cursor.blinking = cursor.blinking;
    }

    // if (lib.structSizedFieldFits(Cursor, out_size, "password_input")) ...
    if rustty_lib::struct_sized_field_fits(&cursor_ty, out_size, "password_input") {
        out_cursor.password_input = cursor.password_input;
    }

    // if (lib.structSizedFieldFits(Cursor, out_size, "visual_style")) ...
    if rustty_lib::struct_sized_field_fits(&cursor_ty, out_size, "visual_style") {
        out_cursor.visual_style = CursorVisualStyle::from_cursor_style(cursor.visual_style);
    }

    Result::Success
}

// fn writeColors(state: *RenderStateWrapper, out_colors: *Colors) Result {
//     const out_size = out_colors.size;
//     if (out_size < @sizeOf(usize)) return .invalid_value;
//     const colors = &state.state.colors;
//     if (lib.structSizedFieldFits(Colors, out_size, "background")) out_colors.background = colors.background.cval();
//     if (lib.structSizedFieldFits(Colors, out_size, "foreground")) out_colors.foreground = colors.foreground.cval();
//     if (colors.cursor) |cursor| { if (lib.structSizedFieldFits(Colors, out_size, "cursor")) out_colors.cursor = cursor.cval(); }
//     if (lib.structSizedFieldFits(Colors, out_size, "cursor_has_value")) out_colors.cursor_has_value = colors.cursor != null;
//     { const palette_offset = @offsetOf(Colors, "palette");
//       if (out_size > palette_offset) {
//         const available = out_size - palette_offset;
//         const max_entries = @min(colors.palette.len, available / @sizeOf(colorpkg.RGB.C));
//         colorpkg.paletteCvalSlice(
//             colors.palette[0..max_entries],
//             out_colors.palette[0..max_entries],
//         );
//       } }
//     return .success;
// }
/// # Safety
/// `state_` must be non-null and live; `out_colors_` must be a valid writable
/// pointer to a `Colors` (its readable prefix per the sized-struct contract).
unsafe fn write_colors(state_: RenderState, out_colors_: *mut Colors) -> Result {
    // SAFETY: `out_colors_` is a valid writable `Colors` (caller contract).
    let out_colors = unsafe { &mut *out_colors_ };
    // const out_size = out_colors.size;
    let out_size = out_colors.size;
    // if (out_size < @sizeOf(usize)) return .invalid_value;
    if out_size < std::mem::size_of::<usize>() {
        return Result::InvalidValue;
    }

    // const colors = &state.state.colors;
    // SB: shared reborrow of the colors sub-field only (transient), never a
    // wrapper-level `&mut` (see `get_typed`'s SOUNDNESS note).
    // SAFETY: `state_` is a live wrapper (caller contract).
    let colors = unsafe { &(*state_).state.colors };
    let colors_ty = colors_struct_type();
    // if (lib.structSizedFieldFits(Colors, out_size, "background")) out_colors.background = colors.background.cval();
    if rustty_lib::struct_sized_field_fits(&colors_ty, out_size, "background") {
        out_colors.background = colors.background;
    }

    // if (lib.structSizedFieldFits(Colors, out_size, "foreground")) out_colors.foreground = colors.foreground.cval();
    if rustty_lib::struct_sized_field_fits(&colors_ty, out_size, "foreground") {
        out_colors.foreground = colors.foreground;
    }

    // if (colors.cursor) |cursor| { if (...,"cursor")) out_colors.cursor = cursor.cval(); }
    if let Some(cursor) = colors.cursor {
        if rustty_lib::struct_sized_field_fits(&colors_ty, out_size, "cursor") {
            out_colors.cursor = cursor;
        }
    }

    // if (lib.structSizedFieldFits(Colors, out_size, "cursor_has_value")) out_colors.cursor_has_value = colors.cursor != null;
    if rustty_lib::struct_sized_field_fits(&colors_ty, out_size, "cursor_has_value") {
        out_colors.cursor_has_value = colors.cursor.is_some();
    }

    // {
    //     const palette_offset = @offsetOf(Colors, "palette");
    //     if (out_size > palette_offset) {
    //         const available = out_size - palette_offset;
    //         const max_entries = @min(colors.palette.len, available / @sizeOf(colorpkg.RGB.C));
    //         colorpkg.paletteCvalSlice(
    //             colors.palette[0..max_entries],
    //             out_colors.palette[0..max_entries],
    //         );
    //     }
    // }
    {
        let palette_offset = colors_ty.offset_of("palette");
        if out_size > palette_offset {
            let available = out_size - palette_offset;
            let max_entries =
                std::cmp::min(colors.palette.len(), available / std::mem::size_of::<RGB>());
            rustty_vt::color::palette_cval_slice(
                &colors.palette[0..max_entries],
                &mut out_colors.palette[0..max_entries],
            );
        }
    }

    Result::Success
}

// pub fn row_iterator_new(alloc_: ?*const CAllocator, result: *RowIterator) ... Result {
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(RowIteratorWrapper) catch { result.* = null; return .out_of_memory; };
//     ptr.* = .{ .alloc, .y = undefined, .raws = undefined, .cells = undefined, .selection = undefined, .dirty = undefined, .state_dirty = undefined, .palette = undefined };
//     result.* = ptr;
//     return .success;
// }
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be a
/// valid writable pointer to a `RowIterator`.
pub unsafe fn row_iterator_new(alloc_: Option<&CAllocator>, result: *mut RowIterator) -> Result {
    let alloc = rustty_lib::allocator::default(alloc_);
    // const ptr = alloc.create(RowIteratorWrapper) catch { result.* = null; return .out_of_memory; };
    // PORT NOTE (W12-01): through the C-resolved allocator; a rejecting caller
    // allocator yields `.out_of_memory` + null out-param like the reference.
    let ptr: *mut RowIteratorWrapper = match rustty_lib::allocator::create_raw(&*alloc) {
        Ok(ptr) => ptr,
        Err(_) => {
            // SAFETY: `result` is a valid writable `RowIterator` (caller contract).
            unsafe { *result = std::ptr::null_mut() };
            return Result::OutOfMemory;
        }
    };
    // PORT NOTE: the reference `undefined` fields become null/sentinel values:
    // `y = POSITION_NONE`, `rows = &[]` (empty slice), `state_dirty`/`palette`
    // null. Real values are installed by `get(.., .row_iterator, ..)` before
    // any observation.
    // SAFETY: `ptr` is valid uninitialized `RowIteratorWrapper` storage.
    unsafe {
        std::ptr::write(
            ptr,
            RowIteratorWrapper {
                alloc,
                y: POSITION_NONE,
                // Empty placeholder slice (never dereferenced: `len == 0` makes
                // `row_iterator_next` return false; `get` installs the real slice).
                rows: std::ptr::slice_from_raw_parts_mut(
                    std::ptr::NonNull::<RenderRow>::dangling().as_ptr(),
                    0,
                ),
                state_dirty: std::ptr::null(),
                palette: std::ptr::null(),
            },
        )
    };
    // result.* = ptr;
    // SAFETY: `result` is a valid writable `RowIterator` (caller contract).
    unsafe { *result = ptr };
    Result::Success
}

// pub fn row_iterator_free(iterator_: RowIterator) ... void {
//     const iterator = iterator_ orelse return;
//     const alloc = iterator.alloc;
//     alloc.destroy(iterator);
// }
/// # Safety
/// `iterator_`, if non-null, must be a handle returned by `row_iterator_new`.
pub unsafe fn row_iterator_free(iterator_: RowIterator) {
    // const iterator = iterator_ orelse return;
    if iterator_.is_null() {
        return;
    }
    // const alloc = iterator.alloc;
    // alloc.destroy(iterator);
    // PORT NOTE (W12-01): return the storage through the C-resolved allocator
    // (caller free callback fires); the moved-out value drops afterwards.
    // SAFETY: `iterator_` is a live `row_iterator_new`-returned wrapper; after
    // the read the storage is treated as uninitialized and freed exactly once.
    let iterator = unsafe { std::ptr::read(iterator_) };
    unsafe { rustty_lib::allocator::destroy_raw(&*iterator.alloc, iterator_) };
    drop(iterator);
}

// pub fn row_iterator_next(iterator_: RowIterator) ... bool {
//     const it = iterator_ orelse return false;
//     // The none sentinel wraps to zero.
//     const next_y = it.y +% 1;
//     if (next_y >= it.raws.len) return false;
//     it.y = next_y;
//     return true;
// }
/// # Safety
/// `iterator_`, if non-null, must be a live iterator.
pub unsafe fn row_iterator_next(iterator_: RowIterator) -> bool {
    // const it = iterator_ orelse return false;
    if iterator_.is_null() {
        return false;
    }
    // SAFETY: `iterator_` is a live wrapper (caller contract).
    let it = unsafe { &mut *iterator_ };
    // The none sentinel wraps to zero.
    // const next_y = it.y +% 1;
    let next_y = it.y.wrapping_add(1);
    // if (next_y >= it.raws.len) return false;
    // PORT NOTE: `<*const [T]>::len` reads the slice-pointer metadata without
    // dereferencing, so no `&*` autoref of the raw pointer is needed.
    let rows_len = it.rows.len();
    if next_y >= rows_len {
        return false;
    }
    // it.y = next_y;
    it.y = next_y;
    true
}

// pub fn row_iterator_next_dirty(
//     iterator_: RowIterator,
//     out_y: ?*size.CellCountInt,
// ) callconv(lib.calling_conv) bool {
//     const it = iterator_ orelse return false;
//     const y_out = out_y orelse return false;
//
//     switch (it.state_dirty.*) {
//         .false => return false,
//         .full => { ... },
//         .partial => { ... },
//     }
// }
/// # Safety
/// `iterator_`, if non-null, must be a live iterator whose borrowed render
/// state has not been updated since it was populated; `out_y`, if non-null,
/// must be a valid writable pointer.
pub unsafe fn row_iterator_next_dirty(
    iterator_: RowIterator,
    out_y: *mut size::CellCountInt,
) -> bool {
    // const it = iterator_ orelse return false;
    if iterator_.is_null() {
        return false;
    }
    // SAFETY: `iterator_` is a live wrapper (caller contract).
    let it = unsafe { &mut *iterator_ };
    // const y_out = out_y orelse return false;
    if out_y.is_null() {
        return false;
    }

    // switch (it.state_dirty.*) { ... }
    // SAFETY: `it.state_dirty` was installed by `get(.row_iterator)` as a raw
    // projection off the live wrapper base (see the field's SB note); the
    // caller contract keeps that wrapper live and un-updated.
    match unsafe { *it.state_dirty } {
        // .false => return false,
        RenderDirty::False => false,
        // .full => {
        //     // The none sentinel wraps to zero.
        //     const next_y = it.y +% 1;
        //     if (next_y >= it.raws.len) return false;
        //     it.y = next_y;
        //     y_out.* = @intCast(next_y);
        //     return true;
        // },
        RenderDirty::Full => {
            // The none sentinel wraps to zero.
            let next_y = it.y.wrapping_add(1);
            if next_y >= it.rows.len() {
                return false;
            }
            it.y = next_y;
            // SAFETY: `out_y` is non-null and writable (caller contract).
            // `@intCast`: rows never exceed `CellCountInt`.
            unsafe { *out_y = next_y as size::CellCountInt };
            true
        }
        // .partial => {
        //     var next_y = it.y +% 1;
        //     while (next_y < it.raws.len) : (next_y += 1) {
        //         if (!it.dirty[next_y]) continue;
        //         it.y = next_y;
        //         y_out.* = @intCast(next_y);
        //         return true;
        //     }
        //     return false;
        // },
        RenderDirty::Partial => {
            let mut next_y = it.y.wrapping_add(1);
            while next_y < it.rows.len() {
                // if (!it.dirty[next_y]) continue;
                // SAFETY: `it.rows` is the live row slice installed by `get`;
                // `next_y` is bounds-checked by the loop condition.
                if !unsafe { (*it.rows)[next_y].dirty } {
                    next_y += 1;
                    continue;
                }
                it.y = next_y;
                // SAFETY: `out_y` is non-null and writable (caller contract).
                unsafe { *out_y = next_y as size::CellCountInt };
                return true;
            }
            false
        }
    }
}

// pub fn row_cells_new(alloc_: ?*const CAllocator, result: *RowCells) ... Result {
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(RowCellsWrapper) catch { result.* = null; return .out_of_memory; };
//     ptr.* = .{ .alloc, .x = undefined, .raws = undefined, .graphemes = undefined, .styles = undefined, .selection = undefined, .palette = undefined };
//     result.* = ptr;
//     return .success;
// }
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be a
/// valid writable pointer to a `RowCells`.
pub unsafe fn row_cells_new(alloc_: Option<&CAllocator>, result: *mut RowCells) -> Result {
    let alloc = rustty_lib::allocator::default(alloc_);
    // const ptr = alloc.create(RowCellsWrapper) catch { result.* = null; return .out_of_memory; };
    // PORT NOTE (W12-01): through the C-resolved allocator; a rejecting caller
    // allocator yields `.out_of_memory` + null out-param like the reference.
    let ptr: *mut RowCellsWrapper = match rustty_lib::allocator::create_raw(&*alloc) {
        Ok(ptr) => ptr,
        Err(_) => {
            // SAFETY: `result` is a valid writable `RowCells` (caller contract).
            unsafe { *result = std::ptr::null_mut() };
            return Result::OutOfMemory;
        }
    };
    // PORT NOTE: reference `undefined` fields become null/sentinel values;
    // real values are installed by `row_get(.., .cells, ..)`.
    // PORT NOTE (reshape): the sentinel is a pointer to a shared empty `CellList`
    // (len 0). `row_cells_next`/`select` must deref it to read the length, so it
    // must be a valid `CellList` (not a dangling pointer); real values are
    // installed by `row_get(.., .cells, ..)`.
    static EMPTY_CELLS: CellList = CellList::empty();
    // SAFETY: `ptr` is valid uninitialized `RowCellsWrapper` storage.
    unsafe {
        std::ptr::write(
            ptr,
            RowCellsWrapper {
                alloc,
                x: POSITION_NONE,
                cells: &EMPTY_CELLS as *const CellList,
                selection: None,
                palette: std::ptr::null(),
            },
        )
    };
    // result.* = ptr;
    // SAFETY: `result` is a valid writable `RowCells` (caller contract).
    unsafe { *result = ptr };
    Result::Success
}

// pub fn row_cells_next(cells_: RowCells) ... bool {
//     const cells = cells_ orelse return false;
//     // The none sentinel wraps to zero.
//     const next_x = cells.x +% 1;
//     if (next_x >= cells.raws.len) return false;
//     cells.x = next_x;
//     return true;
// }
/// # Safety
/// `cells_`, if non-null, must be a live row-cells handle.
pub unsafe fn row_cells_next(cells_: RowCells) -> bool {
    // const cells = cells_ orelse return false;
    if cells_.is_null() {
        return false;
    }
    // SAFETY: `cells_` is a live wrapper (caller contract).
    let cells = unsafe { &mut *cells_ };
    // The none sentinel wraps to zero.
    // const next_x = cells.x +% 1;
    let next_x = cells.x.wrapping_add(1);
    // if (next_x >= cells.raws.len) return false;
    // PORT NOTE (reshape): `cells.cells` is now a `*const CellList`, so the length
    // is read via a deref of the (valid) pointer's `raw` column.
    let cells_len = unsafe { (*cells.cells).len() };
    if next_x >= cells_len {
        return false;
    }
    // cells.x = next_x;
    cells.x = next_x;
    true
}

// pub fn row_cells_select(cells_: RowCells, x: size.CellCountInt) ... Result {
//     const cells = cells_ orelse return .invalid_value;
//     if (x >= cells.raws.len) return .invalid_value;
//     cells.x = x;
//     return .success;
// }
/// # Safety
/// `cells_`, if non-null, must be a live row-cells handle.
pub unsafe fn row_cells_select(cells_: RowCells, x: size::CellCountInt) -> Result {
    // const cells = cells_ orelse return .invalid_value;
    if cells_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `cells_` is a live wrapper (caller contract).
    let cells = unsafe { &mut *cells_ };
    // if (x >= cells.raws.len) return .invalid_value;
    // PORT NOTE (reshape): length via a deref of the `*const CellList` pointer.
    let cells_len = unsafe { (*cells.cells).len() };
    if (x as usize) >= cells_len {
        return Result::InvalidValue;
    }
    // cells.x = x;
    cells.x = x as usize;
    Result::Success
}

// pub fn row_cells_free(cells_: RowCells) ... void {
//     const cells = cells_ orelse return;
//     const alloc = cells.alloc;
//     alloc.destroy(cells);
// }
/// # Safety
/// `cells_`, if non-null, must be a handle returned by `row_cells_new`.
pub unsafe fn row_cells_free(cells_: RowCells) {
    // const cells = cells_ orelse return;
    if cells_.is_null() {
        return;
    }
    // const alloc = cells.alloc;
    // alloc.destroy(cells);
    // PORT NOTE (W12-01): return the storage through the C-resolved allocator
    // (caller free callback fires); the moved-out value drops afterwards.
    // SAFETY: `cells_` is a live `row_cells_new`-returned wrapper; after the
    // read the storage is treated as uninitialized and freed exactly once.
    let cells = unsafe { std::ptr::read(cells_) };
    unsafe { rustty_lib::allocator::destroy_raw(&*cells.alloc, cells_) };
    drop(cells);
}

/// C: GhosttyRenderStateRowCellsData -> RusttyRenderStateRowCellsData (R2)
// pub const RowCellsData = enum(c_int) { invalid = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowCellsData {
    Invalid = 0,
    Raw = 1,
    Style = 2,
    GraphemesLen = 3,
    GraphemesBuf = 4,
    BgColor = 5,
    FgColor = 6,
    Selected = 7,
    HasStyling = 8,
    GraphemesUtf8 = 9,
}

// D11 (P3-W6 T2): see `Data::from_c_int`; same rule, same header hazard.
const _: () = assert!(std::mem::size_of::<RowCellsData>() == std::mem::size_of::<c_int>());

impl RowCellsData {
    /// Narrow an inbound C integer to a `RowCellsData`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range.
    pub fn from_c_int(v: c_int) -> Option<RowCellsData> {
        // contiguous 0..=9
        if (0..=9).contains(&v) {
            // SAFETY: `v` is a valid `RowCellsData` discriminant (range-checked).
            Some(unsafe { std::mem::transmute::<c_int, RowCellsData>(v) })
        } else {
            None
        }
    }
}

// Output type expected for querying the data of the given kind.
// PORT NOTE: `pub fn OutType(comptime self: RowCellsData) type`:
//     .invalid => void,
//     .raw => page.Cell.C,
//     .style => style_c.Style,
//     .graphemes_len => u32,
//     .graphemes_buf => u32,
//     .bg_color, .fg_color => colorpkg.RGB.C,
//     .selected, .has_styling => bool,
//     .graphemes_utf8 => lib.Buffer,
// In Rust the OutType mapping is preserved as the per-arm pointer cast in
// `row_cells_get_typed`.

// pub fn row_cells_get(cells_: RowCells, data: RowCellsData, out: ?*anyopaque) ... Result {
//     const cells = cells_ orelse return .invalid_value;
//     const x = cells.x;
//     if (x >= cells.raws.len) return .invalid_value;
//     return rowCellsGetDispatch(cells, x, data, out);
// }
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn row_cells_get(cells_: RowCells, data: RowCellsData, out: *mut c_void) -> Result {
    // const cells = cells_ orelse return .invalid_value;
    if cells_.is_null() {
        return Result::InvalidValue;
    }
    // const x = cells.x;
    // if (x >= cells.raws.len) return .invalid_value;
    let x = unsafe { (*cells_).x };
    if x >= unsafe { (*(*cells_).cells).len() } {
        return Result::InvalidValue;
    }
    // return rowCellsGetDispatch(cells, x, data, out);
    // SAFETY: `out` validity/type is the caller's contract per `data`.
    unsafe { row_cells_get_dispatch(cells_, x, data, out) }
}

// PORT NOTE (W34-B02): see `get_checked` — the raw-`c_int` entry point carrying
// the reference's runtime-safety guard (which the reference keeps inside
// `rowCellsGetDispatch`), shared by the `extern "C"` wrapper in `lib.rs`.
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn row_cells_get_checked(cells_: RowCells, data: c_int, out: *mut c_void) -> Result {
    // const cells = cells_ orelse return .invalid_value;
    if cells_.is_null() {
        return Result::InvalidValue;
    }
    // const x = cells.x;
    // if (x >= cells.raws.len) return .invalid_value;
    let x = unsafe { (*cells_).x };
    if x >= unsafe { (*(*cells_).cells).len() } {
        return Result::InvalidValue;
    }
    // _ = std.enums.fromInt(RowCellsData, @intFromEnum(data)) orelse {
    //     log.warn("render_state_row_cells_get invalid data value={d}", .{...});
    //     return .invalid_value;
    // };
    let Some(d) = RowCellsData::from_c_int(data) else {
        crate::sys::log_fn(
            crate::sys::StdLevel::Warn,
            LOG_SCOPE,
            format_args!("render_state_row_cells_get invalid data value={data}"),
        );
        return Result::InvalidValue;
    };
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { row_cells_get_dispatch(cells_, x, d, out) }
}

// pub fn row_cells_get_multi(cells_, count, keys, values, out_written) ... Result {
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//
//     // Unwrap the handle and position once for the whole batch rather
//     // than per key. An invalid handle/position fails on the first
//     // key, matching the per-call behavior.
//     const unwrapped: ?struct { *RowCellsWrapper, usize } = valid: {
//         const cells = cells_ orelse break :valid null;
//         const x = cells.x;
//         if (x >= cells.raws.len) break :valid null;
//         break :valid .{ cells, x };
//     };
//
//     for (0..count) |i| {
//         const result = if (unwrapped) |u| rowCellsGetDispatch(u[0], u[1], k[i], v[i]) else Result.invalid_value;
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
// PORT NOTE (W34-B02): `keys` elements are read as raw `c_int` and
// range-checked per element (the reference's `rowCellsGetDispatch` guard).
pub unsafe fn row_cells_get_multi(
    cells_: RowCells,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // const k = keys orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    // const v = values orelse return .invalid_value;
    if values.is_null() {
        return Result::InvalidValue;
    }

    // Unwrap the handle and position once for the whole batch rather
    // than per key. An invalid handle/position fails on the first
    // key, matching the per-call behavior.
    let unwrapped: Option<(RowCells, usize)> = 'valid: {
        if cells_.is_null() {
            break 'valid None;
        }
        let x = unsafe { (*cells_).x };
        if x >= unsafe { (*(*cells_).cells).len() } {
            break 'valid None;
        }
        Some((cells_, x))
    };

    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // const result = if (unwrapped) |u| rowCellsGetDispatch(u[0], u[1], k[i], v[i]) else Result.invalid_value;
        let result = if let Some((cells, x)) = unwrapped {
            let Some(d) = RowCellsData::from_c_int(key) else {
                crate::sys::log_fn(
                    crate::sys::StdLevel::Warn,
                    LOG_SCOPE,
                    format_args!("render_state_row_cells_get invalid data value={key}"),
                );
                if !out_written.is_null() {
                    // SAFETY: caller-provided out pointer, checked non-null.
                    unsafe { *out_written = i };
                }
                return Result::InvalidValue;
            };
            // SAFETY: `out` validity/type is the caller's contract.
            unsafe { row_cells_get_dispatch(cells, x, d, value) }
        } else {
            Result::InvalidValue
        };
        if result != Result::Success {
            if !out_written.is_null() {
                // SAFETY: caller-provided out pointer, checked non-null.
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    if !out_written.is_null() {
        // SAFETY: caller-provided out pointer, checked non-null.
        unsafe { *out_written = count };
    }
    Result::Success
}

// inline fn rowCellsGetDispatch(
//     cells: *const RowCellsWrapper,
//     x: usize,
//     data: RowCellsData,
//     out: ?*anyopaque,
// ) Result {
//     if (comptime std.debug.runtime_safety) { ... }
//     if (out == null) return .invalid_value;
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| rowCellsGetTypedInner(cells, x, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
/// # Safety
/// `cells_` must be non-null and live; `x` in bounds; `out` must point to
/// writable storage of the type required by `data`.
#[inline]
unsafe fn row_cells_get_dispatch(
    cells_: RowCells,
    x: usize,
    data: RowCellsData,
    out: *mut c_void,
) -> Result {
    // if (out == null) return .invalid_value;
    if out.is_null() {
        return Result::InvalidValue;
    }
    match data {
        // .invalid => .invalid_value,
        RowCellsData::Invalid => Result::InvalidValue,
        // SAFETY: `out` validity/type is the caller's contract per `data`.
        _ => unsafe { row_cells_get_typed_inner(cells_, x, data, out) },
    }
}

// fn rowCellsGetTypedInner(
//     cells: *const RowCellsWrapper,
//     x: usize,
//     comptime data: RowCellsData,
//     out: *data.OutType(),
// ) Result {
//     const cell = cells.raws[x];
//     switch (data) { ... }
//     return .success;
// }
/// # Safety
/// `cells_` must be non-null and live; `x` in bounds; `out` must point to
/// writable storage of the type required by `data`.
unsafe fn row_cells_get_typed_inner(
    cells_: RowCells,
    x: usize,
    data: RowCellsData,
    out: *mut c_void,
) -> Result {
    // SAFETY: `cells_` is a live wrapper (caller contract).
    let cells = unsafe { &*cells_ };
    // const cell = cells.raws[x];
    // PORT NOTE (reshape): rustty-vt stores cells SoA (`CellList`); `cell` is the
    // `page.Cell` (`(*cells.cells).raw()[x]`), and the `style`/`grapheme` columns
    // are projected from the same list below.
    // SAFETY: `cells.cells` is a valid `CellList` pointer installed by `row_get`;
    // `x` is bounds-checked by `row_cells_next`/`row_cells_select`.
    let cell_list = unsafe { &*cells.cells };
    let cell: page::Cell = cell_list.raw()[x as usize];
    match data {
        // .invalid => return .invalid_value,
        RowCellsData::Invalid => return Result::InvalidValue,
        // .raw => out.* = cell.cval(),
        RowCellsData::Raw => {
            // SAFETY: `out` points to a writable `page.Cell.C` (`u64`).
            unsafe { *(out as *mut u64) = cell.cval() };
        }
        // .style => if (cell.hasStyling()) {
        //     style_c.Style.write(cells.styles[x], out);
        // } else {
        //     out.* = style_c.Style.default;
        // },
        RowCellsData::Style => {
            if cell.has_styling() {
                // SAFETY: `out` points to a writable `style_c::Style`.
                style_c::Style::write(cell_list.style()[x], unsafe {
                    &mut *(out as *mut style_c::Style)
                });
            } else {
                // SAFETY: `out` points to a writable `style_c::Style`.
                unsafe { *(out as *mut style_c::Style) = style_c::Style::DEFAULT };
            }
        }
        // .graphemes_len => {
        //     if (!cell.hasText()) { out.* = 0; return .success; }
        //     const extra = if (cell.hasGrapheme()) cells.graphemes[x] else &[_]u21{};
        //     out.* = @intCast(1 + extra.len);
        // },
        RowCellsData::GraphemesLen => {
            if !cell.has_text() {
                // SAFETY: `out` points to a writable `u32`.
                unsafe { *(out as *mut u32) = 0 };
                return Result::Success;
            }
            let extra: &[u32] = if cell.has_grapheme() {
                cell_list.grapheme(x as usize)
            } else {
                &[]
            };
            // SAFETY: `out` points to a writable `u32`.
            unsafe { *(out as *mut u32) = (1 + extra.len()) as u32 };
        }
        // .graphemes_buf => {
        //     if (!cell.hasText()) return .success;
        //     const extra = if (cell.hasGrapheme()) cells.graphemes[x] else &[_]u21{};
        //     const buf: [*]u32 = @ptrCast(out);
        //     buf[0] = cell.codepoint();
        //     for (extra, 1..) |cp, i| buf[i] = cp;
        // },
        RowCellsData::GraphemesBuf => {
            if !cell.has_text() {
                return Result::Success;
            }
            let extra: &[u32] = if cell.has_grapheme() {
                cell_list.grapheme(x as usize)
            } else {
                &[]
            };
            // const buf: [*]u32 = @ptrCast(out);
            let buf = out as *mut u32;
            // buf[0] = cell.codepoint();
            // SAFETY: `out`/`buf` points to writable storage of `graphemes_len`
            // `u32`s (the caller sizes it from `.graphemes_len`).
            // PORT NOTE: `cell.codepoint()` is `u21` (a `u32` alias), so the
            // C `u32` buffer slot takes the value directly.
            unsafe { *buf = cell.codepoint() };
            // for (extra, 1..) |cp, i| buf[i] = cp;
            for (i, &cp) in extra.iter().enumerate() {
                // SAFETY: same buffer contract; `i + 1 < graphemes_len`.
                unsafe { *buf.add(i + 1) = cp };
            }
        }
        // .bg_color => {
        //     // Avoid copying the full struct when only partial changes happen.
        //     switch (cell.content_tag) {
        //         .bg_color_palette => { out.* = cells.palette[cell.content.color_palette.data].cval(); },
        //         .bg_color_rgb => { const rgb = cell.content.color_rgb; out.* = .{ .r = rgb.r, .g = rgb.g, .b = rgb.b }; },
        //         .codepoint, .codepoint_grapheme, => {
        //             // The default style has no background.
        //             if (!cell.hasStyling()) return .invalid_value;
        //             switch (cells.styles[x].bg_color) {
        //                 .none => return .invalid_value,
        //                 .palette => |idx| out.* = cells.palette[idx].cval(),
        //                 .rgb => |rgb| out.* = rgb.cval(),
        //             }
        //         },
        //     }
        // },
        RowCellsData::BgColor => {
            // Avoid copying the full struct when only partial changes happen.
            match cell.content_tag() {
                page::ContentTag::BgColorPalette => {
                    // SAFETY: `cells.palette` is a valid palette pointer
                    // installed by `row_get`; `out` points to a writable `RGB`.
                    unsafe {
                        *(out as *mut RGB) =
                            (*cells.palette)[cell.content_color_palette() as usize]
                    };
                }

                page::ContentTag::BgColorRgb => {
                    let rgb = cell.content_color_rgb();
                    // SAFETY: `out` points to a writable `RGB`.
                    unsafe {
                        *(out as *mut RGB) = RGB {
                            r: rgb.r,
                            g: rgb.g,
                            b: rgb.b,
                        }
                    };
                }

                page::ContentTag::Codepoint | page::ContentTag::CodepointGrapheme => {
                    // The default style has no background.
                    if !cell.has_styling() {
                        return Result::InvalidValue;
                    }
                    // PORT NOTE (WS19 T5): `style_pkg::Color` is a byte-complete
                    // `{ tag, data }` value, so the reference's payload `switch`
                    // is a switch on `.tag()` with tag-checked accessors.
                    let bg_color = cell_list.style()[x].bg_color;
                    match bg_color.tag() {
                        ColorTag::None => return Result::InvalidValue,
                        // SAFETY: `cells.palette` is a valid palette pointer
                        // installed by `row_get`; `out` is a writable `RGB`.
                        ColorTag::Palette => unsafe {
                            *(out as *mut RGB) = (*cells.palette)[bg_color.palette() as usize]
                        },
                        // SAFETY: `out` points to a writable `RGB`.
                        ColorTag::Rgb => unsafe { *(out as *mut RGB) = bg_color.rgb() },
                    }
                }
            }
        }
        // .fg_color => {
        //     if (!cell.hasStyling()) return .invalid_value;
        //     switch (cells.styles[x].fg_color) {
        //         .none => return .invalid_value,
        //         .palette => |idx| out.* = cells.palette[idx].cval(),
        //         .rgb => |rgb| out.* = rgb.cval(),
        //     }
        // },
        RowCellsData::FgColor => {
            if !cell.has_styling() {
                return Result::InvalidValue;
            }
            let fg_color = cell_list.style()[x].fg_color;
            match fg_color.tag() {
                ColorTag::None => return Result::InvalidValue,
                // SAFETY: `cells.palette` is a valid palette pointer installed
                // by `row_get`; `out` points to a writable `RGB`.
                ColorTag::Palette => unsafe {
                    *(out as *mut RGB) = (*cells.palette)[fg_color.palette() as usize]
                },
                // SAFETY: `out` points to a writable `RGB`.
                ColorTag::Rgb => unsafe { *(out as *mut RGB) = fg_color.rgb() },
            }
        }
        // .selected => out.* = if (cells.selection) |sel| x >= sel[0] and x <= sel[1] else false,
        RowCellsData::Selected => {
            let selected = match cells.selection {
                Some(sel) => x >= sel[0] as usize && x <= sel[1] as usize,
                None => false,
            };
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = selected };
        }
        // .has_styling => out.* = cell.hasStyling(),
        RowCellsData::HasStyling => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = cell.has_styling() };
        }
        // .graphemes_utf8 => return rowCellsGetGraphemesUtf8(cell, if (cell.hasGrapheme()) cells.graphemes[x] else &.{}, out),
        RowCellsData::GraphemesUtf8 => {
            let extra: &[u32] = if cell.has_grapheme() {
                cell_list.grapheme(x as usize)
            } else {
                &[]
            };
            // SAFETY: `out` points to a writable `lib.Buffer`.
            return unsafe { row_cells_get_graphemes_utf8(cell, extra, out as *mut Buffer) };
        }
    }

    Result::Success
}

// fn rowCellsGetGraphemesUtf8(cell: page.Cell, extra: []const u21, out: *lib.Buffer) Result {
//     if (!cell.hasText()) { out.len = 0; return .success; }
//     const first = cell.codepoint();
//     ... (single-ASCII fast path) ...
//     out.len = 0;
//     var needed = std.unicode.utf8CodepointSequenceLength(first) catch return .invalid_value;
//     for (extra) |cp| needed += std.unicode.utf8CodepointSequenceLength(cp) catch return .invalid_value;
//     out.len = needed;
//     if (out.ptr == null or out.cap < needed) return .out_of_space;
//     const buf = out.ptr.?[0..out.cap];
//     var i: usize = 0;
//     i += std.unicode.utf8Encode(first, buf[i..]) catch return .invalid_value;
//     for (extra) |cp| i += std.unicode.utf8Encode(cp, buf[i..]) catch return .invalid_value;
//     out.len = i;
//     return .success;
// }
//
// PORT NOTE: `std.unicode.utf8CodepointSequenceLength` rejects invalid scalar
// values; the Rust analogue is `char::from_u32(cp)` (returns `None` for invalid
// scalars / surrogates) then `char::len_utf8`. `utf8Encode` is `char::encode_utf8`.
/// # Safety
/// `out` must be a valid writable pointer to a `lib.Buffer`.
unsafe fn row_cells_get_graphemes_utf8(
    cell: page::Cell,
    extra: &[u32],
    out: *mut Buffer,
) -> Result {
    // SAFETY: `out` is a valid writable `Buffer` (caller contract).
    let out = unsafe { &mut *out };

    // if (!cell.hasText()) { out.len = 0; return .success; }
    if !cell.has_text() {
        out.len = 0;
        return Result::Success;
    }

    // const first = cell.codepoint();
    let first = cell.codepoint();

    // Fast path: a single ASCII codepoint written to an adequately
    // sized buffer. This is the overwhelmingly common case for
    // terminal content.
    // if (first < 0x80 and extra.len == 0) {
    //     @branchHint(.likely);
    //     if (out.ptr) |ptr| { if (out.cap >= 1) { ptr[0] = @intCast(first); out.len = 1; return .success; } }
    //     out.len = 1;
    //     return .out_of_space;
    // }
    if first < 0x80 && extra.is_empty() {
        if !out.ptr.is_null() && out.cap >= 1 {
            // SAFETY: `out.ptr` is non-null with `out.cap >= 1` writable bytes
            // (caller contract).
            unsafe { *out.ptr = first as u8 };
            out.len = 1;
            return Result::Success;
        }
        out.len = 1;
        return Result::OutOfSpace;
    }

    // out.len = 0;
    out.len = 0;

    // var needed = std.unicode.utf8CodepointSequenceLength(first) catch return .invalid_value;
    let cp0 = match char::from_u32(first) {
        Some(c) => c,
        None => return Result::InvalidValue,
    };
    let mut needed = cp0.len_utf8();
    // for (extra) |cp| needed += std.unicode.utf8CodepointSequenceLength(cp) catch return .invalid_value;
    for &cp in extra {
        let c = match char::from_u32(cp) {
            Some(c) => c,
            None => return Result::InvalidValue,
        };
        needed += c.len_utf8();
    }
    // out.len = needed;
    out.len = needed;

    // if (out.ptr == null or out.cap < needed) return .out_of_space;
    if out.ptr.is_null() || out.cap < needed {
        return Result::OutOfSpace;
    }

    // const buf = out.ptr.?[0..out.cap];
    // SAFETY: `out.ptr` is non-null with `out.cap` writable bytes (caller contract).
    let buf = unsafe { std::slice::from_raw_parts_mut(out.ptr, out.cap) };
    // var i: usize = 0;
    let mut i: usize = 0;
    // i += std.unicode.utf8Encode(first, buf[i..]) catch return .invalid_value;
    i += cp0.encode_utf8(&mut buf[i..]).len();
    // for (extra) |cp| i += std.unicode.utf8Encode(cp, buf[i..]) catch return .invalid_value;
    for &cp in extra {
        // The validity was checked above; `from_u32` cannot fail here.
        let c = match char::from_u32(cp) {
            Some(c) => c,
            None => return Result::InvalidValue,
        };
        i += c.encode_utf8(&mut buf[i..]).len();
    }

    // out.len = i;
    out.len = i;
    Result::Success
}

/// C: GhosttyRenderStateRowData -> RusttyRenderStateRowData (R2)
// pub const RowData = enum(c_int) { invalid = 0, dirty = 1, raw = 2, cells = 3, selection = 4, cells_raw = 5 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowData {
    Invalid = 0,
    Dirty = 1,
    Raw = 2,
    Cells = 3,
    Selection = 4,
    CellsRaw = 5,
}

// D11 (P3-W6 T2): see `Data::from_c_int`; same rule, same header hazard.
const _: () = assert!(std::mem::size_of::<RowData>() == std::mem::size_of::<c_int>());

impl RowData {
    /// Narrow an inbound C integer to a `RowData`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range.
    pub fn from_c_int(v: c_int) -> Option<RowData> {
        // contiguous 0..=5
        if (0..=5).contains(&v) {
            // SAFETY: `v` is a valid `RowData` discriminant (range-checked).
            Some(unsafe { std::mem::transmute::<c_int, RowData>(v) })
        } else {
            None
        }
    }
}

// Output type expected for querying the data of the given kind.
// PORT NOTE: `pub fn OutType(comptime self: RowData) type`:
//     .invalid => void,
//     .dirty => bool,
//     .raw => row.CRow,
//     .cells => RowCells,
//     .selection => RowSelection,
//     .cells_raw => cell_c.CellsView,
// In Rust the OutType mapping is preserved as the per-arm pointer cast in
// `row_get_typed`.

/// C: GhosttyRenderStateRowOption -> RusttyRenderStateRowOption (R2)
// pub const RowOption = enum(c_int) { dirty = 0 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowOption {
    Dirty = 0,
}

// Input type expected for setting the option.
// PORT NOTE: `pub fn InType(comptime self: RowOption) type`: `.dirty => bool`.

// pub fn row_get(iterator_: RowIterator, data: RowData, out: ?*anyopaque) ... Result {
//     const it = iterator_ orelse return .invalid_value;
//     const y = it.y;
//     if (y >= it.raws.len) return .invalid_value;
//     return rowGetDispatch(it, y, data, out);
// }
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn row_get(iterator_: RowIterator, data: RowData, out: *mut c_void) -> Result {
    // const it = iterator_ orelse return .invalid_value;
    if iterator_.is_null() {
        return Result::InvalidValue;
    }
    // const y = it.y;
    // if (y >= it.raws.len) return .invalid_value;
    let y = unsafe { (*iterator_).y };
    if y >= unsafe { (*iterator_).rows.len() } {
        return Result::InvalidValue;
    }
    // return rowGetDispatch(it, y, data, out);
    // SAFETY: `out` validity/type is the caller's contract per `data`.
    unsafe { row_get_dispatch(iterator_, y, data, out) }
}

// PORT NOTE (W34-B02): see `get_checked` — the raw-`c_int` entry point carrying
// the reference's runtime-safety guard (which the reference keeps inside
// `rowGetDispatch`), shared by the `extern "C"` wrapper in `lib.rs`.
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn row_get_checked(iterator_: RowIterator, data: c_int, out: *mut c_void) -> Result {
    // const it = iterator_ orelse return .invalid_value;
    if iterator_.is_null() {
        return Result::InvalidValue;
    }
    // const y = it.y;
    // if (y >= it.raws.len) return .invalid_value;
    let y = unsafe { (*iterator_).y };
    if y >= unsafe { (*iterator_).rows.len() } {
        return Result::InvalidValue;
    }
    // _ = std.enums.fromInt(RowData, @intFromEnum(data)) orelse {
    //     log.warn("render_state_row_get invalid data value={d}", .{...});
    //     return .invalid_value;
    // };
    let Some(d) = RowData::from_c_int(data) else {
        crate::sys::log_fn(
            crate::sys::StdLevel::Warn,
            LOG_SCOPE,
            format_args!("render_state_row_get invalid data value={data}"),
        );
        return Result::InvalidValue;
    };
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { row_get_dispatch(iterator_, y, d, out) }
}

// pub fn row_get_multi(iterator_, count, keys, values, out_written) ... Result {
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//
//     // Unwrap the handle and position once for the whole batch rather
//     // than per key. An invalid handle/position fails on the first
//     // key, matching the per-call behavior.
//     const unwrapped: ?struct { *RowIteratorWrapper, usize } = valid: {
//         const it = iterator_ orelse break :valid null;
//         const y = it.y;
//         if (y >= it.raws.len) break :valid null;
//         break :valid .{ it, y };
//     };
//
//     for (0..count) |i| {
//         const result = if (unwrapped) |u| rowGetDispatch(u[0], u[1], k[i], v[i]) else Result.invalid_value;
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
// PORT NOTE (W34-B02): `keys` elements are read as raw `c_int` and
// range-checked per element (the reference's `rowGetDispatch` guard).
pub unsafe fn row_get_multi(
    iterator_: RowIterator,
    count: usize,
    keys: *const c_int,
    values: *mut *mut c_void,
    out_written: *mut usize,
) -> Result {
    // const k = keys orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    // const v = values orelse return .invalid_value;
    if values.is_null() {
        return Result::InvalidValue;
    }

    // Unwrap the handle and position once for the whole batch rather
    // than per key. An invalid handle/position fails on the first
    // key, matching the per-call behavior.
    let unwrapped: Option<(RowIterator, usize)> = 'valid: {
        if iterator_.is_null() {
            break 'valid None;
        }
        let y = unsafe { (*iterator_).y };
        if y >= unsafe { (*iterator_).rows.len() } {
            break 'valid None;
        }
        Some((iterator_, y))
    };

    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // const result = if (unwrapped) |u| rowGetDispatch(u[0], u[1], k[i], v[i]) else Result.invalid_value;
        let result = if let Some((it, y)) = unwrapped {
            let Some(d) = RowData::from_c_int(key) else {
                crate::sys::log_fn(
                    crate::sys::StdLevel::Warn,
                    LOG_SCOPE,
                    format_args!("render_state_row_get invalid data value={key}"),
                );
                if !out_written.is_null() {
                    // SAFETY: caller-provided out pointer, checked non-null.
                    unsafe { *out_written = i };
                }
                return Result::InvalidValue;
            };
            // SAFETY: `out` validity/type is the caller's contract.
            unsafe { row_get_dispatch(it, y, d, value) }
        } else {
            Result::InvalidValue
        };
        if result != Result::Success {
            if !out_written.is_null() {
                // SAFETY: caller-provided out pointer, checked non-null.
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    if !out_written.is_null() {
        // SAFETY: caller-provided out pointer, checked non-null.
        unsafe { *out_written = count };
    }
    Result::Success
}

// inline fn rowGetDispatch(
//     it: *RowIteratorWrapper,
//     y: usize,
//     data: RowData,
//     out: ?*anyopaque,
// ) Result {
//     if (comptime std.debug.runtime_safety) { ... }
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| rowGetTyped(it, y, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
/// # Safety
/// `iterator_` must be non-null and live; `y` in bounds; `out` must point to
/// writable storage of the type required by `data`.
#[inline]
unsafe fn row_get_dispatch(
    iterator_: RowIterator,
    y: usize,
    data: RowData,
    out: *mut c_void,
) -> Result {
    match data {
        // .invalid => .invalid_value,
        RowData::Invalid => Result::InvalidValue,
        // SAFETY: `out` validity/type is the caller's contract per `data`.
        _ => unsafe { row_get_typed(iterator_, y, data, out) },
    }
}

// fn rowGetTyped(it: *RowIteratorWrapper, y: usize, comptime data: RowData, out: *data.OutType()) Result {
//     switch (data) { ... }
//     return .success;
// }
/// # Safety
/// `iterator_` must be non-null and live; `y` in bounds; `out` must point to
/// writable storage of the type required by `data`.
unsafe fn row_get_typed(iterator_: RowIterator, y: usize, data: RowData, out: *mut c_void) -> Result {
    // SAFETY: `iterator_` is a live wrapper (caller contract).
    let it = unsafe { &*iterator_ };
    // SAFETY: `it.rows` is a valid slice pointer installed by `get`; `y` is
    // bounds-checked by the caller (`row_get`/`row_get_checked`/`row_get_multi`).
    let row = unsafe { &(*it.rows)[y] };
    match data {
        // .invalid => return .invalid_value,
        RowData::Invalid => return Result::InvalidValue,
        // .dirty => out.* = it.dirty[y],
        RowData::Dirty => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = row.dirty };
        }
        // .raw => out.* = it.raws[y].cval(),
        RowData::Raw => {
            // SAFETY: `out` points to a writable `row.CRow` (`u64`).
            unsafe { *(out as *mut row::CRow) = row.raw.cval() };
        }
        // .cells => {
        //     const cells = out.* orelse return .invalid_value;
        //     const cell_data = it.cells[y].slice();
        //     cells.* = .{ .alloc = cells.alloc, .x = position_none, .raws = ..., .graphemes = ..., .styles = ..., .selection = it.selection[y], .palette = it.palette };
        // },
        RowData::Cells => {
            // const cells = out.* orelse return .invalid_value;
            // SAFETY: `out` points to a writable `RowCells`.
            let cells_ptr = unsafe { *(out as *mut RowCells) };
            if cells_ptr.is_null() {
                return Result::InvalidValue;
            }
            // SAFETY: `cells_ptr` is a live `row_cells_new`-returned wrapper.
            let cells = unsafe { &mut *cells_ptr };
            // cells.* = .{ ... };
            cells.x = POSITION_NONE;
            cells.cells = &row.cells as *const CellList;
            cells.selection = row.selection;
            cells.palette = it.palette;
        }
        // .selection => {
        //     const out_size = out.size;
        //     if (out_size < @sizeOf(RowSelection)) return .invalid_value;
        //     const sel = it.selection[y] orelse return .no_value;
        //     out.start_x = sel[0];
        //     out.end_x = sel[1];
        // },
        RowData::Selection => {
            // SAFETY: `out` points to a writable `RowSelection`.
            let out_sel = unsafe { &mut *(out as *mut RowSelection) };
            // const out_size = out.size;
            let out_size = out_sel.size;
            // if (out_size < @sizeOf(RowSelection)) return .invalid_value;
            if out_size < std::mem::size_of::<RowSelection>() {
                return Result::InvalidValue;
            }
            // const sel = it.selection[y] orelse return .no_value;
            let sel = match row.selection {
                Some(sel) => sel,
                None => return Result::NoValue,
            };
            // out.start_x = sel[0];
            out_sel.start_x = sel[0];
            // out.end_x = sel[1];
            out_sel.end_x = sel[1];
        }
        // .cells_raw => {
        //     const raws: []const page.Cell = it.cells[y].items(.raw);
        //     out.* = .{
        //         .ptr = @ptrCast(raws.ptr),
        //         .len = raws.len,
        //     };
        // },
        RowData::CellsRaw => {
            // PORT NOTE (reshape): the raw column is the row's `CellList.raw()`
            // slice; `page.Cell` is `#[repr(transparent)]` over the `u64` cval
            // (`CCell`), so `@ptrCast(raws.ptr)` is the pointer cast below.
            let raws: &[page::Cell] = row.cells.raw();
            // SAFETY: `out` points to a writable `cell_c::CellsView`.
            unsafe {
                *(out as *mut crate::cell::CellsView) = crate::cell::CellsView {
                    ptr: raws.as_ptr() as *const crate::cell::CCell,
                    len: raws.len(),
                }
            };
        }
    }

    Result::Success
}

// pub fn row_set(iterator_: RowIterator, option: RowOption, value: ?*const anyopaque) ... Result {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(RowOption, ...) catch { log.warn(...); return .invalid_value; } }
//     return switch (option) {
//         inline else => |comptime_option| rowSetTyped(iterator_, comptime_option, @ptrCast(@alignCast(value orelse return .invalid_value))),
//     };
// }
//
// PORT NOTE: the `intToEnum(RowOption, ...)` guard lives in the `extern "C"`
// wrapper in `lib.rs`.
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
pub unsafe fn row_set(iterator_: RowIterator, option: RowOption, value: *const c_void) -> Result {
    // value orelse return .invalid_value
    if value.is_null() {
        return Result::InvalidValue;
    }
    match option {
        // SAFETY: `value` is non-null and of the type required by `option`.
        RowOption::Dirty => unsafe { row_set_typed(iterator_, option, value) },
    }
}

// fn rowSetTyped(iterator_: RowIterator, comptime option: RowOption, value: *const option.InType()) Result {
//     const it = iterator_ orelse return .invalid_value;
//     const y = it.y;
//     if (y >= it.raws.len) return .invalid_value;
//     switch (option) { .dirty => it.dirty[y] = value.* }
//     return .success;
// }
/// # Safety
/// `value` must point to a value of the type required by `option`.
unsafe fn row_set_typed(iterator_: RowIterator, option: RowOption, value: *const c_void) -> Result {
    // const it = iterator_ orelse return .invalid_value;
    if iterator_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `iterator_` is a live wrapper (caller contract).
    let it = unsafe { &mut *iterator_ };
    // const y = it.y;
    // if (y >= it.raws.len) return .invalid_value;
    let y = it.y;
    if y >= it.rows.len() {
        return Result::InvalidValue;
    }
    match option {
        // .dirty => it.dirty[y] = value.*,
        RowOption::Dirty => {
            // PORT NOTE: the reference writes through the `dirty` per-field slice
            // (`it.dirty[y] = value.*`). The Rust iterator stores a `*mut [Row]`
            // with write provenance (installed from the `Vec<Row>` buffer root in
            // `get`); we write `row_data[y].dirty` through it. This mutates the
            // `RenderState.row_data` the iterator points into, exactly as the
            // reference slice write does.
            // SAFETY: `value` points to a readable `bool`.
            let v = unsafe { *(value as *const bool) };
            // SAFETY: `it.rows` carries the `Vec<Row>` buffer-root write
            // provenance; the backing storage is uniquely owned by the live
            // `RenderStateWrapper` and no other reference into it is live
            // during this call. `y` is bounds-checked above.
            unsafe { (*it.rows)[y].dirty = v };
        }
    }

    Result::Success
}

#[cfg(test)]
mod tests;
