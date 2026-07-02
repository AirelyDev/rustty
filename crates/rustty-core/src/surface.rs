//! Surface — the core surface: a terminal attached to a renderer + IO.
//!
//! PORTS: ghostty src/Surface.zig (+ the `apprt.surface.Mailbox` struct from
//! src/apprt/surface.zig:135, which CORE_BLUEPRINT §1 pins HERE — it names
//! the core `Surface`, so it could not live in rustty-apprt-api; apprt-api
//! holds only the `AppMailbox` trait it implements).
//!
//! PORT STATUS: SKELETON (core chunks C3/C4) — CORE_BLUEPRINT §6 rows C5..C15
//! own this file (the faithful Surface.zig struct + logic, landed under the
//! field-parity audit). Chunk C3 declared ONLY the `Surface<A>` shell and the
//! `SurfaceMailbox<A>` field shape, because `renderer_seam::Options<A>`
//! (renderer/Options.zig:18 `surface_mailbox: apprt.surface.Mailbox`) names
//! it one chunk before C5 lands. Chunk C4 (App.zig) adds the MINIMAL slice
//! App actually touches — the `id`/`rt_surface` fields, the
//! `CoreSurfaceHandle` seam impl, and `unimplemented!()` stubs for the four
//! methods App dispatches into (`close`/`needsConfirmQuit`/`handleMessage`/
//! `performBindingAction`, owned by C6/C7/C14). Same sanctioned split as the
//! C3 `app` module skeleton; C5+ fill IN PLACE — the paths below never
//! change.

// PARITY: the reference's identifiers are camelCase (`needsConfirmQuit`,
// `handleMessage`, `performBindingAction`); preserved verbatim per R1/R2
// (renderer_seam.rs / termio thread.rs precedent).
#![allow(non_snake_case)]

use rustty_apprt_api as apprt;

// PORT NOTE (C4 SKELETON — C5 fills the full field set): the faithful
// Surface.zig struct (CORE_BLUEPRINT §1) lands with chunk C5 under the
// field-parity audit. Until then this is an inhabited shell carrying ONLY the
// fields App.zig reaches ([ADV-FIX 2] doctrine: never an uninhabited stand-in
// for a type other code embeds or points at):
//   - `id` — App.findSurfaceByID (App.zig:511 `if (surface.id == id)`).
//   - `rt_surface` — the `CoreSurfaceHandle` seam impl below (the
//     `Target.cval` `.surface => |v| .{ .surface = v.rt_surface }` edge every
//     `performAction(.{ .surface = ... })` call needs).
pub struct Surface<A: apprt::App + 'static> {
    /// Unique ID used to identify this surface for IPC purposes. It is
    /// exposed to the commands running in surfaces as the environment variable
    /// RUSTTY_SURFACE_ID. It must not be zero as zero is used to incicate a null
    /// value when communicating an ID over DBus as DBus does not allow null/maybe
    /// values.
    //
    // PORT NOTE: Surface.zig:62 `id: u64`.
    pub id: u64,

    /// The apprt surface (Surface.zig:72 `rt_surface: *apprt.runtime.Surface`).
    pub rt_surface: *mut A::Surface,
}

/// PORT NOTE (GROWTH POINT, Rust-only glue): the Surface methods App reaches
/// today (`handleMessage`/`performBindingAction`) return `!void`/`!bool` —
/// error unions whose error SETS are *inferred* from the (C6/C7/C14-owned)
/// bodies. Until those chunks land, this opaque marker types the seam
/// (precedent: `rustty_termio::Error`, `rustty_apprt_api::ApprtError`); it
/// gains the real error sets at the growth point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error;

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "core surface operation failed")
    }
}

impl std::error::Error for Error {}

impl<A: apprt::App + 'static> Surface<A> {
    /// Close this surface. This will trigger the runtime to start the
    /// close process, which should ultimately deinitialize this surface.
    //
    // PORT NOTE (C4 STUB — chunk C6 fills): `pub fn close(self: *Surface) void`
    // — Surface.zig:842 (`self.rt_surface.close(self.needsConfirmQuit());`).
    pub fn close(&mut self) {
        unimplemented!("Surface.close lands with core chunk C6 (Surface.zig:842)")
    }

    /// True if the surface requires confirmation to quit. This should be called
    /// by apprt to determine if the surface should confirm before quitting.
    //
    // PORT NOTE (C4 STUB — chunk C6 fills): `pub fn needsConfirmQuit(self:
    // *Surface) bool` — Surface.zig:938. `&self`: the reference receiver is
    // non-const but the body only queries state ([ADV-FIX 2.4] discipline).
    pub fn needsConfirmQuit(&self) -> bool {
        unimplemented!("Surface.needsConfirmQuit lands with core chunk C6 (Surface.zig:938)")
    }

    /// Called from the app thread to handle mailbox messages to our specific
    /// surface.
    //
    // PORT NOTE (C4 STUB — chunk C7 fills): `pub fn handleMessage(self:
    // *Surface, msg: Message) !void` — Surface.zig:960. `Message<'_>` (not
    // `'static`): the one borrowed variant, `ChangeConfig(&Config)`, is
    // DIRECT-dispatched by `App.updateConfig`, never queued (apprt-api
    // `AppMailbox` note).
    pub fn handleMessage(&mut self, msg: apprt::surface::Message<'_>) -> Result<(), Error> {
        let _ = msg;
        unimplemented!("Surface.handleMessage lands with core chunk C7 (Surface.zig:960)")
    }

    /// Perform a binding action. A binding is a keybinding. This function
    /// must be called from the GUI thread.
    ///
    /// This function returns true if the binding action was performed. This
    /// may return false if the binding action is not supported or if the
    /// binding action would do nothing (i.e. previous tab with no tabs).
    ///
    /// NOTE: At the time of writing this comment, only previous/next tab
    /// will ever return false. We can expand this in the future if it becomes
    /// useful. We did previous/next tab so we could implement #498.
    //
    // PORT NOTE (C4 STUB — chunk C14 fills): `pub fn performBindingAction(self:
    // *Surface, action: input.Binding.Action) !bool` — Surface.zig:4775.
    pub fn performBindingAction(
        &mut self,
        action: rustty_input::binding::Action,
    ) -> Result<bool, Error> {
        let _ = action;
        unimplemented!("Surface.performBindingAction lands with core chunk C14 (Surface.zig:4775)")
    }
}

/// The already-landed A5 seam (CORE_BLUEPRINT §1): `Target.cval` reads
/// `v.rt_surface` off the core `*CoreSurface` — rustty-core's `Surface`
/// hands the opaque pointer out through the interface trait.
impl<A: apprt::App + 'static> apprt::CoreSurfaceHandle for Surface<A> {
    fn rt_surface_ptr(&self) -> *mut core::ffi::c_void {
        self.rt_surface.cast()
    }
}

// PORT NOTE: `apprt.surface.Mailbox` (src/apprt/surface.zig:135) — relocated
// HERE per CORE_BLUEPRINT §1. Field shape: `surface: *Surface` -> the core
// surface pointer; `app: App.Mailbox` -> `crate::app::Mailbox<A>`. The `push`
// method ("Surface message sending is actually implemented on the app
// thread...") + the `rustty_apprt_api::surface::AppMailbox` impl land with
// C5, as does the Surface.zig:15 `pub const Mailbox = apprt.surface.Mailbox`
// re-export.
pub struct SurfaceMailbox<A: apprt::App + 'static> {
    pub surface: *mut Surface<A>,
    pub app: crate::app::Mailbox<A>,
}

// PORT NOTE ([ADV-FIX 6a]): like `app::Mailbox<A>`, the reference copies
// `apprt.surface.Mailbox` BY VALUE (`.surface_mailbox = .{ .surface = self,
// .app = app_mailbox }`, Surface.zig:555; by-value `self: Mailbox` receiver
// in the reference `push`). MANUAL `Clone`/`Copy` with no bound on `A` (the
// fields are a raw pointer + a `Copy` mailbox handle).
impl<A: apprt::App + 'static> Clone for SurfaceMailbox<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: apprt::App + 'static> Copy for SurfaceMailbox<A> {}
