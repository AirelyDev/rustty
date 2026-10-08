//! PORTS: ghostty src/terminal/c/terminal.zig — the `KittyGraphics` /
//! `TerminalScreen` / `TerminalScrollbar` aliases, the `TerminalData` enum,
//! `get`, `get_multi`, and `getTyped`.

use super::*;

/// C: GhosttyKittyGraphics -> RusttyKittyGraphics (R2)
// pub const KittyGraphics = kitty_gfx_c.KittyGraphics;
pub type KittyGraphics = KittyGraphicsHandle;

/// C: GhosttyTerminalScreen -> RusttyTerminalScreen (R2)
// pub const TerminalScreen = ScreenSet.Key;
pub type TerminalScreen = ScreenSetKey;

/// C: GhosttyTerminalScrollbar -> RusttyTerminalScrollbar (R2)
// pub const TerminalScrollbar = PageList.Scrollbar.C;
pub type TerminalScrollbar = ScrollbarC;

/// C: GhosttyTerminalMemoryUsage -> RusttyTerminalMemoryUsage (R2)
///
/// This is a sized struct, so new fields may only be added to the end.
/// The figures for each screen are separate fields rather than a nested
/// struct per screen, because only the outermost struct can grow. Add a
/// new per-screen field as a primary and alternate pair.
// pub const TerminalMemoryUsage = extern struct {
//     size: usize = @sizeOf(TerminalMemoryUsage),
//     compression_supported: bool = false,
//     primary_pages: u64 = 0,
//     ...
//     alternate_image_bytes: u64 = 0,
// };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalMemoryUsage {
    pub size: usize,
    pub compression_supported: bool,
    pub primary_pages: u64,
    pub primary_virtual_bytes: u64,
    pub primary_resident_bytes: u64,
    pub primary_compressed_pages: u64,
    pub primary_compressed_bytes: u64,
    pub primary_image_bytes: u64,
    pub alternate_pages: u64,
    pub alternate_virtual_bytes: u64,
    pub alternate_resident_bytes: u64,
    pub alternate_compressed_pages: u64,
    pub alternate_compressed_bytes: u64,
    pub alternate_image_bytes: u64,
}

// PORT NOTE: the reference's field defaults (`.{}`); Rust spells them as
// `Default`.
impl Default for TerminalMemoryUsage {
    fn default() -> TerminalMemoryUsage {
        TerminalMemoryUsage {
            size: size_of::<TerminalMemoryUsage>(),
            compression_supported: false,
            primary_pages: 0,
            primary_virtual_bytes: 0,
            primary_resident_bytes: 0,
            primary_compressed_pages: 0,
            primary_compressed_bytes: 0,
            primary_image_bytes: 0,
            alternate_pages: 0,
            alternate_virtual_bytes: 0,
            alternate_resident_bytes: 0,
            alternate_compressed_pages: 0,
            alternate_compressed_bytes: 0,
            alternate_image_bytes: 0,
        }
    }
}

impl TerminalMemoryUsage {
    /// Gather the memory usage of `t`. The size field is set to
    /// `caller_size` instead of our own size, so that copying the result
    /// into the caller's struct keeps the size they passed in.
    ///
    /// # Safety
    /// `t` must be a live terminal; no `&mut` of it or its screens may be
    /// live (the reads form shared borrows of each screen).
    // fn init(t: *const ZigTerminal, caller_size: usize) TerminalMemoryUsage {
    unsafe fn init(t: *const ZigTerminal, caller_size: usize) -> TerminalMemoryUsage {
        // var result: TerminalMemoryUsage = .{
        //     .size = caller_size,
        //     .compression_supported = terminal_mem.canReclaim(.strict),
        // };
        let mut result = TerminalMemoryUsage {
            size: caller_size,
            compression_supported: rustty_vt::mem::can_reclaim(
                rustty_vt::mem::DecommitMode::Strict,
            ),
            ..TerminalMemoryUsage::default()
        };

        // const primary = t.screens.get(.primary).?.memoryUsage();
        // SAFETY: `t` is live (caller contract); the primary screen always
        // exists.
        let primary = unsafe { (*(*t).screens.get(ScreenSetKey::Primary).unwrap()).memory_usage() };
        result.primary_pages = primary.pages.pages as u64;
        result.primary_virtual_bytes = primary.pages.virtual_bytes as u64;
        result.primary_resident_bytes = primary.pages.resident_bytes as u64;
        result.primary_compressed_pages = primary.pages.compressed_pages as u64;
        result.primary_compressed_bytes = primary.pages.compressed_bytes as u64;
        result.primary_image_bytes = primary.image_bytes as u64;

        // The alternate screen is created on first use. Until then its
        // fields stay zero.
        // if (t.screens.get(.alternate)) |screen| { ... }
        // SAFETY: as above.
        if let Some(screen) = unsafe { (*t).screens.get(ScreenSetKey::Alternate) } {
            // SAFETY: a live screen of the live terminal.
            let alternate = unsafe { (*screen).memory_usage() };
            result.alternate_pages = alternate.pages.pages as u64;
            result.alternate_virtual_bytes = alternate.pages.virtual_bytes as u64;
            result.alternate_resident_bytes = alternate.pages.resident_bytes as u64;
            result.alternate_compressed_pages = alternate.pages.compressed_pages as u64;
            result.alternate_compressed_bytes = alternate.pages.compressed_bytes as u64;
            result.alternate_image_bytes = alternate.image_bytes as u64;
        }

        // return result;
        result
    }
}

/// C: GhosttyTerminalData -> RusttyTerminalData (R2)
// pub const TerminalData = enum(c_int) { ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalData {
    Invalid = 0,
    Cols = 1,
    Rows = 2,
    CursorX = 3,
    CursorY = 4,
    CursorPendingWrap = 5,
    ActiveScreen = 6,
    CursorVisible = 7,
    KittyKeyboardFlags = 8,
    Scrollbar = 9,
    CursorStyle = 10,
    MouseTracking = 11,
    Title = 12,
    Pwd = 13,
    TotalRows = 14,
    ScrollbackRows = 15,
    WidthPx = 16,
    HeightPx = 17,
    ColorForeground = 18,
    ColorBackground = 19,
    ColorCursor = 20,
    ColorPalette = 21,
    ColorForegroundDefault = 22,
    ColorBackgroundDefault = 23,
    ColorCursorDefault = 24,
    ColorPaletteDefault = 25,
    KittyImageStorageLimit = 26,
    KittyImageMediumFile = 27,
    KittyImageMediumTempFile = 28,
    KittyImageMediumSharedMem = 29,
    KittyGraphics = 30,
    Selection = 31,
    ViewportActive = 32,
    VtProcessingError = 33,
    ScrollbackMaxBytes = 34,
    ScrollbackMaxLines = 35,
    /// Output type: `usize` (oracle d7bb4b863).
    ContinuationMaxBytes = 36,
    /// Output type: `ModeConfig` (input/output — oracle cfc19e805).
    Mode = 37,
    /// Output type: `bool` (oracle a69a591af).
    VtGround = 38,
    /// Output type: `bool` (oracle bdb566068).
    CursorAtPrompt = 39,
    /// Output type: `usize` (oracle 5350d4a5f/70f006575).
    ClipboardWriteMaxBytes = 40,
    /// The mouse pointer shape requested by the application through OSC 22.
    ///
    /// Initially `mouse::Shape::Text`. Excludes host hover overrides.
    ///
    /// Output type: `mouse::Shape` (oracle 78494bad8/67fb0c940).
    MouseShape = 41,
    /// Output type: `TerminalMemoryUsage` (oracle 8834e161c).
    MemoryUsage = 42,
    // /// Output type expected for querying the data of the given kind.
    // pub fn OutType(comptime self: TerminalData) type {
    //     return switch (self) {
    //         .invalid => void,
    //         .cols, .rows, .cursor_x, .cursor_y => size.CellCountInt,
    //         .cursor_pending_wrap,
    //         .cursor_visible,
    //         .mouse_tracking,
    //         .viewport_active,
    //         .vt_processing_error,
    //         .vt_ground,
    //         .cursor_at_prompt,
    //         => bool,
    //         .mouse_shape => mouse.Shape,
    //         .active_screen => TerminalScreen,
    //         .kitty_keyboard_flags => u8,
    //         .scrollbar => TerminalScrollbar,
    //         .cursor_style => style_c.Style,
    //         .title, .pwd => lib.String,
    //         .total_rows,
    //         .scrollback_rows,
    //         .scrollback_max_bytes,
    //         .scrollback_max_lines,
    //         .continuation_max_bytes,
    //         .clipboard_write_max_bytes,
    //         => usize,
    //         .width_px, .height_px => u32,
    //         .color_foreground,
    //         .color_background,
    //         .color_cursor,
    //         .color_foreground_default,
    //         .color_background_default,
    //         .color_cursor_default,
    //         => color.RGB.C,
    //         .color_palette, .color_palette_default => color.PaletteC,
    //         .kitty_image_storage_limit => u64,
    //         .kitty_image_medium_file,
    //         .kitty_image_medium_shared_mem,
    //         => bool,
    //         .kitty_image_medium_temp_file => lib.String,
    //         .kitty_graphics => KittyGraphics,
    //         .selection => selection_c.CSelection,
    //         .mode => ModeConfig,
    //         .memory_usage => TerminalMemoryUsage,
    //     };
    // }
    // (`.continuation_max_bytes => usize` rides Bundle S.)
    //
    // PORT NOTE: `OutType` is a comptime type-returning fn (the output type expected
    // for querying each data kind). rustty erases `out` to `*mut c_void` at the FFI
    // boundary and writes it back as the per-arm type inside `get_typed`, so this
    // comptime projection has no Rust analog; it is preserved verbatim as the
    // reference contract for each arm's expected pointee type.
}

// D11 (P3-W6 T2), see `render::Data::from_c_int`: proves reading this value out
// of C memory as a `c_int` leaves the ABI unchanged (the published header
// declares `const RusttyTerminalData *keys` for `get_multi`).
const _: () = assert!(std::mem::size_of::<TerminalData>() == std::mem::size_of::<c_int>());

// PORT NOTE: range-checked `c_int -> enum` projection for the terminal get
// guard (the `std.meta.intToEnum` guard on the raw `c_int` crossing the C
// boundary). `TerminalData` is contiguous TODAY but has not always been.
//
// It went non-contiguous at `P3-W3` T5 (oracle `a27e04e8f` added
// `scrollback_max_bytes = 34` / `scrollback_max_lines = 35` while `33`,
// `vt_processing_error`, was not yet ported) and is CONTIGUOUS AGAIN as of
// `P3-W4` T5, which lands 33 (oracle `439d35e27`). A plain `0..N` range check
// would `transmute` a hole into an UNINHABITED enum value — instant UB — so
// holes are rejected explicitly. The MECHANISM stays even though the list is
// now empty: 36+ may reopen a hole, and re-deriving it from scratch under time
// pressure is exactly how that UB gets reintroduced.
// Bundle M reopened the holes at 36/38; Bundle S closed them again
// (`continuation_max_bytes` = 36, `vt_ground` = 38) — the list is empty but
// the MECHANISM stays.
const TERMINAL_DATA_MAX: c_int = 42;
const TERMINAL_DATA_HOLES: &[c_int] = &[];

impl TerminalData {
    /// Narrow an inbound C integer to a `TerminalData`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range (or on a hole).
    /// `Invalid = 0` is IN range and keeps its `.invalid_value` disposition
    /// inside `get`.
    // PORT NOTE (W35-B08): the reference's guard is
    // `std.enums.fromInt(TerminalData, @intFromEnum(data))`. This used to live
    // in `lib.rs` (`terminal_data_from_c_int`), reachable only from the single
    // `rustty_terminal_get` wrapper; it lives here now so `get_multi` reaches
    // it per array element too, exactly as the reference's `get_multi` reaches
    // the guard by calling `get`.
    pub fn from_c_int(v: c_int) -> Option<TerminalData> {
        if (0..=TERMINAL_DATA_MAX).contains(&v) && !TERMINAL_DATA_HOLES.contains(&v) {
            // SAFETY: `v` is a valid `TerminalData` discriminant (checked
            // above).
            Some(unsafe { std::mem::transmute::<c_int, TerminalData>(v) })
        } else {
            None
        }
    }
}

// pub fn get(terminal_: Terminal, data: TerminalData, out: ?*anyopaque) ... {
//     if (comptime std.debug.runtime_safety) {
//         _ = std.meta.intToEnum(TerminalData, @intFromEnum(data)) catch { ... return .invalid_value; };
//     }
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| getTyped(terminal_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
//
// PORT NOTE: the reference `intToEnum` runtime-safety guard lives on the raw
// `c_int` crossing the C boundary (`get_checked` below, shared by the
// `extern "C"` wrapper in lib.rs AND by `get_multi`); a valid `TerminalData`
// here cannot be out of range. The `inline else => getTyped` comptime dispatch
// becomes a direct `get_typed` call (the typed `out` cast is performed per-arm
// inside `get_typed`).
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out`, if non-null,
/// must point to storage of the type required by `data` (`TerminalData::out_type`).
pub unsafe fn get(terminal_: Terminal, data: TerminalData, out: *mut c_void) -> Result {
    // const out_ptr = out orelse return .invalid_value;
    if out.is_null() {
        return Result::InvalidValue;
    }
    // .invalid => .invalid_value,
    if matches!(data, TerminalData::Invalid) {
        return Result::InvalidValue;
    }
    // inline else => |comptime_data| getTyped(terminal_, comptime_data, @ptrCast(@alignCast(out_ptr)))
    unsafe { get_typed(terminal_, data, out) }
}

// PORT NOTE (W35-B08): the reference carries the runtime-safety guard INSIDE
// `get`, so every caller reaches it — including `get_multi`, which calls `get`
// per array element. rustty's `get` takes the already-narrowed `TerminalData`
// (a `#[repr(C)]` enum cannot even hold an out-of-range tag), so the guard
// lives here, in the raw-`c_int` entry point shared by the `extern "C"` wrapper
// in `lib.rs` AND by `get_multi` — the same "every path goes through the guard"
// shape as the reference.
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `out`, if non-null,
/// must point to storage of the type required by `data`.
pub unsafe fn get_checked(terminal_: Terminal, data: c_int, out: *mut c_void) -> Result {
    // _ = std.meta.intToEnum(TerminalData, @intFromEnum(data)) catch {
    //     log.warn("terminal_get invalid data value={d}", .{@intFromEnum(data)});
    //     return .invalid_value;
    // };
    let Some(d) = TerminalData::from_c_int(data) else {
        crate::sys::log_fn(
            crate::sys::StdLevel::Warn,
            LOG_SCOPE,
            format_args!("terminal_get invalid data value={data}"),
        );
        return Result::InvalidValue;
    };
    // SAFETY: `terminal_`/`out` validity is the caller's contract.
    unsafe { get(terminal_, d, out) }
}

// fn getTyped(terminal_: Terminal, comptime data: TerminalData, out: *data.OutType()) Result {
//     const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
//     switch (data) { ... }
//     return .success;
// }
//
// PORT NOTE: the reference is a `comptime`-monomorphized switch whose `out` is
// typed per arm (`*data.OutType()`). rustty erases `out` to `*mut c_void` and
// casts to the arm's concrete pointer type before the write. Arms whose
// `OutType` projects a vt-c type (`color.RGB.C`/`PaletteC`, `style_c.Style`,
// `selection_c.CSelection`, kitty image/graphics) landed with the get/set
// sub-batch — every arm below is live (VALIDATION_REPORT M3).
//
// # Safety
// `terminal_`, if non-null, is a live wrapper; `out` points at storage of the
// type required by `data`.
unsafe fn get_typed(terminal_: Terminal, data: TerminalData, out: *mut c_void) -> Result {
    // const t: *ZigTerminal = (terminal_ orelse return .invalid_value).terminal;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // PORT NOTE (SOUNDNESS): the reference reads through `*ZigTerminal` and its
    // `screens.active: *Screen`. We mirror that with RAW pointers and never form
    // a `&mut ZigTerminal`/`&mut Screen`: the stream handler aliases this same
    // terminal via a stored raw pointer (created in `vt_handler`), and a Unique
    // (`&mut`) retag over the terminal would invalidate that aliasing tag and
    // make the next `vt_write` UB (the rust-mut-args-strongly-protected rule).
    // All reads here are shared (`&`/scalar) through the raw ptr; the active
    // screen is reached via the raw `screens.active` pointer.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };

    match data {
        // .invalid => return .invalid_value,
        TerminalData::Invalid => return Result::InvalidValue,
        // .cols => out.* = t.cols,
        TerminalData::Cols => unsafe { *(out as *mut size::CellCountInt) = (*t).cols },
        // .rows => out.* = t.rows,
        TerminalData::Rows => unsafe { *(out as *mut size::CellCountInt) = (*t).rows },
        // .cursor_x => out.* = t.screens.active.cursor.x,
        TerminalData::CursorX => unsafe { *(out as *mut size::CellCountInt) = (*active).cursor.x },
        // .cursor_y => out.* = t.screens.active.cursor.y,
        TerminalData::CursorY => unsafe { *(out as *mut size::CellCountInt) = (*active).cursor.y },
        // .cursor_pending_wrap => out.* = t.screens.active.cursor.pending_wrap,
        TerminalData::CursorPendingWrap => unsafe {
            *(out as *mut bool) = (*active).cursor.pending_wrap
        },
        // .active_screen => out.* = t.screens.active_key,
        TerminalData::ActiveScreen => unsafe {
            *(out as *mut TerminalScreen) = (*t).screens.active_key
        },
        // .cursor_visible => out.* = t.modes.get(.cursor_visible),
        TerminalData::CursorVisible => unsafe {
            *(out as *mut bool) = (*t).modes.get(modes::Mode::CursorVisible)
        },
        // .kitty_keyboard_flags => out.* = @as(u8, t.screens.active.kitty_keyboard.current().int()),
        TerminalData::KittyKeyboardFlags => unsafe {
            *(out as *mut u8) = (*active).kitty_keyboard.current().int()
        },
        // .mouse_tracking => out.* = t.modes.get(.mouse_event_x10) or ... or any,
        TerminalData::MouseTracking => unsafe {
            *(out as *mut bool) = (*t).modes.get(modes::Mode::MouseEventX10)
                || (*t).modes.get(modes::Mode::MouseEventNormal)
                || (*t).modes.get(modes::Mode::MouseEventButton)
                || (*t).modes.get(modes::Mode::MouseEventAny)
        },
        // .total_rows => out.* = t.screens.active.pages.total_rows,
        TerminalData::TotalRows => unsafe { *(out as *mut usize) = (*active).pages.total_rows },
        // .scrollback_rows => out.* = t.screens.active.pages.total_rows - t.rows,
        TerminalData::ScrollbackRows => {
            let total = unsafe { (*active).pages.total_rows };
            let rows = unsafe { (*t).rows } as usize;
            unsafe { *(out as *mut usize) = total - rows };
        }
        // .width_px => out.* = t.width_px,
        TerminalData::WidthPx => unsafe { *(out as *mut u32) = (*t).width_px },
        // .height_px => out.* = t.height_px,
        TerminalData::HeightPx => unsafe { *(out as *mut u32) = (*t).height_px },
        // .viewport_active => out.* = t.screens.active.pages.viewport == .active,
        TerminalData::ViewportActive => unsafe {
            *(out as *mut bool) = matches!(
                (*active).pages.viewport,
                rustty_vt::pagelist::Viewport::Active
            )
        },
        // .vt_processing_error => out.* = wrapper.stream.handler.semantic_failure,
        TerminalData::VtProcessingError => unsafe {
            *(out as *mut bool) = (*handler_ptr(terminal_)).semantic_failure
        },

        // .scrollback_max_bytes => {
        //     const max = t.screens.get(.primary).?.pages.limits.bytes.explicit;
        //     if (max == std.math.maxInt(usize)) return .no_value;
        //     out.* = max;
        // },
        TerminalData::ScrollbackMaxBytes => {
            let primary: *mut rustty_vt::screen::Screen = unsafe {
                (*t).screens
                    .get(rustty_vt::screen_set::Key::Primary)
                    .unwrap()
            };
            let max = unsafe { (*primary).pages.limits.bytes.explicit };
            if max == usize::MAX {
                return Result::NoValue;
            }
            unsafe { *(out as *mut usize) = max };
        }
        // .scrollback_max_lines => { ... same, .lines.explicit ... },
        TerminalData::ScrollbackMaxLines => {
            let primary: *mut rustty_vt::screen::Screen = unsafe {
                (*t).screens
                    .get(rustty_vt::screen_set::Key::Primary)
                    .unwrap()
            };
            let max = unsafe { (*primary).pages.limits.lines.explicit };
            if max == usize::MAX {
                return Result::NoValue;
            }
            unsafe { *(out as *mut usize) = max };
        }

        // .continuation_max_bytes => out.* = continuationMaxBytes(wrapper),
        TerminalData::ContinuationMaxBytes => {
            // SAFETY: `terminal_` is a live wrapper (caller contract); `out`
            // points to a writable `usize`.
            unsafe { *(out as *mut usize) = continuation_max_bytes(terminal_) };
        }

        // .vt_ground => out.* = wrapper.stream.ground(),
        // (N452: by place, never a whole-stream `&self`; see `Stream::ground_raw`.)
        TerminalData::VtGround => unsafe {
            *(out as *mut bool) = Stream::ground_raw(std::ptr::addr_of!((*terminal_).stream))
        },

        // .mode => {
        //     const mode = out.toMode() orelse return .invalid_value;
        //     out.value = t.modes.get(mode);
        // },
        TerminalData::Mode => {
            // PORT NOTE: `.mode` is the one IN/OUT arm — the caller initializes
            // `mode` and receives `value`.
            // SAFETY: `out` points to a readable+writable `ModeConfig` (caller
            // contract; `get` already rejected a null `out`).
            let config = unsafe { &mut *(out as *mut ModeConfig) };
            let Some(mode) = config.to_mode() else {
                return Result::InvalidValue;
            };
            // SAFETY: shared read of the owned heap terminal's modes.
            config.value = unsafe { (*t).modes.get(mode) };
        }

        // .cursor_at_prompt => out.* = t.cursorIsAtPrompt(),
        TerminalData::CursorAtPrompt => {
            // SAFETY: `cursor_is_at_prompt` takes `&mut Terminal`; the transient
            // field-level autoref off the raw handle dies within the statement
            // (the rust-mut-args-strongly-protected shape).
            unsafe { *(out as *mut bool) = (*t).cursor_is_at_prompt() };
        }

        // .clipboard_write_max_bytes => out.* = wrapper.stream.handler.kitty_clipboard_write_max_bytes,
        TerminalData::ClipboardWriteMaxBytes => unsafe {
            *(out as *mut usize) = (*handler_ptr(terminal_)).kitty_clipboard_write_max_bytes
        },

        // .mouse_shape => out.* = t.mouse_shape,
        TerminalData::MouseShape => {
            // SAFETY: shared read of the owned heap terminal's mouse_shape
            // field via the raw ptr (no `&mut` retag of the handler-aliased
            // terminal).
            unsafe { *(out as *mut rustty_vt::mouse::Shape) = (*t).mouse_shape };
        }

        // .memory_usage => {
        //     // A smaller size means the caller doesn't have every field of
        //     // the first version of this struct, so reject it. A larger size
        //     // means the caller was built against a newer version with more
        //     // fields. We write only the fields we know and leave the rest
        //     // alone.
        //     //
        //     // When fields are added later, compare against the size of the
        //     // first version here, and copy only min(out.size, current size)
        //     // bytes.
        //     if (out.size < @sizeOf(TerminalMemoryUsage)) return .invalid_value;
        //     out.* = .init(t, out.size);
        // },
        TerminalData::MemoryUsage => {
            // A smaller size means the caller doesn't have every field of
            // the first version of this struct, so reject it. A larger size
            // means the caller was built against a newer version with more
            // fields. We write only the fields we know and leave the rest
            // alone.
            //
            // When fields are added later, compare against the size of the
            // first version here, and copy only min(out.size, current size)
            // bytes.
            let out = out as *mut TerminalMemoryUsage;
            // SAFETY: `out` points to a readable+writable
            // `TerminalMemoryUsage` (caller contract; `get` already rejected
            // a null `out`).
            let caller_size = unsafe { (*out).size };
            if caller_size < size_of::<TerminalMemoryUsage>() {
                return Result::InvalidValue;
            }
            // SAFETY: as above; `t` is the live terminal, read through shared
            // borrows of its screens only.
            unsafe { *out = TerminalMemoryUsage::init(t, caller_size) };
        }

        // .scrollbar => out.* = t.screens.active.pages.scrollbar().cval(),
        TerminalData::Scrollbar => unsafe {
            *(out as *mut TerminalScrollbar) = (*active).pages.scrollbar().cval()
        },
        // .cursor_style => out.* = .fromStyle(t.screens.active.cursor.style),
        TerminalData::CursorStyle => unsafe {
            *(out as *mut crate::style::Style) =
                crate::style::Style::from_style((*active).cursor.style)
        },
        // .title => {
        //     const title = t.getTitle() orelse "";
        //     out.* = .{ .ptr = title.ptr, .len = title.len };
        // },
        TerminalData::Title => {
            // SAFETY: shared read of the terminal's title via the raw ptr (no
            // `&mut` retag of the handler-aliased terminal).
            let title = unsafe { (*t).get_title() }.unwrap_or(b"");
            unsafe { *(out as *mut LibString) = LibString::init(title) };
        }
        // .pwd => {
        //     const pwd = t.getPwd() orelse "";
        //     out.* = .{ .ptr = pwd.ptr, .len = pwd.len };
        // },
        TerminalData::Pwd => {
            let pwd = unsafe { (*t).get_pwd() }.unwrap_or(b"");
            unsafe { *(out as *mut LibString) = LibString::init(pwd) };
        }
        // .color_foreground => out.* = (t.colors.foreground.get() orelse return .no_value).cval(),
        TerminalData::ColorForeground => {
            let rgb = match unsafe { (*t).colors.foreground.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_background => out.* = (t.colors.background.get() orelse return .no_value).cval(),
        TerminalData::ColorBackground => {
            let rgb = match unsafe { (*t).colors.background.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_cursor => out.* = (t.colors.cursor.get() orelse return .no_value).cval(),
        TerminalData::ColorCursor => {
            let rgb = match unsafe { (*t).colors.cursor.get() } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_foreground_default => out.* = (t.colors.foreground.default orelse return .no_value).cval(),
        TerminalData::ColorForegroundDefault => {
            let rgb = match unsafe { (*t).colors.foreground.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_background_default => out.* = (t.colors.background.default orelse return .no_value).cval(),
        TerminalData::ColorBackgroundDefault => {
            let rgb = match unsafe { (*t).colors.background.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_cursor_default => out.* = (t.colors.cursor.default orelse return .no_value).cval(),
        TerminalData::ColorCursorDefault => {
            let rgb = match unsafe { (*t).colors.cursor.default } {
                Some(c) => c,
                None => return Result::NoValue,
            };
            unsafe { *(out as *mut crate::color::RgbC) = rgb };
        }
        // .color_palette => out.* = color.paletteCval(&t.colors.palette.current),
        TerminalData::ColorPalette => unsafe {
            *(out as *mut crate::color::PaletteC) =
                crate::color::palette_cval(&(*t).colors.palette.current)
        },
        // .color_palette_default => out.* = color.paletteCval(t.colors.palette.original),
        TerminalData::ColorPaletteDefault => unsafe {
            *(out as *mut crate::color::PaletteC) =
                crate::color::palette_cval((*t).colors.palette.original())
        },
        // .kitty_image_storage_limit => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = @intCast(t.screens.active.kitty_images.total_limit);
        // },
        TerminalData::KittyImageStorageLimit => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut u64) = (*active).kitty_images.total_limit as u64 };
        }
        // .kitty_image_medium_file => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = t.screens.active.kitty_images.image_limits.file;
        // },
        TerminalData::KittyImageMediumFile => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut bool) = (*active).kitty_images.image_limits.file };
        }
        // .kitty_image_medium_temp_file => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     const dir = switch (t.screens.active.kitty_images.image_limits.temporary_file) {
        //         .enabled => |d| d.directory,
        //         .disabled => "",
        //     };
        //     out.* = .init(dir);
        // },
        TerminalData::KittyImageMediumTempFile => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            // PORT NOTE (byte-slice rule + borrow contract): `RusttyString` is
            // documented as BORROWED — the pointer is valid for as long as the
            // producing object is unchanged. Here it points into
            // `wrapper.tmp_dir_path` (via the active screen's borrowed
            // `DirBorrow`), exactly as upstream's does. `DirBorrow::as_slice`
            // is safe because the active screen's `directory`, when present,
            // borrows `(*terminal_).tmp_dir_path` (P7-W2 T3), which is still
            // live — nothing between the last `set` and this `get` can have
            // dropped or replaced it (the C API is single-threaded per
            // terminal, and only `set`/`free` touch that field).
            let dir: &[u8] = match unsafe { &(*active).kitty_images.image_limits.temporary_file } {
                TemporaryFileLimit::Enabled { directory } => directory.as_slice(),
                TemporaryFileLimit::Disabled => b"",
            };
            unsafe { *(out as *mut LibString) = LibString::init(dir) };
        }
        // .kitty_image_medium_shared_mem => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = t.screens.active.kitty_images.image_limits.shared_memory;
        // },
        TerminalData::KittyImageMediumSharedMem => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            unsafe { *(out as *mut bool) = (*active).kitty_images.image_limits.shared_memory };
        }
        // .kitty_graphics => {
        //     if (comptime !build_options.kitty_graphics) return .no_value;
        //     out.* = &t.screens.active.kitty_images;
        // },
        TerminalData::KittyGraphics => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::NoValue;
            }
            // out.* = &t.screens.active.kitty_images;
            // PORT NOTE: `KittyGraphics` is the opaque `*mut c_void` handle (see
            // kitty_graphics.rs); hand out the address of the active screen's
            // `kitty_images` storage.
            unsafe {
                *(out as *mut crate::kitty_graphics::KittyGraphics) =
                    core::ptr::addr_of_mut!((*active).kitty_images) as *mut c_void
            };
        }
        // .selection => out.* = selection_c.CSelection.fromZig(
        //     t.screens.active.selection orelse return .no_value,
        // ),
        TerminalData::Selection => {
            let sel = match unsafe { (*active).selection } {
                Some(s) => s,
                None => return Result::NoValue,
            };
            unsafe {
                *(out as *mut crate::selection::CSelection) =
                    crate::selection::CSelection::from_zig(sel)
            };
        }
    }

    Result::Success
}

// pub fn get_multi(terminal_, count, keys, values, out_written) ... {
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//     for (0..count) |i| {
//         const result = get(terminal_, k[i], v[i]);
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `keys`/`values`, if
/// non-null, must point to `count` valid elements; `out_written`, if non-null,
/// must be a valid writable pointer.
// PORT NOTE (W35-B08): `keys` is C memory typed `const RusttyTerminalData *` in
// the published header; each element is read as a raw `c_int` (ABI-identical —
// `TerminalData` is `#[repr(C)]`, asserted int-sized above) and range-checked
// by `get_checked`. Reading an element straight into a `TerminalData` would
// materialize an out-of-range discriminant, which is UB in Rust in EVERY build
// mode (where the reference merely loses its `runtime_safety` guard).
pub unsafe fn get_multi(
    terminal_: Terminal,
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

    // for (0..count) |i| {
    for i in 0..count {
        // const result = get(terminal_, k[i], v[i]);
        // SAFETY: `keys`/`values` point at `count` valid elements (caller
        // contract).
        let key = unsafe { *keys.add(i) };
        let value = unsafe { *values.add(i) };
        let result = unsafe { get_checked(terminal_, key, value) };
        // if (result != .success) { if (out_written) |w| w.* = i; return result; }
        if result != Result::Success {
            if !out_written.is_null() {
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    // if (out_written) |w| w.* = count;
    if !out_written.is_null() {
        unsafe { *out_written = count };
    }
    Result::Success
}
