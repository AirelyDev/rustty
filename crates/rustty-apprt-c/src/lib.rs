//! rustty-apprt-c
//!
//! PORTS: the libghostty embedding C API — `src/apprt/embedded.zig`'s `CAPI`
//! block + the `src/main_c.zig` C API hub (the `ghostty_*` `@export` surface for
//! `apprt.embedded`). The Rust backends in `rustty-apprt` (the `embedded`/`none`/
//! `browser` modules) are the source of truth; this crate is a pure FFI
//! projection (R3). cdylib+staticlib+rlib; cbindgen -> include/rustty-apprt.h
//! with `rustty_*` symbols (R2).
//!
//! PORT STATUS: in-progress (chunk E0 — scaffold). The crate compiles/links as a
//! cdylib/staticlib/rlib and exports the global-state precursor `rustty_init`.
//! The 71 `ghostty_*` (`rustty_*`) exports + their CAPI extern structs land in
//! chunks E1..E10; the const-size-asserts (D2) + cbindgen header generation +
//! the C compile+link smoke KAT land in E11 (EMBEDDED_BLUEPRINT §3/§6/§7).
//!
//! PORT NOTE: this is the crate root, the rustty analog of `src/main_c.zig` (the
//! C API hub) together with `apprt.embedded`'s `CAPI` decl. `main_c.zig`
//! `@export`s the C API symbols and references `apprt.runtime.CAPI`; in rustty
//! each C API symbol is authored as a `#[no_mangle] pub unsafe extern "C" fn
//! rustty_<name>(...)` 1:1 with the reference `pub export fn ghostty_<name>` /
//! `@export(&fn, .{ .name = "ghostty_<name>" })` lines (rustty_ prefix per R2).
//! The `extern "C"` wrappers are the single point that crosses the C ABI; they
//! forward to the ported `rustty-apprt` backend.
//!
//! Carved incrementally (R1: every line/comment/test{} ported as the crate is
//! carved, leaves-first, under the acyclicity gate per R4).

use libc::{c_char, c_int};

// ===================================================================
// C API exports — 1:1 with the reference `pub export fn ghostty_*` /
// `@export(..., .{ .name = "ghostty_*" })` lines (rustty_ prefix per R2).
// cbindgen emits these into include/rustty-apprt.h. The bodies forward to the
// ported `rustty-apprt` backend; the `extern "C"` boundary is the ONLY place
// the C ABI is crossed.
// ===================================================================

/// C: rustty_init
///
/// Initialize rustty global state.
///
/// PORT NOTE: rustty analogue of `ghostty_init` (main_c.zig:105):
/// ```zig
/// pub export fn ghostty_init(argc: usize, argv: [*][*:0]u8) c_int {
///     assert(builtin.link_libc);
///     std.os.argv = argv[0..argc];
///     state.init() catch |err| {
///         std.log.err("failed to initialize ghostty error={}", .{err});
///         return 1;
///     };
///     return 0;
/// }
/// ```
/// The reference `assert(builtin.link_libc)` is a no-op here (Rust's `std`
/// always links libc). The `std.os.argv = argv[0..argc]` store is consumed only
/// by the deferred CLI-action detection inside the full `state.init()` (the
/// BREAK #1 global-state seam, not yet ported — see `rustty_apprt::init`); until
/// that lands, `argv` is accepted but not stored. `state.init()` maps to the
/// `rustty_apprt::init()` bootstrap, which sets `rustty_core::STATE` (the
/// `global.zig` `state` analogue) so the CAPI's `global.alloc`-reading paths
/// (`ghostty_surface_free_text`/`_tty_name`/`Text.deinit`) have a live state.
/// EMBEDDED_BLUEPRINT §3 "Global-state precursor" + risk 4: the smoke KAT (E11)
/// MUST call this before `ghostty_app_new`.
///
/// # Safety
/// `argv` must point to `argc` valid, NUL-terminated C strings (the process
/// argv), as the reference `ghostty_init` requires. This scaffold body does not
/// dereference `argv`, but the contract is fixed now for the deferred
/// argv-storage port.
#[no_mangle]
pub unsafe extern "C" fn rustty_init(argc: usize, argv: *mut *mut c_char) -> c_int {
    // reference: assert(builtin.link_libc);  (Rust `std` always links libc.)
    // reference: std.os.argv = argv[0..argc];
    //   Stored for the deferred CLI-action detection in the full state.init()
    //   (BREAK #1 seam); not yet ported, so accepted-but-unused for now.
    let _ = (argc, argv);

    // reference:
    //   state.init() catch |err| {
    //       std.log.err("failed to initialize ghostty error={}", .{err});
    //       return 1;
    //   };
    //   return 0;
    match rustty_apprt::init() {
        Ok(()) => 0,
        Err(err) => {
            // std.log.err(...) — no scoped-log sink in this leaf (vt-c
            // `rustty_build_info` precedent): emit to stderr.
            eprintln!("failed to initialize rustty error={err:?}");
            1
        }
    }
}
