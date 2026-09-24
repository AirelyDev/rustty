//! rustty-apprt
//!
//! "apprt" is the "application runtime" package. This abstracts the
//! application runtime and lifecycle management such as creating windows,
//! getting user input (mouse/keyboard), etc.
//!
//! This enables compile-time interfaces to be built to swap out the underlying
//! application runtime. For example: pure macOS Cocoa, GTK+, browser, etc.
//!
//! The goal is to have different implementations share as much of the core
//! logic as possible, and to only reach out to platform-specific implementation
//! code when absolutely necessary.
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
//! ~~The honest residual = the 13 tracked Phase-3 leaves in `embedded.rs` (ALL
//! dcimgui/Metal Inspector surface ...) and GTK (a whole unported subsystem,
//! GAP_ANALYSIS §3.7 — why the crate is not `done`).~~ Stale (parity tail S-8b,
//! 2026-09-24): the E4 Inspector leaves are RESOLVED (DEFERRALS.md "rustty-apprt
//! embedded E4", WS6) and the GTK apprt is ported under `gtk/`. What still keeps
//! this crate `in-progress`: G3b comment coverage for the GTK files the parity
//! tail's T-D batch rewrites (`gtk/app.rs`, `class/{application,surface,window}.rs`,
//! `media.rs`, `portal.rs`, `portal/open_uri.rs`, `winproto/{wayland,x11}.rs`,
//! `winproto/wayland/hotkeys.rs` — ~730 dropped reference comments, restored after
//! that merge), and two unported, unrecorded reference fns
//! (`class/config.zig` `getMut`, `gsettings.zig` `getAlloc`; neither has a caller
//! in the reference GTK code). The libghostty C ABI wrappers live in the separate `rustty-apprt-c`
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
//
// The reference selector and its companion constant, carried verbatim:
//
// /// The implementation to use for the app runtime. This is comptime chosen
// /// so that every build has exactly one application runtime implementation.
// /// Note: it is very rare to use Runtime directly; most usage will use
// /// Window or something.
// pub const runtime = switch (build_config.artifact) { ... };
//
// /// True if all GPU operations (drawing, resource creation and
// /// destruction) must happen on the app thread. This is the case
// /// when the graphics context is owned by the app thread (GTK's
// /// GLArea). When false, the render thread performs GPU operations
// /// itself.
// pub const must_draw_from_app_thread =
//     @hasDecl(App, "must_draw_from_app_thread") and
//     App.must_draw_from_app_thread;
//
// PORT NOTE: `must_draw_from_app_thread` is the
// `rustty_apprt_api::App::MUST_DRAW_FROM_APP_THREAD` associated const (the
// `@hasDecl` probe becomes a defaulted trait const each backend may
// override; the GTK `App` sets it to `true`).
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

/// Whether the GTK winproto X11 / Wayland arms are built into this crate:
/// the reference's `build_options.x11` / `build_options.wayland`, whose
/// defaults come from the system GTK4's targets (`src/build/gtk.zig`,
/// S65-F6). Decided by `build.rs`; see `gtk_targets.rs`.
pub const GTK_X11: bool = cfg!(rustty_gtk_x11);
/// See [`GTK_X11`].
pub const GTK_WAYLAND: bool = cfg!(rustty_gtk_wayland);

#[cfg(test)]
mod gtk_targets;

// TEST-ONLY: the capture `log::Log` the restored `LOG`-row sites are asserted
// against (stage 14d-3). Not part of any published surface.
#[cfg(test)]
mod log_capture;

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
    rustty_global::set_tmp_dir_path(rustty_os::file::allocTmpDir(&rustty_global::environ()));

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

    // task-14a review round 3 (finding 2): mirror the SAME computed `logging`
    // into the leaf process-global HERE — immediately after it is computed,
    // not at the end of this function (`state.logging = logging`/
    // `STATE.set` below still happen at the end; this is purely about WHEN
    // the leaf-crate mirror below `rustty-core` becomes visible). The oracle
    // resolves `self.logging` this early too (`global.zig:59-82`'s `state =
    // .{ ..., .logging = .{} }` zero-init comment: "this MUST be initialized
    // before any log output because the log function uses the global
    // state") and every reference log call between here and the end of
    // `state.init()` (signals, version log, `crash.init`, locale, glslang,
    // oniguruma, resourcesDir, i18n) reads the ALREADY-RESOLVED value, not a
    // zero-initialized one — `self.logging` is set once, at the top, and
    // never reassigned before `logFn` needs it again. Before this fix, a
    // restored facade `log::warn!` in one of those sub-inits (e.g.
    // `rustty-os/src/locale.rs`'s S111-F5) would read
    // `rustty_global::logging()`'s default rather than this function's own
    // resolved `logging` — even though THIS function's own `log_with(...,
    // &logging)` call sites below (:241 onward) already pass the resolved
    // value explicitly. Moving the mirror here makes both paths agree.
    // `logging` is `Copy` and never reassigned after this line (see the
    // `let` — not `let mut` — above), so publishing it now and again
    // (harmlessly, `OnceLock::set` is idempotent) at the end is safe.
    rustty_global::set_logging(logging);

    // Setup our signal handlers before logging
    init_signals();

    // Setup our Xev backend if we're dynamic
    // if (comptime xev.dynamic) xev.detect() catch |err| {
    //     std.log.warn("failed to detect xev backend, falling back to " ++
    //         "most compatible backend err={}", .{err});
    // };
    //
    // CORRECTED (task 14d-1, closes S43-F2): the PORT NOTE that stood here
    // claimed "`xev.dynamic` is comptime-FALSE in the port on every target ...
    // the detect call comptime-elides". That is true on macOS/kqueue and FALSE
    // on Linux: `rustty-xev` exports `DYNAMIC = true`, `CANDIDATES =
    // [IoUring, Epoll]` and `detect()` there (`dynamic.rs`, the port of
    // `dynamic.zig:46/48/96`), exactly as the reference's `Dynamic` layer does.
    // So detection was NEVER RUN, the process kept `BACKEND_DEFAULT` (the
    // LAST candidate — epoll) whatever the kernel supported, and the backend
    // line below printed a value the port had not computed. `#[cfg]` is the
    // faithful image of the reference's `comptime xev.dynamic` fold: on the
    // static-backend targets the reference elides this block too.
    #[cfg(target_os = "linux")]
    if rustty_xev::DYNAMIC {
        if let Err(_err) = rustty_xev::detect() {
            // `detect()`'s error set is `error{NoAvailableBackends}` — one
            // variant — so Zig's `{}` renders exactly this text. The port's
            // unified D6 seam `Error` carries no name of its own, so the
            // reference's rendering is written out rather than derived from a
            // `{:?}` that would print `Error`.
            log_with(
                Level::Warn,
                "default",
                "failed to detect xev backend, falling back to most compatible \
                 backend err=error.NoAvailableBackends",
                &logging,
            );
        }
    }

    // Output some debug information right away
    log_with(
        Level::Info,
        "default",
        &format!("rustty version={}", rustty_version::VERSION_STRING),
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
    // std.log.info("libxev default backend={t}", .{xev.backend});
    //
    // CORRECTED (task 14d-1, closes S43-F2): this was a HARDCODED
    // `if cfg!(macos) { "kqueue" } else { "io_uring" }` — a value the port
    // never computed, and wrong on any Linux box where `detect()` (now run,
    // above) selects epoll. `xev.backend` is the STATIC `Backend` on a
    // single-candidate target and the `pub var backend` global that `detect`
    // just wrote on a dynamic one; both are read here through the same
    // per-target surface the reference reads.
    #[cfg(target_os = "linux")]
    let xev_backend = rustty_xev::backend().tag_name();
    #[cfg(not(target_os = "linux"))]
    let xev_backend = rustty_xev::Backend::default().tag_name();
    log_with(
        Level::Info,
        "default",
        &format!("libxev default backend={xev_backend}"),
        &logging,
    );

    // As early as possible, initialize our resource limits.
    // self.rlimits = .init();
    let rlimits = rustty_global::ResourceLimits::init(rustty_os::fixMaxFiles());

    // if (build_options.sentry) { ... }
    // (PORT NOTE, P4-E33/E34 `15c50c1db`+`a82637b53`: upstream gates the whole
    // block on the sentry build option so the env-map snapshot isn't built for
    // nothing; `crash.init` also checks it internally.)
    if rustty_crash::build_options::SENTRY {
        // Initialize our crash reporting. The environ map snapshot is
        // owned by crash.init (it is freed by the init thread).
        // const environ_map = try self.environ.createMap(self.alloc);
        // (PORT NOTE: `environ_map()` — the free accessor over the captured
        // global environ, `createMap`'s image — is infallible in Rust.)
        let environ_map = rustty_global::environ_map();
        // crash.init(self.alloc, environ_map) catch |err| { ... };
        if let Err(err) = rustty_crash::init(environ_map) {
            log_with(
                Level::Warn,
                "default",
                &format!("sentry init failed, no crash capture available err={err:?}"),
                &logging,
            );
        }
    }

    // We need to make sure the process locale is set properly. Locale
    // affects a lot of behaviors in a shell.
    //
    // We need to re-sync the environment after this completes.
    // try internal_os.ensureLocale(self.alloc);
    // syncEnviron();
    //
    // (oracle: src/global.zig:204-209, :307-322 — S43-F1: `ensureLocale`
    // mutates LANG/LC_* via setenv/unsetenv (directly, or on macOS via
    // `setLangFromCocoa`'s `setenv("LANG", ...)`), so the process-global
    // captured `ENVIRON` — already lazily captured earlier in this same init,
    // for the `tmp_dir_path` discovery / `RUSTTY_LOG` read above — must be
    // re-published, or every later `environ()`/`environ_map()` consumer
    // (termio child-process environments, a later `sync_environ()` caller)
    // keeps seeing the pre-locale-fix block.
    //
    // Regression-test note: an EARLIER version of this fix left
    // `rustty_core::GlobalState::new((), resources_dir)` below (now
    // `new_with_environ`) grabbing its OWN fresh `Environ::from_process()`
    // capture — a second, independent divergence from the oracle (which
    // reuses the caller-supplied environ; see that call site's comment) that
    // happened to republish the corrected environment as a side effect on
    // the one reachable success path, masking whether THIS line did anything
    // observable. Two divergences cancelling out is not parity: fixing only
    // this line left the masking in place and an end-to-end test below
    // vacuous, so the `GlobalState::new` call was changed to
    // `new_with_environ` in the same commit that added the regression test
    // below (recorded as finding N1, `.port/parity-closure/NEW-FINDINGS.md`).
    //
    // The regression test itself
    // (`crates/rustty-apprt/tests/global_init_resyncs_environ.rs`) is a
    // single-`#[test]` file of its own, for two independent reasons:
    //   * `init_with_action` is guarded by a process-wide `INIT_DONE`
    //     once-flag — the body runs at most once per process, and this
    //     crate's own `bootstrap_tests::full_init_populates_state_and_is_idempotent`
    //     unit test already calls it. `cargo test`'s unit-test harness runs
    //     every `#[test]` in that binary on shared threads with no defined
    //     ordering, so a test placed in that same binary would only exercise
    //     the `sync_environ()` call if it won the race to be the FIRST
    //     caller.
    //   * `std::env::set_var`/`remove_var` are process-global and `unsafe`,
    //     and race every concurrent `getenv` in a shared test binary
    //     (`parallel-cargo-test-exposes-c-init-races` /
    //     `rustty-vt/tests/kitty_temp_dir_security.rs`'s `TMPDIR` lesson); a
    //     file containing exactly one `#[test]` is this workspace's
    //     established pattern for making that sound (that file,
    //     `rustty-global/tests/env_non_utf8.rs`).
    //   The test is macOS-gated: an empty/unset `LANG` unconditionally
    //   routes `ensureLocale` through `setLangFromCocoa`, which `setenv`s
    //   `LANG` from `NSLocale.currentLocale` deterministically. Off Darwin,
    //   whether `ensureLocale` touches `LANG` at all depends on whether
    //   `setlocale(LC_ALL, "")` fails first — a property of the host's
    //   installed locales, not of this code path — making a bare `LANG=""`
    //   an unreliable trigger there; rustty is macOS-first (see
    //   `rustty-completion-plan`).
    rustty_os::ensureLocale().map_err(InitError::Locale)?;
    rustty_global::sync_environ();

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
    //
    // PORT NOTE (S43-F1 follow-up, N1): `GlobalState::new` grabs a FRESH
    // `Environ::from_process()` internally, which is itself an unfaithful
    // extra capture the oracle never performs — the oracle's `.main` arm
    // (`global.zig` `InitOpts`) reuses the ALREADY-captured environ, never
    // re-reads the process here. Threading the already-captured (and, by
    // this point, already `ensureLocale`-resynced) block through
    // `new_with_environ` instead matches that arm exactly, and stops this
    // construction from silently republishing over whatever `sync_environ()`
    // above just published (see `rustty-global/src/lib.rs:415-424`'s own doc
    // comment on `new_with_environ`, which already says this is how `.main`
    // should behave).
    // Bind the clone in its OWN statement rather than inline as an argument
    // expression: `rustty_global::environ()` returns a guard (`EnvironRef`)
    // wrapping a live `RwLock` READ lock, and Rust's temporary-lifetime rule
    // keeps a temporary alive until the end of its ENCLOSING statement — if
    // `.clone()` were called inline as an argument to `new_with_environ`
    // below, that read guard would still be held (not yet dropped) for the
    // full duration of the call, and `new_with_environ` takes a WRITE lock on
    // the very same `RwLock` internally (`environ::set_environ`). Since
    // `std::sync::RwLock` is not reentrant, that is a same-thread self-
    // deadlock — hit and diagnosed while wiring this fix.
    let env = rustty_global::environ().clone();
    let mut state = rustty_core::GlobalState::new_with_environ((), resources_dir, env);
    state.action = if action_detected { Some(()) } else { None };
    state.logging = logging;
    state.rlimits = rlimits;
    let _ = rustty_core::STATE.set(state);
    // task-14a review round 3 (finding 2): this call moved up to right after
    // `logging` is computed (see the PORT NOTE there) so every sub-init in
    // between reads the resolved value instead of a default. Left here too,
    // now a harmless idempotent no-op (`OnceLock::set` on an already-set
    // cell), so the "one computation site, mirrored into the leaf global"
    // shape stays visible at the same point `state.logging`/`STATE.set` are
    // finalized, without depending on execution order for correctness.
    rustty_global::set_logging(logging);

    *done = true;
    Ok(logging)
}

/// Cleans up the global state. This doesn't _need_ to be called but
/// doing so in dev modes will check for memory leaks.
///
/// Asserts that the state exists.
///
/// PORTS: `global.zig` `pub fn deinit() void`, the body `rustty_global`'s
/// leaf `GlobalState::deinit` cannot run because `crash` is above it (R4). Its
/// one caller is `main_ghostty.zig`'s `defer global.deinit();`, i.e. the
/// GUI-launch path of the `rustty` binary (parity S43-F4 / S44-F2).
///
/// PORT NOTE: `rustty_core::STATE` is a `OnceLock` that is never emptied, so
/// the owned `resources_dir` and `tmp_dir_path` are released by process exit
/// rather than here; there is no `std.Io` instance and no GPA (the leak check)
/// in the port. What remains observable is the crash-log flush.
pub fn deinit() {
    // self.resources_dir.deinit(self.alloc);
    //   (owned by the never-emptied `STATE`; see the PORT NOTE above)

    // Flush our crash logs
    // crash.deinit();
    rustty_crash::deinit();

    // Release our tmp_dir_path if needed
    // if (self.tmp_dir_path) |td| freeTmpDir(self.alloc, td);
    //   (`freeTmpDir` is a no-op off Windows; the port's path is an owned
    //   `Vec<u8>` in `rustty_global`, released by process exit.)

    // Release our I/O instance
    // self.io_impl.deinit();
    //
    // if (self.gpa) |*value| {
    //     // We want to ensure that we deinit the GPA because this is
    //     // the point at which it will output if there were safety violations.
    //     _ = value.deinit();
    // }
    //   (no `std.Io` or GPA in the port)
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
