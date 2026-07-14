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
//! PORT STATUS: in-progress (chunks E1+E2+E3+E4 landed). E1 ported `embedded.zig:29-342`
//! — the `App` struct with its `Options` (the `#[repr(C)]` host-callback config),
//! `KeyEvent`, `KeyTarget`, and every `App` method (init/terminate/
//! hasGlobalKeybinds/focusEvent/keyEvent/reloadKeymap/keyboardLayout/wakeup/wait/
//! newSurface/closeSurface/redrawInspector/performAction/performPreAction/
//! performIpc) — plus `impl apprt_api::App for App` (the first concrete backend to
//! inhabit the seam). E2 ports `embedded.zig:343-609` — `Platform`/`PlatformTag`/
//! `EnvVar` (raw `*mut c_void` nsview/uiview, NO objc dep — blueprint §5/§8.1) +
//! the full `Surface` struct + `SurfaceOptions` + `init`/`deinit` + the faithful
//! `impl apprt_api::Surface for Surface` growth-point table (§2). This proves the
//! self-referential `type Core = rustty_core::Surface<embedded::App>` bound against
//! a REAL backend (core's C0 gate used a dummy `A`). E3 ports `embedded.zig:610-990`
//! — the Surface event-sink callbacks (`refresh`/`draw`/`updateContentScale`/
//! `updateSize`/`colorSchemeCallback`/`mouseButtonCallback`/`mousePressureCallback`/
//! `scrollCallback`/`cursorPosCallback`/`preeditCallback`/`textCallback`/
//! `focusCallback`/`occlusionCallback`) that delegate straight to
//! `core_surface.*Callback`, plus `completeClipboardRequest`, `newSurfaceOptions`,
//! and the private `cursorPosToPixels` helper. `draw`/`refresh` reach the Phase-3
//! GPU renderer leaf at RUNTIME (core `Surface::draw`/`refreshCallback` →
//! `renderer_seam` `unimplemented!()`); the delegation is ported LIVE — the panic is
//! in the leaf, not here (EMBEDDED_BLUEPRINT §1). Later chunks fill the rest
//! (EMBEDDED_BLUEPRINT §7):
//!
//! E4 ports `embedded.zig:992-1243` — the `Inspector` (dcimgui + Metal terminal
//! inspector) as a Phase-3 INHABITED PLACEHOLDER (blueprint row E4). The struct +
//! `Backend` enum are inhabited with real fields; the two NON-GPU state methods
//! `updateContentScale`/`updateSize` are REAL (the `content_scale` cache is ported
//! LIVE, the dcimgui side effects deferred); every dcimgui/Metal method
//! (`init`/`deinit`/`initMetal`/`renderMetal`/`newFrame`/`Backend::deinit` + the
//! mouse/scroll/cursor/focus/text/key callbacks) is a documented `unimplemented!()`
//! leaf — dcimgui-sys is NOT wired (the `cimgui.*`/`ImGui_ImplMetal_*`/objc bodies
//! are preserved verbatim as comments, Phase-3/R-d). `queueRender` forwards LIVE to
//! the Surface's `queueInspectorRender` (itself a Phase-3 leaf). E4 also lands
//! `Surface::initInspector` (deferred from E3's range because it constructs the real
//! `Inspector`) and replaces E2's `Inspector` E2->E4 sequencing stub.
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
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::mem::{ManuallyDrop, MaybeUninit};

use allocator_api2::alloc::{Allocator, Global};

use rustty_apprt_api::action::{Action, ActionC, Target, TargetC};
use rustty_apprt_api::ipc;
use rustty_apprt_api::structs::{
    Clipboard, ClipboardRequest, ClipboardRequestType, ColorScheme, ContentScale, CursorPos,
    SurfaceSize,
};
use rustty_apprt_api::surface::should_inherit_working_directory;
use rustty_apprt_api::{
    App as ApprtApp, ApprtError, CoreSurfaceHandle, NewSurfaceContext, Surface as ApprtSurface,
};

use rustty_args::args::zig_string_escape;

use rustty_config::command::Command;
use rustty_config::io::ReadableIO;
use rustty_config::value_types::WorkingDirectory;
use rustty_config::Config;

use rustty_input::binding;
use rustty_input::keyboard::Layout;
use rustty_input::Keymap;

use rustty_vt::input::key;
use rustty_vt::input::key::KeyEvent as CoreKeyEvent;
use rustty_vt::input::key_mods::Mods;
use rustty_vt::input::mouse;

use rustty_core::surface::{Error as CoreSurfaceError, InputEffect};

// const CoreApp = @import("../App.zig");
type CoreApp = rustty_core::app::App<App>;
// const CoreSurface = @import("../Surface.zig");
type CoreSurface = rustty_core::surface::Surface<App>;

// PORT NOTE: reference `pub const resourcesDir = internal_os.resourcesDir;`
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
    //
    // PORT NOTE (ADV-FIX 9): the reference `core` is file-private; the apprt-c
    // C-ABI wrapper `ghostty_config_key_is_binding` reaches it
    // (`event.keyEvent().core()`, embedded.zig:1474), so it is `pub` here.
    pub fn core(&self) -> Option<CoreKeyEvent> {
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
        //
        // SB/PIN NO-MOVE INVARIANT (I2 / R-E): `Box::into_raw` here lowers the box to
        // a raw `*mut Surface` IMMEDIATELY — no movable `Box<Surface>` is ever held
        // after this point, so the container's heap address is stable for its whole
        // life and the self-referential `rt_surface` back-pointer never dangles.
        // `closeSurface` frees via `ManuallyDrop` (below) WITHOUT moving. PROVEN on
        // the non-FFI Miri harness `.port/phase-2/mirihar-surface-pin` (R5):
        // `pin_shipped` CLEAN under SB+TB; the `pin_move*` negatives (relocate the
        // Surface post-init) are UB at the `rt_surface` deref under both. Do NOT
        // refactor this to return/store a `Box<Surface>` or reallocate the Surface
        // after `init` — that reintroduces the R-E dangling-back-pointer UB.
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
        // (PORT NOTE: no logging shim — the reference `log.debug` is preserved as this
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
// embedded::Surface backend — chunk E2 (embedded.zig 343-609)
// =====================================================================

/// Platform-specific configuration for libghostty.
//
// PORT NOTE (blueprint §5/§8.1): the reference `MacOS`/`IOS` wrap the host view in
// an `objc.Object` (`nsview: objc.Object`); rustty stores the RAW `*mut c_void`
// instead — the view is never *messaged* until Phase-3 rendering, so the embedded
// milestone needs NO objc2/coretext dependency (the Darwin metal/quicklook block
// that WOULD need it is a deferred Phase-3 `#[cfg]` leaf). The reference
// `if isDarwin() struct {…} else void` gating is preserved via
// `#[cfg(any(target_os = "macos", target_os = "ios"))]` (the rustty-os `file.rs`
// isDarwin precedent).
//
// reference `pub const Platform = union(PlatformTag) { macos: MacOS, ios: IOS }`.
pub enum Platform {
    // macos: MacOS,
    MacOS(MacOS),
    // ios: IOS,
    IOS(IOS),
}

// If our build target for libghostty is not darwin then we do
// not include macos support at all.
//
// pub const MacOS = if (builtin.target.os.tag.isDarwin()) struct {
//     /// The view to render the surface on.
//     nsview: objc.Object,
// } else void;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub struct MacOS {
    /// The view to render the surface on.
    //
    // reference `nsview: objc.Object`; the raw view pointer (blueprint §5/§8.1 —
    // never messaged pre-render, so no objc wrap).
    pub nsview: *mut c_void,
}
/// reference `MacOS = … else void` (non-darwin).
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub struct MacOS;

// pub const IOS = if (builtin.target.os.tag.isDarwin()) struct {
//     /// The view to render the surface on.
//     uiview: objc.Object,
// } else void;
#[cfg(any(target_os = "macos", target_os = "ios"))]
pub struct IOS {
    /// The view to render the surface on.
    //
    // reference `uiview: objc.Object`; the raw view pointer (blueprint §5/§8.1).
    pub uiview: *mut c_void,
}
/// reference `IOS = … else void` (non-darwin).
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
pub struct IOS;

// The C ABI compatible version of this union. The tag is expected
// to be stored elsewhere.
//
// PORT NOTE: the reference nests this as `Platform.C`; a top-level `PlatformC`
// here (Rust cannot nest a type in an enum's namespace). Modeled as a `#[repr(C)]`
// union mirroring the reference `extern union` (the apprt-api `TargetCValue`/
// `ActionCValue` D2 precedent), pinned by the size/align const-asserts below; both
// members are a single pointer, so no explicit `_padding` field is required.
//
// pub const C = extern union {
//     macos: extern struct { nsview: ?*anyopaque },
//     ios: extern struct { uiview: ?*anyopaque },
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub union PlatformC {
    pub macos: PlatformCMacOS,
    pub ios: PlatformCIOS,
}

/// reference `Platform.C.macos` — `extern struct { nsview: ?*anyopaque }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PlatformCMacOS {
    // nsview: ?*anyopaque,
    pub nsview: *mut c_void,
}

/// reference `Platform.C.ios` — `extern struct { uiview: ?*anyopaque }`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PlatformCIOS {
    // uiview: ?*anyopaque,
    pub uiview: *mut c_void,
}

// D2 const-asserts (blueprint §3): the `Platform.C` union crosses the C ABI, so
// its size/align are pinned. Both members are one pointer wide → the union is
// exactly pointer-sized (width-portable assert; cf. apprt-api `ActionCValue`).
const _: () = assert!(core::mem::size_of::<PlatformC>() == core::mem::size_of::<*mut c_void>());
const _: () = assert!(core::mem::align_of::<PlatformC>() == core::mem::align_of::<*mut c_void>());

/// The error set of [`Platform::init`].
//
// PORT NOTE: the reference `Platform.init` returns `!Platform` with an inferred
// error set — `std.meta.intToEnum`'s `InvalidEnumTag`, plus `NSViewMustBeSet` /
// `UIViewMustBeSet` / `UnsupportedPlatform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformError {
    // std.meta.intToEnum failure
    InvalidEnumTag,
    // error.NSViewMustBeSet
    NSViewMustBeSet,
    // error.UIViewMustBeSet
    UIViewMustBeSet,
    // error.UnsupportedPlatform
    UnsupportedPlatform,
}

impl Platform {
    /// Initialize a Platform a tag and configuration from the C ABI.
    //
    // reference `pub fn init(tag_int: c_int, c_platform: C) !Platform`.
    pub fn init(tag_int: c_int, c_platform: PlatformC) -> Result<Platform, PlatformError> {
        // On non-darwin both branches are `UnsupportedPlatform` and never read
        // `c_platform`; mark it used so `-D warnings` stays green there (on darwin
        // the union is read below).
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let _ = &c_platform;

        // const tag = try std.meta.intToEnum(PlatformTag, tag_int);
        let tag = match tag_int {
            // macos = 1,
            1 => PlatformTag::MacOS,
            // ios = 2,
            2 => PlatformTag::IOS,
            // std.meta.intToEnum → error.InvalidEnumTag on an out-of-range value.
            _ => return Err(PlatformError::InvalidEnumTag),
        };

        // return switch (tag) {
        match tag {
            // .macos => if (MacOS != void) macos: { ... } else error.UnsupportedPlatform,
            PlatformTag::MacOS => {
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                {
                    // const config = c_platform.macos;
                    let config = unsafe { c_platform.macos };
                    // const nsview = objc.Object.fromId(config.nsview orelse
                    //     break :macos error.NSViewMustBeSet);
                    //
                    // PORT NOTE: no objc wrap (blueprint §5); the raw pointer is
                    // stored directly. `?*anyopaque orelse error` → null-check.
                    if config.nsview.is_null() {
                        return Err(PlatformError::NSViewMustBeSet);
                    }
                    // break :macos .{ .macos = .{ .nsview = nsview } };
                    Ok(Platform::MacOS(MacOS {
                        nsview: config.nsview,
                    }))
                }
                #[cfg(not(any(target_os = "macos", target_os = "ios")))]
                {
                    // else error.UnsupportedPlatform
                    Err(PlatformError::UnsupportedPlatform)
                }
            }

            // .ios => if (IOS != void) ios: { ... } else error.UnsupportedPlatform,
            PlatformTag::IOS => {
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                {
                    // const config = c_platform.ios;
                    let config = unsafe { c_platform.ios };
                    // const uiview = objc.Object.fromId(config.uiview orelse
                    //     break :ios error.UIViewMustBeSet);
                    if config.uiview.is_null() {
                        return Err(PlatformError::UIViewMustBeSet);
                    }
                    // break :ios .{ .ios = .{ .uiview = uiview } };
                    Ok(Platform::IOS(IOS {
                        uiview: config.uiview,
                    }))
                }
                #[cfg(not(any(target_os = "macos", target_os = "ios")))]
                {
                    // else error.UnsupportedPlatform
                    Err(PlatformError::UnsupportedPlatform)
                }
            }
        }
    }
}

// pub const PlatformTag = enum(c_int) { ... };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)] // enum(c_int)
pub enum PlatformTag {
    // "0" is reserved for invalid so we can detect unset values
    // from the C API.

    // macos = 1,
    MacOS = 1,
    // ios = 2,
    IOS = 2,
}

// pub const EnvVar = extern struct { ... };
#[repr(C)]
pub struct EnvVar {
    /// The name of the environment variable.
    // key: [*:0]const u8,
    pub key: *const c_char,

    /// The value of the environment variable.
    // value: [*:0]const u8,
    pub value: *const c_char,
}

// pub const Surface = struct { ... };
//
// PORT NOTE (ADV-FIX 9): the reference fields are file-private; the apprt-c C-ABI
// wrappers reach them, so they are `pub` here.
pub struct Surface {
    // app: *App,
    pub app: *mut App,
    // platform: Platform,
    pub platform: Platform,
    // userdata: ?*anyopaque = null,
    pub userdata: *mut c_void,
    // core_surface: CoreSurface,
    pub core_surface: CoreSurface,
    // content_scale: apprt.ContentScale,
    pub content_scale: ContentScale,
    // size: apprt.SurfaceSize,
    pub size: SurfaceSize,
    // cursor_pos: apprt.CursorPos,
    pub cursor_pos: CursorPos,
    // inspector: ?*Inspector = null,
    pub inspector: Option<*mut Inspector>,

    /// The current title of the surface. The embedded apprt saves this so
    /// that getTitle works without the implementer needing to save it.
    //
    // title: ?[:0]const u8 = null,
    //
    // PORT NOTE: `?[:0]const u8` → an owned NUL-terminated `Option<CString>`
    // (written by `App::performPreAction`'s `set_title` arm via `rt_surface`).
    pub title: Option<CString>,
}

/// Surface initialization options.
//
// PORT NOTE: the reference nests this as `Surface.Options` (the
// `ghostty_surface_config_s` extern struct, embedded.zig:425); a top-level
// `#[repr(C)] SurfaceOptions` here (ADV-FIX 9: reached by the apprt-c wrappers /
// backend `App::newSurface`). Field defaults (Zig `= …`) are not expressible on a
// Rust struct — the C host (or `ghostty_surface_config_new`, E6) supplies a
// fully-zeroed/populated value. The `?[*:0]const u8` / `?[*]EnvVar` / `?*anyopaque`
// nullable pointers map to nullable raw pointers; `callconv`-free, so no lint.
#[repr(C)]
pub struct SurfaceOptions {
    /// The platform that this surface is being initialized for and
    /// the associated platform-specific configuration.
    // platform_tag: c_int = 0,
    pub platform_tag: c_int,
    // platform: Platform.C = undefined,
    pub platform: PlatformC,

    /// Userdata passed to some of the callbacks.
    // userdata: ?*anyopaque = null,
    pub userdata: *mut c_void,

    /// The scale factor of the screen.
    // scale_factor: f64 = 1,
    pub scale_factor: f64,

    /// The font size to inherit. If 0, default font size will be used.
    // font_size: f32 = 0,
    pub font_size: f32,

    /// The working directory to load into.
    // working_directory: ?[*:0]const u8 = null,
    pub working_directory: *const c_char,

    /// The command to run in the new surface. If this is set then
    /// the "wait-after-command" option is also automatically set to true,
    /// since this is used for scripting.
    ///
    /// This command always run in a shell (e.g. via `/bin/sh -c`),
    /// despite Ghostty allowing directly executed commands via config.
    /// This is a legacy thing and we should probably change it in the
    /// future once we have a concrete use case.
    // command: ?[*:0]const u8 = null,
    pub command: *const c_char,

    /// Extra environment variables to set for the surface.
    // env_vars: ?[*]EnvVar = null,
    pub env_vars: *const EnvVar,
    // env_var_count: usize = 0,
    pub env_var_count: usize,

    /// Input to send to the command after it is started.
    // initial_input: ?[*:0]const u8 = null,
    pub initial_input: *const c_char,

    /// Wait after the command exits
    // wait_after_command: bool = false,
    pub wait_after_command: bool,

    /// Context for the new surface
    // context: apprt.surface.NewSurfaceContext = .window,
    pub context: NewSurfaceContext,
}

impl Surface {
    /// reference `pub fn init(self: *Surface, app: *App, opts: Options) !void`
    /// (embedded.zig:467). Out-pointer form (`surface` is the uninitialized heap
    /// allocation from [`App::newSurface`]) preserving the reference placement-init
    /// and [ADV-FIX 8] PINNING: core `Surface::init` writes
    /// `core_surface.rt_surface = &*surface`, so the embedded Surface is heap-boxed
    /// and NEVER moved after init.
    ///
    /// # Safety
    ///
    /// `surface` must point at a live, uninitialized `Surface`-sized allocation and
    /// `app` at a live, pinned [`App`].
    pub unsafe fn init(
        surface: *mut Surface,
        app: *mut App,
        opts: SurfaceOptions,
    ) -> Result<(), ApprtError> {
        // self.* = .{
        //     .app = app,
        //     .platform = try .init(opts.platform_tag, opts.platform),
        //     ...
        // };
        //
        // [ADV-FIX 8] out-pointer placement-init: the fallible `platform` is
        // computed FIRST so a `Platform.init` error returns before ANY field is
        // written (mirroring the reference struct-literal, where `try .init` runs
        // before the `self.* = .{…}` store commits). `core_surface` is left
        // uninitialized and written in place by `core_surface.init` below
        // (self-referential — the core `Surface::init` ptr::write idiom).
        let platform = Platform::init(opts.platform_tag, opts.platform).map_err(|_| ApprtError)?;

        unsafe {
            // .app = app,
            std::ptr::write(&raw mut (*surface).app, app);
            // .platform = try .init(...),
            std::ptr::write(&raw mut (*surface).platform, platform);
            // .userdata = opts.userdata,
            std::ptr::write(&raw mut (*surface).userdata, opts.userdata);
            // .core_surface = undefined,  (written in place by core_surface.init)
            // .content_scale = .{ .x = @floatCast(opts.scale_factor), .y = @floatCast(opts.scale_factor) },
            std::ptr::write(
                &raw mut (*surface).content_scale,
                ContentScale {
                    x: opts.scale_factor as f32,
                    y: opts.scale_factor as f32,
                },
            );
            // .size = .{ .width = 800, .height = 600 },
            std::ptr::write(
                &raw mut (*surface).size,
                SurfaceSize {
                    width: 800,
                    height: 600,
                },
            );
            // .cursor_pos = .{ .x = -1, .y = -1 },
            std::ptr::write(
                &raw mut (*surface).cursor_pos,
                CursorPos { x: -1.0, y: -1.0 },
            );
            // (field defaults) inspector: ?*Inspector = null, title: ?[:0]const u8 = null
            std::ptr::write(&raw mut (*surface).inspector, None);
            std::ptr::write(&raw mut (*surface).title, None);
        }

        // Add ourselves to the list of surfaces on the app.
        // try app.core_app.addSurface(self);
        // (PORT NOTE: rustty `addSurface` is infallible — no `try`.)
        unsafe { (*(*app).core_app).addSurface(surface) };
        // errdefer app.core_app.deleteSurface(self);
        // (an errdefer — realized manually on the fallible paths below.)

        // Shallow copy the config so that we can modify it.
        // var config = try apprt.surface.newConfig(app.core_app, &app.config, opts.context);
        // defer config.deinit();
        //
        // (PORT NOTE: rustty `new_config` is infallible; `config` has an explicit
        // `deinit` — no Drop — realized on the success and error paths below.)
        let mut config = rustty_core::app::new_config(
            unsafe { &*(*app).core_app },
            unsafe { &*(*app).config.get() },
            opts.context,
        );

        // If we have a working directory from the options then we set it.
        // if (opts.working_directory) |c_wd| {
        if !opts.working_directory.is_null() {
            // const wd = std.mem.sliceTo(c_wd, 0);
            let wd = unsafe { CStr::from_ptr(opts.working_directory) }.to_bytes();
            // if (wd.len > 0) wd: {
            if !wd.is_empty() {
                'wd: {
                    // var dir = std.fs.openDirAbsolute(wd, .{}) catch |err| {
                    //     log.warn("error opening requested working directory dir={s} err={}", .{ wd, err });
                    //     break :wd;
                    // };
                    // defer dir.close();
                    // const stat = dir.stat() catch |err| {
                    //     log.warn("failed to stat requested working directory dir={s} err={}", .{ wd, err });
                    //     break :wd;
                    // };
                    //
                    // PORT NOTE: `openDirAbsolute` + `dir.stat()` collapse to one
                    // `std::fs::metadata` (follows symlinks; opens+stats atomically),
                    // so the separate stat-error `break :wd` cannot occur once
                    // metadata succeeds — the single error maps to the open-error
                    // arm. `wd` is a byte path → `OsStr::from_bytes` (rustty-os
                    // `path.rs` precedent).
                    use std::os::unix::ffi::OsStrExt;
                    let stat = match std::fs::metadata(std::ffi::OsStr::from_bytes(wd)) {
                        Ok(md) => md,
                        Err(_err) => break 'wd,
                    };

                    // if (stat.kind != .directory) {
                    //     log.warn("requested working directory is not a directory dir={s}", .{wd});
                    //     break :wd;
                    // }
                    if !stat.is_dir() {
                        break 'wd;
                    }

                    // var wd_val: configpkg.WorkingDirectory = .{ .path = wd };
                    let mut wd_val = WorkingDirectory::Path(wd.to_vec());
                    // if (wd_val.finalize(config.arenaAlloc())) |_| {
                    //     config.@"working-directory" = wd_val;
                    // } else |err| {
                    //     log.warn("error finalizing working directory config dir={s} err={}", .{ wd_val.path, err });
                    // }
                    //
                    // PORT NOTE: rustty `WorkingDirectory::finalize` is infallible
                    // (owned allocation; `arenaAlloc` collapses), so the reference
                    // `else |err|` log arm is unreachable and elided.
                    wd_val.finalize();
                    // config.@"working-directory" = wd_val;
                    config.working_directory = Some(wd_val);
                }
            }
        }

        // If we have a command from the options then we set it.
        // if (opts.command) |c_command| {
        if !opts.command.is_null() {
            // const cmd = std.mem.sliceTo(c_command, 0);
            let cmd = unsafe { CStr::from_ptr(opts.command) }.to_bytes();
            // if (cmd.len > 0) {
            if !cmd.is_empty() {
                // config.command = .{ .shell = cmd };
                config.command = Some(Command::Shell(cmd.to_vec()));
                // config.@"wait-after-command" = true;
                config.wait_after_command = true;
            }
        }

        // Apply any environment variables that were requested.
        // if (opts.env_var_count > 0) {
        if opts.env_var_count > 0 {
            // const alloc = config.arenaAlloc();  (collapses — owned allocation)
            // for (opts.env_vars.?[0..opts.env_var_count]) |env_var| {
            let env_vars = unsafe { std::slice::from_raw_parts(opts.env_vars, opts.env_var_count) };
            for env_var in env_vars {
                // const key = std.mem.sliceTo(env_var.key, 0);
                let key = unsafe { CStr::from_ptr(env_var.key) }.to_bytes();
                // const value = std.mem.sliceTo(env_var.value, 0);
                let value = unsafe { CStr::from_ptr(env_var.value) }.to_bytes();
                // try config.env.map.put(alloc, try alloc.dupeZ(u8, key), try alloc.dupeZ(u8, value));
                config.env.map.put(key.to_vec(), value.to_vec());
            }
        }

        // If we have an initial input then we set it.
        // if (opts.initial_input) |c_input| {
        if !opts.initial_input.is_null() {
            // const alloc = config.arenaAlloc();  (collapses — owned allocation)

            // We need to escape the string because the "raw" field
            // expects a Zig string.
            // var buf: std.Io.Writer.Allocating = .init(alloc);
            // defer buf.deinit();
            // try std.zig.stringEscape(std.mem.sliceTo(c_input, 0), &buf.writer);
            let mut buf: Vec<u8> = Vec::new();
            zig_string_escape(
                &mut buf,
                unsafe { CStr::from_ptr(opts.initial_input) }.to_bytes(),
            );

            // config.input.list.clearRetainingCapacity();
            config.input.list.clear();
            // try config.input.list.append(alloc, .{ .raw = try buf.toOwnedSliceSentinel(0) });
            // (PORT NOTE: rustty `ReadableIO::Raw` stores an owned `Vec<u8>` — no NUL
            // sentinel; the Zig `toOwnedSliceSentinel(0)` sentinel is a storage
            // detail with no rustty counterpart.)
            config.input.list.push(ReadableIO::Raw(buf));
        }

        // Wait after command
        // if (opts.wait_after_command) {
        if opts.wait_after_command {
            // config.@"wait-after-command" = true;
            config.wait_after_command = true;
        }

        // Initialize our surface right away. We're given a view that is
        // ready to use.
        // try self.core_surface.init(app.core_app.alloc, &config, app.core_app, app, self);
        let init_res = unsafe {
            CoreSurface::init(
                &raw mut (*surface).core_surface,
                (*(*app).core_app).alloc,
                &config,
                (*app).core_app,
                app,
                surface,
            )
        };
        if init_res.is_err() {
            // errdefers (LIFO): `defer config.deinit()`, then
            // `errdefer app.core_app.deleteSurface(self)`.
            config.deinit();
            unsafe { (*(*app).core_app).deleteSurface(surface) };
            return Err(ApprtError);
        }
        // errdefer self.core_surface.deinit();
        // (an errdefer — realized on the setFontSize failure path below.)

        // If our options requested a specific font-size, set that.
        // if (opts.font_size != 0) {
        if opts.font_size != 0.0 {
            // var font_size = self.core_surface.font_size;
            let mut font_size = unsafe { (*surface).core_surface.font_size };
            // font_size.points = opts.font_size;
            font_size.points = opts.font_size;
            // try self.core_surface.setFontSize(font_size);
            if unsafe { (*surface).core_surface.setFontSize(font_size) }.is_err() {
                // errdefers (LIFO): `errdefer self.core_surface.deinit()`, then
                // `defer config.deinit()`, then
                // `errdefer app.core_app.deleteSurface(self)`.
                unsafe { (*surface).core_surface.deinit() };
                config.deinit();
                unsafe { (*(*app).core_app).deleteSurface(surface) };
                return Err(ApprtError);
            }
        }

        // defer config.deinit(); — the success-path realization.
        config.deinit();
        Ok(())
    }

    /// Initialize the inspector instance. A surface can only have one
    /// inspector at any given time, so this will return the previous inspector
    /// if it was already initialized.
    //
    // pub fn initInspector(self: *Surface) !*Inspector { ... }
    //
    // PORT NOTE: deferred FROM E3 (its reference range, embedded.zig:614) TO E4,
    // because it constructs the real [`Inspector`] (which E2 carried only as a
    // sequencing stub). Now that E4 provides the inhabited `Inspector`, it is ported
    // LIVE. `pub` because the E9 CAPI `ghostty_surface_inspector` wrapper reaches it
    // across the crate boundary (ADV-FIX 9). Reaches the Phase-3 dcimgui leaf
    // `Inspector::init` at RUNTIME (the panic is in the leaf), so a live inspector
    // cannot yet exist — `self.inspector` stays `None` in practice this milestone.
    pub fn initInspector(&mut self) -> Result<*mut Inspector, ApprtError> {
        // if (self.inspector) |v| return v;
        if let Some(v) = self.inspector {
            return Ok(v);
        }

        // const alloc = self.app.core_app.alloc;
        // const inspector = try alloc.create(Inspector);
        // errdefer alloc.destroy(inspector);
        // inspector.* = try .init(self);
        //
        // PORT NOTE: the reference allocates the `Inspector` slot FIRST then
        // placement-inits into it (with `errdefer alloc.destroy` to free on init
        // failure). In Rust the fallible `Inspector::init` is evaluated BEFORE the
        // heap `Box`, so an init error returns WITHOUT allocating (the `errdefer` is
        // subsumed — nothing to free). `Inspector::init` is a Phase-3 dcimgui leaf
        // (`unimplemented!()`), so this line panics once a live inspector is
        // requested — the honest Phase-3 boundary. The `Box` heap-pins the
        // `Inspector`; its `surface` back-pointer stays valid because the embedded
        // `Surface` is itself heap-pinned (ADV-FIX 8).
        let inspector = Inspector::init(core::ptr::from_mut(self))?;
        let inspector: *mut Inspector = Box::into_raw(Box::new(inspector));

        // self.inspector = inspector;
        self.inspector = Some(inspector);

        // return inspector;
        Ok(inspector)
    }

    // pub fn freeInspector(self: *Surface) void { ... }
    //
    // PORT NOTE: `initInspector`'s inverse (both land in E4). `Inspector`
    // create/deinit is a Phase-3 dcimgui+Metal leaf; reached by [`deinit`]. It is a
    // no-op while `inspector` is `None` — which it stays in practice this milestone,
    // since `initInspector` reaches the `Inspector::init` dcimgui leaf
    // (`unimplemented!()`) before it can set `inspector`.
    pub fn freeInspector(&mut self) {
        // if (self.inspector) |v| {
        if let Some(v) = self.inspector {
            // v.deinit();
            unsafe { (*v).deinit() };
            // self.app.core_app.alloc.destroy(v);
            //
            // PORT NOTE: `Inspector` is heap-boxed by `initInspector` (E4); free the
            // box WITHOUT re-running `Inspector`'s drop (the `deinit`-then-`destroy`
            // contract — the `App::closeSurface` precedent).
            unsafe { drop(Box::from_raw(v.cast::<ManuallyDrop<Inspector>>())) };
            // self.inspector = null;
            self.inspector = None;
        }
    }

    // pub fn completeClipboardRequest(
    //     self: *Surface,
    //     str: [:0]const u8,
    //     state: *apprt.ClipboardRequest,
    //     confirmed: bool,
    // ) void { ... }
    //
    // PORT NOTE (ADV-FIX 9 pub-widening): the reference is a file-private `fn`, but
    // the apprt-c `ghostty_surface_complete_clipboard_request` wrapper (E9) reaches
    // it across the crate boundary, so it is `pub` here. `str: [:0]const u8` → a
    // borrowed `&CStr`: the NUL sentinel is LOAD-BEARING — `str.ptr` is handed to
    // the host `confirm_read_clipboard` callback as `[*:0]const u8`, while the `[]`
    // view (`.to_bytes()`, sans sentinel) goes to core. `state` stays a raw
    // `*mut ClipboardRequest` (the heap request from [`Surface::clipboard_request`],
    // Box-allocated there).
    pub fn completeClipboardRequest(
        &mut self,
        str: &CStr,
        state: *mut ClipboardRequest,
        confirmed: bool,
    ) {
        // const alloc = self.app.core_app.alloc;
        // (PORT NOTE: `state` is a Rust `Box` from `clipboard_request`; it is reclaimed +
        // freed via `Box::from_raw` below — no core-app `alloc` handle is needed.)

        // Attempt to complete the request, but we may request
        // confirmation.
        // self.core_surface.completeClipboardRequest(state.*, str, confirmed) catch |err| switch (err) {
        //
        // PORT NOTE: the Zig `catch |err| switch (err)` is rendered as a `match` on
        // the `Result` — the `Ok(())` arm is the reference's no-error path (falls
        // through to `alloc.destroy` below).
        match self.core_surface.completeClipboardRequest(
            unsafe { *state },
            str.to_bytes(),
            confirmed,
        ) {
            Ok(()) => {}

            // error.UnsafePaste,
            // error.UnauthorizedPaste,
            // => { ... return; },
            Err(CoreSurfaceError::UnsafePaste | CoreSurfaceError::UnauthorizedPaste) => {
                // self.app.opts.confirm_read_clipboard(
                //     self.userdata,
                //     str.ptr,
                //     state,
                //     state.*,
                // );
                //
                // PORT NOTE: the reference's 4th arg `state.*` (an
                // `apprt.ClipboardRequest`) IMPLICITLY coerces to the callback's
                // `apprt.ClipboardRequestType` param (Zig tagged-union → tag enum).
                // Rust has no such coercion — the tag is projected explicitly.
                let state_type = match unsafe { *state } {
                    ClipboardRequest::Paste => ClipboardRequestType::Paste,
                    ClipboardRequest::Osc52Read(_) => ClipboardRequestType::Osc52Read,
                    ClipboardRequest::Osc52Write(_) => ClipboardRequestType::Osc52Write,
                };
                unsafe {
                    ((*self.app).opts.confirm_read_clipboard)(
                        self.userdata,
                        str.as_ptr(),
                        state,
                        state_type,
                    );
                }

                // return;
                return;
            }

            // else => log.err("error completing clipboard request err={}", .{err}),
            Err(_err) => {
                // log.err("error completing clipboard request err={}", .{err});
            }
        }

        // We don't defer this because the clipboard confirmation route
        // preserves the clipboard request.
        // alloc.destroy(state);
        //
        // (PORT NOTE: reclaim + drop the `Box` allocated in [`Surface::clipboard_request`].)
        unsafe { drop(Box::from_raw(state)) };
    }

    // pub fn refresh(self: *Surface) void { ... }
    pub fn refresh(&mut self) {
        // self.core_surface.refreshCallback() catch |err| {
        //     log.err("error in refresh callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.refreshCallback() {
            // log.err("error in refresh callback err={}", .{err});
        }
    }

    // pub fn draw(self: *Surface) void { ... }
    //
    // PORT NOTE: `draw` reaches the Phase-3 GPU renderer leaf at RUNTIME (core
    // `Surface::draw` → the raw-self `draw_frame_raw` chain, W5); the delegation is
    // ported LIVE (EMBEDDED_BLUEPRINT §1).
    //
    // W5 (`W5_DESIGN §2.5` caller 2 / §3.5): RAW-self, per the Zig `self: *Surface`
    // receiver. `core_surface` is embedded by value (`:889`), so an `&mut self`
    // receiver would protect it against the concurrent render thread; the raw-self
    // cascade terminates at the C-ABI caller (`rustty_surface_draw`).
    //
    /// # Safety
    /// `self_` is the stable embedded Surface allocation; no protected `&mut Surface`
    /// may span the call.
    pub unsafe fn draw(self_: *mut Surface) {
        // self.core_surface.draw() catch |err| {
        //     log.err("error in draw err={}", .{err});
        //     return;
        // };
        if let Err(_err) = unsafe { CoreSurface::draw(&raw mut (*self_).core_surface) } {
            // log.err("error in draw err={}", .{err});
        }
    }

    // pub fn updateContentScale(self: *Surface, x: f64, y: f64) void { ... }
    pub fn updateContentScale(&mut self, x: f64, y: f64) {
        // We are an embedded API so the caller can send us all sorts of
        // garbage. We want to make sure that the float values are valid
        // and we don't want to support fractional scaling below 1.
        // const x_scaled = @max(1, if (std.math.isNan(x)) 1 else x);
        let x_scaled = (if x.is_nan() { 1.0 } else { x }).max(1.0);
        // const y_scaled = @max(1, if (std.math.isNan(y)) 1 else y);
        let y_scaled = (if y.is_nan() { 1.0 } else { y }).max(1.0);

        // self.content_scale = .{
        //     .x = @floatCast(x_scaled),
        //     .y = @floatCast(y_scaled),
        // };
        self.content_scale = ContentScale {
            x: x_scaled as f32,
            y: y_scaled as f32,
        };

        // self.core_surface.contentScaleCallback(self.content_scale) catch |err| {
        //     log.err("error in content scale callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.contentScaleCallback(self.content_scale) {
            // log.err("error in content scale callback err={}", .{err});
        }
    }

    // pub fn updateSize(self: *Surface, width: u32, height: u32) void { ... }
    pub fn updateSize(&mut self, width: u32, height: u32) {
        // Runtimes sometimes generate superfluous resize events even
        // if the size did not actually change (SwiftUI). We check
        // that the size actually changed from what we last recorded
        // since resizes are expensive.
        // if (self.size.width == width and self.size.height == height) return;
        if self.size.width == width && self.size.height == height {
            return;
        }

        // self.size = .{
        //     .width = width,
        //     .height = height,
        // };
        self.size = SurfaceSize { width, height };

        // Call the primary callback.
        // self.core_surface.sizeCallback(self.size) catch |err| {
        //     log.err("error in size callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.sizeCallback(self.size) {
            // log.err("error in size callback err={}", .{err});
        }
    }

    // pub fn colorSchemeCallback(self: *Surface, scheme: apprt.ColorScheme) void { ... }
    pub fn colorSchemeCallback(&mut self, scheme: ColorScheme) {
        // self.core_surface.colorSchemeCallback(scheme) catch |err| {
        //     log.err("error setting color scheme err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.colorSchemeCallback(scheme) {
            // log.err("error setting color scheme err={}", .{err});
        }
    }

    // pub fn mouseButtonCallback(
    //     self: *Surface,
    //     action: input.MouseButtonState,
    //     button: input.MouseButton,
    //     mods: input.Mods,
    // ) bool { ... }
    pub fn mouseButtonCallback(
        &mut self,
        action: mouse::ButtonState,
        button: mouse::Button,
        mods: Mods,
    ) -> bool {
        // return self.core_surface.mouseButtonCallback(action, button, mods) catch |err| {
        //     log.err("error in mouse button callback err={}", .{err});
        //     return false;
        // };
        //
        // PARITY: not `.unwrap_or(false)` — the arm IS the reference's catch block
        // with its log site (the core surface.rs `performAction`-result precedent).
        #[allow(clippy::manual_unwrap_or, clippy::manual_unwrap_or_default)]
        match self.core_surface.mouseButtonCallback(action, button, mods) {
            Ok(v) => v,
            Err(_err) => {
                // log.err("error in mouse button callback err={}", .{err});
                false
            }
        }
    }

    // pub fn mousePressureCallback(
    //     self: *Surface,
    //     stage: input.MousePressureStage,
    //     pressure: f64,
    // ) void { ... }
    pub fn mousePressureCallback(&mut self, stage: mouse::PressureStage, pressure: f64) {
        // self.core_surface.mousePressureCallback(stage, pressure) catch |err| {
        //     log.err("error in mouse pressure callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.mousePressureCallback(stage, pressure) {
            // log.err("error in mouse pressure callback err={}", .{err});
        }
    }

    // pub fn scrollCallback(
    //     self: *Surface,
    //     xoff: f64,
    //     yoff: f64,
    //     mods: input.ScrollMods,
    // ) void { ... }
    pub fn scrollCallback(&mut self, xoff: f64, yoff: f64, mods: mouse::ScrollMods) {
        // self.core_surface.scrollCallback(xoff, yoff, mods) catch |err| {
        //     log.err("error in scroll callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.scrollCallback(xoff, yoff, mods) {
            // log.err("error in scroll callback err={}", .{err});
        }
    }

    // pub fn cursorPosCallback(
    //     self: *Surface,
    //     x: f64,
    //     y: f64,
    //     mods: input.Mods,
    // ) void { ... }
    pub fn cursorPosCallback(&mut self, x: f64, y: f64, mods: Mods) {
        // Convert our unscaled x/y to scaled.
        // const pos = self.cursorPosToPixels(.{
        //     .x = @floatCast(x),
        //     .y = @floatCast(y),
        // }) catch |err| {
        //     log.err(
        //         "error converting cursor pos to scaled pixels in cursor pos callback err={}",
        //         .{err},
        //     );
        //     return;
        // };
        let pos = match self.cursorPosToPixels(CursorPos {
            x: x as f32,
            y: y as f32,
        }) {
            Ok(v) => v,
            Err(_err) => {
                // log.err(
                //     "error converting cursor pos to scaled pixels in cursor pos callback err={}",
                //     .{err},
                // );
                return;
            }
        };

        // There are cases where the platform reports a mouse motion event
        // without the cursor actually moving. For example, on macOS, updating
        // the window title can trigger a phantom mouse-move event at the same
        // coordinates. This can cause the mouse to incorrectly unhide when
        // mouse-hide-while-typing is enabled (commonly seen with TUI apps
        // like Zellij that frequently update the title). To prevent incorrect
        // behavior, we only continue with callback logic if the cursor has
        // actually moved.
        // if (@abs(self.cursor_pos.x - pos.x) < 1 and
        //     @abs(self.cursor_pos.y - pos.y) < 1) return;
        if (self.cursor_pos.x - pos.x).abs() < 1.0 && (self.cursor_pos.y - pos.y).abs() < 1.0 {
            return;
        }

        // self.cursor_pos = pos;
        self.cursor_pos = pos;

        // self.core_surface.cursorPosCallback(self.cursor_pos, mods) catch |err| {
        //     log.err("error in cursor pos callback err={}", .{err});
        //     return;
        // };
        //
        // PORT NOTE: core `cursorPosCallback` takes `mods: ?input.Mods`; the
        // reference's non-optional `mods` coerces to `Some`.
        if let Err(_err) = self
            .core_surface
            .cursorPosCallback(self.cursor_pos, Some(mods))
        {
            // log.err("error in cursor pos callback err={}", .{err});
        }
    }

    // pub fn preeditCallback(self: *Surface, preedit_: ?[]const u8) void { ... }
    pub fn preeditCallback(&mut self, preedit_: Option<&[u8]>) {
        // _ = self.core_surface.preeditCallback(preedit_) catch |err| {
        //     log.err("error in preedit callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.preeditCallback(preedit_) {
            // log.err("error in preedit callback err={}", .{err});
        }
    }

    // pub fn textCallback(self: *Surface, text: []const u8) void { ... }
    pub fn textCallback(&mut self, text: &[u8]) {
        // _ = self.core_surface.textCallback(text) catch |err| {
        //     log.err("error in key callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.textCallback(text) {
            // log.err("error in key callback err={}", .{err});
        }
    }

    // pub fn focusCallback(self: *Surface, focused: bool) void { ... }
    pub fn focusCallback(&mut self, focused: bool) {
        // self.core_surface.focusCallback(focused) catch |err| {
        //     log.err("error in focus callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.focusCallback(focused) {
            // log.err("error in focus callback err={}", .{err});
        }
    }

    // pub fn occlusionCallback(self: *Surface, visible: bool) void { ... }
    pub fn occlusionCallback(&mut self, visible: bool) {
        // self.core_surface.occlusionCallback(visible) catch |err| {
        //     log.err("error in occlusion callback err={}", .{err});
        //     return;
        // };
        if let Err(_err) = self.core_surface.occlusionCallback(visible) {
            // log.err("error in occlusion callback err={}", .{err});
        }
    }

    /// reference `fn queueInspectorRender(self: *Surface) void` (embedded.zig:922)
    /// — chunk E3/E4. It reaches the `render_inspector` action (dcimgui/Metal
    /// Phase-3 leaf); called by [`App::redrawInspector`]. Kept an `unimplemented!()`
    /// placeholder leaf here.
    pub fn queueInspectorRender(&mut self) {
        unimplemented!("embedded::Surface::queueInspectorRender — chunk E3/E4")
    }

    // pub fn newSurfaceOptions(self: *const Surface, context: apprt.surface.NewSurfaceContext) apprt.Surface.Options { ... }
    //
    // PORT NOTE: `apprt.Surface.Options` is the embedded backend's own
    // [`SurfaceOptions`] (the `ghostty_surface_config_s`); the reference struct-literal
    // relies on Zig field defaults for every field it does not set. Rust has no struct
    // field defaults, so the elided fields are spelled out below with those exact
    // defaults. Reached only by the E7 CAPI `ghostty_surface_inherited_config`
    // (returned BY VALUE to the C host).
    pub fn newSurfaceOptions(&self, context: NewSurfaceContext) -> SurfaceOptions {
        // PORT NOTE: `self.app.config` is read through the E1 `UnsafeCell` accessor.
        let config = unsafe { &*(*self.app).config.get() };

        // const font_size: f32 = font_size: {
        //     if (!self.app.config.@"window-inherit-font-size") break :font_size 0;
        //     break :font_size self.core_surface.font_size.points;
        // };
        let font_size: f32 = 'font_size: {
            if !config.window_inherit_font_size {
                break 'font_size 0.0;
            }
            self.core_surface.font_size.points
        };

        // const working_directory: ?[*:0]const u8 = wd: {
        //     if (!apprt.surface.shouldInheritWorkingDirectory(context, &self.app.config)) break :wd null;
        //     const cwd = self.core_surface.pwd(self.app.core_app.alloc) catch null orelse break :wd null;
        //     defer self.app.core_app.alloc.free(cwd);
        //     break :wd self.app.core_app.alloc.dupeZ(u8, cwd) catch null;
        // };
        //
        // PORT NOTE: rustty `pwd` is infallible (`Option<Vec<u8>>`; the reference's
        // `catch null` collapses to the `None` arm). `cwd` is an owned `Vec<u8>`
        // dropped at scope end (the reference `defer …free(cwd)`); `dupeZ` → a leaked
        // NUL-terminated `CString` owned by the C host (the reference leaks the GPA
        // `dupeZ` result the same way — this is `newSurfaceOptions`'s inherited-config
        // contract). An interior-NUL `cwd` (impossible for a real pwd) maps the
        // `CString::new` failure to `null`, matching the `dupeZ … catch null`.
        let working_directory: *const c_char = 'wd: {
            if !should_inherit_working_directory(context, config) {
                break 'wd core::ptr::null();
            }
            let cwd = match self
                .core_surface
                .pwd(unsafe { (*(*self.app).core_app).alloc })
            {
                Some(v) => v,
                None => break 'wd core::ptr::null(),
            };
            match CString::new(cwd) {
                Ok(c) => c.into_raw().cast_const(),
                Err(_) => core::ptr::null(),
            }
        };

        // return .{
        //     .font_size = font_size,
        //     .working_directory = working_directory,
        //     .context = context,
        // };
        SurfaceOptions {
            font_size,
            working_directory,
            context,
            // (elided reference fields → their `Surface.Options` Zig field defaults)
            // platform_tag: c_int = 0,
            platform_tag: 0,
            // platform: Platform.C = undefined,
            //
            // PORT NOTE: Zig `undefined` is never read here (the C host overwrites
            // `platform`/`platform_tag` before `ghostty_surface_new`); a zeroed union
            // is the faithful placeholder.
            platform: PlatformC {
                macos: PlatformCMacOS {
                    nsview: core::ptr::null_mut(),
                },
            },
            // userdata: ?*anyopaque = null,
            userdata: core::ptr::null_mut(),
            // scale_factor: f64 = 1,
            scale_factor: 1.0,
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
        }
    }

    /// The cursor position from the host directly is in screen coordinates but
    /// all our interface works in pixels.
    //
    // fn cursorPosToPixels(self: *const Surface, pos: apprt.CursorPos) !apprt.CursorPos { ... }
    fn cursorPosToPixels(&self, pos: CursorPos) -> Result<CursorPos, ApprtError> {
        // const scale = try self.getContentScale();
        let scale = self.get_content_scale()?;
        // return .{ .x = pos.x * scale.x, .y = pos.y * scale.y };
        Ok(CursorPos {
            x: pos.x * scale.x,
            y: pos.y * scale.y,
        })
    }
}

/// Inspector is the state required for the terminal inspector. A terminal
/// inspector is 1:1 with a Surface.
//
// pub const Inspector = struct { ... }   (embedded.zig:994)
//
// [CHUNK E4 — Phase-3 INHABITED PLACEHOLDER] The inspector is the dcimgui (Dear
// ImGui) + Metal debug UI. Per the placeholder-leaf doctrine (blueprint row E4)
// the struct is INHABITED with its real fields, and the two NON-GPU state methods
// [`Inspector::updateContentScale`]/[`Inspector::updateSize`] are REAL; every
// method that drives dcimgui or Metal (`init`/`deinit`/`initMetal`/`renderMetal`/
// `newFrame`/the mouse/scroll/cursor/focus/text/key callbacks + `Backend::deinit`)
// is a documented `unimplemented!()` leaf. dcimgui-sys is NOT wired here — the
// `cimgui.*`/`ImGui_ImplMetal_*`/objc bodies are preserved verbatim as comments and
// land when the GPU/dcimgui render path is filled (Phase-3, R-d). `queueRender`
// forwards LIVE to [`Surface::queueInspectorRender`] (itself a Phase-3 leaf — the
// draw/refresh delegation precedent).
pub struct Inspector {
    // const cimgui = @import("dcimgui");
    //
    // PORT NOTE: the reference imports the `dcimgui` binding package for the whole
    // struct. dcimgui-sys is NOT wired this milestone (blueprint E4), so the import
    // has no Rust analogue — every `cimgui.*` reference below is a documented leaf.

    // surface: *Surface,
    pub surface: *mut Surface,
    // ig_ctx: *cimgui.c.ImGuiContext,
    //
    // PORT NOTE: the Dear ImGui context handle. dcimgui-sys is not wired, so its
    // `*ImGuiContext` type has no binding here — stored as an opaque `*mut c_void`
    // (a raw C handle; only ever produced/consumed by the dcimgui leaves).
    pub ig_ctx: *mut c_void,
    // backend: ?Backend = null,
    pub backend: Option<Backend>,
    // content_scale: f64 = 1,
    pub content_scale: f64,

    /// Our previous instant used to calculate delta time for animations.
    // instant: ?std.time.Instant = null,
    pub instant: Option<std::time::Instant>,
}

// const Backend = enum { ... };
//
// PORT NOTE: the reference `Backend` is a file-private `const` enum; Rust's
// pub-field visibility (it appears in the `pub backend` field of the `pub`
// `Inspector`) requires it be at least as visible → `pub`. Its sole variant's
// render backend is Metal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    // metal,
    Metal,
}

impl Backend {
    // pub fn deinit(self: Backend) void { ... }
    //
    // PORT NOTE: dcimgui+Metal Phase-3 leaf — `ImGui_ImplMetal_Shutdown()` (Darwin).
    pub fn deinit(self) {
        // switch (self) {
        //     .metal => if (builtin.target.os.tag.isDarwin()) cimgui.ImGui_ImplMetal_Shutdown(),
        // }
        match self {
            Backend::Metal => {
                unimplemented!(
                    "embedded::Inspector::Backend::deinit (metal) — dcimgui \
                     ImGui_ImplMetal_Shutdown (Phase-3 leaf, embedded.zig:1008-1012)"
                )
            }
        }
    }
}

impl Inspector {
    // pub fn init(surface: *Surface) !Inspector { ... }
    //
    // PORT NOTE: constructs the Dear ImGui context + activates the core inspector —
    // a dcimgui + `CoreInspector.setup()` Phase-3 leaf. The reference body is
    // preserved verbatim below; `!Inspector` (its only error is `error.OutOfMemory`
    // from `ImGui_CreateContext`) maps to `Result<Inspector, ApprtError>`.
    pub fn init(surface: *mut Surface) -> Result<Inspector, ApprtError> {
        // const ig_ctx = cimgui.c.ImGui_CreateContext(null) orelse return error.OutOfMemory;
        // errdefer cimgui.c.ImGui_DestroyContext(ig_ctx);
        // cimgui.c.ImGui_SetCurrentContext(ig_ctx);
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        // io.BackendPlatformName = "ghostty_embedded";
        //
        // // Setup our core inspector
        // CoreInspector.setup();
        // surface.core_surface.activateInspector() catch |err| {
        //     log.err("failed to activate inspector err={}", .{err});
        // };
        //
        // return .{
        //     .surface = surface,
        //     .ig_ctx = ig_ctx,
        // };
        //
        // TODO(port): dcimgui ImGui_CreateContext + core activateInspector (Phase-3
        // leaf, embedded.zig:1015-1032). The verbatim body above is the not-yet-
        // ported leaf. See DEFERRALS.md "P2 crash-guard: on-feature quicklook/
        // inspector leaves".
        //
        // P2 INTERIM crash-guard (PREPORT_SWIFT_APP §4 abort policy): under
        // panic=abort an `unimplemented!()` here ABORTS the whole app the moment the
        // user OPENS the inspector — this is the FIRST reachable leaf on that feature
        // (surface_inspector → initInspector → here), aborting before any of the
        // downstream leaves (core `activateInspector`, `initMetal`/`renderMetal`) can
        // run. Return the export's own init-failure sentinel `error`: the reference
        // `rustty_surface_inspector` catches it (`initInspector() catch return null`)
        // and the macOS app null-guards it (SurfaceView_AppKit.swift:155 `guard let
        // cInspector … else { return nil }`), so the inspector simply does not open.
        // Because no live `Inspector` can then exist, the whole downstream metal +
        // `activateInspector` chain stays UNREACHABLE on-feature (nothing to guard
        // there). Recoverable feature-unavailable, no abort. R1: faithful to the
        // reference's own init-failure null path.
        let _ = surface;
        eprintln!("inspector not yet ported");
        Err(ApprtError)
    }

    // pub fn deinit(self: *Inspector) void { ... }
    //
    // PORT NOTE: core `deactivateInspector` + dcimgui context teardown — Phase-3
    // leaf. Reached by [`Surface::freeInspector`] (never this milestone: no live
    // inspector can be constructed, since `init` above is a leaf).
    pub fn deinit(&mut self) {
        // self.surface.core_surface.deactivateInspector();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        // if (self.backend) |v| v.deinit();
        // cimgui.c.ImGui_DestroyContext(self.ig_ctx);
        unimplemented!(
            "embedded::Inspector::deinit — core deactivateInspector + dcimgui \
             DestroyContext (Phase-3 leaf, embedded.zig:1034-1039)"
        )
    }

    /// Queue a render for the next frame.
    //
    // pub fn queueRender(self: *Inspector) void { ... }
    //
    // PORT NOTE: a pure delegation to [`Surface::queueInspectorRender`] (no dcimgui);
    // ported LIVE — the panic is in that Phase-3 leaf, not here (the draw/refresh
    // delegation precedent).
    pub fn queueRender(&mut self) {
        // self.surface.queueInspectorRender();
        unsafe { (*self.surface).queueInspectorRender() };
    }

    /// Initialize the inspector for a metal backend.
    //
    // pub fn initMetal(self: *Inspector, device: objc.Object) bool { ... }
    //
    // PORT NOTE: objc/Metal + dcimgui `ImGui_ImplMetal_Init` — Phase-3 leaf. The
    // objc `device: objc.Object` handle → an opaque `*mut c_void` (no objc dep this
    // milestone, blueprint §5/§8.1). Target of the E10 Darwin CAPI
    // `ghostty_inspector_metal_init`.
    pub fn initMetal(&mut self, device: *mut c_void) -> bool {
        // defer device.msgSend(void, objc.sel("release"), .{});
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        //
        // if (self.backend) |v| {
        //     v.deinit();
        //     self.backend = null;
        // }
        //
        // if (!cimgui.ImGui_ImplMetal_Init(device.value)) {
        //     log.warn("failed to initialize metal backend", .{});
        //     return false;
        // }
        // self.backend = .metal;
        //
        // log.debug("initialized metal backend", .{});
        // return true;
        let _ = device;
        unimplemented!(
            "embedded::Inspector::initMetal — objc/Metal + dcimgui ImGui_ImplMetal_Init \
             (Phase-3 leaf, embedded.zig:1047-1064)"
        )
    }

    // pub fn renderMetal(self: *Inspector, command_buffer: objc.Object, desc: objc.Object) !void { ... }
    //
    // PORT NOTE: the objc/Metal render pass (dcimgui `ImGui_ImplMetal_NewFrame`/
    // `RenderDrawData` + `MTLRenderCommandEncoder` msgSends) — Phase-3 leaf. The two
    // objc `objc.Object` handles → opaque `*mut c_void`. Target of the E10 Darwin
    // CAPI `ghostty_inspector_metal_render`.
    pub fn renderMetal(
        &mut self,
        command_buffer: *mut c_void,
        desc: *mut c_void,
    ) -> Result<(), ApprtError> {
        // defer {
        //     command_buffer.msgSend(void, objc.sel("release"), .{});
        //     desc.msgSend(void, objc.sel("release"), .{});
        // }
        // assert(self.backend == .metal);
        // //log.debug("render", .{});
        //
        // // Setup our imgui frame. We need to render multiple frames to ensure
        // // ImGui completes all its state processing. I don't know how to fix
        // // this.
        // for (0..2) |_| {
        //     cimgui.ImGui_ImplMetal_NewFrame(desc.value);
        //     try self.newFrame();
        //     cimgui.c.ImGui_NewFrame();
        //
        //     // Build our UI
        //     render: {
        //         const surface = &self.surface.core_surface;
        //         const inspector = surface.inspector orelse break :render;
        //         inspector.render(surface);
        //     }
        //
        //     // Render
        //     cimgui.c.ImGui_Render();
        // }
        //
        // // MTLRenderCommandEncoder
        // const encoder = command_buffer.msgSend(
        //     objc.Object,
        //     objc.sel("renderCommandEncoderWithDescriptor:"),
        //     .{desc.value},
        // );
        // defer encoder.msgSend(void, objc.sel("endEncoding"), .{});
        // cimgui.ImGui_ImplMetal_RenderDrawData(
        //     cimgui.c.ImGui_GetDrawData(),
        //     command_buffer.value,
        //     encoder.value,
        // );
        let _ = (command_buffer, desc);
        unimplemented!(
            "embedded::Inspector::renderMetal — objc/Metal render + dcimgui \
             ImplMetal_NewFrame/RenderDrawData (Phase-3 leaf, embedded.zig:1066-1109)"
        )
    }

    // pub fn updateContentScale(self: *Inspector, x: f64, y: f64) void { ... }
    //
    // PORT NOTE (blueprint E4: REAL, non-GPU): the one pure-Rust effect — caching
    // `content_scale` (read by `cursorPosCallback`'s pixel scaling) — is ported LIVE.
    // The dcimgui style-scaling side effect (`ImGuiStyle_ScaleAllSizes` etc.) is a
    // documented Phase-3 leaf, elided until dcimgui-sys is wired; the method does NOT
    // panic.
    pub fn updateContentScale(&mut self, x: f64, y: f64) {
        // _ = y;   (the vertical scale is unused — `x` drives both axes)
        let _ = y;
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);   [dcimgui leaf — deferred]

        // Cache our scale because we use it for cursor position calculations.
        // self.content_scale = x;
        self.content_scale = x;

        // Setup a new style and scale it appropriately. We must use the
        // ImGuiStyle constructor to get proper default values (e.g.,
        // CurveTessellationTol) rather than zero-initialized values.
        // var style: cimgui.c.ImGuiStyle = undefined;
        // cimgui.ext.ImGuiStyle_ImGuiStyle(&style);
        // cimgui.c.ImGuiStyle_ScaleAllSizes(&style, @floatCast(x));
        // const active_style = cimgui.c.ImGui_GetStyle();
        // active_style.* = style;
        // [dcimgui style-scaling leaf — deferred until dcimgui-sys is wired (Phase-3)]
    }

    // pub fn updateSize(self: *Inspector, width: u32, height: u32) void { ... }
    //
    // PORT NOTE (blueprint E4: REAL, non-GPU): the imgui IO `DisplaySize` write is a
    // dcimgui leaf, deferred until dcimgui-sys is wired (Phase-3). The Inspector
    // holds no Rust-side size field (`DisplaySize` lives in imgui's IO), so the
    // ported body accepts the size and defers — it does NOT panic. `width`/`height`
    // are function parameters (never linted for unused).
    pub fn updateSize(&mut self, width: u32, height: u32) {
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        // io.DisplaySize = .{ .x = @floatFromInt(width), .y = @floatFromInt(height) };
        //
        // (accepted-but-deferred: the imgui IO `DisplaySize` write is the only effect
        // and it is a dcimgui leaf — see the PORT NOTE above.)
        let _ = (width, height);
    }

    // pub fn mouseButtonCallback(self, action, button, mods) void { ... }
    //
    // PORT NOTE: dcimgui `ImGuiIO_AddMouseButtonEvent` (imgui button mapping) —
    // Phase-3 leaf. Target of the E9 CAPI `ghostty_inspector_mouse_button`.
    pub fn mouseButtonCallback(
        &mut self,
        action: mouse::ButtonState,
        button: mouse::Button,
        mods: Mods,
    ) {
        // _ = mods;
        //
        // self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        //
        // const imgui_button = switch (button) {
        //     .left => cimgui.c.ImGuiMouseButton_Left,
        //     .middle => cimgui.c.ImGuiMouseButton_Middle,
        //     .right => cimgui.c.ImGuiMouseButton_Right,
        //     else => return, // unsupported
        // };
        //
        // cimgui.c.ImGuiIO_AddMouseButtonEvent(io, imgui_button, action == .press);
        let _ = (action, button, mods);
        unimplemented!(
            "embedded::Inspector::mouseButtonCallback — dcimgui \
             ImGuiIO_AddMouseButtonEvent (Phase-3 leaf, embedded.zig:1134-1154)"
        )
    }

    // pub fn scrollCallback(self, xoff, yoff, mods) void { ... }
    //
    // PORT NOTE: dcimgui `ImGuiIO_AddMouseWheelEvent` — Phase-3 leaf. Target of the
    // E9 CAPI `ghostty_inspector_mouse_scroll`.
    pub fn scrollCallback(&mut self, xoff: f64, yoff: f64, mods: mouse::ScrollMods) {
        // self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        //
        // // For precision scrolling (trackpads), the values are in pixels which
        // // scroll way too fast. Scale them down to approximate discrete wheel
        // // notches. imgui expects 1.0 to scroll ~5 lines of text.
        // const scale: f64 = if (mods.precision) 0.1 else 1.0;
        // cimgui.c.ImGuiIO_AddMouseWheelEvent(
        //     io,
        //     @floatCast(xoff * scale),
        //     @floatCast(yoff * scale),
        // );
        let _ = (xoff, yoff, mods);
        unimplemented!(
            "embedded::Inspector::scrollCallback — dcimgui ImGuiIO_AddMouseWheelEvent \
             (Phase-3 leaf, embedded.zig:1156-1175)"
        )
    }

    // pub fn cursorPosCallback(self: *Inspector, x: f64, y: f64) void { ... }
    //
    // PORT NOTE: dcimgui `ImGuiIO_AddMousePosEvent` (scaled by `content_scale`) —
    // Phase-3 leaf. Target of the E9 CAPI `ghostty_inspector_mouse_pos`.
    pub fn cursorPosCallback(&mut self, x: f64, y: f64) {
        // self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        // cimgui.c.ImGuiIO_AddMousePosEvent(
        //     io,
        //     @floatCast(x * self.content_scale),
        //     @floatCast(y * self.content_scale),
        // );
        let _ = (x, y);
        unimplemented!(
            "embedded::Inspector::cursorPosCallback — dcimgui ImGuiIO_AddMousePosEvent \
             (Phase-3 leaf, embedded.zig:1177-1186)"
        )
    }

    // pub fn focusCallback(self: *Inspector, focused: bool) void { ... }
    //
    // PORT NOTE: dcimgui `ImGuiIO_AddFocusEvent` — Phase-3 leaf. Target of the E9
    // CAPI `ghostty_inspector_set_focus`.
    pub fn focusCallback(&mut self, focused: bool) {
        // self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        // cimgui.c.ImGuiIO_AddFocusEvent(io, focused);
        let _ = focused;
        unimplemented!(
            "embedded::Inspector::focusCallback — dcimgui ImGuiIO_AddFocusEvent \
             (Phase-3 leaf, embedded.zig:1188-1193)"
        )
    }

    // pub fn textCallback(self: *Inspector, text: [:0]const u8) void { ... }
    //
    // PORT NOTE: dcimgui `ImGuiIO_AddInputCharactersUTF8` — Phase-3 leaf. The
    // `[:0]const u8` NUL-terminated slice → `&CStr` (`text.ptr` is the load-bearing
    // sentinel handed to imgui). Target of the E9 CAPI `ghostty_inspector_text`.
    pub fn textCallback(&mut self, text: &CStr) {
        // self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        // cimgui.c.ImGuiIO_AddInputCharactersUTF8(io, text.ptr);
        let _ = text;
        unimplemented!(
            "embedded::Inspector::textCallback — dcimgui ImGuiIO_AddInputCharactersUTF8 \
             (Phase-3 leaf, embedded.zig:1195-1200)"
        )
    }

    // pub fn keyCallback(self, action, key, mods) !void { ... }
    //
    // PORT NOTE: dcimgui `ImGuiIO_AddKeyEvent` (modifiers + `key.imguiKey()` — the
    // `Key::imgui_key` dcimgui binding, itself a deferred leaf in rustty-vt) —
    // Phase-3 leaf. Target of the E9 CAPI `ghostty_inspector_key`.
    pub fn keyCallback(
        &mut self,
        action: key::Action,
        key: key::Key,
        mods: Mods,
    ) -> Result<(), ApprtError> {
        // self.queueRender();
        // cimgui.c.ImGui_SetCurrentContext(self.ig_ctx);
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        //
        // // Update all our modifiers
        // cimgui.c.ImGuiIO_AddKeyEvent(io, cimgui.c.ImGuiKey_LeftShift, mods.shift);
        // cimgui.c.ImGuiIO_AddKeyEvent(io, cimgui.c.ImGuiKey_LeftCtrl, mods.ctrl);
        // cimgui.c.ImGuiIO_AddKeyEvent(io, cimgui.c.ImGuiKey_LeftAlt, mods.alt);
        // cimgui.c.ImGuiIO_AddKeyEvent(io, cimgui.c.ImGuiKey_LeftSuper, mods.super);
        //
        // // Send our keypress
        // if (key.imguiKey()) |imgui_key| {
        //     cimgui.c.ImGuiIO_AddKeyEvent(
        //         io,
        //         imgui_key,
        //         action == .press or action == .repeat,
        //     );
        // }
        let _ = (action, key, mods);
        unimplemented!(
            "embedded::Inspector::keyCallback — dcimgui ImGuiIO_AddKeyEvent + \
             key.imguiKey (Phase-3 leaf, embedded.zig:1202-1226)"
        )
    }

    // fn newFrame(self: *Inspector) !void { ... }
    //
    // PORT NOTE: the imgui frame delta-time setup (`io.DeltaTime`) — Phase-3 leaf. A
    // private helper called ONLY by the deferred [`Inspector::renderMetal`], so it is
    // `#[allow(dead_code)]` until that render path goes live (E10 / Phase-3).
    #[allow(dead_code)]
    fn newFrame(&mut self) -> Result<(), ApprtError> {
        // const io: *cimgui.c.ImGuiIO = cimgui.c.ImGui_GetIO();
        //
        // // Determine our delta time
        // const now = try std.time.Instant.now();
        // io.DeltaTime = if (self.instant) |prev| delta: {
        //     const since_ns: f64 = @floatFromInt(now.since(prev));
        //     const ns_per_s: f64 = @floatFromInt(std.time.ns_per_s);
        //     const since_s: f32 = @floatCast(since_ns / ns_per_s);
        //     break :delta @max(0.00001, since_s);
        // } else (1.0 / 60.0);
        // self.instant = now;
        unimplemented!(
            "embedded::Inspector::newFrame — dcimgui io.DeltaTime delta-time setup \
             (Phase-3 leaf, embedded.zig:1228-1240)"
        )
    }
}

// The EMBEDDED_BLUEPRINT §2 growth-point table: the exact set of methods core's
// `Surface.zig`/`App.zig` call on `rt_surface`, ported faithfully from
// embedded.zig:597-982. In the reference these are plain `pub fn`s on `Surface`;
// Rust routes them through the `apprt_api::Surface` trait (the A5 seam). This is
// the FIRST concrete backend to inhabit `type Core =
// rustty_core::Surface<embedded::App>`.
impl ApprtSurface for Surface {
    type App = App;
    type Core = CoreSurface;

    // pub fn core(self: *Surface) *CoreSurface { return &self.core_surface; }
    fn core(&self) -> *mut Self::Core {
        // The returned pointer is as stable as the backend's surface allocation
        // (the embedded Surface is heap-boxed + pinned, ADV-FIX 8).
        core::ptr::from_ref(&self.core_surface).cast_mut()
    }

    // pub fn rtApp(self: *const Surface) *App { return self.app; }
    fn rt_app(&self) -> *mut Self::App {
        self.app
    }

    // W-ATTACH: expose the host view pointer + content scale so core routes
    // them into the renderer's windowed layer-attach at finalizeSurfaceInit
    // (embedded holds the raw view in `platform.macos.nsview` /
    // `platform.ios.uiview`, blueprint §5/§8.1).
    //
    // PORT NOTE (R1 parity): the reference `Metal.init` reads BOTH the macOS
    // `nsview` and the iOS `uiview` off `apprt.Surface`'s platform union; the iOS
    // arm was previously dropped (returned null), so restore it. The `nsview` /
    // `uiview` field access only exists on darwin (`MacOS`/`IOS` are ZSTs
    // off-darwin, §2.9), so both concrete arms are `#[cfg]`-gated to darwin and a
    // non-darwin wildcard returns null (the platform never renders there).
    fn host_view(&self) -> *mut c_void {
        match &self.platform {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Platform::MacOS(m) => m.nsview,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Platform::IOS(i) => i.uiview,
            #[cfg(not(any(target_os = "macos", target_os = "ios")))]
            _ => core::ptr::null_mut(),
        }
    }

    fn host_content_scale(&self) -> f64 {
        self.content_scale.x.into()
    }

    // pub fn close(self: *const Surface, process_alive: bool) void { ... }
    fn close(&self, process_alive: bool) {
        // const func = self.app.opts.close_surface orelse {
        //     log.info("runtime embedder does not support closing a surface", .{});
        //     return;
        // };
        let func = match unsafe { (*self.app).opts.close_surface } {
            Some(func) => func,
            None => {
                // log.info("runtime embedder does not support closing a surface", .{});
                return;
            }
        };

        // func(self.userdata, process_alive);
        func(self.userdata, process_alive);
    }

    // pub fn deinit(self: *Surface) void { ... }
    fn deinit(&mut self) {
        // Shut down our inspector
        // self.freeInspector();
        self.freeInspector();

        // Free our title
        // if (self.title) |v| self.app.core_app.alloc.free(v);
        // (PORT NOTE: drop the owned `CString`.)
        self.title = None;

        // Remove ourselves from the list of known surfaces in the app.
        // self.app.core_app.deleteSurface(self);
        //
        // PORT NOTE: `self.app` (a raw `*mut App`) is captured BEFORE the raw
        // `self` pointer so `deleteSurface(self_ptr)` uses only raw pointers (the
        // `&mut`-args-strongly-protected discipline — Zig pointers are unprotected).
        let app = self.app;
        let self_ptr = core::ptr::from_mut(self);
        unsafe { (*(*app).core_app).deleteSurface(self_ptr) };

        // Clean up our core surface so that all the rendering and IO stop.
        // self.core_surface.deinit();
        unsafe { self.core_surface.deinit() };
    }

    // pub fn getContentScale(self: *const Surface) !apprt.ContentScale { return self.content_scale; }
    fn get_content_scale(&self) -> Result<ContentScale, ApprtError> {
        Ok(self.content_scale)
    }

    // pub fn getSize(self: *const Surface) !apprt.SurfaceSize { return self.size; }
    fn get_size(&self) -> Result<SurfaceSize, ApprtError> {
        Ok(self.size)
    }

    // pub fn getTitle(self: *Surface) ?[:0]const u8 { return self.title; }
    fn get_title(&self) -> Option<&CStr> {
        self.title.as_deref()
    }

    // pub fn supportsClipboard(self: *const Surface, clipboard_type: apprt.Clipboard) bool { ... }
    fn supports_clipboard(&self, clipboard_type: Clipboard) -> bool {
        // return switch (clipboard_type) {
        match clipboard_type {
            // .standard => true,
            Clipboard::Standard => true,
            // .selection, .primary => self.app.opts.supports_selection_clipboard,
            Clipboard::Selection | Clipboard::Primary => unsafe {
                (*self.app).opts.supports_selection_clipboard
            },
        }
    }

    // pub fn clipboardRequest(self: *Surface, clipboard_type, state) !bool { ... }
    fn clipboard_request(
        &self,
        clipboard_type: Clipboard,
        state: ClipboardRequest,
    ) -> Result<bool, ApprtError> {
        // We need to allocate to get a pointer to store our clipboard request
        // so that it is stable until the read_clipboard callback and call
        // complete_clipboard_request. This sucks but clipboard requests aren't
        // high throughput so it's probably fine.
        // const alloc = self.app.core_app.alloc;
        // const state_ptr = try alloc.create(apprt.ClipboardRequest);
        // errdefer alloc.destroy(state_ptr);
        // state_ptr.* = state;
        let state_ptr: *mut ClipboardRequest = Box::into_raw(Box::new(state));

        // const started = self.app.opts.read_clipboard(
        //     self.userdata,
        //     @intCast(@intFromEnum(clipboard_type)),
        //     state_ptr,
        // );
        let started = unsafe {
            ((*self.app).opts.read_clipboard)(self.userdata, clipboard_type as c_int, state_ptr)
        };
        // if (!started) {
        //     alloc.destroy(state_ptr);
        //     return false;
        // }
        if !started {
            unsafe { drop(Box::from_raw(state_ptr)) };
            return Ok(false);
        }

        // return true;
        // (On success `state_ptr` is intentionally leaked to the host — it owns the
        // request until `complete_clipboard_request` frees it, E9.)
        Ok(true)
    }

    // pub fn setClipboard(self: *const Surface, clipboard_type, contents, confirm) !void { ... }
    fn set_clipboard(
        &self,
        clipboard_type: Clipboard,
        contents: &[rustty_apprt_api::structs::ClipboardContent<'_>],
        confirm: bool,
    ) -> Result<(), ApprtError> {
        // const alloc = self.app.core_app.alloc;
        // const array = try alloc.alloc(CAPI.ClipboardContent, contents.len);
        // defer alloc.free(array);
        // for (contents, 0..) |content, i| {
        //     array[i] = .{ .mime = content.mime, .data = content.data };
        // }
        //
        // PORT NOTE: `apprt.ClipboardContent.mime`/`.data` are `[:0]const u8`
        // (NUL-terminated byte slices → `&[u8]` with a guaranteed trailing NUL); the
        // CAPI `ClipboardContent` wants `[*:0]const u8` → `.as_ptr()`. The heap
        // `alloc.alloc`/`defer free` pair is an owned `Vec` (dropped at scope end).
        let array: Vec<ClipboardContent> = contents
            .iter()
            .map(|content| ClipboardContent {
                mime: content.mime.as_ptr().cast(),
                data: content.data.as_ptr().cast(),
            })
            .collect();

        // self.app.opts.write_clipboard(
        //     self.userdata,
        //     @intCast(@intFromEnum(clipboard_type)),
        //     array.ptr,
        //     array.len,
        //     confirm,
        // );
        unsafe {
            ((*self.app).opts.write_clipboard)(
                self.userdata,
                clipboard_type as c_int,
                array.as_ptr(),
                array.len(),
                confirm,
            );
        }

        Ok(())
    }

    // pub fn getCursorPos(self: *const Surface) !apprt.CursorPos { return self.cursor_pos; }
    fn get_cursor_pos(&self) -> Result<CursorPos, ApprtError> {
        Ok(self.cursor_pos)
    }

    // pub fn defaultTermioEnv(self: *const Surface) !std.process.EnvMap { ... }
    fn default_termio_env(&self) -> Result<HashMap<String, String>, ApprtError> {
        // const alloc = self.app.core_app.alloc;
        // var env = try internal_os.getEnvMap(alloc);
        // errdefer env.deinit();
        //
        // (PORT NOTE: rustty `getEnvMap` is infallible and returns an owned map; on
        // non-darwin `env` is never mutated below — silence `unused_mut` there.)
        #[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(unused_mut))]
        let mut env = rustty_os::env::getEnvMap();

        // if (comptime builtin.target.os.tag.isDarwin()) {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            // if (env.get("__XCODE_BUILT_PRODUCTS_DIR_PATHS") != null) {
            if env.contains_key("__XCODE_BUILT_PRODUCTS_DIR_PATHS") {
                // env.remove("__XCODE_BUILT_PRODUCTS_DIR_PATHS");
                env.remove("__XCODE_BUILT_PRODUCTS_DIR_PATHS");
                env.remove("__XPC_DYLD_LIBRARY_PATH");
                env.remove("DYLD_FRAMEWORK_PATH");
                env.remove("DYLD_INSERT_LIBRARIES");
                env.remove("DYLD_LIBRARY_PATH");
                env.remove("LD_LIBRARY_PATH");
                env.remove("SECURITYSESSIONID");
                env.remove("XPC_SERVICE_NAME");
            }

            // Remove this so that running `rustty` within Rustty works.
            // env.remove("RUSTTY_MAC_LAUNCH_SOURCE");
            //
            // PORT NOTE (R2 identity): the launch-source env var is `RUSTTY_MAC_LAUNCH_SOURCE`
            // (the rustty-os `desktop.rs` `launchedFromDesktop` precedent).
            env.remove("RUSTTY_MAC_LAUNCH_SOURCE");

            // If we were launched from the desktop then we want to
            // remove the LANGUAGE env var so that we don't inherit
            // our translation settings for Rustty. If we aren't from
            // the desktop then we didn't set our LANGUAGE var so we
            // don't need to remove it.
            // if (internal_os.launchedFromDesktop()) env.remove("LANGUAGE");
            if rustty_os::desktop::launchedFromDesktop() {
                env.remove("LANGUAGE");
            }
        }

        // return env;
        Ok(env)
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
