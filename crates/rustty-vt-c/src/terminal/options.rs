//! PORTS: ghostty src/terminal/c/terminal.zig — the `Option` enum, `set`,
//! `setTyped`, and `TerminalCursorStyle`.

use super::*;

/// C: GhosttyTerminalOption -> RusttyTerminalOption (R2)
// pub const Option = enum(c_int) { ... };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Option_ {
    Userdata = 0,
    WritePty = 1,
    Bell = 2,
    Enquiry = 3,
    Xtversion = 4,
    TitleChanged = 5,
    SizeCb = 6,
    ColorScheme = 7,
    DeviceAttributes = 8,
    Title = 9,
    Pwd = 10,
    ColorForeground = 11,
    ColorBackground = 12,
    ColorCursor = 13,
    ColorPalette = 14,
    KittyImageStorageLimit = 15,
    KittyImageMediumFile = 16,
    KittyImageMediumTempFile = 17,
    KittyImageMediumSharedMem = 18,
    ApcMaxBytes = 19,
    ApcMaxBytesKitty = 20,
    Selection = 21,
    DefaultCursorStyle = 22,
    DefaultCursorBlink = 23,
    GlyphProtocol = 24,
    PwdChanged = 25,
    ClipboardWrite = 26,
    ScrollbackMaxBytes = 27,
    ScrollbackMaxLines = 28,
    /// Input type: `DesktopNotificationFn` (oracle c3655ba25).
    DesktopNotification = 29,
    /// Input type: `ProgressReportFn` (oracle 47d602c42).
    ProgressReport = 30,
    /// Input type: `*const usize` (oracle d7bb4b863).
    ContinuationMaxBytes = 31,
    /// Input type: `*const bool` (oracle 38e891e6c).
    TitleReport = 32,
    /// Input type: `*const ModeConfig` (oracle cfc19e805).
    ModeDefault = 33,
    /// Input type: `*const ModeConfig` (oracle cfc19e805).
    Mode = 34,
    /// Input type: `UnknownSequenceFn` (oracle 6b990de5b).
    UnknownSequence = 35,
    /// Input type: `*const usize` (oracle 6b990de5b).
    UnknownMaxBytes = 36,
    /// Input type: `*const lib.String` (oracle 74efadb44).
    TerminfoName = 37,
    /// Input type: `ClipboardReadFn` (oracle e03475c0c).
    ClipboardRead = 38,
    /// Input type: `*const usize` (oracle 5350d4a5f/70f006575).
    ClipboardWriteMaxBytes = 39,
    /// Input type: `*const bool` (oracle c55f213aa).
    ResizePullScrollback = 40,
    /// Input type: `RenderHoldFn` (oracle e50779498).
    RenderHold = 41,
    // /// Input type expected for setting the option.
    // pub fn InType(comptime self: Option) type {
    //     return switch (self) {
    //         .userdata => ?*const anyopaque,
    //         .write_pty => ?Effects.WritePtyFn,
    //         .bell => ?Effects.BellFn,
    //         .color_scheme => ?Effects.ColorSchemeFn,
    //         .device_attributes => ?Effects.DeviceAttributesFn,
    //         .enquiry => ?Effects.EnquiryFn,
    //         .xtversion => ?Effects.XtversionFn,
    //         .title_changed => ?Effects.TitleChangedFn,
    //         .pwd_changed => ?Effects.PwdChangedFn,
    //         .size_cb => ?Effects.SizeFn,
    //         .clipboard_write => ?Effects.ClipboardWriteFn,
    //         .clipboard_read => ?Effects.ClipboardReadFn,
    //         .unknown_sequence => ?Effects.UnknownSequenceFn,
    //         .render_hold => ?Effects.RenderHoldFn,
    //         .title, .pwd, .terminfo_name => ?*const lib.String,
    //         .color_foreground, .color_background, .color_cursor => ?*const color.RGB.C,
    //         .color_palette => ?*const color.PaletteC,
    //         .kitty_image_storage_limit => ?*const u64,
    //         .kitty_image_medium_file,
    //         .kitty_image_medium_shared_mem,
    //         .glyph_protocol,
    //         .title_report,
    //         .resize_pull_scrollback,
    //         => ?*const bool,
    //         .kitty_image_medium_temp_file => ?*const lib.String,
    //         .apc_max_bytes,
    //         .apc_max_bytes_kitty,
    //         .scrollback_max_bytes,
    //         .scrollback_max_lines,
    //         .continuation_max_bytes,
    //         .unknown_max_bytes,
    //         .clipboard_write_max_bytes,
    //         => ?*const usize,
    //         .selection => ?*const selection_c.CSelection,
    //         .default_cursor_style => ?*const TerminalCursorStyle,
    //         .default_cursor_blink => ?*const bool,
    //         .mode, .mode_default => ?*const ModeConfig,
    //     };
    // }
    //
    // PORT NOTE: `InType` is a comptime type-returning fn (the input type expected
    // for setting each option). rustty erases `value` to `*const c_void` at the FFI
    // boundary and reads it back as the per-arm type inside `set_typed`, so this
    // comptime projection has no Rust analog; it is preserved verbatim as the
    // reference contract for each arm's expected pointee type.
}

/// Zig `@intCast` from a C-ABI `uint64_t` into a narrower destination, with the
/// out-of-range value taken to the destination's maximum instead of trapping.
///
/// PORT NOTE (task R13-P, `UPSTREAM-DEFECTS.md` UD63): generic over the
/// destination so the saturating arm is testable on a 64-bit host, where
/// `usize::try_from(u64)` can never fail. The only production caller is the
/// `kitty_image_storage_limit` setter.
pub(super) fn saturating_from_u64<T: TryFrom<u64>>(v: u64, max: T) -> T {
    T::try_from(v).unwrap_or(max)
}

// pub fn set(terminal_: Terminal, option: Option, value: ?*const anyopaque) ... {
//     if (comptime std.debug.runtime_safety) { _ = intToEnum(Option, ...) catch { ... return .invalid_value; }; }
//     const wrapper = terminal_ orelse return .invalid_value;
//     return switch (option) {
//         inline else => |comptime_option| setTyped(wrapper, comptime_option, @ptrCast(@alignCast(value))),
//     };
// }
//
// PORT NOTE: the `intToEnum(Option, ...)` runtime-safety guard on the raw `c_int`
// crossing the C boundary lives in the lib.rs `extern "C"` wrapper (the cell/key
// pattern); here `option` is already a valid `#[repr(C)] Option_`. The `inline
// else => setTyped` comptime dispatch becomes a direct `set_typed` call.
//
// PORT NOTE (formerly PARTIAL FILL): the `.selection` arm landed first (it is
// exercised by `selection.zig`'s `selection_format_alloc uses active
// selection` test); the get/set sub-batch has since landed every other option
// arm — no `todo!("vt-c set <option>")` remains (VALIDATION_REPORT M3).
/// # Safety
/// `terminal_`, if non-null, must be a live terminal handle; `value`, if
/// non-null, must point to storage of the type required by `option`
/// (`Option_::in_type`).
pub unsafe fn set(terminal_: Terminal, option: Option_, value: *const c_void) -> Result {
    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // inline else => |comptime_option| setTyped(wrapper, comptime_option, @ptrCast(@alignCast(value)))
    unsafe { set_typed(terminal_, option, value) }
}

// fn setTyped(wrapper: *TerminalWrapper, comptime option: Option, value: option.InType()) Result {
//     switch (option) { ...; .selection => { ... }; ... }
//     return .success;
// }
//
// # Safety
// `terminal_`, if non-null, is a live wrapper; `value` points at storage of the
// type required by `option`.
unsafe fn set_typed(terminal_: Terminal, option: Option_, value: *const c_void) -> Result {
    // PORT NOTE (SOUNDNESS): the reference is `setTyped(wrapper, ...)`, mutating
    // the public fields `wrapper.effects`, `wrapper.terminal.*`, and
    // `wrapper.stream.handler.*`. rustty reaches each through RAW pointers off
    // `terminal_` (the `*mut TerminalWrapper`) and forms a `&mut` only to the
    // disjoint sub-field it writes — never an overlapping `&mut TerminalWrapper`
    // or `&mut ZigTerminal` that would invalidate the stream handler's stored
    // alias of the terminal (the rust-mut-args-strongly-protected discipline).
    // `value`, for callback-fn arms, IS the C function pointer (the reference's
    // `?Effects.WritePtyFn` etc.); for data arms it points at the typed value.
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };

    match option {
        // .userdata => wrapper.effects.userdata = @constCast(value),
        Option_::Userdata => unsafe {
            (*terminal_).effects.userdata = value as *mut c_void;
        },
        // .write_pty => wrapper.effects.write_pty = value,
        Option_::WritePty => unsafe {
            (*terminal_).effects.write_pty =
                std::mem::transmute::<*const c_void, Option<WritePtyFn>>(value);
        },
        // .bell => wrapper.effects.bell = value,
        Option_::Bell => unsafe {
            (*terminal_).effects.bell = std::mem::transmute::<*const c_void, Option<BellFn>>(value);
        },
        // .color_scheme => wrapper.effects.color_scheme = value,
        Option_::ColorScheme => unsafe {
            (*terminal_).effects.color_scheme =
                std::mem::transmute::<*const c_void, Option<ColorSchemeFn>>(value);
        },
        // .device_attributes => wrapper.effects.device_attributes_cb = value,
        Option_::DeviceAttributes => unsafe {
            (*terminal_).effects.device_attributes_cb =
                std::mem::transmute::<*const c_void, Option<DeviceAttributesFn>>(value);
        },
        // .enquiry => wrapper.effects.enquiry = value,
        Option_::Enquiry => unsafe {
            (*terminal_).effects.enquiry =
                std::mem::transmute::<*const c_void, Option<EnquiryFn>>(value);
        },
        // .xtversion => wrapper.effects.xtversion = value,
        Option_::Xtversion => unsafe {
            (*terminal_).effects.xtversion =
                std::mem::transmute::<*const c_void, Option<XtversionFn>>(value);
        },
        // .title_changed => wrapper.effects.title_changed = value,
        Option_::TitleChanged => unsafe {
            (*terminal_).effects.title_changed =
                std::mem::transmute::<*const c_void, Option<TitleChangedFn>>(value);
        },
        // .pwd_changed => wrapper.effects.pwd_changed = value,
        Option_::PwdChanged => unsafe {
            (*terminal_).effects.pwd_changed =
                std::mem::transmute::<*const c_void, Option<PwdChangedFn>>(value);
        },
        // .size_cb => wrapper.effects.size_cb = value,
        Option_::SizeCb => unsafe {
            (*terminal_).effects.size_cb =
                std::mem::transmute::<*const c_void, Option<SizeFn>>(value);
        },
        // .render_hold => wrapper.effects.render_hold = value,
        Option_::RenderHold => unsafe {
            (*terminal_).effects.render_hold =
                std::mem::transmute::<*const c_void, Option<RenderHoldFn>>(value);
        },
        // .clipboard_write => {
        //     wrapper.effects.clipboard_write = value;
        //     wrapper.stream.handler.effects.clipboard_write = if (value != null)
        //         &Effects.clipboardWriteTrampoline
        //     else
        //         null;
        // },
        Option_::ClipboardWrite => unsafe {
            (*terminal_).effects.clipboard_write =
                std::mem::transmute::<*const c_void, Option<ClipboardWriteFn>>(value);
            // The trampoline is installed dynamically to preserve the
            // handler's null fast path (a null Zig-level effect makes Kitty
            // clipboard writes fail up front instead of spooling a
            // transaction that can never commit; see Effects).
            (*handler_ptr(terminal_)).effects.clipboard_write = if !value.is_null() {
                Some(Effects::clipboard_write_trampoline)
            } else {
                None
            };
        },
        // .clipboard_read => {
        //     wrapper.effects.clipboard_read = value;
        //     wrapper.stream.handler.effects.clipboard_read = if (value != null)
        //         &Effects.clipboardReadTrampoline
        //     else
        //         null;
        // },
        Option_::ClipboardRead => unsafe {
            (*terminal_).effects.clipboard_read =
                std::mem::transmute::<*const c_void, Option<ClipboardReadFn>>(value);
            // Installed dynamically to keep the handler's null fast path
            // (reads stay denied without a callback; see Effects).
            (*handler_ptr(terminal_)).effects.clipboard_read = if !value.is_null() {
                Some(Effects::clipboard_read_trampoline)
            } else {
                None
            };
        },

        // .unknown_sequence => {
        //     wrapper.effects.unknown_sequence = value;
        //     wrapper.stream.handler.unknown_sequence = if (value != null)
        //         &Effects.unknownSequenceTrampoline
        //     else
        //         null;
        // },
        Option_::UnknownSequence => unsafe {
            (*terminal_).effects.unknown_sequence =
                std::mem::transmute::<*const c_void, Option<UnknownSequenceFn>>(value);
            // The trampoline is installed dynamically to preserve the
            // handler's null fast path.
            (*handler_ptr(terminal_)).unknown_sequence = if !value.is_null() {
                Some(Effects::unknown_sequence_trampoline)
            } else {
                None
            };
        },

        // .title_report => wrapper.stream.handler.title_report = if (value) |ptr|
        //     ptr.*
        // else
        //     false,
        Option_::TitleReport => {
            let ptr = value as *const bool;
            let v = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `bool` (caller contract).
                unsafe { *ptr }
            } else {
                false
            };
            // SAFETY: raw place write off the handle (no wrapper-level `&mut`).
            unsafe { (*handler_ptr(terminal_)).title_report = v };
        }

        // .desktop_notification => wrapper.effects.desktop_notification = value,
        Option_::DesktopNotification => unsafe {
            (*terminal_).effects.desktop_notification =
                std::mem::transmute::<*const c_void, Option<DesktopNotificationFn>>(value);
        },

        // .progress_report => wrapper.effects.progress_report = value,
        Option_::ProgressReport => unsafe {
            (*terminal_).effects.progress_report =
                std::mem::transmute::<*const c_void, Option<ProgressReportFn>>(value);
        },

        // .title => {
        //     const str = if (value) |v| v.ptr[0..v.len] else "";
        //     wrapper.terminal.setTitle(str) catch return .out_of_memory;
        // },
        Option_::Title => {
            // const str = if (value) |v| v.ptr[0..v.len] else "";
            //
            // PORT NOTE (WS8 T7): the caller is allowed to hand back the bytes
            // `rustty_terminal_title()` returned, i.e. the terminal's OWN
            // buffer. We therefore keep the source as a RAW pointer and use the
            // alias-tolerant `set_title_raw`; forming a `&[u8]` here and
            // passing it to `set_title` would make the source a strongly
            // protected argument, and writing into it would be Stacked-Borrows
            // UB (memory: `rust-mut-args-strongly-protected`).
            let ptr = value as *const LibString;
            let (src, src_len): (*const u8, usize) = if !ptr.is_null() {
                let s = unsafe { *ptr };
                (s.ptr, s.len)
            } else {
                (b"".as_ptr(), 0)
            };
            // wrapper.terminal.setTitle(str) catch return .out_of_memory;
            // SAFETY: write the `title` sub-field of the terminal via the raw ptr;
            // `set_title_raw` takes `&mut Terminal` over a region disjoint from
            // the stream handler's alias usage (no concurrent `vt_write` is
            // live), and tolerates `src` overlapping the terminal's title.
            if unsafe { (*t).set_title_raw(src, src_len) }.is_err() {
                return Result::OutOfMemory;
            }
        }
        // .pwd => {
        //     const str = if (value) |v| v.ptr[0..v.len] else "";
        //     wrapper.terminal.setPwd(str) catch return .out_of_memory;
        // },
        Option_::Pwd => {
            // See the WS8 T7 PORT NOTE on `Option_::Title` above — the source
            // may alias the terminal's own pwd buffer, so it stays raw.
            let ptr = value as *const LibString;
            let (src, src_len): (*const u8, usize) = if !ptr.is_null() {
                let s = unsafe { *ptr };
                (s.ptr, s.len)
            } else {
                (b"".as_ptr(), 0)
            };
            if unsafe { (*t).set_pwd_raw(src, src_len) }.is_err() {
                return Result::OutOfMemory;
            }
        }

        // .terminfo_name => {
        //     const str = if (value) |v| v.ptr[0..v.len] else "";
        //     if (str.len > wrapper.terminfo_name_buf.len) return .invalid_value;
        //     @memcpy(wrapper.terminfo_name_buf[0..str.len], str);
        //     wrapper.stream.handler.terminfo_name = if (str.len > 0)
        //         wrapper.terminfo_name_buf[0..str.len]
        //     else
        //         null;
        // },
        Option_::TerminfoName => {
            // const str = if (value) |v| v.ptr[0..v.len] else "";
            let ptr = value as *const LibString;
            let (src, src_len): (*const u8, usize) = if !ptr.is_null() {
                let s = unsafe { *ptr };
                (s.ptr, s.len)
            } else {
                (b"".as_ptr(), 0)
            };
            // if (str.len > wrapper.terminfo_name_buf.len) return .invalid_value;
            //
            // The bounds check FIRST, then a copy of EXACTLY `str.len` bytes
            // (the `960c2cca5` overflow-class contract).
            if src_len > rustty_vt::stream_terminal::MAX_TERMINFO_NAME_BYTES {
                return Result::InvalidValue;
            }
            // @memcpy(wrapper.terminfo_name_buf[0..str.len], str);
            //
            // SB (SOUNDNESS): the buffer is written via a raw place projection
            // off the wrapper base and the handler's published slice is rebuilt
            // from the SAME projection — no wrapper-level `&mut` is formed, so
            // the stream handler's other self-referential aliases survive (see
            // the `terminfo_name_buf` field note).
            let buf: *mut u8 =
                unsafe { core::ptr::addr_of_mut!((*terminal_).terminfo_name_buf) } as *mut u8;
            // SAFETY: `src` has `src_len` readable bytes (caller contract);
            // `src_len <= MAX_TERMINFO_NAME_BYTES` was checked above; the C
            // caller's string cannot alias this internal buffer (it is never
            // exposed), so the ranges do not overlap (`@memcpy`'s contract).
            unsafe { core::ptr::copy_nonoverlapping(src, buf, src_len) };
            // wrapper.stream.handler.terminfo_name = ...;
            // SAFETY: the published slice borrows the heap-stable wrapper
            // buffer; it lives exactly as long as the wrapper (freed in `free`
            // together with the stream).
            unsafe {
                (*handler_ptr(terminal_)).terminfo_name = if src_len > 0 {
                    Some(core::slice::from_raw_parts(buf as *const u8, src_len))
                } else {
                    None
                };
            }
        }

        // .color_foreground => {
        //     wrapper.terminal.colors.foreground.default = if (value) |v| .fromC(v.*) else null;
        //     wrapper.terminal.flags.dirty.palette = true;
        // },
        Option_::ColorForeground => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.foreground.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_background => { ... },
        Option_::ColorBackground => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.background.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_cursor => { ... },
        Option_::ColorCursor => unsafe {
            let ptr = value as *const crate::color::RgbC;
            (*t).colors.cursor.default = if !ptr.is_null() { Some(*ptr) } else { None };
            (*t).flags.dirty.palette = true;
        },
        // .color_palette => {
        //     wrapper.terminal.colors.palette.changeDefault(
        //         wrapper.terminal.gpa(),
        //         if (value) |v| color.paletteZval(v) else color.default,
        //     ) catch return .out_of_memory;
        //     wrapper.terminal.flags.dirty.palette = true;
        // },
        Option_::ColorPalette => unsafe {
            let ptr = value as *const crate::color::PaletteC;
            let def = if !ptr.is_null() {
                crate::color::palette_zval(ptr)
            } else {
                rustty_vt::color::DEFAULT
            };
            // `wrapper.terminal.gpa()`, read raw (no `&mut Terminal`).
            let gpa = (*(*t).screens.active).alloc;
            if (*t).colors.palette.change_default(gpa, def).is_err() {
                return Result::OutOfMemory;
            }
            (*t).flags.dirty.palette = true;
        },

        // .kitty_image_storage_limit => {
        //     if (comptime !build_options.kitty_graphics) return .success;
        //     const limit: usize = if (value) |v| @intCast(v.*) else 0;
        //     var it = wrapper.terminal.screens.all.iterator();
        //     while (it.next()) |entry| {
        //         const screen = entry.value.*;
        //         screen.kitty_images.setLimit(screen.io, screen.alloc, screen, limit);
        //     }
        // },
        Option_::KittyImageStorageLimit => {
            // if (comptime !build_options.kitty_graphics) return .success;
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::Success;
            }
            // const limit: usize = if (value) |v| @intCast(v.*) else 0;
            // PORT NOTE (W33-B07): `@intCast` is a CHECKED narrowing — a
            // safety-checked build traps when the `uint64_t` limit exceeds
            // `usize` (only reachable where `usize` is 32-bit). Measured with
            // Zig 0.16.0 (`r13p-reference-probe.zig`, the same cast narrowed to
            // u32): Debug and ReleaseSafe panic "integer does not fit in
            // destination type", ReleaseFast wraps (4294967296 -> 0).
            //
            // PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD63. The
            // value is a C-ABI caller's `uint64_t`, not an invariant, and this
            // is an `extern "C"` setter, so a panic aborts. A limit above the
            // address space is a request for "no practical limit", so it
            // saturates; the earlier `.unwrap()` trapped in EVERY profile,
            // which was stricter than the reference's own safe builds.
            let ptr = value as *const u64;
            let limit: usize = if !ptr.is_null() {
                saturating_from_u64(unsafe { *ptr }, usize::MAX)
            } else {
                0
            };
            // var it = wrapper.terminal.screens.all.iterator();
            // while (it.next()) |entry| {
            //     const screen = entry.value.*;
            //     screen.kitty_images.setLimit(screen.io, screen.alloc, screen, limit);
            // }
            // (`4371871bc` made `setLimit` infallible; the `catch return
            // .out_of_memory` is gone upstream.)
            for slot in unsafe { (*t).screens.all.iter() } {
                if let Some(screen) = *slot {
                    // PORT NOTE: `setLimit(&mut self, alloc, s: *mut Screen, limit)`
                    // aliases `screen.kitty_images` (the receiver) with `screen`
                    // (the `s` arg). Mirror `ScreenSet::init`: MOVE `kitty_images`
                    // out, run `set_limit` against the raw screen pointer, then
                    // move it back.
                    let alloc = unsafe { (*screen).alloc };
                    let mut images = core::mem::take(unsafe { &mut (*screen).kitty_images });
                    images.set_limit(alloc, screen, limit);
                    unsafe { (*screen).kitty_images = images };
                }
            }
        }
        // .kitty_image_medium_file,
        // .kitty_image_medium_shared_mem,
        // => {
        //     if (comptime !build_options.kitty_graphics) return .success;
        //     const val = (value orelse return .success).*;
        //     var it = wrapper.terminal.screens.all.iterator();
        //     while (it.next()) |entry| {
        //         const screen = entry.value.*;
        //         switch (option) {
        //             .kitty_image_medium_file => screen.kitty_images.image_limits.file = val,
        //             .kitty_image_medium_shared_mem => screen.kitty_images.image_limits.shared_memory = val,
        //             else => unreachable,
        //         }
        //     }
        // },
        // PORT NOTE (oracle `e8525c0fd`, `P3-W11 T1`): `kitty_image_medium_temp_file`
        // LEFT this multi-arm group when its value type changed from `bool*` to
        // `RusttyString*`; it now has its own arm below.
        Option_::KittyImageMediumFile | Option_::KittyImageMediumSharedMem => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::Success;
            }
            // const val = (value orelse return .success).*;
            let ptr = value as *const bool;
            if ptr.is_null() {
                return Result::Success;
            }
            let val = unsafe { *ptr };
            for slot in unsafe { (*t).screens.all.iter() } {
                if let Some(screen) = *slot {
                    let limits = unsafe { &mut (*screen).kitty_images.image_limits };
                    match option {
                        Option_::KittyImageMediumFile => limits.file = val,
                        Option_::KittyImageMediumSharedMem => limits.shared_memory = val,
                        _ => unreachable!(),
                    }
                }
            }
        }

        // .kitty_image_medium_temp_file => {
        //     if (comptime !build_options.kitty_graphics) return .success;
        //     if (value) |v| {
        //         if (v.len > wrapper.tmp_dir_path.len) return .out_of_memory;
        //         @memcpy(wrapper.tmp_dir_path[0..v.len], v.ptr[0..v.len]);
        //         var it = wrapper.terminal.screens.all.iterator();
        //         while (it.next()) |entry| {
        //             const screen = entry.value.*;
        //             screen.kitty_images.image_limits.temporary_file = .{
        //                 .enabled = .{ .directory = wrapper.tmp_dir_path[0..v.len] },
        //             };
        //         }
        //     } else {
        //         var it = wrapper.terminal.screens.all.iterator();
        //         while (it.next()) |entry| {
        //             const screen = entry.value.*;
        //             screen.kitty_images.image_limits.temporary_file = .disabled;
        //         }
        //     }
        // },
        Option_::KittyImageMediumTempFile => {
            if !rustty_vt::options::KITTY_GRAPHICS {
                return Result::Success;
            }
            // const alloc = wrapper.terminal.gpa();
            // SAFETY: `(*terminal_).alloc` is the live C-resolved allocator
            // (F-008), valid until `free` reclaims it.
            let alloc: &dyn ZigAllocator = unsafe { &*(*terminal_).alloc };

            let ptr = value as *const LibString;
            if ptr.is_null() {
                // `else` arm: a NULL value pointer DISABLES the medium.
                //
                // SAFETY (Stacked Borrows discipline, path clear): every
                // screen is switched to `Disabled` FIRST, so none of them
                // still borrows `wrapper.tmp_dir_path` by the time it is
                // freed just below — matching upstream's
                // `if (wrapper.tmp_dir_path) |old| alloc.free(old);` AFTER
                // its own screen loop.
                for slot in unsafe { (*t).screens.all.iter() } {
                    if let Some(screen) = *slot {
                        unsafe {
                            (*screen).kitty_images.image_limits.temporary_file =
                                TemporaryFileLimit::Disabled;
                        }
                    }
                }
                // if (wrapper.tmp_dir_path) |old| alloc.free(old);
                // wrapper.tmp_dir_path = null;
                if let Some(old) = unsafe { (*terminal_).tmp_dir_path.take() } {
                    alloc.free_bytes(old);
                }
                return Result::Success;
            }

            // PORT NOTE (oracle `960c2cca5`/`105d0a545`, P7-W2 T3): the
            // reference copies the caller's bytes into
            // `wrapper.tmp_dir_path`, now an allocator-owned `?[]u8` (was a
            // fixed `[max_path_bytes]u8` buffer before `105d0a545`) whose
            // slice the limits then BORROW. `960c2cca5` fixed a `@memcpy(&buf,
            // v.ptr[0..v.len])` whose destination was the WHOLE buffer while
            // the source was `v.len` bytes — a length mismatch; that fix's
            // observable contract still applies here:
            //   (a) the length is bounds-checked FIRST, against
            //       `MAX_PATH_BYTES`, returning `RUSTTY_OUT_OF_MEMORY`;
            //   (b) EXACTLY `v.len` bytes are copied — never more.
            //
            // DECISIONS B6 (faithful): the wrapper allocates exactly ONE copy
            // (`dir` below), through the CALLER's allocator (F-008), and
            // every screen's `Limits` borrows it — not an independent
            // per-screen copy, and not a Rust-global-heap allocation.
            let v = unsafe { &*ptr };
            let len = v.len;
            // if (v.len > wrapper.tmp_dir_path.len) return .out_of_memory;
            if len > MAX_PATH_BYTES {
                return Result::OutOfMemory;
            }
            // const path = alloc.dupe(u8, v.ptr[0..v.len]) catch return .out_of_memory;
            let new_ptr = alloc.alloc_bytes(len);
            if new_ptr.is_null() {
                return Result::OutOfMemory;
            }
            if len > 0 {
                // SAFETY: `new_ptr` is a freshly allocated, non-overlapping
                // `len`-byte buffer (caller contract guarantees `v.ptr` is
                // readable for `len` bytes).
                unsafe { core::ptr::copy_nonoverlapping(v.ptr, new_ptr, len) };
            }
            let dir = ZigSlice::new(new_ptr, len);
            // var it = wrapper.terminal.screens.all.iterator();
            // while (it.next()) |entry| { ... screen.kitty_images.image_limits.temporary_file = .{ .enabled = .{ .directory = path } }; }
            //
            // SAFETY (Stacked Borrows discipline, path set twice): every
            // screen is pointed at the NEW `dir` here, BEFORE the wrapper's
            // OLD copy is freed below — so a screen updated earlier in this
            // very loop never observes a dangling pointer, and neither does
            // one updated on a LATER call (the old copy this call is about to
            // replace was already superseded for every screen by the time the
            // PREVIOUS call returned).
            for slot in unsafe { (*t).screens.all.iter() } {
                if let Some(screen) = *slot {
                    unsafe {
                        (*screen).kitty_images.image_limits.temporary_file =
                            TemporaryFileLimit::Enabled {
                                // SAFETY: `dir` is the wrapper's own copy,
                                // stored into `(*terminal_).tmp_dir_path`
                                // just below and kept alive until it is
                                // replaced or freed by a LATER call (or by
                                // `free`, which tears down every screen
                                // first) — never while this screen still
                                // holds this borrow.
                                directory: DirBorrow::new(std::ptr::slice_from_raw_parts(
                                    dir.ptr, dir.len,
                                )),
                            };
                    }
                }
            }
            // Every screen points at the new copy now so the previous
            // one can be released.
            // if (wrapper.tmp_dir_path) |old| alloc.free(old);
            // wrapper.tmp_dir_path = path;
            if let Some(old) = unsafe { (*terminal_).tmp_dir_path.replace(dir) } {
                alloc.free_bytes(old);
            }
        }

        // .apc_max_bytes => {
        //     wrapper.stream.handler.apc_handler.max_bytes = if (value) |ptr|
        //         .initFull(ptr.*)
        //     else
        //         .{};
        // },
        Option_::ApcMaxBytes => {
            // PORT NOTE: the reference `EnumMap` ports to the always-full
            // `[usize; Protocol::COUNT]` array (apc.rs). `.initFull(ptr.*)` sets
            // every protocol to the value; the `else .{}` branch (an EnumMap with
            // no entries) is, under the always-full port, the field's struct
            // default — `default_max_bytes` per protocol.
            let handler = unsafe { &mut *handler_ptr(terminal_) };
            let ptr = value as *const usize;
            if !ptr.is_null() {
                let v = unsafe { *ptr };
                handler.apc_handler.max_bytes = [v; rustty_vt::apc::Protocol::COUNT];
            } else {
                handler.apc_handler.max_bytes = [
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Kitty),
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Glyph),
                ];
            }
        }
        // .apc_max_bytes_kitty => {
        //     if (value) |ptr| {
        //         wrapper.stream.handler.apc_handler.max_bytes.put(.kitty, ptr.*);
        //     } else {
        //         wrapper.stream.handler.apc_handler.max_bytes.remove(.kitty);
        //     }
        // },
        Option_::ApcMaxBytesKitty => {
            // PORT NOTE: `.put(.kitty, v)` sets the kitty slot. `.remove(.kitty)`
            // removes the entry so the later `.get(.kitty) orelse
            // defaultMaxBytes(.kitty)` falls back to the default; under the
            // always-full array port that is exactly resetting the slot to
            // `defaultMaxBytes(.kitty)`.
            let handler = unsafe { &mut *handler_ptr(terminal_) };
            let ptr = value as *const usize;
            let kitty = rustty_vt::apc::Protocol::Kitty as usize;
            if !ptr.is_null() {
                handler.apc_handler.max_bytes[kitty] = unsafe { *ptr };
            } else {
                handler.apc_handler.max_bytes[kitty] =
                    rustty_vt::apc::Protocol::default_max_bytes(rustty_vt::apc::Protocol::Kitty);
            }
        }

        // .glyph_protocol => {
        //     const enabled = (value orelse return .success).*;
        //     wrapper.stream.handler.apc_handler.enable(.glyph, enabled);
        //     if (!enabled) wrapper.terminal.glyph_glossary.clearAndFree(wrapper.terminal.gpa());
        // },
        Option_::GlyphProtocol => {
            // const enabled = (value orelse return .success).*;
            let ptr = value as *const bool;
            if ptr.is_null() {
                return Result::Success;
            }
            let enabled = unsafe { *ptr };
            // wrapper.stream.handler.apc_handler.enable(.glyph, enabled);
            unsafe {
                (*handler_ptr(terminal_))
                    .apc_handler
                    .enable(rustty_vt::apc::Protocol::Glyph, enabled)
            };
            // if (!enabled) wrapper.terminal.glyph_glossary.clearAndFree(wrapper.terminal.gpa());
            if !enabled {
                // PORT NOTE: rustty's `Glossary::clear_and_free` carries its own
                // allocator (the `gpa()` thread is internal); the reference passes
                // `wrapper.terminal.gpa()` explicitly.
                unsafe { (*t).glyph_glossary.clear_and_free() };
            }
        }

        // .selection => {
        //     if (value) |ptr| {
        //         const sel = ptr.toZig() orelse return .invalid_value;
        //         wrapper.terminal.screens.active.select(sel) catch return .out_of_memory;
        //     } else {
        //         wrapper.terminal.screens.active.clearSelection();
        //     }
        // },
        Option_::Selection => {
            let active: *mut rustty_vt::screen::Screen = unsafe { (*t).screens.active };
            // if (value) |ptr| { ... } else { ... }
            let ptr = value as *const crate::selection::CSelection;
            if !ptr.is_null() {
                // const sel = ptr.toZig() orelse return .invalid_value;
                let sel = match unsafe { (*ptr).to_zig() } {
                    Some(s) => s,
                    None => return Result::InvalidValue,
                };
                // wrapper.terminal.screens.active.select(sel) catch return .out_of_memory;
                if unsafe { (*active).select(Some(sel)) }.is_err() {
                    return Result::OutOfMemory;
                }
            } else {
                // wrapper.terminal.screens.active.clearSelection();
                unsafe { (*active).clear_selection() };
            }
        }

        // .default_cursor_style => {
        //     const style = (if (value) |ptr| ptr.* else TerminalCursorStyle.block).toZig() orelse return .invalid_value;
        //     wrapper.terminal.setDefaultCursorStyle(style);
        // },
        Option_::DefaultCursorStyle => {
            // const style = (if (value) |ptr| ptr.* else .block).toZig() orelse return .invalid_value;
            //
            // D11: the incoming value is read as the raw `c_int` it is on the
            // wire, NOT as the closed Rust `TerminalCursorStyle` — the oracle's
            // enum is non-exhaustive, so any `c_int` is representable there,
            // while a Rust closed-enum read of an out-of-range discriminant is
            // UB. `from_c_int` reproduces the reference `_ => null` arm.
            let ptr = value as *const c_int;
            let raw = if !ptr.is_null() {
                unsafe { *ptr }
            } else {
                TerminalCursorStyle::Block as c_int
            };
            let style =
                match TerminalCursorStyle::from_c_int(raw).and_then(TerminalCursorStyle::to_zig) {
                    Some(s) => s,
                    None => return Result::InvalidValue,
                };
            // wrapper.terminal.setDefaultCursorStyle(style);
            //
            // PORT NOTE (oracle c594031d5): the C API no longer writes the
            // handler's `default_cursor_style` (that field is gone) nor pokes
            // the active screen's cursor style; `Terminal` owns both and its
            // setter applies the change only while the cursor is still on its
            // defaults.
            unsafe { (*t).set_default_cursor_style(style) };
        }
        // .default_cursor_blink => {
        //     const blink = if (value) |ptr| ptr.* else false;
        //     wrapper.terminal.setDefaultCursorBlink(blink);
        // },
        Option_::DefaultCursorBlink => {
            // const blink = if (value) |ptr| ptr.* else false;
            let ptr = value as *const bool;
            let blink = if !ptr.is_null() {
                unsafe { *ptr }
            } else {
                false
            };
            // wrapper.terminal.setDefaultCursorBlink(blink);
            //
            // PORT NOTE: the reference's `blink` here is a plain `bool` (the C
            // option is a `bool*`), widened to `Terminal.Cursor.default_blink`'s
            // `?bool` at the setter boundary — a C caller can never select the
            // `null` "emulator default" arm through this option.
            unsafe { (*t).set_default_cursor_blink(Some(blink)) };
        }

        // .scrollback_max_bytes => wrapper.terminal.setScrollbackMaxBytes(
        //     if (value) |ptr| ptr.* else null,
        // ),
        Option_::ScrollbackMaxBytes => {
            let ptr = value as *const usize;
            let max = if ptr.is_null() {
                None
            } else {
                Some(unsafe { *ptr })
            };
            unsafe { (*t).set_scrollback_max_bytes(max) };
        }

        // .scrollback_max_lines => wrapper.terminal.setScrollbackMaxLines(
        //     if (value) |ptr| ptr.* else null,
        // ),
        Option_::ScrollbackMaxLines => {
            let ptr = value as *const usize;
            let max = if ptr.is_null() {
                None
            } else {
                Some(unsafe { *ptr })
            };
            unsafe { (*t).set_scrollback_max_lines(max) };
        }

        // .continuation_max_bytes => setContinuationMaxBytes(
        //     wrapper,
        //     if (value) |ptr| ptr.* else default_continuation_max_bytes,
        // ),
        Option_::ContinuationMaxBytes => {
            let ptr = value as *const usize;
            let max_bytes = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `usize` (caller contract).
                unsafe { *ptr }
            } else {
                DEFAULT_CONTINUATION_MAX_BYTES
            };
            // SAFETY: `terminal_` is a live wrapper (caller contract).
            unsafe { set_continuation_max_bytes(terminal_, max_bytes) };
        }

        // .unknown_max_bytes => wrapper.stream.handler.apc_handler.unknown_max_bytes =
        //     if (value) |ptr| ptr.* else 0,
        Option_::UnknownMaxBytes => {
            let ptr = value as *const usize;
            let v = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `usize` (caller contract).
                unsafe { *ptr }
            } else {
                0
            };
            // SAFETY: raw place write off the handle (no wrapper-level `&mut`).
            unsafe { (*handler_ptr(terminal_)).apc_handler.unknown_max_bytes = v };
        }

        // .clipboard_write_max_bytes => wrapper.stream.handler.kitty_clipboard_write_max_bytes =
        //     if (value) |ptr| ptr.* else kitty_clipboard.max_write_size,
        Option_::ClipboardWriteMaxBytes => {
            let ptr = value as *const usize;
            let v = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `usize` (caller contract).
                unsafe { *ptr }
            } else {
                rustty_vt::kitty::clipboard::max_write_size
            };
            // SAFETY: raw place write off the handle (no wrapper-level `&mut`).
            unsafe { (*handler_ptr(terminal_)).kitty_clipboard_write_max_bytes = v };
        }

        // .resize_pull_scrollback => wrapper.terminal.flags.resize_pull_scrollback =
        //     if (value) |ptr| ptr.* else true,
        Option_::ResizePullScrollback => {
            let ptr = value as *const bool;
            let v = if !ptr.is_null() {
                // SAFETY: `value` points to a readable `bool` (caller contract).
                unsafe { *ptr }
            } else {
                true
            };
            // SAFETY: raw place write through the handle's terminal pointer
            // (no wrapper-level or terminal-level `&mut`).
            unsafe { (*(*terminal_).terminal).flags.resize_pull_scrollback = v };
        }

        // .mode, .mode_default => {
        //     const config = (value orelse return .invalid_value).*;
        //     const mode = config.toMode() orelse return .invalid_value;
        //     switch (option) {
        //         .mode => wrapper.terminal.modes.set(mode, config.value),
        //         .mode_default => {
        //             if (!modes.defaultConfigurable(mode)) return .invalid_value;
        //             wrapper.terminal.modes.setDefault(mode, config.value);
        //         },
        //         else => unreachable,
        //     }
        // },
        Option_::Mode | Option_::ModeDefault => {
            // const config = (value orelse return .invalid_value).*;
            let ptr = value as *const ModeConfig;
            if ptr.is_null() {
                return Result::InvalidValue;
            }
            // SAFETY: `value` points to a readable `ModeConfig` (caller contract).
            let config = unsafe { *ptr };
            // const mode = config.toMode() orelse return .invalid_value;
            let Some(mode) = config.to_mode() else {
                return Result::InvalidValue;
            };
            match option {
                // .mode => wrapper.terminal.modes.set(mode, config.value),
                // SAFETY: raw place access to the owned heap terminal.
                Option_::Mode => unsafe { (*t).modes.set(mode, config.value) },
                Option_::ModeDefault => {
                    // if (!modes.defaultConfigurable(mode)) return .invalid_value;
                    if !modes::default_configurable(mode) {
                        return Result::InvalidValue;
                    }
                    // wrapper.terminal.modes.setDefault(mode, config.value);
                    // SAFETY: raw place access to the owned heap terminal.
                    unsafe { (*t).modes.set_default(mode, config.value) };
                }
                // else => unreachable,
                _ => unreachable!(),
            }
        }
    }

    // return .success;
    Result::Success
}

/// C: GhosttyTerminalCursorStyle -> RusttyTerminalCursorStyle (R2)
// pub const TerminalCursorStyle = enum(c_int) { bar = 0, block = 1, underline = 2, block_hollow = 3, _ };
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCursorStyle {
    Bar = 0,
    Block = 1,
    Underline = 2,
    BlockHollow = 3,
}

impl TerminalCursorStyle {
    /// Narrow a wire `c_int` to a declared enumerator, `None` for anything else.
    ///
    /// PORT NOTE (D11): the reference enum is non-exhaustive (`_`), so every
    /// `c_int` is a representable value in Zig and `toZig`'s `_ => null` arm is
    /// what rejects unknown discriminants. Rust has no non-exhaustive
    /// `#[repr(C)]` enum, so the narrowing must happen at the read: this is a
    /// soundness requirement of the Rust representation (loading an
    /// out-of-range discriminant into the closed enum is UB), not a behavioural
    /// divergence. The public header declares
    /// `RUSTTY_TERMINAL_CURSOR_STYLE_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE`
    /// (INT_MAX), so out-of-range values are reachable from a conforming
    /// caller and must return `.invalid_value` exactly as the oracle does.
    fn from_c_int(value: c_int) -> Option<Self> {
        match value {
            0 => Some(TerminalCursorStyle::Bar),
            1 => Some(TerminalCursorStyle::Block),
            2 => Some(TerminalCursorStyle::Underline),
            3 => Some(TerminalCursorStyle::BlockHollow),
            // `_ => null`
            _ => None,
        }
    }

    // fn toZig(self: TerminalCursorStyle) ?Screen.CursorStyle {
    //     return switch (self) {
    //         .bar => .bar,
    //         .block => .block,
    //         .underline => .underline,
    //         .block_hollow => .block_hollow,
    //         _ => null,
    //     };
    // }
    // PORT NOTE: the reference enum is non-exhaustive (`_`), mapping any unknown
    // discriminant to `null`. The Rust `#[repr(C)]` enum carries only the four
    // named variants, so that arm lives in `from_c_int` above, which is what
    // callers must use to build this type out of a wire `c_int`. The signature
    // stays `Option` so the call site is a verbatim `orelse return
    // .invalid_value`.
    fn to_zig(self) -> Option<rustty_vt::screen::CursorStyle> {
        match self {
            // .bar => .bar,
            TerminalCursorStyle::Bar => Some(rustty_vt::screen::CursorStyle::Bar),
            // .block => .block,
            TerminalCursorStyle::Block => Some(rustty_vt::screen::CursorStyle::Block),
            // .underline => .underline,
            TerminalCursorStyle::Underline => Some(rustty_vt::screen::CursorStyle::Underline),
            // .block_hollow => .block_hollow,
            TerminalCursorStyle::BlockHollow => Some(rustty_vt::screen::CursorStyle::BlockHollow),
        }
    }
}
