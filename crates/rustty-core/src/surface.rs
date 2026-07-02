//! Surface — the core surface: a terminal attached to a renderer + IO.
//!
//! PORTS: ghostty src/Surface.zig (+ the `apprt.surface.Mailbox` struct from
//! src/apprt/surface.zig:135, which CORE_BLUEPRINT §1 pins HERE — it names
//! the core `Surface`, so it could not live in rustty-apprt-api; apprt-api
//! holds only the `AppMailbox` trait it implements).
//!
//! PORT STATUS: SKELETON (core chunk C3) — CORE_BLUEPRINT §6 rows C5..C15 own
//! this file (the faithful Surface.zig struct + logic, landed under the
//! field-parity audit). Chunk C3 declares ONLY the `Surface<A>` shell and the
//! `SurfaceMailbox<A>` field shape, because `renderer_seam::Options<A>`
//! (renderer/Options.zig:18 `surface_mailbox: apprt.surface.Mailbox`) names
//! it one chunk before C5 lands. Same sanctioned split as the C3 `app`
//! module skeleton; C5 fills IN PLACE — the paths below never change.

use rustty_apprt_api as apprt;

// PORT NOTE (C3 SKELETON — C5 fills): the faithful Surface.zig field set
// (CORE_BLUEPRINT §1) lands with chunk C5. Until then this is an inhabited
// placeholder shell ([ADV-FIX 2] doctrine: never an uninhabited stand-in for
// a type other code embeds or points at); C3/C4 code only names it behind
// `*mut`.
pub struct Surface<A: apprt::App + 'static> {
    _skeleton: core::marker::PhantomData<A>,
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
