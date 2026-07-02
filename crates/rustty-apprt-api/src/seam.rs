//! The A5 trait seam — the crux that breaks the apprt<->core cycle.
//!
//! In ghostty, `src/apprt/action.zig` and `src/Surface.zig`/`src/App.zig` form a
//! cycle: `action.zig` names `*CoreSurface` (from `src/Surface.zig`), and the
//! core `Surface`/`App` call `apprt.performAction(...)`. Zig tolerates this
//! because it compiles the whole program as one unit and picks a SINGLE apprt
//! backend at comptime. Rust crates must be acyclic, so we invert the edge with
//! traits: `rustty-core` (unported) will `impl` these traits, and this crate
//! (the interface) never names core.
//!
//! GENERIC, NOT `dyn` (deliberate): [`App::perform_action`] is generic over the
//! surface-handle type `C`. This mirrors Zig's comptime single-backend selection
//! — there is never dynamic dispatch across apprt backends. Consequently [`App`]
//! is intentionally NOT `dyn`-compatible (object-safe). Do NOT "fix" it into a
//! `dyn App`: monomorphization is the faithful model, and a trait object would
//! both misrepresent the reference and forbid the generic `perform_action`.

use crate::action::{Action, Target};
use crate::ipc;
use crate::structs::{
    Clipboard, ClipboardContent, ClipboardRequest, ContentScale, CursorPos, SurfaceSize,
};

/// The error returned by [`App::perform_action`].
///
/// PORT NOTE (A5 glue): the reference `performAction` returns `!bool` — an error
/// union whose error SET is *inferred* from the concrete apprt backend
/// (`src/App.zig`/`src/Surface.zig`, not yet ported). apprt-api is the interface
/// crate and must not depend on core, so the concrete error set is a GROWTH
/// POINT to be filled when core/apprt land. Until then it is an opaque marker
/// error, which is enough to type the seam and break the cycle.
// NOTE: intentionally constructible (a plain unit struct, not `#[non_exhaustive]`)
// so rustty-core's `App` impl can actually return it. It gains real variants at
// the GROWTH POINT when the concrete backend error set lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprtError;

impl core::fmt::Display for ApprtError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "apprt action failed")
    }
}

impl std::error::Error for ApprtError {}

/// The bridge that reaches the opaque runtime-surface handle WITHOUT apprt-api
/// depending on core — THIS is what breaks the cycle.
///
/// PORT NOTE: the reference `Target.cval` does `.surface => |v| .{ .surface =
/// v.rt_surface }`, reading the `rt_surface` field of the core `*CoreSurface`.
/// `rt_surface` is `*apprt.Surface` — the apprt-owned opaque handle. rustty-core's
/// `Surface` implements this trait so [`Target::cval`](crate::action::Target::cval)
/// can obtain that pointer through the interface, never naming core here.
pub trait CoreSurfaceHandle {
    /// Return the opaque `rt_surface` pointer (`*apprt.Surface`) for this core
    /// surface. Opaque to apprt-api; only the concrete apprt backend interprets
    /// it. Matches the reference `CoreSurface.rt_surface`.
    fn rt_surface_ptr(&self) -> *mut core::ffi::c_void;
}

/// The application runtime backend seam.
///
/// PORT NOTE: this stands in for the parts of `src/App.zig` / the apprt runtime
/// that the ported `action.zig`/`ipc.zig` types force to exist NOW: the
/// `performAction`/`performIpc`/`wakeup` entry points that core calls on the
/// runtime, plus (core C0, CORE_BLUEPRINT §2) the `config`/`keyboardLayout`
/// surface core's `App.zig`/`Surface.zig` reach through `rt_app`. See
/// PORT_PLAN break #8.
pub trait App: Sized {
    /// The runtime's surface type. Bound to `Surface<App = Self>` so the two
    /// halves of the seam stay in lock-step (mirrors `apprt.runtime.App`/
    /// `apprt.runtime.Surface` referring to each other).
    type Surface: Surface<App = Self>;

    /// Perform an [`Action`] on a [`Target`]. Generic over the core
    /// surface-handle `C` (comptime single-backend monomorphization, per the
    /// module doc). Returns whether the action was handled (reference `!bool`).
    fn perform_action<C: CoreSurfaceHandle>(
        &self,
        target: Target<'_, C>,
        action: Action<'_>,
    ) -> Result<bool, ApprtError>;

    /// Perform an IPC [`ipc::Action`] against an [`ipc::Target`]. Returns whether
    /// it was handled (reference `!bool`, error set `ipc.Errors`).
    fn perform_ipc(
        &self,
        target: ipc::Target<'_>,
        action: ipc::Action<'_>,
    ) -> Result<bool, ipc::Errors>;

    /// Wake up the runtime event loop. Mirrors the runtime `wakeup` entry point
    /// core uses to nudge the app thread.
    fn wakeup(&self);

    /// The runtime's configuration. The backend owns its `Config`
    /// (embedded.zig:125 `config: Config`); core reads it as a field —
    /// `rt_app.config.keybind.set.getEvent(event)` — App.zig:340 (keyEvent).
    fn config(&self) -> &rustty_config::Config;

    /// Loads the keyboard layout.
    ///
    /// Kind of expensive so this should be avoided if possible. When I say
    /// "kind of expensive" I mean that its not something you probably want
    /// to run on every keypress.
    //
    // PORT NOTE: `rt_app.keyboardLayout()` — Surface.zig:3271 (mouseModsWithCapture
    // option-as-alt detection); embedded.zig:219 (macOS-only detection, returns
    // `input.KeyboardLayout` = `rustty_input::keyboard::Layout`).
    fn keyboard_layout(&self) -> rustty_input::keyboard::Layout;
}

/// The surface seam.
///
/// PORT NOTE: the counterpart to [`App`] for a single surface. The associated
/// `App` link keeps [`App::Surface`] and this trait consistent; the concrete
/// surface methods below are the exact set core's `Surface.zig`/`App.zig`
/// call on `rt_surface` (CORE_BLUEPRINT §2 — grep-complete: getCursorPos ×12,
/// setClipboard ×5, getContentScale ×5, supportsClipboard ×2, getTitle ×1,
/// getSize ×1, defaultTermioEnv ×1, close ×1, clipboardRequest ×1, core ×1,
/// rtApp ×2). Signatures are taken from apprt/embedded.zig, the first concrete
/// backend; errors use the opaque [`ApprtError`] growth-point marker (same
/// rationale as [`App::perform_action`]).
pub trait Surface: Sized {
    /// The owning application runtime. Bound to `App<Surface = Self>` so the
    /// seam's two halves refer to each other, exactly as the reference runtime
    /// types do.
    type App: App<Surface = Self>;

    /// The CORE surface type this runtime surface embeds/points at
    /// (rustty-core's `Surface<Self::App>`).
    //
    // PORT NOTE (C0 DECISION, CORE_BLUEPRINT §1): the reference embedded.zig
    // Surface holds `core_surface: CoreSurface` and hands out `*CoreSurface`
    // via `core()`. Zig names core directly; apprt-api must not (break #8), so
    // the pointee is an associated type that core constrains at its impl
    // blocks: `A::Surface: Surface<App = A, Core = core::Surface<A>>`.
    type Core;

    /// Return the core surface for this runtime surface.
    //
    // PORT NOTE: `pub fn core(self: *Surface) *CoreSurface` — embedded.zig:633
    // (`return &self.core_surface;`). The returned pointer is as stable as the
    // backend's surface allocation (embedded heap-allocates its Surface).
    fn core(&self) -> *mut Self::Core;

    /// Return the runtime app that owns this surface.
    //
    // PORT NOTE: `pub fn rtApp(self: *const Surface) *App` — embedded.zig:637
    // (`return self.app;`).
    fn rt_app(&self) -> *mut Self::App;

    /// Close the surface. `process_alive` is the core `needsConfirmQuit`
    /// result (whether the surface should confirm because a process is alive).
    //
    // PORT NOTE: `pub fn close(self: *const Surface, process_alive: bool) void`
    // — embedded.zig:641.
    fn close(&self, process_alive: bool);

    /// The content scale (DPI ratio) of the surface.
    //
    // PORT NOTE: `pub fn getContentScale(self: *const Surface) !apprt.ContentScale`
    // — embedded.zig:650.
    fn get_content_scale(&self) -> Result<ContentScale, ApprtError>;

    /// The size of the surface in pixels.
    //
    // PORT NOTE: `pub fn getSize(self: *const Surface) !apprt.SurfaceSize`
    // — embedded.zig:654.
    fn get_size(&self) -> Result<SurfaceSize, ApprtError>;

    /// The current title of the surface, if any.
    //
    // PORT NOTE: `pub fn getTitle(self: *Surface) ?[:0]const u8` —
    // embedded.zig:658. The NUL-terminated slice maps to `&CStr`; the borrow is
    // tied to `&self`, which is what the backend actually guarantees (a pointer
    // into its stored title). [ADV-FIX 2.4] the reference takes `*Surface`
    // (non-const) but the body is a field read; `&self` is faithful.
    fn get_title(&self) -> Option<&std::ffi::CStr>;

    /// Whether the surface supports the given clipboard type.
    //
    // PORT NOTE: `pub fn supportsClipboard(self: *const Surface, clipboard_type:
    // apprt.Clipboard) bool` — embedded.zig:662.
    fn supports_clipboard(&self, clipboard_type: Clipboard) -> bool;

    /// Start a clipboard read request. Returns whether the request STARTED
    /// (the backend may decline, e.g. no read-clipboard callback).
    //
    // PORT NOTE: `pub fn clipboardRequest(self: *Surface, clipboard_type:
    // apprt.Clipboard, state: apprt.ClipboardRequest) !bool` — embedded.zig:672.
    // [ADV-FIX 1] the `bool` is load-bearing at Surface.zig:5822
    // `startClipboardRequest` (the performable-keybind pass-through) — do NOT
    // collapse it into the error.
    fn clipboard_request(
        &self,
        clipboard_type: Clipboard,
        state: ClipboardRequest,
    ) -> Result<bool, ApprtError>;

    /// Write the given contents to the clipboard, optionally confirming with
    /// the user first.
    //
    // PORT NOTE: `pub fn setClipboard(self: *const Surface, clipboard_type:
    // apprt.Clipboard, contents: []const apprt.ClipboardContent, confirm: bool)
    // !void` — embedded.zig:735 (parameter order per the core callsites,
    // Surface.zig:2201/2317).
    fn set_clipboard(
        &self,
        clipboard_type: Clipboard,
        contents: &[ClipboardContent<'_>],
        confirm: bool,
    ) -> Result<(), ApprtError>;

    /// The current cursor position on the surface.
    //
    // PORT NOTE: `pub fn getCursorPos(self: *const Surface) !apprt.CursorPos`
    // — embedded.zig:760.
    fn get_cursor_pos(&self) -> Result<CursorPos, ApprtError>;

    /// The default environment for the termio subprocess of this surface.
    //
    // PORT NOTE: `pub fn defaultTermioEnv(self: *const Surface)
    // !std.process.EnvMap` — embedded.zig:953. `std.process.EnvMap` maps to
    // `HashMap<String, String>` (== `rustty_termio::command::EnvMap` == what
    // rustty-os `getEnvMap` returns — the established EnvMap shape).
    fn default_termio_env(&self) -> Result<std::collections::HashMap<String, String>, ApprtError>;
}
