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
/// runtime. Everything else the runtime exposes (getCursorPos, setClipboard,
/// clipboardRequest, defaultTermioEnv, ...) is a GROWTH POINT added as
/// rustty-core's `App.zig`/`Surface.zig` land. See PORT_PLAN break #8.
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
}

/// The surface seam.
///
/// PORT NOTE: the counterpart to [`App`] for a single surface. Only the
/// associated `App` link is pinned now (it is what keeps [`App::Surface`] and
/// this trait consistent). The concrete surface methods (getContentScale,
/// getCursorPos, supportsClipboard, ...) are a GROWTH POINT filled when
/// rustty-core's `Surface.zig` lands.
pub trait Surface: Sized {
    /// The owning application runtime. Bound to `App<Surface = Self>` so the
    /// seam's two halves refer to each other, exactly as the reference runtime
    /// types do.
    type App: App<Surface = Self>;
}
