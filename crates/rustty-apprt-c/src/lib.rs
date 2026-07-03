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
//! runtime-exercisable core (EMBEDDED_BLUEPRINT §1: no renderer/xev leaf). E7 lands
//! the surface lifecycle + state/query + render/window exports
//! (`rustty_surface_config_new`/`_new`/`_free`/`_userdata`/`_app`/`_inherited_config`/
//! `_update_config`/`_needs_confirm_quit`/`_process_exited`/`_has_selection`/
//! `_read_selection`/`_read_text`/`_free_text`/`_size`/`_foreground_pid`/`_tty_name`/
//! `_refresh`/`_draw`/`_set_size`/`_set_color_scheme`/`_set_content_scale`/
//! `_set_focus`/`_set_occlusion`) + the `main_c.zig` `String`/`rustty_string_free`
//! precursor `_tty_name` returns. Only `_config_new` is runtime-live; every other E7
//! export needs a live surface, so it reaches the Phase-3 GPU leaf past
//! `rustty_surface_new` (compiles + links now, panics when driven). E8 lands the
//! input exports (key/text/preedit/mouse); E9 lands the surface-action exports
//! (`_request_close`/`_split`/`_split_focus`/`_split_resize`/`_split_equalize`/
//! `_binding_action`), the `_complete_clipboard_request` sink, and the 10 inspector
//! exports (`_surface_inspector`/`_inspector_free`/`_set_size`/`_set_content_scale`/
//! `_mouse_button`/`_mouse_pos`/`_mouse_scroll`/`_key`/`_text`/`_set_focus`). All of
//! E8/E9 need a live surface (and the per-inspector sinks a live inspector, itself a
//! Phase-3 dcimgui leaf), so they link now and reach the leaf only when driven. The
//! remaining `ghostty_*` (`rustty_*`) exports land in chunk E10 (misc + Darwin-gated);
//! cbindgen header generation + the C compile+link smoke KAT land in E11
//! (EMBEDDED_BLUEPRINT §3/§6/§7).
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

use core::ffi::{c_void, CStr};
use std::mem::ManuallyDrop;

use allocator_api2::alloc::Global;

use libc::{c_char, c_int};

// The concrete embedded backend (`rustty-apprt`): `App` is the runtime `App` the
// exports drive; `Options` is its `#[repr(C)]` host-callback config; `KeyTarget`
// selects app- vs surface-scoped key events. `Surface`/`SurfaceOptions` are the
// backend surface + its `ghostty_surface_config_s` options (E7 surface lifecycle);
// `PlatformC`/`PlatformCMacOS` build the zeroed platform union for the default
// `ghostty_surface_config_new`.
use rustty_apprt::embedded::{
    self, App, Inspector, KeyTarget, Options, PlatformC, PlatformCMacOS, Surface, SurfaceOptions,
};
// The `apprt_api::App` TRAIT (`perform_action`) the embedded `App` implements —
// imported under a distinct name so it does not shadow the concrete `App` above.
use rustty_apprt_api::App as ApprtApp;
// `Target`/`Action` (the `performAction` target+action) and `ColorScheme` (the
// `app_set_color_scheme` enum). `NewSurfaceContext` is the inherited-config context
// (`ghostty_surface_inherited_config`); `ApprtError` is the `App::newSurface`
// error the `ghostty_surface_new` path logs.
use rustty_apprt_api::{Action, ApprtError, ColorScheme, NewSurfaceContext, Target};
// The split-action value types the E9 surface-action exports pass BY VALUE across
// the C ABI (`ghostty_surface_split`/`_split_focus`/`_split_resize` take
// `apprt.action.{SplitDirection,GotoSplit,ResizeSplit.Direction}` directly). Each is
// an FFI-safe `#[repr(i32)]`/`#[repr(C)]` type (apprt-api action.rs); `ResizeSplit`
// is reassembled from its `{direction, amount}` params. (`ResizeSplit.Direction` is
// hoisted to the sibling `ResizeSplitDirection` — Rust cannot nest a type in a
// struct's value namespace.)
use rustty_apprt_api::action::{GotoSplit, ResizeSplit, ResizeSplitDirection, SplitDirection};
// `apprt.ClipboardRequest` — the heap request `ghostty_surface_complete_clipboard_request`
// receives back as `*mut ClipboardRequest` (the `Box` from `Surface::clipboard_request`, E3).
use rustty_apprt_api::ClipboardRequest;
// `ghostty_app_new`/`_update_config`/`config_key_is_binding` take a `*Config` — the
// `rustty_config::Config` the config C-ABI (`rustty_config_new`) hands the host.
use rustty_config::Config;
// The core `Text.Viewport` (the read-text viewport metadata `readTextLocked` reads
// off `dumpTextLocked`'s result).
use rustty_core::surface::TextViewport;
// The core `terminal.Selection` `readTextLocked` receives (resolved by the caller
// from the active screen's selection or the wire `Selection`).
use rustty_vt::selection::Selection as CoreSelection;
// The E8 input exports' mods/mouse wire types (all `input.*` in the reference):
// `Mods`/`ModsBacking` reinterpret the raw `c_int` mods a host passes
// (`ghostty_surface_{key_translation_mods,mouse_button,mouse_pos}`); `mouse`'s
// `ButtonState`/`Button`/`PressureStage`/`ScrollMods` are the mouse-event value
// types (`ghostty_surface_mouse_{button,scroll,pressure}`).
use rustty_vt::input::key;
use rustty_vt::input::key_mods::{Mods, ModsBacking};
use rustty_vt::input::mouse;
// `input.Binding.Flags.C` (`ghostty_surface_key_is_binding`'s out-param, written
// from `Flags.cval()`) and `input.Mods.translation` (`ghostty_surface_key_translation_mods`);
// `translation` is a `rustty-input` free fn (it needs the apprt-layer `OptionAsAlt`).
// `binding` (the module) is also brought in for `input.Binding.Action.parse` — the
// E9 `ghostty_surface_binding_action` parser; it is module-qualified (`binding::Action`)
// to avoid clashing with the apprt-api `Action` imported above.
use rustty_input::binding::{self, FlagsC};
use rustty_input::key_mods::translation;

// The raw C-API `KeyEvent` (E5, capi.rs) — the wire form `ghostty_app_key` /
// `ghostty_config_key_is_binding` receive, converted to the native form via
// `KeyEvent::keyEvent`. E7 adds `Selection`/`SurfaceSize`/`Text` — the wire structs
// the surface read/query exports consume/return.
use crate::capi::{KeyEvent, Selection, SurfaceSize, Text};

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

// ===================================================================
// main_c.zig precursor — the `String` (`ghostty_string_s`) C hub type +
// `ghostty_string_free`. embedded.zig imports it (`const String =
// @import("../main_c.zig").String;`, embedded.zig:23) and E7's
// `ghostty_surface_tty_name` returns it. `main_c.zig` is the top-level C API hub
// this crate root represents; its items are folded in as the exports that need
// them land (the E0 `rustty_init` precedent). rustty-config ships its own local
// deferred projection of this same struct (`c_api.rs`, no `deinit`); this is the
// canonical `main_c` port with the freeing pair (ADV-FIX 9 pub-widening).
// ===================================================================

/// PORTS: `main_c.zig`'s `pub const String = extern struct { ... }`
/// (`ghostty_string_s`) — a length-prefixed byte string handed to the C host and
/// reclaimed via [`rustty_string_free`].
//
// pub const String = extern struct {
//     ptr: ?[*]const u8,
//     len: usize,
//     sentinel: bool,
//     ...
// };
//
// PORT NOTE: the std prelude `String` is never used in this crate root (the config
// c_api precedent fully-qualifies `std::string::String` where it needs it), so this
// extern projection safely takes the `String` name to match the reference type.
// `?[*]const u8` → the raw `*const u8` the rustty CAPI convention uses for a nullable
// pointer (null == empty).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct String {
    // ptr: ?[*]const u8,
    pub ptr: *const u8,
    // len: usize,
    pub len: usize,
    // sentinel: bool,
    pub sentinel: bool,
}

impl String {
    // pub const empty: String = .{ .ptr = null, .len = 0, .sentinel = false };
    fn empty() -> String {
        String {
            ptr: core::ptr::null(),
            len: 0,
            sentinel: false,
        }
    }

    // pub fn fromSlice(slice: anytype) String { ... }
    //
    // PORT NOTE: the reference `fromSlice` records a slice's `.ptr`/`.len` and
    // derives `sentinel` from the slice TYPE (`[:0]` → true). Rust has no
    // compile-time slice-sentinel reflection, so `sentinel` is passed explicitly.
    // The caller's owned bytes (the reference `dupeZ` copy) are taken by value and
    // leaked here (fusing `dupeZ` + `fromSlice` + the sentinel append — the config
    // c_api `from_path` precedent) so the pointer stays valid for the C host, which
    // reclaims the allocation via [`rustty_string_free`]. For a sentinel string the
    // backing allocation is `len + 1` bytes (data + trailing NUL) with `len`
    // excluding the sentinel, matching the reference `[:0]const u8` convention (and
    // the E5 `Text` NUL-termination discipline).
    fn from_slice(mut v: Vec<u8>, sentinel: bool) -> String {
        let len = v.len();
        if sentinel {
            v.push(0);
        }
        let ptr = Box::leak(v.into_boxed_slice()).as_ptr();
        String { ptr, len, sentinel }
    }

    // pub fn deinit(self: *const String) void {
    //     const ptr = self.ptr orelse return;
    //     if (self.sentinel) {
    //         state.alloc.free(ptr[0..self.len :0]);
    //     } else {
    //         state.alloc.free(ptr[0..self.len]);
    //     }
    // }
    //
    // PORT NOTE: the rustty global-alloc seam is Rust's global allocator (the E5
    // `Text::deinit` precedent), so `state.alloc.free` reconstructs and drops the
    // leaked `Box<[u8]>` produced by [`String::from_slice`]. A sentinel string frees
    // `len + 1` bytes (`ptr[0..len :0]` — data + NUL); a plain string frees `len`.
    fn deinit(&self) {
        // const ptr = self.ptr orelse return;
        if self.ptr.is_null() {
            return;
        }
        // SAFETY: per the producer contract `self.ptr` is the start of a not-yet-freed
        // `Box<[u8]>` of `len + 1` (sentinel) or `len` (plain) bytes; reconstruct and
        // drop it to free via the global allocator.
        unsafe {
            let n = if self.sentinel {
                self.len + 1
            } else {
                self.len
            };
            drop(Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                self.ptr as *mut u8,
                n,
            )));
        }
    }
}

/// C: rustty_string_free
///
/// Free a string allocated by Rustty.
///
/// PORT NOTE: rustty analogue of `main_c.zig`'s
/// `pub export fn ghostty_string_free(str: String) void { str.deinit(); }`
/// (main_c.zig:155). The `String` is passed by value (it is `#[repr(C)]` + `Copy`);
/// [`String::deinit`] frees the backing allocation. The producing export
/// ([`rustty_surface_tty_name`]) documents this as the required free.
///
/// # Safety
/// `str` must be a `String` returned by a rustty export (e.g.
/// [`rustty_surface_tty_name`]) and not already freed; it is invalid afterwards.
#[no_mangle]
pub unsafe extern "C" fn rustty_string_free(str: String) {
    // str.deinit();
    str.deinit();
}

// ===================================================================
// Surface lifecycle + state/query + render/window exports — embedded.zig `CAPI`
// block (embedded.zig:1538-1760), chunk E7. `ghostty_surface_config_new` (a pure
// zeroed/defaulted `Surface.Options`) is runtime-live; every other export needs a
// LIVE surface, which can only exist past `ghostty_surface_new` → core
// `Surface::init` → the Phase-3 `renderer_seam::{Renderer,Thread}` leaf. So these
// exports COMPILE + LINK + header-generate NOW; `refresh`/`draw`/`set_size`/… reach
// the GPU leaf and PANIC when driven against a real surface (EMBEDDED_BLUEPRINT §1,
// the honest runtime boundary). Authored 1:1 with the reference `export fn
// ghostty_surface_*` (rustty_ prefix per R2); bodies forward to the ported backend.
// ===================================================================

/// C: rustty_surface_config_new
///
/// # Safety
/// Pure value constructor — no pointers are dereferenced. Marked `unsafe extern`
/// only for signature uniformity with the rest of the C API.
//
// export fn ghostty_surface_config_new() apprt.Surface.Options {
//     return .{};
// }
//
// PORT NOTE: the reference `.{}` default-initializes `Surface.Options` from its Zig
// field defaults. Rust has no struct field defaults, so those exact defaults are
// spelled out below (the E3 `newSurfaceOptions` precedent, which does the same for
// its elided fields). `platform` is reference `undefined` (never read — the C host
// overwrites `platform`/`platform_tag` before `ghostty_surface_new`); a zeroed union
// is the faithful placeholder.
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_config_new() -> SurfaceOptions {
    // return .{};
    SurfaceOptions {
        // platform_tag: c_int = 0,
        platform_tag: 0,
        // platform: Platform.C = undefined,
        platform: PlatformC {
            macos: PlatformCMacOS {
                nsview: core::ptr::null_mut(),
            },
        },
        // userdata: ?*anyopaque = null,
        userdata: core::ptr::null_mut(),
        // scale_factor: f64 = 1,
        scale_factor: 1.0,
        // font_size: f32 = 0,
        font_size: 0.0,
        // working_directory: ?[*:0]const u8 = null,
        working_directory: core::ptr::null(),
        // command: ?[*:0]const u8 = null,
        command: core::ptr::null(),
        // env_vars: ?[*]EnvVar = null,
        env_vars: core::ptr::null(),
        // env_var_count: usize = 0,
        env_var_count: 0,
        // initial_input: ?[*:0]const u8 = null,
        initial_input: core::ptr::null(),
        // wait_after_command: bool = false,
        wait_after_command: false,
        // context: apprt.surface.NewSurfaceContext = .window,
        context: NewSurfaceContext::Window,
    }
}

/// C: rustty_surface_new
///
/// Create a new surface as part of an app.
///
/// # Safety
/// `app` must be a valid pointer returned by [`rustty_app_new`]; `opts` must point
/// to a valid [`SurfaceOptions`]. Returns null on error. The returned pointer, if
/// non-null, is a heap-boxed + pinned surface (ADV-FIX 8) released with
/// [`rustty_surface_free`].
//
// PORT NOTE (improper_ctypes_definitions): `*const SurfaceOptions` carries raw
// pointers + the `Platform.C` union; pinned by the E5/E2 D2 const-asserts + cbindgen
// (E11), faithful to the reference `*const apprt.Surface.Options` signature.
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_new(
    app: *mut App,
    opts: *const SurfaceOptions,
) -> *mut Surface {
    // return surface_new_(app, opts) catch |err| {
    //     log.err("error initializing surface err={}", .{err});
    //     return null;
    // };
    // SAFETY: `app`/`opts` validity is the caller's contract.
    match unsafe { surface_new_(app, opts) } {
        Ok(surface) => surface,
        Err(err) => {
            // no scoped-log sink in this leaf (vt-c precedent): emit to stderr.
            eprintln!("error initializing surface err={err:?}");
            core::ptr::null_mut()
        }
    }
}

/// # Safety
/// See [`rustty_surface_new`].
//
// fn surface_new_(app: *App, opts: *const apprt.Surface.Options) !*Surface {
//     return try app.newSurface(opts.*);
// }
unsafe fn surface_new_(
    app: *mut App,
    opts: *const SurfaceOptions,
) -> Result<*mut Surface, ApprtError> {
    // return try app.newSurface(opts.*);
    //
    // PORT NOTE: `opts.*` copies the `Surface.Options` out of the caller's pointer
    // (`ptr::read` — `SurfaceOptions` is `#[repr(C)]`, trivially copyable).
    // `App::newSurface` heap-boxes + pins the backend `Surface` (ADV-FIX 8).
    // SAFETY: `app`/`opts` validity is the caller's contract.
    unsafe { (*app).newSurface(core::ptr::read(opts)) }
}

/// C: rustty_surface_free
///
/// # Safety
/// `ptr` must be a valid surface returned by [`rustty_surface_new`]; it is invalid
/// after this call returns.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_free(ptr: *mut Surface) {
    // ptr.app.closeSurface(ptr);
    //
    // PORT NOTE: capture the `app` raw pointer BEFORE handing `ptr` to
    // `closeSurface` (raw-pointer args, the `&mut`-args-strongly-protected
    // discipline). `closeSurface` deinits + frees the boxed surface.
    // SAFETY: `ptr` validity is the caller's contract.
    let app = unsafe { (*ptr).app };
    unsafe { (*app).closeSurface(ptr) };
}

/// C: rustty_surface_userdata
///
/// Returns the userdata associated with the surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_userdata(surface: *mut Surface) -> *mut c_void {
    // return surface.userdata;
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).userdata }
}

/// C: rustty_surface_app
///
/// Returns the app associated with a surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_app(surface: *mut Surface) -> *mut App {
    // return surface.app;
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).app }
}

/// C: rustty_surface_inherited_config
///
/// Returns the config to use for surfaces that inherit from this one.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_inherited_config(
    surface: *mut Surface,
    source: NewSurfaceContext,
) -> SurfaceOptions {
    // return surface.newSurfaceOptions(source);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).newSurfaceOptions(source) }
}

/// C: rustty_surface_update_config
///
/// Update the configuration to the provided config for only this surface.
///
/// # Safety
/// `surface` must be valid; `config` must point to a valid [`Config`] the caller
/// owns (it may be freed as soon as this returns).
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_update_config(
    surface: *mut Surface,
    config: *const Config,
) {
    // surface.core_surface.updateConfig(config) catch |err| {
    //     log.err("error updating config err={}", .{err});
    //     return;
    // };
    // SAFETY: `surface`/`config` validity is the caller's contract.
    if let Err(err) = unsafe { (*surface).core_surface.updateConfig(&*config) } {
        eprintln!("error updating config err={err:?}");
    }
}

/// C: rustty_surface_needs_confirm_quit
///
/// Returns true if the surface needs to confirm quitting.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_needs_confirm_quit(surface: *mut Surface) -> bool {
    // return surface.core_surface.needsConfirmQuit();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.needsConfirmQuit() }
}

/// C: rustty_surface_process_exited
///
/// Returns true if the surface process has exited.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_process_exited(surface: *mut Surface) -> bool {
    // return surface.core_surface.child_exited;
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.child_exited }
}

/// C: rustty_surface_has_selection
///
/// Returns true if the surface has a selection.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_has_selection(surface: *mut Surface) -> bool {
    // return surface.core_surface.hasSelection();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.hasSelection() }
}

/// C: rustty_surface_read_selection
///
/// Same as rustty_surface_read_text but reads from the user selection,
/// if any.
///
/// # Safety
/// `surface` must be valid; `result` must point to a writable [`Text`]. On `true`
/// the written `Text` owns a heap C-string freed via [`rustty_surface_free_text`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_read_selection(
    surface: *mut Surface,
    result: *mut Text,
) -> bool {
    // const core_surface = &surface.core_surface;
    // SAFETY: `surface` validity is the caller's contract.
    let core_surface: *mut CoreSurface =
        unsafe { core::ptr::addr_of_mut!((*surface).core_surface) };

    // core_surface.renderer_state.mutex.lock();
    // defer core_surface.renderer_state.mutex.unlock();
    //
    // PORT NOTE: `renderer_state.mutex` is a `*mut Mutex`; the `defer unlock` runs on
    // every scope exit — realized here by computing the result into a local and
    // unlocking once before the single return (both the no-selection and the
    // read arms flow through it).
    let mutex = unsafe { (*core_surface).renderer_state.mutex };
    unsafe { (*mutex).lock() };

    // If we don't have a selection, do nothing.
    // const core_sel = core_surface.io.terminal.screens.active.selection orelse return false;
    let active = unsafe { (*core_surface).io.terminal.screens.active };
    let out = match unsafe { (*active).selection } {
        // (no selection → `orelse return false`)
        None => false,
        // Read the text from the selection.
        // return readTextLocked(surface, core_sel, result);
        Some(core_sel) => unsafe { readTextLocked(surface, core_sel, result) },
    };

    unsafe { (*mutex).unlock() };
    out
}

/// C: rustty_surface_read_text
///
/// Read some arbitrary text from the surface.
///
/// This is an expensive operation so it shouldn't be called too
/// often. We recommend that callers cache the result and throttle
/// calls to this function.
///
/// # Safety
/// `surface` must be valid; `result` must point to a writable [`Text`]. On `true`
/// the written `Text` owns a heap C-string freed via [`rustty_surface_free_text`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_read_text(
    surface: *mut Surface,
    sel: Selection,
    result: *mut Text,
) -> bool {
    // surface.core_surface.renderer_state.mutex.lock();
    // defer surface.core_surface.renderer_state.mutex.unlock();
    // SAFETY: `surface` validity is the caller's contract.
    let core_surface: *mut CoreSurface =
        unsafe { core::ptr::addr_of_mut!((*surface).core_surface) };
    let mutex = unsafe { (*core_surface).renderer_state.mutex };
    unsafe { (*mutex).lock() };

    // const core_sel = sel.core(
    //     surface.core_surface.renderer_state.terminal.screens.active,
    // ) orelse return false;
    //
    // PORT NOTE: `sel.core` (E5 `Selection::core`) borrows the active screen through
    // `renderer_state.terminal` (a `*mut Terminal`); the `defer unlock` is realized
    // as in `rustty_surface_read_selection` (single unlock before return).
    let active = unsafe { (*(*core_surface).renderer_state.terminal).screens.active };
    let out = match sel.core(unsafe { &*active }) {
        None => false,
        // return readTextLocked(surface, core_sel, result);
        Some(core_sel) => unsafe { readTextLocked(surface, core_sel, result) },
    };

    unsafe { (*mutex).unlock() };
    out
}

/// # Safety
/// The caller must hold `core_surface.renderer_state.mutex`; `surface`/`result`
/// must be valid (see [`rustty_surface_read_text`]).
//
// fn readTextLocked(surface: *Surface, core_sel: terminal.Selection, result: *Text) bool { ... }
//
// PORT NOTE (non_snake_case): the reference identifier `readTextLocked` is preserved
// 1:1 (R2/R3); silence the snake-case lint at this fn.
#[allow(non_snake_case)]
unsafe fn readTextLocked(
    surface: *mut Surface,
    core_sel: CoreSelection,
    result: *mut Text,
) -> bool {
    // const core_surface = &surface.core_surface;
    // SAFETY: `surface` validity is the caller's contract.
    let core_surface: *mut CoreSurface =
        unsafe { core::ptr::addr_of_mut!((*surface).core_surface) };

    // Get our text directly from the core surface.
    // const text = core_surface.dumpTextLocked(global.alloc, core_sel) catch |err| {
    //     log.warn("error reading text err={}", .{err});
    //     return false;
    // };
    //
    // PORT NOTE: the reference `global.alloc` is the process-global allocator; the
    // rustty seam is Rust's global allocator (`allocator_api2 Global`). The returned
    // `text.text` is a `Vec<u8>` whose length EXCLUDES the sentinel (the vt
    // `selection_string` convention); the E5 `Text::deinit` frees `text_len + 1`
    // bytes, so a trailing NUL is appended + the buffer leaked below.
    let mut text = match unsafe { (*core_surface).dumpTextLocked(Global, core_sel) } {
        Ok(text) => text,
        Err(err) => {
            // log.warn("error reading text err={}", .{err});
            eprintln!("error reading text err={err:?}");
            return false;
        }
    };

    // const vp: CoreSurface.Text.Viewport = text.viewport orelse .{
    //     .tl_px_x = -1,
    //     .tl_px_y = -1,
    //     .offset_start = 0,
    //     .offset_len = 0,
    // };
    let vp: TextViewport = text.viewport.unwrap_or(TextViewport {
        tl_px_x: -1.0,
        tl_px_y: -1.0,
        offset_start: 0,
        offset_len: 0,
    });

    // result.* = .{
    //     .tl_px_x = vp.tl_px_x,
    //     .tl_px_y = vp.tl_px_y,
    //     .offset_start = vp.offset_start,
    //     .offset_len = vp.offset_len,
    //     .text = text.text.ptr,
    //     .text_len = text.text.len,
    // };
    //
    // PORT NOTE: the reference `[:0]const u8` allocation already carries the sentinel;
    // the rustty `Vec<u8>` does not, so we push the NUL and leak the `Box<[u8]>` (of
    // exactly `text_len + 1` bytes) that the E5 `Text::deinit` reconstructs. `text_len`
    // is the DATA length (pre-push), matching the reference `text.text.len`.
    let text_len = text.text.len();
    // text.text.ptr / text.text.len  (append the sentinel + leak for the C host)
    text.text.push(0);
    let ptr = Box::leak(text.text.into_boxed_slice()).as_ptr();
    // SAFETY: `result` is a valid writable `Text` (caller contract).
    unsafe {
        *result = Text {
            tl_px_x: vp.tl_px_x,
            tl_px_y: vp.tl_px_y,
            offset_start: vp.offset_start,
            offset_len: vp.offset_len,
            text: ptr as *const c_char,
            text_len,
        };
    }

    // return true;
    true
}

/// C: rustty_surface_free_text
///
/// # Safety
/// `ptr` must point to a [`Text`] previously written by [`rustty_surface_read_text`]/
/// [`rustty_surface_read_selection`] and not already freed. `_surface` is unused.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_free_text(_surface: *mut Surface, ptr: *mut Text) {
    // ptr.deinit();
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).deinit() };
}

/// C: rustty_surface_refresh
///
/// Tell the surface that it needs to schedule a render
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_refresh(surface: *mut Surface) {
    // surface.refresh();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).refresh() };
}

/// C: rustty_surface_draw
///
/// Tell the surface that it needs to schedule a render
/// call as soon as possible (NOW if possible).
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
//
// PORT NOTE: `draw` reaches the Phase-3 GPU renderer leaf at RUNTIME (backend
// `Surface::draw` → core `Surface::draw` → `renderer_seam` `unimplemented!()`); it
// compiles + links now and panics only when driven against a live surface.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_draw(surface: *mut Surface) {
    // surface.draw();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).draw() };
}

/// C: rustty_surface_set_size
///
/// Update the size of a surface. This will trigger resize notifications
/// to the pty and the renderer.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_size(surface: *mut Surface, w: u32, h: u32) {
    // surface.updateSize(w, h);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).updateSize(w, h) };
}

/// C: rustty_surface_size
///
/// Return the size information a surface has.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_size(surface: *mut Surface) -> SurfaceSize {
    // const grid_size = surface.core_surface.size.grid();
    // SAFETY: `surface` validity is the caller's contract.
    let grid_size = unsafe { (*surface).core_surface.size.grid() };
    // return .{
    //     .columns = grid_size.columns,
    //     .rows = grid_size.rows,
    //     .width_px = surface.core_surface.size.screen.width,
    //     .height_px = surface.core_surface.size.screen.height,
    //     .cell_width_px = surface.core_surface.size.cell.width,
    //     .cell_height_px = surface.core_surface.size.cell.height,
    // };
    let size = unsafe { (*surface).core_surface.size };
    SurfaceSize {
        columns: grid_size.columns,
        rows: grid_size.rows,
        width_px: size.screen.width,
        height_px: size.screen.height,
        cell_width_px: size.cell.width,
        cell_height_px: size.cell.height,
    }
}

/// C: rustty_surface_foreground_pid
///
/// Returns the PID of the foreground process for the surface PTY.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_foreground_pid(surface: *mut Surface) -> u64 {
    // return surface.core_surface.getProcessInfo(.foreground_pid) orelse 0;
    //
    // PORT NOTE: the reference comptime `getProcessInfo(.foreground_pid)` selector is
    // the core `getForegroundPid` half (the core comptime-return split).
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.getForegroundPid() }.unwrap_or(0)
}

/// C: rustty_surface_tty_name
///
/// Returns the PTY name for the surface. The returned string must be
/// freed by the caller via rustty_string_free.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_tty_name(surface: *mut Surface) -> String {
    // const tty_name = surface.core_surface.getProcessInfo(.tty_name) orelse return .empty;
    //
    // PORT NOTE: the `.tty_name` half of the core comptime `getProcessInfo` split
    // (`getTtyName` → `Option<&CStr>`); its bytes are copied out immediately (the
    // reference `dupeZ`), releasing the borrow.
    // SAFETY: `surface` validity is the caller's contract.
    let copy: Vec<u8> = match unsafe { (*surface).core_surface.getTtyName() } {
        Some(tty_name) => tty_name.to_bytes().to_vec(),
        None => return String::empty(),
    };

    // const copy = surface.app.core_app.alloc.dupeZ(u8, tty_name) catch |err| {
    //     log.err("error allocating tty name err={}", .{err});
    //     return .empty;
    // };
    // return .fromSlice(copy);
    //
    // PORT NOTE: `dupeZ` is the `to_vec()` copy above; Rust's `Vec` allocation is
    // infallible, so the reference `catch |err| return .empty` alloc-error arm is
    // unreachable/elided. `.fromSlice(copy)` leaks the NUL-terminated copy as a
    // sentinel `String` (freed by the host via `rustty_string_free`).
    String::from_slice(copy, true)
}

/// C: rustty_surface_set_color_scheme
///
/// Update the color scheme of the surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_color_scheme(surface: *mut Surface, scheme_raw: c_int) {
    // const scheme = std.meta.intToEnum(apprt.ColorScheme, scheme_raw) catch {
    //     log.warn(
    //         "invalid color scheme to ghostty_surface_set_color_scheme value={}",
    //         .{scheme_raw},
    //     );
    //     return;
    // };
    //
    // PORT NOTE: `apprt.ColorScheme` is `enum(u2) { light = 0, dark = 1 }`; the
    // `intToEnum` guard lives here on the raw `c_int` crossing the C boundary (the
    // E6 `rustty_app_set_color_scheme` precedent).
    let scheme = match scheme_raw {
        0 => ColorScheme::Light,
        1 => ColorScheme::Dark,
        _ => {
            eprintln!("invalid color scheme to rustty_surface_set_color_scheme value={scheme_raw}");
            return;
        }
    };

    // surface.colorSchemeCallback(scheme);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).colorSchemeCallback(scheme) };
}

/// C: rustty_surface_set_content_scale
///
/// Update the content scale of the surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_content_scale(surface: *mut Surface, x: f64, y: f64) {
    // surface.updateContentScale(x, y);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).updateContentScale(x, y) };
}

/// C: rustty_surface_set_focus
///
/// Update the focused state of a surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_focus(surface: *mut Surface, focused: bool) {
    // surface.focusCallback(focused);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).focusCallback(focused) };
}

/// C: rustty_surface_set_occlusion
///
/// Update the occlusion state of a surface.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_occlusion(surface: *mut Surface, visible: bool) {
    // surface.occlusionCallback(visible);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).occlusionCallback(visible) };
}

// ===========================================================================
// CAPI input exports (embedded.zig 1758-1920, chunk E8): the raw key/text/preedit
// and mouse (button/pos/scroll/pressure/ime) sinks. Each wrapper marshals the wire
// form the host passes (`c_int` mods reinterpreted as `input.Mods`, `input.Mouse*`
// value types) and forwards to the ported `rustty-apprt` backend `Surface` callback
// methods (E3) or the core surface directly. Every export here needs a LIVE surface
// (created via `rustty_surface_new`, the Phase-3 GPU-leaf boundary), so it links now
// and reaches the renderer leaf only when driven — NO new `unimplemented!()` leaf in
// apprt-c (DEFERRALS.md, E7 precedent).
// ===========================================================================

/// C: rustty_surface_key_translation_mods
///
/// Filter the mods if necessary. This handles settings such as
/// `macos-option-as-alt`. The filtered mods should be used for
/// key translation but should NOT be sent back via the `_key`
/// function -- the original mods should be used for that.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_key_translation_mods(
    surface: *mut Surface,
    mods_raw: c_int,
) -> c_int {
    // const mods: input.Mods = @bitCast(@as(
    //     input.Mods.Backing,
    //     @truncate(@as(c_uint, @bitCast(mods_raw))),
    // ));
    //
    // PORT NOTE: reinterpret the signed `c_int` as unsigned (`@bitCast` ->
    // `as c_uint`), truncate to the `u16` `Mods.Backing` (`@truncate` ->
    // `as ModsBacking`), then reinterpret the backing bits as `Mods`
    // (`@bitCast` -> `Mods::from_int`), the E5 `KeyEvent::keyEvent` precedent.
    let mods = Mods::from_int(mods_raw as core::ffi::c_uint as ModsBacking);
    // const result = mods.translation(
    //     surface.core_surface.config.macos_option_as_alt orelse
    //         surface.app.keyboardLayout().detectOptionAsAlt(),
    // );
    //
    // PORT NOTE: `Mods.translation` is ported as the `rustty-input` free fn
    // `key_mods::translation` (it needs the apprt-layer `OptionAsAlt`, so it cannot
    // live on `rustty-vt`'s `Mods`). The `orelse` is lazy — `keyboardLayout()`
    // (`detectOptionAsAlt`) is only evaluated when the config value is unset —
    // mirrored with `unwrap_or_else`.
    // SAFETY: `surface` validity is the caller's contract.
    let option_as_alt = unsafe { (*surface).core_surface.config.macos_option_as_alt }
        // SAFETY: `surface.app` is the stable backend `App` pointer set at init.
        .unwrap_or_else(|| unsafe { (*(*surface).app).keyboardLayout() }.detect_option_as_alt());
    let result = translation(mods, option_as_alt);
    // return @intCast(@as(input.Mods.Backing, @bitCast(result)));
    //
    // PORT NOTE: reinterpret `result` as its `u16` backing (`@bitCast` -> `int()`)
    // then widen to the `c_int` the ABI returns (`@intCast`).
    result.int() as c_int
}

/// C: rustty_surface_key
///
/// Send this for raw keypresses (i.e. the keyDown event on macOS).
/// This will handle the keymap translation and send the appropriate
/// key and char events.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `event.text`,
/// if non-null, must be a valid NUL-terminated C string that outlives the call.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_key(surface: *mut Surface, event: KeyEvent) -> bool {
    // return surface.app.keyEvent(
    //     .{ .surface = surface },
    //     event.keyEvent(),
    // ) catch |err| {
    //     log.warn("error processing key event err={}", .{err});
    //     return false;
    // };
    // SAFETY: `event.text` validity is the caller's contract (KeyEvent::keyEvent).
    let kev = unsafe { event.keyEvent() };
    // SAFETY: `surface`/`surface.app` validity is the caller's contract.
    match unsafe { (*(*surface).app).keyEvent(KeyTarget::Surface(surface), kev) } {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error processing key event err={err:?}");
            false
        }
    }
}

/// C: rustty_surface_key_is_binding
///
/// Returns true if the given key event would trigger a binding
/// if it were sent to the surface right now. The "right now"
/// is important because things like trigger sequences are only
/// valid until the next key event.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `event.text`,
/// if non-null, must be a valid NUL-terminated C string that outlives the call;
/// `c_flags`, if non-null, must be a valid writable [`FlagsC`] pointer.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_key_is_binding(
    surface: *mut Surface,
    event: KeyEvent,
    c_flags: *mut FlagsC,
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

    // const flags = surface.core_surface.keyEventIsBinding(
    //     core_event,
    // ) orelse return false;
    // SAFETY: `surface` validity is the caller's contract.
    let flags = match unsafe { (*surface).core_surface.keyEventIsBinding(core_event) } {
        Some(v) => v,
        None => return false,
    };
    // if (c_flags) |ptr| ptr.* = flags.cval();
    //
    // PORT NOTE: the reference `?*` nullable pointer is a raw `*mut FlagsC`; the
    // `if (c_flags) |ptr|` capture is a null-check before the write.
    if !c_flags.is_null() {
        // SAFETY: `c_flags` is non-null and the caller's contract makes it writable.
        unsafe { *c_flags = flags.cval() };
    }
    // return true;
    true
}

/// C: rustty_surface_text
///
/// Send raw text to the terminal. This is treated like a paste
/// so this isn't useful for sending escape sequences. For that,
/// individual key input should be used.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `ptr` must
/// point to at least `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_text(surface: *mut Surface, ptr: *const u8, len: usize) {
    // surface.textCallback(ptr[0..len]);
    // SAFETY: `ptr[0..len]` validity is the caller's contract.
    let text = unsafe { core::slice::from_raw_parts(ptr, len) };
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).textCallback(text) };
}

/// C: rustty_surface_preedit
///
/// Set the preedit text for the surface. This is used for IME
/// composition. If the length is 0, then the preedit text is cleared.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `ptr` must
/// point to at least `len` readable bytes when `len != 0`.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_preedit(surface: *mut Surface, ptr: *const u8, len: usize) {
    // surface.preeditCallback(if (len == 0) null else ptr[0..len]);
    //
    // PORT NOTE: the reference `?[]const u8` — `null` when `len == 0`, else the
    // `ptr[0..len]` slice — maps to `Option<&[u8]>`.
    let preedit = if len == 0 {
        None
    } else {
        // SAFETY: `ptr[0..len]` validity is the caller's contract (len != 0 here).
        Some(unsafe { core::slice::from_raw_parts(ptr, len) })
    };
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).preeditCallback(preedit) };
}

/// C: rustty_surface_mouse_captured
///
/// Returns true if the surface currently has mouse capturing
/// enabled.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_captured(surface: *mut Surface) -> bool {
    // return surface.core_surface.mouseCaptured();
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).core_surface.mouseCaptured() }
}

/// C: rustty_surface_mouse_button
///
/// Tell the surface that it needs to schedule a render
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_button(
    surface: *mut Surface,
    action: mouse::ButtonState,
    button: mouse::Button,
    mods: c_int,
) -> bool {
    // return surface.mouseButtonCallback(
    //     action,
    //     button,
    //     @bitCast(@as(
    //         input.Mods.Backing,
    //         @truncate(@as(c_uint, @bitCast(mods))),
    //     )),
    // );
    //
    // PORT NOTE: the `c_int` mods reinterpret is the `key_translation_mods` /
    // `KeyEvent::keyEvent` precedent (`Mods::from_int(mods as c_uint as Backing)`).
    // SAFETY: `surface` validity is the caller's contract.
    unsafe {
        (*surface).mouseButtonCallback(
            action,
            button,
            Mods::from_int(mods as core::ffi::c_uint as ModsBacking),
        )
    }
}

/// C: rustty_surface_mouse_pos
///
/// Update the mouse position within the view.
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_pos(
    surface: *mut Surface,
    x: f64,
    y: f64,
    mods: c_int,
) {
    // surface.cursorPosCallback(
    //     x,
    //     y,
    //     @bitCast(@as(
    //         input.Mods.Backing,
    //         @truncate(@as(c_uint, @bitCast(mods))),
    //     )),
    // );
    // SAFETY: `surface` validity is the caller's contract.
    unsafe {
        (*surface).cursorPosCallback(
            x,
            y,
            Mods::from_int(mods as core::ffi::c_uint as ModsBacking),
        )
    };
}

/// C: rustty_surface_mouse_scroll
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_scroll(
    surface: *mut Surface,
    x: f64,
    y: f64,
    scroll_mods: c_int,
) {
    // surface.scrollCallback(
    //     x,
    //     y,
    //     @bitCast(@as(u8, @truncate(@as(c_uint, @bitCast(scroll_mods))))),
    // );
    //
    // PORT NOTE: reinterpret the signed `c_int` as unsigned, truncate to the `u8`
    // `ScrollMods` backing, then reinterpret the bits as `ScrollMods`
    // (`@bitCast` -> `ScrollMods::from_int`, the vt `Mods::from_int` idiom).
    // SAFETY: `surface` validity is the caller's contract.
    unsafe {
        (*surface).scrollCallback(
            x,
            y,
            mouse::ScrollMods::from_int(scroll_mods as core::ffi::c_uint as u8),
        )
    };
}

/// C: rustty_surface_mouse_pressure
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_mouse_pressure(
    surface: *mut Surface,
    stage_raw: u32,
    pressure: f64,
) {
    // const stage = std.meta.intToEnum(
    //     input.MousePressureStage,
    //     stage_raw,
    // ) catch {
    //     log.warn(
    //         "invalid mouse pressure stage value={}",
    //         .{stage_raw},
    //     );
    //     return;
    // };
    //
    // PORT NOTE: `intToEnum` is the fallible tag guard on the raw `u32` crossing the
    // C boundary (the E7 `set_color_scheme` precedent). `MousePressureStage` is
    // `enum(u8) { none=0, normal=1, deep=2 }`.
    let stage = match stage_raw {
        0 => mouse::PressureStage::None,
        1 => mouse::PressureStage::Normal,
        2 => mouse::PressureStage::Deep,
        _ => {
            eprintln!("invalid mouse pressure stage value={stage_raw}");
            return;
        }
    };

    // surface.mousePressureCallback(stage, pressure);
    // SAFETY: `surface` validity is the caller's contract.
    unsafe { (*surface).mousePressureCallback(stage, pressure) };
}

/// C: rustty_surface_ime_point
///
/// # Safety
/// `surface` must be a valid pointer returned by [`rustty_surface_new`]; `x`/`y`/
/// `width`/`height` must be valid writable `f64` pointers.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_ime_point(
    surface: *mut Surface,
    x: *mut f64,
    y: *mut f64,
    width: *mut f64,
    height: *mut f64,
) {
    // const pos = surface.core_surface.imePoint();
    // SAFETY: `surface` validity is the caller's contract.
    let pos = unsafe { (*surface).core_surface.imePoint() };
    // x.* = pos.x;
    // y.* = pos.y;
    // width.* = pos.width;
    // height.* = pos.height;
    // SAFETY: `x`/`y`/`width`/`height` writability is the caller's contract.
    unsafe {
        *x = pos.x;
        *y = pos.y;
        *width = pos.width;
        *height = pos.height;
    }
}

// ===========================================================================
// CAPI surface-action + clipboard + inspector exports (embedded.zig 1922-2100,
// chunk E9): the 6 surface-action sinks (request_close / split / split_focus /
// split_resize / split_equalize / binding_action), the 1 clipboard-completion
// sink, and the 10 inspector sinks (surface_inspector / inspector_free +
// set_size / set_content_scale + mouse_button / mouse_pos / mouse_scroll / key /
// text / set_focus). The surface-action exports forward to the backend `App`'s
// `perform_action` (the apprt-api trait — E6 `open_config` precedent, here with a
// `Target::Surface`) or the core surface directly; the clipboard sink forwards to
// the backend `Surface::completeClipboardRequest` (E3); the inspector sinks forward
// to the E4 `Inspector` methods. Every export here needs a LIVE surface (created via
// `rustty_surface_new`, the Phase-3 GPU-leaf boundary), and the inspector sinks
// additionally need a LIVE `Inspector` — `ghostty_surface_inspector` reaches the
// Phase-3 dcimgui `Inspector::init` leaf, so no inspector can yet exist and the
// per-inspector sinks reach the E4 `unimplemented!()` methods when driven. All link
// now and reach the leaf only when exercised; NO new `unimplemented!()` leaf is added
// in apprt-c (DEFERRALS.md, E7/E8 precedent).
// ===========================================================================

/// C: rustty_surface_request_close
///
/// Request that the surface become closed. This will go through the
/// normal trigger process that a close surface input binding would.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_request_close(ptr: *mut Surface) {
    // ptr.core_surface.close();
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).core_surface.close() };
}

/// C: rustty_surface_split
///
/// Request that the surface split in the given direction.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_split(ptr: *mut Surface, direction: SplitDirection) {
    // _ = ptr.app.performAction(
    //     .{ .surface = &ptr.core_surface },
    //     .new_split,
    //     direction,
    // ) catch |err| {
    //     log.err("error creating new split err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: the reference `*App` method `performAction(.{ .surface =
    // &ptr.core_surface }, .new_split, direction)` maps to the `apprt_api::App` trait
    // `perform_action(Target::Surface(&core_surface), Action::NewSplit(direction))`
    // (the E6 `open_config` precedent — here the `Target` carries the surface, so `C`
    // is inferred as `CoreSurface`). The `bool` result is discarded (reference `_ =`).
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*(*ptr).app).perform_action(
            Target::Surface(&(*ptr).core_surface),
            Action::NewSplit(direction),
        )
    } {
        eprintln!("error creating new split err={err:?}");
    }
}

/// C: rustty_surface_split_focus
///
/// Focus on the next split (if any).
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_split_focus(ptr: *mut Surface, direction: GotoSplit) {
    // _ = ptr.app.performAction(
    //     .{ .surface = &ptr.core_surface },
    //     .goto_split,
    //     direction,
    // ) catch |err| {
    //     log.err("error creating new split err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: the reference log message is "error creating new split" verbatim —
    // a copy-paste of `ghostty_surface_split`'s message; preserved faithfully (R3).
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*(*ptr).app).perform_action(
            Target::Surface(&(*ptr).core_surface),
            Action::GotoSplit(direction),
        )
    } {
        eprintln!("error creating new split err={err:?}");
    }
}

/// C: rustty_surface_split_resize
///
/// Resize the current split by moving the split divider in the given
/// direction. `direction` specifies which direction the split divider will
/// move relative to the focused split. `amount` is a fractional value
/// between 0 and 1 that specifies by how much the divider will move.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_split_resize(
    ptr: *mut Surface,
    direction: ResizeSplitDirection,
    amount: u16,
) {
    // _ = ptr.app.performAction(
    //     .{ .surface = &ptr.core_surface },
    //     .resize_split,
    //     .{ .direction = direction, .amount = amount },
    // ) catch |err| {
    //     log.err("error resizing split err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: `.resize_split`'s value is the `ResizeSplit` struct, reassembled by
    // field name from the two params (`.{ .direction, .amount }`).
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*(*ptr).app).perform_action(
            Target::Surface(&(*ptr).core_surface),
            Action::ResizeSplit(ResizeSplit { direction, amount }),
        )
    } {
        eprintln!("error resizing split err={err:?}");
    }
}

/// C: rustty_surface_split_equalize
///
/// Equalize the size of all splits in the current window.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_split_equalize(ptr: *mut Surface) {
    // _ = ptr.app.performAction(
    //     .{ .surface = &ptr.core_surface },
    //     .equalize_splits,
    //     {},
    // ) catch |err| {
    //     log.err("error equalizing splits err={}", .{err});
    //     return;
    // };
    //
    // PORT NOTE: the reference void value `{}` for `.equalize_splits` maps to the
    // fieldless `Action::EqualizeSplits` variant.
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*(*ptr).app).perform_action(
            Target::Surface(&(*ptr).core_surface),
            Action::EqualizeSplits,
        )
    } {
        eprintln!("error equalizing splits err={err:?}");
    }
}

/// C: rustty_surface_binding_action
///
/// Invoke an action on the surface.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`]; `action_ptr`
/// must point to `action_len` valid bytes.
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_binding_action(
    ptr: *mut Surface,
    action_ptr: *const u8,
    action_len: usize,
) -> bool {
    // const action_str = action_ptr[0..action_len];
    // SAFETY: `action_ptr`/`action_len` validity is the caller's contract.
    let action_str = unsafe { core::slice::from_raw_parts(action_ptr, action_len) };

    // const action = input.Binding.Action.parse(action_str) catch |err| {
    //     log.err("error parsing binding action action={s} err={}", .{ action_str, err });
    //     return false;
    // };
    let action = match binding::Action::parse(action_str) {
        Ok(action) => action,
        Err(err) => {
            eprintln!(
                "error parsing binding action action={} err={err:?}",
                // PORT NOTE: fully-qualified — the crate-local C-API `String` (the
                // `main_c.zig` heap-string type) shadows `std::string::String` here.
                std::string::String::from_utf8_lossy(action_str),
            );
            return false;
        }
    };

    // return ptr.core_surface.performBindingAction(action) catch |err| {
    //     log.err("error performing binding action action={f} err={}", .{ action, err });
    //     return false;
    // };
    //
    // PORT NOTE: Zig passes the `action` union to `performBindingAction` BY VALUE (a
    // copy), leaving the local `action` live for the `catch` error log. Rust's
    // `performBindingAction(&mut self, action: binding::Action)` CONSUMES its arg, so
    // `action.clone()` reproduces Zig's by-value copy and keeps `action` available for
    // the error branch's `action={f}` (rendered via `Debug`, the faithful analog of
    // the reference's custom `{f}` `Action.format`).
    // SAFETY: `ptr` validity is the caller's contract.
    match unsafe { (*ptr).core_surface.performBindingAction(action.clone()) } {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error performing binding action action={action:?} err={err:?}");
            false
        }
    }
}

/// C: rustty_surface_complete_clipboard_request
///
/// Complete a clipboard read request started via the read callback.
/// This can only be called once for a given request. Once it is called
/// with a request the request pointer will be invalidated.
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`]; `str` must be a
/// valid NUL-terminated C string; `state` must be the request pointer handed to the
/// host by the read callback (it is invalidated by this call).
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_complete_clipboard_request(
    ptr: *mut Surface,
    str: *const c_char,
    state: *mut ClipboardRequest,
    confirmed: bool,
) {
    // ptr.completeClipboardRequest(
    //     std.mem.sliceTo(str, 0),
    //     state,
    //     confirmed,
    // );
    //
    // PORT NOTE: `str: [*:0]const u8` + `std.mem.sliceTo(str, 0)` → a borrowed `&CStr`
    // (`CStr::from_ptr`): the backend `completeClipboardRequest` slices it
    // (`.to_bytes()`) for core and re-hands `.as_ptr()` to the host
    // `confirm_read_clipboard` callback (the NUL sentinel is load-bearing, E3).
    // SAFETY: `ptr`/`str`/`state` validity is the caller's contract.
    unsafe {
        (*ptr).completeClipboardRequest(CStr::from_ptr(str), state, confirmed);
    }
}

/// C: rustty_surface_inspector
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_inspector(ptr: *mut Surface) -> *mut Inspector {
    // return ptr.initInspector() catch |err| {
    //     log.err("error initializing inspector err={}", .{err});
    //     return null;
    // };
    //
    // PORT NOTE: `?*Inspector` → a nullable `*mut Inspector` (cbindgen emits the raw
    // pointer; `null` for the error case). `initInspector` reaches the Phase-3 dcimgui
    // `Inspector::init` leaf at RUNTIME (EMBEDDED_BLUEPRINT §1) — so a live inspector
    // cannot yet be built; this export links now and panics in that leaf when driven.
    // SAFETY: `ptr` validity is the caller's contract.
    match unsafe { (*ptr).initInspector() } {
        Ok(v) => v,
        Err(err) => {
            eprintln!("error initializing inspector err={err:?}");
            core::ptr::null_mut()
        }
    }
}

/// C: rustty_inspector_free
///
/// # Safety
/// `ptr` must be a valid pointer returned by [`rustty_surface_new`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_free(ptr: *mut Surface) {
    // ptr.freeInspector();
    //
    // PORT NOTE: the reference `ghostty_inspector_free` takes a `*Surface` (NOT a
    // `*Inspector`) — it frees the surface-owned inspector via `freeInspector` (E4).
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).freeInspector() };
}

/// C: rustty_inspector_set_size
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_set_size(ptr: *mut Inspector, w: u32, h: u32) {
    // ptr.updateSize(w, h);
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).updateSize(w, h) };
}

/// C: rustty_inspector_set_content_scale
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_set_content_scale(ptr: *mut Inspector, x: f64, y: f64) {
    // ptr.updateContentScale(x, y);
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).updateContentScale(x, y) };
}

/// C: rustty_inspector_mouse_button
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_mouse_button(
    ptr: *mut Inspector,
    action: mouse::ButtonState,
    button: mouse::Button,
    mods: c_int,
) {
    // ptr.mouseButtonCallback(
    //     action,
    //     button,
    //     @bitCast(@as(
    //         input.Mods.Backing,
    //         @truncate(@as(c_uint, @bitCast(mods))),
    //     )),
    // );
    //
    // PORT NOTE: the `c_int` mods reinterpret is the E8 `mouse_button` precedent
    // (`Mods::from_int(mods as c_uint as Backing)`).
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe {
        (*ptr).mouseButtonCallback(
            action,
            button,
            Mods::from_int(mods as core::ffi::c_uint as ModsBacking),
        )
    };
}

/// C: rustty_inspector_mouse_pos
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_mouse_pos(ptr: *mut Inspector, x: f64, y: f64) {
    // ptr.cursorPosCallback(x, y);
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).cursorPosCallback(x, y) };
}

/// C: rustty_inspector_mouse_scroll
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_mouse_scroll(
    ptr: *mut Inspector,
    x: f64,
    y: f64,
    scroll_mods: c_int,
) {
    // ptr.scrollCallback(
    //     x,
    //     y,
    //     @bitCast(@as(u8, @truncate(@as(c_uint, @bitCast(scroll_mods))))),
    // );
    //
    // PORT NOTE: the E8 `mouse_scroll` precedent — reinterpret the `c_int` as unsigned,
    // truncate to the `u8` `ScrollMods` backing, then reinterpret the bits.
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe {
        (*ptr).scrollCallback(
            x,
            y,
            mouse::ScrollMods::from_int(scroll_mods as core::ffi::c_uint as u8),
        )
    };
}

/// C: rustty_inspector_key
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_key(
    ptr: *mut Inspector,
    action: key::Action,
    key: key::Key,
    c_mods: c_int,
) {
    // ptr.keyCallback(
    //     action,
    //     key,
    //     @bitCast(@as(
    //         input.Mods.Backing,
    //         @truncate(@as(c_uint, @bitCast(c_mods))),
    //     )),
    // ) catch |err| {
    //     log.err("error processing key event err={}", .{err});
    //     return;
    // };
    // SAFETY: `ptr` validity is the caller's contract.
    if let Err(err) = unsafe {
        (*ptr).keyCallback(
            action,
            key,
            Mods::from_int(c_mods as core::ffi::c_uint as ModsBacking),
        )
    } {
        eprintln!("error processing key event err={err:?}");
    }
}

/// C: rustty_inspector_text
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`];
/// `str` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_text(ptr: *mut Inspector, str: *const c_char) {
    // ptr.textCallback(std.mem.sliceTo(str, 0));
    //
    // PORT NOTE: `str: [*:0]const u8` + `std.mem.sliceTo(str, 0)` → a borrowed `&CStr`
    // (`CStr::from_ptr`); the backend `textCallback` re-hands `text.ptr` to imgui (the
    // NUL sentinel is load-bearing, E4).
    // SAFETY: `ptr`/`str` validity is the caller's contract.
    unsafe { (*ptr).textCallback(CStr::from_ptr(str)) };
}

/// C: rustty_inspector_set_focus
///
/// # Safety
/// `ptr` must be a valid [`Inspector`] pointer returned by [`rustty_surface_inspector`].
#[no_mangle]
pub unsafe extern "C" fn rustty_inspector_set_focus(ptr: *mut Inspector, focused: bool) {
    // ptr.focusCallback(focused);
    // SAFETY: `ptr` validity is the caller's contract.
    unsafe { (*ptr).focusCallback(focused) };
}
