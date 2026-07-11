//! rustty-apprt
//!
//! PORTS: ghostty src/apprt/ (gtk/, embedded, none, browser, action, surface, ipc, structs,
//! runtime, ...) minus the apprt-api iface seam; links gtk-rs/gobject, objc, wayland
//! (bindings, not reimplementations, per R3). Consumes breaks #8, #10, #13, #14.
//!
//! PORT STATUS: in-progress (chunks E0–E11 ALL LANDED; header realigned per
//! VALIDATION_REPORT.md §3b — the old "chunk E0 — scaffold" line was stale by
//! eleven chunks). `none` + `browser` are FULLY ported (EMBEDDED_BLUEPRINT §4);
//! `embedded` (the FIRST concrete `apprt_api` trait backend, §2) is FILLED
//! (E1 App backend, E2 Surface backend, E3 event-sink callbacks, E4 Inspector
//! inhabited placeholder, E6–E10 CAPI wiring, E11 C-ABI smoke KAT + finale).
//! The honest residual = the 13 tracked Phase-3 leaves in `embedded.rs` (ALL
//! dcimgui/Metal Inspector surface: `Surface::queueInspectorRender` +
//! `Inspector::{init,deinit,initMetal,renderMetal}` + `Backend::deinit` + the
//! 6 IO callbacks + `newFrame` — DEFERRALS.md "rustty-apprt embedded E4"), and
//! GTK (a whole unported subsystem, GAP_ANALYSIS §3.7 — why the crate is not
//! `done`). The libghostty C ABI wrappers live in the separate `rustty-apprt-c`
//! top-sink crate (vt-c precedent, §3). This crate also hosts the global-state
//! precursor [`init`] the C ABI's `rustty_init` calls.
//!
//! Carved incrementally (R1: every line/comment/test{} ported as the crate is
//! carved, leaves-first, under the acyclicity gate per R4).

// PORT NOTE: the reference `apprt.zig` selects ONE runtime backend at comptime
// (`pub const runtime = switch (build_config.artifact) { ... }`) and re-exports
// `none`/`browser`/`embedded` as sibling modules. In rustty each backend is a
// module below; the concrete `A` is chosen by the consumer (the embedded C ABI
// binds `A = embedded::App`, EMBEDDED_BLUEPRINT §2). The `apprt.ipc`/`action`/
// `surface`/`runtime`/`structs` interface seam already lives in
// `rustty-apprt-api` (break #8); this crate carries the concrete backends.
pub mod browser;
pub mod embedded;
pub mod none;

/// Error returned by [`init`] (the global-state bootstrap).
///
/// PORT NOTE: the reference `state.init()` returns `!void` over the union of
/// allocator/env/cli/os errors. The E0 precursor can only fail while resolving
/// the resources directory (the one upward call the minimal bootstrap makes); the
/// rest of the reference error set arrives with the deferred full `state.init()`.
#[derive(Debug)]
pub enum InitError {
    /// PORT NOTE: `apprt.runtime.resourcesDir(self.alloc)` failed (global.zig `init`).
    ResourcesDir(rustty_os::resourcesdir::ResourcesDirError),
}

/// Initialize rustty global state (`rustty_core::STATE`).
///
/// PORT NOTE: the rustty analogue of the reference `global.zig` `state.init()`
/// that `ghostty_init` (main_c.zig) calls. The FULL reference body — GPA /
/// allocator selection, CLI action detection, logging config, `crash.init`,
/// locale/i18n, POSIX signal setup — is the BREAK #1 deferred seam
/// (`rustty_global::GlobalState::init` is commented pending this rustty-core /
/// apprt wiring). The MINIMAL precursor the libghostty C ABI needs (EMBEDDED_BLUEPRINT
/// §3 "Global-state precursor" + risk 4) is to SET `STATE` so `global_state()`
/// reads (the CAPI `free_text`/`tty_name`/`Text.deinit` paths) don't panic:
/// the process-global allocator seam is `()` (Rust's global allocator is
/// process-wide), and `resources_dir` is resolved via the apprt-runtime
/// `resourcesDir` call the reference `init` makes
/// (`self.resources_dir = try apprt.runtime.resourcesDir(self.alloc)`).
///
/// Idempotent: a second call is a no-op (`OnceLock::set` returns `Err` once set,
/// which is ignored) — matching the once-only `ghostty_init` contract while being
/// robust to repeated bootstrap.
pub fn init() -> Result<(), InitError> {
    // reference (global.zig init, the fields set + the one upward call kept):
    //   self.resources_dir = try apprt.runtime.resourcesDir(self.alloc);
    let resources_dir = rustty_os::resourcesDir().map_err(InitError::ResourcesDir)?;

    // reference: `self.* = .{ .alloc = ..., .action = null, ... }` — the
    // seam-free field construction (rustty_global `GlobalState::new`). The
    // allocator seam is `()` (see PORT NOTE above).
    let state = rustty_core::GlobalState::new((), resources_dir);

    // reference `global.zig` `pub var state: GlobalState = undefined;` set at
    // `init` — the rustty `STATE` is a `OnceLock`; set it (ignore the already-set
    // `Err`, keeping the bootstrap idempotent).
    let _ = rustty_core::STATE.set(state);

    Ok(())
}
