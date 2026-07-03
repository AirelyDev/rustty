//! The main entrypoint for the `rustty` application. This also serves
//! as the process initialization code for the `librustty` library.
//!
//! PORTS: ghostty src/main_ghostty.zig — the `main()` dispatch (action → GUI),
//! `logFn`, and `std_options`. Byte-faithful (R1); the `Ghostty`/`libghostty`
//! product-identity strings are retargeted to the rustty product name (R2 —
//! routed through rustty-identity).
//!
//! # ADV-FIX 2 — action DETECTION lives in the BIN
//!
//! The reference `main()` calls `state.init()` (global.zig), which detects the
//! action via `cli.action.detectArgs(cli.ghostty.Action, alloc)` and stores it in
//! the process-global `GlobalState.action`. In rustty that detection CANNOT live
//! in `rustty_apprt::init` (apprt sits below cli in the port order, so calling cli
//! there cycles `apprt→cli`), and `rustty_core::STATE` is bound `Action=()` and
//! cannot hold a cli action. So the `rustty` BIN — which legitimately deps cli —
//! runs the detection itself over `os::args` and constructs its OWN
//! `rustty_global::GlobalState<cli::ghostty::Action>` (NOT core's `Action=()`
//! STATE). `rustty_apprt::init` stays cli-free.

// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const posix = std.posix;
// const build_config = @import("build_config.zig");
use rustty_build_config as build_config;
// const macos = @import("macos");           -> logFn unified-logging (deferred seam)
// const cli = @import("cli.zig");
use rustty_cli::action::{self, DetectError};
use rustty_cli::ghostty::Action;
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
/// ([`DetectError`]) and resources-dir resolution ([`rustty_apprt::InitError`],
/// the reference `apprt.runtime.resourcesDir`). The remaining reference init
/// failures (GHOSTTY_LOG parse, locale, glslang, oniguruma, i18n) are deferred
/// seams (see `state_init`).
enum InitError {
    // error.MultipleActions / error.InvalidAction (cli.action.detectArgs).
    Detect(DetectError),
    // The `try apprt.runtime.resourcesDir(self.alloc)` failure — surfaced by the
    // reference `else =>` arm ("invalid CLI invocation err={}").
    ResourcesDir(rustty_apprt::InitError),
}

/// Initialize our global state (the bin's own `GlobalState<Action>`).
///
/// PORTS: the slice of `global.zig` `GlobalState.init` that is LIVE for the
/// headless bin — action detection (ADV-FIX 2), the action-implies-no-stderr-log
/// rule, and resources-dir resolution. The rest of the reference `init` body is a
/// BREAK #1 deferred seam preserved verbatim in `rustty_global::GlobalState::init`
/// (GPA/allocator selection, GHOSTTY_LOG parse, signal setup, xev detect, the
/// version/optimize/runtime/font/renderer/libxev log lines, rlimit init,
/// crash.init, ensureLocale, glslang.init, oni.init, i18n.init) — those land as
/// rustty-core / apprt wiring (Phase-3). Reference body for the live slice:
///
///   // We first try to parse any action that we may be executing.
///   self.action = try cli.action.detectArgs(cli.ghostty.Action, self.alloc);
///   // If we have an action executing, we disable logging by default ...
///   if (self.action != null) self.logging.stderr = false;
///   ...
///   // Find our resources directory once ...
///   self.resources_dir = try apprt.runtime.resourcesDir(self.alloc);
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

    // If we have an action executing, we disable logging by default
    // since we write to stderr we don't want logs messing up our
    // output.
    // if (self.action != null) self.logging.stderr = false;
    if state.action.is_some() {
        state.logging.stderr = false;
    }

    // Find our resources directory once for the app so every launch
    // hereafter can use this cached value.
    // self.resources_dir = try apprt.runtime.resourcesDir(self.alloc);
    //   PORT NOTE: `rustty_apprt::init` performs exactly this one upward call and
    //   sets the process-global `rustty_core::STATE.resources_dir` (the seam that
    //   list_themes' `.resources` warning path reads — DEFERRALS.md "cli C5"). The
    //   bin's own `GlobalState<Action>` carries the action; core STATE carries the
    //   resources dir. Both are populated here (the "C6 bin wiring" the C5 note
    //   scheduled). i18n.init off the resolved dir is a deferred seam.
    rustty_apprt::init().map_err(InitError::ResourcesDir)?;

    Ok(state)
}

// pub fn main() !MainReturn {
pub fn main() {
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
                //   R2: "ghostty" -> the rustty product name (rustty-identity APP_NAME).
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
                InitError::ResourcesDir(e) => {
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
    //   R2: "developing Ghostty" -> "developing rustty".
    if cfg!(debug_assertions) {
        log_fn(
            LogLevel::Warn,
            "This is a debug build. Performance will be very poor.",
            &state.logging,
        );
        log_fn(
            LogLevel::Warn,
            "You should only use a debug build for developing rustty.",
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
        //   R2: "ghostty" / "Ghostty" / "Ghostty.app" -> the rustty product name.
        let app = rustty_identity::APP_NAME;
        let banner = format!(
            "Usage: {app} +<action> [flags]\n\n\
             This is the {app} helper CLI that accompanies the graphical {app} app.\n\
             To launch the terminal directly, please launch the graphical app\n\
             (i.e. {app}.app on macOS). This CLI can be used to perform various\n\
             actions such as inspecting the version, listing fonts, etc.\n\
             \n\
             On macOS, the terminal can also be launched using `open -na {app}.app`,\n\
             or `open -na {app}.app --args --foo=bar --baz=qux` to pass arguments.\n\
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
    // PORT NOTE (Phase-3 GUI leaf): the windowed app path creates `rustty_core::App`
    // and drives the concrete apprt backend (embedded/GTK) event loop over xev (D6).
    // The reference reaches here only for a GUI build (`app_runtime != .none`); the
    // headless `.none` bin never does. This is the same "deferred placeholder leaf"
    // discipline the plan uses for GPU/GTK/xev — the live product runs headless
    // today (see the action + usage-banner paths above).
    unimplemented!(
        "GUI launch (App.create + apprt.App.init/run over xev) is the Phase-3 leaf; \
         the headless `rustty` CLI (build_config.app_runtime == .none) runs today"
    );
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

impl LogLevel {
    // `level.asText()` — the reference severity text (`.warn` -> "warning",
    // `.err` -> "error").
    fn as_text(self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warning",
            LogLevel::Err => "error",
        }
    }
}

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
    // On Mac, we use unified logging. To view this:
    //
    //   sudo log stream --level debug --predicate 'subsystem=="io.airely.rustty"'
    //     R2: the reference predicate names the bundle id `com.mitchellh.ghostty`.
    //
    // macOS logging is thread safe so no need for locks/mutexes
    // macos: {
    //     if (comptime !builtin.target.os.tag.isDarwin()) break :macos;
    //     if (!state.logging.macos) break :macos;
    //     const prefix = if (scope == .default) "" else @tagName(scope) ++ ": ";
    //     // Convert our levels to Mac levels
    //     const mac_level: macos.os.LogType = switch (level) {
    //         .debug => .debug, .info => .info, .warn => .err, .err => .fault,
    //     };
    //     // Initialize a logger. This is slow to do on every operation
    //     // but we shouldn't be logging too much.
    //     const logger = macos.os.Log.create(build_config.bundle_id, @tagName(scope));
    //     defer logger.release();
    //     logger.log(std.heap.c_allocator, mac_level, prefix ++ format, args);
    // }
    //
    // PORT NOTE (deferred seam): macOS unified logging (`macos.os.Log`) is the same
    // log-facade seam the rest of the tree defers (rustty-os stubs `std.log.scoped`).

    // stderr: {
    //     // don't log debug messages to stderr unless we are a debug build
    //     if (comptime builtin.mode != .Debug and level == .debug) break :stderr;
    if !cfg!(debug_assertions) && matches!(level, LogLevel::Debug) {
        return;
    }
    //     // skip if we are not logging to stderr
    //     if (!state.logging.stderr) break :stderr;
    if !logging.stderr {
        return;
    }
    //     // Lock so we are thread-safe
    //     var buf: [64]u8 = undefined;
    //     const stderr = std.debug.lockStderrWriter(&buf);
    //     defer std.debug.unlockStderrWriter();
    //     const level_txt = comptime level.asText();
    //     const prefix = if (scope == .default) ": " else "(" ++ @tagName(scope) ++ "): ";
    //     nosuspend stderr.print(level_txt ++ prefix ++ format ++ "\n", args) catch break :stderr;
    //     nosuspend stderr.flush() catch break :stderr;
    // }
    //   All `main_ghostty.zig` log sites use the `.default` scope -> the ": " prefix.
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{}: {message}", level.as_text());
    let _ = stderr.flush();
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
//     .logFn = logFn,
// };
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
    // subsystem. In rustty those subsystems are SEPARATE crates, each with its own
    // `cargo test` (the workspace test graph), so a bin test cannot (and need not)
    // re-`_ =` them. The Rust analogue touches the `rustty` exe's own real dispatch
    // surface — the cli action dispatcher (the entrypoint the bin actually drives)
    // — which is what this file's `main` reifies. Anonymous `test {}` — skipped by
    // check_test_counts.py's zig counter (zig=0); ported for R1 completeness.
    use super::*;

    #[test]
    fn ref_all_decls() {
        // The dispatcher surface the bin drives (`state.action` detection + run).
        assert_eq!(Action::ALL.len(), 18);
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
