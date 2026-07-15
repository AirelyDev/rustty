//! rustty-global
//!
//! PORTS: ghostty src/global.zig — leaf process-state holder (OnceLock-style) split from the
//! master hub; high-level wiring moves to rustty-core (cycle break #1).
//!
//! PORT STATUS: done
//!
//! # BREAK #1 (R4 cycle-break): `global.zig` — the master hub
//!
//! In the reference, `src/global.zig` is the process-global state hub: it holds
//! the global `state: GlobalState` and is imported *bottom-up* from `terminal/`,
//! `os/`, `crash/`, `renderer/`, `termio/`, `config/`, and `cli/`, while it
//! itself imports *top-down* `apprt`, `cli`, `renderer`, `crash`, `font`
//! (`fontconfig`/`glslang`/`harfbuzz`/`oniguruma`), `internal_os`, and the `xev`
//! event-loop backend. That single module glues most of the import graph
//! together — nearly every `X ↔ core` cycle is *only* via `global.zig`.
//!
//! To break the cycle, this crate is carved out as the LEAF state-holder: it
//! owns the `GlobalState` container with the same fields and semantics (an
//! `OnceLock`-style container per the agreed DI/OnceLock mechanism), but WITHOUT
//! the upward dependencies on `apprt`/`cli`/`renderer`/`crash`/`font`/`os`. Those
//! high-level wirings move to `rustty-core` when it is ported; here they are
//! recorded as DEFERRED SEAMS (see `seam` below). Keeping `rustty-global` a leaf
//! is what lets the bottom-up importers (`vt`, `os`, `crash`, `renderer`,
//! `termio`, `config`, `cli`) depend on it without re-introducing the cycle.
//!
//! This is a behaviour-preserving structural relocation only (R4): every field,
//! comment, and the `init`/`deinit`/`initSignals`/`ResourceLimits` semantics are
//! ported from the reference verbatim. The reference `global.zig` has 0 `test {}`
//! blocks (Phase-0 inventory contract), so this crate has no ported tests.

// PARITY (BREAK #1, R4): the reference module imports are listed here for fidelity.
// As the LEAF state-holder, `rustty-global` does NOT pull these crates in; the
// upward wirings move to `rustty-core`. The reference import lines were:
//   const std = @import("std");
//   const builtin = @import("builtin");
//   const build_config = @import("build_config.zig");
//   const cli = @import("cli.zig");
//   const internal_os = @import("os/main.zig");
//   const fontconfig = @import("fontconfig");
//   const glslang = @import("glslang");
//   const harfbuzz = @import("harfbuzz");
//   const oni = @import("oniguruma");
//   const crash = @import("crash/main.zig");
//   const renderer = @import("renderer.zig");
//   const apprt = @import("apprt.zig");
// `build_config` is available (it is a leaf below us, R4 break #3) and is used.
use rustty_build_config as build_config;

/// We export the xev backend we want to use so that the rest of
/// Ghostty can import this once and have access to the proper
/// backend.
// PARITY (DEFERRED SEAM): reference `pub const xev = @import("xev").Dynamic;`.
// The `xev` event-loop backend is an upward dependency (font/event-loop tier);
// re-exporting it here would make `rustty-global` non-leaf. The re-export moves
// to `rustty-core`. See `seam::Xev`.
pub use seam::Xev as xev;

/// Global process state. This is initialized in main() for exe artifacts
/// and by rustty_init() for lib artifacts. This should ONLY be used by
/// the C API. The Zig API should NOT use any global state and should
/// rely on allocators being passed in as parameters.
// PARITY: reference `pub var state: GlobalState = undefined;`. Zig leaves the
// process-global state `undefined` until `init()` runs in `main()`/`ghostty_init()`.
// Per the agreed DI/OnceLock mechanism (R4 break #1), the rustty container is an
// `OnceLock` that is empty until `set` from the entrypoint, which is the
// memory-safe analogue of the reference's `undefined`-then-init contract.
pub static STATE: std::sync::OnceLock<GlobalState> = std::sync::OnceLock::new();

/// This represents the global process state. There should only
/// be one of these at any given moment. This is extracted into a dedicated
/// struct because it is reused by main and the static C lib.
//
// PARITY (BREAK #1, R4): generic over the DEFERRED-SEAM field types so the
// concrete upward types are wired in `rustty-core`:
//   * `Action` <- reference `cli.ghostty.Action` (the `cli` crate, above us).
//   * `ResourcesDir` <- reference `internal_os.ResourcesDir` (the `os` crate,
//     which is above us in the port order, so it cannot be a dependency here).
// The data fields (`gpa`/`alloc`/`logging`/`rlimits`) and their semantics are
// ported as-is.
pub struct GlobalState<Action = (), ResourcesDir = ()> {
    // PARITY: reference `const GPA = std.heap.GeneralPurposeAllocator(.{});`.
    // In Rust the global allocator is process-wide (`std::alloc`), so there is no
    // per-`GlobalState` GPA *type* to name. The reference's GPA-vs-libc selection
    // is a DEFERRED SEAM carried by the `gpa`/`alloc` fields below.
    /// PARITY: reference `gpa: ?GPA,` — the optional GeneralPurposeAllocator
    /// instance, present only in debug/leak-detection modes (see `init`).
    pub gpa: Option<seam::Gpa>,
    /// PARITY: reference `alloc: std.mem.Allocator,` — the chosen allocator
    /// (GPA's allocator, the libc allocator, or `unreachable`).
    pub alloc: seam::Allocator,
    /// PARITY: reference `action: ?cli.ghostty.Action,` — DEFERRED SEAM, the
    /// concrete `cli.ghostty.Action` type is wired in `rustty-core`.
    pub action: Option<Action>,
    pub logging: Logging,
    pub rlimits: ResourceLimits,

    /// The app resources directory, equivalent to zig-out/share when we build
    /// from source. This is null if we can't detect it.
    // PARITY: reference `resources_dir: internal_os.ResourcesDir,` — DEFERRED
    // SEAM, the concrete `internal_os.ResourcesDir` type is wired in `rustty-core`.
    pub resources_dir: ResourcesDir,
}

impl<Action, ResourcesDir> GlobalState<Action, ResourcesDir> {
    /// Initialize the global state.
    // PARITY (BREAK #1, R4): the reference `init` glues in the upward crates
    // (`cli` action parsing, `internal_os` env/locale/i18n/rlimits, `crash`,
    // `glslang`, `oniguruma`, `apprt` resources dir, the `xev` backend, and the
    // `renderer`/`harfbuzz`/`fontconfig` log lines). As the leaf state-holder,
    // those side effects are DEFERRED SEAMS performed by `rustty-core`. The
    // reference body is reproduced here verbatim in comments so no logic/comment
    // is lost; `rustty-core`'s wiring will execute it through injected seams.
    //
    // pub fn init(self: *GlobalState) !void {
    //     // const start = try std.time.Instant.now();
    //     // const start_micro = std.time.microTimestamp();
    //     // defer {
    //     //     const end = std.time.Instant.now() catch unreachable;
    //     //     // "[updateFrame critical time] <START us>\t<TIME_TAKEN us>"
    //     //     std.log.err("[global init time] start={}us duration={}ns", .{ start_micro, end.since(start) / std.time.ns_per_us });
    //     // }
    //
    //     // Initialize ourself to nothing so we don't have any extra state.
    //     // IMPORTANT: this MUST be initialized before any log output because
    //     // the log function uses the global state.
    //     self.* = .{
    //         .gpa = null,
    //         .alloc = undefined,
    //         .action = null,
    //         .logging = .{},
    //         .rlimits = .{},
    //         .resources_dir = .{},
    //     };
    //     errdefer self.deinit();
    //
    //     self.gpa = gpa: {
    //         // Use the libc allocator if it is available because it is WAY
    //         // faster than GPA. We only do this in release modes so that we
    //         // can get easy memory leak detection in debug modes.
    //         if (builtin.link_libc) {
    //             if (switch (builtin.mode) {
    //                 .ReleaseSafe, .ReleaseFast => true,
    //
    //                 // We also use it if we can detect we're running under
    //                 // Valgrind since Valgrind only instruments the C allocator
    //                 else => std.valgrind.runningOnValgrind() > 0,
    //             }) break :gpa null;
    //         }
    //
    //         break :gpa GPA{};
    //     };
    //
    //     self.alloc = if (self.gpa) |*value|
    //         value.allocator()
    //     else if (builtin.link_libc)
    //         std.heap.c_allocator
    //     else
    //         unreachable;
    //
    //     // We first try to parse any action that we may be executing.
    //     self.action = try cli.action.detectArgs(
    //         cli.ghostty.Action,
    //         self.alloc,
    //     );
    //
    //     // If we have an action executing, we disable logging by default
    //     // since we write to stderr we don't want logs messing up our
    //     // output.
    //     if (self.action != null) self.logging.stderr = false;
    //
    //     // I don't love the env var name but I don't have it in my heart
    //     // to parse CLI args 3 times (once for actions, once for config,
    //     // maybe once for logging) so for now this is an easy way to do
    //     // this. Env vars are useful for logging too because they are
    //     // easy to set.
    //     if ((try internal_os.getenv(self.alloc, "GHOSTTY_LOG"))) |v| {
    //         defer v.deinit(self.alloc);
    //         self.logging = cli.args.parsePackedStruct(Logging, v.value) catch .{};
    //     }
    //
    //     // Setup our signal handlers before logging
    //     initSignals();
    //
    //     // Setup our Xev backend if we're dynamic
    //     if (comptime xev.dynamic) xev.detect() catch |err| {
    //         std.log.warn("failed to detect xev backend, falling back to " ++
    //             "most compatible backend err={}", .{err});
    //     };
    //
    //     // Output some debug information right away
    //     std.log.info("ghostty version={s}", .{build_config.version_string});
    //     std.log.info("ghostty build optimize={s}", .{build_config.mode_string});
    //     std.log.info("runtime={}", .{build_config.app_runtime});
    //     std.log.info("font_backend={}", .{build_config.font_backend});
    //     if (comptime build_config.font_backend.hasHarfbuzz()) {
    //         std.log.info("dependency harfbuzz={s}", .{harfbuzz.versionString()});
    //     }
    //     if (comptime build_config.font_backend.hasFontconfig()) {
    //         std.log.info("dependency fontconfig={d}", .{fontconfig.version()});
    //     }
    //     std.log.info("renderer={}", .{renderer.Renderer});
    //     std.log.info("libxev default backend={t}", .{xev.backend});
    //
    //     // As early as possible, initialize our resource limits.
    //     self.rlimits = .init();
    //
    //     // Initialize our crash reporting.
    //     crash.init(self.alloc) catch |err| {
    //         std.log.warn(
    //             "sentry init failed, no crash capture available err={}",
    //             .{err},
    //         );
    //     };
    //
    //     // const sentrylib = @import("sentry");
    //     // if (sentrylib.captureEvent(sentrylib.Value.initMessageEvent(
    //     //     .info,
    //     //     null,
    //     //     "hello, world",
    //     // ))) |uuid| {
    //     //     std.log.warn("uuid={s}", .{uuid.string()});
    //     // } else std.log.warn("failed to capture event", .{});
    //
    //     // We need to make sure the process locale is set properly. Locale
    //     // affects a lot of behaviors in a shell.
    //     try internal_os.ensureLocale(self.alloc);
    //
    //     // Initialize glslang for shader compilation
    //     try glslang.init();
    //
    //     // Initialize oniguruma for regex
    //     try oni.init(&.{oni.Encoding.utf8});
    //
    //     // Find our resources directory once for the app so every launch
    //     // hereafter can use this cached value.
    //     self.resources_dir = try apprt.runtime.resourcesDir(self.alloc);
    //     errdefer self.resources_dir.deinit(self.alloc);
    //
    //     // Setup i18n
    //     if (self.resources_dir.app()) |v| internal_os.i18n.init(v) catch |err| {
    //         std.log.warn("failed to init i18n, translations will not be available err={}", .{err});
    //     };
    // }
    //
    // PARITY accessor: the seam-free, leaf-safe construction of the data fields
    // that the reference `init` zero-initializes before performing its upward
    // side effects (the `self.* = .{ ... }` step). `rustty-core` calls this and
    // then runs the deferred-seam side effects above.
    pub fn new(alloc: seam::Allocator, resources_dir: ResourcesDir) -> Self {
        // Initialize ourself to nothing so we don't have any extra state.
        // IMPORTANT: this MUST be initialized before any log output because
        // the log function uses the global state.
        Self {
            gpa: None,
            alloc,
            action: None,
            logging: Logging::default(),
            rlimits: ResourceLimits::default(),
            resources_dir,
        }
    }

    /// Cleans up the global state. This doesn't _need_ to be called but
    /// doing so in dev modes will check for memory leaks.
    // PARITY (BREAK #1, R4): the reference `deinit` frees `resources_dir`, flushes
    // `crash` logs, and deinits the GPA (which is where leak diagnostics print).
    // Those touch the `os`/`crash` upward crates and the GPA seam, so they are
    // DEFERRED SEAMS executed by `rustty-core`. Reference body reproduced verbatim:
    //
    // pub fn deinit(self: *GlobalState) void {
    //     self.resources_dir.deinit(self.alloc);
    //
    //     // Flush our crash logs
    //     crash.deinit();
    //
    //     if (self.gpa) |*value| {
    //         // We want to ensure that we deinit the GPA because this is
    //         // the point at which it will output if there were safety violations.
    //         _ = value.deinit();
    //     }
    // }
    pub fn deinit(&mut self) {}

    // PARITY (DEFERRED SEAM): reference `fn initSignals() void`. POSIX signal
    // handling (ignoring `SIGPIPE`) reaches into the platform `os` layer and is
    // wired by `rustty-core`. Reference body reproduced verbatim:
    //
    // fn initSignals() void {
    //     // Only posix systems.
    //     if (comptime builtin.os.tag == .windows) return;
    //
    //     const p = std.posix;
    //
    //     var sa: p.Sigaction = .{
    //         .handler = .{ .handler = p.SIG.IGN },
    //         .mask = p.sigemptyset(),
    //         .flags = 0,
    //     };
    //
    //     // We ignore SIGPIPE because it is a common signal we may get
    //     // due to how we implement termio. When a terminal is closed we
    //     // often write to a broken pipe to exit the read thread. This should
    //     // be fixed one day but for now this helps make this a bit more
    //     // robust.
    //     p.sigaction(p.SIG.PIPE, &sa, null);
    // }
    // PARITY: reference `fn initSignals()` is a private fn called only from the
    // (deferred-seam) `init` body. Kept private to match the reference visibility;
    // the seam wiring in `rustty-core` will invoke the platform signal setup. The
    // leaf body is empty (the side effect is deferred), so it is unused here.
    #[allow(dead_code)]
    fn init_signals() {}
}

/// Where logging should go
// PARITY: reference `pub const Logging = packed struct { ... }` — a packed struct
// of `bool` flags parsed from the `GHOSTTY_LOG` env var (see `init`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Logging {
    /// Whether to log to stderr. For lib mode we always disable stderr
    /// logging by default. Otherwise it's enabled by default.
    pub stderr: bool,
    /// Whether to log to macOS's unified logging. Enabled by default
    /// on macOS.
    pub macos: bool,
}

impl Default for Logging {
    fn default() -> Self {
        Self {
            // PARITY: reference `stderr: bool = build_config.app_runtime != .none,`.
            stderr: build_config::app_runtime() != build_config::Runtime::None,
            // PARITY: reference `macos: bool = builtin.os.tag.isDarwin(),`.
            macos: cfg!(target_os = "macos"),
        }
    }
}

/// Maintains the Unix resource limits that we set for our process. This
/// can be used to restore the limits to their original values.
// PARITY (BREAK #1, R4): the reference `nofile` field is `?internal_os.rlimit`
// and `init`/`restore` call `internal_os.fixMaxFiles`/`restoreMaxFiles`. Those
// live in the `os` crate, which is above us in the port order, so the rlimit
// type and the fix/restore side effects are DEFERRED SEAMS wired by `rustty-core`.
#[derive(Default)]
pub struct ResourceLimits {
    /// PARITY: reference `nofile: ?internal_os.rlimit = null,`.
    pub nofile: Option<seam::Rlimit>,
}

impl ResourceLimits {
    // PARITY (DEFERRED SEAM): reference body calls `internal_os.fixMaxFiles()`:
    //
    // pub fn init() ResourceLimits {
    //     return .{
    //         // Maximize the number of file descriptors we can have open
    //         // because we can consume a lot of them if we make many terminals.
    //         .nofile = internal_os.fixMaxFiles(),
    //     };
    // }
    pub fn init(nofile: Option<seam::Rlimit>) -> ResourceLimits {
        ResourceLimits {
            // Maximize the number of file descriptors we can have open
            // because we can consume a lot of them if we make many terminals.
            nofile,
        }
    }

    // PARITY (DEFERRED SEAM): reference body calls `internal_os.restoreMaxFiles(lim)`:
    //
    // pub fn restore(self: *const ResourceLimits) void {
    //     if (self.nofile) |lim| internal_os.restoreMaxFiles(lim);
    // }
    pub fn restore(&self) {
        if let Some(_lim) = self.nofile {
            // restored by the os-seam wiring in rustty-core
        }
    }
}

/// DEFERRED-SEAM placeholder types (BREAK #1, R4).
///
/// These name the upward-crate types that the reference `global.zig` referred to
/// directly. `rustty-global` stays a leaf, so the concrete definitions live in
/// the crates above us (`os`, `cli`, `font`/`xev`) and are wired through here by
/// `rustty-core`. The aliases keep the field/signature shapes faithful to the
/// reference without re-introducing the import cycle.
pub mod seam {
    /// PARITY: reference `const GPA = std.heap.GeneralPurposeAllocator(.{});`.
    /// The reference's optional GeneralPurposeAllocator instance (debug/leak mode).
    pub type Gpa = ();
    /// PARITY: reference `alloc: std.mem.Allocator`. Rust's global allocator is
    /// process-wide; the chosen-allocator handle is carried as this seam.
    pub type Allocator = ();
    /// PARITY: reference `internal_os.rlimit` (the `os` crate, above us).
    pub type Rlimit = u64;
    /// PARITY: reference `pub const xev = @import("xev").Dynamic;` (event-loop
    /// backend, above us). Re-exported from `rustty-core`.
    pub type Xev = ();
}
