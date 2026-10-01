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
//! inspector). The struct + `Backend` enum are inhabited with real fields;
//! `init`/`deinit`/`initMetal`/`renderMetal`/`newFrame`/`Backend::deinit`/
//! `updateContentScale`/`updateSize`/`queueRender` are all LIVE — the Metal
//! render loop (`renderMetal`'s double-frame loop +
//! `rustty_inspector::Inspector::render` + the `MTLRenderCommandEncoder` objc2
//! `msg_send!` construction) is the apprt-tier finale of the inspector port
//! (see `renderMetal`'s own doc for the R4 acyclic `SurfaceData`/`wakeup` seam).
//! The mouse/scroll/cursor/focus/text/key callbacks (`mouseButtonCallback`/
//! `scrollCallback`/`cursorPosCallback`/`focusCallback`/`textCallback`/
//! `keyCallback`), plus `updateSize`'s `DisplaySize` write, are now LIVE too
//! (Task 6/9 un-defer — this closes the crash-on-open bug where
//! `becomeFirstResponder` -> `setFocus` -> `focusCallback` hit an
//! `unimplemented!()` panic sink): each drives the matching dcimgui
//! `ImGuiIO_Add*Event` behind `ImGui_SetCurrentContext(self.ig_ctx)` (INV-11).
//! `keyCallback`'s `Key -> ImGuiKey` mapping lands as the free fn
//! `key_to_imgui_key` right after this `impl Inspector` block (see its own doc
//! for why the mapping is NOT filled into rustty-vt's `Key::imgui_key`).
//! `queueRender` forwards LIVE to
//! the Surface's `queueInspectorRender` (itself a Phase-3 leaf). E4 also lands
//! `Surface::initInspector` (deferred from E3's range because it constructs the real
//! `Inspector`) and replaces E2's `Inspector` E2->E4 sequencing stub.
//!
//! [ADV-FIX 8] PINNING INVARIANT (correctness-critical, for E2): `embedded::Surface`
//! holds `core_surface: rustty_core::Surface<App>` BY VALUE, and core
//! `Surface::init` writes `core_surface.rt_surface = &embedded_surface` (a pointer
//! back to the container). The embedded `Surface` MUST therefore be heap-allocated
//! (Box) and never moved after init — `rustty_surface_new` returns a `*mut Surface`
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
// P3-W7 T2/T4: `put`/`getv`/`removev`/`hasv` on the byte-typed `EnvMap`. Only
// `default_termio_env`'s Darwin arm calls them, so the import carries the same cfg.
#[cfg(any(target_os = "macos", target_os = "ios"))]
use rustty_global::EnvMapExt as _;
use std::cell::UnsafeCell;
use std::ffi::{CStr, CString};
use std::mem::{ManuallyDrop, MaybeUninit};

use allocator_api2::alloc::{Allocator, Global};

// The Dear ImGui context handle type (`*cimgui.c.ImGuiContext`) backing the
// inspector's per-Surface ImGui context (INV-11). dcimgui-sys is the vendored
// Dear ImGui + Dear Bindings C API leaf.
use dcimgui_sys::ImGuiContext;

use rustty_apprt_api::action::{Action, ActionC, Target, TargetC};
use rustty_apprt_api::ipc;
use rustty_apprt_api::structs::{
    Clipboard, ClipboardReadResult, ClipboardRequest, ClipboardRequestType, ColorScheme,
    ContentScale, CursorPos, SurfaceSize,
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

// const log = std.log.scoped(.embedded_window);
//
/// The `@tagName` of the reference's `std.log.scoped(.embedded_window)`
/// (`embedded.zig:26`), carried as `log::` facade `target:` per
/// `DECISION-logging.md` §6r4. No brand token, so nothing to rebrand here.
const LOG_SCOPE: &str = "embedded_window";

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
//
// PORT NOTE: off Darwin the reference's `Keymap` is `KeymapNoop`, whose
// `init`/`reload` error sets carry no tags; the port spells that empty set as
// the unit error, the same choice `keymap_noop.rs` records — hence the
// `clippy::result_unit_err` allows on the two signatures below, matching that
// module's per-occurrence allow.
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
    /// The keyboard layout keymap. This is lazily initialized on first
    /// use because creating it requires talking to the text input
    /// system (TIS on macOS), and the first such call in a process is
    /// slow (multiple milliseconds). It is only needed once keyboard
    /// events start flowing, at which point the system is warm.
    //
    // keymap: ?input.Keymap,
    //
    // PORT NOTE (S46-F1): this was an eagerly-initialized `keymap: Keymap`,
    // which cost every embedding host that TIS call at `App::init` whether or
    // not a key was ever pressed, and made `App::init` fallible where upstream's
    // is not. `UnsafeCell` (rather than a bare `Option`) because the lazy
    // initialization happens inside `keyboardLayout`, which the apprt-api `App`
    // seam declares as `&self` while the reference's takes a mutable `*App` —
    // exactly the reconciliation `config` below already makes, and sound for the
    // same reason (heap-owned, driven single-threaded through the C-ABI).
    pub keymap: UnsafeCell<Option<Keymap>>,

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
/// C type: rustty_runtime_config_s -> rustty_runtime_config_s (R2)
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

    /// Read the clipboard value. The result only reports facts about
    /// the clipboard: whether the request was started (in which case
    /// complete_clipboard_request or deny_clipboard_request must
    /// eventually be called with the given state pointer), whether the
    /// clipboard has no servable contents, or whether the clipboard
    /// can't be read at all. How each non-started state is answered is
    /// up to the core.
    ///
    /// The MIME types are exactly the representations the caller wants
    /// served; the embedder should read only those. Text-like types
    /// are always requested as the canonical "text/plain". The final
    /// bool asks for the listing of all MIME types available on the
    /// clipboard to be delivered with the completion.
    // read_clipboard: *const fn (
    //     SurfaceUD,
    //     c_int,
    //     *apprt.ClipboardRequest,
    //     [*]const [*:0]const u8,
    //     usize,
    //     bool,
    // ) callconv(.c) apprt.ClipboardReadResult,
    //
    // PORT NOTE (D11, closed-enum inbound): the reference returns the CLOSED
    // `apprt.ClipboardReadResult` enum(c_int) straight across the C ABI. A
    // `#[repr(C)]` Rust return would materialize an out-of-range host value
    // as UB before any check, so the callback returns the raw `c_int` and
    // `clipboard_request` is the narrowing point.
    pub read_clipboard: extern "C" fn(
        *mut c_void,
        c_int,
        *mut ClipboardRequest,
        *const *const c_char,
        usize,
        bool,
    ) -> c_int,

    /// This may be called after a read clipboard call to request
    /// confirmation that the clipboard value is safe to read. The
    /// embedder must call complete_clipboard_request (usually with
    /// the confirmation's contents, which are only borrowed for
    /// this call) or deny_clipboard_request with the given request.
    // confirm_read_clipboard: *const fn (
    //     SurfaceUD,
    //     *const CAPI.ClipboardConfirm,
    //     *apprt.ClipboardRequest,
    //     apprt.ClipboardRequestType,
    // ) callconv(.c) void,
    pub confirm_read_clipboard: extern "C" fn(
        *mut c_void,
        *const ClipboardConfirm,
        *mut ClipboardRequest,
        ClipboardRequestType,
    ),

    /// Write the clipboard value.
    // write_clipboard: *const fn (SurfaceUD, c_int, [*]const CAPI.ClipboardContent, usize, bool) callconv(.c) void,
    pub write_clipboard: extern "C" fn(*mut c_void, c_int, *const ClipboardContent, usize, bool),

    /// Close the current surface given by this function.
    // close_surface: ?*const fn (SurfaceUD, bool) callconv(.c) void = null,
    pub close_surface: Option<extern "C" fn(*mut c_void, bool)>,
}

/// This is the key event sent for rustty_surface_key and
/// rustty_app_key.
//
// PORT NOTE: the reference nests this as `App.KeyEvent`; a top-level struct here.
// The `?[:0]const u8` text is a borrowed, NUL-terminated C string (built by the
// CAPI from `rustty_input_key_s`); modeled as a borrowed byte slice
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
    /// Convert a librustty key event into a core key event.
    //
    // PORT NOTE (ADV-FIX 9): the reference `core` is file-private; the apprt-c
    // C-ABI wrapper `rustty_config_key_is_binding` reaches it
    // (`event.keyEvent().core()`, embedded.zig:1474), so it is `pub` here.
    pub fn core(&self) -> Option<CoreKeyEvent> {
        // const text: []const u8 = if (self.text) |v| v else "";
        let text: &[u8] = self.text.unwrap_or(b"");
        // const unshifted_codepoint: u21 = std.math.cast(u21, self.unshifted_codepoint) orelse 0;
        //
        // CORRECTION (task 20, S46-F2): this was assigned straight through under a
        // note that said "rustty models `u21` as `u32` ... so the
        // `std.math.cast(u21, u32)` is the identity and the `orelse 0` fallback is
        // unreachable". The ALIAS is `u32`; the cast is not the identity. A host
        // value above `maxInt(u21)` (0x1F_FFFF) becomes 0 in the reference, which
        // then matches keybinds by physical key instead of by an impossible
        // codepoint.
        let unshifted_codepoint: key::u21 = if self.unshifted_codepoint <= 0x1F_FFFF {
            self.unshifted_codepoint
        } else {
            0
        };

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
    // PORT NOTE (S46-F1): infallible. The reference `init` is `!void` only
    // because of `config.clone`; rustty's `clone_config` is infallible, and the
    // keymap is no longer built here.
    pub fn init(core_app: *mut CoreApp, config: &Config, opts: Options) -> App {
        // We have to clone the config.
        // const alloc = core_app.alloc;
        // var config_clone = try config.clone(alloc);
        // errdefer config_clone.deinit();
        //
        // PORT NOTE: rustty `clone_config` is infallible (no allocator param); the
        // reference placement-init `init(self: *App, …)` returns the value here
        // (Rust has no placement-init — cf. core `App::init`; the CAPI
        // `rustty_app_new` boxes it).
        let config_clone = config.clone_config();

        // self.* = .{ .core_app = core_app, .config = config_clone, .opts = opts, .keymap = null };
        App {
            core_app,
            config: UnsafeCell::new(config_clone),
            opts,
            keymap: UnsafeCell::new(None),
        }
    }

    pub fn terminate(&mut self) {
        // if (self.keymap) |*v| v.deinit();
        if let Some(v) = self.keymap.get_mut() {
            v.deinit();
        }
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
    //
    // PORT NOTE (N443, P7-W7 T5b fix round 1): raw-self. A key event can run a
    // binding whose embedder action callback re-enters this app (the macOS
    // host's `new_*` handlers call `rustty_surface_new` -> `newSurface`), so no
    // protected reference to the app is held here; the core app is read out by
    // place and entered raw-self too.
    ///
    /// # Safety
    /// `self_` is the live embedded app; main thread.
    pub unsafe fn keyEvent(
        self_: *const App,
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
                if unsafe { CoreApp::keyEvent((*self_).core_app, &*self_, input_event) } {
                    InputEffect::Consumed
                } else {
                    InputEffect::Ignored
                }
            }

            // .surface => |surface| try surface.core_surface.keyCallback(input_event),
            //
            // (N442: raw-self through the embedded surface's raw root, so no
            // protected `&mut` spans a keybind's re-entrant host callback.)
            KeyTarget::Surface(surface) => unsafe {
                CoreSurface::keyCallback(&raw mut (*surface).core_surface, input_event)?
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
    // See the `KeymapError` note: off Darwin this is the noop keymap's empty
    // error set, spelled as the unit error like `keymap_noop.rs`.
    #[allow(clippy::result_unit_err)]
    pub fn reloadKeymap(&mut self) -> Result<(), KeymapError> {
        // Reload the keymap. If it was never initialized we don't need
        // to do anything since lazy initialization will pick up the
        // current layout.
        // if (self.keymap) |*v| try v.reload();
        match self.keymap.get_mut() {
            Some(v) => v.reload(),
            None => Ok(()),
        }
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
            // Lazily initialize the keymap.
            // const keymap: *input.Keymap = keymap: {
            //     if (self.keymap == null) {
            //         self.keymap = input.Keymap.init() catch |err| {
            //             log.warn("error initializing keymap err={}", .{err});
            //             return .unknown;
            //         };
            //     }
            //     break :keymap &self.keymap.?;
            // };
            //
            // SAFETY: see the `keymap` field's PORT NOTE — the app is heap-owned
            // and driven single-threaded through the C-ABI, and no other borrow of
            // the cell is live across this block.
            let slot = unsafe { &mut *self.keymap.get() };
            if slot.is_none() {
                match Keymap::init() {
                    Ok(v) => *slot = Some(v),
                    Err(err) => {
                        // log.warn("error initializing keymap err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error initializing keymap err={:?}", err);
                        return Layout::Unknown;
                    }
                }
            }
            let keymap = slot.as_ref().unwrap();

            // Any layout larger than this is not something we can handle.
            // var buf: [256]u8 = undefined;
            let mut buf: [u8; 256] = [0; 256];
            // const id = keymap.sourceId(&buf) catch |err| {
            //     comptime assert(@TypeOf(err) == error{OutOfMemory});
            //     return .unknown;
            // };
            let id = match keymap.source_id(&mut buf) {
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
    //
    // PORT NOTE (N443, P7-W7 T5b fix round 1): raw-self. The macOS host calls
    // `rustty_surface_new` from inside action callbacks (`new_split`,
    // `new_tab`, `new_window`), while the app's `perform_action(&self)` holds a
    // shared protector on this app; a `&mut self` retag here would be a write
    // under it (Stacked Borrows). The surface's `app` back-pointer is `self_`.
    ///
    /// # Safety
    /// `self_` is the live embedded app; main thread.
    pub unsafe fn newSurface(
        self_: *mut App,
        opts: SurfaceOptions,
    ) -> Result<*mut Surface, ApprtError> {
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
            if let Err(err) = Surface::init(surface, self_, opts) {
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
                    // SAFETY: `rt` is the embedded surface's own back-pointer
                    // (`core_surface.rt_surface`, set at `Surface::init` from the
                    // surface's stable allocation), live while its core surface
                    // is the action target. `title` is a sibling field of
                    // `core_surface`, outside the core-surface bytes the target
                    // points at; the write is a single place write on the main
                    // thread, and no reference into `title` is live across it.
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
            // .new_tab => return false,
            ipc::Action::NewTab(_) => Ok(false),
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
        //
        // PORT NOTE: `{t}` is Zig's TAG-NAME format, so `target={t}` is
        // `Target::key()` (`TargetKey` derives `Debug`); the reference's `action`
        // parameter is likewise the `apprt.Action.Key` ENUM, so `action={}` is
        // `Action::key()`. The reference's third field, `value={any}`, has NO
        // counterpart here: rustty folds the reference's separate `value` payload
        // INTO `Action`, and `Action<'a>` does not implement `Debug` (deriving it
        // would cascade the derive across every payload type in this crate and
        // several below it — a public-surface change far wider than one debug
        // line). Filed as a new finding rather than papered over; the two fields
        // the reference's own tag names carry are live.
        log::debug!(
            target: LOG_SCOPE,
            "dispatching action target={:?} action={:?}",
            target.key(),
            action.key(),
        );

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

// The rest of the backend lives in topic files, split along embedded.zig's own
// sections (P7-W13 K). Every public item is re-exported here, so
// `rustty_apprt::embedded::*` paths are unchanged.
mod inspector;
mod platform;
mod surface;
#[cfg(test)]
mod tests;
mod working_dir;

pub use inspector::{Backend, Inspector};
pub(crate) use platform::platform_host_view;
pub use platform::{
    EnvVar, MacOS, Platform, PlatformC, PlatformCIOS, PlatformCMacOS, PlatformError, PlatformTag,
    IOS,
};
pub use surface::{Surface, SurfaceOptions};
use working_dir::{
    inherited_working_directory_c, requested_working_directory_is_usable, InheritedWorkingDirectory,
};

/// [E1 -> E2 SEQUENCING STUB] reference `CAPI.ClipboardContent` — the
/// `rustty_clipboard_content_s` extern struct (embedded.zig). The reference
/// nests it in the `CAPI` namespace (which the blueprint maps to the separate
/// `rustty-apprt-c` crate, E5), but [`Options::write_clipboard`] — a BACKEND field
/// — references it, so it must live in this crate (`rustty-apprt-c` deps
/// `rustty-apprt`, not the reverse). E5/apprt-c hand-pins / re-exports this exact
/// extern struct for the generated header (ADV-FIX 9 pub-widening).
///
/// One representation of clipboard contents. The data is binary-safe
/// and its length is explicit; it is not sentinel-terminated.
//
// (`8c7a34d4c`: LAYOUT CHANGE — `data` became a length-carrying binary-safe
// pointer; must byte-match rustty-embed.h `rustty_clipboard_content_s` — the
// header, the @embed-abi KAT, and this definition move together.)
#[repr(C)]
pub struct ClipboardContent {
    // mime: [*:0]const u8,
    pub mime: *const c_char,
    // data: [*]const u8,
    pub data: *const u8,
    // len: usize,
    pub len: usize,
}

/// Reference `CAPI.ClipboardComplete` — the `rustty_clipboard_complete_s`
/// extern struct.
///
/// The payload for completing a clipboard read request. See
/// Surface.CompleteClipboard for the field documentation.
#[repr(C)]
pub struct ClipboardComplete {
    // contents: ?[*]const ClipboardContent,
    pub contents: *const ClipboardContent,
    pub contents_len: usize,
    // available: ?[*]const [*:0]const u8,
    pub available: *const *const c_char,
    pub available_len: usize,
    pub confirmed: bool,
    pub remember: bool,
}

/// Reference `CAPI.ClipboardConfirm` — the `rustty_clipboard_confirm_s`
/// extern struct.
///
/// The payload of a clipboard read confirmation request: the
/// would-be completion contents plus the information shown in the
/// permission prompt. All memory is borrowed for the duration of
/// the confirm_read_clipboard callback.
#[repr(C)]
pub struct ClipboardConfirm {
    // contents: ?[*]const ClipboardContent,
    pub contents: *const ClipboardContent,
    pub contents_len: usize,
    // available: ?[*]const [*:0]const u8,
    pub available: *const *const c_char,
    pub available_len: usize,

    /// The human friendly name of the requesting program for the
    /// prompt, null when the protocol doesn't carry one.
    // name: ?[*:0]const u8,
    pub name: *const c_char,

    /// True when the user's decision may be remembered as a
    /// session grant, reported back through the completion's
    /// remember field.
    pub can_remember: bool,
}
