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
pub mod formatter;
pub mod grid_ref;
pub mod grid_ref_tracked;
pub mod key_encode;
pub mod key_event;
pub mod kitty_graphics;
pub mod modes;
pub mod mouse_encode;
pub mod mouse_event;
pub mod osc;
pub mod paste;
pub mod point;
pub mod render;
pub mod result;
pub mod row;
pub mod selection;
pub mod selection_gesture;
pub mod sgr;
pub mod size_report;
pub mod style;
pub mod sys;
pub mod terminal;
#[cfg(test)]
mod test_util;
// pub const types = @import("types.zig");
// PORT NOTE: now landed (this sub-batch). `types.zig` registers the C-ABI
// extern-struct layout for ~29 types whose `.C` projections are now all defined.
pub mod types;
mod writer;

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

// ----- key_encode -----
// pub const key_encoder_new = key_encode.new;  (ghostty_key_encoder_new)
/// C: rustty_key_encoder_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a key-encoder handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_encoder_new(
    alloc: *const rustty_lib::allocator::Allocator,
    result: *mut key_encode::Encoder,
) -> c_int {
    // SAFETY: `alloc`/`result` validity is the caller's contract.
    unsafe { key_encode::new(alloc.as_ref(), result) }.to_c_int()
}

// pub const key_encoder_free = key_encode.free;  (ghostty_key_encoder_free)
/// C: rustty_key_encoder_free
///
/// # Safety
/// `encoder`, if non-null, must be a handle returned by `rustty_key_encoder_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_encoder_free(encoder: key_encode::Encoder) {
    // SAFETY: `encoder` validity is the caller's contract.
    unsafe { key_encode::free(encoder) }
}

// pub const key_encoder_setopt = key_encode.setopt;  (ghostty_key_encoder_setopt)
/// C: rustty_key_encoder_setopt
///
/// # Safety
/// `encoder`, if non-null, must be a `rustty_key_encoder_new` handle; `value`,
/// if non-null, must point to a readable value of the option's in-type.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_encoder_setopt(
    encoder: key_encode::Encoder,
    option: c_int,
    value: *const std::ffi::c_void,
) {
    // SAFETY: pointer validity is the caller's contract; the option-range guard
    // runs inside `key_encode::setopt`.
    unsafe { key_encode::setopt(encoder, option, value) }
}

// pub const key_encoder_setopt_from_terminal = key_encode.setopt_from_terminal;
//   (ghostty_key_encoder_setopt_from_terminal)
/// C: rustty_key_encoder_setopt_from_terminal
///
/// # Safety
/// `encoder`/`terminal`, if non-null, must be live handles.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_encoder_setopt_from_terminal(
    encoder: key_encode::Encoder,
    terminal: terminal::Terminal,
) {
    // SAFETY: handle validity is the caller's contract.
    unsafe { key_encode::setopt_from_terminal(encoder, terminal) }
}

// pub const key_encoder_encode = key_encode.encode;  (ghostty_key_encoder_encode)
/// C: rustty_key_encoder_encode
///
/// # Safety
/// `encoder`/`event` must be non-null `new` handles; `out` (if non-null) must
/// point to `out_len` writable bytes; `out_written` must be valid, writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_key_encoder_encode(
    encoder: key_encode::Encoder,
    event: key_event::Event,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { key_encode::encode(encoder, event, out, out_len, out_written) }.to_c_int()
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

// ----- mouse_encode -----
// pub const mouse_encoder_new = mouse_encode.new;  (ghostty_mouse_encoder_new)
/// C: rustty_mouse_encoder_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a mouse-encoder handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_encoder_new(
    alloc: *const rustty_lib::allocator::Allocator,
    result: *mut mouse_encode::Encoder,
) -> c_int {
    // SAFETY: `alloc`/`result` validity is the caller's contract.
    unsafe { mouse_encode::new(alloc.as_ref(), result) }.to_c_int()
}

// pub const mouse_encoder_free = mouse_encode.free;  (ghostty_mouse_encoder_free)
/// C: rustty_mouse_encoder_free
///
/// # Safety
/// `encoder`, if non-null, must be a handle returned by `rustty_mouse_encoder_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_encoder_free(encoder: mouse_encode::Encoder) {
    // SAFETY: `encoder` validity is the caller's contract.
    unsafe { mouse_encode::free(encoder) }
}

// pub const mouse_encoder_setopt = mouse_encode.setopt;  (ghostty_mouse_encoder_setopt)
/// C: rustty_mouse_encoder_setopt
///
/// # Safety
/// `encoder`, if non-null, must be a `rustty_mouse_encoder_new` handle; `value`,
/// if non-null, must point to a readable value of the option's in-type.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_encoder_setopt(
    encoder: mouse_encode::Encoder,
    option: c_int,
    value: *const std::ffi::c_void,
) {
    // SAFETY: pointer validity is the caller's contract; the option-range guard
    // runs inside `mouse_encode::setopt`.
    unsafe { mouse_encode::setopt(encoder, option, value) }
}

// pub const mouse_encoder_setopt_from_terminal = mouse_encode.setopt_from_terminal;
//   (ghostty_mouse_encoder_setopt_from_terminal)
/// C: rustty_mouse_encoder_setopt_from_terminal
///
/// # Safety
/// `encoder`/`terminal`, if non-null, must be live handles.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_encoder_setopt_from_terminal(
    encoder: mouse_encode::Encoder,
    terminal: terminal::Terminal,
) {
    // SAFETY: handle validity is the caller's contract.
    unsafe { mouse_encode::setopt_from_terminal(encoder, terminal) }
}

// pub const mouse_encoder_reset = mouse_encode.reset;  (ghostty_mouse_encoder_reset)
/// C: rustty_mouse_encoder_reset
///
/// # Safety
/// `encoder`, if non-null, must be a `rustty_mouse_encoder_new` handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_encoder_reset(encoder: mouse_encode::Encoder) {
    // SAFETY: `encoder` validity is the caller's contract.
    unsafe { mouse_encode::reset(encoder) }
}

// pub const mouse_encoder_encode = mouse_encode.encode;  (ghostty_mouse_encoder_encode)
/// C: rustty_mouse_encoder_encode
///
/// # Safety
/// `encoder`/`event` must be `rustty_mouse_encoder_new`/`rustty_mouse_event_new`
/// handles; `out` (if non-null) must point to `out_len` writable bytes;
/// `out_written` must be valid, writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_encoder_encode(
    encoder: mouse_encode::Encoder,
    event: mouse_event::Event,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { mouse_encode::encode(encoder, event, out, out_len, out_written) }.to_c_int()
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

// ----- sgr -----
// pub const sgr_new = sgr.new;  (ghostty_sgr_new)
/// C: rustty_sgr_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a sgr-parser handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_sgr_new(
    alloc: *const rustty_lib::allocator::Allocator,
    result: *mut sgr::Parser,
) -> c_int {
    // SAFETY: `alloc`/`result` validity is the caller's contract.
    unsafe { sgr::new(alloc.as_ref(), result) }.to_c_int()
}

// pub const sgr_free = sgr.free;  (ghostty_sgr_free)
/// C: rustty_sgr_free
///
/// # Safety
/// `parser`, if non-null, must be a handle returned by `rustty_sgr_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_sgr_free(parser: sgr::Parser) {
    // SAFETY: `parser` validity is the caller's contract.
    unsafe { sgr::free(parser) }
}

// pub const sgr_reset = sgr.reset;  (ghostty_sgr_reset)
/// C: rustty_sgr_reset
///
/// # Safety
/// `parser`, if non-null, must be a handle returned by `rustty_sgr_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_sgr_reset(parser: sgr::Parser) {
    // SAFETY: `parser` validity is the caller's contract.
    unsafe { sgr::reset(parser) }
}

// pub const sgr_set_params = sgr.setParams;  (ghostty_sgr_set_params)
/// C: rustty_sgr_set_params
///
/// # Safety
/// `parser`, if non-null, must be a handle returned by `rustty_sgr_new`;
/// `params` must point to `len` readable `u16`; `seps`, if non-null, must point
/// to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_sgr_set_params(
    parser: sgr::Parser,
    params: *const u16,
    seps: *const u8,
    len: usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { sgr::set_params(parser, params, seps, len) }.to_c_int()
}

// pub const sgr_next = sgr.next;  (ghostty_sgr_next)
/// C: rustty_sgr_next
///
/// # Safety
/// `parser`, if non-null, must be a handle returned by `rustty_sgr_new`;
/// `result` must be a valid, writable pointer to a `sgr::AttributeC`.
#[no_mangle]
pub unsafe extern "C" fn rustty_sgr_next(
    parser: sgr::Parser,
    result: *mut sgr::AttributeC,
) -> bool {
    // SAFETY: `parser`/`result` validity is the caller's contract.
    unsafe { sgr::next(parser, result) }
}

// pub const sgr_unknown_full = sgr.unknown_full;  (ghostty_sgr_unknown_full)
/// C: rustty_sgr_unknown_full
///
/// # Safety
/// `ptr`, if non-null, must be a valid, writable pointer to a `*const u16`.
#[no_mangle]
pub unsafe extern "C" fn rustty_sgr_unknown_full(
    unknown: sgr::UnknownC,
    ptr: *mut *const u16,
) -> usize {
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { sgr::unknown_full(unknown, ptr) }
}

// pub const sgr_unknown_partial = sgr.unknown_partial;  (ghostty_sgr_unknown_partial)
/// C: rustty_sgr_unknown_partial
///
/// # Safety
/// `ptr`, if non-null, must be a valid, writable pointer to a `*const u16`.
#[no_mangle]
pub unsafe extern "C" fn rustty_sgr_unknown_partial(
    unknown: sgr::UnknownC,
    ptr: *mut *const u16,
) -> usize {
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { sgr::unknown_partial(unknown, ptr) }
}

// pub const sgr_attribute_tag = sgr.attribute_tag;  (ghostty_sgr_attribute_tag)
/// C: rustty_sgr_attribute_tag
#[no_mangle]
pub extern "C" fn rustty_sgr_attribute_tag(attr: sgr::AttributeC) -> sgr::Tag {
    sgr::attribute_tag(attr)
}

// pub const sgr_attribute_value = sgr.attribute_value;  (ghostty_sgr_attribute_value)
/// C: rustty_sgr_attribute_value
///
/// # Safety
/// `attr` must be a valid pointer to an `AttributeC`; the returned pointer
/// aliases `attr.value` and is valid as long as `attr` is.
#[no_mangle]
pub unsafe extern "C" fn rustty_sgr_attribute_value(
    attr: *mut sgr::AttributeC,
) -> *mut sgr::CValue {
    // SAFETY: `attr` validity is the caller's contract.
    unsafe { sgr::attribute_value(attr) }
}

// ----- osc -----
// pub const osc_new = osc.new;  (ghostty_osc_new)
/// C: rustty_osc_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to an osc-parser handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_osc_new(
    alloc: *const rustty_lib::allocator::Allocator,
    result: *mut osc::Parser,
) -> c_int {
    // SAFETY: `alloc`/`result` validity is the caller's contract.
    unsafe { osc::new(alloc.as_ref(), result) }.to_c_int()
}

// pub const osc_free = osc.free;  (ghostty_osc_free)
/// C: rustty_osc_free
///
/// # Safety
/// `parser`, if non-null, must be a handle returned by `rustty_osc_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_osc_free(parser: osc::Parser) {
    // SAFETY: `parser` validity is the caller's contract.
    unsafe { osc::free(parser) }
}

// pub const osc_next = osc.next;  (ghostty_osc_next)
/// C: rustty_osc_next
///
/// # Safety
/// `parser` must be a non-null handle returned by `rustty_osc_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_osc_next(parser: osc::Parser, byte: u8) {
    // SAFETY: `parser` validity is the caller's contract.
    unsafe { osc::next(parser, byte) }
}

// pub const osc_reset = osc.reset;  (ghostty_osc_reset)
/// C: rustty_osc_reset
///
/// # Safety
/// `parser` must be a non-null handle returned by `rustty_osc_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_osc_reset(parser: osc::Parser) {
    // SAFETY: `parser` validity is the caller's contract.
    unsafe { osc::reset(parser) }
}

// pub const osc_end = osc.end;  (ghostty_osc_end)
/// C: rustty_osc_end
///
/// # Safety
/// `parser` must be a non-null handle returned by `rustty_osc_new`. The returned
/// command pointer aliases the parser and is valid until the next parse op.
#[no_mangle]
pub unsafe extern "C" fn rustty_osc_end(parser: osc::Parser, terminator: u8) -> osc::Command {
    // SAFETY: `parser` validity is the caller's contract.
    unsafe { osc::end(parser, terminator) }
}

// pub const osc_command_type = osc.commandType;  (ghostty_osc_command_type)
/// C: rustty_osc_command_type
///
/// # Safety
/// `command`, if non-null, must be a live command pointer returned by
/// `rustty_osc_end`.
#[no_mangle]
pub unsafe extern "C" fn rustty_osc_command_type(
    command: osc::Command,
) -> rustty_vt::osc::CommandKey {
    // SAFETY: `command` validity is the caller's contract.
    unsafe { osc::command_type(command) }
}

// pub const osc_command_data = osc.commandData;  (ghostty_osc_command_data)
/// C: rustty_osc_command_data
///
/// # Safety
/// `command`, if non-null, must be a live command pointer; `out`, for a valid
/// `data` kind, must point to writable storage of that kind's out-type.
#[no_mangle]
pub unsafe extern "C" fn rustty_osc_command_data(
    command: osc::Command,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> bool {
    // SAFETY: `command`/`out` validity is the caller's contract.
    unsafe { osc::command_data(command, data, out) }
}

// ----- formatter -----
// pub const formatter_terminal_new = formatter.terminal_new;  (ghostty_formatter_terminal_new)
/// C: rustty_formatter_terminal_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a formatter handle; `terminal`, if non-null, must
/// be a live terminal handle; `opts.selection`, if non-null, must point to a
/// valid `CSelection`.
#[no_mangle]
pub unsafe extern "C" fn rustty_formatter_terminal_new(
    alloc: *const rustty_lib::allocator::Allocator,
    result: *mut formatter::Formatter,
    terminal: terminal::Terminal,
    opts: formatter::TerminalOptions,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { formatter::terminal_new(alloc.as_ref(), result, terminal, opts) }.to_c_int()
}

// pub const formatter_format_buf = formatter.format_buf;  (ghostty_formatter_format_buf)
/// C: rustty_formatter_format_buf
///
/// # Safety
/// `formatter`, if non-null, must be a handle returned by
/// `rustty_formatter_terminal_new`; `out` (if non-null) must point to `out_len`
/// writable bytes; `out_written` must be a valid, writable pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_formatter_format_buf(
    formatter: formatter::Formatter,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { formatter::format_buf(formatter, out, out_len, out_written) }.to_c_int()
}

// pub const formatter_format_alloc = formatter.format_alloc;  (ghostty_formatter_format_alloc)
/// C: rustty_formatter_format_alloc
///
/// # Safety
/// `formatter`, if non-null, must be a `rustty_formatter_terminal_new` handle;
/// `alloc`, if non-null, a valid allocator; `out_ptr`/`out_len` must be
/// valid, writable pointers. The returned buffer is owned by the caller and must
/// be freed with the same allocator.
#[no_mangle]
pub unsafe extern "C" fn rustty_formatter_format_alloc(
    formatter: formatter::Formatter,
    alloc: *const rustty_lib::allocator::Allocator,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { formatter::format_alloc(formatter, alloc.as_ref(), out_ptr, out_len) }.to_c_int()
}

// pub const formatter_free = formatter.free;  (ghostty_formatter_free)
/// C: rustty_formatter_free
///
/// # Safety
/// `formatter`, if non-null, must be a handle returned by
/// `rustty_formatter_terminal_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_formatter_free(formatter: formatter::Formatter) {
    // SAFETY: `formatter` validity is the caller's contract.
    unsafe { formatter::free(formatter) }
}

// ----- types -----
// pub const type_json = types.get_json;  (ghostty_type_json)
/// C: rustty_type_json
///
/// Returns a pointer to the NUL-terminated JSON string describing the layout of
/// all C API extern structs (size/align/field offsets), for FFI consumers.
#[no_mangle]
pub extern "C" fn rustty_type_json() -> *const std::os::raw::c_char {
    types::get_json()
}

// ----- kitty_graphics -----
// PORT NOTE: 1:1 with the 16 `@export(&c.kitty_graphics_*, ...)` lines in
// `lib_vt.zig`. The `c.zig` aliases map two names to differently-named module
// fns: `kitty_graphics_image -> image_get_handle`, `kitty_graphics_placement_next
// -> placement_iterator_next`. The typed enum args (`Data`, `ImageData`,
// `PlacementData`) cross the C ABI as their `enum(c_int)` (cbindgen emits a C
// enum); the `placement_iterator_set` `intToEnum` runtime-safety guard lives on
// the raw `c_int` `option` here (the typed `set` cannot receive an invalid tag).

// pub const kitty_graphics_get = kitty_graphics.get;
/// C: rustty_kitty_graphics_get
///
/// # Safety
/// `graphics` must be a valid `KittyGraphics` handle; `out` must point to storage
/// of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_get(
    graphics: kitty_graphics::KittyGraphics,
    data: kitty_graphics::Data,
    out: *mut std::ffi::c_void,
) -> c_int {
    // SAFETY: `graphics`/`out` validity is the caller's contract.
    unsafe { kitty_graphics::get(graphics, data, out) }.to_c_int()
}

// pub const kitty_graphics_image = kitty_graphics.image_get_handle;
/// C: rustty_kitty_graphics_image
///
/// # Safety
/// `graphics` must be a valid `KittyGraphics` handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_image(
    graphics: kitty_graphics::KittyGraphics,
    image_id: u32,
) -> kitty_graphics::ImageHandle {
    // SAFETY: `graphics` validity is the caller's contract.
    unsafe { kitty_graphics::image_get_handle(graphics, image_id) }
}

// pub const kitty_graphics_image_get = kitty_graphics.image_get;
/// C: rustty_kitty_graphics_image_get
///
/// # Safety
/// `image`, if non-null, must point to a valid `Image`; `out` must point to
/// storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_image_get(
    image: kitty_graphics::ImageHandle,
    data: kitty_graphics::ImageData,
    out: *mut std::ffi::c_void,
) -> c_int {
    // SAFETY: `image`/`out` validity is the caller's contract.
    unsafe { kitty_graphics::image_get(image, data, out) }.to_c_int()
}

// pub const kitty_graphics_image_get_multi = kitty_graphics.image_get_multi;
/// C: rustty_kitty_graphics_image_get_multi
///
/// # Safety
/// See `kitty_graphics::image_get_multi`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_image_get_multi(
    image: kitty_graphics::ImageHandle,
    count: usize,
    keys: *const kitty_graphics::ImageData,
    values: *const *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { kitty_graphics::image_get_multi(image, count, keys, values, out_written) }.to_c_int()
}

// pub const kitty_graphics_placement_iterator_new = kitty_graphics.placement_iterator_new;
/// C: rustty_kitty_graphics_placement_iterator_new
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid C allocator; `out` must be a
/// valid, writable pointer to a `PlacementIterator`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_iterator_new(
    alloc_: *const rustty_lib::allocator::Allocator,
    out: *mut kitty_graphics::PlacementIterator,
) -> c_int {
    // SAFETY: `alloc_`/`out` validity is the caller's contract.
    unsafe { kitty_graphics::placement_iterator_new(alloc_.as_ref(), out) }.to_c_int()
}

// pub const kitty_graphics_placement_iterator_free = kitty_graphics.placement_iterator_free;
/// C: rustty_kitty_graphics_placement_iterator_free
///
/// # Safety
/// `iter`, if non-null, must be a pointer returned by the matching `_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_iterator_free(
    iter: kitty_graphics::PlacementIterator,
) {
    // SAFETY: `iter` validity is the caller's contract.
    unsafe { kitty_graphics::placement_iterator_free(iter) }
}

// pub const kitty_graphics_placement_iterator_set = kitty_graphics.placement_iterator_set;
/// C: rustty_kitty_graphics_placement_iterator_set
///
/// # Safety
/// `iter`, if non-null, must be a live iterator; `value`, if non-null, must point
/// to a value of the type required by `option`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_iterator_set(
    iter: kitty_graphics::PlacementIterator,
    option: c_int,
    value: *const std::ffi::c_void,
) -> c_int {
    use kitty_graphics::PlacementIteratorOption;
    // PORT NOTE: `if (comptime std.debug.runtime_safety) intToEnum(...) catch
    // return .invalid_value;` — the guard lives here on the raw `c_int`.
    let opt = match option {
        0 => PlacementIteratorOption::Layer,
        _ => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: `iter`/`value` validity is the caller's contract.
    unsafe { kitty_graphics::placement_iterator_set(iter, opt, value) }.to_c_int()
}

// pub const kitty_graphics_placement_next = kitty_graphics.placement_iterator_next;
/// C: rustty_kitty_graphics_placement_next
///
/// # Safety
/// `iter`, if non-null, must be a live iterator seeded by `get(.., .placement_iterator, ..)`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_next(
    iter: kitty_graphics::PlacementIterator,
) -> bool {
    // SAFETY: `iter` validity is the caller's contract.
    unsafe { kitty_graphics::placement_iterator_next(iter) }
}

// pub const kitty_graphics_placement_get = kitty_graphics.placement_get;
/// C: rustty_kitty_graphics_placement_get
///
/// # Safety
/// `iter`, if non-null, must be a live iterator positioned on an entry; `out`
/// must point to storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_get(
    iter: kitty_graphics::PlacementIterator,
    data: kitty_graphics::PlacementData,
    out: *mut std::ffi::c_void,
) -> c_int {
    // SAFETY: `iter`/`out` validity is the caller's contract.
    unsafe { kitty_graphics::placement_get(iter, data, out) }.to_c_int()
}

// pub const kitty_graphics_placement_get_multi = kitty_graphics.placement_get_multi;
/// C: rustty_kitty_graphics_placement_get_multi
///
/// # Safety
/// See `kitty_graphics::placement_get_multi`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_get_multi(
    iter: kitty_graphics::PlacementIterator,
    count: usize,
    keys: *const kitty_graphics::PlacementData,
    values: *const *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { kitty_graphics::placement_get_multi(iter, count, keys, values, out_written) }
        .to_c_int()
}

// pub const kitty_graphics_placement_rect = kitty_graphics.placement_rect;
/// C: rustty_kitty_graphics_placement_rect
///
/// # Safety
/// `iter`/`image`/`terminal`, if non-null, must be live; `out` must be a valid,
/// writable pointer to a `CSelection`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_rect(
    iter: kitty_graphics::PlacementIterator,
    image: kitty_graphics::ImageHandle,
    terminal: terminal::Terminal,
    out: *mut selection::CSelection,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { kitty_graphics::placement_rect(iter, image, terminal, out) }.to_c_int()
}

// pub const kitty_graphics_placement_pixel_size = kitty_graphics.placement_pixel_size;
/// C: rustty_kitty_graphics_placement_pixel_size
///
/// # Safety
/// `iter`/`image`/`terminal`, if non-null, must be live; `out_width`/`out_height`
/// must be valid, writable pointers to `u32`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_pixel_size(
    iter: kitty_graphics::PlacementIterator,
    image: kitty_graphics::ImageHandle,
    terminal: terminal::Terminal,
    out_width: *mut u32,
    out_height: *mut u32,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { kitty_graphics::placement_pixel_size(iter, image, terminal, out_width, out_height) }
        .to_c_int()
}

// pub const kitty_graphics_placement_grid_size = kitty_graphics.placement_grid_size;
/// C: rustty_kitty_graphics_placement_grid_size
///
/// # Safety
/// `iter`/`image`/`terminal`, if non-null, must be live; `out_cols`/`out_rows`
/// must be valid, writable pointers to `u32`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_grid_size(
    iter: kitty_graphics::PlacementIterator,
    image: kitty_graphics::ImageHandle,
    terminal: terminal::Terminal,
    out_cols: *mut u32,
    out_rows: *mut u32,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { kitty_graphics::placement_grid_size(iter, image, terminal, out_cols, out_rows) }
        .to_c_int()
}

// pub const kitty_graphics_placement_viewport_pos = kitty_graphics.placement_viewport_pos;
/// C: rustty_kitty_graphics_placement_viewport_pos
///
/// # Safety
/// `iter`/`image`/`terminal`, if non-null, must be live; `out_col`/`out_row` must
/// be valid, writable pointers to `i32`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_viewport_pos(
    iter: kitty_graphics::PlacementIterator,
    image: kitty_graphics::ImageHandle,
    terminal: terminal::Terminal,
    out_col: *mut i32,
    out_row: *mut i32,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { kitty_graphics::placement_viewport_pos(iter, image, terminal, out_col, out_row) }
        .to_c_int()
}

// pub const kitty_graphics_placement_source_rect = kitty_graphics.placement_source_rect;
/// C: rustty_kitty_graphics_placement_source_rect
///
/// # Safety
/// `iter`/`image`, if non-null, must be live; the four `out_*` pointers must be
/// valid, writable pointers to `u32`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_source_rect(
    iter: kitty_graphics::PlacementIterator,
    image: kitty_graphics::ImageHandle,
    out_x: *mut u32,
    out_y: *mut u32,
    out_width: *mut u32,
    out_height: *mut u32,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe {
        kitty_graphics::placement_source_rect(iter, image, out_x, out_y, out_width, out_height)
    }
    .to_c_int()
}

// pub const kitty_graphics_placement_render_info = kitty_graphics.placement_render_info;
/// C: rustty_kitty_graphics_placement_render_info
///
/// # Safety
/// `iter`/`image`/`terminal`, if non-null, must be live; `out`, if non-null, must
/// be a valid, writable pointer to a `PlacementRenderInfo`.
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_render_info(
    iter: kitty_graphics::PlacementIterator,
    image: kitty_graphics::ImageHandle,
    terminal: terminal::Terminal,
    out: *mut kitty_graphics::PlacementRenderInfo,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { kitty_graphics::placement_render_info(iter, image, terminal, out) }.to_c_int()
}

// ----- render -----
// PORT NOTE: 1:1 with the 19 `@export(&c.render_state_*, ...)` lines in
// `lib_vt.zig`. The `c.zig` aliases map `render_state_<name> -> render.<name>`
// (e.g. `render_state_new -> render.new`). The `intToEnum` runtime-safety guards
// for `Data`/`SetOption`/`RowData`/`RowOption`/`RowCellsData` live here on the
// raw `c_int` crossing the C boundary; an out-of-range value returns
// `INVALID_VALUE` (mirroring the reference `log.warn` + early return).

// pub const render_state_new = render.new;
/// C: rustty_render_state_new
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a render-state handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_new(
    alloc_: *const rustty_lib::allocator::Allocator,
    result: *mut render::RenderState,
) -> c_int {
    // SAFETY: `alloc_`/`result` validity is the caller's contract.
    unsafe { render::new(alloc_.as_ref(), result) }.to_c_int()
}

// pub const render_state_free = render.free;
/// C: rustty_render_state_free
///
/// # Safety
/// `state`, if non-null, must be a handle returned by `rustty_render_state_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_free(state: render::RenderState) {
    // SAFETY: `state` validity is the caller's contract.
    unsafe { render::free(state) }
}

// pub const render_state_update = render.update;
/// C: rustty_render_state_update
///
/// # Safety
/// `state`/`terminal`, if non-null, must be live handles.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_update(
    state: render::RenderState,
    terminal: terminal::Terminal,
) -> c_int {
    // SAFETY: `state`/`terminal` validity is the caller's contract.
    unsafe { render::update(state, terminal) }.to_c_int()
}

// pub const render_state_get = render.get;
/// C: rustty_render_state_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_get(
    state: render::RenderState,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(Data, ...) catch { log.warn(...); return .invalid_value; }
    let Some(d) = render_data_from_c_int(data) else {
        // log.warn("render_state_get invalid data value={d}", .{...});
        eprintln!("render_state_get invalid data value={data}");
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { render::get(state, d, out) }.to_c_int()
}

// pub const render_state_get_multi = render.get_multi;
/// C: rustty_render_state_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_get_multi(
    state: render::RenderState,
    count: usize,
    keys: *const render::Data,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { render::get_multi(state, count, keys, values, out_written) }.to_c_int()
}

// pub const render_state_set = render.set;
/// C: rustty_render_state_set
///
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_set(
    state: render::RenderState,
    option: c_int,
    value: *const std::ffi::c_void,
) -> c_int {
    use render::SetOption;
    // _ = std.meta.intToEnum(SetOption, ...) catch { log.warn(...); return .invalid_value; }
    let opt = match option {
        0 => SetOption::Dirty,
        _ => {
            // log.warn("render_state_set invalid option value={d}", .{...});
            eprintln!("render_state_set invalid option value={option}");
            return result::Result::InvalidValue.to_c_int();
        }
    };
    // SAFETY: `value` validity for `opt` is the caller's contract.
    unsafe { render::set(state, opt, value) }.to_c_int()
}

// pub const render_state_colors_get = render.colors_get;
/// C: rustty_render_state_colors_get
///
/// # Safety
/// `out_colors`, if non-null, must be a valid, writable pointer to a `Colors`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_colors_get(
    state: render::RenderState,
    out_colors: *mut render::Colors,
) -> c_int {
    // SAFETY: `out_colors` validity is the caller's contract.
    unsafe { render::colors_get(state, out_colors) }.to_c_int()
}

// pub const render_state_row_iterator_new = render.row_iterator_new;
/// C: rustty_render_state_row_iterator_new
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a row-iterator handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_iterator_new(
    alloc_: *const rustty_lib::allocator::Allocator,
    result: *mut render::RowIterator,
) -> c_int {
    // SAFETY: `alloc_`/`result` validity is the caller's contract.
    unsafe { render::row_iterator_new(alloc_.as_ref(), result) }.to_c_int()
}

// pub const render_state_row_iterator_next = render.row_iterator_next;
/// C: rustty_render_state_row_iterator_next
///
/// # Safety
/// `iterator`, if non-null, must be a live iterator.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_iterator_next(
    iterator: render::RowIterator,
) -> bool {
    // SAFETY: `iterator` validity is the caller's contract.
    unsafe { render::row_iterator_next(iterator) }
}

// pub const render_state_row_iterator_free = render.row_iterator_free;
/// C: rustty_render_state_row_iterator_free
///
/// # Safety
/// `iterator`, if non-null, must be a handle returned by the matching `_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_iterator_free(iterator: render::RowIterator) {
    // SAFETY: `iterator` validity is the caller's contract.
    unsafe { render::row_iterator_free(iterator) }
}

// pub const render_state_row_get = render.row_get;
/// C: rustty_render_state_row_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_get(
    iterator: render::RowIterator,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(RowData, ...) catch { log.warn(...); return .invalid_value; }
    let Some(d) = render_row_data_from_c_int(data) else {
        // log.warn("render_state_row_get invalid data value={d}", .{...});
        eprintln!("render_state_row_get invalid data value={data}");
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { render::row_get(iterator, d, out) }.to_c_int()
}

// pub const render_state_row_get_multi = render.row_get_multi;
/// C: rustty_render_state_row_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_get_multi(
    iterator: render::RowIterator,
    count: usize,
    keys: *const render::RowData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { render::row_get_multi(iterator, count, keys, values, out_written) }.to_c_int()
}

// pub const render_state_row_set = render.row_set;
/// C: rustty_render_state_row_set
///
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_set(
    iterator: render::RowIterator,
    option: c_int,
    value: *const std::ffi::c_void,
) -> c_int {
    use render::RowOption;
    // _ = std.meta.intToEnum(RowOption, ...) catch { log.warn(...); return .invalid_value; }
    let opt = match option {
        0 => RowOption::Dirty,
        _ => {
            // log.warn("render_state_row_set invalid option value={d}", .{...});
            eprintln!("render_state_row_set invalid option value={option}");
            return result::Result::InvalidValue.to_c_int();
        }
    };
    // SAFETY: `value` validity for `opt` is the caller's contract.
    unsafe { render::row_set(iterator, opt, value) }.to_c_int()
}

// pub const render_state_row_cells_new = render.row_cells_new;
/// C: rustty_render_state_row_cells_new
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a row-cells handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_cells_new(
    alloc_: *const rustty_lib::allocator::Allocator,
    result: *mut render::RowCells,
) -> c_int {
    // SAFETY: `alloc_`/`result` validity is the caller's contract.
    unsafe { render::row_cells_new(alloc_.as_ref(), result) }.to_c_int()
}

// pub const render_state_row_cells_next = render.row_cells_next;
/// C: rustty_render_state_row_cells_next
///
/// # Safety
/// `cells`, if non-null, must be a live row-cells handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_cells_next(cells: render::RowCells) -> bool {
    // SAFETY: `cells` validity is the caller's contract.
    unsafe { render::row_cells_next(cells) }
}

// pub const render_state_row_cells_select = render.row_cells_select;
/// C: rustty_render_state_row_cells_select
///
/// # Safety
/// `cells`, if non-null, must be a live row-cells handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_cells_select(
    cells: render::RowCells,
    x: rustty_vt::size::CellCountInt,
) -> c_int {
    // SAFETY: `cells` validity is the caller's contract.
    unsafe { render::row_cells_select(cells, x) }.to_c_int()
}

// pub const render_state_row_cells_free = render.row_cells_free;
/// C: rustty_render_state_row_cells_free
///
/// # Safety
/// `cells`, if non-null, must be a handle returned by the matching `_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_cells_free(cells: render::RowCells) {
    // SAFETY: `cells` validity is the caller's contract.
    unsafe { render::row_cells_free(cells) }
}

// pub const render_state_row_cells_get = render.row_cells_get;
/// C: rustty_render_state_row_cells_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_cells_get(
    cells: render::RowCells,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(RowCellsData, ...) catch { log.warn(...); return .invalid_value; }
    let Some(d) = render_row_cells_data_from_c_int(data) else {
        // log.warn("render_state_row_cells_get invalid data value={d}", .{...});
        eprintln!("render_state_row_cells_get invalid data value={data}");
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { render::row_cells_get(cells, d, out) }.to_c_int()
}

// pub const render_state_row_cells_get_multi = render.row_cells_get_multi;
/// C: rustty_render_state_row_cells_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_cells_get_multi(
    cells: render::RowCells,
    count: usize,
    keys: *const render::RowCellsData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { render::row_cells_get_multi(cells, count, keys, values, out_written) }.to_c_int()
}

// PORT NOTE: range-checked `c_int -> enum` projections for the render getters/
// setters (the `std.meta.intToEnum` guards). `Data`/`RowData`/`RowCellsData` are
// contiguous `#[repr(C)]` enums; the range check IS `intToEnum`, after which a
// `transmute` from the validated `c_int` is the enum.
const RENDER_DATA_COUNT: c_int = 18; // contiguous 0..=17
fn render_data_from_c_int(v: c_int) -> Option<render::Data> {
    if (0..RENDER_DATA_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `render::Data` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, render::Data>(v) })
    } else {
        None
    }
}

const RENDER_ROW_DATA_COUNT: c_int = 5; // contiguous 0..=4
fn render_row_data_from_c_int(v: c_int) -> Option<render::RowData> {
    if (0..RENDER_ROW_DATA_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `render::RowData` discriminant (range-checked).
        Some(unsafe { std::mem::transmute::<c_int, render::RowData>(v) })
    } else {
        None
    }
}

const RENDER_ROW_CELLS_DATA_COUNT: c_int = 10; // contiguous 0..=9
fn render_row_cells_data_from_c_int(v: c_int) -> Option<render::RowCellsData> {
    if (0..RENDER_ROW_CELLS_DATA_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `render::RowCellsData` discriminant (range-checked).
        Some(unsafe { std::mem::transmute::<c_int, render::RowCellsData>(v) })
    } else {
        None
    }
}

// ----- selection_gesture -----
// PORT NOTE: 1:1 with the 9 `@export(&c.selection_gesture_*, ...)` lines in
// `lib_vt.zig`. The `c.zig` aliases map `selection_gesture_<name> ->
// selection_gesture.<name>`, with `selection_gesture_event ->
// selection_gesture.handle_event`. The `intToEnum` guards for
// `Data`/`EventType`/`EventOption` live here on the raw `c_int`.

// pub const selection_gesture_new = selection_gesture.new;
/// C: rustty_selection_gesture_new
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid allocator; `out_gesture`, if
/// non-null, must be a valid, writable pointer to a gesture handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_new(
    alloc_: *const rustty_lib::allocator::Allocator,
    out_gesture: *mut selection_gesture::Gesture,
) -> c_int {
    // SAFETY: `alloc_`/`out_gesture` validity is the caller's contract.
    unsafe { selection_gesture::new(alloc_.as_ref(), out_gesture) }.to_c_int()
}

// pub const selection_gesture_free = selection_gesture.free;
/// C: rustty_selection_gesture_free
///
/// # Safety
/// `gesture`, if non-null, must be a handle returned by `_new`; `terminal`, if
/// non-null, must be a live terminal handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_free(
    gesture: selection_gesture::Gesture,
    terminal: terminal::Terminal,
) {
    // SAFETY: `gesture`/`terminal` validity is the caller's contract.
    unsafe { selection_gesture::free(gesture, terminal) }
}

// pub const selection_gesture_reset = selection_gesture.reset;
/// C: rustty_selection_gesture_reset
///
/// # Safety
/// `gesture`/`terminal`, if non-null, must be live handles.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_reset(
    gesture: selection_gesture::Gesture,
    terminal: terminal::Terminal,
) {
    // SAFETY: `gesture`/`terminal` validity is the caller's contract.
    unsafe { selection_gesture::reset(gesture, terminal) }
}

// pub const selection_gesture_event = selection_gesture.handle_event;
/// C: rustty_selection_gesture_event
///
/// # Safety
/// `gesture`/`terminal`/`event`, if non-null, must be live handles;
/// `out_selection`, if non-null, must be a valid, writable pointer to a `CSelection`.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_event(
    gesture: selection_gesture::Gesture,
    terminal: terminal::Terminal,
    event: selection_gesture::Event,
    out_selection: *mut selection::CSelection,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection_gesture::handle_event(gesture, terminal, event, out_selection) }.to_c_int()
}

// pub const selection_gesture_get = selection_gesture.get;
/// C: rustty_selection_gesture_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_get(
    gesture: selection_gesture::Gesture,
    terminal: terminal::Terminal,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(Data, ...) catch { log.warn(...); return .invalid_value; }
    let Some(d) = selection_gesture_data_from_c_int(data) else {
        // log.warn("selection_gesture_get invalid data value={d}", .{...});
        eprintln!("selection_gesture_get invalid data value={data}");
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { selection_gesture::get(gesture, terminal, d, out) }.to_c_int()
}

// pub const selection_gesture_get_multi = selection_gesture.get_multi;
/// C: rustty_selection_gesture_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_get_multi(
    gesture: selection_gesture::Gesture,
    terminal: terminal::Terminal,
    count: usize,
    keys: *const selection_gesture::Data,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection_gesture::get_multi(gesture, terminal, count, keys, values, out_written) }
        .to_c_int()
}

// pub const selection_gesture_event_new = selection_gesture.event_new;
/// C: rustty_selection_gesture_event_new
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid allocator; `out_event`, if
/// non-null, must be a valid, writable pointer to an event handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_event_new(
    alloc_: *const rustty_lib::allocator::Allocator,
    out_event: *mut selection_gesture::Event,
    event_type: c_int,
) -> c_int {
    // _ = std.meta.intToEnum(EventType, ...) catch return .invalid_value;
    let Some(et) = selection_gesture_event_type_from_c_int(event_type) else {
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `alloc_`/`out_event` validity is the caller's contract.
    unsafe { selection_gesture::event_new(alloc_.as_ref(), out_event, et) }.to_c_int()
}

// pub const selection_gesture_event_free = selection_gesture.event_free;
/// C: rustty_selection_gesture_event_free
///
/// # Safety
/// `event`, if non-null, must be a handle returned by `_event_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_event_free(event: selection_gesture::Event) {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { selection_gesture::event_free(event) }
}

// pub const selection_gesture_event_set = selection_gesture.event_set;
/// C: rustty_selection_gesture_event_set
///
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_event_set(
    event: selection_gesture::Event,
    option: c_int,
    value: *const std::ffi::c_void,
) -> c_int {
    // if (comptime std.debug.runtime_safety) { _ = intToEnum(EventOption, ...) catch { log.warn(...); return .invalid_value; } }
    let Some(opt) = selection_gesture_event_option_from_c_int(option) else {
        // log.warn("selection_gesture_event_set invalid option value={d}", .{...});
        eprintln!("selection_gesture_event_set invalid option value={option}");
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `value` validity for `opt` is the caller's contract.
    unsafe { selection_gesture::event_set(event, opt, value) }.to_c_int()
}

// PORT NOTE: range-checked `c_int -> enum` projections for the selection_gesture
// guards. `Data`/`EventType`/`EventOption` are contiguous `#[repr(C)]` enums.
const SELECTION_GESTURE_DATA_COUNT: c_int = 5; // contiguous 0..=4
fn selection_gesture_data_from_c_int(v: c_int) -> Option<selection_gesture::Data> {
    if (0..SELECTION_GESTURE_DATA_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `Data` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, selection_gesture::Data>(v) })
    } else {
        None
    }
}

const SELECTION_GESTURE_EVENT_TYPE_COUNT: c_int = 5; // contiguous 0..=4
fn selection_gesture_event_type_from_c_int(v: c_int) -> Option<selection_gesture::EventType> {
    if (0..SELECTION_GESTURE_EVENT_TYPE_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `EventType` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, selection_gesture::EventType>(v) })
    } else {
        None
    }
}

const SELECTION_GESTURE_EVENT_OPTION_COUNT: c_int = 10; // contiguous 0..=9
fn selection_gesture_event_option_from_c_int(v: c_int) -> Option<selection_gesture::EventOption> {
    if (0..SELECTION_GESTURE_EVENT_OPTION_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `EventOption` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, selection_gesture::EventOption>(v) })
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
        // _ = kitty_graphics; (main.zig line 219)
        let _ = super::kitty_graphics::get as unsafe fn(_, _, _) -> _;
        // _ = render; _ = selection_gesture;
        let _ = super::render::get as unsafe fn(_, _, _) -> _;
        let _ = super::selection_gesture::get as unsafe fn(_, _, _, _) -> _;
    }
}
