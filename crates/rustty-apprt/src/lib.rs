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
// The gtk4-rs deps are BOTH target-gated (Linux/FreeBSD) and `gtk`-feature-
// optional (`Cargo.toml`), so the macOS/embedded build is byte-behavior-identical
// whether or not the feature is named. `Runtime::Gtk` is already the Linux AND
// FreeBSD default (`rustty-build-config/src/apprt.rs:26-36`, the port of
// `apprt/runtime.zig:14-24` `.linux, .freebsd => .gtk`); the `rustty` bin binds
// it in L5.5.
//
// PORT NOTE (W2D/F-002): the OS half of the gate is `any(linux, freebsd)`, not
// `linux` — the reference has NO os.tag guard on the gtk apprt at all; the only
// OS input is `Runtime.default()` (`runtime.zig:18` `.linux, .freebsd => .gtk`),
// so FreeBSD builds and launches the SAME GTK application Linux does. gtk4-rs
// discovers its libraries through pkg-config with no OS assumption, and FreeBSD
// ports ships gtk4/libadwaita/gtk4-layer-shell/wayland/x11.
#[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
pub mod gtk;
pub mod none;

use rustty_core::Logging;

/// PARITY: `cli.args.parsePackedStruct(Logging, v.value)` (global.zig init).
/// `Logging` is a foreign type to rustty-args' `PackedStruct` trait (orphan
/// rule), so the packed-field contract is threaded through this local newtype.
/// Field order/names mirror the reference packed struct: `stderr`, `macos`.
#[derive(Clone, Copy, Default)]
struct LoggingParse(Logging);

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

/// Error returned by [`init_with_action`] / [`init`] (the global-state bootstrap).
///
/// PORT NOTE: the reference `state.init()` returns `!void` over the union of
/// allocator/env/cli/os errors. The cli `DetectError` half lives at the callers
/// that run detection (the bin; `rustty_init` in rustty-apprt-c) — apprt stays
/// cli-free (ADV-FIX 2). The `getenv` error is `Infallible` off-Windows and is
/// discharged inline. `crash.init` and `i18n.init` failures are WARN-only
/// (`catch |err| log.warn`, faithful), so they do not appear here.
#[derive(Debug)]
pub enum InitError {
    /// `apprt.runtime.resourcesDir(self.alloc)` failed (global.zig `init`).
    ResourcesDir(rustty_os::resourcesdir::ResourcesDirError),
    /// `try internal_os.ensureLocale(self.alloc)` failed.
    Locale(std::io::Error),
    /// `try glslang.init()` failed.
    Glslang(rustty_renderer::shadertoy::Error),
    /// `try oni.init(&.{oni.Encoding.utf8})` failed.
    Oniguruma(oni::Error),
}

/// Once-guard for the side-effecting bootstrap. The reference contract is
/// once-only (`ghostty_init` / `main()`); the port's callers (the bin, the C
/// ABI, and the gtk integration KATs) may re-enter, so the body runs exactly
/// once and later calls return the already-computed logging. A failed init
/// leaves the guard unset (retry allowed, mirroring the reference where a
/// failed `ghostty_init` means the process exits anyway).
static INIT_DONE: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// Initialize rustty global state (`rustty_core::STATE`) — the FULL
/// `global.zig` `GlobalState.init` body (BREAK #1, now wired), minus the two
/// pieces that live at other tiers by ratified decision:
///   * GPA/allocator selection — the allocator seam is `()` (Rust's global
///     allocator is process-wide; allocator doctrine, PORT_RULES).
///   * `cli.action.detectArgs` — detection lives ABOVE apprt (ADV-FIX 2); the
///     caller passes `action_detected` and this body applies the
///     action-implies-no-stderr rule + stashes the PRESENCE (`Option<()>`)
///     into `STATE.action`. The concrete `cli::rustty::Action` value is held
///     by the detecting tier (the bin's own state / rustty-apprt-c's stash).
pub fn init_with_action(action_detected: bool) -> Result<rustty_core::Logging, InitError> {
    use rustty_build_config as build_config;
    use rustty_core::logging::{log_with, Level};

    let mut done = INIT_DONE.lock().expect("init guard");
    if *done {
        return Ok(rustty_core::global_state().logging);
    }

    // (reference: `self.* = .{ ... }` zero-init happens implicitly — every
    // field below is computed before the single STATE.set at the end.)

    // self.gpa = ...; self.alloc = ...;
    //   Allocator doctrine: Rust's global allocator is process-wide; the
    //   GPA-vs-libc selection collapses (seam::Allocator = ()).

    // Discover and save the temporary directory path.
    //   `self.tmp_dir_path = try allocTmpDir(self.alloc, self.environ);`
    //
    //   PORT NOTE (P3-W7 T1/T4, area-E `E16`/`E20`): upstream's `global.init`
    //   imports `os/file.zig`'s `allocTmpDir` directly. `rustty-os` is ABOVE the
    //   leaf `rustty-global` (R4), so the edge is inverted: the initializer —
    //   which can see both — does the discovery and publishes it. This is what
    //   makes `file::randomTmpPath` a cached lookup instead of a per-call
    //   `allocTmpDir` + `freeTmpDir` pair.
    rustty_global::set_tmp_dir_path(
        rustty_os::file::allocTmpDir(&rustty_global::environ()).into_bytes(),
    );

    // We first try to parse any action that we may be executing.
    //   (done by the caller; `action_detected` carries the result.)
    // If we have an action executing, we disable logging by default ...
    // if ((try internal_os.getenv(self.alloc, "GHOSTTY_LOG"))) |v| { ... }
    //   R2: RUSTTY_LOG. The env error is Infallible off-Windows (discharged).
    //   P3-W7 T4: upstream moved this read INTO `global.init` and it now goes
    //   through the captured environment block:
    //     const v = self.environ.getAlloc(self.alloc, "GHOSTTY_LOG") catch ...
    let env = rustty_global::environ().get_alloc(b"RUSTTY_LOG");
    let logging = compute_logging(action_detected, env.as_deref());

    // Setup our signal handlers before logging
    init_signals();

    // Setup our Xev backend if we're dynamic
    //   PORT NOTE: `xev.dynamic` is comptime-FALSE in the port on every target
    //   (rustty-xev is the static kqueue surface on macOS; the Linux
    //   io_uring/epoll selection is dynamic.rs's own concern) — the detect call
    //   comptime-elides, exactly like the reference on a static backend.

    // Output some debug information right away
    log_with(
        Level::Info,
        "default",
        &format!("rustty version={}", build_config::VERSION_STRING),
        &logging,
    );
    log_with(
        Level::Info,
        "default",
        &format!("rustty build optimize={}", build_config::mode_string()),
        &logging,
    );
    log_with(
        Level::Info,
        "default",
        &format!("runtime={:?}", build_config::app_runtime()),
        &logging,
    );
    log_with(
        Level::Info,
        "default",
        &format!("font_backend={:?}", build_config::font_backend()),
        &logging,
    );
    // if (comptime build_config.font_backend.hasHarfbuzz())
    //     std.log.info("dependency harfbuzz={s}", .{harfbuzz.versionString()});
    if build_config::font_backend().has_harfbuzz() {
        let (mut major, mut minor, mut micro) = (0u32, 0u32, 0u32);
        // SAFETY: hb_version writes three u32 out-params (vendored harfbuzz).
        unsafe { harfbuzz_sys::hb_version(&mut major, &mut minor, &mut micro) };
        log_with(
            Level::Info,
            "default",
            &format!("dependency harfbuzz={major}.{minor}.{micro}"),
            &logging,
        );
    }
    // if (comptime build_config.font_backend.hasFontconfig())
    //     std.log.info("dependency fontconfig={d}", .{fontconfig.version()});
    if build_config::font_backend().has_fontconfig() {
        // SAFETY: FcGetVersion takes no args, reads a compile-time constant.
        let v = unsafe { fontconfig_sys::FcGetVersion() };
        log_with(
            Level::Info,
            "default",
            &format!("dependency fontconfig={v}"),
            &logging,
        );
    }
    log_with(
        Level::Info,
        "default",
        &format!("renderer={:?}", build_config::renderer()),
        &logging,
    );
    // std.log.info("libxev default backend={t}", ...) — static per target (see
    // the xev PORT NOTE above).
    let xev_backend = if cfg!(target_os = "macos") {
        "kqueue"
    } else {
        "io_uring"
    };
    log_with(
        Level::Info,
        "default",
        &format!("libxev default backend={xev_backend}"),
        &logging,
    );

    // As early as possible, initialize our resource limits.
    // self.rlimits = .init();
    let rlimits = rustty_global::ResourceLimits::init(rustty_os::fixMaxFiles());

    // Initialize our crash reporting.
    // crash.init(self.alloc) catch |err| log.warn(...);
    if let Err(err) = rustty_crash::init() {
        log_with(
            Level::Warn,
            "default",
            &format!("sentry init failed, no crash capture available err={err:?}"),
            &logging,
        );
    }

    // We need to make sure the process locale is set properly. Locale
    // affects a lot of behaviors in a shell.
    // try internal_os.ensureLocale(self.alloc);
    rustty_os::ensureLocale().map_err(InitError::Locale)?;

    // Initialize glslang for shader compilation
    // try glslang.init();
    rustty_renderer::shadertoy::init().map_err(InitError::Glslang)?;

    // Initialize oniguruma for regex
    // try oni.init(&.{oni.Encoding.utf8});
    //   Routed through the process-global OnceLock guard (ensure_init inits
    //   EXACTLY `&[Encoding::utf8()]`, the reference's set) — the ratified fix
    //   for concurrent onig_initialize (parallel-test C-init races).
    oni::testing::ensure_init().map_err(InitError::Oniguruma)?;

    // Find our resources directory once for the app so every launch
    // hereafter can use this cached value.
    // self.resources_dir = try apprt.runtime.resourcesDir(self.alloc);
    //
    // PORT NOTE (W16-GTK-02): `apprt.runtime` is the comptime-selected apprt
    // (`apprt.zig:41-49`), and EVERY apprt exports `resourcesDir`: `none`,
    // `browser` and `embedded` alias `internal_os.resourcesDir` verbatim
    // (`none.zig:6`, `browser.zig:2`, `embedded.zig:28`) while `gtk` overrides it
    // with the Flatpak-aware helper (`gtk.zig:4` -> `gtk/flatpak.zig:7-29`).
    // Rustty's runtime selector is the `cfg` that declares `mod gtk` (the
    // documented cfg-as-comptime-switch, top of this file), so the dispatch is
    // spelled as the same two-arm cfg here. Off the gtk gate this is byte-
    // identical to the previous unconditional `rustty_os::resourcesDir()` — the
    // none/embedded/browser alias.
    #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
    let resources_dir = gtk::resources_dir().map_err(InitError::ResourcesDir)?;
    #[cfg(not(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk")))]
    let resources_dir = rustty_os::resourcesDir().map_err(InitError::ResourcesDir)?;

    // Populate the config crate's `.resources` theme-search seam (BREAK #1)
    // from the SAME resources dir (list_themes C6 note); idempotent.
    let _ = rustty_config::theme::set_resources_dir(resources_dir.clone());

    // Setup i18n
    // if (self.resources_dir.app()) |v| internal_os.i18n.init(v) catch |err| log.warn(...);
    if let Some(v) = resources_dir.app() {
        if let Err(err) = rustty_os::i18n::init(v) {
            log_with(
                Level::Warn,
                "default",
                &format!("failed to init i18n, translations will not be available err={err:?}"),
                &logging,
            );
        }
    }

    // reference `pub var state: GlobalState = undefined;` set at init — the
    // rustty STATE is a OnceLock; construct the fully-populated state and set
    // it (ignore the already-set Err, keeping the bootstrap idempotent).
    let mut state = rustty_core::GlobalState::new((), resources_dir);
    state.action = if action_detected { Some(()) } else { None };
    state.logging = logging;
    state.rlimits = rlimits;
    let _ = rustty_core::STATE.set(state);
    // Mirror the SAME computed logging into the leaf process-global so crates
    // below rustty-core (rustty-config's `.config` scoped logger) see the
    // reference `global.logging()` accessor's value — the stderr gate that
    // keeps action output clean (`if (self.action != null) self.logging.stderr
    // = false;`). One computation site (`compute_logging` above), two mirrors.
    rustty_global::set_logging(logging);

    *done = true;
    Ok(logging)
}

/// Compat wrapper — the pre-WS4 `init()` signature (no CLI action detected:
/// the pure GUI-launch bootstrap). The gtk integration KATs and any embedded
/// caller that performs no detection use this.
pub fn init() -> Result<(), InitError> {
    init_with_action(false).map(|_| ())
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
        let rc = unsafe { libc::sigaction(libc::SIGPIPE, std::ptr::null(), &mut current) };
        assert_eq!(rc, 0);
        assert_eq!(current.sa_sigaction, libc::SIG_IGN);
    }

    #[test]
    fn full_init_populates_state_and_is_idempotent() {
        let logging = init_with_action(false).expect("full init");
        // STATE is set with the same logging + a populated rlimits snapshot.
        let state = rustty_core::global_state();
        assert_eq!(state.logging, logging);
        assert!(state.action.is_none());
        // fixMaxFiles returns Some on unix in practice; assert only what the
        // contract guarantees everywhere: the field was stamped by init.
        #[cfg(unix)]
        assert!(state.rlimits.nofile.is_some());

        // Second call: no re-run of side effects, same logging back.
        let again = init_with_action(false).expect("idempotent");
        assert_eq!(again, logging);

        // The init() compat wrapper also succeeds post-init.
        init().expect("compat wrapper");

        // Signals were installed by the full init.
        #[cfg(unix)]
        {
            let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
            unsafe { libc::sigaction(libc::SIGPIPE, std::ptr::null(), &mut current) };
            assert_eq!(current.sa_sigaction, libc::SIG_IGN);
        }
    }
}
