//! rustty-apprt::gtk::winproto
//!
//! PORTS: ghostty `src/apprt/gtk/winproto.zig` (156 loc) — the windowing-protocol
//! backend seam. The reference is two `union(Protocol)` tagged unions — [`App`]
//! (one per application) and [`Window`] (one per window) — each with a `none`,
//! `wayland`, and `x11` arm selected at comptime by `build_options.wayland` /
//! `build_options.x11` (`winproto.zig:24-27,89-92`). rustty models them as Rust
//! `enum`s whose `wayland`/`x11` arms are `#[cfg(feature = …)]`-gated: this task
//! (L5.1 T8) builds with NEITHER feature, so only the always-present `.none`
//! ([`noop`]) arm compiles and zero protocol crates are pulled (PREPORT_GTK §5.4,
//! the noop-first sub-milestone). The `wayland` arm lands in L5.4, `x11` in L6.
//!
//! # The `inline for` -> explicit-chain divergence (`winproto.zig:35-47`)
//!
//! The reference's `App.init` walks the union fields with a comptime `inline for`
//! and returns the first arm whose `init` yields non-null. Rust has no field
//! reflection, so [`App::init`] is an **explicit ordered chain** in declaration
//! order (`none`, then the cfg-gated `wayland`, then `x11`) — behavior-identical:
//! the first arm to return `Some` wins, else the unconditional `.none` default
//! (`winproto.zig:46`). Because `noop::App::init` always returns `Ok(None)`, a
//! noop-only build always lands on `.none`.

pub mod noop;

/// The blur-region helper ([`BlurRegion`](blur_region::BlurRegion)) is consumed
/// by the Wayland AND X11 arms, so it is gated to either.
#[cfg(any(feature = "wayland", feature = "x11"))]
mod blur_region;
#[cfg(feature = "wayland")]
pub mod wayland;
#[cfg(feature = "x11")]
pub mod x11;

use rustty_config::Config as CoreConfig;

/// The error returned by the winproto backends' `init`/lifecycle methods.
///
/// PORTS: the inferred error set of the reference's `!App` / `!Window` returns
/// (`winproto.zig:29,94`). The only backends that can fail are `wayland`/`x11`
/// (display/protocol probing), and both are `#[cfg(feature = …)]`-gated OFF in
/// this task — so this build has NO error source: [`noop`] is infallible. The
/// type is therefore an empty (uninhabited) enum in a noop-only build, and grows
/// the `Wayland` variant when the `wayland` feature is on (L5.4). `App::init`'s
/// caller ([`crate::gtk::class::application::Application::new`]) still ports the
/// reference's fallback-to-`.none`-on-error path (`application.zig:345-356`).
#[derive(Debug)]
pub enum WinprotoError {
    /// A Wayland-backend failure (display probe / registry roundtrip /
    /// unsupported quick terminal). See [`wayland::Error`].
    #[cfg(feature = "wayland")]
    Wayland(wayland::Error),
    /// An X11-backend failure (surface probe / libX11 load / property request).
    /// See [`x11::Error`].
    #[cfg(feature = "x11")]
    X11(x11::Error),
}

impl core::fmt::Display for WinprotoError {
    #[allow(unused_variables)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Each cfg'd arm appears only when its feature is on. With NEITHER
        // protocol feature the enum is uninhabited: the `_` arm (present only then)
        // discharges the match via the uninhabited `match *self {}`.
        match self {
            #[cfg(feature = "wayland")]
            WinprotoError::Wayland(e) => write!(f, "wayland winproto error: {e}"),
            #[cfg(feature = "x11")]
            WinprotoError::X11(e) => write!(f, "x11 winproto error: {e}"),
            #[cfg(not(any(feature = "wayland", feature = "x11")))]
            _ => match *self {},
        }
    }
}

impl std::error::Error for WinprotoError {}

/// PORTS: `winproto.zig:16-20` `Protocol`. The discriminant of the [`App`] /
/// [`Window`] tagged unions — which windowing protocol is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// `.none` (`winproto.zig:17`) — the no-op backend ([`noop`]).
    None,
    /// `.wayland` (`winproto.zig:18`) — the Wayland backend (L5.4).
    #[cfg(feature = "wayland")]
    Wayland,
    /// `.x11` (`winproto.zig:19`) — the X11 backend (L6).
    #[cfg(feature = "x11")]
    X11,
}

/// PORTS: `winproto.zig:22-81` `App` — the per-application windowing state. One
/// instance per [`Application`](crate::gtk::class::application::Application),
/// stored in its private state (`application.zig:176`).
#[derive(Debug)]
pub enum App {
    /// `none: noop.App` (`winproto.zig:25`).
    None(noop::App),
    /// `wayland: wayland.App` (`winproto.zig:26`).
    #[cfg(feature = "wayland")]
    Wayland(wayland::App),
    /// `x11: x11.App` (`winproto.zig:27`).
    #[cfg(feature = "x11")]
    X11(x11::App),
}

impl Default for App {
    /// The unconditional fallback arm, `.none` (`winproto.zig:46`).
    fn default() -> Self {
        App::None(noop::App)
    }
}

impl App {
    /// PORTS: `winproto.zig:29-47` `App.init`. Probe each backend in declaration
    /// order and return the first that "detects" itself as active; if none does
    /// (always the case in a noop-only build, since [`noop::App::init`] returns
    /// `Ok(None)`), fall back to the unconditional `.none` (`winproto.zig:46`).
    /// See the module doc for the `inline for` -> explicit-chain divergence.
    ///
    /// `gdk_display` is threaded to the protocol arms (`wayland`/`x11`), which
    /// probe the live display; the always-present `noop` arm needs none.
    pub fn init(
        gdk_display: &gdk4::Display,
        app_id: &str,
        config: &CoreConfig,
    ) -> Result<App, WinprotoError> {
        // noop is first in declaration order (`winproto.zig:25`). It never
        // detects itself, so this never returns here in a noop-only build.
        if let Some(v) = noop::App::init(app_id, config)? {
            return Ok(App::None(v));
        }

        #[cfg(feature = "wayland")]
        if let Some(v) = wayland::App::init(gdk_display, app_id, config)? {
            return Ok(App::Wayland(v));
        }
        #[cfg(feature = "x11")]
        if let Some(v) = x11::App::init(gdk_display, app_id, config)? {
            return Ok(App::X11(v));
        }

        // `gdk_display` is consumed only by the cfg-gated protocol arms above; on
        // a noop-only build it is otherwise unused.
        let _ = gdk_display;
        Ok(App::default())
    }

    /// PORTS: `winproto.zig:49-53` `App.deinit`. Dispatch to the active arm's
    /// `deinit`.
    pub fn deinit(&mut self) {
        match self {
            App::None(v) => v.deinit(),
            #[cfg(feature = "wayland")]
            App::Wayland(v) => v.deinit(),
            #[cfg(feature = "x11")]
            App::X11(v) => v.deinit(),
        }
    }

    /// PORTS: the union discriminant read the reference performs with
    /// `@tagName(wp)` when it logs the selected protocol (`application.zig:358`).
    /// Returns which backend is active.
    #[must_use]
    pub fn protocol(&self) -> Protocol {
        match self {
            App::None(_) => Protocol::None,
            #[cfg(feature = "wayland")]
            App::Wayland(_) => Protocol::Wayland,
            #[cfg(feature = "x11")]
            App::X11(_) => Protocol::X11,
        }
    }

    /// Test-observability seam (rustty-specific): the X11 arm's XKB base event
    /// code, or `None` on any other arm. Lets the live X11 KAT prove
    /// `XkbQueryExtension` ran without exposing the arm's internals.
    #[cfg(feature = "x11")]
    #[doc(hidden)]
    #[must_use]
    pub fn x11_xkb_base_event_code(&self) -> Option<std::os::raw::c_int> {
        match self {
            App::X11(v) => Some(v.xkb_base_event_code()),
            _ => None,
        }
    }

    /// PORTS: `winproto.zig:55-63` `App.eventMods`. Dispatch to the active arm's
    /// `eventMods`; when it returns `None` (always, for the noop arm), fall back
    /// to [`crate::gtk::key::translate_mods`] (`winproto.zig:62`).
    #[must_use]
    pub fn event_mods(
        &self,
        device: Option<&gdk4::Device>,
        gtk_mods: gdk4::ModifierType,
    ) -> rustty_vt::input::key_mods::Mods {
        let arm = match self {
            App::None(v) => v.event_mods(device, gtk_mods),
            #[cfg(feature = "wayland")]
            App::Wayland(v) => v.event_mods(device, gtk_mods),
            #[cfg(feature = "x11")]
            App::X11(v) => v.event_mods(device, gtk_mods),
        };
        arm.unwrap_or_else(|| crate::gtk::key::translate_mods(gtk_mods))
    }

    /// PORTS: `winproto.zig:65-69` `App.supportsQuickTerminal`. Dispatch to the
    /// active arm.
    #[must_use]
    pub fn supports_quick_terminal(&self) -> bool {
        match self {
            App::None(v) => v.supports_quick_terminal(),
            #[cfg(feature = "wayland")]
            App::Wayland(v) => v.supports_quick_terminal(),
            #[cfg(feature = "x11")]
            App::X11(v) => v.supports_quick_terminal(),
        }
    }

    /// PORTS: `winproto.zig:71-81` `App.initQuickTerminal`. Dispatch to the active
    /// arm — [`class::window::Window`]'s `propQuickTerminal` (`window.zig:1204`)
    /// initializes the quick terminal at the app layer. On a noop-only build this
    /// is a no-op.
    ///
    /// [`class::window::Window`]: crate::gtk::class::window::Window
    pub fn init_quick_terminal(
        &mut self,
        window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<(), WinprotoError> {
        match self {
            App::None(v) => v.init_quick_terminal(window),
            #[cfg(feature = "wayland")]
            App::Wayland(v) => v.init_quick_terminal(window),
            #[cfg(feature = "x11")]
            App::X11(v) => v.init_quick_terminal(window),
        }
    }
}

/// PORTS: `winproto.zig:83-155` `Window` — the per-window windowing state. In
/// Wayland terms this is really "Surface"-specific state, but ghostty uses
/// "Surface" for something else, so the reference calls it "Window"
/// (`winproto.zig:83-88`). One instance per window (`window.zig:238`).
///
/// PORT STATUS: the `Window` arms carry only the surface this task needs — the
/// tagged union shape, `deinit`, and `clientSideDecorationEnabled`. The reference's
/// `init` (`winproto.zig:94-118`) and the `resizeEvent`/`syncAppearance`/
/// `addSubprocessEnv`/`setUrgent` methods (`winproto.zig:126-154`) take the
/// `class/window.zig` `Window` (`ApprtWindow`) and `std.process.EnvMap`, whose
/// ports land with the GTK `Window` class (later L5/L6 tasks); they are added to
/// this union and to [`noop::Window`] when that consumer exists.
#[derive(Debug)]
pub enum Window {
    /// `none: noop.Window` (`winproto.zig:90`). The default arm — a fresh window
    /// begins on `.none` before it is realized (`window.zig:312` `priv.winproto =
    /// .none`).
    None(noop::Window),
    /// `wayland: wayland.Window` (`winproto.zig:91`).
    #[cfg(feature = "wayland")]
    Wayland(wayland::Window),
    /// `x11: x11.Window` (`winproto.zig:92`).
    #[cfg(feature = "x11")]
    X11(x11::Window),
}

impl Default for Window {
    /// A fresh window begins on the `.none` arm before it is realized
    /// (`window.zig:312` `priv.winproto = .none`). Manual (not derived) because the
    /// `None` arm carries a payload.
    fn default() -> Self {
        Window::None(noop::Window)
    }
}

impl Window {
    /// PORTS: `winproto.zig:95-118` `Window.init`. Select the per-window backend
    /// matching the active app-level [`App`] arm. The reference's `inline for` over
    /// the union fields becomes an explicit match (Rust has no field reflection);
    /// on a noop-only build only the `.none` arm exists, so this always builds a
    /// [`noop::Window`]. `window` is the realizing [`class::window::Window`]
    /// (`window.zig:1301-1314`).
    ///
    /// [`class::window::Window`]: crate::gtk::class::window::Window
    pub fn init(
        app: &App,
        window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<Window, WinprotoError> {
        match app {
            App::None(a) => Ok(Window::None(noop::Window::init(a, window)?)),
            #[cfg(feature = "wayland")]
            App::Wayland(a) => Ok(Window::Wayland(wayland::Window::init(a, window)?)),
            #[cfg(feature = "x11")]
            App::X11(a) => Ok(Window::X11(x11::Window::init(a, window)?)),
        }
    }

    /// PORTS: `winproto.zig:120-124` `Window.deinit`. Dispatch to the active arm.
    pub fn deinit(&mut self) {
        match self {
            Window::None(v) => v.deinit(),
            #[cfg(feature = "wayland")]
            Window::Wayland(v) => v.deinit(),
            #[cfg(feature = "x11")]
            Window::X11(v) => v.deinit(),
        }
    }

    /// PORTS: `winproto.zig:138-142` `Window.clientSideDecorationEnabled`.
    /// Dispatch to the active arm.
    #[must_use]
    pub fn client_side_decoration_enabled(&self) -> bool {
        match self {
            Window::None(v) => v.client_side_decoration_enabled(),
            #[cfg(feature = "wayland")]
            Window::Wayland(v) => v.client_side_decoration_enabled(),
            #[cfg(feature = "x11")]
            Window::X11(v) => v.client_side_decoration_enabled(),
        }
    }

    /// PORTS: `winproto.zig` `Window.resizeEvent`. Dispatch to the active arm
    /// ([`class::window::Window`]'s `propGdkSurfaceDims`, `window.zig:1151`).
    ///
    /// [`class::window::Window`]: crate::gtk::class::window::Window
    pub fn resize_event(&mut self) -> Result<(), WinprotoError> {
        match self {
            Window::None(v) => v.resize_event(),
            #[cfg(feature = "wayland")]
            Window::Wayland(v) => v.resize_event(),
            #[cfg(feature = "x11")]
            Window::X11(v) => v.resize_event(),
        }
    }

    /// PORTS: `winproto.zig` `Window.syncAppearance`. Dispatch to the active arm
    /// (`window.zig:707,1220`).
    pub fn sync_appearance(&mut self) -> Result<(), WinprotoError> {
        match self {
            Window::None(v) => v.sync_appearance(),
            #[cfg(feature = "wayland")]
            Window::Wayland(v) => v.sync_appearance(),
            #[cfg(feature = "x11")]
            Window::X11(v) => v.sync_appearance(),
        }
    }

    /// PORTS: `winproto.zig:144-148` `Window.addSubprocessEnv`. Dispatch to the
    /// active arm; only the X11 arm contributes anything (`WINDOWID`). Called from
    /// `class::Surface::default_termio_env` (`surface.zig:1633`).
    pub fn add_subprocess_env(
        &mut self,
        env: &mut rustty_global::EnvMap,
    ) -> Result<(), WinprotoError> {
        match self {
            Window::None(v) => v.add_subprocess_env(env),
            #[cfg(feature = "wayland")]
            Window::Wayland(v) => v.add_subprocess_env(env),
            #[cfg(feature = "x11")]
            Window::X11(v) => v.add_subprocess_env(env),
        }
    }

    /// PORTS: `winproto.zig` `Window.setUrgent`. Dispatch to the active arm
    /// (`window.zig:884,2000`).
    pub fn set_urgent(&mut self, urgent: bool) -> Result<(), WinprotoError> {
        match self {
            Window::None(v) => v.set_urgent(urgent),
            #[cfg(feature = "wayland")]
            Window::Wayland(v) => v.set_urgent(urgent),
            #[cfg(feature = "x11")]
            Window::X11(v) => v.set_urgent(urgent),
        }
    }
}
