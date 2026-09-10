//! The main entrypoint for the `rustty` application. This also serves
//! as the process initialization code for the `librustty` library.
//!
//! PORTS: ghostty src/main_ghostty.zig — the `main()` dispatch (action → GUI),
//! `logFn`, and `std_options`. Byte-faithful (R1); the `Ghostty`/`libghostty`
//! product-identity strings are retargeted to the Rustty product name (R2 —
//! routed through rustty-identity).
//!
//! # ADV-FIX 2 — action DETECTION lives in the BIN
//!
//! The reference `main()` calls `state.init()` (global.zig), which detects the
//! action via `cli.action.detectArgs(cli.ghostty.Action, alloc)` and stores it in
//! the process-global `GlobalState.action`. In Rustty that detection CANNOT live
//! in `rustty_apprt::init` (apprt sits below cli in the port order, so calling cli
//! there cycles `apprt→cli`), and `rustty_core::STATE` is bound `Action=()` and
//! cannot hold a cli action. So the `rustty` BIN — which legitimately deps cli —
//! runs the detection itself over `os::args` and constructs its OWN
//! `rustty_global::GlobalState<cli::rustty::Action>` (NOT core's `Action=()`
//! STATE). `rustty_apprt::init` stays cli-free.

// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const posix = std.posix;
// const build_config = @import("build_config.zig");
use rustty_build_config as build_config;
// const macos = @import("macos");           -> logFn unified-logging (WS4:
//   `rustty_core::logging::log_with`'s macOS sink, via `rustty_os::macos::Log`)
// const cli = @import("cli.zig");
use rustty_cli::action::{self, DetectError};
use rustty_cli::rustty::Action;
// const renderer = @import("renderer.zig"); -> only used by the deferred GUI leaf
// const apprt = @import("apprt.zig");
//
// const App = @import("App.zig");           -> the deferred Phase-3 GUI leaf
// const Ghostty = @import("main_c.zig").Ghostty;
//   -> the libghostty C-API hub is re-homed to rustty-apprt-c (ADV-FIX 6); the
//      `rustty` exe does not reference it.
// const state = &@import("global.zig").state;
//   -> ADV-FIX 2: the bin builds its OWN `GlobalState<Action>` (see `state_init`),
//      NOT the process-global `rustty_core::STATE` (which is `Action=()`).
use rustty_global::{GlobalState, Logging};

use std::io::Write;

// /// The return type for main() depends on the build artifact. The lib build
// /// also calls "main" in order to run the CLI actions, but it calls it as
// /// an API and not an entrypoint.
// const MainReturn = switch (build_config.artifact) {
//     .lib => noreturn,
//     else => void,
// };
//
// PORT NOTE: the `rustty` exe is the `else => void` arm. `main` never returns
// normally — every path ends in `std::process::exit` (action / usage banner /
// init error) or the deferred GUI leaf (`unimplemented!()`), so the Rust `fn main`
// returns `()` after the (diverging) dispatch.

/// The bin's `state.init()` failure set — a faithful projection of the reference
/// `GlobalState.init` error union that `main()`'s `state.init() catch` switches
/// on. Only the two live upward calls can fail: CLI action detection
/// ([`DetectError`]) and the FULL `rustty_apprt::init_with_action` failure set
/// (WS4 — [`rustty_apprt::InitError`]).
enum InitError {
    // error.MultipleActions / error.InvalidAction (cli.action.detectArgs).
    Detect(DetectError),
    // The FULL `rustty_apprt::init_with_action` failure set (WS4) — surfaced by
    // the reference `else =>` arm ("invalid CLI invocation err={}").
    Init(rustty_apprt::InitError),
}

/// Initialize our global state (the bin's own `GlobalState<Action>`).
///
/// PORTS: `global.zig` `GlobalState.init` — action detection (ADV-FIX 2, LIVE
/// here) plus the FULL reference init body (GPA/allocator selection collapse,
/// GHOSTTY_LOG parse, signal setup, xev detect, the version/optimize/runtime/
/// font/renderer/libxev log lines, rlimit init, crash.init, ensureLocale,
/// glslang.init, oni.init, resourcesDir, i18n.init) — wired in
/// `rustty_apprt::init_with_action` (WS4, the BREAK #1 seam landed). Reference
/// body for the slice this fn drives directly:
///
///   // We first try to parse any action that we may be executing.
///   self.action = try cli.action.detectArgs(cli.ghostty.Action, self.alloc);
///   // If we have an action executing, we disable logging by default ...
///   if (self.action != null) self.logging.stderr = false;
///   ... (the rest — see `rustty_apprt::init_with_action`'s doc)
fn state_init() -> Result<GlobalState<Action>, InitError> {
    // Initialize ourself to nothing so we don't have any extra state.
    // (reference `self.* = .{ .gpa = null, .alloc = undefined, .action = null,
    //  .logging = .{}, .rlimits = .{}, .resources_dir = .{} };` — the seam-free
    //  data-field construction; the allocator seam is `()` — Rust's global
    //  allocator is process-wide, so there is no GPA-vs-libc selection to make.)
    let mut state: GlobalState<Action> = GlobalState::new((), ());

    // We first try to parse any action that we may be executing.
    // self.action = try cli.action.detectArgs(cli.ghostty.Action, self.alloc);
    //   ADV-FIX 2: `detect_args::<Action>` walks `os::args` (the reference
    //   `detectArgs` over the process args iterator) — this is the detection the
    //   reference performed inside `global.zig`, relocated to the bin.
    state.action = action::detect_args::<Action>().map_err(InitError::Detect)?;

    // If we have an action executing, we disable logging by default ... —
    // WS4: the rule now lives INSIDE `init_with_action` (compute_logging),
    // which also applies the RUSTTY_LOG override; the bin mirrors the result
    // into its own GlobalState<Action> so `log_fn` reads one source of truth.
    // (The pre-WS4 inline `if state.action.is_some() { stderr = false }` is
    // deleted — it is now dead duplication.)

    // The FULL reference init body (GHOSTTY_LOG parse, signals, xev, the
    // version/... log lines, rlimits, crash.init, ensureLocale, glslang.init,
    // oni.init, resourcesDir, i18n.init) — the BREAK #1 seam, wired in
    // rustty_apprt::init_with_action (WS4).
    state.logging =
        rustty_apprt::init_with_action(state.action.is_some()).map_err(InitError::Init)?;

    Ok(state)
}

// pub fn main() !MainReturn {
pub fn main() {
    // task-14a review round 2 (CRITICAL 2 item 2): install BEFORE
    // `state_init()`, not after. `state_init()` -> `rustty_apprt::
    // init_with_action` runs the bootstrap sequence (locale, crash-init,
    // glslang, oniguruma, resources dir, i18n) that several `LOG` ledger rows
    // restore diagnostics INSIDE (e.g. S111-F5 in `rustty-os/src/locale.rs`),
    // and `rustty_core::STATE` is not populated until the very end of that
    // sequence (`rustty-apprt/src/lib.rs`'s `STATE.set`). Installing after
    // `state_init()` returned meant every one of those bootstrap-window sites
    // would execute before ANY logger existed and be silently dropped, while
    // the oracle's `std_options.logFn` is live from process entry. Installing
    // here instead means: bootstrap-window sites read `Logging::default()`
    // (via `logging::log`'s own pre-`STATE` fallback) — genuinely
    // oracle-faithful, since the reference's zero-initialized `state` gives
    // the exact same "default logging config" behavior before its own
    // `state.init()` resolves the real flags — and every site AFTER
    // `state_init()` succeeds reads the fully resolved `RUSTTY_LOG`/
    // action-derived flags once `STATE` is set.
    rustty_core::logging::install_app_log_backend();
    // We first start by initializing our global state. This will setup
    // process-level state we need to run the terminal. The reason we use
    // a global is because the C API needs to be able to access this state;
    // no other Zig code should EVER access the global state.
    // state.init() catch |err| { ... defer posix.exit(1); switch (err) { ... } };
    let state = match state_init() {
        Ok(state) => state,
        Err(err) => {
            // var buffer: [1024]u8; var stderr_writer = File.stderr().writer(&buffer);
            // const stderr = &stderr_writer.interface;
            let mut stderr = std::io::stderr().lock();
            // defer posix.exit(1);
            // const ErrSet = @TypeOf(err) || error{Unknown};
            // switch (@as(ErrSet, @errorCast(err))) {
            match err {
                // error.MultipleActions => try stderr.print(
                //     "Error: multiple CLI actions specified. You must specify only one\n" ++
                //         "action starting with the `+` character.\n", .{}),
                InitError::Detect(DetectError::MultipleActions) => {
                    let _ = stderr.write_all(
                        b"Error: multiple CLI actions specified. You must specify only one\n\
                          action starting with the `+` character.\n",
                    );
                }
                // error.InvalidAction => try stderr.print(
                //     "Error: unknown CLI action specified. CLI actions are specified with\n" ++
                //         "the '+' character.\n\n" ++
                //         "All valid CLI actions can be listed with `ghostty +help`\n", .{}),
                //   R2: "ghostty" -> the Rustty product name (rustty-identity APP_NAME).
                InitError::Detect(DetectError::InvalidAction) => {
                    let msg = format!(
                        "Error: unknown CLI action specified. CLI actions are specified with\n\
                         the '+' character.\n\n\
                         All valid CLI actions can be listed with `{} +help`\n",
                        rustty_identity::APP_NAME,
                    );
                    let _ = stderr.write_all(msg.as_bytes());
                }
                // else => try stderr.print("invalid CLI invocation err={}\n", .{err}),
                InitError::Init(e) => {
                    let _ =
                        stderr.write_all(format!("invalid CLI invocation err={e:?}\n").as_bytes());
                }
            }
            // try stderr.flush();
            let _ = stderr.flush();
            // defer posix.exit(1);
            std::process::exit(1);
        }
    };
    // defer state.deinit();
    //   PORT NOTE: every path below ends in `std::process::exit` (or the diverging
    //   GUI leaf), which — like the reference `posix.exit` — does NOT run deferred
    //   cleanup, so `state.deinit()` is faithfully skipped on those paths (leak
    //   diagnostics are the GPA seam, deferred). `state` is dropped otherwise.
    // const alloc = state.alloc;
    //   PORT NOTE: the reference `alloc` param is dropped tree-wide (Rust uses the
    //   global allocator); actions take no allocator argument.

    // if (comptime builtin.mode == .Debug) {
    //     std.log.warn("This is a debug build. Performance will be very poor.", .{});
    //     std.log.warn("You should only use a debug build for developing Ghostty.", .{});
    //     std.log.warn("Otherwise, please rebuild in a release mode.", .{});
    // }
    //   R2: "developing Ghostty" -> "developing Rustty".
    if cfg!(debug_assertions) {
        log_fn(
            LogLevel::Warn,
            "This is a debug build. Performance will be very poor.",
            &state.logging,
        );
        log_fn(
            LogLevel::Warn,
            "You should only use a debug build for developing Rustty.",
            &state.logging,
        );
        log_fn(
            LogLevel::Warn,
            "Otherwise, please rebuild in a release mode.",
            &state.logging,
        );
    }

    // Execute our action if we have one
    // if (state.action) |action| {
    if let Some(action) = state.action {
        // std.log.info("executing CLI action={}", .{action});
        log_fn(
            LogLevel::Info,
            &format!("executing CLI action={action:?}"),
            &state.logging,
        );
        // posix.exit(action.run(alloc) catch |err| err: {
        //     std.log.err("CLI action failed error={}", .{err});
        //     break :err 1;
        // });
        // return;
        let code = action.run().unwrap_or_else(|err| {
            log_fn(
                LogLevel::Err,
                &format!("CLI action failed error={err:?}"),
                &state.logging,
            );
            1
        });
        std::process::exit(i32::from(code));
    }

    // if (comptime build_config.app_runtime == .none) {
    if build_config::app_runtime() == build_config::Runtime::None {
        // const stdout = std.io.getStdOut().writer();
        let mut stdout = std::io::stdout().lock();
        // try stdout.print("Usage: ghostty +<action> [flags]\n\n", .{});
        // try stdout.print(\\This is the Ghostty helper CLI ... \\, .{});
        //   R2: the `Usage:` command position keeps the lowercase command name
        //   (`app` = APP_NAME); every other position here is the capitalized
        //   product name per the oracle (`app_name` = APP_NAME_TITLECASE).
        let app = rustty_identity::APP_NAME;
        let app_name = rustty_identity::APP_NAME_TITLECASE;
        let banner = format!(
            "Usage: {app} +<action> [flags]\n\n\
             This is the {app_name} helper CLI that accompanies the graphical {app_name} app.\n\
             To launch the terminal directly, please launch the graphical app\n\
             (i.e. {app_name}.app on macOS). This CLI can be used to perform various\n\
             actions such as inspecting the version, listing fonts, etc.\n\
             \n\
             On macOS, the terminal can also be launched using `open -na {app_name}.app`,\n\
             or `open -na {app_name}.app --args --foo=bar --baz=qux` to pass arguments.\n\
             \n\
             We don't have proper help output yet, sorry! Please refer to the\n\
             source code or Discord community for help for now. We'll fix this in time.\n",
        );
        let _ = stdout.write_all(banner.as_bytes());
        let _ = stdout.flush();

        // posix.exit(0);
        std::process::exit(0);
    }

    // Create our app state
    // const app: *App = try App.create(alloc);
    // defer app.destroy();
    //
    // // Create our runtime app
    // var app_runtime: apprt.App = undefined;
    // try app_runtime.init(app, .{});
    // defer app_runtime.terminate();
    //
    // // Since - by definition - there are no surfaces when first started, the
    // // quit timer may need to be started. The start timer will get cancelled if/
    // // when the first surface is created.
    // if (@hasDecl(apprt.App, "startQuitTimer")) app_runtime.startQuitTimer();
    //
    // // Run the GUI event loop
    // try app_runtime.run();
    //
    // PORT NOTE (Phase-3 GUI leaf, L5.1 T12 — LANDED for `Runtime::Gtk`): the
    // windowed app path creates `rustty_core::App` and drives the concrete apprt
    // backend event loop. The reference reaches here only for a GUI build
    // (`app_runtime != .none`); the headless `.none` bin never does (it exits via
    // the usage banner above). For the NATIVE GTK backend the whole leaf
    // (`App.create` + `apprt.App.init`/`terminate`/`run`) is [`run_gtk`], selected
    // on `app_runtime() == .gtk` and gated
    // `#[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]`
    // (R5). A full window still needs the Surface tasks (T13+) and the
    // L4 Renderer; today this enters the hand-rolled loop and ticks the core app.
    //
    // PORT NOTE (W2D/F-002): the OS half is `any(linux, freebsd)` because the
    // reference's own reach-condition is `app_runtime != .none`, and
    // `Runtime.default()` is `.gtk` for BOTH (`apprt/runtime.zig:14-24`
    // `.linux, .freebsd => .gtk`). Before this widening a FreeBSD build had
    // `app_runtime() == .gtk` but no compiled arm, so it fell through to the
    // `unimplemented!` below — a deterministic panic where the reference
    // launches its GUI.
    #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
    if build_config::app_runtime() == build_config::Runtime::Gtk {
        // `run_gtk` diverges (`-> !`): every arm ends in `std::process::exit`
        // after running the reference's `terminate`/`destroy` teardown, so control
        // never returns to the placeholder below.
        run_gtk(&state.logging);
    }

    // Any configured runtime whose backend is NOT compiled into this build — a
    // Linux/FreeBSD `--no-default-features` build with the GTK backend off, or a
    // target that is neither `.none` (usage-banner, handled above) nor a built GUI
    // backend — remains the deferred placeholder leaf. On macOS `app_runtime()`
    // is `.none`, so this is unreachable there (R5: byte-identical).
    unimplemented!(
        "GUI launch has no compiled backend in this build (app_runtime={:?}); the \
         GTK backend lands via `--features gtk` (default on Linux/FreeBSD), the \
         headless `rustty` CLI (build_config.app_runtime == .none) runs today",
        build_config::app_runtime()
    );
}

/// PORTS: `main_ghostty.zig:96-112` — the GUI launch leaf for the NATIVE gtk4-rs
/// apprt backend. Faithfully reproduces the reference sequence:
///
/// ```zig
/// const app: *App = try App.create(alloc);   // App = CoreApp (src/App.zig)
/// defer app.destroy();
/// var app_runtime: apprt.App = undefined;
/// try app_runtime.init(app, .{});
/// defer app_runtime.terminate();
/// if (@hasDecl(apprt.App, "startQuitTimer")) app_runtime.startQuitTimer();
/// try app_runtime.run();
/// ```
///
/// `App` (reference `src/App.zig`) → [`rustty_core::app::App`] over the GTK apprt
/// [`App`](rustty_apprt::gtk::App); `app_runtime.init(app, .{})` →
/// [`gtk::App::init`](rustty_apprt::gtk::App::init) (the core-app pointer is a
/// stable heap backref via [`Box`]); `app_runtime.run()` →
/// [`gtk::App::run`](rustty_apprt::gtk::App::run) (the hand-rolled event loop that
/// ticks the core app, `class/application.zig:474-592`).
///
/// The two reference `defer`s discharge in LIFO order — `terminate` then
/// `destroy` — on every exit path, and `run`'s error propagates THROUGH them (the
/// reference `try`): both are performed explicitly before the final
/// `std::process::exit`, which — like the reference `posix.exit` / normal `main`
/// return — is the process end (`run_gtk` returns `!`).
#[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]
fn run_gtk(logging: &Logging) -> ! {
    use allocator_api2::alloc::Global;
    use rustty_apprt::gtk;
    use rustty_core::app::App as CoreApp;

    // const app: *App = try App.create(alloc);   defer app.destroy();
    //   `App.create` → `rustty_core::App::create`; the reference `alloc` param is
    //   the workspace's global allocator (`allocator_api2::alloc::Global`). The
    //   returned `Box` is the reference's stable `*App` heap pointer.
    let mut core_app: Box<CoreApp<gtk::App>> = match CoreApp::<gtk::App>::create(Global) {
        Ok(app) => app,
        // `try App.create` — a create failure (font `SharedGridSet` init) aborts
        // startup. The reference propagates it out of `main` (nonzero exit); the
        // port logs and exits 1 (no teardown is owed — nothing was constructed).
        Err(err) => {
            log_fn(
                LogLevel::Err,
                &format!("failed to initialize the app error={err:?}"),
                logging,
            );
            std::process::exit(1);
        }
    };
    // The reference passes `app` (its already-stable address) as the runtime App's
    // backref; the `Box`'s heap address is stable for the app's lifetime.
    let core_ptr: *mut CoreApp<gtk::App> = &mut *core_app;

    // var app_runtime: apprt.App = undefined; try app_runtime.init(app, .{});
    //   defer app_runtime.terminate();
    //   `gtk::App::init`'s `InitError` is uninhabited (the reference `init` is
    //   infallible in Rust — OOM aborts in the global allocator), so the `Err` arm
    //   is unreachable and matched with `match err {}`.
    let app_runtime = match gtk::App::init(core_ptr.cast()) {
        Ok(rt) => rt,
        Err(err) => match err {},
    };

    // if (@hasDecl(apprt.App, "startQuitTimer")) app_runtime.startQuitTimer();
    //   PORT NOTE: for the GTK apprt this `@hasDecl` probe is comptime-FALSE, so
    //   the reference elides the call. `startQuitTimer` is a PRIVATE method of the
    //   `Application` GObject (`class/application.zig:823`), reached only via the
    //   `.quit_timer` apprt action (`:2381`); it is NOT a public decl of the gtk
    //   `apprt.App` (`App.zig` exposes only init/run/terminate/wakeup/
    //   performAction/performIpc/redrawInspector). Hence `gtk::App` has no
    //   `start_quit_timer` and nothing is ported here. (The probe is true only for
    //   the macOS apprt, whose `App` does declare it.)

    // try app_runtime.run();
    let result = app_runtime.run();

    // The two reference `defer`s, discharged LIFO on the way out (the reference
    // `try` propagates the error THROUGH them): terminate first, then destroy.
    // defer app_runtime.terminate();
    app_runtime.terminate();
    // defer app.destroy();
    core_app.destroy();

    match result {
        // The reference `main` returns normally after `run` → exit 0.
        Ok(()) => std::process::exit(0),
        // `try app_runtime.run()` failing propagates out of `main` (nonzero exit).
        Err(err) => {
            log_fn(
                LogLevel::Err,
                &format!("the GTK event loop exited with an error={err:?}"),
                logging,
            );
            std::process::exit(1);
        }
    }
}

/// Log severity — the reference `std.log.Level` (`.debug`/`.info`/`.warn`/`.err`).
#[derive(Clone, Copy)]
enum LogLevel {
    #[allow(dead_code)] // .debug: only produced by std.log.debug (none in this file)
    Debug,
    Info,
    Warn,
    Err,
}

// PORT NOTE (WS4): `level.asText()` used to live here; `log_fn` now delegates
// to the shared `rustty_core::logging::Level::as_text` (Task 8) — this local
// copy is dead and removed.

/// The function std.log will call.
// fn logFn(
//     comptime level: std.log.Level,
//     comptime scope: @TypeOf(.EnumLiteral),
//     comptime format: []const u8,
//     args: anytype,
// ) void {
//
// PORT NOTE: the reference registers this as `std_options.logFn`, invoked by every
// `std.log.*` call. Rust has no such global log hook, so the bin calls `log_fn`
// directly at each reference `std.log.warn`/`.info`/`.err` site. All log messages
// in `main_ghostty.zig` use the `.default` scope; the `format`/`args` pair is
// pre-formatted into `message` at the call site. The reference `logging` fields are
// read from the process-global `state` — here they come from the bin's own
// `GlobalState<Action>.logging`, passed in.
fn log_fn(level: LogLevel, message: &str, logging: &Logging) {
    // WS4: the full logFn (macOS unified-logging + stderr sinks) is the shared
    // rustty_core::logging port; the bin delegates with its own state's
    // logging (ADV-FIX 2: the bin's GlobalState<Action> is separate from
    // rustty_core::STATE). All main_ghostty.zig sites use the `.default` scope.
    let core_level = match level {
        LogLevel::Debug => rustty_core::logging::Level::Debug,
        LogLevel::Info => rustty_core::logging::Level::Info,
        LogLevel::Warn => rustty_core::logging::Level::Warn,
        LogLevel::Err => rustty_core::logging::Level::Err,
    };
    rustty_core::logging::log_with(core_level, "default", message, logging);
}

// pub const std_options: std.Options = .{
//     // Our log level is always at least info in every build mode.
//     //
//     // Note, we don't lower this to debug even with conditional logging
//     // via GHOSTTY_LOG because our debug logs are very expensive to
//     // calculate and we want to make sure they're optimized out in
//     // builds.
//     .log_level = switch (builtin.mode) {
//         .Debug => .debug,
//         else => .info,
//     },
//
//     .logFn = logFn,
//
//     // If are building for a non-MacOS Darwin target (e.g., iOS), we need to
//     // disable stack tracing for the time being. This is due to the fact that
//     // Zig switched to using _dyld_get_image_header_containing_address and some
//     // other (deprecated) calls to speed up stack unwinding; these calls are
//     // available on MacOS, but not on other platforms.
//     //
//     // A fix has already been submitted to exempt non-MacOS (but still Darwin)
//     // targets, so this can likely be removed in Zig 0.17.0, or a 0.16.x patch
//     // version if it releases beforehand.
//     //
//     // More details:
//     //   https://codeberg.org/ziglang/zig/commit/89f86e46d278a35a613bbc662cdd3f65ffc76ed7
//     //
//     .allow_stack_tracing = if (builtin.target.os.tag.isDarwin() and builtin.target.os.tag != .macos)
//         false
//     else
//         !builtin.strip_debug_info,
// };
//
// PORT NOTE (`caf48a41e`, phase-6): `.allow_stack_tracing` is N/A — it is a
// Zig `std.Options` switch over Zig's own unwinder (`.port/phase-3/NON_PORTS.md`
// E36); Rust backtraces are governed by `RUST_BACKTRACE`/the panic hook and
// carry no build-time strip toggle. The range fixed the non-iOS arm from
// `builtin.strip_debug_info` (inverted: tracing allowed only in STRIPPED
// builds) to `!builtin.strip_debug_info`; the fix changes nothing here for the
// same reason the option itself did not — the reference block above is
// carried at its end state so the projection stays complete.
//
// PORT NOTE: Rust has no `std.Options` global to register. The compile-time log
// level is preserved as `LOG_LEVEL` (the `logFn` above enforces the same
// debug-only-in-Debug-build gate); `logFn` is called directly at each log site
// (see `main`). This is the faithful projection of the reference `std_options`.
#[allow(dead_code)]
const LOG_LEVEL: LogLevel = if cfg!(debug_assertions) {
    LogLevel::Debug
} else {
    LogLevel::Info
};

#[cfg(test)]
mod tests {
    // test {
    //     _ = @import("pty.zig");
    //     _ = @import("Command.zig");
    //     _ = @import("font/main.zig");
    //     _ = @import("apprt.zig");
    //     _ = @import("renderer.zig");
    //     _ = @import("termio.zig");
    //     _ = @import("input.zig");
    //     _ = @import("cli.zig");
    //     _ = @import("surface_mouse.zig");
    //
    //     // Libraries
    //     _ = @import("tripwire.zig");
    //     _ = @import("benchmark/main.zig");
    //     _ = @import("crash/main.zig");
    //     _ = @import("datastruct/main.zig");
    //     _ = @import("inspector/main.zig");
    //     _ = @import("lib/main.zig");
    //     _ = @import("terminal/main.zig");
    //     _ = @import("terminfo/main.zig");
    //     _ = @import("simd/main.zig");
    //     _ = @import("synthetic/main.zig");
    //     _ = @import("unicode/main.zig");
    //     _ = @import("unicode/props_uucode.zig");
    //     _ = @import("unicode/symbols_uucode.zig");
    //
    //     // Extra
    //     _ = @import("extra/bash.zig");
    //     _ = @import("extra/fish.zig");
    //     _ = @import("extra/sublime.zig");
    //     _ = @import("extra/vim.zig");
    //     _ = @import("extra/zsh.zig");
    // }
    //
    // PORT NOTE: the reference's anonymous `test {}` `refAllDecls`-style block
    // reifies the whole ghostty module graph so `zig build test` compiles every
    // subsystem. In Rustty those subsystems are SEPARATE crates, each with its own
    // `cargo test` (the workspace test graph), so a bin test cannot (and need not)
    // re-`_ =` them. The Rust analogue touches the `rustty` exe's own real dispatch
    // surface — the cli action dispatcher (the entrypoint the bin actually drives)
    // — which is what this file's `main` reifies. Anonymous `test {}` — skipped by
    // check_test_counts.py's zig counter (zig=0); ported for R1 completeness.
    use super::*;

    #[test]
    fn ref_all_decls() {
        // The dispatcher surface the bin drives (`state.action` detection + run).
        // (19: P4 par-3 added `+new-tab`, 034506f14.)
        assert_eq!(Action::ALL.len(), 19);
        // The `.none` / `.gtk` branch predicate `main` switches on.
        let _ = build_config::app_runtime();
        let _ = build_config::Runtime::None;
        // The log projection.
        let _ = LOG_LEVEL;
        log_fn(
            LogLevel::Info,
            "reify",
            &Logging {
                stderr: false,
                macos: false,
            },
        );
        // The init error surface `main`'s `state.init() catch` switches on.
        let _ = InitError::Detect(DetectError::MultipleActions);
        let _ = InitError::Detect(DetectError::InvalidAction);
    }
}
