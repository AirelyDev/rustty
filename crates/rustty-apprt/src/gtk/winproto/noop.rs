//! rustty-apprt::gtk::winproto::noop
//!
//! PORTS: ghostty `src/apprt/gtk/winproto/noop.zig` (73 loc) — the no-op
//! windowing-protocol backend. This is the ALWAYS-PRESENT fallback arm of the
//! [`winproto`](super) tagged union (`winproto.zig:25` `none: noop.App`): its
//! [`App::init`] never succeeds ([`noop.zig:13-20`] returns `null`), so the
//! ordered-init chain in [`super::App::init`] always falls through to it, and a
//! [`Window`] built on it reports client-side decorations enabled
//! (`noop.zig:65-68`).
//!
//! # Divergence from the reference signatures
//!
//! The reference's `noop.App.init` takes `(_: Allocator, _: *gdk.Display, _:
//! [:0]const u8, _: *const Config)` and ignores every argument — the uniform
//! signature exists only so Zig's `inline for` over the union fields
//! (`winproto.zig:35`) can call each arm identically. rustty's [`super::App::init`]
//! dispatches with an **explicit ordered chain** (Rust has no field reflection),
//! so each arm may take only the arguments it needs: `noop` needs none but its
//! `app_id`/`config` are kept for signature parity with the protocol arms, and —
//! because `noop` genuinely needs no display — the `*gdk.Display` is dropped here
//! so [`App::init`] is callable in a headless (display-free) test. The `alloc`
//! parameter is dropped tier-wide (the rustty allocator seam is the global
//! allocator; PREPORT_GTK §2, invariant 2).

use rustty_config::Config as CoreConfig;

use super::WinprotoError;

/// PORTS: `noop.zig:12-38` `noop.App`. App-level windowing state for the no-op
/// backend — there is none, so this is a zero-sized unit. It exists so the
/// [`super::App`] union has an always-present `.none` arm.
#[derive(Debug, Default)]
pub struct App;

impl App {
    /// PORTS: `noop.zig:13-20` `App.init`. Always returns `Ok(None)`: the no-op
    /// backend never "detects" itself as the active protocol, so the union's
    /// ordered-init chain ([`super::App::init`]) falls through past it to the
    /// unconditional `.none` default. See the module doc for why the reference's
    /// `*gdk.Display`/`Allocator` params are absent here.
    #[allow(clippy::unnecessary_wraps)] // signature parity with the fallible protocol arms
    pub fn init(_app_id: &str, _config: &CoreConfig) -> Result<Option<App>, WinprotoError> {
        Ok(None)
    }

    /// PORTS: `noop.zig:22-24` `App.deinit`. Nothing to release.
    #[allow(clippy::unused_self)]
    pub fn deinit(&mut self) {}

    /// PORTS: `noop.zig:26-32` `App.eventMods`. The no-op backend has no
    /// protocol-specific modifier read, so it always returns `None` — the
    /// [`super::App::event_mods`] fallback then uses [`crate::gtk::key::translate_mods`].
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn event_mods(
        &self,
        _device: Option<&gdk4::Device>,
        _gtk_mods: gdk4::ModifierType,
    ) -> Option<rustty_vt::input::key_mods::Mods> {
        None
    }

    /// PORTS: `noop.zig:34-36` `App.supportsQuickTerminal`. The no-op backend
    /// never supports the quick terminal.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn supports_quick_terminal(&self) -> bool {
        false
    }

    // PORT NOTE: the reference's `eventMods` (`noop.zig:26-32`),
    // `initQuickTerminal` (`noop.zig:37`), and the `Window`-parameterized methods
    // reference `input.Mods`, `*gdk.Device`, and `class/window.zig` (`ApprtWindow`)
    // — types whose ports land with the key encoder and the `Window` class (later
    // L5/L6 tasks). They are added to `noop::App`/`noop::Window` when those
    // consumers exist; this task lands only the surface the winproto union's
    // `init`/`deinit` fall-through and the `Application` teardown require.
}

/// PORTS: `noop.zig:40-73` `noop.Window`. Per-window windowing state for the
/// no-op backend — none. Present so the [`super::Window`] union has an
/// always-present `.none` arm.
#[derive(Debug, Default)]
pub struct Window;

impl Window {
    /// PORTS: `noop.zig:49-51` `Window.deinit`. Nothing to release.
    #[allow(clippy::unused_self)]
    pub fn deinit(&mut self) {}

    /// PORTS: `noop.zig:62-68` `Window.clientSideDecorationEnabled`. Returns
    /// `true` — the no-op backend always reports client-side decorations enabled.
    /// This should be the ACTUAL present state of the window, not the desired
    /// state (`noop.zig:63-64`).
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn client_side_decoration_enabled(&self) -> bool {
        true
    }
}
