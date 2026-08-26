//! rustty-vt-c
//!
//! PORTS: ghostty src/terminal/c/ (the C ABI layer, break #4). The Rust
//! enums/types in rustty-vt are the source of truth; this crate is a pure FFI
//! projection (R3). cdylib+staticlib+rlib; cbindgen -> include/rustty.h with
//! `rustty_*` symbols (R2).
//!
//! PORT STATUS: done (flipped per VALIDATION_REPORT.md M3: all 34 c/
//! modules are landed — terminal, render, kitty_graphics, selection*,
//! formatter, key/mouse, sgr, osc, cell, row, style, grid_ref, size_report,
//! types included — with zero live todo!(); completion ledgered in
//! DEFERRALS.md under "vt-c").
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
pub mod color_scheme;
mod cwrap;
pub mod focus;
pub mod formatter;
pub mod grid_ref;
pub mod grid_ref_tracked;
pub mod io;
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
pub mod snapshot;
pub mod style;
pub mod sys;
pub mod terminal;
#[cfg(test)]
mod test_util;
pub mod unicode;
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
    // tag). `log.warn(...)` routes through the C-ABI log sink (`sys::log_fn`,
    // the `std_options.logFn` analog installed by `lib_vt.zig:363-381`) with
    // the reference's scope (`build_info.zig:7` `std.log.scoped(.build_info_c)`):
    // delivered to an installed `rustty_sys_set(RUSTTY_SYS_OPT_LOG, ...)`
    // callback, silently discarded when none is installed.
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
            sys::log_fn(
                sys::StdLevel::Warn,
                "build_info_c",
                format_args!("build_info invalid data value={data}"),
            );
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
/// `color` must be a valid readable `RgbC`; `r`/`g`/`b` valid writable `u8`.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_rgb_get(
    color: *const color::RgbC,
    r: *mut u8,
    g: *mut u8,
    b: *mut u8,
) {
    // SAFETY: caller contract (see # Safety).
    unsafe { color::rgb_get(color, r, g, b) }
}

// pub const color_parse = color.parse;  (ghostty_color_parse)
/// C: rustty_color_parse
/// # Safety
/// `value`, if non-null, readable for `len` bytes; `out` valid writable.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_parse(
    value: *const u8,
    len: usize,
    out: *mut color::RgbC,
) -> c_int {
    // SAFETY: caller contract.
    unsafe { color::parse(value, len, out) }.to_c_int()
}

/// C: rustty_color_parse_x11
/// # Safety
/// `name`, if non-null, readable for `len` bytes; `out` valid writable.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_parse_x11(
    name: *const u8,
    len: usize,
    out: *mut color::RgbC,
) -> c_int {
    // SAFETY: caller contract.
    unsafe { color::parse_x11(name, len, out) }.to_c_int()
}

/// C: rustty_color_parse_palette_entry
/// # Safety
/// `value`, if non-null, readable for `len`; `out_index`/`out_rgb` writable.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_parse_palette_entry(
    value: *const u8,
    len: usize,
    out_index: *mut u8,
    out_rgb: *mut color::RgbC,
) -> c_int {
    // SAFETY: caller contract.
    unsafe { color::parse_palette_entry(value, len, out_index, out_rgb) }.to_c_int()
}

/// C: rustty_color_palette_default
/// # Safety
/// `out` must point to writable storage for 256 RGB values.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_palette_default(out: *mut color::PaletteC) {
    // SAFETY: caller contract.
    unsafe { color::palette_default(out) }
}

/// C: rustty_color_palette_generate
/// # Safety
/// `bg`/`fg`/`out` valid; `base`/`skip` valid or null; out may alias base.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_palette_generate(
    base: *const color::PaletteC,
    skip: *const color::PaletteMask,
    bg: *const color::RgbC,
    fg: *const color::RgbC,
    harmonious: bool,
    out: *mut color::PaletteC,
) {
    // SAFETY: caller contract.
    unsafe { color::palette_generate(base, skip, bg, fg, harmonious, out) }
}

/// C: rustty_color_luminance
/// # Safety
/// `color` must be a valid readable RGB pointer.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_luminance(color_: *const color::RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { color::luminance(color_) }
}

/// C: rustty_color_perceived_luminance
/// # Safety
/// `color` must be a valid readable RGB pointer.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_perceived_luminance(color_: *const color::RgbC) -> f64 {
    // SAFETY: caller contract.
    unsafe { color::perceived_luminance(color_) }
}

/// C: rustty_color_contrast
/// # Safety
/// `a`/`b` must be valid readable RGB pointers.
#[cfg(feature = "color")]
#[no_mangle]
pub unsafe extern "C" fn rustty_color_contrast(
    a: *const color::RgbC,
    b: *const color::RgbC,
) -> f64 {
    // SAFETY: caller contract.
    unsafe { color::contrast(a, b) }
}

/// C: rustty_color_x11_names
#[cfg(feature = "color")]
#[no_mangle]
pub extern "C" fn rustty_color_x11_names() -> *const color::X11Entry {
    color::x11_names()
}

/// C: rustty_color_x11_name_count
#[cfg(feature = "color")]
#[no_mangle]
pub extern "C" fn rustty_color_x11_name_count() -> usize {
    color::x11_name_count()
}

// ----- color_scheme -----
// pub const color_scheme_report_encode = color_scheme.report_encode;
//   (@export name ghostty_color_scheme_report_encode)
/// C: rustty_color_scheme_report_encode
/// # Safety
/// `buf`, if non-null, writable for `buf_len`; `out_written` valid writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_color_scheme_report_encode(
    scheme: c_int,
    buf: *mut u8,
    buf_len: usize,
    out_written: *mut usize,
) -> c_int {
    use rustty_vt::device_status::ColorScheme;
    // Integer-range guard on the raw c_int crossing the C boundary
    // (RusttyColorScheme: LIGHT=0, DARK=1 — see include/rustty/vt/device.h).
    let scheme = match scheme {
        0 => ColorScheme::Light,
        1 => ColorScheme::Dark,
        _ => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: caller contract.
    unsafe { color_scheme::report_encode(scheme, buf, buf_len, out_written) }.to_c_int()
}

// ----- focus -----
// pub const focus_encode = focus.encode;  (ghostty_focus_encode)
/// C: rustty_focus_encode
///
/// # Safety
/// `out`, if non-null, must point to `out_len` writable bytes; `out_written`
/// must be a valid, writable pointer to `usize`.
#[cfg(feature = "input-encode")]
#[no_mangle]
pub unsafe extern "C" fn rustty_focus_encode(
    // D11: inbound as the C integer, narrowed below. See `FocusEvent::from_c_int`.
    event: c_int,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    let event = match focus::FocusEvent::from_c_int(event) {
        Some(e) => e,
        None => return result::Result::InvalidValue.to_c_int(),
    };
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
// pub const alloc_alloc = allocator.alloc;  (@export name ghostty_alloc)
/// C: rustty_alloc
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid C allocator. The returned
/// pointer (if non-null) must be released with `rustty_free`.
#[no_mangle]
pub unsafe extern "C" fn rustty_alloc(
    alloc_: *const rustty_lib::allocator::Allocator,
    len: usize,
) -> *mut u8 {
    // SAFETY: `alloc_` validity is the caller's contract.
    unsafe { allocator::alloc(alloc_.as_ref(), len) }
}

// pub const alloc_free = allocator.free;  (@export name ghostty_free)
/// C: rustty_free
///
/// # Safety
/// `ptr`, if non-null, must be a buffer previously returned by
/// `rustty_alloc` with the same `alloc_` and `len`.
#[no_mangle]
pub unsafe extern "C" fn rustty_free(
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
    // D11: inbound as the C integer, narrowed below. See `LogLevel::from_c_int`.
    level: c_int,
    scope_ptr: *const u8,
    scope_len: usize,
    message_ptr: *const u8,
    message_len: usize,
) {
    // This export returns `void`, so there is no error channel to report an
    // out-of-range level on. Dropping the record is the same disposition the
    // `key_event` setters already use for an out-of-range `intToEnum` on this
    // boundary (see the PORT NOTE above `rustty_key_event_new`), and it is
    // strictly better than the alternative it replaces, which was UB.
    let level = match sys::LogLevel::from_c_int(level) {
        Some(l) => l,
        None => return,
    };
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
        3 => Option_::RandomSecure,
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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

// pub const terminal_paste = paste.terminal_paste;  (ghostty_terminal_paste)
/// C: rustty_terminal_paste
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `req`, if
/// non-null, must point to a readable `RusttyPaste` whose `mimes`/`reader`
/// describe valid memory; `out_written`, if non-null, must be a valid,
/// writable pointer to `bool`.
#[cfg(feature = "input-encode")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_paste(
    terminal: terminal::Terminal,
    req: *const paste::Request,
    out_written: *mut bool,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { paste::terminal_paste(terminal, req, out_written) }.to_c_int()
}

// ----- unicode -----
// pub const unicode_codepoint_width = unicode.codepoint_width;
//   (@export name ghostty_unicode_codepoint_width)
/// C: rustty_unicode_codepoint_width
#[no_mangle]
pub extern "C" fn rustty_unicode_codepoint_width(cp: u32) -> u8 {
    unicode::codepoint_width(cp)
}

// pub const unicode_grapheme_width = unicode.grapheme_width;
//   (@export name ghostty_unicode_grapheme_width)
/// C: rustty_unicode_grapheme_width
///
/// # Safety
/// `cps`, if non-null, must be readable for `len` u32s; `width` writable or null.
#[no_mangle]
pub unsafe extern "C" fn rustty_unicode_grapheme_width(
    cps: *const u32,
    len: usize,
    width: *mut u8,
) -> usize {
    // SAFETY: caller contract.
    unsafe { unicode::grapheme_width(cps, len, width) }
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
    // D11: inbound as the C integer, narrowed below. See `Style::from_c_int`.
    style: c_int,
    size: size_report::Size,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    let style = match size_report::SizeReportStyle::from_c_int(style) {
        Some(s) => s,
        None => return result::Result::InvalidValue.to_c_int(),
    };
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_action(event: key_event::Event, action: c_int) {
    // _ = std.meta.intToEnum(key.Action, ...) catch { log.warn(...); return; };
    let Some(a) = key_action_from_c_int(action) else {
        // log.warn("set_action invalid action value={d}", .{...});
        sys::log_fn(
            sys::StdLevel::Warn,
            "key_event",
            format_args!("set_action invalid action value={action}"),
        );
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_set_key(event: key_event::Event, k: c_int) {
    // _ = std.meta.intToEnum(key.Key, ...) catch { log.warn(...); return; };
    let Some(key) = key_key_from_c_int(k) else {
        // log.warn("set_key invalid key value={d}", .{...});
        sys::log_fn(
            sys::StdLevel::Warn,
            "key_event",
            format_args!("set_key invalid key value={k}"),
        );
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
#[no_mangle]
pub unsafe extern "C" fn rustty_key_event_get_unshifted_codepoint(event: key_event::Event) -> u32 {
    // SAFETY: `event` validity is the caller's contract.
    unsafe { key_event::get_unshifted_codepoint(event) }
}

// PORT NOTE (`-Dvt-features`): the `c_int -> enum` projections below, and the
// selection-gesture pair further down, are private helpers of ONE export group
// each, so a build that trims that group leaves them unreferenced. Zig simply
// never analyzes an unreferenced fn, so the reference's `if (features.X)` gating
// costs it nothing; Rust compiles them and warns. Each therefore carries the
// same `#[cfg]` as the exports it serves — a cfg rather than an
// `allow(dead_code)`, which would also blind the DEFAULT build.
//
// PORT NOTE: range-checked `c_int -> enum` projections for the guarded setters.
// `key::Key`/`key::Action`/`mouse::Action`/`mouse::Button` are contiguous
// `#[repr(C)]` enums; the range check IS `std.meta.intToEnum` (reject out-of-
// range), after which a `transmute` from the validated `c_int` is the enum.
#[cfg(feature = "input-encode")]
const KEY_KEY_COUNT: c_int = 176; // contiguous 0..=175

#[cfg(feature = "input-encode")]
fn key_key_from_c_int(v: c_int) -> Option<rustty_vt::input::key::Key> {
    if (0..KEY_KEY_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `key::Key` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, rustty_vt::input::key::Key>(v) })
    } else {
        None
    }
}

#[cfg(feature = "input-encode")]
fn key_action_from_c_int(v: c_int) -> Option<rustty_vt::input::key::Action> {
    match v {
        0 => Some(rustty_vt::input::key::Action::Release),
        1 => Some(rustty_vt::input::key::Action::Press),
        2 => Some(rustty_vt::input::key::Action::Repeat),
        _ => None,
    }
}

#[cfg(feature = "input-encode")]
fn mouse_action_from_c_int(v: c_int) -> Option<rustty_vt::input::mouse::Action> {
    match v {
        0 => Some(rustty_vt::input::mouse::Action::Press),
        1 => Some(rustty_vt::input::mouse::Action::Release),
        2 => Some(rustty_vt::input::mouse::Action::Motion),
        _ => None,
    }
}

#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_set_action(event: mouse_event::Event, action: c_int) {
    // _ = std.meta.intToEnum(Action, ...) catch { log.warn(...); return; };
    let Some(a) = mouse_action_from_c_int(action) else {
        // log.warn("set_action invalid action value={d}", .{...});
        sys::log_fn(
            sys::StdLevel::Warn,
            "mouse_event",
            format_args!("set_action invalid action value={action}"),
        );
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
#[no_mangle]
pub unsafe extern "C" fn rustty_mouse_event_set_button(event: mouse_event::Event, button: c_int) {
    // _ = std.meta.intToEnum(Button, ...) catch { log.warn(...); return; };
    let Some(b) = mouse_button_from_c_int(button) else {
        // log.warn("set_button invalid button value={d}", .{...});
        sys::log_fn(
            sys::StdLevel::Warn,
            "mouse_event",
            format_args!("set_button invalid button value={button}"),
        );
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
#[cfg(feature = "input-encode")]
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
// PORT NOTE (W83-B01-01): the `cell_get` `intToEnum(CellData, ...)`
// runtime-safety guard on the raw `c_int` crossing the C boundary MOVED to
// `cell::CellData::from_c_int` / `cell::get_checked`, so the guard is shared by
// this single-get wrapper AND by the `cell_get_multi` element loop — matching
// the reference, where `get_multi` reaches the guard by calling the guarded
// `get`. An out-of-range value returns `INVALID_VALUE` (mirroring the reference
// early return).

// pub const cell_get = cell.get;  (ghostty_cell_get)
/// C: rustty_cell_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_cell_get(
    cell_: cell::CCell,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // SAFETY: `out` validity is the caller's contract.
    unsafe { cell::get_checked(cell_, data, out) }.to_c_int()
}

// pub const cell_get_multi = cell.get_multi;  (ghostty_cell_get_multi)
/// C: rustty_cell_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_cell_get_multi(
    cell_: cell::CCell,
    count: usize,
    keys: *const cell::CellData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // PORT NOTE (W83-B01-01): the parameter type stays `*const cell::CellData`
    // so the published C signature keeps `const RusttyCellData *keys`, but the
    // ARRAY ELEMENTS are C-supplied integers: they are read as raw `c_int`
    // (ABI-identical — `CellData` is `#[repr(C)]`, asserted int-sized) and
    // range-checked per element, because materializing an out-of-range
    // discriminant as a Rust enum is UB in every build mode.
    // SAFETY: pointer validity is the caller's contract.
    unsafe { cell::get_multi(cell_, count, keys as *const c_int, values, out_written) }.to_c_int()
}

// ----- row -----
// pub const row_get = row.get;  (ghostty_row_get)
/// C: rustty_row_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_row_get(
    row_: row::CRow,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // SAFETY: `out` validity is the caller's contract.
    unsafe { row::get_checked(row_, data, out) }.to_c_int()
}

// pub const row_get_multi = row.get_multi;  (ghostty_row_get_multi)
/// C: rustty_row_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_row_get_multi(
    row_: row::CRow,
    count: usize,
    keys: *const row::RowData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // PORT NOTE (W84-B01-01): the parameter type stays `*const row::RowData` so
    // the published C signature keeps `const RusttyRowData *keys`, but the
    // ARRAY ELEMENTS are C-supplied integers: they are read as raw `c_int`
    // (ABI-identical — `RowData` is `#[repr(C)]`, asserted int-sized) and
    // range-checked per element, because materializing an out-of-range
    // discriminant as a Rust enum is UB in every build mode.
    // SAFETY: pointer validity is the caller's contract.
    unsafe { row::get_multi(row_, count, keys as *const c_int, values, out_written) }.to_c_int()
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

// PORT NOTE: the range-checked `c_int -> enum` projections for the cell/row
// getters (the `std.meta.intToEnum` guards) MOVED to `cell::CellData::from_c_int`
// / `row::RowData::from_c_int` under W83-B01-01/W84-B01-01, so each guard (and
// its `get_checked` entry point) is shared by the single-get wrappers above and
// by the `*_get_multi` element loops — matching the reference, where `get_multi`
// reaches the guard by calling the guarded `get`.

// PORT NOTE: range-checked `c_int -> enum` projection for the terminal set
// guard (the `std.meta.intToEnum` guard on the raw `c_int` crossing the C
// boundary). `Option` is a contiguous `#[repr(C)]` enum.
//
// The `TerminalData` projection (with its explicit hole list) moved to
// `terminal::TerminalData::from_c_int` under W35-B08 — `get_multi` must reach
// the same guard per array element, so it can no longer live in this file.
//
// contiguous 0..=39 (27/28 = scrollback max bytes/lines; 29/30 = the
// desktop-notification and progress-report effects, oracle c3655ba25 47d602c42;
// 31 = continuation_max_bytes, Bundle S d7bb4b863, closing the hole Bundle M
// reserved for it; 32..=37 = the Bundle M options; 38 = clipboard_read,
// oracle e03475c0c; 39 = clipboard_write_max_bytes, oracle 5350d4a5f)
const TERMINAL_OPTION_COUNT: c_int = 40;
fn terminal_option_from_c_int(v: c_int) -> Option<terminal::Option_> {
    if (0..TERMINAL_OPTION_COUNT).contains(&v) {
        // SAFETY: `v` is a valid terminal `Option` discriminant (range-checked).
        Some(unsafe { std::mem::transmute::<c_int, terminal::Option_>(v) })
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
pub extern "C" fn rustty_sgr_attribute_tag(attr: sgr::AttributeC) -> c_int {
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
#[cfg(feature = "formatter")]
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

// pub const formatter_format = formatter.format;  (ghostty_formatter_format)
/// C: rustty_formatter_format
///
/// # Safety
/// `formatter`, if non-null, must be a handle returned by
/// `rustty_formatter_terminal_new`; `writer`'s callback contract is the
/// caller's (it must not call formatter or terminal APIs using the same
/// formatter or its terminal).
#[cfg(feature = "formatter")]
#[no_mangle]
pub unsafe extern "C" fn rustty_formatter_format(
    formatter: formatter::Formatter,
    writer: io::Writer,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { formatter::format(formatter, writer) }.to_c_int()
}

// pub const formatter_format_buf = formatter.format_buf;  (ghostty_formatter_format_buf)
/// C: rustty_formatter_format_buf
///
/// # Safety
/// `formatter`, if non-null, must be a handle returned by
/// `rustty_formatter_terminal_new`; `out` (if non-null) must point to `out_len`
/// writable bytes; `out_written` must be a valid, writable pointer.
#[cfg(feature = "formatter")]
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
#[cfg(feature = "formatter")]
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
#[cfg(feature = "formatter")]
#[no_mangle]
pub unsafe extern "C" fn rustty_formatter_free(formatter: formatter::Formatter) {
    // SAFETY: `formatter` validity is the caller's contract.
    unsafe { formatter::free(formatter) }
}

// ----- snapshot -----
// pub const snapshot_encode = snapshot.encode;  (ghostty_snapshot_encode)
/// C: rustty_snapshot_encode
///
/// # Safety
/// `terminal`, if non-null, must be a live `rustty_terminal_new` handle with no
/// concurrent mutation; `writer`'s callback must not call terminal APIs with
/// the same terminal handle.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_encode(
    terminal: terminal::Terminal,
    writer: io::Writer,
) -> c_int {
    // SAFETY: pointer/callback validity is the caller's contract.
    unsafe { snapshot::encode(terminal, writer) }.to_c_int()
}

// pub const snapshot_encode_buf = snapshot.encode_buf;  (ghostty_snapshot_encode_buf)
/// C: rustty_snapshot_encode_buf
///
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `buf`, if non-null,
/// must point to `buf_len` writable bytes; `out_written` must be writable.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_encode_buf(
    terminal: terminal::Terminal,
    buf: *mut u8,
    buf_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { snapshot::encode_buf(terminal, buf, buf_len, out_written) }.to_c_int()
}

// pub const snapshot_encode_alloc = snapshot.encode_alloc;  (ghostty_snapshot_encode_alloc)
/// C: rustty_snapshot_encode_alloc
///
/// # Safety
/// `terminal`, if non-null, must be a live terminal handle; `alloc`, if
/// non-null, a valid allocator; `out_ptr`/`out_len` must be writable. The
/// returned buffer is caller-owned and released with `rustty_free` + the same
/// allocator.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_encode_alloc(
    terminal: terminal::Terminal,
    alloc: *const rustty_lib::allocator::Allocator,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { snapshot::encode_alloc(terminal, alloc.as_ref(), out_ptr, out_len) }.to_c_int()
}

// pub const snapshot_decoder_new = snapshot.decoder_new;  (ghostty_snapshot_decoder_new)
/// C: rustty_snapshot_decoder_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `decoder` must be
/// writable; `reader`'s callback and userdata must remain valid until FINISH
/// is reached or the decoder is freed.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_new(
    alloc: *const rustty_lib::allocator::Allocator,
    decoder: *mut snapshot::Decoder,
    reader: io::Reader,
) -> c_int {
    // SAFETY: pointer/callback validity is the caller's contract.
    unsafe { snapshot::decoder_new(alloc.as_ref(), decoder, reader) }.to_c_int()
}

// pub const snapshot_decoder_new_buf = snapshot.decoder_new_buf;  (ghostty_snapshot_decoder_new_buf)
/// C: rustty_snapshot_decoder_new_buf
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `decoder` must be
/// writable; `ptr`, if non-null, must point to `len` bytes that remain valid
/// and immutable until FINISH is reached or the decoder is freed.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_new_buf(
    alloc: *const rustty_lib::allocator::Allocator,
    decoder: *mut snapshot::Decoder,
    ptr: *const u8,
    len: usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { snapshot::decoder_new_buf(alloc.as_ref(), decoder, ptr, len) }.to_c_int()
}

// pub const snapshot_decoder_free = snapshot.decoder_free;  (ghostty_snapshot_decoder_free)
/// C: rustty_snapshot_decoder_free
///
/// # Safety
/// `decoder`, if non-null, must be a handle returned by a decoder constructor,
/// not yet freed.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_free(decoder: snapshot::Decoder) {
    // SAFETY: `decoder` validity is the caller's contract.
    unsafe { snapshot::decoder_free(decoder) }
}

// pub const snapshot_decoder_set = snapshot.decoder_set;  (ghostty_snapshot_decoder_set)
/// C: rustty_snapshot_decoder_set
///
/// # Safety
/// `decoder`, if non-null, must be a live decoder handle; `value`, if
/// non-null, must point to the type documented for `option`.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_set(
    decoder: snapshot::Decoder,
    option: c_int,
    value: *const std::ffi::c_void,
) -> c_int {
    // D11: the raw C integer is narrowed here (the reference enum's open `_`
    // arm returns `.invalid_value` inside `decoder_set`).
    let Some(opt) = snapshot::DecoderOption::from_c_int(option) else {
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: pointer validity is the caller's contract.
    unsafe { snapshot::decoder_set(decoder, opt, value) }.to_c_int()
}

// pub const snapshot_decoder_get = snapshot.decoder_get;  (ghostty_snapshot_decoder_get)
/// C: rustty_snapshot_decoder_get
///
/// # Safety
/// `decoder`, if non-null, must be a live decoder handle; `out`, if non-null,
/// must point to writable storage of the type documented for `data`.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_get(
    decoder: snapshot::Decoder,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // D11: narrow the raw C integer (the reference's open `_` arm).
    let Some(key) = snapshot::DecoderData::from_c_int(data) else {
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: pointer validity is the caller's contract.
    unsafe { snapshot::decoder_get(decoder, key, out) }.to_c_int()
}

// pub const snapshot_decoder_get_multi = snapshot.decoder_get_multi;  (ghostty_snapshot_decoder_get_multi)
/// C: rustty_snapshot_decoder_get_multi
///
/// # Safety
/// `decoder`, if non-null, must be a live decoder handle; `keys`/`values`, if
/// non-null, must point to `count` elements; `out_written`, if non-null, must
/// be writable.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_get_multi(
    decoder: snapshot::Decoder,
    count: usize,
    keys: *const snapshot::DecoderData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract; the per-element key
    // narrowing lives inside `decoder_get_multi` (it reads the storage as raw
    // `c_int`s).
    unsafe { snapshot::decoder_get_multi(decoder, count, keys, values, out_written) }.to_c_int()
}

// pub const snapshot_decoder_ready = snapshot.decoder_ready;  (ghostty_snapshot_decoder_ready)
/// C: rustty_snapshot_decoder_ready
///
/// # Safety
/// `decoder`, if non-null, must be a live decoder handle; `terminal` must be
/// writable.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_ready(
    decoder: snapshot::Decoder,
    terminal: *mut terminal::Terminal,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { snapshot::decoder_ready(decoder, terminal) }.to_c_int()
}

// pub const snapshot_decoder_next = snapshot.decoder_next;  (ghostty_snapshot_decoder_next)
/// C: rustty_snapshot_decoder_next
///
/// # Safety
/// `decoder`, if non-null, must be a live decoder handle whose READY terminal
/// (if any) is still alive.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_next(decoder: snapshot::Decoder) -> c_int {
    // SAFETY: `decoder` validity is the caller's contract.
    unsafe { snapshot::decoder_next(decoder) }.to_c_int()
}

// pub const snapshot_decoder_decode = snapshot.decoder_decode;  (ghostty_snapshot_decoder_decode)
/// C: rustty_snapshot_decoder_decode
///
/// # Safety
/// `decoder`, if non-null, must be a live decoder handle; `terminal` must be
/// writable.
#[cfg(feature = "snapshot")]
#[no_mangle]
pub unsafe extern "C" fn rustty_snapshot_decoder_decode(
    decoder: snapshot::Decoder,
    terminal: *mut terminal::Terminal,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { snapshot::decoder_decode(decoder, terminal) }.to_c_int()
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

// ----- wasm allocator convenience exports -----
//
// PORT: `lib_vt.zig`'s `if (builtin.target.cpu.arch.isWasm()) { … }` block:
//
//     // On Wasm we need to export our allocator convenience functions.
//     if (builtin.target.cpu.arch.isWasm()) {
//         const alloc = @import("lib/allocator/wasm.zig");
//         @export(&alloc.allocBytes, .{ .name = "ghostty_wasm_alloc" });
//         @export(&alloc.freeBytes, .{ .name = "ghostty_wasm_free" });
//         @export(&alloc.allocOpaque, .{ .name = "ghostty_wasm_alloc_opaque" });
//         @export(&alloc.freeOpaque, .{ .name = "ghostty_wasm_free_opaque" });
//         @export(&alloc.takeOpaque, .{ .name = "ghostty_wasm_take_opaque" });
//     }
//
// rustty relies on pointers-to-types for ABI compatibility, and creating those
// in wasm from JS is tedious — the generic byte allocator plus the reusable
// opaque out-parameter slots let a wasm host (e.g. browser JS) manage caller-
// owned ABI storage against the module's own allocator without changing the C
// API. The bodies live in `rustty_lib::allocator::wasm` (the
// `lib/allocator/wasm.zig` analog); here we re-export them under the
// `rustty_wasm_*` names, wasm-only (matching the reference's `isWasm()` gate so
// native builds carry no extra symbols).
#[cfg(target_arch = "wasm32")]
mod wasm_alloc_exports {
    use rustty_lib::allocator::wasm as alloc;

    #[no_mangle]
    pub extern "C" fn rustty_wasm_alloc(len: usize) -> *mut u8 {
        alloc::alloc_bytes(len)
    }
    #[no_mangle]
    pub extern "C" fn rustty_wasm_free(ptr: *mut u8, len: usize) {
        alloc::free_bytes(ptr, len)
    }
    #[no_mangle]
    pub extern "C" fn rustty_wasm_alloc_opaque() -> *mut alloc::Opaque {
        alloc::alloc_opaque()
    }
    #[no_mangle]
    pub extern "C" fn rustty_wasm_free_opaque(ptr: *mut alloc::Opaque) {
        alloc::free_opaque(ptr)
    }
    #[no_mangle]
    pub extern "C" fn rustty_wasm_take_opaque(ptr: *mut alloc::Opaque) -> alloc::Opaque {
        alloc::take_opaque(ptr)
    }
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
#[cfg(feature = "kitty-graphics")]
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_get(
    graphics: kitty_graphics::KittyGraphics,
    // D11: inbound as the C integer, narrowed below. See `Data::from_c_int`.
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    let data = match kitty_graphics::Data::from_c_int(data) {
        Some(d) => d,
        None => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: `graphics`/`out` validity is the caller's contract.
    unsafe { kitty_graphics::get(graphics, data, out) }.to_c_int()
}

// pub const kitty_graphics_image = kitty_graphics.image_get_handle;
/// C: rustty_kitty_graphics_image
///
/// # Safety
/// `graphics` must be a valid `KittyGraphics` handle.
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_image_get(
    image: kitty_graphics::ImageHandle,
    // D11: inbound as the C integer, narrowed below. See `ImageData::from_c_int`.
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    let data = match kitty_graphics::ImageData::from_c_int(data) {
        Some(d) => d,
        None => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: `image`/`out` validity is the caller's contract.
    unsafe { kitty_graphics::image_get(image, data, out) }.to_c_int()
}

// pub const kitty_graphics_image_get_multi = kitty_graphics.image_get_multi;
/// C: rustty_kitty_graphics_image_get_multi
///
/// # Safety
/// See `kitty_graphics::image_get_multi`.
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
#[no_mangle]
pub unsafe extern "C" fn rustty_kitty_graphics_placement_get(
    iter: kitty_graphics::PlacementIterator,
    // D11: inbound as the C integer, narrowed below. See `PlacementData::from_c_int`.
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    let data = match kitty_graphics::PlacementData::from_c_int(data) {
        Some(d) => d,
        None => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: `iter`/`out` validity is the caller's contract.
    unsafe { kitty_graphics::placement_get(iter, data, out) }.to_c_int()
}

// pub const kitty_graphics_placement_get_multi = kitty_graphics.placement_get_multi;
/// C: rustty_kitty_graphics_placement_get_multi
///
/// # Safety
/// See `kitty_graphics::placement_get_multi`.
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "kitty-graphics")]
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
#[cfg(feature = "render-state")]
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
#[cfg(feature = "render-state")]
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
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_update(
    state: render::RenderState,
    terminal: terminal::Terminal,
) -> c_int {
    // SAFETY: `state`/`terminal` validity is the caller's contract.
    unsafe { render::update(state, terminal) }.to_c_int()
}

// pub const render_state_begin_update = render.begin_update;
/// C: rustty_render_state_begin_update
///
/// # Safety
/// `state`/`terminal`, if non-null, must be live handles.
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_begin_update(
    state: render::RenderState,
    terminal: terminal::Terminal,
) -> c_int {
    // SAFETY: `state`/`terminal` validity is the caller's contract.
    unsafe { render::begin_update(state, terminal) }.to_c_int()
}

// pub const render_state_end_update = render.end_update;
/// C: rustty_render_state_end_update
///
/// # Safety
/// `state`, if non-null, must be a live handle.
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_end_update(state: render::RenderState) -> c_int {
    // SAFETY: `state` validity is the caller's contract.
    unsafe { render::end_update(state) }.to_c_int()
}

// pub const render_state_clean = render.clean;
/// C: rustty_render_state_clean
///
/// # Safety
/// `state`, if non-null, must be a live handle.
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_clean(state: render::RenderState) -> c_int {
    // SAFETY: `state` validity is the caller's contract.
    unsafe { render::clean(state) }.to_c_int()
}

// pub const render_state_get = render.get;
/// C: rustty_render_state_get
///
/// # Safety
/// `out` must point to writable storage of the type required by `data`.
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_get(
    state: render::RenderState,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(Data, ...) catch { log.warn(...); return .invalid_value; }
    // PORT NOTE (W34-B02): the guard + `log.warn` live in `render::get_checked`
    // so that `get_multi` reaches them per array element, exactly as the
    // reference's `get_multi` reaches the guard by calling `get`.
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { render::get_checked(state, data, out) }.to_c_int()
}

// pub const render_state_get_multi = render.get_multi;
/// C: rustty_render_state_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_get_multi(
    state: render::RenderState,
    count: usize,
    keys: *const render::Data,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // PORT NOTE (W34-B02): the parameter type stays `*const render::Data` so the
    // published C signature keeps `const RusttyRenderStateData *keys`, but the
    // ARRAY ELEMENTS are C-supplied integers: they are read as raw `c_int`
    // (ABI-identical — `Data` is `#[repr(C)]`, asserted int-sized) and
    // range-checked per element, because materializing an out-of-range
    // discriminant as a Rust enum is UB in every build mode.
    // SAFETY: pointer validity is the caller's contract.
    unsafe { render::get_multi(state, count, keys as *const c_int, values, out_written) }.to_c_int()
}

// pub const render_state_set = render.set;
/// C: rustty_render_state_set
///
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
#[cfg(feature = "render-state")]
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
            sys::log_fn(
                sys::StdLevel::Warn,
                render::LOG_SCOPE,
                format_args!("render_state_set invalid option value={option}"),
            );
            return result::Result::InvalidValue.to_c_int();
        }
    };
    // SAFETY: `value` validity for `opt` is the caller's contract.
    unsafe { render::set(state, opt, value) }.to_c_int()
}

// pub const render_state_row_iterator_new = render.row_iterator_new;
/// C: rustty_render_state_row_iterator_new
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a row-iterator handle.
#[cfg(feature = "render-state")]
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
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_iterator_next(
    iterator: render::RowIterator,
) -> bool {
    // SAFETY: `iterator` validity is the caller's contract.
    unsafe { render::row_iterator_next(iterator) }
}

// pub const render_state_row_iterator_next_dirty = render.row_iterator_next_dirty;
/// C: rustty_render_state_row_iterator_next_dirty
///
/// # Safety
/// `iterator`, if non-null, must be a live iterator whose borrowed render state
/// has not been updated since it was populated; `out_y`, if non-null, must be a
/// valid writable pointer.
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_iterator_next_dirty(
    iterator: render::RowIterator,
    out_y: *mut u16,
) -> bool {
    // SAFETY: `iterator`/`out_y` validity is the caller's contract.
    unsafe { render::row_iterator_next_dirty(iterator, out_y) }
}

// pub const render_state_row_iterator_free = render.row_iterator_free;
/// C: rustty_render_state_row_iterator_free
///
/// # Safety
/// `iterator`, if non-null, must be a handle returned by the matching `_new`.
#[cfg(feature = "render-state")]
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
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_get(
    iterator: render::RowIterator,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(RowData, ...) catch { log.warn(...); return .invalid_value; }
    // PORT NOTE (W34-B02): guard + `log.warn` live in `render::row_get_checked`
    // so `row_get_multi` reaches them per array element.
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { render::row_get_checked(iterator, data, out) }.to_c_int()
}

// pub const render_state_row_get_multi = render.row_get_multi;
/// C: rustty_render_state_row_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_get_multi(
    iterator: render::RowIterator,
    count: usize,
    keys: *const render::RowData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // PORT NOTE (W34-B02): see `rustty_render_state_get_multi` — the elements
    // are read as raw `c_int` and range-checked per element.
    // SAFETY: pointer validity is the caller's contract.
    unsafe { render::row_get_multi(iterator, count, keys as *const c_int, values, out_written) }
        .to_c_int()
}

// pub const render_state_row_set = render.row_set;
/// C: rustty_render_state_row_set
///
/// # Safety
/// `value`, if non-null, must point to a value of the type required by `option`.
#[cfg(feature = "render-state")]
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
            sys::log_fn(
                sys::StdLevel::Warn,
                render::LOG_SCOPE,
                format_args!("render_state_row_set invalid option value={option}"),
            );
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
#[cfg(feature = "render-state")]
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
#[cfg(feature = "render-state")]
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
#[cfg(feature = "render-state")]
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
#[cfg(feature = "render-state")]
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
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_cells_get(
    cells: render::RowCells,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(RowCellsData, ...) catch { log.warn(...); return .invalid_value; }
    // PORT NOTE (W34-B02): guard + `log.warn` live in
    // `render::row_cells_get_checked` so `row_cells_get_multi` reaches them per
    // array element.
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { render::row_cells_get_checked(cells, data, out) }.to_c_int()
}

// pub const render_state_row_cells_get_multi = render.row_cells_get_multi;
/// C: rustty_render_state_row_cells_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[cfg(feature = "render-state")]
#[no_mangle]
pub unsafe extern "C" fn rustty_render_state_row_cells_get_multi(
    cells: render::RowCells,
    count: usize,
    keys: *const render::RowCellsData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // PORT NOTE (W34-B02): see `rustty_render_state_get_multi` — the elements
    // are read as raw `c_int` and range-checked per element.
    // SAFETY: pointer validity is the caller's contract.
    unsafe { render::row_cells_get_multi(cells, count, keys as *const c_int, values, out_written) }
        .to_c_int()
}

// PORT NOTE (W34-B02): the range-checked `c_int -> enum` projections for the
// render getters (the `std.meta.intToEnum` guards) MOVED to `render.rs` as
// `Data::from_c_int` / `RowData::from_c_int` / `RowCellsData::from_c_int`, so
// the guard (and its `log.warn`) is shared by the single-get wrappers above and
// by the `*_get_multi` element loops — matching the reference, where `get_multi`
// reaches the guard by calling the guarded `get`.

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
#[cfg(feature = "selection")]
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
#[cfg(feature = "selection")]
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
#[cfg(feature = "selection")]
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
#[cfg(feature = "selection")]
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
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_get(
    gesture: selection_gesture::Gesture,
    terminal: terminal::Terminal,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(Data, ...) catch { log.warn(...); return .invalid_value; }
    // PORT NOTE (W66-GETMULTI-RAW-01): the guard + `log.warn` live in
    // `selection_gesture::get_checked` so that `get_multi` reaches them per
    // array element, exactly as the reference's `get_multi` reaches the guard
    // by calling `get`.
    // SAFETY: `out` validity/type is the caller's contract.
    unsafe { selection_gesture::get_checked(gesture, terminal, data, out) }.to_c_int()
}

// pub const selection_gesture_get_multi = selection_gesture.get_multi;
/// C: rustty_selection_gesture_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_get_multi(
    gesture: selection_gesture::Gesture,
    terminal: terminal::Terminal,
    count: usize,
    keys: *const selection_gesture::Data,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // PORT NOTE (W66-GETMULTI-RAW-01): the parameter type stays
    // `*const selection_gesture::Data` so the published C signature keeps
    // `const RusttySelectionGestureData *keys`, but the ARRAY ELEMENTS are
    // C-supplied integers: they are read as raw `c_int` (ABI-identical —
    // `Data` is `#[repr(C)]`, asserted int-sized) and range-checked per
    // element, because materializing an out-of-range discriminant as a Rust
    // enum is UB in every build mode.
    // SAFETY: pointer validity is the caller's contract.
    unsafe {
        selection_gesture::get_multi(
            gesture,
            terminal,
            count,
            keys as *const c_int,
            values,
            out_written,
        )
    }
    .to_c_int()
}

// pub const selection_gesture_event_new = selection_gesture.event_new;
/// C: rustty_selection_gesture_event_new
///
/// # Safety
/// `alloc_`, if non-null, must point to a valid allocator; `out_event`, if
/// non-null, must be a valid, writable pointer to an event handle.
#[cfg(feature = "selection")]
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
#[cfg(feature = "selection")]
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
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_selection_gesture_event_set(
    event: selection_gesture::Event,
    option: c_int,
    value: *const std::ffi::c_void,
) -> c_int {
    // if (comptime std.debug.runtime_safety) { _ = intToEnum(EventOption, ...) catch { log.warn(...); return .invalid_value; } }
    let Some(opt) = selection_gesture_event_option_from_c_int(option) else {
        // log.warn("selection_gesture_event_set invalid option value={d}", .{...});
        sys::log_fn(
            sys::StdLevel::Warn,
            selection_gesture::LOG_SCOPE,
            format_args!("selection_gesture_event_set invalid option value={option}"),
        );
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `value` validity for `opt` is the caller's contract.
    unsafe { selection_gesture::event_set(event, opt, value) }.to_c_int()
}

// PORT NOTE: range-checked `c_int -> enum` projections for the selection_gesture
// guards. `EventType`/`EventOption` are contiguous `#[repr(C)]` enums. (`Data`'s
// projection lives in the module as `selection_gesture::Data::from_c_int`, since
// `get_multi` must reach it per array element — W66-GETMULTI-RAW-01.)
#[cfg(feature = "selection")]
const SELECTION_GESTURE_EVENT_TYPE_COUNT: c_int = 5; // contiguous 0..=4

#[cfg(feature = "selection")]
fn selection_gesture_event_type_from_c_int(v: c_int) -> Option<selection_gesture::EventType> {
    if (0..SELECTION_GESTURE_EVENT_TYPE_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `EventType` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, selection_gesture::EventType>(v) })
    } else {
        None
    }
}

#[cfg(feature = "selection")]
const SELECTION_GESTURE_EVENT_OPTION_COUNT: c_int = 10; // contiguous 0..=9

#[cfg(feature = "selection")]
fn selection_gesture_event_option_from_c_int(v: c_int) -> Option<selection_gesture::EventOption> {
    if (0..SELECTION_GESTURE_EVENT_OPTION_COUNT).contains(&v) {
        // SAFETY: `v` is a valid `EventOption` discriminant (range-checked above).
        Some(unsafe { std::mem::transmute::<c_int, selection_gesture::EventOption>(v) })
    } else {
        None
    }
}

// ----- terminal -----
// PORT NOTE: 1:1 with the `@export(&c.terminal_*, ...)` lines in `lib_vt.zig`.
// The `c.zig` aliases map `terminal_<name> -> terminal.<name>` (e.g.
// `terminal_new -> terminal.new`). The `set` `intToEnum` runtime-safety guard
// lives inside `terminal::set` on the raw `c_int` option; the `mode_*`/`get`
// guards run inside the ported module fns (the C-ABI enum args are crossed
// directly). `ScrollViewport`/`TerminalData` are `#[repr(C)]` (oracle
// 03d5fa268 removed the `Options` struct from the ABI entirely).

// pub const terminal_new = terminal.new;  (ghostty_terminal_new)
/// C: rustty_terminal_new
///
/// # Safety
/// `alloc`, if non-null, must point to a valid allocator; `result` must be a
/// valid, writable pointer to a terminal handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_new(
    alloc: *const rustty_lib::allocator::Allocator,
    result: *mut terminal::Terminal,
    cols: u16,
    rows: u16,
) -> c_int {
    // SAFETY: `alloc`/`result` validity is the caller's contract.
    unsafe { terminal::new(alloc, result, cols, rows) }.to_c_int()
}

// pub const terminal_free = terminal.free;  (ghostty_terminal_free)
/// C: rustty_terminal_free
///
/// # Safety
/// `terminal`, if non-null, must be a handle returned by `rustty_terminal_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_free(terminal: terminal::Terminal) {
    // SAFETY: `terminal` validity is the caller's contract.
    unsafe { terminal::free(terminal) }
}

// pub const terminal_reset = terminal.reset;  (ghostty_terminal_reset)
/// C: rustty_terminal_reset
///
/// # Safety
/// `terminal`, if non-null, must be a handle returned by `rustty_terminal_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_reset(terminal: terminal::Terminal) {
    // SAFETY: `terminal` validity is the caller's contract.
    unsafe { terminal::reset(terminal) }
}

// pub const terminal_resize = terminal.resize;  (ghostty_terminal_resize)
/// C: rustty_terminal_resize
///
/// # Safety
/// `terminal`, if non-null, must be a handle returned by `rustty_terminal_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_resize(
    terminal: terminal::Terminal,
    cols: rustty_vt::size::CellCountInt,
    rows: rustty_vt::size::CellCountInt,
    cell_width_px: u32,
    cell_height_px: u32,
) -> c_int {
    // SAFETY: `terminal` validity is the caller's contract.
    unsafe { terminal::resize(terminal, cols, rows, cell_width_px, cell_height_px) }.to_c_int()
}

// pub const terminal_set = terminal.set;  (ghostty_terminal_set)
/// C: rustty_terminal_set
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `value`, if
/// non-null, must point to a value of the type required by `option`.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_set(
    terminal: terminal::Terminal,
    option: c_int,
    value: *const std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(Option, ...) catch { log.warn(...); return .invalid_value; }
    let Some(opt) = terminal_option_from_c_int(option) else {
        // log.warn("terminal_set invalid option value={d}", .{...});
        // PORT NOTE (W35-B06): the reference's `log.warn` on the `.terminal_c`
        // scope reaches `terminal/c/sys.zig`'s `logFn`, which routes to the
        // installed C log callback and DROPS the message when none is installed
        // — it never writes to stderr. `sys::log_fn` is that sink.
        sys::log_fn(
            sys::StdLevel::Warn,
            terminal::LOG_SCOPE,
            format_args!("terminal_set invalid option value={option}"),
        );
        return result::Result::InvalidValue.to_c_int();
    };
    // SAFETY: `terminal`/`value` validity is the caller's contract.
    unsafe { terminal::set(terminal, opt, value) }.to_c_int()
}

// pub const terminal_vt_write = terminal.vt_write;  (ghostty_terminal_vt_write)
/// C: rustty_terminal_vt_write
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `ptr`, if
/// non-null, must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_vt_write(
    terminal: terminal::Terminal,
    ptr: *const u8,
    len: usize,
) {
    // SAFETY: `terminal`/`ptr` validity is the caller's contract.
    unsafe { terminal::vt_write(terminal, ptr, len) }
}

// pub const terminal_vt_write_until_ground = terminal.vt_write_until_ground;  (ghostty_terminal_vt_write_until_ground)
/// C: rustty_terminal_vt_write_until_ground
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `data`, if
/// non-null, must point to `len` readable bytes; `out_consumed`, if non-null,
/// must be writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_vt_write_until_ground(
    terminal: terminal::Terminal,
    data: *const u8,
    len: usize,
    out_consumed: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { terminal::vt_write_until_ground(terminal, data, len, out_consumed) }.to_c_int()
}

// pub const terminal_continuation_write = terminal.continuation_write;  (ghostty_terminal_continuation_write)
/// C: rustty_terminal_continuation_write
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `writer`'s
/// callback must not call terminal APIs with the same terminal handle.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_continuation_write(
    terminal: terminal::Terminal,
    writer: io::Writer,
) -> c_int {
    // SAFETY: pointer/callback validity is the caller's contract.
    unsafe { terminal::continuation_write(terminal, writer) }.to_c_int()
}

// pub const terminal_continuation_buf = terminal.continuation_buf;  (ghostty_terminal_continuation_buf)
/// C: rustty_terminal_continuation_buf
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `buf`, if
/// non-null, must point to `buf_len` writable bytes; `out_written`, if
/// non-null, must be writable.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_continuation_buf(
    terminal: terminal::Terminal,
    buf: *mut u8,
    buf_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { terminal::continuation_buf(terminal, buf, buf_len, out_written) }.to_c_int()
}

// pub const terminal_continuation_alloc = terminal.continuation_alloc;  (ghostty_terminal_continuation_alloc)
/// C: rustty_terminal_continuation_alloc
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `alloc`,
/// if non-null, a valid allocator; `out_ptr`/`out_len`, if non-null, must be
/// writable. The returned bytes are caller-owned and released with
/// `rustty_free` + the same allocator.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_continuation_alloc(
    terminal: terminal::Terminal,
    alloc: *const rustty_lib::allocator::Allocator,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { terminal::continuation_alloc(terminal, alloc.as_ref(), out_ptr, out_len) }.to_c_int()
}

// pub const terminal_scroll_viewport = terminal.scroll_viewport;  (ghostty_terminal_scroll_viewport)
/// C: rustty_terminal_scroll_viewport
///
/// # Safety
/// `terminal`, if non-null, must be a handle returned by `rustty_terminal_new`.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_scroll_viewport(
    terminal: terminal::Terminal,
    behavior: terminal::ScrollViewport,
) {
    // SAFETY: `terminal` validity is the caller's contract.
    unsafe { terminal::scroll_viewport(terminal, behavior) }
}

// pub const terminal_compression_activity = terminal.compression_activity;  (ghostty_terminal_compression_activity)
/// C: rustty_terminal_compression_activity
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle;
/// `out_activity`, if non-null, must be a valid, writable pointer to a `u64`.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_compression_activity(
    terminal: terminal::Terminal,
    out_activity: *mut u64,
) -> c_int {
    // SAFETY: `terminal`/`out_activity` validity is the caller's contract.
    unsafe { terminal::compression_activity(terminal, out_activity) }.to_c_int()
}

// pub const terminal_compress = terminal.compress;  (ghostty_terminal_compress)
/// C: rustty_terminal_compress
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `out_result`,
/// if non-null, must be a valid, writable pointer to a
/// `RusttyTerminalCompressionResult`.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_compress(
    terminal: terminal::Terminal,
    mode: c_int,
    out_result: *mut terminal::CompressionResult,
) -> c_int {
    // SAFETY: `terminal`/`out_result` validity is the caller's contract. The
    // `mode` `intToEnum` guard lives inside `terminal::compress` (an explicit
    // `match` on the raw `c_int`; never a transmute).
    unsafe { terminal::compress(terminal, mode, out_result) }.to_c_int()
}

// (cfc19e805: `terminal_mode_get`/`terminal_mode_set` were REMOVED from the
// export list — modes are configured through `RUSTTY_TERMINAL_OPT_MODE` /
// `RUSTTY_TERMINAL_OPT_MODE_DEFAULT` and queried via
// `RUSTTY_TERMINAL_DATA_MODE`.)

// pub const terminal_get = terminal.get;  (ghostty_terminal_get)
/// C: rustty_terminal_get
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `out` must
/// point to writable storage of the type required by `data`.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_get(
    terminal: terminal::Terminal,
    data: c_int,
    out: *mut std::ffi::c_void,
) -> c_int {
    // _ = std.meta.intToEnum(TerminalData, ...) catch { log.warn(...); return .invalid_value; }
    // PORT NOTE (W35-B08): the guard + `log.warn` live in `terminal::get_checked`
    // so that `get_multi` reaches them per array element, exactly as the
    // reference's `get_multi` reaches the guard by calling `get`.
    // SAFETY: `terminal`/`out` validity is the caller's contract.
    unsafe { terminal::get_checked(terminal, data, out) }.to_c_int()
}

// pub const terminal_get_multi = terminal.get_multi;  (ghostty_terminal_get_multi)
/// C: rustty_terminal_get_multi
///
/// # Safety
/// `keys`/`values`/`out_written` validity is the caller's contract.
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_get_multi(
    terminal: terminal::Terminal,
    count: usize,
    keys: *const terminal::TerminalData,
    values: *mut *mut std::ffi::c_void,
    out_written: *mut usize,
) -> c_int {
    // PORT NOTE (W35-B08): the parameter type stays `*const terminal::TerminalData`
    // so the published C signature keeps `const RusttyTerminalData *keys`, but the
    // ARRAY ELEMENTS are C-supplied integers: they are read as raw `c_int`
    // (ABI-identical — `TerminalData` is `#[repr(C)]`, asserted int-sized) and
    // range-checked per element, because materializing an out-of-range
    // discriminant as a Rust enum is UB in every build mode.
    // SAFETY: pointer validity is the caller's contract.
    unsafe { terminal::get_multi(terminal, count, keys as *const c_int, values, out_written) }
        .to_c_int()
}

// pub const terminal_select_word = selection.word;  (ghostty_terminal_select_word)
/// C: rustty_terminal_select_word
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `options`, if
/// non-null, must point to a valid `SelectWordOptions`; `out_selection`, if
/// non-null, must be a valid, writable pointer to a `CSelection`.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_select_word(
    terminal: terminal::Terminal,
    options: *const selection::SelectWordOptions,
    out_selection: *mut selection::CSelection,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::word(terminal, options, out_selection) }.to_c_int()
}

// pub const terminal_select_word_between = selection.word_between;
//   (ghostty_terminal_select_word_between)
/// C: rustty_terminal_select_word_between
///
/// # Safety
/// `terminal`/`options`/`out_selection` validity is the caller's contract.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_select_word_between(
    terminal: terminal::Terminal,
    options: *const selection::SelectWordBetweenOptions,
    out_selection: *mut selection::CSelection,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::word_between(terminal, options, out_selection) }.to_c_int()
}

// pub const terminal_select_line = selection.line;  (ghostty_terminal_select_line)
/// C: rustty_terminal_select_line
///
/// # Safety
/// `terminal`/`options`/`out_selection` validity is the caller's contract.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_select_line(
    terminal: terminal::Terminal,
    options: *const selection::SelectLineOptions,
    out_selection: *mut selection::CSelection,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::line(terminal, options, out_selection) }.to_c_int()
}

// pub const terminal_select_all = selection.all;  (ghostty_terminal_select_all)
/// C: rustty_terminal_select_all
///
/// # Safety
/// `terminal`/`out_selection` validity is the caller's contract.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_select_all(
    terminal: terminal::Terminal,
    out_selection: *mut selection::CSelection,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::all(terminal, out_selection) }.to_c_int()
}

// pub const terminal_select_output = selection.output;  (ghostty_terminal_select_output)
/// C: rustty_terminal_select_output
///
/// # Safety
/// `terminal`/`out_selection` validity is the caller's contract; `ref_` must be a
/// valid `CGridRef`.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_select_output(
    terminal: terminal::Terminal,
    ref_: grid_ref::CGridRef,
    out_selection: *mut selection::CSelection,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::output(terminal, ref_, out_selection) }.to_c_int()
}

// pub const terminal_selection_format_buf = selection.format_buf;
//   (ghostty_terminal_selection_format_buf)
/// C: rustty_terminal_selection_format_buf
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `out` (if
/// non-null) must point to `out_len` writable bytes; `out_written` must be a
/// valid, writable pointer.
#[cfg(all(feature = "formatter", feature = "selection"))]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_selection_format_buf(
    terminal: terminal::Terminal,
    opts: selection::FormatOptions,
    out: *mut u8,
    out_len: usize,
    out_written: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::format_buf(terminal, opts, out, out_len, out_written) }.to_c_int()
}

// pub const terminal_selection_format_alloc = selection.format_alloc;
//   (ghostty_terminal_selection_format_alloc)
/// C: rustty_terminal_selection_format_alloc
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `alloc`, if
/// non-null, a valid allocator; `out_ptr`/`out_len` must be valid, writable
/// pointers. The returned buffer is owned by the caller and must be freed with the
/// same allocator.
#[cfg(all(feature = "formatter", feature = "selection"))]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_selection_format_alloc(
    terminal: terminal::Terminal,
    alloc: *const rustty_lib::allocator::Allocator,
    opts: selection::FormatOptions,
    out_ptr: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::format_alloc(terminal, alloc, opts, out_ptr, out_len) }.to_c_int()
}

// pub const terminal_selection_adjust = selection.adjust;  (ghostty_terminal_selection_adjust)
/// C: rustty_terminal_selection_adjust
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `selection`,
/// if non-null, must be a valid, writable pointer to a `CSelection`.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_selection_adjust(
    terminal: terminal::Terminal,
    selection: *mut selection::CSelection,
    // D11: inbound as the C integer, narrowed below. See `Adjustment::from_c_int`.
    adjustment: c_int,
) -> c_int {
    let adjustment = match selection::Adjustment::from_c_int(adjustment) {
        Some(a) => a,
        // log.warn("terminal_selection_adjust invalid adjustment value={d}", .{...});
        // PORT NOTE (W35-B03): the reference's `.selection_c`-scoped `log.warn`
        // is routed by `terminal/c/sys.zig`'s `logFn` to the installed C log
        // callback (and dropped when none is installed); `sys::log_fn` is that
        // sink. The disposition (`.invalid_value`) is unchanged.
        None => {
            sys::log_fn(
                sys::StdLevel::Warn,
                selection::LOG_SCOPE,
                format_args!("terminal_selection_adjust invalid adjustment value={adjustment}"),
            );
            return result::Result::InvalidValue.to_c_int();
        }
    };
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::adjust(terminal, selection, adjustment) }.to_c_int()
}

// pub const terminal_selection_order = selection.order;  (ghostty_terminal_selection_order)
/// C: rustty_terminal_selection_order
///
/// # Safety
/// `terminal`/`selection`, if non-null, must be live; `out_order`, if non-null,
/// must be a valid, writable pointer to an `Order`.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_selection_order(
    terminal: terminal::Terminal,
    selection: *const selection::CSelection,
    out_order: *mut selection::Order,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::order(terminal, selection, out_order) }.to_c_int()
}

// pub const terminal_selection_ordered = selection.ordered;  (ghostty_terminal_selection_ordered)
/// C: rustty_terminal_selection_ordered
///
/// # Safety
/// `terminal`/`selection`, if non-null, must be live; `out_selection`, if
/// non-null, must be a valid, writable pointer to a `CSelection`.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_selection_ordered(
    terminal: terminal::Terminal,
    selection: *const selection::CSelection,
    // D11: inbound as the C integer, narrowed below. See `Order::from_c_int`.
    desired: c_int,
    out_selection: *mut selection::CSelection,
) -> c_int {
    let desired = match selection::Order::from_c_int(desired) {
        Some(o) => o,
        // log.warn("terminal_selection_ordered invalid desired value={d}", .{...});
        // PORT NOTE (W35-B03): see `rustty_terminal_selection_adjust` — the
        // reference's `.selection_c`-scoped `log.warn` goes to the C log
        // callback via `sys::log_fn`, never to stderr.
        None => {
            sys::log_fn(
                sys::StdLevel::Warn,
                selection::LOG_SCOPE,
                format_args!("terminal_selection_ordered invalid desired value={desired}"),
            );
            return result::Result::InvalidValue.to_c_int();
        }
    };
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::ordered(terminal, selection, desired, out_selection) }.to_c_int()
}

// pub const terminal_selection_contains = selection.contains;  (ghostty_terminal_selection_contains)
/// C: rustty_terminal_selection_contains
///
/// # Safety
/// `terminal`/`selection`, if non-null, must be live; `out_contains`, if
/// non-null, must be a valid, writable pointer to a `bool`.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_selection_contains(
    terminal: terminal::Terminal,
    selection: *const selection::CSelection,
    pt: point::PointC,
    out_contains: *mut bool,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::contains(terminal, selection, pt, out_contains) }.to_c_int()
}

// pub const terminal_selection_equal = selection.equal;  (ghostty_terminal_selection_equal)
/// C: rustty_terminal_selection_equal
///
/// # Safety
/// `terminal`/`a`/`b`, if non-null, must be live; `out_equal`, if non-null, must
/// be a valid, writable pointer to a `bool`.
#[cfg(feature = "selection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_selection_equal(
    terminal: terminal::Terminal,
    a: *const selection::CSelection,
    b: *const selection::CSelection,
    out_equal: *mut bool,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { selection::equal(terminal, a, b, out_equal) }.to_c_int()
}

// pub const terminal_grid_ref = terminal.grid_ref;  (ghostty_terminal_grid_ref)
/// C: rustty_terminal_grid_ref
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `out_ref`, if
/// non-null, must be a valid, writable pointer to a `CGridRef`.
#[cfg(any(feature = "grid-introspection", feature = "selection"))]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_grid_ref(
    terminal: terminal::Terminal,
    pt: point::PointC,
    out_ref: *mut grid_ref::CGridRef,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { terminal::grid_ref(terminal, pt, out_ref) }.to_c_int()
}

// pub const terminal_grid_ref_track = terminal.grid_ref_track;  (ghostty_terminal_grid_ref_track)
/// C: rustty_terminal_grid_ref_track
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `out_ref`, if
/// non-null, must be a valid, writable pointer to a `CTrackedGridRef`.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_grid_ref_track(
    terminal: terminal::Terminal,
    pt: point::PointC,
    out_ref: *mut grid_ref_tracked::CTrackedGridRef,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { terminal::grid_ref_track(terminal, pt, out_ref) }.to_c_int()
}

// pub const terminal_point_from_grid_ref = terminal.point_from_grid_ref;
//   (ghostty_terminal_point_from_grid_ref)
/// C: rustty_terminal_point_from_grid_ref
///
/// # Safety
/// `terminal`, if non-null, must be a `rustty_terminal_new` handle; `ref_` must
/// be a valid `CGridRef`; `out`, if non-null, must be a valid, writable pointer
/// to a `Coordinate`.
#[cfg(any(feature = "grid-introspection", feature = "selection"))]
#[no_mangle]
pub unsafe extern "C" fn rustty_terminal_point_from_grid_ref(
    terminal: terminal::Terminal,
    ref_: *const grid_ref::CGridRef,
    // D11: inbound as the C integer, narrowed below. See `TagC::from_c_int`.
    tag: c_int,
    out: *mut rustty_vt::point::Coordinate,
) -> c_int {
    // PORT NOTE: `tag: point.Tag` crosses the C ABI as its `enum(c_int)`
    // projection (`point::TagC`); bridge to the pure-VT `point::Tag`.
    let tag = match point::TagC::from_c_int(tag) {
        Some(t) => t,
        None => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: pointer validity is the caller's contract.
    unsafe { terminal::point_from_grid_ref(terminal, ref_, tag.to_tag(), out) }.to_c_int()
}

// ----- grid_ref -----
// PORT NOTE: 1:1 with the 5 `@export(&c.grid_ref_*, ...)` lines; the `c.zig`
// aliases map `grid_ref_<name> -> grid_ref.grid_ref_<name>`.

// pub const grid_ref_cell = grid_ref.grid_ref_cell;  (ghostty_grid_ref_cell)
/// C: rustty_grid_ref_cell
///
/// # Safety
/// `ref_` must be a valid pointer to a `CGridRef`; `out`, if non-null, must be a
/// valid, writable pointer to a `CCell`.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_grid_ref_cell(
    ref_: *const grid_ref::CGridRef,
    out: *mut cell::CCell,
) -> c_int {
    // SAFETY: `ref_`/`out` validity is the caller's contract.
    unsafe { grid_ref::grid_ref_cell(ref_, out) }.to_c_int()
}

// pub const grid_ref_row = grid_ref.grid_ref_row;  (ghostty_grid_ref_row)
/// C: rustty_grid_ref_row
///
/// # Safety
/// `ref_` must be a valid pointer to a `CGridRef`; `out`, if non-null, must be a
/// valid, writable pointer to a `CRow`.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_grid_ref_row(
    ref_: *const grid_ref::CGridRef,
    out: *mut row::CRow,
) -> c_int {
    // SAFETY: `ref_`/`out` validity is the caller's contract.
    unsafe { grid_ref::grid_ref_row(ref_, out) }.to_c_int()
}

// pub const grid_ref_graphemes = grid_ref.grid_ref_graphemes;  (ghostty_grid_ref_graphemes)
/// C: rustty_grid_ref_graphemes
///
/// # Safety
/// `ref_` must be a valid `CGridRef`; `out_buf`, if non-null, must point to
/// `buf_len` writable `u32`; `out_len` must be a valid, writable pointer.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_grid_ref_graphemes(
    ref_: *const grid_ref::CGridRef,
    out_buf: *mut u32,
    buf_len: usize,
    out_len: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { grid_ref::grid_ref_graphemes(ref_, out_buf, buf_len, out_len) }.to_c_int()
}

// pub const grid_ref_hyperlink_uri = grid_ref.grid_ref_hyperlink_uri;
//   (ghostty_grid_ref_hyperlink_uri)
/// C: rustty_grid_ref_hyperlink_uri
///
/// # Safety
/// `ref_` must be a valid `CGridRef`; `out_buf`, if non-null, must point to
/// `buf_len` writable bytes; `out_len` must be a valid, writable pointer.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_grid_ref_hyperlink_uri(
    ref_: *const grid_ref::CGridRef,
    out_buf: *mut u8,
    buf_len: usize,
    out_len: *mut usize,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { grid_ref::grid_ref_hyperlink_uri(ref_, out_buf, buf_len, out_len) }.to_c_int()
}

// pub const grid_ref_style = grid_ref.grid_ref_style;  (ghostty_grid_ref_style)
/// C: rustty_grid_ref_style
///
/// # Safety
/// `ref_` must be a valid pointer to a `CGridRef`; `out`, if non-null, must be a
/// valid, writable pointer to a `Style`.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_grid_ref_style(
    ref_: *const grid_ref::CGridRef,
    out: *mut style::Style,
) -> c_int {
    // SAFETY: `ref_`/`out` validity is the caller's contract.
    unsafe { grid_ref::grid_ref_style(ref_, out) }.to_c_int()
}

// ----- grid_ref_tracked -----
// PORT NOTE: 1:1 with the 5 `@export(&c.tracked_grid_ref_*, ...)` lines; the
// `c.zig` aliases map `tracked_grid_ref_<name> ->
// grid_ref_tracked.tracked_grid_ref_<name>`.

// pub const tracked_grid_ref_free = grid_ref_tracked.tracked_grid_ref_free;
//   (ghostty_tracked_grid_ref_free)
/// C: rustty_tracked_grid_ref_free
///
/// # Safety
/// `ref_`, if non-null, must be a handle returned by `rustty_terminal_grid_ref_track`.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_tracked_grid_ref_free(ref_: grid_ref_tracked::CTrackedGridRef) {
    // SAFETY: `ref_` validity is the caller's contract.
    unsafe { grid_ref_tracked::tracked_grid_ref_free(ref_) }
}

// pub const tracked_grid_ref_has_value = grid_ref_tracked.tracked_grid_ref_has_value;
//   (ghostty_tracked_grid_ref_has_value)
/// C: rustty_tracked_grid_ref_has_value
///
/// # Safety
/// `ref_`, if non-null, must be a live tracked-grid-ref handle.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_tracked_grid_ref_has_value(
    ref_: grid_ref_tracked::CTrackedGridRef,
) -> bool {
    // SAFETY: `ref_` validity is the caller's contract.
    unsafe { grid_ref_tracked::tracked_grid_ref_has_value(ref_) }
}

// pub const tracked_grid_ref_point = grid_ref_tracked.tracked_grid_ref_point;
//   (ghostty_tracked_grid_ref_point)
/// C: rustty_tracked_grid_ref_point
///
/// # Safety
/// `ref_`, if non-null, must be a live handle; `out`, if non-null, must be a
/// valid, writable pointer to a `Coordinate`.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_tracked_grid_ref_point(
    ref_: grid_ref_tracked::CTrackedGridRef,
    // D11: inbound as the C integer, narrowed below. See `TagC::from_c_int`.
    tag: c_int,
    out: *mut rustty_vt::point::Coordinate,
) -> c_int {
    // PORT NOTE: `tag: point.Tag` crosses as `point::TagC`; bridge to `point::Tag`.
    let tag = match point::TagC::from_c_int(tag) {
        Some(t) => t,
        None => return result::Result::InvalidValue.to_c_int(),
    };
    // SAFETY: pointer validity is the caller's contract.
    unsafe { grid_ref_tracked::tracked_grid_ref_point(ref_, tag.to_tag(), out) }.to_c_int()
}

// pub const tracked_grid_ref_set = grid_ref_tracked.tracked_grid_ref_set;
//   (ghostty_tracked_grid_ref_set)
/// C: rustty_tracked_grid_ref_set
///
/// # Safety
/// `ref_`/`terminal`, if non-null, must be live handles.
#[cfg(feature = "grid-introspection")]
#[no_mangle]
pub unsafe extern "C" fn rustty_tracked_grid_ref_set(
    ref_: grid_ref_tracked::CTrackedGridRef,
    terminal: terminal::Terminal,
    pt: point::PointC,
) -> c_int {
    // SAFETY: handle validity is the caller's contract.
    unsafe { grid_ref_tracked::tracked_grid_ref_set(ref_, terminal, pt) }.to_c_int()
}

// pub const tracked_grid_ref_snapshot = grid_ref_tracked.tracked_grid_ref_snapshot;
//   (ghostty_tracked_grid_ref_snapshot)
/// C: rustty_tracked_grid_ref_snapshot
///
/// # Safety
/// `ref_`, if non-null, must be a live handle; `out_ref`, if non-null, must be a
/// valid, writable pointer to a `CGridRef`.
#[cfg(all(feature = "grid-introspection", feature = "snapshot"))]
#[no_mangle]
pub unsafe extern "C" fn rustty_tracked_grid_ref_snapshot(
    ref_: grid_ref_tracked::CTrackedGridRef,
    out_ref: *mut grid_ref::CGridRef,
) -> c_int {
    // SAFETY: pointer validity is the caller's contract.
    unsafe { grid_ref_tracked::tracked_grid_ref_snapshot(ref_, out_ref) }.to_c_int()
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
        let _ = super::color_scheme::report_encode as unsafe fn(_, _, _, _) -> _;
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
        // _ = unicode; (main.zig)
        let _ = super::unicode::codepoint_width as fn(_) -> _;
        let _ = super::unicode::grapheme_width as unsafe fn(_, _, _) -> _;
    }
}

// W36-B07 (Rust-only proving tests): the root-level wrapper guards in THIS file
// emit the reference's `log.warn` diagnostics. Ghostty routes them through
// `std_options.logFn` (`lib_vt.zig:363-381` selects `terminal/c/sys.zig`'s
// `logFn` for c_abi builds), which delivers them to the installed C log
// callback and DROPS them when none is installed (`if (global.log == null)
// return;`). They must never reach stderr.
#[cfg(test)]
mod log_root_tests {
    use super::sys::LogLevel;
    use std::ffi::c_void;
    use std::os::raw::c_int;
    use std::sync::Mutex;

    static CAPTURED: Mutex<Vec<(LogLevel, String, String)>> = Mutex::new(Vec::new());

    extern "C" fn log_cb(
        _userdata: *mut c_void,
        level: LogLevel,
        scope_ptr: *const u8,
        scope_len: usize,
        message_ptr: *const u8,
        message_len: usize,
    ) {
        // SAFETY: the sink hands us the scope/message byte ranges.
        let scope = unsafe { std::slice::from_raw_parts(scope_ptr, scope_len) };
        // SAFETY: as above.
        let message = unsafe { std::slice::from_raw_parts(message_ptr, message_len) };
        CAPTURED.lock().unwrap().push((
            level,
            String::from_utf8_lossy(scope).into_owned(),
            String::from_utf8_lossy(message).into_owned(),
        ));
    }

    /// Trip every root-wrapper guard that carries a diagnostic. Each guard
    /// rejects the selector BEFORE touching its handle argument, so the null
    /// handles are never dereferenced (that ordering is itself asserted here:
    /// a regression that dereferenced first would crash this test).
    fn trip_all_guards(sel: c_int) {
        const C_INVALID_VALUE: c_int = -2;

        assert_eq!(C_INVALID_VALUE, unsafe {
            super::rustty_build_info(sel, std::ptr::null_mut())
        });
        // The key/mouse setters return void: the reference's guard `return`s.
        unsafe { super::rustty_key_event_set_action(std::ptr::null_mut(), sel) };
        unsafe { super::rustty_key_event_set_key(std::ptr::null_mut(), sel) };
        unsafe { super::rustty_mouse_event_set_action(std::ptr::null_mut(), sel) };
        unsafe { super::rustty_mouse_event_set_button(std::ptr::null_mut(), sel) };
        assert_eq!(C_INVALID_VALUE, unsafe {
            super::rustty_selection_gesture_get(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                sel,
                std::ptr::null_mut(),
            )
        });
        assert_eq!(C_INVALID_VALUE, unsafe {
            super::rustty_selection_gesture_event_set(std::ptr::null_mut(), sel, std::ptr::null())
        });
    }

    #[test]
    fn root_wrapper_invalid_selectors_warn_through_the_c_log_callback() {
        // Serialize against every other test that mutates the process-wide sys
        // GLOBAL (the sys tests, the terminal/render scoped-log tests).
        let _guard = super::sys::tests::reset_global();
        CAPTURED.lock().unwrap().clear();

        // A selector value used by no other test, so the capture can be
        // filtered: tests run in parallel and other tests also trip
        // invalid-selector warnings while this callback is installed.
        const SEL: c_int = 31339;

        // No callback installed: `logFn` returns immediately, so nothing is
        // emitted anywhere (in particular NOT to stderr), and the return codes
        // are unchanged.
        trip_all_guards(SEL);
        assert!(CAPTURED.lock().unwrap().is_empty());

        // Install a capturing callback: now every diagnostic is delivered.
        assert_eq!(super::result::Result::Success, unsafe {
            super::sys::set(super::sys::Option_::Log, log_cb as *const c_void)
        });

        trip_all_guards(SEL);

        // Clear the callback before asserting, so a failure cannot strand it.
        assert_eq!(super::result::Result::Success, unsafe {
            super::sys::set(super::sys::Option_::Log, std::ptr::null())
        });

        let all = CAPTURED.lock().unwrap().clone();
        // Every diagnostic — whoever emitted it — carries the reference's level.
        for (level, _, _) in all.iter() {
            assert_eq!(LogLevel::Warning, *level);
        }
        let captured: Vec<(&String, &String)> = all
            .iter()
            .filter(|(_, _, m)| m.ends_with("=31339"))
            .map(|(_, s, m)| (s, m))
            .collect();
        // Scopes are the reference's `std.log.scoped` tags: `.build_info_c`
        // (build_info.zig:7), `.key_event` (key_event.zig:8), `.mouse_event`
        // (mouse_event.zig:11), `.selection_gesture_c`
        // (selection_gesture.zig:15).
        let expected = [
            ("build_info_c", "build_info invalid data value=31339"),
            ("key_event", "set_action invalid action value=31339"),
            ("key_event", "set_key invalid key value=31339"),
            ("mouse_event", "set_action invalid action value=31339"),
            ("mouse_event", "set_button invalid button value=31339"),
            (
                "selection_gesture_c",
                "selection_gesture_get invalid data value=31339",
            ),
            (
                "selection_gesture_c",
                "selection_gesture_event_set invalid option value=31339",
            ),
        ];
        assert_eq!(expected.len(), captured.len(), "captured: {all:?}");
        for (i, (scope, message)) in captured.iter().enumerate() {
            assert_eq!(expected[i].0, scope.as_str());
            assert_eq!(expected[i].1, message.as_str());
        }
    }
}
