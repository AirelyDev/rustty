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
use rustty_vt::style::{ColorTag, Fg, Style};

// const log = std.log.scoped(.render_state_c);

// const RenderStateWrapper = struct {
//     alloc: std.mem.Allocator,
//     state: renderpkg.RenderState = .empty,
// };
//
// PORT NOTE (D3 storage, internal — NOT C-ABI): like the terminal wrapper, the
// `renderpkg.RenderState` is owned through its own `allocator_api2::Global`; the
// C `alloc_` only selects the rustty-lib resolution. The wrapper is `Box`'d on
// the Rust heap and handed across the C ABI as a raw `*mut RenderStateWrapper`.
pub struct RenderStateWrapper {
    // PORT NOTE: the reference `alloc: std.mem.Allocator` is the C-resolved
    // allocator. rustty-vt's `RenderState` allocates exclusively through its own
    // `allocator_api2::Global`, so this field is the resolved handle kept for
    // parity (validating the C `alloc_`); the actual `deinit`/`update` use Global.
    pub alloc: Box<dyn rustty_lib::allocator::ZigAllocator>,
    pub state: ZigRenderState,
}

// const RowIteratorWrapper = struct {
//     alloc: std.mem.Allocator,
//
//     /// The current index (also y value) into the row list.
//     y: ?size.CellCountInt,
//
//     /// These are the raw pointers into the render state data.
//     raws: []const page.Row,
//     cells: []const std.MultiArrayList(renderpkg.RenderState.Cell),
//     selection: []const ?[2]size.CellCountInt,
//     dirty: []bool,
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

    /// The current index (also y value) into the row list.
    pub y: Option<size::CellCountInt>,

    /// These are the raw pointers into the render state data.
    // PORT NOTE: `raws`/`cells`/`selection`/`dirty` collapse to a raw `*mut
    // [Row]` (ptr+len) since rustty-vt's `RenderState` owns rows as `Vec<Row>`.
    // MUT provenance is required: the reference MultiArrayList slices are
    // writable and `row_set(.dirty)` writes through this pointer — a
    // `*const`-derived (SharedReadOnly) tag cast to `*mut` still forbids the
    // write under Stacked Borrows (Miri-caught in `render_row_get_set_dirty`).
    pub rows: *mut [RenderRow],

    /// The color palette from the render state, needed to resolve
    /// palette-indexed background colors on cells.
    pub palette: *const Palette,
}

// const RowCellsWrapper = struct {
//     alloc: std.mem.Allocator,
//     x: ?size.CellCountInt,
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
    pub x: Option<size::CellCountInt>,
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
pub type Dirty = RenderDirty;

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
// PORT NOTE (D3 lifecycle): `alloc.create` becomes a `Box::into_raw` on the Rust
// heap; `RenderState::empty()` supplies the `.state = .empty` field default. The
// `RenderStateWrapper` allocation cannot fail in the Rust global heap path
// (`Box::new` aborts on OOM), so the `error.OutOfMemory` arm is preserved for
// the C-ABI contract but is unreachable here.
fn new_(alloc_: Option<&CAllocator>) -> std::result::Result<RenderState, OutOfMemory> {
    let alloc = rustty_lib::allocator::default(alloc_);
    let ptr = Box::into_raw(Box::new(RenderStateWrapper {
        alloc,
        state: ZigRenderState::empty(),
    }));
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
    // const alloc = state.alloc;
    // state.state.deinit(alloc);
    // alloc.destroy(state);
    // SAFETY: `state_` is a live `new`-returned wrapper (caller contract).
    let mut state = unsafe { Box::from_raw(state_) };
    let gpa = allocator_api2::alloc::Global;
    state.state.deinit(gpa);
    // The `Box` frees the wrapper on drop (the reference `alloc.destroy`).
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

// pub fn get(state_: RenderState, data: Data, out: ?*anyopaque) ... Result {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.meta.intToEnum(Data, @intFromEnum(data)) catch {
//             log.warn("render_state_get invalid data value={d}", .{@intFromEnum(data)});
//             return .invalid_value;
//         };
//     }
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| getTyped(state_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
//
// PORT NOTE: the `intToEnum(Data, ...)` runtime-safety guard on the raw `c_int`
// crossing the C boundary lives in the `extern "C"` wrapper in `lib.rs`; by the
// time `get` is reached the `data` enum is already validated.
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get(state_: RenderState, data: Data, out: *mut c_void) -> Result {
    match data {
        Data::Invalid => Result::InvalidValue,
        // SAFETY: `out` validity/type is the caller's contract per `data`.
        _ => unsafe { get_typed(state_, data, out) },
    }
}

// pub fn get_multi(state_, count, keys, values, out_written) ... Result {
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//     for (0..count) |i| {
//         const result = get(state_, k[i], v[i]);
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
pub unsafe fn get_multi(
    state_: RenderState,
    count: usize,
    keys: *const Data,
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

    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // const result = get(state_, k[i], v[i]);
        let result = unsafe { get(state_, key, value) };
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

// fn getTyped(state_: RenderState, comptime data: Data, out: *data.OutType()) Result {
//     const state = state_ orelse return .invalid_value;
//     switch (data) { ... }
//     return .success;
// }
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
unsafe fn get_typed(state_: RenderState, data: Data, out: *mut c_void) -> Result {
    // const state = state_ orelse return .invalid_value;
    if state_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `state_` is a live wrapper (caller contract).
    let state = unsafe { &mut *state_ };
    match data {
        // .invalid => return .invalid_value,
        Data::Invalid => return Result::InvalidValue,
        // .cols => out.* = state.state.cols,
        Data::Cols => {
            // SAFETY: `out` points to a writable `size::CellCountInt`.
            unsafe { *(out as *mut size::CellCountInt) = state.state.cols };
        }
        // .rows => out.* = state.state.rows,
        Data::Rows => {
            // SAFETY: `out` points to a writable `size::CellCountInt`.
            unsafe { *(out as *mut size::CellCountInt) = state.state.rows };
        }
        // .dirty => out.* = state.state.dirty,
        Data::Dirty => {
            // SAFETY: `out` points to a writable `Dirty`.
            unsafe { *(out as *mut Dirty) = state.state.dirty };
        }
        // .row_iterator => {
        //     const it = out.* orelse return .invalid_value;
        //     const row_data = state.state.row_data.slice();
        //     it.* = .{ .alloc = it.alloc, .y = null, .raws = ..., .cells = ..., .selection = ..., .dirty = ..., .palette = &state.state.colors.palette };
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
            // it.* = .{ .alloc = it.alloc, .y = null, .raws = row_data.items(.raw), ... .palette = &state.state.colors.palette };
            it.y = None;
            // MUT slice: `row_set(.dirty)` later writes through this pointer,
            // so it must carry write provenance (see the field's PORT NOTE).
            it.rows = state.state.row_data.as_mut_slice() as *mut [RenderRow];
            it.palette = &state.state.colors.palette as *const Palette;
        }
        // .color_background => out.* = state.state.colors.background.cval(),
        Data::ColorBackground => {
            // SAFETY: `out` points to a writable `RGB`.
            unsafe { *(out as *mut RGB) = state.state.colors.background };
        }
        // .color_foreground => out.* = state.state.colors.foreground.cval(),
        Data::ColorForeground => {
            // SAFETY: `out` points to a writable `RGB`.
            unsafe { *(out as *mut RGB) = state.state.colors.foreground };
        }
        // .color_cursor => {
        //     const cursor = state.state.colors.cursor orelse return .invalid_value;
        //     out.* = cursor.cval();
        // },
        Data::ColorCursor => {
            let cursor = match state.state.colors.cursor {
                Some(cursor) => cursor,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `RGB`.
            unsafe { *(out as *mut RGB) = cursor };
        }
        // .color_cursor_has_value => out.* = state.state.colors.cursor != null,
        Data::ColorCursorHasValue => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = state.state.colors.cursor.is_some() };
        }
        // .color_palette => out.* = colorpkg.paletteCval(&state.state.colors.palette),
        Data::ColorPalette => {
            // SAFETY: `out` points to a writable `PaletteC` (`[RGB; 256]`).
            unsafe {
                *(out as *mut [RGB; 256]) = crate::color::palette_cval(&state.state.colors.palette)
            };
        }
        // .cursor_visual_style => out.* = CursorVisualStyle.fromCursorStyle(state.state.cursor.visual_style),
        Data::CursorVisualStyle => {
            // SAFETY: `out` points to a writable `CursorVisualStyle`.
            unsafe {
                *(out as *mut CursorVisualStyle) =
                    CursorVisualStyle::from_cursor_style(state.state.cursor.visual_style)
            };
        }
        // .cursor_visible => out.* = state.state.cursor.visible,
        Data::CursorVisible => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = state.state.cursor.visible };
        }
        // .cursor_blinking => out.* = state.state.cursor.blinking,
        Data::CursorBlinking => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = state.state.cursor.blinking };
        }
        // .cursor_password_input => out.* = state.state.cursor.password_input,
        Data::CursorPasswordInput => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = state.state.cursor.password_input };
        }
        // .cursor_viewport_has_value => out.* = state.state.cursor.viewport != null,
        Data::CursorViewportHasValue => {
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = state.state.cursor.viewport.is_some() };
        }
        // .cursor_viewport_x => { const vp = state.state.cursor.viewport orelse return .invalid_value; out.* = vp.x; },
        Data::CursorViewportX => {
            let vp = match &state.state.cursor.viewport {
                Some(vp) => vp,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `size::CellCountInt`.
            unsafe { *(out as *mut size::CellCountInt) = vp.x };
        }
        // .cursor_viewport_y => { const vp = ... orelse return .invalid_value; out.* = vp.y; },
        Data::CursorViewportY => {
            let vp = match &state.state.cursor.viewport {
                Some(vp) => vp,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `size::CellCountInt`.
            unsafe { *(out as *mut size::CellCountInt) = vp.y };
        }
        // .cursor_viewport_wide_tail => { const vp = ... orelse return .invalid_value; out.* = vp.wide_tail; },
        Data::CursorViewportWideTail => {
            let vp = match &state.state.cursor.viewport {
                Some(vp) => vp,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `bool`.
            unsafe { *(out as *mut bool) = vp.wide_tail };
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
    // SAFETY: `state_` is a live wrapper (caller contract).
    let state = unsafe { &mut *state_ };
    match option {
        // .dirty => state.state.dirty = value.*,
        SetOption::Dirty => {
            // SAFETY: `value` points to a readable `Dirty`.
            state.state.dirty = unsafe { *(value as *const Dirty) };
        }
    }
    Result::Success
}

// pub fn colors_get(state_: RenderState, out_colors_: ?*Colors) ... Result {
//     const state = state_ orelse return .invalid_value;
//     const out_colors = out_colors_ orelse return .invalid_value;
//     const out_size = out_colors.size;
//     if (out_size < @sizeOf(usize)) return .invalid_value;
//     const colors = state.state.colors;
//     if (lib.structSizedFieldFits(Colors, out_size, "background")) out_colors.background = colors.background.cval();
//     if (lib.structSizedFieldFits(Colors, out_size, "foreground")) out_colors.foreground = colors.foreground.cval();
//     if (colors.cursor) |cursor| { if (lib.structSizedFieldFits(Colors, out_size, "cursor")) out_colors.cursor = cursor.cval(); }
//     if (lib.structSizedFieldFits(Colors, out_size, "cursor_has_value")) out_colors.cursor_has_value = colors.cursor != null;
//     { const palette_offset = @offsetOf(Colors, "palette");
//       if (out_size > palette_offset) {
//         const available = out_size - palette_offset;
//         const max_entries = @min(colors.palette.len, available / @sizeOf(colorpkg.RGB.C));
//         for (0..max_entries) |i| out_colors.palette[i] = colors.palette[i].cval();
//       } }
//     return .success;
// }
/// # Safety
/// `out_colors_`, if non-null, must be a valid writable pointer to a `Colors`.
pub unsafe fn colors_get(state_: RenderState, out_colors_: *mut Colors) -> Result {
    // const state = state_ orelse return .invalid_value;
    if state_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `state_` is a live wrapper (caller contract).
    let state = unsafe { &*state_ };
    // const out_colors = out_colors_ orelse return .invalid_value;
    if out_colors_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `out_colors_` is a non-null valid writable `Colors`.
    let out_colors = unsafe { &mut *out_colors_ };
    // const out_size = out_colors.size;
    let out_size = out_colors.size;
    // if (out_size < @sizeOf(usize)) return .invalid_value;
    if out_size < std::mem::size_of::<usize>() {
        return Result::InvalidValue;
    }

    // const colors = state.state.colors;
    let colors = &state.state.colors;
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
    //         for (0..max_entries) |i| out_colors.palette[i] = colors.palette[i].cval();
    //     }
    // }
    {
        let palette_offset = colors_ty.offset_of("palette");
        if out_size > palette_offset {
            let available = out_size - palette_offset;
            let max_entries =
                std::cmp::min(colors.palette.len(), available / std::mem::size_of::<RGB>());
            for i in 0..max_entries {
                out_colors.palette[i] = colors.palette[i];
            }
        }
    }

    Result::Success
}

// pub fn row_iterator_new(alloc_: ?*const CAllocator, result: *RowIterator) ... Result {
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(RowIteratorWrapper) catch { result.* = null; return .out_of_memory; };
//     ptr.* = .{ .alloc, .y = undefined, .raws = undefined, .cells = undefined, .selection = undefined, .dirty = undefined, .palette = undefined };
//     result.* = ptr;
//     return .success;
// }
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `result` must be a
/// valid writable pointer to a `RowIterator`.
pub unsafe fn row_iterator_new(alloc_: Option<&CAllocator>, result: *mut RowIterator) -> Result {
    let alloc = rustty_lib::allocator::default(alloc_);
    // PORT NOTE: the reference `undefined` fields become null/`None` sentinels:
    // `y = None`, `rows = &[]` (empty slice), `palette = null`. Real values are
    // installed by `get(.., .row_iterator, ..)` before any observation.
    let ptr = Box::into_raw(Box::new(RowIteratorWrapper {
        alloc,
        y: None,
        // Empty placeholder slice (never dereferenced: `len == 0` makes
        // `row_iterator_next` return false; `get` installs the real slice).
        rows: std::ptr::slice_from_raw_parts_mut(
            std::ptr::NonNull::<RenderRow>::dangling().as_ptr(),
            0,
        ),
        palette: std::ptr::null(),
    }));
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
    // SAFETY: `iterator_` is a live `row_iterator_new`-returned wrapper.
    drop(unsafe { Box::from_raw(iterator_) });
}

// pub fn row_iterator_next(iterator_: RowIterator) ... bool {
//     const it = iterator_ orelse return false;
//     const next_y: size.CellCountInt = if (it.y) |y| y + 1 else 0;
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
    // const next_y: size.CellCountInt = if (it.y) |y| y + 1 else 0;
    let next_y: size::CellCountInt = match it.y {
        Some(y) => y + 1,
        None => 0,
    };
    // if (next_y >= it.raws.len) return false;
    // PORT NOTE: `<*const [T]>::len` reads the slice-pointer metadata without
    // dereferencing, so no `&*` autoref of the raw pointer is needed.
    let rows_len = it.rows.len();
    if (next_y as usize) >= rows_len {
        return false;
    }
    // it.y = next_y;
    it.y = Some(next_y);
    true
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
    // PORT NOTE: reference `undefined` fields become null/`None` sentinels;
    // real values are installed by `row_get(.., .cells, ..)`.
    // PORT NOTE (reshape): the sentinel is a pointer to a shared empty `CellList`
    // (len 0). `row_cells_next`/`select` must deref it to read the length, so it
    // must be a valid `CellList` (not a dangling pointer); real values are
    // installed by `row_get(.., .cells, ..)`.
    static EMPTY_CELLS: CellList = CellList::empty();
    let ptr = Box::into_raw(Box::new(RowCellsWrapper {
        alloc,
        x: None,
        cells: &EMPTY_CELLS as *const CellList,
        selection: None,
        palette: std::ptr::null(),
    }));
    // result.* = ptr;
    // SAFETY: `result` is a valid writable `RowCells` (caller contract).
    unsafe { *result = ptr };
    Result::Success
}

// pub fn row_cells_next(cells_: RowCells) ... bool {
//     const cells = cells_ orelse return false;
//     const next_x: size.CellCountInt = if (cells.x) |x| x + 1 else 0;
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
    // const next_x: size.CellCountInt = if (cells.x) |x| x + 1 else 0;
    let next_x: size::CellCountInt = match cells.x {
        Some(x) => x + 1,
        None => 0,
    };
    // if (next_x >= cells.raws.len) return false;
    // PORT NOTE (reshape): `cells.cells` is now a `*const CellList`, so the length
    // is read via a deref of the (valid) pointer's `raw` column.
    let cells_len = unsafe { (*cells.cells).len() };
    if (next_x as usize) >= cells_len {
        return false;
    }
    // cells.x = next_x;
    cells.x = Some(next_x);
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
    cells.x = Some(x);
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
    // SAFETY: `cells_` is a live `row_cells_new`-returned wrapper.
    drop(unsafe { Box::from_raw(cells_) });
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
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(RowCellsData, ...) catch { log.warn(...); return .invalid_value; } }
//     if (out == null) return .invalid_value;
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| rowCellsGetTyped(cells_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
//
// PORT NOTE: the `intToEnum(RowCellsData, ...)` guard lives in the `extern "C"`
// wrapper in `lib.rs`.
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn row_cells_get(cells_: RowCells, data: RowCellsData, out: *mut c_void) -> Result {
    // if (out == null) return .invalid_value;
    if out.is_null() {
        return Result::InvalidValue;
    }
    match data {
        // .invalid => .invalid_value,
        RowCellsData::Invalid => Result::InvalidValue,
        // SAFETY: `out` validity/type is the caller's contract per `data`.
        _ => unsafe { row_cells_get_typed(cells_, data, out) },
    }
}

// pub fn row_cells_get_multi(cells_, count, keys, values, out_written) ... Result {
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//     for (0..count) |i| { const result = row_cells_get(cells_, k[i], v[i]); if (result != .success) { if (out_written) |w| w.* = i; return result; } }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
pub unsafe fn row_cells_get_multi(
    cells_: RowCells,
    count: usize,
    keys: *const RowCellsData,
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

    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // const result = row_cells_get(cells_, k[i], v[i]);
        let result = unsafe { row_cells_get(cells_, key, value) };
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

// fn rowCellsGetTyped(cells_: RowCells, comptime data: RowCellsData, out: *data.OutType()) Result {
//     const cells = cells_ orelse return .invalid_value;
//     const x = cells.x orelse return .invalid_value;
//     const cell = cells.raws[x];
//     switch (data) { ... }
//     return .success;
// }
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
unsafe fn row_cells_get_typed(cells_: RowCells, data: RowCellsData, out: *mut c_void) -> Result {
    // const cells = cells_ orelse return .invalid_value;
    if cells_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `cells_` is a live wrapper (caller contract).
    let cells = unsafe { &*cells_ };
    // const x = cells.x orelse return .invalid_value;
    let x = match cells.x {
        Some(x) => x,
        None => return Result::InvalidValue,
    };
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
        // .style => out.* = if (cell.hasStyling()) style_c.Style.fromStyle(cells.styles[x]) else style_c.Style.fromStyle(.{}),
        RowCellsData::Style => {
            let s = if cell.has_styling() {
                style_c::Style::from_style(cell_list.style()[x as usize])
            } else {
                style_c::Style::from_style(Style::default())
            };
            // SAFETY: `out` points to a writable `style_c::Style`.
            unsafe { *(out as *mut style_c::Style) = s };
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
        //     const s: Style = if (cell.hasStyling()) cells.styles[x] else .{};
        //     const bg = s.bg(&cell, cells.palette) orelse return .invalid_value;
        //     out.* = bg.cval();
        // },
        RowCellsData::BgColor => {
            let s: Style = if cell.has_styling() {
                cell_list.style()[x as usize]
            } else {
                Style::default()
            };
            // SAFETY: `cells.palette` is a valid palette pointer installed by
            // `row_get`.
            let palette = unsafe { &*cells.palette };
            let bg = match s.bg(&cell, palette) {
                Some(bg) => bg,
                None => return Result::InvalidValue,
            };
            // SAFETY: `out` points to a writable `RGB`.
            unsafe { *(out as *mut RGB) = bg };
        }
        // .fg_color => {
        //     const s: Style = if (cell.hasStyling()) cells.styles[x] else .{};
        //     if (s.fg_color == .none) return .invalid_value;
        //     const fg = s.fg(.{ .default = .{}, .palette = cells.palette });
        //     out.* = fg.cval();
        // },
        RowCellsData::FgColor => {
            let s: Style = if cell.has_styling() {
                cell_list.style()[x as usize]
            } else {
                Style::default()
            };
            if matches!(s.fg_color.tag(), ColorTag::None) {
                return Result::InvalidValue;
            }
            // SAFETY: `cells.palette` is a valid palette pointer installed by
            // `row_get`.
            let palette = unsafe { &*cells.palette };
            // PORT NOTE: `.{ .default = .{}, .palette = cells.palette }` relies on
            // `Fg`'s field defaults: `default = RGB{0,0,0}` and `bold = null`.
            let fg = s.fg(Fg {
                default: RGB { r: 0, g: 0, b: 0 },
                palette,
                bold: None,
            });
            // SAFETY: `out` points to a writable `RGB`.
            unsafe { *(out as *mut RGB) = fg };
        }
        // .selected => out.* = if (cells.selection) |sel| x >= sel[0] and x <= sel[1] else false,
        RowCellsData::Selected => {
            let selected = match cells.selection {
                Some(sel) => x >= sel[0] && x <= sel[1],
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
//     out.len = 0;
//     if (!cell.hasText()) return .success;
//     var needed = std.unicode.utf8CodepointSequenceLength(cell.codepoint()) catch return .invalid_value;
//     for (extra) |cp| needed += std.unicode.utf8CodepointSequenceLength(cp) catch return .invalid_value;
//     out.len = needed;
//     if (out.ptr == null or out.cap < needed) return .out_of_space;
//     const buf = out.ptr.?[0..out.cap];
//     var i: usize = 0;
//     i += std.unicode.utf8Encode(cell.codepoint(), buf[i..]) catch return .invalid_value;
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
    // out.len = 0;
    out.len = 0;

    // if (!cell.hasText()) return .success;
    if !cell.has_text() {
        return Result::Success;
    }

    // var needed = std.unicode.utf8CodepointSequenceLength(cell.codepoint()) catch return .invalid_value;
    let cp0 = match char::from_u32(cell.codepoint()) {
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
    // i += std.unicode.utf8Encode(cell.codepoint(), buf[i..]) catch return .invalid_value;
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
// pub const RowData = enum(c_int) { invalid = 0, dirty = 1, raw = 2, cells = 3, selection = 4 };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowData {
    Invalid = 0,
    Dirty = 1,
    Raw = 2,
    Cells = 3,
    Selection = 4,
}

// Output type expected for querying the data of the given kind.
// PORT NOTE: `pub fn OutType(comptime self: RowData) type`:
//     .invalid => void,
//     .dirty => bool,
//     .raw => row.CRow,
//     .cells => RowCells,
//     .selection => RowSelection,
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
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(RowData, ...) catch { log.warn(...); return .invalid_value; } }
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| rowGetTyped(iterator_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
//
// PORT NOTE: the `intToEnum(RowData, ...)` guard lives in the `extern "C"`
// wrapper in `lib.rs`.
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn row_get(iterator_: RowIterator, data: RowData, out: *mut c_void) -> Result {
    match data {
        // .invalid => .invalid_value,
        RowData::Invalid => Result::InvalidValue,
        // SAFETY: `out` validity/type is the caller's contract per `data`.
        _ => unsafe { row_get_typed(iterator_, data, out) },
    }
}

// pub fn row_get_multi(iterator_, count, keys, values, out_written) ... Result { ... }
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
pub unsafe fn row_get_multi(
    iterator_: RowIterator,
    count: usize,
    keys: *const RowData,
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

    for i in 0..count {
        // SAFETY: caller guarantees `keys`/`values` have `count` elements.
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        // const result = row_get(iterator_, k[i], v[i]);
        let result = unsafe { row_get(iterator_, key, value) };
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

// fn rowGetTyped(iterator_: RowIterator, comptime data: RowData, out: *data.OutType()) Result {
//     const it = iterator_ orelse return .invalid_value;
//     const y = it.y orelse return .invalid_value;
//     switch (data) { ... }
//     return .success;
// }
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
unsafe fn row_get_typed(iterator_: RowIterator, data: RowData, out: *mut c_void) -> Result {
    // const it = iterator_ orelse return .invalid_value;
    if iterator_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `iterator_` is a live wrapper (caller contract).
    let it = unsafe { &*iterator_ };
    // const y = it.y orelse return .invalid_value;
    let y = match it.y {
        Some(y) => y,
        None => return Result::InvalidValue,
    };
    // SAFETY: `it.rows` is a valid slice pointer installed by `get`; `y` is
    // bounds-checked by `row_iterator_next`.
    let row = unsafe { &(*it.rows)[y as usize] };
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
        //     cells.* = .{ .alloc = cells.alloc, .x = null, .raws = ..., .graphemes = ..., .styles = ..., .selection = it.selection[y], .palette = it.palette };
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
            cells.x = None;
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
//     const y = it.y orelse return .invalid_value;
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
    // const y = it.y orelse return .invalid_value;
    let y = match it.y {
        Some(y) => y,
        None => return Result::InvalidValue,
    };
    match option {
        // .dirty => it.dirty[y] = value.*,
        RowOption::Dirty => {
            // PORT NOTE: the reference writes through the `dirty` per-field slice
            // (`it.dirty[y] = value.*`). The Rust iterator stores a `*mut [Row]`
            // with write provenance (installed from `row_data.as_mut_slice()` in
            // `get`); we write `row_data[y].dirty` through it. This mutates the
            // `RenderState.row_data` the iterator points into, exactly as the
            // reference slice write does.
            // SAFETY: `value` points to a readable `bool`.
            let v = unsafe { *(value as *const bool) };
            // SAFETY: `it.rows` came from `RenderState.row_data.as_mut_slice()`
            // (write provenance); the backing storage is uniquely owned by the
            // live `RenderStateWrapper` and no other reference into it is live
            // during this call. `y` is bounds-checked by `row_iterator_next`.
            unsafe { (*it.rows)[y as usize].dirty = v };
        }
    }

    Result::Success
}

#[cfg(test)]
mod tests;
