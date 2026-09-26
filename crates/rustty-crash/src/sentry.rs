//! rustty-crash::sentry
//!
//! PORTS: ghostty src/crash/sentry.zig — the process-wide Sentry client
//! (init/deinit + the beforeSend crash-metadata callback + the local-disk
//! Transport). 0 reference tests.
//!
//! PORT STATUS: done
//!
//! PORT NOTE (Zig laziness vs Rust eagerness — the `-Dsentry=false` build):
//! line 7 of the reference is `const sentry = if (build_options.sentry)
//! @import("sentry");`, and its two entry points open with `if (comptime
//! !build_options.sentry) return;` (`init`:60, `deinit`:213). With the option
//! false, Zig never SEMANTICALLY ANALYSES the sentry-native binding or anything
//! that touches it, and `SharedDeps.zig:407` does not link the library at all.
//! Rust type-checks every module it compiles, so `#[cfg(feature = "sentry")]`
//! stands in for that lazy analysis: the FFI-reaching declarations below —
//! `initThread`/`cacheDir`, `beforeSend`, the `setTag` tag-name helpers, the
//! disk `Transport`, and the tails of `init`/`deinit` past the reference's own
//! early return — are exactly the set Zig skips. Everything the tier ABOVE
//! crash names (`ThreadState`, `ThreadStateSurface`, `Type`,
//! `setThreadState`/`clearThreadState`, `init`, `deinit`) stays present and
//! compiled in BOTH configurations, because the reference's own callers
//! (src/Surface.zig, renderer/Thread.zig, src/App.zig) are analysed in both.
//! This is the same substitution, with the same argument, that
//! `rustty-font-shaper`'s `shaper/harfbuzz.rs` gate carries off macOS.
//!
//! SEAM (integration-tier blueprint): the reference's `ThreadState.surface`
//! is a concrete `*Surface` (src/Surface.zig, the core tier ABOVE crash);
//! `beforeSend`'s surface-metadata block is seamed via the local
//! [`ThreadStateSurface`] trait below — rustty-core's Surface implements it
//! later (DEFERRALS.md). Everything else ports as-is.
#![allow(non_snake_case)]

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const builtin = @import("builtin");
// const build_config = @import("../build_config.zig");
// (Only the `setTag` block of `initThread` and its tag-name helpers read this,
// so it carries the same gate they do — see the module header.)
#[cfg(feature = "sentry")]
use rustty_build_config as build_config;
// const build_options = @import("build_options");
use crate::build_options;
// const sentry = if (build_options.sentry) @import("sentry");
// (PORT NOTE: the Zig binding package `pkg/sentry` is hosted in-crate as
// `sentry_pkg`. The reference's `if (build_options.sentry)` is reproduced, NOT
// collapsed: the cargo feature gates the `sentry-sys` dependency itself — the
// port of `SharedDeps.zig:407` — so with it off there is no binding module to
// import, precisely as Zig has no `sentry` decl to analyse. Every reaching path
// is ALSO behind the same `build_options.sentry` runtime-const checks the
// reference uses, which is what keeps the two halves in step.)
#[cfg(feature = "sentry")]
use crate::sentry_pkg as sentry;
// const internal_os = @import("../os/main.zig");
// (Reached only by `initThread`/`cacheDir` — same gate; see the module header.)
#[cfg(feature = "sentry")]
use rustty_os as internal_os;
// const crash = @import("main.zig");
// (Reached only by `initThread`'s `defaultDir` and the `Transport`'s envelope
// parse — same gate; see the module header.)
#[cfg(feature = "sentry")]
use crate as crash;
// PORT NOTE (P4-E33, `15c50c1db` "crash: do not use global state"): upstream
// removed the `global = @import("../global.zig")` import entirely — this file
// performs NO live global reads anymore. The `rustty_global::EnvMap` mentions
// below are the TYPE of the caller-supplied environment snapshot only (the
// Rust image of `std.process.Environ.Map`, hosted by the leaf state crate).
// const Surface = @import("../Surface.zig");
// (PORT NOTE: seamed via `ThreadStateSurface` — see the module doc above. crash
// must also NOT dep rustty-renderer for the size types: renderer already deps
// crash, so the reverse edge would cycle — hence plain `(u32, u32)` tuples.)

// const log = std.log.scoped(.sentry);
//
// PORT NOTE: the reference's scope tag, carried through the `log` facade as
// `target: LOG_SCOPE` (DECISION-logging.md §6r4).
// (All five emissions live in the gated region — same gate; see the module
// header. The `include_str!` test below reads the SOURCE, not the const, so it
// still holds in both configurations.)
#[cfg(feature = "sentry")]
const LOG_SCOPE: &str = "sentry";

/// The global state for the Sentry SDK. This is unavoidable since crash
/// handling is a global process-wide thing.
// var init_thread: ?std.Thread = null;
// PORT NOTE: Zig's plain global (init/deinit "should only be called from one
// thread" per the doc below); Rust statics must be Sync, so the handle rides
// in a Mutex. `JoinHandle::join` consumes the handle, so deinit `take`s it.
#[cfg(feature = "sentry")]
static INIT_THREAD: std::sync::Mutex<Option<std::thread::JoinHandle<Result<(), InitThreadError>>>> =
    std::sync::Mutex::new(None);

// Directory memory, holds the cache and state dirs persistently. This
// prevents any sort of crashes due to initialization races.
// var dir_mem: [std.fs.max_path_bytes * 2]u8 = undefined;
// PORT NOTE: the fixed static buffer + `FixedBufferAllocator` (which the two
// dirs are `dupe`d into so they outlive the init thread's arena) collapse into
// the owned byte-paths INSIDE the two statics below — same persistence, same
// "set once on the init thread, read by the Transport" discipline. Rust
// statics must be Sync, so (like INIT_THREAD above) the Zig plain globals
// (`?[]const u8`) ride in Mutexes.

/// Holds the XDG cache dir.
// var cache_dir_: ?[]const u8 = null;
#[cfg(feature = "sentry")]
static CACHE_DIR: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

/// Holds the XDG state dir.
// var state_dir_: ?[]const u8 = null;
#[cfg(feature = "sentry")]
static STATE_DIR: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);

/// SEAM (integration-tier blueprint): the read surface `beforeSend` needs from
/// the reference's `*Surface` — `surface.size.screen`, `surface.size.grid()`,
/// `surface.size.cell` — as plain `(u32, u32)` tuples (acyclicity-REQUIRED:
/// crash must not dep rustty-renderer for its size types, and this is NOT
/// apprt-api's `CoreSurfaceHandle`, which is a different concern).
/// rustty-core's Surface implements this later.
pub trait ThreadStateSurface {
    /// `surface.size.screen.{width, height}` (pixels).
    fn screen_size(&self) -> (u32, u32);
    /// `surface.size.grid().{columns, rows}`.
    fn grid_size(&self) -> (u32, u32);
    /// `surface.size.cell.{width, height}` (pixels).
    fn cell_size(&self) -> (u32, u32);
}

/// Thread-local state that can be set by thread main functions so that
/// crashes have more context.
///
/// This is a hack over Sentry native SDK limitations. The native SDK has
/// one global scope for all threads and no support for thread-local scopes.
/// This means that if we want to set thread-specific data we have to do it
/// on our own in the on crash callback.
pub struct ThreadState<S: ThreadStateSurface> {
    /// Thread type, used to tag the crash
    // PORT NOTE: `type: Type` — `type` is a Rust keyword.
    pub type_: Type,

    /// The surface that this thread is attached to.
    // PORT NOTE: `surface: *Surface` -> a raw `*const S` through the seam trait.
    // `beforeSend` keeps the reference's LIVE (possibly-torn) raw-pointer read
    // semantics — see its "likely unsafe... most likely fine" comment; a
    // snapshot here would change those semantics, so do NOT snapshot.
    pub surface: *const S,
}

// pub const Type = enum { main, renderer, io };
// PORT NOTE: nested as `ThreadState.Type` in the reference; hoisted to module
// level (Rust inherent associated types are unstable; `ThreadState` is also
// generic over the seam, which the tag is not).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Main,
    Renderer,
    Io,
}

impl Type {
    /// PORT NOTE: Zig `@tagName(thr_state.type)` — the reference enum tag names.
    // (Read only by `beforeSend`, which the `-Dsentry=false` build does not
    // analyse — module header. The decl is KEPT in both configurations because
    // `Type` is part of the surface the core tier names; only the lint moves.)
    #[cfg_attr(not(feature = "sentry"), allow(dead_code))]
    const fn tagName(self) -> &'static str {
        match self {
            Type::Main => "main",
            Type::Renderer => "renderer",
            Type::Io => "io",
        }
    }
}

// See ThreadState. This should only ever be set by the owner of the
// thread entry function.
// pub threadlocal var thread_state: ?ThreadState = null;
// PORT NOTE: the reference assigns the threadlocal directly
// (`crash.sentry.thread_state = .{...}` / `= null`); Rust thread-locals are
// accessed through `setThreadState`/`clearThreadState` below, and the stored
// form erases the seam type parameter to `*const dyn ThreadStateSurface`
// (the reference has exactly one concrete Surface type; the vtable is the
// Rust image of that static dispatch).
thread_local! {
    static THREAD_STATE: std::cell::Cell<Option<ErasedThreadState>> =
        const { std::cell::Cell::new(None) };
}

/// The type-erased image of a [`ThreadState<S>`] (what the thread-local
/// actually stores; see the PORT NOTE above).
#[derive(Clone, Copy)]
// (Both fields are read only by `beforeSend` — see `Type::tagName` above.)
#[cfg_attr(not(feature = "sentry"), allow(dead_code))]
struct ErasedThreadState {
    type_: Type,
    surface: *const dyn ThreadStateSurface,
}

/// PORT NOTE: `crash.sentry.thread_state = .{ .type = ..., .surface = ... };`.
pub fn setThreadState<S: ThreadStateSurface + 'static>(state: ThreadState<S>) {
    THREAD_STATE.set(Some(ErasedThreadState {
        type_: state.type_,
        surface: state.surface as *const dyn ThreadStateSurface,
    }));
}

/// PORT NOTE: `crash.sentry.thread_state = .{ .type = ..., .surface = ... };`
/// for a caller that reaches the surface through the seam trait object:
/// termio's io and io-reader threads, whose surface is
/// `io.surface_mailbox.surface` (`AppMailbox::crash_surface`). LEDGER S211-F2,
/// S212-F2.
pub fn setThreadStateDyn(type_: Type, surface: *const dyn ThreadStateSurface) {
    THREAD_STATE.set(Some(ErasedThreadState { type_, surface }));
}

/// PORT NOTE: `crash.sentry.thread_state = null;` (the callers' paired
/// `defer crash.sentry.thread_state = null`).
pub fn clearThreadState() {
    THREAD_STATE.set(None);
}

/// The calling thread's `thread_state`: its type and surface, or `None`.
///
/// PORT NOTE: the read half of the reference's `pub threadlocal var
/// thread_state`, which any code may read; `beforeSend` reads the thread-local
/// directly. Added for LEDGER S211-F2 / S212-F2, whose tests check the value
/// the termio threads set.
pub fn threadState() -> Option<(Type, *const dyn ThreadStateSurface)> {
    THREAD_STATE.get().map(|s| (s.type_, s.surface))
}

/// PORT NOTE: the reified error set of `init`'s `!void` — `std.Thread.spawn`'s
/// failure (Rust: `Builder::spawn`'s io::Error).
#[derive(Debug)]
pub enum InitError {
    Spawn(std::io::Error),
}

/// Process-wide initialization of our Sentry client.
///
/// This should only be called from one thread, and deinit should be called
/// from the same thread that calls init to avoid data races.
///
/// PRIVACY NOTE: I want to make it very clear that Ghostty by default does
/// NOT send any data over the network. We use the Sentry native SDK to collect
/// crash reports and logs, but we only store them locally (see Transport).
/// It is up to the user to grab the logs and manually send them to us
/// (or to their own Sentry instance) if they want to.
// pub fn init(gpa: Allocator, environ_map: std.process.Environ.Map) !void {
// PORT NOTE (P4-E33/E34, `15c50c1db` + `a82637b53` end state): `init` takes the
// environment-map snapshot BY VALUE (owned) and moves it into the init thread —
// no live global environment reads happen on either thread after this point
// (the main-thread-`setenv`-vs-init-thread-read race class this removes).
// The `gpa: Allocator` parameter collapses (the injected allocator is Rust's
// global allocator; same collapse as dir.rs). The reference's four manual
// `map.deinit()` arms (build-off, Windows, spawn-failure, thread exit) collapse
// into Drop — N-A-by-ownership glue (same reasoning as E9).
// (In the `-Dsentry=false` build the ONLY thing this fn does with the map is
// drop it — which is precisely the reference's `var map = environ_map;
// map.deinit();` early-return arm quoted below, expressed by ownership. Rust
// still calls that an unused binding, so the lint is allowed for that config
// only.)
#[cfg_attr(not(feature = "sentry"), allow(unused_variables))]
pub fn init(environ_map: rustty_global::EnvMap) -> Result<(), InitError> {
    // if (comptime !build_options.sentry) {
    //     var map = environ_map;
    //     map.deinit();
    //     return;
    // }
    if !build_options::SENTRY {
        return Ok(());
    }

    // Not supported on Windows currently, doesn't build.
    // if (comptime builtin.os.tag == .windows) {
    //     var map = environ_map;
    //     map.deinit();
    //     return;
    // }
    if cfg!(windows) {
        return Ok(());
    }

    // PORT NOTE (Zig laziness; module header): everything from here down is
    // what the reference's `if (comptime !build_options.sentry) return;` above
    // makes unanalysed, and what `SharedDeps.zig:407` does not link. In Rust it
    // carries the `sentry` feature gate for that reason. The two guards above
    // stay LIVE and unchanged — `build_options::SENTRY` is still read exactly
    // where the reference reads it.
    #[cfg(feature = "sentry")]
    {
        // Must only start once
        // assert(init_thread == null);
        let mut init_thread = INIT_THREAD.lock().unwrap();
        assert!(init_thread.is_none());

        // We use a thread for initializing Sentry because initialization is
        // slow enough to matter for process startup: resolving our directories
        // can take multiple milliseconds on macOS (Apple APIs) and Sentry's
        // own init does disk I/O. Everything Sentry is doing initially is safe
        // to do on a separate thread and fast enough that its very likely to
        // be done before a crash occurs.
        //
        // The environ map is a snapshot owned by the thread (and freed there),
        // so it is safe against concurrent mutations of the process environment
        // (e.g. ensureLocale on the main thread).
        // const thr = std.Thread.spawn(
        //     .{},
        //     initThread,
        //     .{ gpa, environ_map },
        // ) catch |err| {
        //     var map = environ_map;
        //     map.deinit();
        //     return err;
        // };
        // (PORT NOTE: on spawn failure the moved-in closure — and the map it owns —
        // is dropped by `Builder::spawn`; the manual deinit arm collapses.)
        //
        // thr.setName(single_threaded.io(), "sentry-init") catch {};
        // PORT NOTE: Rust can only name a thread AT spawn (`Builder::name`) — the same
        // "sentry-init" name the reference sets post-spawn. Upstream: "Naming the
        // thread from here only works on some platforms (e.g. Linux). On Darwin the
        // thread names itself in initThread."
        let thr = rustty_zigstd::thread::spawn_config()
            .name("sentry-init".to_string())
            .spawn(move || initThread(environ_map))
            .map_err(InitError::Spawn)?;
        // init_thread = thr;
        *init_thread = Some(thr);
    }
    Ok(())
}

/// PORT NOTE: the reified error set of `initThread`'s `!void` — the state/cache
/// dir lookups' errors + `error.SentryInitFailed`.
#[cfg(feature = "sentry")]
#[derive(Debug)]
enum InitThreadError {
    SentryInitFailed,
    /// The `crash.defaultDir` / `internal_os.xdg.cache` lookups' error.
    Dir(internal_os::xdg::DirError),
    /// The macOS `internal_os.macos.cacheDir` (NSCachesDirectory) error.
    #[cfg(target_os = "macos")]
    AppleApi(internal_os::macos::CacheDirError),
}

// fn initThread(gpa: Allocator, environ_map_: std.process.Environ.Map) !void {
// PORT NOTE (P4-E34, `a82637b53` "crash: resolve sentry directories on the init
// thread"): ALL directory resolution happens HERE, on the init thread, against
// the owned snapshot — on macOS the NSFileManager URLForDirectory: call was the
// single largest cost in global.init (2967-4018µs → 30-70µs). The reference's
// `if (comptime !build_options.sentry) return;` guard was dropped upstream in
// this restructure (the thread is only ever spawned when sentry is enabled).
#[cfg(feature = "sentry")]
fn initThread(environ_map: rustty_global::EnvMap) -> Result<(), InitThreadError> {
    // var environ_map = environ_map_;
    // defer environ_map.deinit();
    // (PORT NOTE: the thread owns the snapshot; Drop frees it on every path.)

    // Right now, on Darwin, `std.Thread.setName` can only name the current
    // thread, and we have no way to get the current thread from within it,
    // so instead we use this code to name the thread instead.
    // if (builtin.os.tag.isDarwin()) {
    //     internal_os.macos.pthread_setname_np(&"sentry-init".*);
    // }
    // (PORT NOTE: rustty-os gates its macos module on `target_os = "macos"` — the
    // macOS member of the reference's Darwin set this crate builds for.)
    // (PORT NOTE, LEDGER S79-F5: the thread is still named "sentry-init" on
    // every Darwin target, not only macOS: `init` spawns it with
    // `Builder::name("sentry-init")`, and std names a spawned thread from inside
    // it with `pthread_setname_np(name)` on every `target_vendor = "apple"`
    // target (rust 1.93.1 `library/std/src/sys/thread/unix.rs:451-458`).
    // MEASURED on macOS: a thread spawned that way reads back "sentry-init"
    // through `pthread_getname_np`. This call repeats that on macOS.)
    #[cfg(target_os = "macos")]
    unsafe {
        internal_os::macos::pthread_setname_np(c"sentry-init".as_ptr())
    };

    // Get our directories.
    // var single_threaded: std.Io.Threaded = .init_single_threaded;
    // defer single_threaded.deinit();
    // var fba: std.heap.FixedBufferAllocator = .init(&dir_mem);
    // (PORT NOTE: the single-threaded Io provider is N-A — Rust std does I/O
    // without a provider handle; the FBA collapses into the statics' owned
    // byte-paths, see `dir_mem` above; W94-B02-01: raw path bytes.)

    // state_dir_ = state_dir: {
    //     const dir = try crash.defaultDir(
    //         single_threaded.io(),
    //         gpa,
    //         &environ_map,
    //     );
    //     defer gpa.free(dir.path);
    //     break :state_dir try fba.allocator().dupe(u8, dir.path);
    // };
    let state_dir: Vec<u8> = crash::defaultDir(&environ_map)
        .map_err(InitThreadError::Dir)?
        .path;
    *STATE_DIR.lock().unwrap() = Some(state_dir);
    // errdefer state_dir_ = null;
    struct StateDirErrdefer;
    impl Drop for StateDirErrdefer {
        fn drop(&mut self) {
            *STATE_DIR.lock().unwrap() = None;
        }
    }
    let state_dir_errdefer = StateDirErrdefer;

    // const cache_dir = cache_dir: {
    //     const dir = try cacheDir(
    //         single_threaded.io(),
    //         gpa,
    //         &environ_map,
    //     );
    //     defer gpa.free(dir);
    //     break :cache_dir try fba.allocator().dupe(u8, dir);
    // };
    let cache_dir: Vec<u8> = cacheDir(&environ_map)?;
    // cache_dir_ = cache_dir;
    // (PORT NOTE: the reference aliases the static and the local to the same
    // FBA bytes; Rust clones the bytes into the static, the local keeps
    // feeding `sentry_options_set_database_path_n` below.)
    *CACHE_DIR.lock().unwrap() = Some(cache_dir.clone());
    // errdefer cache_dir_ = null;
    struct CacheDirErrdefer;
    impl Drop for CacheDirErrdefer {
        fn drop(&mut self) {
            *CACHE_DIR.lock().unwrap() = None;
        }
    }
    let cache_dir_errdefer = CacheDirErrdefer;

    // const transport = sentry.Transport.init(&Transport.send);
    let transport = sentry::Transport::init(Transport::send);
    // This will crash if the transport was never used so we avoid
    // that for now. This probably leaks some memory but it'd be very
    // small and a one time cost. Once this is fixed upstream we can
    // remove this.
    //errdefer transport.deinit();

    // const opts = sentry.c.sentry_options_new();
    let opts = unsafe { sentry::c::sentry_options_new() };
    // errdefer sentry.c.sentry_options_free(opts);
    // PORT NOTE: errdefer -> drop guard, forgotten at the function's success exit;
    // it stays armed across `sentry_init` exactly like the reference (the
    // `error.SentryInitFailed` return path frees `opts` there too).
    struct OptsErrdefer(*mut sentry::c::sentry_options_t);
    impl Drop for OptsErrdefer {
        fn drop(&mut self) {
            unsafe { sentry::c::sentry_options_free(self.0) }
        }
    }
    let opts_errdefer = OptsErrdefer(opts);

    // sentry.c.sentry_options_set_release_n(
    //     opts,
    //     build_config.version_string.ptr,
    //     build_config.version_string.len,
    // );
    unsafe {
        sentry::c::sentry_options_set_release_n(
            opts,
            rustty_version::VERSION_STRING.as_ptr().cast(),
            rustty_version::VERSION_STRING.len(),
        )
    };
    // sentry.c.sentry_options_set_transport(opts, @ptrCast(transport));
    unsafe { sentry::c::sentry_options_set_transport(opts, transport.cast()) };

    // Set our crash callback. See beforeSend for more details on what we
    // do here and why we use this.
    // sentry.c.sentry_options_set_before_send(opts, beforeSend, null);
    unsafe {
        sentry::c::sentry_options_set_before_send(opts, Some(beforeSend), std::ptr::null_mut())
    };

    // sentry.c.sentry_options_set_database_path_n(
    //     opts,
    //     cache_dir.ptr,
    //     cache_dir.len,
    // );
    unsafe {
        sentry::c::sentry_options_set_database_path_n(
            opts,
            cache_dir.as_ptr().cast(),
            cache_dir.len(),
        )
    };

    // if (comptime builtin.mode == .Debug) {
    if cfg!(debug_assertions) {
        // Debug logging for Sentry
        // sentry.c.sentry_options_set_debug(opts, @intFromBool(true));
        unsafe { sentry::c::sentry_options_set_debug(opts, i32::from(true)) };
    }

    // Initialize
    // if (sentry.c.sentry_init(opts) != 0) return error.SentryInitFailed;
    if unsafe { sentry::c::sentry_init(opts) } != 0 {
        return Err(InitThreadError::SentryInitFailed);
    }
    // (the errdefer window closed: `sentry_init` took ownership of `opts`.)
    std::mem::forget(opts_errdefer);

    // Setup some basic tags that we always want present
    // sentry.setTag("build-mode", build_config.mode_string);
    sentry::setTag("build-mode", build_config::mode_string());
    // sentry.setTag("app-runtime", @tagName(build_config.app_runtime));
    sentry::setTag("app-runtime", tagNameRuntime(build_config::app_runtime()));
    // sentry.setTag("font-backend", @tagName(build_config.font_backend));
    sentry::setTag(
        "font-backend",
        tagNameFontBackend(build_config::font_backend()),
    );
    // sentry.setTag("renderer", @tagName(build_config.renderer));
    sentry::setTag("renderer", tagNameRenderer(build_config::renderer()));

    // Log some information about sentry
    // log.debug("sentry initialized database={s}", .{cache_dir});
    log::debug!(target: LOG_SCOPE, "sentry initialized database={}", String::from_utf8_lossy(&cache_dir));

    // (normal return: the reference's dir errdefers don't run — the statics
    // keep the resolved dirs for the Transport.)
    std::mem::forget(state_dir_errdefer);
    std::mem::forget(cache_dir_errdefer);
    Ok(())
}

// fn cacheDir(io: std.Io, alloc: Allocator, environ_map: *const std.process.Environ.Map) ![]const u8 {
// PORT NOTE (P4-E33): file-local, resolves the Sentry database (cache) dir from
// the caller-supplied snapshot only. The `io`/`alloc` parameters are N-A /
// collapsed as everywhere else in this file.
#[cfg(feature = "sentry")]
fn cacheDir(environ_map: &rustty_global::EnvMap) -> Result<Vec<u8>, InitThreadError> {
    // On macOS, we prefer to use the NSCachesDirectory value to be
    // a more idiomatic macOS application. But if XDG env vars are set
    // we will respect them.
    //
    // if (comptime builtin.os.tag == .macos) macos: {
    //     const xdg_cache_home = environ_map.get("XDG_CACHE_HOME") orelse break :macos;
    //     if (xdg_cache_home.len > 0) {
    //         return try internal_os.macos.cacheDir(
    //             alloc,
    //             "sentry",
    //         );
    //     }
    // }
    //
    // PORT NOTE: reference defect (R13), UD21 (LEDGER S79-F1). The condition
    // above is the inverse of its own comment: upstream `15c50c1db` inverted it
    // while extracting the fn, so a set-and-nonempty `XDG_CACHE_HOME` picks
    // NSCachesDirectory and an unset one picks the XDG path. That visibly breaks
    // the reference's evident intent, so under R13 (as extended 2026-09-23) the
    // port does what the comment says: set and non-empty `XDG_CACHE_HOME` →
    // respect XDG (`xdg.cache` below); unset or empty → NSCachesDirectory
    // (`XDG_CACHE_HOME=""` is treated as unset). This is the phase-4 P4-E35
    // behaviour; parity tail T-E briefly ported the inversion (under R13 as it
    // stood before the extension) and this round reverts that. Listed in
    // `docs/ghostty-bug-fixes.md`. Test `cache_dir_follows_the_intended_condition`.
    #[cfg(target_os = "macos")]
    {
        let respect_xdg = environ_map
            .get(&b"XDG_CACHE_HOME"[..])
            .is_some_and(|xdg_cache_home| !xdg_cache_home.is_empty());
        if !respect_xdg {
            // (W94-B02-01: the macOS arm's UTF-8 `String` converts losslessly
            // via `into_bytes` — the statics are raw path bytes.)
            return internal_os::macos::cacheDir("sentry")
                .map(String::into_bytes)
                .map_err(InitThreadError::AppleApi);
        }
    }

    // return try internal_os.xdg.cache(
    //     io,
    //     alloc,
    //     environ_map,
    //     .{ .subdir = "ghostty/sentry" },
    // );
    // (R2: the app-name segment of the `"ghostty/sentry"` subdir is sourced
    // from rustty-identity, the identity source-of-truth crate, not hardcoded —
    // same as dir.rs.)
    let subdir = format!("{}/sentry", rustty_identity::APP_NAME);
    internal_os::xdg::cache(
        environ_map,
        internal_os::xdg::Options {
            subdir: Some(subdir.as_bytes()),
            ..Default::default()
        },
    )
    .map_err(InitThreadError::Dir)
}

/// Process-wide deinitialization of our Sentry client. This ensures all
/// our data is flushed.
// (In the `-Dsentry=false` build the Windows guard below is the LAST statement
// of the fn, so clippy calls its `return` needless. It is the faithful port of
// `if (comptime builtin.os.tag == .windows) return;` and stays spelled that way
// under R1, so the lint is allowed for that config only — the guard itself is
// unchanged and still live.)
#[cfg_attr(not(feature = "sentry"), allow(clippy::needless_return))]
pub fn deinit() {
    // if (comptime !build_options.sentry) return;
    if !build_options::SENTRY {
        return;
    }

    // if (comptime builtin.os.tag == .windows) return;
    if cfg!(windows) {
        return;
    }

    // PORT NOTE (Zig laziness; module header): as in `init`, the tail past the
    // reference's own `if (comptime !build_options.sentry) return;` is the part
    // Zig never analyses and `SharedDeps.zig:407` never links, so it carries the
    // `sentry` feature gate. Both guards above stay live.
    #[cfg(feature = "sentry")]
    {
        // If we're still initializing then wait for init to finish. This
        // is highly unlikely since init is a very fast operation but we want
        // to avoid the possibility.
        // const thr = init_thread orelse return;
        // PORT NOTE: `JoinHandle::join` consumes the handle, so it is `take`n out of
        // the global (the reference leaves the joined `std.Thread` value behind).
        let thr = match INIT_THREAD.lock().unwrap().take() {
            Some(thr) => thr,
            None => return,
        };
        // thr.join();
        // (PORT NOTE: the reference's spawn wrapper discards `initThread`'s error;
        // the joined Result is likewise discarded.)
        let _ = thr.join();
        // _ = sentry.c.sentry_close();
        let _ = unsafe { sentry::c::sentry_close() };
    }
}

// fn beforeSend(
//     event_val: sentry.c.sentry_value_t,
//     _: ?*anyopaque,
//     _: ?*anyopaque,
// ) callconv(.c) sentry.c.sentry_value_t {
#[cfg(feature = "sentry")]
unsafe extern "C" fn beforeSend(
    event_val: sentry::c::sentry_value_t,
    _hint: *mut std::ffi::c_void,
    _closure: *mut std::ffi::c_void,
) -> sentry::c::sentry_value_t {
    // The native SDK at the time of writing doesn't support thread-local
    // scopes. The full SDK has one global scope. So we use the beforeSend
    // handler to set thread-specific data such as window size, grid size,
    // etc. that we can use to debug crashes.

    // If we don't have thread state we can't reliably determine
    // metadata such as surface dimensions. In the future we can probably
    // drop full app state (all surfaces, all windows, etc.).
    // const thr_state = thread_state orelse {
    //     log.debug("no thread state, skipping crash metadata", .{});
    //     return event_val;
    // };
    let thr_state = match THREAD_STATE.get() {
        Some(v) => v,
        None => {
            // log.debug("no thread state, skipping crash metadata", .{});
            log::debug!(target: LOG_SCOPE, "no thread state, skipping crash metadata");
            return event_val;
        }
    };

    // Get our event contexts. At this point Sentry has already merged
    // all the contexts so we should have this key. If not, we create it.
    // const event: sentry.Value = .{ .value = event_val };
    let event = sentry::Value { value: event_val };
    // const contexts = event.get("contexts") orelse contexts: {
    //     const obj = sentry.Value.initObject();
    //     event.set("contexts", obj);
    //     break :contexts obj;
    // };
    let contexts = match event.get("contexts") {
        Some(v) => v,
        None => {
            let obj = sentry::Value::initObject();
            event.set("contexts", obj);
            obj
        }
    };
    // const tags = event.get("tags") orelse tags: {
    //     const obj = sentry.Value.initObject();
    //     event.set("tags", obj);
    //     break :tags obj;
    // };
    let tags = match event.get("tags") {
        Some(v) => v,
        None => {
            let obj = sentry::Value::initObject();
            event.set("tags", obj);
            obj
        }
    };

    // Store our thread type
    // tags.set("thread-type", sentry.Value.initString(@tagName(thr_state.type)));
    tags.set(
        "thread-type",
        sentry::Value::initString(thr_state.type_.tagName()),
    );

    // Read the surface data. This is likely unsafe because on a crash
    // other threads can continue running. We don't have race-safe way to
    // access this data so this might be corrupted but it's most likely fine.
    {
        let obj = sentry::Value::initObject();
        // errdefer obj.decref();
        // (PORT NOTE: nothing below is fallible; the errdefer collapses.)
        // const surface = thr_state.surface;
        let surface = thr_state.surface;
        // SEAM: the reference reads `surface.size.screen`/`surface.size.grid()`
        // /`surface.size.cell` off the raw pointer; the trait methods below are
        // the same LIVE (possibly-torn) reads — deliberately not snapshotted.
        let (screen_width, screen_height) = unsafe { (*surface).screen_size() };
        // const grid_size = surface.size.grid();
        let (grid_columns, grid_rows) = unsafe { (*surface).grid_size() };
        let (cell_width, cell_height) = unsafe { (*surface).cell_size() };
        obj.set(
            "screen-width",
            // sentry.Value.initInt32(std.math.cast(i32, surface.size.screen.width) orelse -1),
            sentry::Value::initInt32(i32::try_from(screen_width).unwrap_or(-1)),
        );
        obj.set(
            "screen-height",
            // sentry.Value.initInt32(std.math.cast(i32, surface.size.screen.height) orelse -1),
            sentry::Value::initInt32(i32::try_from(screen_height).unwrap_or(-1)),
        );
        obj.set(
            "grid-columns",
            // sentry.Value.initInt32(std.math.cast(i32, grid_size.columns) orelse -1),
            sentry::Value::initInt32(i32::try_from(grid_columns).unwrap_or(-1)),
        );
        obj.set(
            "grid-rows",
            // sentry.Value.initInt32(std.math.cast(i32, grid_size.rows) orelse -1),
            sentry::Value::initInt32(i32::try_from(grid_rows).unwrap_or(-1)),
        );
        obj.set(
            "cell-width",
            // sentry.Value.initInt32(std.math.cast(i32, surface.size.cell.width) orelse -1),
            sentry::Value::initInt32(i32::try_from(cell_width).unwrap_or(-1)),
        );
        obj.set(
            "cell-height",
            // sentry.Value.initInt32(std.math.cast(i32, surface.size.cell.height) orelse -1),
            sentry::Value::initInt32(i32::try_from(cell_height).unwrap_or(-1)),
        );

        contexts.set("Dimensions", obj);
    }

    event_val
}

/// PORT NOTE: Zig `@tagName(build_config.app_runtime)` — the reference
/// `apprt.Runtime` enum tag names (src/apprt/runtime.zig), byte-exact.
#[cfg(feature = "sentry")]
const fn tagNameRuntime(v: build_config::Runtime) -> &'static str {
    match v {
        build_config::Runtime::None => "none",
        build_config::Runtime::Gtk => "gtk",
    }
}

/// PORT NOTE: Zig `@tagName(build_config.font_backend)` — the reference
/// `font.Backend` enum tag names (src/font/backend.zig), byte-exact.
#[cfg(feature = "sentry")]
const fn tagNameFontBackend(v: build_config::font::Backend) -> &'static str {
    match v {
        build_config::font::Backend::Freetype => "freetype",
        build_config::font::Backend::FreetypeWindows => "freetype_windows",
        build_config::font::Backend::FontconfigFreetype => "fontconfig_freetype",
        build_config::font::Backend::Coretext => "coretext",
        build_config::font::Backend::CoretextFreetype => "coretext_freetype",
        build_config::font::Backend::CoretextHarfbuzz => "coretext_harfbuzz",
        build_config::font::Backend::CoretextNoshape => "coretext_noshape",
        build_config::font::Backend::WebCanvas => "web_canvas",
    }
}

/// PORT NOTE: Zig `@tagName(build_config.renderer)` — the reference
/// `renderer.Backend` enum tag names (src/renderer/backend.zig), byte-exact.
#[cfg(feature = "sentry")]
const fn tagNameRenderer(v: build_config::renderer::Backend) -> &'static str {
    match v {
        build_config::renderer::Backend::Opengl => "opengl",
        build_config::renderer::Backend::Metal => "metal",
        build_config::renderer::Backend::Webgl => "webgl",
    }
}

/// PORT NOTE: the reified error set of `Transport.sendInternal`'s `!void` — the
/// unset-state-dir guard + the envelope parse errors + the filesystem writes
/// (the crash-dir lookup left this set in `15c50c1db`: the dir is resolved on
/// the init thread now, see `STATE_DIR`).
// (The payloads are carried for the reference's `log.warn("... err={}", .{err})`
// — currently a comment, like every log line in the port — and for Debug;
// dead-code analysis intentionally ignores derived Debug, hence the allow.)
#[cfg(feature = "sentry")]
#[allow(dead_code)]
#[derive(Debug)]
enum SendError {
    // error.StateDirNotInitialized
    StateDirNotInitialized,
    Envelope(crate::sentry_envelope::Error),
    Io(std::io::Error),
}

#[cfg(feature = "sentry")]
impl From<crate::sentry_envelope::Error> for SendError {
    fn from(err: crate::sentry_envelope::Error) -> SendError {
        SendError::Envelope(err)
    }
}

#[cfg(feature = "sentry")]
impl From<std::io::Error> for SendError {
    fn from(err: std::io::Error) -> SendError {
        SendError::Io(err)
    }
}

#[cfg(feature = "sentry")]
pub struct Transport;

#[cfg(feature = "sentry")]
impl Transport {
    // pub fn send(envelope: *sentry.Envelope, ud: ?*anyopaque) callconv(.c) void {
    /// # Safety
    ///
    /// Only for sentry-native to call as the transport `send_func`
    /// (`envelope` is the live envelope whose ownership the transport takes).
    pub unsafe extern "C" fn send(envelope: *mut sentry::Envelope, ud: *mut std::ffi::c_void) {
        // _ = ud;
        let _ = ud;
        // defer envelope.deinit();
        // (PORT NOTE: single exit path; the deferred deinit runs after the catch.)

        // Call our internal impl. If it fails there is nothing we can do
        // but log to the user.
        // sendInternal(envelope) catch |err| {
        //     log.warn("failed to persist crash report err={}", .{err});
        // };
        if let Err(err) = unsafe { Self::sendInternal(envelope) } {
            // log.warn("failed to persist crash report err={}", .{err});
            //
            // PORT NOTE (N11): Zig's `{}` on an error renders `error.<Name>`;
            // `{err:?}` renders the Rust error's own Debug. Workspace-dominant
            // rendering; N11's scope, not this site's.
            log::warn!(target: LOG_SCOPE, "failed to persist crash report err={err:?}");
        }

        unsafe { sentry::Envelope::deinit(envelope) };
    }

    /// Implementation of send but we can use Zig errors.
    // fn sendInternal(envelope: *sentry.Envelope) !void {
    /// # Safety
    ///
    /// `envelope` must be the live envelope pointer `send` received.
    unsafe fn sendInternal(envelope: *mut sentry::Envelope) -> Result<(), SendError> {
        // const state_dir = state_dir_ orelse return error.StateDirNotInitialized;
        let state_dir = match STATE_DIR.lock().unwrap().clone() {
            Some(v) => v,
            None => return Err(SendError::StateDirNotInitialized),
        };

        // The I/O and allocator we use here are just meant to get the job
        // done for saving the crash report.
        // var single_threaded: std.Io.Threaded = .init_single_threaded;
        // defer single_threaded.deinit();
        // var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        // (PORT NOTE, `15c50c1db`: upstream swapped the arena's child from
        // `global.alloc()` to `std.heap.page_allocator` — no global state; in
        // Rust the arena collapses into ownership as before, and the Io
        // provider is N-A.)

        // Parse into an envelope structure
        // const json = envelope.serialize();
        let json = unsafe { sentry::Envelope::serialize(envelope) };
        // defer sentry.free(@ptrCast(json.ptr));
        struct JsonDefer(*mut std::ffi::c_void);
        impl Drop for JsonDefer {
            fn drop(&mut self) {
                unsafe { crate::sentry_pkg::free(self.0) }
            }
        }
        let _json_defer = JsonDefer(json.as_ptr().cast_mut().cast());

        // var parsed: crash.Envelope = parsed: {
        //     var reader: std.Io.Reader = .fixed(json);
        //     break :parsed try crash.Envelope.parse(alloc, &reader);
        // };
        // (PORT NOTE: Zig's `var` binding — never mutated after parse, so no `mut`.)
        let parsed: crash::Envelope = {
            let mut reader: &[u8] = json;
            crash::Envelope::parse(&mut reader)?
        };
        // defer parsed.deinit(); — Drop.

        // If our envelope doesn't have an event then we don't do anything.
        // To figure this out we first encode it into a string, parse it,
        // and check if it has an event. Kind of wasteful but the best
        // option we have at the time of writing this since the C API doesn't
        // expose this information.
        if Self::shouldDiscard(&parsed)? {
            // log.info("sentry envelope does not contain crash, discarding", .{});
            log::info!(target: LOG_SCOPE, "sentry envelope does not contain crash, discarding");
            return Ok(());
        }

        // Generate a UUID for this envelope. The envelope DOES have an event_id
        // header but I don't think there is any public API way to get it
        // afaict so we generate a new UUID for the filename just so we don't
        // conflict.
        // const uuid = sentry.UUID.init();
        let uuid = sentry::UUID::init();

        // try std.Io.Dir.cwd().createDirPath(single_threaded.io(), state_dir);
        // (W94-B02-01: `state_dir` is raw path bytes now — via `OsStr`.)
        std::fs::create_dir_all(crate::dir::os_str_from_bytes(&state_dir))?;

        // Build our final path and write to it.
        // const path = try std.fs.path.join(alloc, &.{
        //     state_dir,
        //     try std.fmt.allocPrint(alloc, "{s}.ghosttycrash", .{uuid.string()}),
        // });
        let uuid_string = uuid.string();
        let path = std::path::Path::new(crate::dir::os_str_from_bytes(&state_dir)).join(format!(
            "{}.{}crash",
            // (PORT NOTE: `uuid.string()` is `[36:0]u8` — `{s}` formats the 36
            // UUID chars before the NUL sentinel.)
            std::str::from_utf8(&uuid_string[..36]).unwrap(),
            // (R2: the `.ghosttycrash` report extension's app-name segment is
            // sourced from rustty-identity, not hardcoded.)
            rustty_identity::APP_NAME,
        ));
        // const file = try std.Io.Dir.cwd().createFile(single_threaded.io(), path, .{});
        let mut file = std::fs::File::create(&path)?;
        // defer file.close(single_threaded.io()); — Drop.
        // var buf: [4096]u8 = undefined;
        // var file_writer = file.writer(single_threaded.io(), &buf);
        // try file_writer.interface.writeAll(json);
        // try file_writer.end();
        // (PORT NOTE: the fixed 4096-byte buffered writer + `end()` flush collapse
        // to one full write.)
        std::io::Write::write_all(&mut file, json)?;

        // log.warn("crash report written to disk path={s}", .{path});
        log::warn!(target: LOG_SCOPE, "crash report written to disk path={}", path.display());
        Ok(())
    }

    // fn shouldDiscard(envelope: *const crash.Envelope) !bool {
    // (PORT NOTE: the reference `!bool` never errors in the body; the Result shape
    // is kept for the caller's `try`.)
    fn shouldDiscard(envelope: &crash::Envelope) -> Result<bool, SendError> {
        // If we have an event item then we're good.
        // for (envelope.items.items) |item| {
        for item in &envelope.items {
            // if (item.itemType() == .event) return false;
            if item.itemType() == crate::sentry_envelope::ItemType::Event {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    #[cfg_attr(not(feature = "sentry"), allow(unused_imports))]
    use super::*;

    // PORT NOTE (Zig laziness; module header): the four `beforeSend` tests below
    // drive the sentry-native value API, so they only exist in the
    // `-Dsentry=true` build — the reference has no test{} here at all (0
    // reference tests in sentry.zig), and in a `-Dsentry=false` build there is
    // no `beforeSend` to drive. They are GATED, never deleted; `cargo test -p
    // rustty-crash --features sentry` (the G-FEATMATRIX arm) runs them. The
    // fifth test reads this file's SOURCE text, so it holds in both.

    /// rustty-only seam test (rust >= zig; sentry.zig has 0 reference tests):
    /// prove the `ThreadStateSurface` trait seam feeds `beforeSend`'s
    /// Dimensions/tags metadata exactly as the reference's `*Surface` reads
    /// would. PURE value-API only — no `sentry_init`, no transport/backend
    /// thread (the task/reference gate: tests must not start the client).
    #[cfg(feature = "sentry")]
    #[test]
    fn before_send_thread_state_surface_seam() {
        struct DummySurface;
        impl ThreadStateSurface for DummySurface {
            fn screen_size(&self) -> (u32, u32) {
                (800, 600)
            }
            fn grid_size(&self) -> (u32, u32) {
                (80, 24)
            }
            fn cell_size(&self) -> (u32, u32) {
                // u32::MAX exercises the `std.math.cast(i32, ...) orelse -1`
                // overflow arm.
                (10, u32::MAX)
            }
        }
        static SURFACE: DummySurface = DummySurface;

        // crash.sentry.thread_state = .{ .type = .renderer, .surface = ... };
        setThreadState(ThreadState {
            type_: Type::Renderer,
            surface: &SURFACE,
        });

        let event = sentry::Value::initObject();
        let result = sentry::Value {
            value: unsafe { beforeSend(event.value, std::ptr::null_mut(), std::ptr::null_mut()) },
        };

        // tags.thread-type == "renderer"
        let tags = result.get("tags").expect("tags object set");
        let thread_type = tags.get("thread-type").expect("thread-type set");
        let s = unsafe {
            std::ffi::CStr::from_ptr(sentry::c::sentry_value_as_string(thread_type.value))
        };
        assert_eq!(s.to_bytes(), b"renderer");

        // contexts.Dimensions.{screen,grid,cell} from the live seam reads.
        let contexts = result.get("contexts").expect("contexts object set");
        let dims = contexts.get("Dimensions").expect("Dimensions set");
        let get_i32 = |key: &str| -> i32 {
            unsafe { sentry::c::sentry_value_as_int32(dims.get(key).expect(key).value) }
        };
        assert_eq!(get_i32("screen-width"), 800);
        assert_eq!(get_i32("screen-height"), 600);
        assert_eq!(get_i32("grid-columns"), 80);
        assert_eq!(get_i32("grid-rows"), 24);
        assert_eq!(get_i32("cell-width"), 10);
        // std.math.cast(i32, u32::MAX) orelse -1 == -1.
        assert_eq!(get_i32("cell-height"), -1);

        // defer crash.sentry.thread_state = null;
        clearThreadState();
        result.decref();
    }

    /// task-14e-2 / S79-F2 — `sentry.zig:239`, the only one of the file's five
    /// diagnostics with a driver that does not need a live sentry client.
    ///
    /// Rides the same path `before_send_no_thread_state` above drives.
    #[cfg(feature = "sentry")]
    #[test]
    fn before_send_without_thread_state_logs_the_skip() {
        let cap = crate::log_capture::start();
        clearThreadState();
        let event = sentry::Value::initObject();
        let result = sentry::Value {
            value: unsafe { beforeSend(event.value, std::ptr::null_mut(), std::ptr::null_mut()) },
        };
        let messages = cap.messages();
        drop(cap);

        // Positive reach: the reference's arm returns the event UNTOUCHED, so
        // these two absences are what prove the early return ran.
        assert!(result.get("tags").is_none());
        assert!(result.get("contexts").is_none());
        result.decref();

        assert_eq!(
            vec![(
                log::Level::Debug,
                "sentry".to_string(),
                "no thread state, skipping crash metadata".to_string(),
            )],
            messages
        );
    }

    /// The absence half of the same site: WITH thread state, the skip line must
    /// not fire. Positive reach: the metadata the non-skip path writes.
    #[cfg(feature = "sentry")]
    #[test]
    fn before_send_with_thread_state_does_not_log_the_skip() {
        struct DummySurface;
        impl ThreadStateSurface for DummySurface {
            fn screen_size(&self) -> (u32, u32) {
                (800, 600)
            }
            fn grid_size(&self) -> (u32, u32) {
                (80, 24)
            }
            fn cell_size(&self) -> (u32, u32) {
                (10, 20)
            }
        }
        static SURFACE: DummySurface = DummySurface;

        let cap = crate::log_capture::start();
        setThreadState(ThreadState {
            type_: Type::Renderer,
            surface: &SURFACE,
        });
        let event = sentry::Value::initObject();
        let result = sentry::Value {
            value: unsafe { beforeSend(event.value, std::ptr::null_mut(), std::ptr::null_mut()) },
        };
        let messages = cap.messages();
        drop(cap);

        assert!(
            result.get("contexts").is_some(),
            "beforeSend took the skip arm, so this test never reached the code \
             whose silence it asserts"
        );
        clearThreadState();
        result.decref();

        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }

    /// task-14e-2 / S79-F2 — the four `sentry.zig` diagnostics that have no
    /// driver here.
    ///
    /// STRUCTURAL, and deliberately so. `sentry initialized database=` needs a
    /// completed `sentry_init`; the other three sit inside `Transport::send` /
    /// `sendInternal`, which need a live `sentry.Envelope` the SDK hands the
    /// transport. This crate's existing tests are explicitly forbidden from
    /// starting the client (see `before_send_thread_state_surface_seam`'s
    /// header: "PURE value-API only — no `sentry_init`, no transport/backend
    /// thread"), so there is no in-crate way to drive them. This pins what can
    /// be observed: level, scope and message text of each emission. It is a
    /// weaker statement than a behavioural test and is reported as such.
    ///
    /// The macro/level tokens are ASSEMBLED FROM FRAGMENTS: a literal
    /// `log::warn!` here would be a live-looking facade call to
    /// `check_log_reconcile.py`'s scanner and to its LIVE-TREE self-test arm.
    #[test]
    fn the_four_undrivable_diagnostics_are_live_at_the_reference_level_and_scope() {
        const SRC: &str = include_str!("sentry.rs");
        let call = |level: &str, message: &str| -> String {
            let m = ["log", "::", level, "!"].concat();
            format!("{m}(target: LOG_SCOPE, \"{message}\"")
        };

        // `const log = std.log.scoped(.sentry);`
        assert_eq!(
            1,
            SRC.matches("const LOG_SCOPE: &str = \"sentry\";").count(),
            "the reference's scope tag is not declared exactly once"
        );

        for (level, message) in [
            // log.debug("sentry initialized database={s}", .{cache_dir});
            ("debug", "sentry initialized database={}"),
            // log.warn("failed to persist crash report err={}", .{err});
            ("warn", "failed to persist crash report err={err:?}"),
            // log.info("sentry envelope does not contain crash, discarding", .{});
            ("info", "sentry envelope does not contain crash, discarding"),
            // log.warn("crash report written to disk path={s}", .{path});
            ("warn", "crash report written to disk path={}"),
        ] {
            let wanted = call(level, message);
            assert_eq!(
                1,
                SRC.matches(&wanted).count(),
                "expected exactly one live emission `{wanted}`"
            );
        }
    }

    /// rustty-only: without thread state `beforeSend` must pass the event
    /// through untouched (the reference's early-return arm).
    #[cfg(feature = "sentry")]
    #[test]
    fn before_send_no_thread_state() {
        clearThreadState();
        let event = sentry::Value::initObject();
        let result = sentry::Value {
            value: unsafe { beforeSend(event.value, std::ptr::null_mut(), std::ptr::null_mut()) },
        };
        assert!(result.get("tags").is_none());
        assert!(result.get("contexts").is_none());
        result.decref();
    }

    // No reference test. LEDGER S79-F1, reference defect (R13) UD21: the
    // reference's macOS `cacheDir` (`sentry.zig:188-207`) returns
    // `NSCachesDirectory` when `XDG_CACHE_HOME` is set and non-empty, and the
    // XDG cache otherwise: the inverse of its own comment ("if XDG env vars
    // are set we will respect them"). The port implements the comment.
    // Negative control: reinstate the reference's condition (NSCaches when
    // set and non-empty) and the first assertion fails with the two paths
    // swapped.
    #[cfg(all(feature = "sentry", target_os = "macos"))]
    #[test]
    fn cache_dir_follows_the_intended_condition() {
        let map = |pairs: &[(&str, &str)]| -> rustty_global::EnvMap {
            pairs
                .iter()
                .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
                .collect()
        };
        let ns_caches = internal_os::macos::cacheDir("sentry").unwrap().into_bytes();
        let xdg = format!("/x/{}/sentry", rustty_identity::APP_NAME).into_bytes();
        assert_ne!(ns_caches, xdg);

        // Set and non-empty: the XDG cache under `$XDG_CACHE_HOME`.
        let got = cacheDir(&map(&[("XDG_CACHE_HOME", "/x"), ("HOME", "/h")])).unwrap();
        assert_eq!(xdg, got);
        // Unset, or empty: NSCachesDirectory.
        let got = cacheDir(&map(&[("HOME", "/h")])).unwrap();
        assert_eq!(ns_caches, got);
        let got = cacheDir(&map(&[("XDG_CACHE_HOME", ""), ("HOME", "/h")])).unwrap();
        assert_eq!(ns_caches, got);
    }
}
