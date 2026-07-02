//! rustty-apprt::embedded
//!
//! Application runtime for the embedded version of rustty. The embedded
//! version is when rustty is embedded within a parent host application,
//! rather than owning the application lifecycle itself. This is used for
//! example for the macOS build of rustty so that we can use a native
//! Swift+XCode-based application.
//!
//! PORTS: ghostty `src/apprt/embedded.zig` (2250 loc, 0 tests). This is the FIRST
//! concrete apprt backend — it proves the `apprt_api` generic trait seam against a
//! real `A = embedded::App` (EMBEDDED_BLUEPRINT §2).
//!
//! PORT STATUS: in-progress (chunk E1 — `App` backend). This chunk ports
//! `embedded.zig:29-342` — the `App` struct with its `Options` (the `#[repr(C)]`
//! host-callback config), `KeyEvent`, `KeyTarget`, and every `App` method
//! (init/terminate/hasGlobalKeybinds/focusEvent/keyEvent/reloadKeymap/
//! keyboardLayout/wakeup/wait/newSurface/closeSurface/redrawInspector/
//! performAction/performPreAction/performIpc) — plus `impl apprt_api::App for App`
//! (the first concrete backend to inhabit the seam). Later chunks fill the rest
//! (EMBEDDED_BLUEPRINT §7):
//!
//! E2 (343-609): `Platform`/`PlatformTag`/`EnvVar` + the full `Surface` struct +
//! `Surface.Options`/`init`/`deinit`; `impl apprt_api::Surface for Surface`.
//!
//! E3 (610-990): surface event callbacks (refresh/draw/updateSize/mouse/…;
//! draw/refresh reach the Phase-3 GPU leaf at runtime).
//!
//! E4 (992-1243): `Inspector` (dcimgui+Metal) — a Phase-3 inhabited placeholder
//! with `unimplemented!()` leaves. Deferrable/parallel.
//!
//! To make `impl apprt_api::App` typecheck NOW (its `type Surface = Surface`
//! binding forces `Surface: apprt_api::Surface`), this chunk carries a MINIMAL
//! `Surface`/`SurfaceOptions` E1->E2 SEQUENCING STUB (see the block near the end
//! of this file) — every apprt-api trait method + `init`/`queueInspectorRender`
//! is an `unimplemented!()` placeholder leaf; E2 replaces the block wholesale.
//!
//! [ADV-FIX 8] PINNING INVARIANT (correctness-critical, for E2): `embedded::Surface`
//! holds `core_surface: rustty_core::Surface<App>` BY VALUE, and core
//! `Surface::init` writes `core_surface.rt_surface = &embedded_surface` (a pointer
//! back to the container). The embedded `Surface` MUST therefore be heap-allocated
//! (Box) and never moved after init — `ghostty_surface_new` returns a `*mut Surface`
//! to a boxed, pinned backend; any move dangles `rt_surface`. [`App::newSurface`]
//! already mirrors the reference `alloc.create(Surface)` + in-place `init(self, …)`.
//!
//! Carved incrementally (R1: every line/comment/test{} ported as the crate is
//! carved, leaves-first, under the acyclicity gate per R4).

// PORT NOTE: the reference passes raw `*Surface` / `*App` across the public method
// args exactly as the Zig does (`closeSurface`/`redrawInspector`/`keyEvent`'s
// surface target), so those public fns deref raw-pointer args — silenced at module
// scope (rustty-core App.zig precedent). The reference identifiers are camelCase
// (`focusEvent`, `keyEvent`, `newSurface`, …); preserved 1:1 and silenced too.
#![allow(non_snake_case)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_char, c_int, c_void};
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::mem::{ManuallyDrop, MaybeUninit};

use allocator_api2::alloc::{Allocator, Global};

use rustty_apprt_api::action::{Action, ActionC, Target, TargetC};
use rustty_apprt_api::ipc;
use rustty_apprt_api::structs::{
    Clipboard, ClipboardRequest, ClipboardRequestType, ContentScale, CursorPos, SurfaceSize,
};
use rustty_apprt_api::{App as ApprtApp, ApprtError, CoreSurfaceHandle, Surface as ApprtSurface};

use rustty_config::Config;

use rustty_input::binding;
use rustty_input::keyboard::Layout;
use rustty_input::Keymap;

use rustty_vt::input::key;
use rustty_vt::input::key::KeyEvent as CoreKeyEvent;
use rustty_vt::input::key_mods::Mods;

use rustty_core::surface::{Error as CoreSurfaceError, InputEffect};

// const CoreApp = @import("../App.zig");
type CoreApp = rustty_core::app::App<App>;
// const CoreSurface = @import("../Surface.zig");
type CoreSurface = rustty_core::surface::Surface<App>;

// PORT: reference `pub const resourcesDir = internal_os.resourcesDir;`
// (embedded.zig:27).
pub use rustty_os::resourcesDir;

/// The error set of [`App::init`] / [`App::reloadKeymap`] — the platform-selected
/// `input.Keymap`'s `init`/`reload` error (the reference `try Keymap.init()` /
/// `try self.keymap.reload()`; `config.clone` is infallible in rustty). It is
/// `keymap_darwin::Error` on macOS (the Carbon-backed keymap) and the noop
/// keymap's empty `()` set elsewhere.
#[cfg(target_os = "macos")]
pub type KeymapError = rustty_input::keymap_darwin::Error;
/// See the macOS variant.
#[cfg(not(target_os = "macos"))]
pub type KeymapError = ();

pub struct App {
    // core_app: *CoreApp,
    //
    // PORT NOTE (ADV-FIX 9): the reference fields are file-private; the apprt-c
    // C-ABI wrappers reach them, so they are `pub` here.
    pub core_app: *mut CoreApp,
    // opts: Options,
    pub opts: Options,
    // keymap: input.Keymap,
    pub keymap: Keymap,

    /// The configuration for the app. This is owned by this structure.
    //
    // config: Config,
    //
    // PORT NOTE: `config` is wrapped in an `UnsafeCell` (rather than the reference
    // bare `config: Config`) to reconcile the reference's `performAction`/
    // `performPreAction`, which take a MUTABLE `self: *App` (embedded.zig:267/289)
    // and REASSIGN `self.config` on a `config_change` action (:313), with the
    // apprt-api `App::perform_action(&self)` SHARED-ref trait shape. The app is
    // heap-owned and driven single-threaded through the C-ABI, so the interior
    // mutation is sound — this mirrors Zig's unrestricted `*App` mutability.
    pub config: UnsafeCell<Config>,
}

/// Because we only expect the embedding API to be used in embedded
/// environments, the options are extern so that we can expose it
/// directly to a C callconv and not pay for any translation costs.
///
/// C type: ghostty_runtime_config_s
//
// PORT NOTE (ADV-FIX 9): the reference nests this as `App.Options` and accesses it
// file-privately; the apprt-c C-ABI wrappers reach it, so it is a top-level `pub`
// struct here. It stays in `rustty-apprt::embedded` (NOT the apprt-c crate)
// because it is a BACKEND field (`App.opts`); apprt-c hand-pins it in cbindgen
// `after_includes` (EMBEDDED_BLUEPRINT §3). The `?*anyopaque` aliases `AppUD` /
// `SurfaceUD` map to `*mut c_void`; `callconv(.c)` fn pointers to `extern "C" fn`;
// the nullable `?*const fn` (`close_surface`) to `Option<extern "C" fn>` (its null
// niche keeps `size_of` identical to a bare fn pointer — the fn-ptr-ABI backstop
// is the cbindgen typedef + the smoke KAT's typed stubs, per ADV-FIX 7, not size).
//
// PORT NOTE (improper_ctypes_definitions): the `action` callback's `ActionC` /
// `TargetC` params transitively carry `rustty_input`'s `Mods` (via `TriggerC`),
// which apprt-api has NOT yet made byte-exact-C — that C-ABI projection is a
// DOCUMENTED deferral (apprt-api `action.rs` "C-ABI PROJECTION (deferred pinning)"),
// pinned by the D2 const-asserts + cbindgen in E5. The reference callback signature
// is faithful (`*const fn (*App, apprt.Target.C, apprt.Action.C) callconv(.c) bool`);
// the lint is silenced pending that pinning.
#[allow(improper_ctypes_definitions)]
#[repr(C)]
pub struct Options {
    // These are just aliases to make the function signatures below
    // more obvious what values will be sent.
    // const AppUD = ?*anyopaque;      -> *mut c_void
    // const SurfaceUD = ?*anyopaque;  -> *mut c_void
    /// Userdata that is passed to all the callbacks.
    // userdata: AppUD = null,
    pub userdata: *mut c_void,

    /// True if the selection clipboard is supported.
    // supports_selection_clipboard: bool = false,
    pub supports_selection_clipboard: bool,

    /// Callback called to wakeup the event loop. This should trigger
    /// a full tick of the app loop.
    // wakeup: *const fn (AppUD) callconv(.c) void,
    pub wakeup: extern "C" fn(*mut c_void),

    /// Callback called to handle an action.
    // action: *const fn (*App, apprt.Target.C, apprt.Action.C) callconv(.c) bool,
    pub action: extern "C" fn(*mut App, TargetC, ActionC) -> bool,

    /// Read the clipboard value. Returns true if the clipboard request
    /// was started and complete_clipboard_request may be called with the
    /// given state pointer. Returns false if the clipboard request couldn't
    /// be started (such as when no text is available for a paste request).
    // read_clipboard: *const fn (SurfaceUD, c_int, *apprt.ClipboardRequest) callconv(.c) bool,
    pub read_clipboard: extern "C" fn(*mut c_void, c_int, *mut ClipboardRequest) -> bool,

    /// This may be called after a read clipboard call to request
    /// confirmation that the clipboard value is safe to read. The embedder
    /// must call complete_clipboard_request with the given request.
    // confirm_read_clipboard: *const fn (SurfaceUD, [*:0]const u8, *apprt.ClipboardRequest, apprt.ClipboardRequestType) callconv(.c) void,
    pub confirm_read_clipboard:
        extern "C" fn(*mut c_void, *const c_char, *mut ClipboardRequest, ClipboardRequestType),

    /// Write the clipboard value.
    // write_clipboard: *const fn (SurfaceUD, c_int, [*]const CAPI.ClipboardContent, usize, bool) callconv(.c) void,
    pub write_clipboard: extern "C" fn(*mut c_void, c_int, *const ClipboardContent, usize, bool),

    /// Close the current surface given by this function.
    // close_surface: ?*const fn (SurfaceUD, bool) callconv(.c) void = null,
    pub close_surface: Option<extern "C" fn(*mut c_void, bool)>,
}

/// This is the key event sent for ghostty_surface_key and
/// ghostty_app_key.
//
// PORT NOTE: the reference nests this as `App.KeyEvent`; a top-level struct here.
// The `?[:0]const u8` text is a borrowed, NUL-terminated C string (built by the
// CAPI from `ghostty_input_key_s`); modeled as a borrowed byte slice
// `Option<&'a [u8]>` (R3 byte-slice convention), hence the `'a` lifetime.
pub struct KeyEvent<'a> {
    pub action: key::Action,
    pub mods: Mods,
    pub consumed_mods: Mods,
    pub keycode: u32,
    pub text: Option<&'a [u8]>,
    pub unshifted_codepoint: u32,
    pub composing: bool,
}

impl KeyEvent<'_> {
    /// Convert a libghostty key event into a core key event.
    fn core(&self) -> Option<CoreKeyEvent> {
        // const text: []const u8 = if (self.text) |v| v else "";
        let text: &[u8] = self.text.unwrap_or(b"");
        // const unshifted_codepoint: u21 = std.math.cast(u21, self.unshifted_codepoint) orelse 0;
        //
        // PORT NOTE: rustty models `u21` as `u32` (`rustty_vt::input::key::u21`),
        // so the `std.math.cast(u21, u32)` is the identity and the `orelse 0`
        // fallback is unreachable; assigned directly.
        let unshifted_codepoint: key::u21 = self.unshifted_codepoint;

        // We want to get the physical unmapped key to process keybinds.
        // const physical_key = keycode: for (input.keycodes.entries) |entry| {
        //     if (entry.native == self.keycode) break :keycode entry.key;
        // } else .unidentified;
        let mut physical_key = key::Key::Unidentified;
        for entry in rustty_input::keycodes::entries() {
            if entry.native == self.keycode {
                physical_key = entry.key;
                break;
            }
        }

        // Build our final key event
        Some(CoreKeyEvent {
            action: self.action,
            key: physical_key,
            mods: self.mods,
            consumed_mods: self.consumed_mods,
            composing: self.composing,
            utf8: text.to_vec(),
            unshifted_codepoint,
        })
    }
}

impl App {
    pub fn init(
        core_app: *mut CoreApp,
        config: &Config,
        opts: Options,
    ) -> Result<App, KeymapError> {
        // We have to clone the config.
        // const alloc = core_app.alloc;
        // var config_clone = try config.clone(alloc);
        // errdefer config_clone.deinit();
        //
        // PORT NOTE: rustty `clone_config` is infallible (no allocator param); the
        // reference placement-init `init(self: *App, …)` returns the value here
        // (Rust has no placement-init — cf. core `App::init`; the CAPI
        // `ghostty_app_new` boxes it).
        let mut config_clone = config.clone_config();

        // var keymap = try input.Keymap.init();
        // errdefer keymap.deinit();
        let keymap = match Keymap::init() {
            Ok(keymap) => keymap,
            Err(err) => {
                // errdefer config_clone.deinit(): rustty `Config` uses an explicit
                // `deinit` (no `Drop`), so the errdefer is honored manually.
                config_clone.deinit();
                return Err(err);
            }
        };

        // self.* = .{ .core_app = core_app, .config = config_clone, .opts = opts, .keymap = keymap };
        Ok(App {
            core_app,
            config: UnsafeCell::new(config_clone),
            opts,
            keymap,
        })
    }

    pub fn terminate(&mut self) {
        // self.keymap.deinit();
        self.keymap.deinit();
        // self.config.deinit();
        self.config.get_mut().deinit();
    }

    /// Returns true if there are any global keybinds in the configuration.
    pub fn hasGlobalKeybinds(&self) -> bool {
        // var it = self.config.keybind.set.bindings.iterator();
        // while (it.next()) |entry| {
        //     switch (entry.value_ptr.*) {
        //         .leader => {},
        //         inline .leaf, .leaf_chained => |leaf| if (leaf.flags.global) return true,
        //     }
        // }
        let config = unsafe { &*self.config.get() };
        for (_trigger, value) in config.keybind.set.iter_bindings() {
            match value {
                binding::Value::Leader(_) => {}
                binding::Value::Leaf(leaf) => {
                    if leaf.flags.global {
                        return true;
                    }
                }
                binding::Value::LeafChained(leaf) => {
                    if leaf.flags.global {
                        return true;
                    }
                }
            }
        }

        // return false;
        false
    }

    /// See CoreApp.focusEvent
    pub fn focusEvent(&self, focused: bool) {
        // self.core_app.focusEvent(focused);
        unsafe { (*self.core_app).focusEvent(focused) };
    }

    /// See CoreApp.keyEvent.
    pub fn keyEvent(
        &self,
        target: KeyTarget,
        event: KeyEvent<'_>,
    ) -> Result<bool, CoreSurfaceError> {
        // Convert our C key event into a Zig one.
        // const input_event: input.KeyEvent = event.core() orelse return false;
        let input_event: CoreKeyEvent = match event.core() {
            Some(v) => v,
            None => return Ok(false),
        };

        // Invoke the core rustty logic to handle this input.
        // const effect: CoreSurface.InputEffect = switch (target) {
        let effect: InputEffect = match target {
            // .app => if (self.core_app.keyEvent(self, input_event)) .consumed else .ignored,
            KeyTarget::App => {
                if unsafe { (*self.core_app).keyEvent(self, input_event) } {
                    InputEffect::Consumed
                } else {
                    InputEffect::Ignored
                }
            }

            // .surface => |surface| try surface.core_surface.keyCallback(input_event),
            KeyTarget::Surface(surface) => unsafe {
                (*surface).core_surface.keyCallback(input_event)?
            },
        };

        // return switch (effect) {
        //     .closed => true,
        //     .ignored => false,
        //     .consumed => true,
        // };
        Ok(match effect {
            InputEffect::Closed => true,
            InputEffect::Ignored => false,
            InputEffect::Consumed => true,
        })
    }

    /// This should be called whenever the keyboard layout was changed.
    pub fn reloadKeymap(&mut self) -> Result<(), KeymapError> {
        // Reload the keymap
        // try self.keymap.reload();
        self.keymap.reload()
    }

    /// Loads the keyboard layout.
    ///
    /// Kind of expensive so this should be avoided if possible. When I say
    /// "kind of expensive" I mean that its not something you probably want
    /// to run on every keypress.
    pub fn keyboardLayout(&self) -> Layout {
        // We only support keyboard layout detection on macOS.
        // if (comptime builtin.os.tag != .macos) return .unknown;
        //
        // PORT NOTE: the reference early-returns `.unknown` on non-macOS then runs
        // the macOS body; Rust reorders into `#[cfg]` blocks (the non-macOS
        // `Keymap` has no `source_id`), so the macOS body is the tail expr there.
        #[cfg(target_os = "macos")]
        {
            // Any layout larger than this is not something we can handle.
            // var buf: [256]u8 = undefined;
            let mut buf: [u8; 256] = [0; 256];
            // const id = self.keymap.sourceId(&buf) catch |err| {
            //     comptime assert(@TypeOf(err) == error{OutOfMemory});
            //     return .unknown;
            // };
            let id = match self.keymap.source_id(&mut buf) {
                Ok(id) => id,
                Err(_err) => return Layout::Unknown,
            };

            // return input.KeyboardLayout.mapAppleId(id) orelse .unknown;
            Layout::map_apple_id(id).unwrap_or(Layout::Unknown)
        }
        #[cfg(not(target_os = "macos"))]
        {
            Layout::Unknown
        }
    }

    pub fn wakeup(&self) {
        // self.opts.wakeup(self.opts.userdata);
        (self.opts.wakeup)(self.opts.userdata);
    }

    /// PORT NOTE: reference `pub fn wait(self: *const App) !void { _ = self; }` —
    /// the inferred error set is EMPTY (the body never errors), so this is
    /// infallible and returns `()`. There is no apprt-api trait method for `wait`;
    /// the app-loop caller invoking `rt_app.wait()` is the deferred xev seam (D6).
    pub fn wait(&self) {
        // _ = self;
    }

    /// Create a new surface for the app.
    pub fn newSurface(&mut self, opts: SurfaceOptions) -> Result<*mut Surface, ApprtError> {
        // Grab a surface allocation because we're going to need it.
        // var surface = try self.core_app.alloc.create(Surface);
        // errdefer self.core_app.alloc.destroy(surface);
        //
        // [ADV-FIX 8] PINNING + reference placement-init: allocate an
        // uninitialized heap slot; `init` writes through the out-pointer and the
        // Surface is NEVER moved afterwards (core `Surface::init` stores
        // `core_surface.rt_surface = &*surface`).
        let uninit: *mut MaybeUninit<Surface> = Box::into_raw(Box::new(MaybeUninit::uninit()));
        let surface: *mut Surface = uninit.cast();

        // Create the surface
        // try surface.init(self, opts);
        // errdefer surface.deinit();
        unsafe {
            if let Err(err) = Surface::init(surface, core::ptr::from_mut(self), opts) {
                // errdefer self.core_app.alloc.destroy(surface): free the raw slot
                // WITHOUT running `Surface`'s drop (the memory is uninitialized on
                // the init-error path). The second `errdefer surface.deinit()` never
                // fires (no fallible op follows `init`).
                drop(Box::from_raw(uninit));
                return Err(err);
            }
        }

        // return surface;
        Ok(surface)
    }

    /// Close the given surface.
    pub fn closeSurface(&self, surface: *mut Surface) {
        // surface.deinit();
        unsafe { (*surface).deinit() };
        // self.core_app.alloc.destroy(surface);
        //
        // PORT NOTE: the reference frees the surface allocation via the core app
        // allocator; the rustty surface is heap-boxed (`newSurface`), so freeing is
        // reconstructing the `Box` and dropping it WITHOUT running `Surface`'s drop
        // (`deinit` above already tore it down — the `alloc.destroy`-after-`deinit`
        // contract, matching core `Surface::deinit`). `ManuallyDrop` frees the
        // allocation without dropping the (already-deinit'd) `Surface`.
        unsafe { drop(Box::from_raw(surface as *mut ManuallyDrop<Surface>)) };
    }

    pub fn redrawInspector(&self, surface: *mut Surface) {
        // _ = self;
        // surface.queueInspectorRender();
        unsafe { (*surface).queueInspectorRender() };
    }

    fn performPreAction<C: CoreSurfaceHandle>(&self, target: &Target<'_, C>, action: &Action<'_>) {
        // Special case certain actions before they are sent to the embedder
        // switch (action) {
        match action {
            // .set_title => switch (target) {
            Action::SetTitle(value) => match target {
                // .app => {},
                Target::App => {}
                // .surface => |surface| { ... },
                Target::Surface(surface) => {
                    // Dupe the title so that we can store it. If we get an allocation
                    // error we just ignore it, since this only breaks a few minor things.
                    // const alloc = self.core_app.alloc;
                    // if (surface.rt_surface.title) |v| alloc.free(v);
                    // surface.rt_surface.title = alloc.dupeZ(u8, value.title) catch null;
                    //
                    // PORT NOTE: `surface.rt_surface` reaches this embedded backend
                    // via the opaque `rt_surface` handle (`CoreSurfaceHandle`). The
                    // assignment drops the old `Option<CString>` (the reference's
                    // `alloc.free(v)`); `CString::new(..).ok()` is the `dupeZ .. catch
                    // null` (None on the — here, interior-NUL — failure).
                    let rt = surface.rt_surface_ptr() as *mut Surface;
                    unsafe {
                        (*rt).title = CString::new(value.title).ok();
                    }
                }
            },

            // .config_change => switch (target) {
            Action::ConfigChange(value) => match target {
                // .surface => {},
                Target::Surface(_) => {}
                // For app updates, we update our core config. We need to
                // clone it because the caller owns the param.
                // .app => if (value.config.clone(self.core_app.alloc)) |config| {
                //     self.config.deinit();
                //     self.config = config;
                // } else |err| { log.err("error updating app config err={}", .{err}); },
                //
                // PORT NOTE: rustty `clone_config` is infallible, so the reference
                // `else |err|` log arm is unreachable and elided. The mutation goes
                // through the `config` `UnsafeCell` (see the `App.config` field note).
                Target::App => {
                    let config = unsafe { &mut *self.config.get() };
                    config.deinit();
                    *config = value.config.clone_config();
                }
            },

            // else => {},
            _ => {}
        }
    }

    /// Send the given IPC to a running rustty. Returns `true` if the action was
    /// able to be performed, `false` otherwise.
    ///
    /// Note that this is a static function. Since this is called from a CLI app (or
    /// some other process that is not rustty) there is no full-featured apprt App
    /// to use.
    //
    // PORT NOTE: reference `performIpc` is a STATIC fn (no `self`) — an associated
    // fn here (cf. `none::App::perform_ipc`). Its `comptime action: apprt.ipc.Action.Key`
    // + `_: apprt.ipc.Action.Value(action)` combine into the single runtime
    // `ipc::Action`. The inferred `!bool` error set narrows to `ipc::Errors`. The
    // apprt-api `App::perform_ipc(&self)` trait method routes to this (below).
    pub fn performIpc<A: Allocator>(
        _alloc: A,
        _target: ipc::Target<'_>,
        action: ipc::Action<'_>,
    ) -> Result<bool, ipc::Errors> {
        // switch (action) {
        match action {
            // .new_window => return false,
            ipc::Action::NewWindow(_) => Ok(false),
            // .toggle_quick_terminal => return false,
            ipc::Action::ToggleQuickTerminal => Ok(false),
        }
    }
}

/// The target of a key event. This is used to determine some subtly
/// different behavior between app and surface key events.
//
// PORT NOTE: the reference nests this as `App.KeyTarget`; a top-level enum here.
pub enum KeyTarget {
    // app,
    App,
    // surface: *Surface,
    Surface(*mut Surface),
}

impl ApprtApp for App {
    type Surface = Surface;

    /// Perform a given action. Returns `true` if the action was able to be
    /// performed, `false` otherwise.
    //
    // PORT NOTE: the reference `performAction` is a comptime-generic method
    // (`comptime action: apprt.Action.Key`, `value: apprt.Action.Value(action)`);
    // the apprt-api seam projects it to a single runtime `Action`. `self: *App`
    // (mutable) is reconciled with the trait's `&self` via the `config` `UnsafeCell`
    // (the only self-mutation is `performPreAction`'s `config_change`).
    fn perform_action<C: CoreSurfaceHandle>(
        &self,
        target: Target<'_, C>,
        action: Action<'_>,
    ) -> Result<bool, ApprtError> {
        // Special case certain actions before they are sent to the
        // embedded apprt.
        // self.performPreAction(target, action, value);
        self.performPreAction(&target, &action);

        // log.debug("dispatching action target={t} action={} value={any}", .{ target, action, value });
        // (PORT: no logging shim — the reference `log.debug` is preserved as this
        // comment, the established core/termio convention.)

        // return self.opts.action(self, target.cval(), @unionInit(apprt.Action, @tagName(action), value).cval());
        //
        // PORT NOTE: the callback receives the reference's mutable `self: *App`;
        // recovered as a raw `*mut App` pointer cast (sound to hand to the C
        // callback — it is not dereferenced as a Rust `&mut` here).
        let self_ptr = core::ptr::from_ref(self).cast_mut();
        Ok((self.opts.action)(self_ptr, target.cval(), action.cval()))
    }

    fn perform_ipc(
        &self,
        target: ipc::Target<'_>,
        action: ipc::Action<'_>,
    ) -> Result<bool, ipc::Errors> {
        // The reference `performIpc` is a STATIC fn; the trait entry routes to it.
        // The allocator is unused by the embedded impl (reference `_: Allocator`);
        // pass the process `Global` allocator.
        App::performIpc(Global, target, action)
    }

    fn wakeup(&self) {
        App::wakeup(self)
    }

    fn config(&self) -> &Config {
        // config: Config (owned by this structure), returned as a shared ref.
        // (Interior-mutable via `UnsafeCell` — see the `App.config` field note.)
        unsafe { &*self.config.get() }
    }

    fn keyboard_layout(&self) -> Layout {
        self.keyboardLayout()
    }
}

// =====================================================================
// [E1 -> E2 SEQUENCING STUB] embedded::Surface
//
// The full `embedded::Surface` (reference `pub const Surface = struct { ... }`,
// embedded.zig:410-609 — its field set, `Surface.Options`, `init`/`deinit`, the
// surface event callbacks, and the `impl apprt_api::Surface` growth-point bodies)
// lands in chunk E2/E3 (EMBEDDED_BLUEPRINT §7). This MINIMAL stub exists ONLY so
// E1's `impl apprt_api::App for App` (which binds `type Surface = Surface`,
// forcing `Surface: apprt_api::Surface`) and the App methods that name `Surface`
// (`newSurface`/`closeSurface`/`redrawInspector`, `keyEvent`'s surface arm,
// `performPreAction`'s `set_title` arm) compile and gate green now. Every
// apprt-api trait method + `init`/`queueInspectorRender` is an `unimplemented!()`
// placeholder leaf; the fields are the exact reference subset those E1 sites read.
// E2 replaces this whole block with the faithful port.
// =====================================================================

pub struct Surface {
    // reference `core_surface: CoreSurface` (embedded.zig:414) — read by
    // `App::keyEvent`'s surface arm (`surface.core_surface.keyCallback`). E2 adds
    // the full field set (app/platform/userdata/content_scale/size/cursor_pos/
    // inspector/…).
    pub core_surface: CoreSurface,

    /// The current title of the surface. The embedded apprt saves this so
    /// that getTitle works without the implementer needing to save it.
    //
    // reference `title: ?[:0]const u8 = null` (embedded.zig:422) — written by
    // `performPreAction`'s `set_title` arm (`surface.rt_surface.title = ...`).
    pub title: Option<CString>,
}

/// [E1 -> E2 SEQUENCING STUB] reference `Surface.Options` — the
/// `ghostty_surface_config_s` extern struct (embedded.zig:425-465). The faithful
/// field-for-field extern struct lands in E2; this empty placeholder only types
/// [`App::newSurface`]'s `opts` parameter so E1 compiles.
pub struct SurfaceOptions;

impl Surface {
    /// reference `pub fn init(self: *Surface, app: *App, opts: Options) !void`
    /// (embedded.zig:467) — chunk E2. Out-pointer form (`surface` is the
    /// uninitialized heap allocation) preserving the reference placement-init +
    /// [ADV-FIX 8] PINNING (core `Surface::init` writes
    /// `core_surface.rt_surface = &*surface`, so the embedded Surface is
    /// heap-boxed and NEVER moved after init).
    ///
    /// # Safety
    ///
    /// `surface` must point at a live, uninitialized `Surface`-sized allocation
    /// and `app` at a live, pinned [`App`].
    pub unsafe fn init(
        _surface: *mut Surface,
        _app: *mut App,
        _opts: SurfaceOptions,
    ) -> Result<(), ApprtError> {
        unimplemented!("embedded::Surface::init — chunk E2 (embedded.zig 343-609)")
    }

    /// reference `queueInspectorRender` — chunk E3/E4 (Inspector; Phase-3
    /// dcimgui/Metal leaf). Called by [`App::redrawInspector`].
    pub fn queueInspectorRender(&mut self) {
        unimplemented!("embedded::Surface::queueInspectorRender — chunk E3/E4")
    }
}

// [E1 -> E2 SEQUENCING STUB] see the block header above. E2 fills these with the
// faithful embedded.zig:597-990 bodies (the EMBEDDED_BLUEPRINT §2 growth-point
// table).
impl ApprtSurface for Surface {
    type App = App;
    type Core = CoreSurface;

    fn core(&self) -> *mut Self::Core {
        unimplemented!("embedded::Surface::core — chunk E2")
    }

    fn rt_app(&self) -> *mut Self::App {
        unimplemented!("embedded::Surface::rt_app — chunk E2")
    }

    fn close(&self, _process_alive: bool) {
        unimplemented!("embedded::Surface::close — chunk E2")
    }

    fn deinit(&mut self) {
        unimplemented!("embedded::Surface::deinit — chunk E2")
    }

    fn get_content_scale(&self) -> Result<ContentScale, ApprtError> {
        unimplemented!("embedded::Surface::get_content_scale — chunk E2")
    }

    fn get_size(&self) -> Result<SurfaceSize, ApprtError> {
        unimplemented!("embedded::Surface::get_size — chunk E2")
    }

    fn get_title(&self) -> Option<&CStr> {
        unimplemented!("embedded::Surface::get_title — chunk E2")
    }

    fn supports_clipboard(&self, _clipboard_type: Clipboard) -> bool {
        unimplemented!("embedded::Surface::supports_clipboard — chunk E2")
    }

    fn clipboard_request(
        &self,
        _clipboard_type: Clipboard,
        _state: ClipboardRequest,
    ) -> Result<bool, ApprtError> {
        unimplemented!("embedded::Surface::clipboard_request — chunk E2")
    }

    fn set_clipboard(
        &self,
        _clipboard_type: Clipboard,
        _contents: &[rustty_apprt_api::structs::ClipboardContent<'_>],
        _confirm: bool,
    ) -> Result<(), ApprtError> {
        unimplemented!("embedded::Surface::set_clipboard — chunk E2")
    }

    fn get_cursor_pos(&self) -> Result<CursorPos, ApprtError> {
        unimplemented!("embedded::Surface::get_cursor_pos — chunk E2")
    }

    fn default_termio_env(&self) -> Result<std::collections::HashMap<String, String>, ApprtError> {
        unimplemented!("embedded::Surface::default_termio_env — chunk E2")
    }
}

/// [E1 -> E2 SEQUENCING STUB] reference `CAPI.ClipboardContent` — the
/// `ghostty_clipboard_content_s` extern struct (embedded.zig:1287). The reference
/// nests it in the `CAPI` namespace (which the blueprint maps to the separate
/// `rustty-apprt-c` crate, E5), but [`Options::write_clipboard`] — a BACKEND field
/// — references it, so it must live in this crate (`rustty-apprt-c` deps
/// `rustty-apprt`, not the reverse). E5/apprt-c hand-pins / re-exports this exact
/// extern struct for the generated header (ADV-FIX 9 pub-widening).
#[repr(C)]
pub struct ClipboardContent {
    // mime: [*:0]const u8,
    pub mime: *const c_char,
    // data: [*:0]const u8,
    pub data: *const c_char,
}
