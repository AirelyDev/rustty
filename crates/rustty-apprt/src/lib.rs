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
// PORT NOTE (R5): the reference re-exports `gtk` unconditionally (`apprt.zig:17`
// `pub const gtk = @import("apprt/gtk.zig");`) and lets the comptime `runtime`
// switch pick it (`apprt.zig:45` `.gtk => gtk,`). In rustty the selector is a
// `cfg` instead: the GTK backend is a NATIVE gtk4-rs apprt (AD-1) — a sibling of
// `embedded`/`none` that `impl`s the `rustty_apprt_api` seam directly against
// GObject widgets, NOT an app over the libghostty C ABI (that is the
// macOS/Swift path, `rustty-apprt-c`, which this leaves untouched).
//
// The gtk4-rs deps are BOTH Linux-target-gated and `gtk`-feature-optional
// (`Cargo.toml`), so the macOS/embedded build is byte-behavior-identical whether
// or not the feature is named. `Runtime::Gtk` is already the Linux default
// (`rustty-build-config/src/apprt.rs:30`); the `rustty` bin binds it in L5.5.
#[cfg(all(target_os = "linux", feature = "gtk"))]
pub mod gtk;
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

use rustty_core::Logging;

/// PARITY: `cli.args.parsePackedStruct(Logging, v.value)` (global.zig init).
/// `Logging` is a foreign type to rustty-args' `PackedStruct` trait (orphan
/// rule), so the packed-field contract is threaded through this local newtype.
/// Field order/names mirror the reference packed struct: `stderr`, `macos`.
#[derive(Clone, Copy)]
struct LoggingParse(Logging);

impl Default for LoggingParse {
    fn default() -> Self {
        Self(Logging::default())
    }
}

impl rustty_args::args::PackedStruct for LoggingParse {
    // inline for (info.fields) |field| @field(result, field.name) = b;
    fn set_all(&mut self, b: bool) {
        self.0.stderr = b;
        self.0.macos = b;
    }

    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"stderr" => {
                self.0.stderr = value;
                true
            }
            b"macos" => {
                self.0.macos = value;
                true
            }
            _ => false,
        }
    }

    fn field_names() -> &'static [&'static [u8]] {
        &[b"stderr", b"macos"]
    }
}

/// The logging-config slice of the reference `GlobalState.init`:
///
///   if (self.action != null) self.logging.stderr = false;
///   if ((try internal_os.getenv(self.alloc, "GHOSTTY_LOG"))) |v| {
///       defer v.deinit(self.alloc);
///       self.logging = cli.args.parsePackedStruct(Logging, v.value) catch .{};
///   }
///
/// R2: the env var is `RUSTTY_LOG` (the `RUSTTY_RESOURCES_DIR` retarget
/// precedent). Pure over its inputs so it is unit-testable; `init_with_action`
/// feeds it the live env read.
fn compute_logging(action_detected: bool, env_value: Option<&[u8]>) -> Logging {
    let mut logging = Logging::default();
    // If we have an action executing, we disable logging by default
    // since we write to stderr we don't want logs messing up our
    // output.
    if action_detected {
        logging.stderr = false;
    }
    if let Some(v) = env_value {
        // `catch .{}` — a parse failure falls back to the DEFAULT Logging.
        logging = rustty_args::args::parse_packed_struct::<LoggingParse>(v)
            .map(|p| p.0)
            .unwrap_or_else(|_| Logging::default());
    }
    logging
}

// fn initSignals() void { ... } (global.zig — see the verbatim body preserved
// in rustty-global's lib.rs seam comment; this is its live wiring.)
fn init_signals() {
    // Only posix systems.
    // if (comptime builtin.os.tag == .windows) return;
    #[cfg(unix)]
    unsafe {
        // var sa: p.Sigaction = .{
        //     .handler = .{ .handler = p.SIG.IGN },
        //     .mask = p.sigemptyset(),
        //     .flags = 0,
        // };
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = libc::SIG_IGN;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;

        // We ignore SIGPIPE because it is a common signal we may get
        // due to how we implement termio. When a terminal is closed we
        // often write to a broken pipe to exit the read thread. This should
        // be fixed one day but for now this helps make this a bit more
        // robust.
        // p.sigaction(p.SIG.PIPE, &sa, null);
        libc::sigaction(libc::SIGPIPE, &sa, std::ptr::null_mut());
    }
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

    // Populate the config crate's `.resources` theme-search seam (BREAK #1) from
    // the SAME resources dir (list_themes.rs C6 note: both the core `STATE` and
    // this config seam are populated by the bin's init so they stay consistent).
    // Without this, bundled themes (`theme=NAME`) and `+list-themes` never
    // resolve the resources-dir location. `set_resources_dir` returns `Err` if
    // already set (idempotent bootstrap); ignore it, mirroring the `STATE.set`
    // below.
    let _ = rustty_config::theme::set_resources_dir(resources_dir.clone());

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

#[cfg(test)]
mod bootstrap_tests {
    use super::*;

    // reference (global.zig init): `if (self.action != null) self.logging.stderr = false;`
    // then `self.logging = cli.args.parsePackedStruct(Logging, v.value) catch .{};`
    #[test]
    fn compute_logging_defaults() {
        let l = compute_logging(false, None);
        assert_eq!(l, rustty_core::Logging::default());
    }

    #[test]
    fn compute_logging_action_disables_stderr() {
        let l = compute_logging(true, None);
        assert!(!l.stderr);
    }

    #[test]
    fn compute_logging_env_stderr_flag() {
        // RUSTTY_LOG=stderr with an action: the env parse REPLACES the
        // action-adjusted logging (reference assigns `self.logging = ... catch .{}`).
        let l = compute_logging(true, Some(b"stderr"));
        assert!(l.stderr);
    }

    #[test]
    fn compute_logging_env_bool_shorthand() {
        let l = compute_logging(false, Some(b"true"));
        assert!(l.stderr && l.macos);
        let l = compute_logging(false, Some(b"false"));
        assert!(!l.stderr && !l.macos);
    }

    #[test]
    fn compute_logging_env_negation() {
        let l = compute_logging(false, Some(b"no-stderr,no-macos"));
        assert!(!l.stderr && !l.macos);
    }

    #[test]
    fn compute_logging_env_invalid_falls_back_to_default() {
        // reference `catch .{}` — a parse error yields Logging{} (NOT the
        // action-adjusted value).
        let l = compute_logging(true, Some(b"bogus-flag"));
        assert_eq!(l, rustty_core::Logging::default());
    }

    #[cfg(unix)]
    #[test]
    fn init_signals_ignores_sigpipe() {
        init_signals();
        let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::sigaction(libc::SIGPIPE, std::ptr::null(), &mut current)
        };
        assert_eq!(rc, 0);
        assert_eq!(current.sa_sigaction, libc::SIG_IGN);
    }
}
