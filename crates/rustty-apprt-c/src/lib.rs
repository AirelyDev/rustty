//! rustty-apprt-c
//!
//! PORTS: the libghostty embedding C API — `src/apprt/embedded.zig`'s `CAPI`
//! block + the `src/main_c.zig` C API hub (the `ghostty_*` `@export` surface for
//! `apprt.embedded`). The Rust backends in `rustty-apprt` (the `embedded`/`none`/
//! `browser` modules) are the source of truth; this crate is a pure FFI
//! projection (R3). cdylib+staticlib+rlib; cbindgen -> include/rustty-apprt.h
//! with `rustty_*` symbols (R2).
//!
//! PORT STATUS: in-progress. The crate compiles/links as a cdylib/staticlib/rlib
//! and exports the global-state precursor `rustty_init`. E5 landed the CAPI extern
//! structs (`capi.rs`) with their D2 const-asserts. E6 lands the 13 app-lifecycle +
//! app/config exports (`rustty_app_new`/`_tick`/`_userdata`/`_free`/`_set_focus`/
//! `_key`/`_keyboard_changed`/`_open_config`/`_update_config`/`_needs_confirm_quit`/
//! `_has_global_keybinds`/`_set_color_scheme`, `rustty_config_key_is_binding`) — the
//! runtime-exercisable core (EMBEDDED_BLUEPRINT §1: no renderer/xev leaf). The
//! remaining `ghostty_*` (`rustty_*`) exports land in chunks E7..E10; cbindgen header
//! generation + the C compile+link smoke KAT land in E11 (EMBEDDED_BLUEPRINT §3/§6/§7).
//!
//! PORT NOTE: this is the crate root, the rustty analog of `src/main_c.zig` (the
//! C API hub) together with `apprt.embedded`'s `CAPI` decl. `main_c.zig`
//! `@export`s the C API symbols and references `apprt.runtime.CAPI`; in rustty
//! each C API symbol is authored as a `#[no_mangle] pub unsafe extern "C" fn
//! rustty_<name>(...)` 1:1 with the reference `pub export fn ghostty_<name>` /
//! `@export(&fn, .{ .name = "ghostty_<name>" })` lines (rustty_ prefix per R2).
//! The `extern "C"` wrappers are the single point that crosses the C ABI; they
//! forward to the ported `rustty-apprt` backend.
//!
//! Carved incrementally (R1: every line/comment/test{} ported as the crate is
//! carved, leaves-first, under the acyclicity gate per R4).

use core::ffi::c_void;
use std::mem::ManuallyDrop;

use allocator_api2::alloc::Global;

use libc::{c_char, c_int};

// The concrete embedded backend (`rustty-apprt`): `App` is the runtime `App` the
// exports drive; `Options` is its `#[repr(C)]` host-callback config; `KeyTarget`
// selects app- vs surface-scoped key events.
use rustty_apprt::embedded::{self, App, KeyTarget, Options};
// The `apprt_api::App` TRAIT (`perform_action`) the embedded `App` implements —
// imported under a distinct name so it does not shadow the concrete `App` above.
use rustty_apprt_api::App as ApprtApp;
// `Target`/`Action` (the `performAction` target+action) and `ColorScheme` (the
// `app_set_color_scheme` enum).
use rustty_apprt_api::{Action, ColorScheme, Target};
// `ghostty_app_new`/`_update_config`/`config_key_is_binding` take a `*Config` — the
// `rustty_config::Config` the config C-ABI (`rustty_config_new`) hands the host.
use rustty_config::Config;

// The raw C-API `KeyEvent` (E5, capi.rs) — the wire form `ghostty_app_key` /
// `ghostty_config_key_is_binding` receive, converted to the native form via
// `KeyEvent::keyEvent`.
use crate::capi::KeyEvent;

// const CoreApp = @import("../App.zig");  (embedded.zig:19)
//
// The core App instantiated at `A = embedded::App` — the FIRST concrete backend
// (EMBEDDED_BLUEPRINT §2). `app_new_` creates it (`CoreApp.create`) and the embedded
// `App` stores the stable pointer as `core_app: *CoreApp`.
type CoreApp = rustty_core::app::App<App>;
// const CoreSurface = @import("../Surface.zig");
//
// Only used to name the `C: CoreSurfaceHandle` type parameter of `perform_action`'s
// `Target::App` in `ghostty_app_open_config` (the app target carries no surface, so
// `C` is otherwise unconstrained and must be annotated).
type CoreSurface = rustty_core::surface::Surface<App>;

// The `apprt.embedded.CAPI` block's extern structs (`KeyEvent`/`SurfaceSize`/
// `Text`/`Point`/`Selection` + their D2 const-size/align-asserts). The reference
// nests these in `pub const CAPI = struct { ... }` alongside the exports; the
// exports (E6..E10) are authored as `#[no_mangle]` free fns in this crate, the
// extern structs live in the `capi` module (E5). `ClipboardContent` (a backend
// field type) stays in `rustty-apprt`; `capi` only re-asserts its ABI.
pub mod capi;

// ===================================================================
// C API exports — 1:1 with the reference `pub export fn ghostty_*` /
// `@export(..., .{ .name = "ghostty_*" })` lines (rustty_ prefix per R2).
// cbindgen emits these into include/rustty-apprt.h. The bodies forward to the
// ported `rustty-apprt` backend; the `extern "C"` boundary is the ONLY place
// the C ABI is crossed.
// ===================================================================

/// C: rustty_init
///
/// Initialize rustty global state.
///
/// PORT NOTE: rustty analogue of `ghostty_init` (main_c.zig:105):
/// ```zig
/// pub export fn ghostty_init(argc: usize, argv: [*][*:0]u8) c_int {
///     assert(builtin.link_libc);
///     std.os.argv = argv[0..argc];
///     state.init() catch |err| {
///         std.log.err("failed to initialize ghostty error={}", .{err});
///         return 1;
///     };
///     return 0;
/// }
/// ```
/// The reference `assert(builtin.link_libc)` is a no-op here (Rust's `std`
/// always links libc). The `std.os.argv = argv[0..argc]` store is consumed only
/// by the deferred CLI-action detection inside the full `state.init()` (the
/// BREAK #1 global-state seam, not yet ported — see `rustty_apprt::init`); until
/// that lands, `argv` is accepted but not stored. `state.init()` maps to the
/// `rustty_apprt::init()` bootstrap, which sets `rustty_core::STATE` (the
/// `global.zig` `state` analogue) so the CAPI's `global.alloc`-reading paths
/// (`ghostty_surface_free_text`/`_tty_name`/`Text.deinit`) have a live state.
/// EMBEDDED_BLUEPRINT §3 "Global-state precursor" + risk 4: the smoke KAT (E11)
/// MUST call this before `ghostty_app_new`.
///
/// # Safety
/// `argv` must point to `argc` valid, NUL-terminated C strings (the process
/// argv), as the reference `ghostty_init` requires. This scaffold body does not
/// dereference `argv`, but the contract is fixed now for the deferred
/// argv-storage port.
#[no_mangle]
pub unsafe extern "C" fn rustty_init(argc: usize, argv: *mut *mut c_char) -> c_int {
    // reference: assert(builtin.link_libc);  (Rust `std` always links libc.)
    // reference: std.os.argv = argv[0..argc];
    //   Stored for the deferred CLI-action detection in the full state.init()
    //   (BREAK #1 seam); not yet ported, so accepted-but-unused for now.
    let _ = (argc, argv);

    // reference:
    //   state.init() catch |err| {
    //       std.log.err("failed to initialize ghostty error={}", .{err});
    //       return 1;
    //   };
    //   return 0;
    match rustty_apprt::init() {
        Ok(()) => 0,
        Err(err) => {
            // std.log.err(...) — no scoped-log sink in this leaf (vt-c
            // `rustty_build_info` precedent): emit to stderr.
            eprintln!("failed to initialize rustty error={err:?}");
            1
        }
    }
}

// ===================================================================
// App lifecycle exports — embedded.zig `CAPI` block (embedded.zig:1398-1535),
// chunk E6. The 11 app-lifecycle exports + the 2 app/config exports. These are the
// runtime-EXERCISABLE core (EMBEDDED_BLUEPRINT §1): `app_new` builds `CoreApp` +
// keymap + config clone — NO renderer, NO xev — so the whole group is live NOW (the
// GPU/xev leaf boundary is `ghostty_surface_new`, chunk E7+). Each is authored 1:1
// with the reference `export fn ghostty_*` (rustty_ prefix per R2); the bodies
// forward to the ported `rustty-apprt` embedded backend / core App.
// ===================================================================

/// The error set of [`app_new_`] — the union of `CoreApp::create`'s `CreateError`
/// and [`App::init`]'s `KeymapError` (the reference `app_new_`'s inferred `!*App`
/// error set). Kept as ONE enum so the reference's single `catch |err|` log site in
/// [`rustty_app_new`] is preserved with the concrete `err` value.
//
// PORT NOTE: the payloads ARE read — [`rustty_app_new`] formats the whole error via
// `{err:?}` (the reference `log.err("… err={}", .{err})`) — but derived-`Debug` reads
// are invisible to dead-code analysis (and on non-macOS `KeymapError` is `()`), so the
// lint is allowed here.
#[allow(dead_code)]
#[derive(Debug)]
enum AppNewError {
    // const core_app = try CoreApp.create(global.alloc);
    // (CoreApp.create's error: font.SharedGridSet.InitError || Allocator.Error.)
    Create(rustty_core::app::CreateError),
    // try app.init(core_app, config, opts.*);
    // (App.init's error: input.Keymap.init().)
    Keymap(embedded::KeymapError),
}

/// C: rustty_app_new
///
/// Create a new app.
///
/// # Safety
/// `opts` must point to a valid [`Options`] and `config` to a valid [`Config`]
/// (the config the host built via the config C-ABI); both are read (not retained)
/// for the duration of the call. Returns null on error. The returned pointer, if
/// non-null, must be released with [`rustty_app_free`].
//
// PORT NOTE (improper_ctypes_definitions): the `*const Options` parameter carries
// the `action` callback whose `ActionC`/`TargetC` args transitively hold
// `rustty_input`'s `Mods` (apprt-api's deferred C-ABI pinning); the backend
// silences the same lint on `Options`. Pinned by the E5 D2 const-asserts + cbindgen
// (E11) — faithful to the reference `*const apprt.runtime.App.Options` signature.
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_app_new(opts: *const Options, config: *const Config) -> *mut App {
    // return app_new_(opts, config) catch |err| {
    //     log.err("error initializing app err={}", .{err});
    //     return null;
    // };
    // SAFETY: `opts`/`config` validity is the caller's contract (see # Safety).
    match unsafe { app_new_(opts, config) } {
        Ok(app) => app,
        Err(err) => {
            // no scoped-log sink in this leaf (vt-c precedent): emit to stderr.
            eprintln!("error initializing app err={err:?}");
            core::ptr::null_mut()
        }
    }
}

/// # Safety
/// See [`rustty_app_new`].
//
// fn app_new_(opts, config) !*App { ... }  (embedded.zig:1409)
unsafe fn app_new_(opts: *const Options, config: *const Config) -> Result<*mut App, AppNewError> {
    // const core_app = try CoreApp.create(global.alloc);
    // errdefer core_app.destroy();
    //
    // PORT NOTE: the reference `global.alloc` is the process-global allocator; the
    // rustty seam is Rust's global allocator (`allocator_api2 Global`), matching
    // `rustty_apprt::init`'s `()` alloc seam. `CoreApp::create` returns a
    // `Box<CoreApp>`; `into_raw` yields the stable `*mut CoreApp` the embedded `App`
    // stores (reference `core_app: *CoreApp`). The `errdefer core_app.destroy()` is
    // realized manually on the `App::init` error path below.
    let core_app: *mut CoreApp =
        Box::into_raw(CoreApp::create(Global).map_err(AppNewError::Create)?);

    // Create our runtime app
    // var app = try global.alloc.create(App);
    // errdefer global.alloc.destroy(app);
    // try app.init(core_app, config, opts.*);
    // errdefer app.terminate();
    //
    // PORT NOTE: rustty `App::init` RETURNS the value (no placement-init), so the
    // reference's `alloc.create(App)` uninitialized slot + its `errdefer destroy`
    // collapse into boxing the returned value below (boxing is infallible, so the
    // `errdefer app.terminate()` never fires). `opts.*` copies the `Options` out of
    // the caller's pointer — `ptr::read` (Options is `#[repr(C)]`, trivially
    // copyable). On an `App::init` error the `errdefer core_app.destroy()` fires:
    // reconstruct the `Box<CoreApp>` and `destroy` it.
    // SAFETY: `opts`/`config` validity is the caller's contract.
    let opts_val = unsafe { core::ptr::read(opts) };
    let app = match App::init(core_app, unsafe { &*config }, opts_val) {
        Ok(app) => app,
        Err(err) => {
            // errdefer core_app.destroy();
            unsafe { CoreApp::destroy(Box::from_raw(core_app)) };
            return Err(AppNewError::Keymap(err));
        }
    };

    // return app;
    Ok(Box::into_raw(Box::new(app)))
}

/// C: rustty_app_tick
///
/// Tick the event loop. This should be called whenever the "wakeup"
/// callback is invoked for the runtime.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`] and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn rustty_app_tick(v: *mut App) {
    // v.core_app.tick(v) catch |err| {
    //     log.err("error app tick err={}", .{err});
    // };
    //
    // PORT NOTE: `core_app` (a separate heap allocation) and the rt-app `&*v` never
    // alias, so the `&mut CoreApp` receiver + `&App` argument are sound.
    // SAFETY: `v` validity is the caller's contract.
    let core_app = unsafe { (*v).core_app };
    if let Err(err) = unsafe { (*core_app).tick(&*v) } {
        eprintln!("error app tick err={err:?}");
    }
}

/// C: rustty_app_userdata
///
/// Return the userdata associated with the app.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_userdata(v: *mut App) -> *mut c_void {
    // return v.opts.userdata;
    // SAFETY: `v` validity is the caller's contract.
    unsafe { (*v).opts.userdata }
}

/// C: rustty_app_free
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`]; it is invalid after
/// this call returns.
#[no_mangle]
pub unsafe extern "C" fn rustty_app_free(v: *mut App) {
    // const core_app = v.core_app;
    // SAFETY: `v` validity is the caller's contract.
    let core_app = unsafe { (*v).core_app };
    // v.terminate();
    unsafe { (*v).terminate() };
    // global.alloc.destroy(v);
    //
    // PORT NOTE: `terminate()` already tore down the App's keymap+config; Zig's
    // `destroy` then frees the raw memory WITHOUT running any destructor. `ManuallyDrop`
    // frees the `App` box without re-dropping its (already-deinit'd) fields — the
    // `closeSurface` precedent (embedded.rs).
    unsafe { drop(Box::from_raw(v as *mut ManuallyDrop<App>)) };
    // core_app.destroy();
    unsafe { CoreApp::destroy(Box::from_raw(core_app)) };
}

/// C: rustty_app_set_focus
///
/// Update the focused state of the app.
///
/// # Safety
/// `app` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_set_focus(app: *mut App, focused: bool) {
    // app.focusEvent(focused);
    // SAFETY: `app` validity is the caller's contract.
    unsafe { (*app).focusEvent(focused) };
}

/// C: rustty_app_key
///
/// Notify the app of a global keypress capture. This will return
/// true if the key was captured by the app, in which case the caller
/// should not process the key.
///
/// # Safety
/// `app` must be valid; `event.text`, if non-null, must be a valid NUL-terminated
/// C string that outlives the call.
#[no_mangle]
pub unsafe extern "C" fn rustty_app_key(app: *mut App, event: KeyEvent) -> bool {
    // return app.keyEvent(.app, event.keyEvent()) catch |err| {
    //     log.warn("error processing key event err={}", .{err});
    //     return false;
    // };
    // SAFETY: `event.text` validity is the caller's contract (KeyEvent::keyEvent).
    let kev = unsafe { event.keyEvent() };
    // SAFETY: `app` validity is the caller's contract.
    match unsafe { (*app).keyEvent(KeyTarget::App, kev) } {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error processing key event err={err:?}");
            false
        }
    }
}

/// C: rustty_config_key_is_binding
///
/// Returns true if the given key event would trigger a binding
/// if it were sent to the surface right now. The "right now"
/// is important because things like trigger sequences are only
/// valid until the next key event.
///
/// # Safety
/// `config` must be a valid [`Config`] pointer; `event.text`, if non-null, must be
/// a valid NUL-terminated C string that outlives the call.
#[no_mangle]
pub unsafe extern "C" fn rustty_config_key_is_binding(
    config: *mut Config,
    event: KeyEvent,
) -> bool {
    // const core_event = event.keyEvent().core() orelse {
    //     log.warn("error processing key event", .{});
    //     return false;
    // };
    // SAFETY: `event.text` validity is the caller's contract (KeyEvent::keyEvent).
    let core_event = match unsafe { event.keyEvent() }.core() {
        Some(v) => v,
        None => {
            eprintln!("error processing key event");
            return false;
        }
    };

    // return config.keyEventIsBinding(core_event);
    //
    // PORT NOTE: `keyEventIsBinding` -> `key_event_is_binding` (rustty-config's
    // snake_case method convention).
    // SAFETY: `config` validity is the caller's contract.
    unsafe { (*config).key_event_is_binding(core_event) }
}

/// C: rustty_app_keyboard_changed
///
/// Notify the app that the keyboard was changed. This causes the
/// keyboard layout to be reloaded from the OS.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_keyboard_changed(v: *mut App) {
    // v.reloadKeymap() catch |err| {
    //     log.err("error reloading keyboard map err={}", .{err});
    //     return;
    // };
    // SAFETY: `v` validity is the caller's contract.
    if let Err(err) = unsafe { (*v).reloadKeymap() } {
        eprintln!("error reloading keyboard map err={err:?}");
    }
}

/// C: rustty_app_open_config
///
/// Open the configuration.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_open_config(v: *mut App) {
    // _ = v.performAction(.app, .open_config, {}) catch |err| {
    //     log.err("error reloading config err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: the reference `*App` method `performAction(.app, .open_config, {})`
    // maps to the `apprt_api::App` trait's `perform_action(Target::App,
    // Action::OpenConfig)` (the embedded `App` is the first concrete backend). The
    // `Target::App` carries no surface, so its `C: CoreSurfaceHandle` type parameter
    // is annotated as `CoreSurface`. The `bool` result is discarded (reference `_ =`).
    // SAFETY: `v` validity is the caller's contract.
    if let Err(err) = unsafe { (*v).perform_action(Target::<CoreSurface>::App, Action::OpenConfig) }
    {
        eprintln!("error reloading config err={err:?}");
    }
}

/// C: rustty_app_update_config
///
/// Update the configuration to the provided config. This will propagate
/// to all surfaces as well.
///
/// # Safety
/// `v` must be valid; `config` must point to a valid [`Config`] the caller owns
/// (it may be freed as soon as this returns).
#[no_mangle]
pub unsafe extern "C" fn rustty_app_update_config(v: *mut App, config: *const Config) {
    // v.core_app.updateConfig(v, config) catch |err| {
    //     log.err("error updating config err={}", .{err});
    //     return;
    // };
    // SAFETY: `v`/`config` validity is the caller's contract.
    let core_app = unsafe { (*v).core_app };
    if let Err(err) = unsafe { (*core_app).updateConfig(&*v, &*config) } {
        eprintln!("error updating config err={err:?}");
    }
}

/// C: rustty_app_needs_confirm_quit
///
/// Returns true if the app needs to confirm quitting.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_needs_confirm_quit(v: *mut App) -> bool {
    // return v.core_app.needsConfirmQuit();
    // SAFETY: `v` validity is the caller's contract.
    unsafe { (*(*v).core_app).needsConfirmQuit() }
}

/// C: rustty_app_has_global_keybinds
///
/// Returns true if the app has global keybinds.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_has_global_keybinds(v: *mut App) -> bool {
    // return v.hasGlobalKeybinds();
    // SAFETY: `v` validity is the caller's contract.
    unsafe { (*v).hasGlobalKeybinds() }
}

/// C: rustty_app_set_color_scheme
///
/// Update the color scheme of the app.
///
/// # Safety
/// `v` must be a valid pointer returned by [`rustty_app_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_app_set_color_scheme(v: *mut App, scheme_raw: c_int) {
    // const scheme = std.meta.intToEnum(apprt.ColorScheme, scheme_raw) catch {
    //     log.warn(
    //         "invalid color scheme to ghostty_surface_set_color_scheme value={}",
    //         .{scheme_raw},
    //     );
    //     return;
    // };
    //
    // PORT NOTE: `apprt.ColorScheme` is `enum(u2) { light = 0, dark = 1 }`; the
    // `intToEnum` guard lives here on the raw `c_int` crossing the C boundary (vt-c
    // `rustty_build_info` precedent). The reference warn string names
    // `ghostty_surface_set_color_scheme` (a ghostty copy-paste quirk inside the
    // app-scoped fn) — preserved verbatim in the comment above; the emitted line
    // uses the fn's own rustty symbol.
    let scheme = match scheme_raw {
        0 => ColorScheme::Light,
        1 => ColorScheme::Dark,
        _ => {
            eprintln!("invalid color scheme to rustty_app_set_color_scheme value={scheme_raw}");
            return;
        }
    };

    // v.core_app.colorSchemeEvent(v, scheme) catch |err| {
    //     log.err("error setting color scheme err={}", .{err});
    //     return;
    // };
    // SAFETY: `v` validity is the caller's contract.
    let core_app = unsafe { (*v).core_app };
    if let Err(err) = unsafe { (*core_app).colorSchemeEvent(&*v, scheme) } {
        eprintln!("error setting color scheme err={err:?}");
    }
}
