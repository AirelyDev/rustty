//! rustty-crash::sentry
//!
//! PORTS: ghostty src/crash/sentry.zig — the process-wide Sentry client
//! (init/deinit + the beforeSend crash-metadata callback + the local-disk
//! Transport). 0 reference tests.
//!
//! PORT STATUS: done
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
use rustty_build_config as build_config;
// const build_options = @import("build_options");
use crate::build_options;
// const sentry = if (build_options.sentry) @import("sentry");
// (PORT NOTE: the Zig binding package `pkg/sentry` is hosted in-crate as
// `sentry_pkg`; the `if (build_options.sentry)` comptime gate collapses — the
// module always compiles, and every reaching path is behind the same
// `build_options.sentry` runtime-const checks the reference uses.)
use crate::sentry_pkg as sentry;
// const internal_os = @import("../os/main.zig");
use rustty_os as internal_os;
// const crash = @import("main.zig");
use crate as crash;
// const state = &@import("../global.zig").state;
// (PORT, break #1: rustty-global stays INJECTED, not a hard dep. The single
// use is `state.alloc` as `sendInternal`'s arena child allocator, which
// collapses into Rust ownership below.)
// const Surface = @import("../Surface.zig");
// (PORT NOTE: seamed via `ThreadStateSurface` — see the module doc above. crash
// must also NOT dep rustty-renderer for the size types: renderer already deps
// crash, so the reverse edge would cycle — hence plain `(u32, u32)` tuples.)

// const log = std.log.scoped(.sentry);

/// The global state for the Sentry SDK. This is unavoidable since crash
/// handling is a global process-wide thing.
// var init_thread: ?std.Thread = null;
// PORT NOTE: Zig's plain global (init/deinit "should only be called from one
// thread" per the doc below); Rust statics must be Sync, so the handle rides
// in a Mutex. `JoinHandle::join` consumes the handle, so deinit `take`s it.
static INIT_THREAD: std::sync::Mutex<Option<std::thread::JoinHandle<Result<(), InitThreadError>>>> =
    std::sync::Mutex::new(None);

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

/// PORT NOTE: `crash.sentry.thread_state = null;` (the callers' paired
/// `defer crash.sentry.thread_state = null`).
pub fn clearThreadState() {
    THREAD_STATE.set(None);
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
/// (or they own Sentry instance) if they want to.
// pub fn init(gpa: Allocator) !void {
// PORT NOTE: the `gpa: Allocator` parameter collapses (break #1 — the injected
// allocator is Rust's global allocator; same collapse as dir.rs).
pub fn init() -> Result<(), InitError> {
    // if (comptime !build_options.sentry) return;
    if !build_options::SENTRY {
        return Ok(());
    }

    // Not supported on Windows currently, doesn't build.
    // if (comptime builtin.os.tag == .windows) return;
    if cfg!(windows) {
        return Ok(());
    }

    // const start = try std.time.Instant.now();
    // const start_micro = std.time.microTimestamp();
    // defer {
    //     const end = std.time.Instant.now() catch unreachable;
    //     // "[updateFrame critical time] <START us>\t<TIME_TAKEN us>"
    //     std.log.err("[sentry init time] start={}us duration={}ns", .{ start_micro, end.since(start) / std.time.ns_per_us });
    // }

    // Must only start once
    // assert(init_thread == null);
    let mut init_thread = INIT_THREAD.lock().unwrap();
    assert!(init_thread.is_none());

    // We use a thread for initializing Sentry because initialization takes
    // ~2k ns on my M3 Max. That's not a LOT of time but it's enough to be
    // 90% of our pre-App startup time. Everything Sentry is doing initially
    // is safe to do on a separate thread and fast enough that its very
    // likely to be done before a crash occurs.
    // const thr = try std.Thread.spawn(
    //     .{},
    //     initThread,
    //     .{gpa},
    // );
    // thr.setName("sentry-init") catch {};
    // PORT NOTE: Rust can only name a thread AT spawn (`Builder::name`) — the same
    // "sentry-init" name the reference sets post-spawn (and re-sets from
    // within the thread on Darwin; see initThread).
    let thr = std::thread::Builder::new()
        .name("sentry-init".to_string())
        .spawn(initThread)
        .map_err(InitError::Spawn)?;
    // init_thread = thr;
    *init_thread = Some(thr);
    Ok(())
}

/// PORT NOTE: the reified error set of `initThread`'s `!void` — the cache-dir
/// lookups' errors + `error.SentryInitFailed`.
#[derive(Debug)]
enum InitThreadError {
    SentryInitFailed,
    /// The `internal_os.xdg.cache` lookup's error.
    Dir(internal_os::xdg::DirError),
    /// The macOS `internal_os.macos.cacheDir` (NSCachesDirectory) error.
    #[cfg(target_os = "macos")]
    AppleApi(internal_os::macos::CacheDirError),
}

// fn initThread(gpa: Allocator) !void {
fn initThread() -> Result<(), InitThreadError> {
    // if (comptime !build_options.sentry) return;
    if !build_options::SENTRY {
        return Ok(());
    }

    // Right now, on Darwin, `std.Thread.setName` can only name the current
    // thread, and we have no way to get the current thread from within it,
    // so instead we use this code to name the thread instead.
    // if (builtin.os.tag.isDarwin()) {
    //     internal_os.macos.pthread_setname_np(&"sentry-init".*);
    // }
    // (PORT NOTE: rustty-os gates its macos module on `target_os = "macos"` — the
    // macOS member of the reference's Darwin set this crate builds for.)
    #[cfg(target_os = "macos")]
    unsafe {
        internal_os::macos::pthread_setname_np(c"sentry-init".as_ptr())
    };

    // var arena = std.heap.ArenaAllocator.init(gpa);
    // defer arena.deinit();
    // const alloc = arena.allocator();
    // (PORT NOTE: the arena collapses to Rust ownership — every allocation below
    // frees on all paths.)

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
            build_config::VERSION_STRING.as_ptr().cast(),
            build_config::VERSION_STRING.len(),
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

    // Determine the Sentry cache directory.
    // (W94-B02-01: raw path bytes — `xdg::cache` is byte-typed; the macOS
    // `cacheDir` arm's UTF-8 `String` converts losslessly via `into_bytes`.)
    let cache_dir: Vec<u8> = 'cache_dir: {
        // On macOS, we prefer to use the NSCachesDirectory value to be
        // a more idiomatic macOS application. But if XDG env vars are set
        // we will respect them.
        // if (comptime builtin.os.tag == .macos) macos: {
        //     if (global.environ().containsUnemptyConstant("XDG_CACHE_HOME")) break :macos;
        //     break :cache_dir try internal_os.macos.cacheDir(
        //         alloc,
        //         "sentry",
        //     );
        // }
        //
        // PORT NOTE: `containsUnemptyConstant`, NOT a bare presence check —
        // upstream treats `XDG_CACHE_HOME=""` as UNSET and still prefers
        // `NSCachesDirectory`. And it reads the CAPTURED global environ so an
        // embedder-supplied block (`global.init(.{ .c = ... })`) is honored,
        // which is also what the `xdg.cache` arm below uses via `environMap`.
        #[cfg(target_os = "macos")]
        {
            if !rustty_global::environ::environ().contains_unempty_constant(b"XDG_CACHE_HOME") {
                break 'cache_dir internal_os::macos::cacheDir("sentry")
                    .map_err(InitThreadError::AppleApi)?
                    .into_bytes();
            }
        }

        // break :cache_dir try internal_os.xdg.cache(
        //     alloc,
        //     .{ .subdir = "ghostty/sentry" },
        // );
        // (R2: the app-name segment of the `"ghostty/sentry"` subdir is
        // sourced from rustty-identity, the identity source-of-truth crate,
        // not hardcoded — same as dir.rs.)
        // PORT NOTE: spelled as the reference's explicit `break :cache_dir`
        // rather than a bare tail expression — off Darwin the `break` above is
        // `cfg`-stripped, and a labelled block whose label is never named warns.
        let subdir = format!("{}/sentry", rustty_identity::APP_NAME);
        break 'cache_dir internal_os::xdg::cache(
            &rustty_global::environ_map(),
            internal_os::xdg::Options {
                subdir: Some(subdir.as_bytes()),
                ..Default::default()
            },
        )
        .map_err(InitThreadError::Dir)?;
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
    let _ = cache_dir;

    Ok(())
}

/// Process-wide deinitialization of our Sentry client. This ensures all
/// our data is flushed.
pub fn deinit() {
    // if (comptime !build_options.sentry) return;
    if !build_options::SENTRY {
        return;
    }

    // if (comptime builtin.os.tag == .windows) return;
    if cfg!(windows) {
        return;
    }

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

// fn beforeSend(
//     event_val: sentry.c.sentry_value_t,
//     _: ?*anyopaque,
//     _: ?*anyopaque,
// ) callconv(.c) sentry.c.sentry_value_t {
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
const fn tagNameRuntime(v: build_config::Runtime) -> &'static str {
    match v {
        build_config::Runtime::None => "none",
        build_config::Runtime::Gtk => "gtk",
    }
}

/// PORT NOTE: Zig `@tagName(build_config.font_backend)` — the reference
/// `font.Backend` enum tag names (src/font/backend.zig), byte-exact.
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
const fn tagNameRenderer(v: build_config::renderer::Backend) -> &'static str {
    match v {
        build_config::renderer::Backend::Opengl => "opengl",
        build_config::renderer::Backend::Metal => "metal",
        build_config::renderer::Backend::Webgl => "webgl",
    }
}

/// PORT NOTE: the reified error set of `Transport.sendInternal`'s `!void` — the
/// envelope parse errors + the crash-dir lookup + the filesystem writes.
// (The payloads are carried for the reference's `log.warn("... err={}", .{err})`
// — currently a comment, like every log line in the port — and for Debug;
// dead-code analysis intentionally ignores derived Debug, hence the allow.)
#[allow(dead_code)]
#[derive(Debug)]
enum SendError {
    Envelope(crate::sentry_envelope::Error),
    Dir(internal_os::xdg::DirError),
    Io(std::io::Error),
}

impl From<crate::sentry_envelope::Error> for SendError {
    fn from(err: crate::sentry_envelope::Error) -> SendError {
        SendError::Envelope(err)
    }
}

impl From<internal_os::xdg::DirError> for SendError {
    fn from(err: internal_os::xdg::DirError) -> SendError {
        SendError::Dir(err)
    }
}

impl From<std::io::Error> for SendError {
    fn from(err: std::io::Error) -> SendError {
        SendError::Io(err)
    }
}

pub struct Transport;

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
            let _ = err;
        }

        unsafe { sentry::Envelope::deinit(envelope) };
    }

    /// Implementation of send but we can use Zig errors.
    // fn sendInternal(envelope: *sentry.Envelope) !void {
    /// # Safety
    ///
    /// `envelope` must be the live envelope pointer `send` received.
    unsafe fn sendInternal(envelope: *mut sentry::Envelope) -> Result<(), SendError> {
        // var arena = std.heap.ArenaAllocator.init(state.alloc);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        // (PORT, break #1: the `state.alloc` read — this file's only
        // rustty-global touch — collapses with the arena into Rust ownership.)

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
            return Ok(());
        }

        // Generate a UUID for this envelope. The envelope DOES have an event_id
        // header but I don't think there is any public API way to get it
        // afaict so we generate a new UUID for the filename just so we don't
        // conflict.
        // const uuid = sentry.UUID.init();
        let uuid = sentry::UUID::init();

        // Get our XDG state directory where we'll store the crash reports.
        // This directory must exist for writing to work.
        // const dir = try crash.defaultDir(alloc);
        let dir = crash::defaultDir()?;
        // try std.fs.cwd().makePath(dir.path);
        // (W94-B02-01: `dir.path` is raw path bytes now — via `OsStr`.)
        std::fs::create_dir_all(crate::dir::os_str_from_bytes(&dir.path))?;

        // Build our final path and write to it.
        // const path = try std.fs.path.join(alloc, &.{
        //     dir.path,
        //     try std.fmt.allocPrint(alloc, "{s}.ghosttycrash", .{uuid.string()}),
        // });
        let uuid_string = uuid.string();
        let path = std::path::Path::new(crate::dir::os_str_from_bytes(&dir.path)).join(format!(
            "{}.{}crash",
            // (PORT NOTE: `uuid.string()` is `[36:0]u8` — `{s}` formats the 36
            // UUID chars before the NUL sentinel.)
            std::str::from_utf8(&uuid_string[..36]).unwrap(),
            // (R2: the `.ghosttycrash` report extension's app-name segment is
            // sourced from rustty-identity, not hardcoded.)
            rustty_identity::APP_NAME,
        ));
        // const file = try std.fs.cwd().createFile(path, .{});
        let mut file = std::fs::File::create(&path)?;
        // defer file.close(); — Drop.
        // var buf: [4096]u8 = undefined;
        // var file_writer = file.writer(&buf);
        // try file_writer.interface.writeAll(json);
        // try file_writer.end();
        // (PORT NOTE: the fixed 4096-byte buffered writer + `end()` flush collapse
        // to one full write.)
        std::io::Write::write_all(&mut file, json)?;

        // log.warn("crash report written to disk path={s}", .{path});
        let _ = path;
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
    use super::*;

    /// rustty-only seam test (rust >= zig; sentry.zig has 0 reference tests):
    /// prove the `ThreadStateSurface` trait seam feeds `beforeSend`'s
    /// Dimensions/tags metadata exactly as the reference's `*Surface` reads
    /// would. PURE value-API only — no `sentry_init`, no transport/backend
    /// thread (the task/reference gate: tests must not start the client).
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

    /// rustty-only: without thread state `beforeSend` must pass the event
    /// through untouched (the reference's early-return arm).
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
}
