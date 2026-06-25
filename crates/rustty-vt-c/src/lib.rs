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
pub mod color;
pub mod focus;
pub mod modes;
pub mod result;
pub mod sys;
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
