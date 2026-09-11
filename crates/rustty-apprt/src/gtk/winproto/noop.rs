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

    /// PORTS: `noop.zig:37` `App.initQuickTerminal`. The no-op backend has no
    /// quick-terminal support, so this is a no-op — [`class::window::Window`]'s
    /// `propQuickTerminal` (`window.zig:1204`) calls it on this arm and it does
    /// nothing (the reference's empty body). The `window` param mirrors the
    /// reference's `*ApprtWindow`; it is unused here.
    ///
    /// [`class::window::Window`]: crate::gtk::class::window::Window
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    pub fn init_quick_terminal(
        &mut self,
        _window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<(), WinprotoError> {
        Ok(())
    }

    /// PORTS: `noop.zig:40-46` `App.bindGlobalShortcuts`. The no-op backend has no
    /// windowing protocol at all, so it never binds anything and the caller falls
    /// back to the XDG desktop portal.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn bind_global_shortcuts(
        &mut self,
        _shortcuts: &crate::gtk::class::global_shortcuts::GlobalShortcuts,
        _config: &CoreConfig,
    ) -> bool {
        false
    }

    /// PORTS: `noop.zig:47` `App.clearGlobalShortcuts`. Nothing was bound, so
    /// nothing is cleared (the reference's empty body).
    #[allow(clippy::unused_self)]
    pub fn clear_global_shortcuts(&mut self) {}

    // CORRECTION (task-18, closes S60-F9). The note that stood here said
    // `eventMods` (`noop.zig:26-32`), `initQuickTerminal` (`noop.zig:37`) and the
    // `Window`-parameterized methods were NOT yet ported, pending the key encoder
    // and the `Window` class, and that "this task lands only the surface the
    // winproto union's `init`/`deinit` fall-through and the `Application`
    // teardown require". Every clause of that is now false: `App::event_mods`
    // (`:55`), `App::init_quick_terminal` (`:79`), `App::supports_quick_terminal`,
    // `App::bind_global_shortcuts`/`clear_global_shortcuts` and the whole of
    // `noop::Window` are live above and below, with their real `Mods` /
    // `gdk4::Device` / `gtk4::Window` types.
    //
    // ONE reference decl is genuinely absent, and it is absent on purpose:
    // `Window.updateConfigEvent` (`noop.zig:64-67`) takes
    // `*const ApprtWindow.DerivedConfig`, and `DerivedConfig` DOES NOT EXIST in
    // `class/window.zig` at pin `20abdb50a` — grep the reference tree and the only
    // hit for either `updateConfigEvent` or `ApprtWindow.DerivedConfig` is this one
    // declaration. Zig accepts it because nothing calls it, so its signature is
    // never analysed; the x11 and wayland arms of the same union do not declare it
    // either. There is nothing to port it AGAINST. If upstream revives it (a
    // caller, or a real `DerivedConfig`), port it then — see `NEW-FINDINGS.md` N32.
}

/// PORTS: `noop.zig:40-73` `noop.Window`. Per-window windowing state for the
/// no-op backend — none. Present so the [`super::Window`] union has an
/// always-present `.none` arm.
#[derive(Debug, Default)]
pub struct Window;

impl Window {
    /// PORTS: `noop.zig:41-47` `Window.init`. Always succeeds with the empty
    /// per-window state — the no-op backend needs nothing from the app-level
    /// state or the window. The reference threads `(Allocator, *App, *ApprtWindow)`
    /// for signature parity with the protocol arms; here `app`/`window` are unused
    /// on the noop path (the `Allocator` is dropped tier-wide, PREPORT_GTK §2).
    #[allow(clippy::unnecessary_wraps)]
    pub fn init(
        _app: &App,
        _window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<Window, WinprotoError> {
        Ok(Window)
    }

    /// PORTS: `noop.zig:49-51` `Window.deinit`. Nothing to release.
    #[allow(clippy::unused_self)]
    pub fn deinit(&mut self) {}

    /// PORTS: `noop.zig:56` `Window.resizeEvent`. No-op for the no-op backend.
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    pub fn resize_event(&mut self) -> Result<(), WinprotoError> {
        Ok(())
    }

    /// PORTS: `noop.zig:58` `Window.syncAppearance`. No-op for the no-op backend.
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    pub fn sync_appearance(&mut self) -> Result<(), WinprotoError> {
        Ok(())
    }

    /// PORTS: `noop.zig:72` `Window.setUrgent`. No-op for the no-op backend.
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    pub fn set_urgent(&mut self, _urgent: bool) -> Result<(), WinprotoError> {
        Ok(())
    }

    /// PORTS: `noop.zig:70` `Window.addSubprocessEnv`. The no-op backend adds
    /// nothing to the child environment.
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    pub fn add_subprocess_env(
        &mut self,
        _env: &mut rustty_global::EnvMap,
    ) -> Result<(), WinprotoError> {
        Ok(())
    }

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
