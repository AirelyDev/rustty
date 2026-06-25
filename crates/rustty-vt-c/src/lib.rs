//! rustty-vt-c
//!
//! PORTS: ghostty src/terminal/c/ (the C ABI layer, break #4). The Rust
//! enums/types in rustty-vt are the source of truth; this crate is a pure FFI
//! projection (R3). cdylib+staticlib+rlib; cbindgen -> include/rustty.h with
//! `rustty_*` symbols (R2).
//!
//! PORT STATUS: in-progress (Stage B foundation: the small/foundational c/
//! files are ported; the heavy c/ files — terminal, render, kitty_graphics,
//! selection*, formatter, key/mouse, sgr, osc, cell, row, style, grid_ref,
//! size_report, types — land in later sub-batches under the acyclicity gate).
//!
//! PORT NOTE: this is the crate root, the rustty analog of `terminal/c/main.zig`
//! (the C API hub). `main.zig` declares every `c/<file>.zig` as a `pub const`
//! module and then `pub const <name> = <module>.<fn>;` re-exports the C API.
//! In rustty each module is a `mod` below and each C API symbol is authored as a
//! `#[no_mangle] pub extern "C" fn rustty_<name>(...)` 1:1 with the
//! `@export(&c.<fn>, .{ .name = "ghostty_<name>" })` lines (rustty_ prefix per
//! R2). The `extern "C"` wrappers are the single point that crosses the C ABI;
//! they forward to the ported module functions.

use std::os::raw::c_int;

// ----- terminal/c/main.zig: module hub -----
//
// PORT NOTE: the reference declares (this sub-batch's subset):
//   pub const allocator = @import("allocator.zig");
//   pub const color = @import("color.zig");
//   pub const focus = @import("focus.zig");
//   pub const modes = @import("modes.zig");
//   pub const sys = @import("sys.zig");
//   pub const types = @import("types.zig");  // DEFERRED (see below)
//   const buildpkg = @import("build_info.zig");
//   const Result = @import("result.zig").Result;
// plus the heavy modules ported later.
pub mod allocator;
pub mod build_info;
pub mod cell;
pub mod color;
mod cwrap;
pub mod focus;
pub mod key_event;
pub mod modes;
pub mod mouse_event;
pub mod paste;
pub mod result;
pub mod row;
pub mod size_report;
pub mod style;
pub mod sys;
#[cfg(test)]
mod test_util;
mod writer;

// PORT NOTE (DEFERRED): `pub const types = @import("types.zig");` and
// `pub const type_json = types.get_json;` are NOT ported in this foundation
// sub-batch. `types.zig` registers the C-ABI extern-struct layout for ~29 types
// (terminal.DeviceAttributes, formatter.TerminalOptions, selection.CSelection,
// render.Colors, grid_ref.CGridRef, style.Style, ...) whose `.C` projections are
// defined in the heavy `c/` files that have NOT yet landed. Porting `types.zig`
// with a partially-populated `structs` map would DROP reference registrations
// (an R1 fidelity violation), so it is gated to the sub-batch that lands those
// C struct types. Its 2 tests ride with it.

// ===================================================================
// C API exports — 1:1 with the `@export(&c.<fn>, .{ .name = "ghostty_<name>" })`
// lines in `lib_vt.zig` (rustty_ prefix per R2). cbindgen emits these into
// include/rustty.h. The bodies forward to the ported module functions; the
// `extern "C"` boundary is the ONLY place the C ABI is crossed.
// ===================================================================

// ----- build_info -----
// pub const build_info = buildpkg.get;  (@export name ghostty_build_info)
/// C: rustty_build_info
///
/// # Safety
/// `out` must point to writable storage of the type required by `data` (see
/// `build_info::BuildInfo::OutType`); for an out-of-range `data` value the call
/// returns `INVALID_VALUE` without dereferencing `out`.
#[no_mangle]
pub unsafe extern "C" fn rustty_build_info(data: c_int, out: *mut std::ffi::c_void) -> c_int {
    use build_info::BuildInfo;
    // SAFETY: `out` validity is the caller's contract (see fn-level # Safety).
    // PORT NOTE: the integer-range guard (`std.meta.intToEnum(BuildInfo, ...)
    // catch { log.warn(...); return .invalid_value; }`) lives here, on the raw
    // `c_int` crossing the C boundary (the typed `get` cannot receive an invalid
    // tag). `log.warn("build_info invalid data value={d}", ...)` is preserved as
    // the eprintln below (no scoped-log sink in this leaf).
    let info = match data {
        0 => BuildInfo::Invalid,
        1 => BuildInfo::Simd,
        2 => BuildInfo::KittyGraphics,
        3 => BuildInfo::TmuxControlMode,
        4 => BuildInfo::Optimize,
        5 => BuildInfo::VersionString,
        6 => BuildInfo::VersionMajor,
        7 => BuildInfo::VersionMinor,
        8 => BuildInfo::VersionPatch,
        9 => BuildInfo::VersionPre,
        10 => BuildInfo::VersionBuild,
        _ => {
            // log.warn("build_info invalid data value={d}", .{@intFromEnum(data)});
            eprintln!("build_info invalid data value={data}");
            return result::Result::InvalidValue.to_c_int();
        }
    };
    unsafe { build_info::get(info, out) }.to_c_int()
}

// ----- color -----
// pub const color_rgb_get = color.rgb_get;  (ghostty_color_rgb_get)
/// C: rustty_color_rgb_get
///
/// # Safety
/// `r`, `g`, and `b` must be valid, writable pointers to `u8`.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_rgb_get(c: color::RgbC, r: *mut u8, g: *mut u8, b: *mut u8) {
    // SAFETY: `r`/`g`/`b` validity is the caller's contract.
    unsafe { color::rgb_get(c, r, g, b) }
}

// ----- focus -----
// pub const focus_encode = focus.encode;  (ghostty_focus_encode)
/// C: rustty_focus_encode
///
/// # Safety
/// `out`, if non-null, must point to `out_len` writable bytes; `out_written`
/// must be a valid, writable pointer to `usize`.
#[no_mangle]
pub unsafe extern "C" fn rustty_focus_encode(
    event: focus::FocusEvent,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: `out`/`out_written` validity is the caller's contract.
    unsafe { focus::encode(event.to_vt(), out, out_len, out_written) }.to_c_int()
}

// ----- modes -----
// pub const mode_report_encode = modes.report_encode;  (ghostty_mode_report_encode)
/// C: rustty_mode_report_encode
///
/// # Safety
/// `out`, if non-null, must point to `out_len` writable bytes; `out_written`
/// must be a valid, writable pointer to `usize`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mode_report_encode(
    tag: rustty_vt::modes::ModeTagBacking,
    state: modes::ReportState,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: `out`/`out_written` validity is the caller's contract.
    unsafe { modes::report_encode(tag, state, out, out_len, out_written) }.to_c_int()
}

// ----- allocator -----
// pub const alloc_alloc = allocator.alloc;  (ghostty_alloc_alloc)
/// C: rustty_alloc_alloc
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid C allocator. The returned
/// pointer (if non-null) must be released with `rustty_alloc_free`.
#[no_mangle]
pub unsafe extern "C" fn rustty_alloc_alloc(
    alloc_: *const rustty_lib::allocator::Allocator,
    len: usize,
) -> *mut u8 {
    // SAFETY: `alloc_` validity is the caller's contract.
    unsafe { allocator::alloc(alloc_.as_ref(), len) }
}

// pub const alloc_free = allocator.free;  (ghostty_alloc_free)
/// C: rustty_alloc_free
///
/// # Safety
/// `ptr`, if non-null, must be a buffer previously returned by
/// `rustty_alloc_alloc` with the same `alloc_` and `len`.
#[no_mangle]
pub unsafe extern "C" fn rustty_alloc_free(
    alloc_: *const rustty_lib::allocator::Allocator,
    ptr: *mut u8,
    len: usize,
) {
    // SAFETY: `alloc_`/`ptr`/`len` validity is the caller's contract.
    unsafe { allocator::free(alloc_.as_ref(), ptr, len) }
}

// ----- sys -----
// pub const sys_log_stderr = sys.logStderr;  (ghostty_sys_log_stderr)
/// C: rustty_sys_log_stderr
///
/// # Safety
/// `scope_ptr`/`message_ptr` must point to `scope_len`/`message_len` valid bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_sys_log_stderr(
    userdata: *mut std::ffi::c_void,
    level: sys::LogLevel,
    scope_ptr: *const u8,
    scope_len: usize,
    message_ptr: *const u8,
    message_len: usize,
) {
    // SAFETY: `scope_ptr`/`message_ptr` validity is the caller's contract.
    unsafe {
        sys::log_stderr(
            userdata,
            level,
            scope_ptr,
            scope_len,
            message_ptr,
            message_len,
        )
    }
}

// pub const sys_set = sys.set;  (ghostty_sys_set)
/// C: rustty_sys_set
///
/// # Safety
/// `value` must be valid for the type required by `option`, or null. An
/// out-of-range `option` returns `INVALID_VALUE`.
#[no_mangle]
pub unsafe extern "C" fn rustty_sys_set(option: c_int, value: *const std::ffi::c_void) -> c_int {
    use sys::Option_;
    // PORT NOTE: the `std.meta.intToEnum(Option, ...) catch return .invalid_value`
    // guard lives here on the raw `c_int` crossing the C boundary.
    let opt = match option {
        0 => Option_::Userdata,
        1 => Option_::DecodePng,
        2 => Option_::Log,
        _ => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: `value` validity for `opt` is the caller's contract.
    unsafe { sys::set(opt, value) }.to_c_int()
}

// ----- paste -----
// pub const paste_is_safe = paste.is_safe;  (ghostty_paste_is_safe)
/// C: rustty_paste_is_safe
///
/// # Safety
/// `data`, if non-null, must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_paste_is_safe(data: *const u8, len: usize) -> bool {
    // SAFETY: `data`/`len` validity is the caller's contract.
    unsafe { paste::is_safe(data, len) }
}

// pub const paste_encode = paste.encode;  (ghostty_paste_encode)
/// C: rustty_paste_encode
///
/// # Safety
/// `data`, if non-null, must point to `data_len` writable bytes; `out`, if
/// non-null, must point to `out_len` writable bytes; `out_written` must be a
/// valid, writable pointer to `usize`.
#[no_mangle]
pub unsafe extern "C" fn rustty_paste_encode(
    data: *mut u8,
    data_len: usize,
    bracketed: bool,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { paste::encode(data, data_len, bracketed, out, out_len, out_written) }.to_c_int()
}

// ----- size_report -----
// pub const size_report_encode = size_report.encode;  (ghostty_size_report_encode)
/// C: rustty_size_report_encode
///
/// # Safety
/// `out`, if non-null, must point to `out_len` writable bytes; `out_written`
/// must be a valid, writable pointer to `usize`.
#[no_mangle]
pub unsafe extern "C" fn rustty_size_report_encode(
    style: size_report::Style,
    size: size_report::Size,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: `out`/`out_written` validity is the caller's contract.
    unsafe { size_report::encode(style, size, out, out_len, out_written) }.to_c_int()
}

// ----- key_event -----
// PORT NOTE: the `set_action`/`set_key` `intToEnum` runtime-safety guards live
// here on the raw `c_int` crossing the C boundary; an out-of-range value is
// dropped (a no-op, mirroring the reference's `log.warn` + early `return`). The
// `key::Mods` is crossed as its documented `u16` backing (a packed C bitfield).

// pub const key_event_new = key_event.new;  (ghostty_key_event_new)
/// C: rustty_key_event_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a key-event handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_new(
    alloc: *const rustty_lib::allocator::Allocator,
    result: *mut key_event::Event,
) -> c_int {
    // SAFETY: `alloc`/`result` validity is the caller's contract.
    unsafe { key_event::new(alloc.as_ref(), result) }.to_c_int()
}

// pub const key_event_free = key_event.free;  (ghostty_key_event_free)
/// C: rustty_key_event_free
///
/// # Safety
/// `event`, if non-null, must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_free(event: key_event::Event) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::free(event) }
}

// pub const key_event_set_action = key_event.set_action;  (ghostty_key_event_set_action)
/// C: rustty_key_event_set_action
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_action(event: key_event::Event, action: c_int) {
    // _ = std.meta.intToEnum(key.Action, ...) catch { log.warn(...); return; };
    let Some(a) = key_action_from_c_int(action) else {
        // log.warn("set_action invalid action value={d}", .{...});
        eprintln!("set_action invalid action value={action}");
        return;
    };
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::set_action(event, a) }
}

// pub const key_event_get_action = key_event.get_action;  (ghostty_key_event_get_action)
/// C: rustty_key_event_get_action
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_get_action(event: key_event::Event) -> c_int {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::get_action(event) as c_int }
}

// pub const key_event_set_key = key_event.set_key;  (ghostty_key_event_set_key)
/// C: rustty_key_event_set_key
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_key(event: key_event::Event, k: c_int) {
    // _ = std.meta.intToEnum(key.Key, ...) catch { log.warn(...); return; };
    let Some(key) = key_key_from_c_int(k) else {
        // log.warn("set_key invalid key value={d}", .{...});
        eprintln!("set_key invalid key value={k}");
        return;
    };
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::set_key(event, key) }
}

// pub const key_event_get_key = key_event.get_key;  (ghostty_key_event_get_key)
/// C: rustty_key_event_get_key
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_get_key(event: key_event::Event) -> c_int {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::get_key(event) as c_int }
}

// pub const key_event_set_mods = key_event.set_mods;  (ghostty_key_event_set_mods)
/// C: rustty_key_event_set_mods
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_mods(
    event: key_event::Event,
    mods: rustty_vt::input::key_mods::ModsBacking,
) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::set_mods(event, rustty_vt::input::key::Mods::from_int(mods)) }
}

// pub const key_event_get_mods = key_event.get_mods;  (ghostty_key_event_get_mods)
/// C: rustty_key_event_get_mods
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_get_mods(
    event: key_event::Event,
) -> rustty_vt::input::key_mods::ModsBacking {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::get_mods(event) }.int()
}

// pub const key_event_set_consumed_mods = key_event.set_consumed_mods;  (ghostty_key_event_set_consumed_mods)
/// C: rustty_key_event_set_consumed_mods
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_consumed_mods(
    event: key_event::Event,
    consumed_mods: rustty_vt::input::key_mods::ModsBacking,
) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe {
        key_event::set_consumed_mods(event, rustty_vt::input::key::Mods::from_int(consumed_mods))
    }
}

// pub const key_event_get_consumed_mods = key_event.get_consumed_mods;  (ghostty_key_event_get_consumed_mods)
/// C: rustty_key_event_get_consumed_mods
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_get_consumed_mods(
    event: key_event::Event,
) -> rustty_vt::input::key_mods::ModsBacking {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::get_consumed_mods(event) }.int()
}

// pub const key_event_set_composing = key_event.set_composing;  (ghostty_key_event_set_composing)
/// C: rustty_key_event_set_composing
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_composing(event: key_event::Event, composing: bool) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::set_composing(event, composing) }
}

// pub const key_event_get_composing = key_event.get_composing;  (ghostty_key_event_get_composing)
/// C: rustty_key_event_get_composing
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_get_composing(event: key_event::Event) -> bool {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::get_composing(event) }
}

// pub const key_event_set_utf8 = key_event.set_utf8;  (ghostty_key_event_set_utf8)
/// C: rustty_key_event_set_utf8
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`; `utf8`, if
/// non-null, must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_utf8(
    event: key_event::Event,
    utf8: *const u8,
    len: usize,
) {
    // SAFETY: `event`/`utf8` validity is the caller's contract.
    unsafe { key_event::set_utf8(event, utf8, len) }
}

// pub const key_event_get_utf8 = key_event.get_utf8;  (ghostty_key_event_get_utf8)
/// C: rustty_key_event_get_utf8
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`; `len`, if
/// non-null, must be a valid, writable pointer to `usize`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_get_utf8(
    event: key_event::Event,
    len: *mut usize,
) -> *const u8 {
    // SAFETY: `event`/`len` validity is the caller's contract.
    unsafe { key_event::get_utf8(event, len) }
}

// pub const key_event_set_unshifted_codepoint = key_event.set_unshifted_codepoint;
//   (ghostty_key_event_set_unshifted_codepoint)
/// C: rustty_key_event_set_unshifted_codepoint
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_unshifted_codepoint(
    event: key_event::Event,
    codepoint: u32,
) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::set_unshifted_codepoint(event, codepoint) }
}

// pub const key_event_get_unshifted_codepoint = key_event.get_unshifted_codepoint;
//   (ghostty_key_event_get_unshifted_codepoint)
/// C: rustty_key_event_get_unshifted_codepoint
///
/// # Safety
/// `event` must be a handle returned by `rustty_key_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_get_unshifted_codepoint(event: key_event::Event) -> u32 {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::get_unshifted_codepoint(event) }
}

// PORT NOTE: range-checked `c_int -> enum` projections for the guarded setters.
// `key::Key`/`key::Action`/`mouse::Action`/`mouse::Button` are contiguous
// `#[repr(C)]` enums; the range check IS `std.meta.intToEnum` (reject out-of-
// range), after which a `transmute` from the validated `c_int` is the enum.
const KEY_KEY_COUNT: c_int = 176; // contiguous 0..=175
fn key_key_from_c_int(v: c_int) -> Option<rustty_vt::input::key::Key> {
    if (0..KEY_KEY_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `key::Key` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, rustty_vt::input::key::Key>(v) })
    } else {
        None
    }
}

fn key_action_from_c_int(v: c_int) -> Option<rustty_vt::input::key::Action> {
    match v {
        0 => Some(rustty_vt::input::key::Action::Release),
        1 => Some(rustty_vt::input::key::Action::Press),
        2 => Some(rustty_vt::input::key::Action::Repeat),
        _ => None,
    }
}

fn mouse_action_from_c_int(v: c_int) -> Option<rustty_vt::input::mouse::Action> {
    match v {
        0 => Some(rustty_vt::input::mouse::Action::Press),
        1 => Some(rustty_vt::input::mouse::Action::Release),
        2 => Some(rustty_vt::input::mouse::Action::Motion),
        _ => None,
    }
}

fn mouse_button_from_c_int(v: c_int) -> Option<rustty_vt::input::mouse::Button> {
    if (0..=11).contains(&v) {
        // SAFETY: `v` is a valid `mouse::Button` discriminant (range-checked).
        Some(unsafe { std::mem::transmute::<c_int, rustty_vt::input::mouse::Button>(v) })
    } else {
        None
    }
}

// ----- mouse_event -----
// PORT NOTE: the `set_action`/`set_button` `intToEnum` guards live here on the
// raw `c_int`; `mouse::Mods` is crossed as its `u16` backing; `Position` (Pos)
// is an `#[repr(C)]` `{ f32, f32 }` crossed by value.

// pub const mouse_event_new = mouse_event.new;  (ghostty_mouse_event_new)
/// C: rustty_mouse_event_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a mouse-event handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_new(
    alloc: *const rustty_lib::allocator::Allocator,
    result: *mut mouse_event::Event,
) -> c_int {
    // SAFETY: `alloc`/`result` validity is the caller's contract.
    unsafe { mouse_event::new(alloc.as_ref(), result) }.to_c_int()
}

// pub const mouse_event_free = mouse_event.free;  (ghostty_mouse_event_free)
/// C: rustty_mouse_event_free
///
/// # Safety
/// `event`, if non-null, must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_free(event: mouse_event::Event) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::free(event) }
}

// pub const mouse_event_set_action = mouse_event.set_action;  (ghostty_mouse_event_set_action)
/// C: rustty_mouse_event_set_action
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_set_action(event: mouse_event::Event, action: c_int) {
    // _ = std.meta.intToEnum(Action, ...) catch { log.warn(...); return; };
    let Some(a) = mouse_action_from_c_int(action) else {
        // log.warn("set_action invalid action value={d}", .{...});
        eprintln!("set_action invalid action value={action}");
        return;
    };
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::set_action(event, a) }
}

// pub const mouse_event_get_action = mouse_event.get_action;  (ghostty_mouse_event_get_action)
/// C: rustty_mouse_event_get_action
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_get_action(event: mouse_event::Event) -> c_int {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::get_action(event) as c_int }
}

// pub const mouse_event_set_button = mouse_event.set_button;  (ghostty_mouse_event_set_button)
/// C: rustty_mouse_event_set_button
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_set_button(event: mouse_event::Event, button: c_int) {
    // _ = std.meta.intToEnum(Button, ...) catch { log.warn(...); return; };
    let Some(b) = mouse_button_from_c_int(button) else {
        // log.warn("set_button invalid button value={d}", .{...});
        eprintln!("set_button invalid button value={button}");
        return;
    };
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::set_button(event, b) }
}

// pub const mouse_event_clear_button = mouse_event.clear_button;  (ghostty_mouse_event_clear_button)
/// C: rustty_mouse_event_clear_button
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_clear_button(event: mouse_event::Event) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::clear_button(event) }
}

// pub const mouse_event_get_button = mouse_event.get_button;  (ghostty_mouse_event_get_button)
/// C: rustty_mouse_event_get_button
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`; `out`, if
/// non-null, must be a valid, writable pointer to a `c_int` button code.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_get_button(
    event: mouse_event::Event,
    out: *mut c_int,
) -> bool {
    // PORT NOTE: the reference writes the `Button` enum (a `c_int`) through `out`.
    let mut b: mouse_event::Button = mouse_event::Button::Unknown;
    // SAFETY: `event` validity is the caller's contract.
    let has = unsafe { mouse_event::get_button(event, &mut b) };
    if has && !out.is_null() {
        // SAFETY: `out` is a valid writable pointer (caller contract).
        unsafe { *out = b as c_int };
    }
    has
}

// pub const mouse_event_set_mods = mouse_event.set_mods;  (ghostty_mouse_event_set_mods)
/// C: rustty_mouse_event_set_mods
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_set_mods(
    event: mouse_event::Event,
    mods: rustty_vt::input::key_mods::ModsBacking,
) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::set_mods(event, rustty_vt::input::key::Mods::from_int(mods)) }
}

// pub const mouse_event_get_mods = mouse_event.get_mods;  (ghostty_mouse_event_get_mods)
/// C: rustty_mouse_event_get_mods
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_get_mods(
    event: mouse_event::Event,
) -> rustty_vt::input::key_mods::ModsBacking {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::get_mods(event) }.int()
}

// pub const mouse_event_set_position = mouse_event.set_position;  (ghostty_mouse_event_set_position)
/// C: rustty_mouse_event_set_position
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_set_position(
    event: mouse_event::Event,
    pos: mouse_event::Position,
) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::set_position(event, pos) }
}

// pub const mouse_event_get_position = mouse_event.get_position;  (ghostty_mouse_event_get_position)
/// C: rustty_mouse_event_get_position
///
/// # Safety
/// `event` must be a handle returned by `rustty_mouse_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_get_position(
    event: mouse_event::Event,
) -> mouse_event::Position {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { mouse_event::get_position(event) }
}

// ----- cell -----
// PORT NOTE: the `cell_get` `intToEnum(CellData, ...)` runtime-safety guard lives
// here on the raw `c_int` crossing the C boundary; an out-of-range value returns
// `INVALID_VALUE` (mirroring the reference early return).

// pub const cell_get = cell.get;  (ghostty_cell_get)
/// C: rustty_cell_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_cell_get(
    cell_: cell::CCell,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    let Some(d) = cell_data_from_c_int(data) else {
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `out` validity is the caller's contract.
    unsafe { cell::get(cell_, d, out) }.to_c_int()
}

// pub const cell_get_multi = cell.get_multi;  (ghostty_cell_get_multi)
/// C: rustty_cell_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[no_mangle]
pub unsafe extern "C" fn rustty_cell_get_multi(
    cell_: cell::CCell,
    count: usize,
    keys: *const cell::CellData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { cell::get_multi(cell_, count, keys, values, out_written) }.to_c_int()
}

// ----- row -----
// pub const row_get = row.get;  (ghostty_row_get)
/// C: rustty_row_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_row_get(
    row_: row::CRow,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    let Some(d) = row_data_from_c_int(data) else {
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `out` validity is the caller's contract.
    unsafe { row::get(row_, d, out) }.to_c_int()
}

// pub const row_get_multi = row.get_multi;  (ghostty_row_get_multi)
/// C: rustty_row_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[no_mangle]
pub unsafe extern "C" fn rustty_row_get_multi(
    row_: row::CRow,
    count: usize,
    keys: *const row::RowData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { row::get_multi(row_, count, keys, values, out_written) }.to_c_int()
}

// ----- style -----
// pub const style_default = style.default_style;  (ghostty_style_default)
/// C: rustty_style_default
///
/// # Safety
/// `result` must be a valid, writable pointer to a `style::Style`.
#[no_mangle]
pub unsafe extern "C" fn rustty_style_default(result: *mut style::Style) {
    // SAFETY: `result` validity is the caller's contract.
    unsafe { style::default_style(result) }
}

// pub const style_is_default = style.style_is_default;  (ghostty_style_is_default)
/// C: rustty_style_is_default
///
/// # Safety
/// `s` must be a valid, readable pointer to a `style::Style`.
#[no_mangle]
pub unsafe extern "C" fn rustty_style_is_default(s: *const style::Style) -> bool {
    // SAFETY: `s` validity is the caller's contract.
    unsafe { style::style_is_default(s) }
}

// PORT NOTE: range-checked `c_int -> enum` projections for the cell/row getters
// (the `std.meta.intToEnum` guard). `CellData`/`RowData` are contiguous
// `#[repr(C)]` enums; the range check IS `intToEnum`, after which a `transmute`
// from the validated `c_int` is the enum.
const CELL_DATA_COUNT: c_int = 12; // contiguous 0..=11
fn cell_data_from_c_int(v: c_int) -> Option<cell::CellData> {
    if (0..CELL_DATA_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `CellData` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, cell::CellData>(v) })
    } else {
        None
    }
}

const ROW_DATA_COUNT: c_int = 9; // contiguous 0..=8
fn row_data_from_c_int(v: c_int) -> Option<row::RowData> {
    if (0..ROW_DATA_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `RowData` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, row::RowData>(v) })
    } else {
        None
    }
}

#[cfg(test)]
mod main_tests {
    // PORT NOTE: ghostty `terminal/c/main.zig` `test { _ = allocator; _ = color;
    // ... }` forces every C API module's tests to be compiled/run. In Rust the
    // crate's `#[cfg(test)]` modules are compiled by the test build; this test
    // references each ported module so the `refAllDecls`-style coverage holds for
    // the foundation subset. (The heavy modules + `types` are referenced when
    // they land.)
    #[test]
    fn ref_all_decls() {
        // _ = allocator; _ = buildpkg; _ = color; _ = focus; _ = modes; _ = sys;
        // We want to make sure we run the tests for the C allocator interface.
        let _ = super::result::Result::Success;
        let _ = super::color::rgb_get as unsafe fn(_, _, _, _);
        let _ = super::focus::encode as unsafe fn(_, _, _, _) -> _;
        let _ = super::modes::report_encode as unsafe fn(_, _, _, _, _) -> _;
        let _ = super::build_info::get as unsafe fn(_, _) -> _;
        let _ = super::allocator::alloc as unsafe fn(_, _) -> _;
        let _ = super::sys::set as unsafe fn(_, _) -> _;
    }
}
