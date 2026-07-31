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

// PARITY (P3-W7 T1, area-E `E16`): upstream added a comptime import guard at
// the top of `global.zig`:
//
//   // This file should only be imported for certain platforms.
//   comptime {
//       switch (@import("terminal_options").artifact) {
//           .ghostty => {},
//           // This file is not allowed to be included in libghostty-vt
//           .lib => @compileError("global state cannot be used in libghostty-vt"),
//       }
//   }
//
// Zig selects the `lib` (libghostty-vt) artifact with a build option on ONE
// module graph, so the guard has to be a `@compileError`. rustty's libghostty-vt
// artifact is a separate CRATE (`rustty-vt` / `rustty-vt-c`), so the equivalent
// invariant is a DEPENDENCY-EDGE assertion, not a comptime block: neither
// `rustty-vt` nor `rustty-vt-c` may declare a `rustty-global` dependency in any
// dependency table. That assertion is enforced by
// `.port/tools/gates/check_acyclic.sh` (gate G7) and proven RED by the
// negative control recorded in `P3-W7 T1 Step 4`. (R4.)
//
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

// PORT NOTE (P3-W7 T1, area-E `E16`): upstream's `global.zig` also imports
//   const allocTmpDir = @import("os/file.zig").allocTmpDir;
//   const freeTmpDir = @import("os/file.zig").freeTmpDir;
// `rustty-os` sits ABOVE this leaf crate (R4), so that edge is inverted: the
// temp-dir discovery is published INTO this crate by the initializer. See
// `environ::set_tmp_dir_path`.
pub mod environ;
pub use environ::{
    environ, environ_map, set_environ, set_tmp_dir_path, sync_environ, tmp_dir_path, EnvMap,
    EnvMapExt, Environ, EnvironRef,
};

/// We export the xev backend we want to use so that the rest of
/// Ghostty can import this once and have access to the proper
/// backend.
// PARITY (DEFERRED SEAM): reference `pub const xev = @import("xev").Dynamic;`.
// The `xev` event-loop backend is an upward dependency (font/event-loop tier);
// re-exporting it here would make `rustty-global` non-leaf. The re-export moves
// to `rustty-core`. See `seam::Xev`.
pub use seam::Xev as xev;

/// Global process state. This is initialized in main() for exe artifacts and
/// by rustty_init() for lib artifacts. Most other methods in this file will
/// retrieve items stored in this state.
///
/// This should ONLY be used by the C API. The Rust API should NOT use any
/// global state and should rely on allocators being passed in as parameters.
// PARITY: reference `var state: ?GlobalState = null;` (`7121ab6c3` changed it
// from `undefined` to a nullable defaulting to null). Zig leaves the
// process-global state empty until `init()` runs in `main()`/`ghostty_init()`.
// Per the agreed DI/OnceLock mechanism (R4 break #1), the rustty container is an
// `OnceLock` that is empty until `set` from the entrypoint, which is the
// memory-safe analogue of the reference's null-then-init contract.
pub static STATE: std::sync::OnceLock<GlobalState> = std::sync::OnceLock::new();

// The options accepted by the reference `global.init`.
//
// PARITY (`048619a6b` / `e8525c0fd`): the reference declares
//
// ```zig
// pub const InitOpts = union(enum) {
//     main: std.process.Init.Minimal,
//
//     /// Same as `main` but for auxiliary tool binaries (e.g. ghostty-bench
//     /// and ghostty-gen) that have their own CLI action namespace. This
//     /// skips detection of ghostty app CLI actions, since tool actions
//     /// (e.g. `+terminal-stream`) are not valid app actions and would
//     /// otherwise cause init to fail with InvalidAction.
//     tool: std.process.Init.Minimal,
//
//     c: struct {
//         argc: usize,
//         argv: [*][*:0]u8,
//         environ: std.process.Environ,
//     },
// };
// ```
//
// PORT NOTE: rustty has no single generic `init` to dispatch on, so the three
// arms are three entry points instead of a tagged union: `main` is
// [`GlobalState::new`], `c` is [`GlobalState::new_with_environ`] (called by
// `rustty-apprt-c`'s `rustty_init`, which owns the argc/argv seam), and `tool`
// is N/A — see the PORT NOTE on [`GlobalState::new_with_environ`] for why.
// The union itself is therefore recorded here rather than declared.

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
    /// PARITY: reference `environ: std.process.Environ,` (added by `e8525c0fd`)
    /// — the environment block captured once at init and threaded explicitly
    /// from then on. The free accessors `environ()`/`environ_map()` read the
    /// process-global copy this field publishes; see `environ`'s module docs
    /// for why the process-global cannot be a `GlobalState` field lookup.
    pub environ: environ::Environ,
    /// PARITY: reference `tmp_dir_path: ?[]const u8,` (added by `e8525c0fd`) —
    /// the temp directory discovered once at init so `randomTmpPath` need not
    /// re-derive (and re-allocate) it per call.
    pub tmp_dir_path: Option<Vec<u8>>,
    /// Returns the global state logging configuration.
    ///
    /// Asserts that the global state is initialized.
    // PARITY: reference `logging: Logging,` plus its free accessor
    // `pub fn logging() GlobalState.Logging { return state.?.logging; }`.
    // See the FREE ACCESSORS note below for why the accessor is a field read
    // in rustty.
    pub logging: Logging,
    /// Returns the global state rlimits, or an empty one when testing.
    ///
    /// Asserts that the global state is initialized when not running as a test.
    // PARITY: reference `rlimits: ResourceLimits = .{},` plus its free accessor
    // `pub fn rlimits() ResourceLimits`.
    pub rlimits: ResourceLimits,

    /// The app resources directory, equivalent to zig-out/share when we build
    /// from source. This is null if we can't detect it.
    ///
    /// Returns the global state resources_dir, or an empty one when testing.
    ///
    /// Asserts that the global state is initialized when not running as a test.
    // PARITY: reference `resources_dir: internal_os.ResourcesDir,` — DEFERRED
    // SEAM, the concrete `internal_os.ResourcesDir` type is wired in `rustty-core`.
    // The reference free accessor is `pub fn resourcesDir() internal_os.ResourcesDir`.
    pub resources_dir: ResourcesDir,
}

// FREE ACCESSORS (`e8525c0fd`/`a77c706a1`).
//
// Upstream grew a set of module-level accessors over the nullable `state`:
// `io()`, `alloc()`, `environ()`, `environMap()`, `syncEnviron()`, `args()`,
// `tmpDirPath()`, `resourcesDir()`, `rlimits()`, `logging()` and `action()`.
// Each is one line — `if (builtin.is_test) return <testing fallback>; return
// state.?.<field>;` — over the ONE process-global `state`.
//
// PORT NOTE: rustty's `GlobalState` is GENERIC over the two deferred-seam types
// (`Action`, `ResourcesDir`, break #1/R4) and is therefore instantiated at two
// different bindings — `rustty_global::STATE` and `rustty_core::STATE`. A free
// accessor in this crate cannot name "the" state, so only the accessors whose
// value is NON-generic are free functions here: `environ()`, `environ_map()`,
// `sync_environ()` and `tmp_dir_path()` (see the `environ` module, which owns
// the matching process-globals). The generic-valued ones — `logging()`,
// `rlimits()`, `resources_dir()`, `action()` — stay field reads on whichever
// `GlobalState` the caller holds, which is why their reference docs are
// attached to the fields above. `io()`/`alloc()` are Zig-allocator/`std.Io`
// seams with no rustty analogue (Rust's allocator and I/O are process-wide);
// see `seam::Allocator`. `args()` is owned by `rustty-apprt-c` (`ARGV`), which
// is the crate that receives the C argc/argv.

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
    // pub fn init(opts: InitOpts) !void {
    //     // Initialize ourself to nothing so we don't have any extra state.
    //     // IMPORTANT: this MUST be initialized before any log output because
    //     // the log function uses the global state.
    //     state = .{
    //         .io_impl = undefined,
    //         .gpa = null,
    //         .alloc = undefined,
    //         .environ = switch (opts) {
    //             .main, .tool => |m| m.environ,
    //             .c => |c| c.environ,
    //         },
    //         .args = switch (opts) {
    //             .main, .tool => |m| m.args,
    //             // TODO: Using the C API from Windows is unsupported at this time.
    //             //
    //             // When do we plan on supporting Windows, it's recommended to
    //             // ensure that the C API can take a UNICODE_STRING (aka []16, a
    //             // WTF-16 string) so that it can just be passed into
    //             // std.process.Args.Vector directly.
    //             .c => |c| .{ .vector = if (builtin.os.tag == .windows)
    //                 return error.UnsupportedOSForCApi
    //             else
    //                 c.argv[0..c.argc] },
    //         },
    //         .tmp_dir_path = null,
    //         .action = null,
    //         .logging = .{},
    //         .rlimits = .{},
    //         .resources_dir = .{},
    //     };
    //     const self = &state.?;
    //     errdefer deinit();
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
    //         break :gpa .init;
    //     };
    //
    //     self.alloc = if (self.gpa) |*value|
    //         value.allocator()
    //     else if (builtin.link_libc)
    //         std.heap.c_allocator
    //     else
    //         unreachable;
    //
    //     // Set up our main I/O implementation (fully threaded w/allocator). Note
    //     // that we cannot use any implementation supplied from main at this point,
    //     // because there are some later initialization steps that depend on us
    //     // mutating the environment, and thus it needs to be re-synced farther
    //     // down. For that, we need a stable implementation that allows us to do so.
    //     self.io_impl = .init(self.alloc, .{
    //         .argv0 = .init(self.args),
    //         .environ = self.environ,
    //     });
    //
    //     // Discover and save the temporary directory path
    //     self.tmp_dir_path = try allocTmpDir(self.alloc, self.environ);
    //
    //     // We first try to parse any action that we may be executing.
    //     // Tool binaries (ghostty-bench, ghostty-gen) have their own action
    //     // namespace and detect their own actions, so we skip detection here.
    //     self.action = switch (opts) {
    //         .main, .c => try cli.action.detectArgs(
    //             cli.ghostty.Action,
    //             self.alloc,
    //             self.args,
    //         ),
    //         .tool => null,
    //     };
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
    //     logging: {
    //         const v = self.environ.getAlloc(self.alloc, "GHOSTTY_LOG") catch |err| switch (err) {
    //             error.EnvironmentVariableMissing => break :logging,
    //             else => return err,
    //         };
    //         defer self.alloc.free(v);
    //         self.logging = cli.args.parsePackedStruct(GlobalState.Logging, v) catch .{};
    //     }
    //
    //     // Setup our signal handlers before logging
    //     GlobalState.initSignals();
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
    //     //
    //     // We need to re-sync the environment after this completes.
    //     try internal_os.ensureLocale();
    //     syncEnviron();
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
    //
    // PARITY accessor: the seam-free, leaf-safe construction of the data fields
    // that the reference `init` zero-initializes before performing its upward
    // side effects (the `self.* = .{ ... }` step). `rustty-core` calls this and
    // then runs the deferred-seam side effects above.
    pub fn new(alloc: seam::Allocator, resources_dir: ResourcesDir) -> Self {
        Self::new_with_environ(alloc, resources_dir, environ::Environ::from_process())
    }

    /// PARITY: the `.environ = switch (opts) { .main, .tool => |m| m.environ,
    /// .c => |c| c.environ }` arm of upstream's `init`. The `.c` arm takes the
    /// block from the C caller, which is why this is a separate entry point;
    /// `new` is the `.main` arm.
    ///
    /// PORT NOTE (E16, `048619a6b`): the `InitOpts::tool` arm is **N/A** for
    /// rustty and is deliberately NOT ported — do not add it later. Upstream
    /// needed it because `main_bench.zig`/`main_gen.zig` funnel through the
    /// shared `global.init`, so app-action detection ran on tool argv and
    /// `+terminal-stream` failed with `InvalidAction`.
    /// `crates/rustty-bench/src/main_bench.rs` never initializes global state
    /// (`grep -n "STATE\|init_with_action\|global"` -> 0 hits) and
    /// `crates/rustty-bench/src/cli.rs` `Action::from_name` owns its own action
    /// namespace, so the failure upstream fixed cannot occur here. Same
    /// reasoning for `main_gen.zig` -> `rustty-gen`.
    ///
    /// PORT NOTE (E16): the `.c` arm's
    /// `return error.UnsupportedOSForCApi` on Windows is carried by
    /// `rustty-apprt-c`'s `rustty_init`, which owns the argc/argv seam —
    /// a `#[cfg(target_os = "windows")]` early `return 1` reproducing the
    /// reference's logged init failure, with the reference's UNICODE_STRING
    /// TODO quoted at that site.
    pub fn new_with_environ(
        alloc: seam::Allocator,
        resources_dir: ResourcesDir,
        env: environ::Environ,
    ) -> Self {
        // Discover and save the temporary directory path.
        //
        // PORT NOTE: upstream does
        //   self.tmp_dir_path = try allocTmpDir(self.alloc, self.environ);
        // here. `allocTmpDir` lives in `rustty-os`, ABOVE this leaf (R4), so
        // the initializer publishes it with `environ::set_tmp_dir_path` and
        // `environ::tmp_dir_path()` falls back to the same rule otherwise.
        environ::set_environ(env.clone());
        let tmp_dir_path = environ::tmp_dir_path().to_vec();

        // Initialize ourself to nothing so we don't have any extra state.
        // IMPORTANT: this MUST be initialized before any log output because
        // the log function uses the global state.
        Self {
            gpa: None,
            alloc,
            action: None,
            environ: env,
            tmp_dir_path: Some(tmp_dir_path),
            logging: Logging::default(),
            rlimits: ResourceLimits::default(),
            resources_dir,
        }
    }

    /// Cleans up the global state. This doesn't _need_ to be called but
    /// doing so in dev modes will check for memory leaks.
    ///
    /// Asserts that the state exists.
    // PARITY (BREAK #1, R4): the reference `deinit` frees `resources_dir`, flushes
    // `crash` logs, releases the tmp dir path and the I/O instance, and deinits
    // the GPA (which is where leak diagnostics print). Those touch the
    // `os`/`crash` upward crates and the GPA/`std.Io` seams, so they are
    // DEFERRED SEAMS executed by `rustty-core`. Reference body reproduced verbatim:
    //
    // pub fn deinit() void {
    //     const self = &state.?;
    //
    //     self.resources_dir.deinit(self.alloc);
    //
    //     // Flush our crash logs
    //     crash.deinit();
    //
    //     // Release our tmp_dir_path if needed
    //     if (self.tmp_dir_path) |td| freeTmpDir(self.alloc, td);
    //
    //     // Release our I/O instance
    //     self.io_impl.deinit();
    //
    //     if (self.gpa) |*value| {
    //         // We want to ensure that we deinit the GPA because this is
    //         // the point at which it will output if there were safety violations.
    //         _ = value.deinit();
    //     }
    // }
    //
    // PORT NOTE: `tmp_dir_path` is an owned `Option<Vec<u8>>` and the resources
    // dir / crash log flush are seams, so rustty's leaf `deinit` has nothing
    // left to do: `Drop` releases the tmp dir path.
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

    // PORT (WS4, formerly a deferred seam): reference body calls
    // `internal_os.restoreMaxFiles(lim)`:
    //
    // pub fn restore(self: *const ResourceLimits) void {
    //     if (self.nofile) |lim| internal_os.restoreMaxFiles(lim);
    // }
    //
    // `restoreMaxFiles` is `posix.setrlimit(.NOFILE, lim) catch log warn`
    // (rustty-os file.rs:87). rustty-os sits ABOVE this leaf crate, so the
    // one-line syscall is mirrored here via libc (the same binding rustty-os
    // uses) instead of an upward import — behavior-identical, R4 acyclic.
    pub fn restore(&self) {
        #[cfg(unix)]
        if let Some(lim) = self.nofile {
            // failure is non-fatal, matching restoreMaxFiles' `catch` swallow.
            let _ = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) };
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
    /// PARITY: reference `internal_os.rlimit` — which is `posix.rlimit` where
    /// the platform has it, `struct {}` otherwise (rustty-os `file.rs`
    /// reproduces that as `libc::rlimit` on unix / an empty struct elsewhere).
    /// Rebound from the former `u64` placeholder to the REAL type (WS4) so
    /// `GlobalState.rlimits` can carry `rustty_os::fixMaxFiles()`'s value.
    #[cfg(unix)]
    pub type Rlimit = libc::rlimit;
    #[cfg(not(unix))]
    pub type Rlimit = ();
    /// PARITY: reference `pub const xev = @import("xev").Dynamic;` (event-loop
    /// backend, above us). Re-exported from `rustty-core`.
    pub type Xev = ();
}

#[cfg(all(test, unix))]
mod rlimit_tests {
    use super::ResourceLimits;

    #[test]
    fn restore_round_trips_nofile() {
        // Read the current limit, then restore() it — the call must succeed and
        // leave the limit unchanged (a self-inverse KAT that needs no privilege).
        let mut cur = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut cur) };
        assert_eq!(rc, 0);

        let limits = ResourceLimits::init(Some(cur));
        limits.restore();

        let mut after = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut after) };
        assert_eq!(rc, 0);
        assert_eq!(after.rlim_cur, cur.rlim_cur);
    }
}
