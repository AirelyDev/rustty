//! rustty-apprt::gtk::winproto::wayland
//!
//! PORTS: ghostty `src/apprt/gtk/winproto/wayland.zig` (487 loc) — the Wayland
//! window-protocol backend. Two types: [`App`] (per-application: the `wl_display`
//! plus the bound [`globals::Globals`]) and [`Window`] (per-surface: the owned
//! protocol objects that drive CSD, background blur, quick-terminal slide, and
//! `xdg_activation` urgency).
//!
//! Gated `#[cfg(feature = "wayland")]` at the [`super`] module; pulls the
//! `gdk4-wayland` / `wayland-client` / `wayland-backend` / `wayland-protocols` /
//! `wayland-protocols-plasma` crates (Cargo `wayland` feature). On GNOME/mutter —
//! the common target — only `wl_compositor` + `xdg_activation_v1` are advertised,
//! so the KDE + `ext_background_effect` arms are graceful no-ops.
//!
//! # The wl_display -> foreign-backend -> registry bridge
//!
//! GDK owns the real `wl_display`. We reach it via GDK's *raw* accessor
//! ([`gdk4_wayland::WaylandDisplay::wl_display_raw`], which needs no `wayland_crate`
//! Cargo feature), wrap it in a wayland-backend *foreign* backend (guest mode: it
//! does NOT own the display, §1.11), build a wayland-client [`Connection`] over
//! that backend, and get a fresh `wl_registry` to enumerate globals — exactly the
//! reference's `getRegistry` + `roundtrip` (`wayland.zig:45-49`, `Globals.zig:82-92`).
//!
//! # DEFERRALS (recorded in `.port/phase-2/DEFERRALS.md`)
//!
//! * **Quick terminal / `wlr-layer-shell`.** [`App::supports_quick_terminal`]
//!   returns `false` and [`App::init_quick_terminal`] returns
//!   [`Error::QuickTerminalUnsupported`]. The reference gates the quick terminal on
//!   `gtk4-layer-shell` (`wayland.zig:70-88,310-389`); mutter does not implement
//!   `wlr-layer-shell`, so ghostty disables it on GNOME anyway. rustty does not
//!   pull `gtk4-layer-shell`; the [`Window`] keeps the `slide` field + destroy path
//!   so the KDE slide arm drops in with the layer-shell work.
//! * **Post-init async event delivery.** The registry [`Connection`] gets its own
//!   libwayland event queue (`Backend::from_foreign_display` calls
//!   `wl_display_create_queue`), which GDK's main loop does not dispatch. The
//!   initial two-phase roundtrip in [`App::init`] captures all bind-time state
//!   (default decoration mode, blur caps, output order); events that arrive LATER
//!   (a compositor decoration-mode change, an `xdg_activation` token `done`) are
//!   dispatched only if the queue is cranked. Integrating the queue fd into the
//!   GLib main loop is the follow-up; requests we SEND (decoration mode, blur
//!   region, activation commit) go out on the shared display fd via
//!   [`Connection::flush`] and are unaffected.

mod globals;

use gdk4::prelude::*;
use gdk4_wayland::prelude::WaylandSurfaceExtManual;
use gtk4::prelude::*;

use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, EventQueue, QueueHandle};
use wayland_backend::client::ObjectId;
use wayland_client::Proxy;

use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::xdg::activation::v1::client::xdg_activation_token_v1::XdgActivationTokenV1;
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;
use wayland_protocols_plasma::server_decoration::client::org_kde_kwin_server_decoration::OrgKdeKwinServerDecoration;
use wayland_protocols_plasma::server_decoration::client::org_kde_kwin_server_decoration_manager::Mode;
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide::OrgKdeKwinSlide;
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide_manager::OrgKdeKwinSlideManager;
use wayland_client::protocol::wl_compositor::WlCompositor;

use rustty_config::value_types::{BackgroundBlur, WindowDecoration};
use rustty_config::Config as CoreConfig;

use self::globals::Globals;
use super::blur_region::BlurRegion;
use super::WinprotoError;

/// The rustty `Window` GObject the winproto `Window::init` is realized against.
/// Downcast from the generic `&impl IsA<gtk4::Window>` so the arm can read the
/// per-window config (`background-blur`, `window-decoration`) the reference reads
/// off its `*ApprtWindow` (`wayland.zig:93,254-258,302-307`).
type ApprtWindow = crate::gtk::class::window::Window;

/// PORTS: the inferred error set of `wayland.zig`'s `!App` / `!Window` /
/// `!initQuickTerminal` returns. Wrapped by [`super::WinprotoError::Wayland`].
#[derive(Debug)]
pub enum Error {
    /// `error.NoWaylandDisplay` (`wayland.zig:46`).
    NoWaylandDisplay,
    /// `error.NotWaylandSurface` / `error.NoWaylandSurface` (`wayland.zig:126,133,137`).
    NoWaylandSurface,
    /// `error.RoundtripFailed` (`Globals.zig:84,90`).
    RoundtripFailed,
    /// The quick terminal needs `wlr-layer-shell`, which is DEFERRED (see module
    /// docs); the reference builds it unconditionally (`wayland.zig:80-88`).
    QuickTerminalUnsupported,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::NoWaylandDisplay => write!(f, "GDK Wayland display exposes no wl_display"),
            Error::NoWaylandSurface => write!(f, "GTK window has no Wayland surface"),
            Error::RoundtripFailed => write!(f, "wl_display roundtrip failed"),
            Error::QuickTerminalUnsupported => {
                write!(f, "quick terminal (wlr-layer-shell) is not supported")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<Error> for WinprotoError {
    fn from(e: Error) -> Self {
        WinprotoError::Wayland(e)
    }
}

/// PORTS: `wayland.zig:27-89` `App`. Application-wide Wayland state.
pub struct App {
    /// PORTS: `wayland.zig:28` `display: *wl.Display`. GDK owns the `wl_display`;
    /// this proxy is a BORROW (§1.11) — never destroyed here. Kept for parity and
    /// so the [`Connection`] backing it (inside `globals`) stays reachable.
    _display: wayland_client::protocol::wl_display::WlDisplay,

    /// PORTS: `wayland.zig:29` `globals: *Globals`. Boxed for a stable address
    /// (the reference heap-allocates for the same reason, `Globals.zig:72-80`).
    globals: Box<Globals>,

    /// The event queue the registry + globals dispatch into. Kept alive so the
    /// [`Connection`]'s libwayland queue is not torn down; see the post-init
    /// DEFERRAL in the module docs.
    _event_queue: EventQueue<Globals>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("wayland::App")
            .field("globals", &self.globals)
            .finish()
    }
}

impl App {
    /// PORTS: `wayland.zig:31-56` `App.init`. Probe the GDK display for Wayland; if
    /// it is not a Wayland display, return `Ok(None)` — THE ORDERED-INIT FALLBACK
    /// HINGE (`winproto.zig:41-44`): under X11 this makes [`super::App::init`] fall
    /// through to the x11/noop arm. If it IS Wayland but exposes no `wl_display`,
    /// or the registry roundtrip fails, return an error.
    pub fn init(
        gdk_display: &gdk4::Display,
        _app_id: &str,
        _config: &CoreConfig,
    ) -> Result<Option<App>, WinprotoError> {
        // `gobject.ext.cast(gdk_wayland.WaylandDisplay, gdk_display) orelse return null`
        // (`wayland.zig:40-43`).
        let Some(wl_display) = gdk_display.downcast_ref::<gdk4_wayland::WaylandDisplay>() else {
            return Ok(None);
        };

        // `gdk_wayland_display.getWlDisplay() orelse return error.NoWaylandDisplay`
        // (`wayland.zig:45-47`). The raw accessor needs no `wayland_crate` feature.
        let ptr = wl_display
            .wl_display_raw()
            .ok_or(Error::NoWaylandDisplay)?;

        // Wrap GDK's foreign display: guest-mode backend (does NOT own the display)
        // -> connection -> fresh registry. `Backend::from_foreign_display` is
        // `unsafe`: the pointer is a live `wl_display` GDK owns for its lifetime,
        // which outlives this App (§1.11).
        let backend = unsafe {
            wayland_backend::sys::client::Backend::from_foreign_display(ptr.as_ptr().cast())
        };
        let conn = Connection::from_backend(backend);
        let mut event_queue = conn.new_event_queue::<Globals>();
        let qh = event_queue.handle();
        let display = conn.display();
        // `display.getRegistry()` (`Globals.zig:82`).
        let registry = display.get_registry(&qh, ());

        let mut boxed = Box::new(Globals::new(conn, qh, registry));

        // First roundtrip: enumerate + bind globals + install listeners
        // (`Globals.zig:84`).
        event_queue
            .roundtrip(&mut boxed)
            .map_err(|_| Error::RoundtripFailed)?;
        // Second roundtrip if a bound global emits initial-state events
        // (`Globals.zig:89-91`).
        if boxed.needs_roundtrip {
            event_queue
                .roundtrip(&mut boxed)
                .map_err(|_| Error::RoundtripFailed)?;
        }

        Ok(Some(App {
            _display: display,
            globals: boxed,
            _event_queue: event_queue,
        }))
    }

    /// PORTS: `wayland.zig:58-60` `App.deinit`. The [`Connection`] (in `globals`)
    /// is dropped here; because the backend is guest-mode it does NOT destroy GDK's
    /// `wl_display` (§1.11).
    #[allow(clippy::unused_self)]
    pub fn deinit(&mut self) {}

    /// PORTS: `wayland.zig:62-68` `App.eventMods`. Returns `None`, so
    /// [`super::App::event_mods`] falls back to `key::translateMods`
    /// (`winproto.zig:62`).
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn event_mods(
        &self,
        _device: Option<&gdk4::Device>,
        _gtk_mods: gdk4::ModifierType,
    ) -> Option<rustty_vt::input::key_mods::Mods> {
        None
    }

    /// PORTS: `wayland.zig:70-78` `App.supportsQuickTerminal`. DEFERRED: returns
    /// `false` (the quick terminal needs `wlr-layer-shell` via `gtk4-layer-shell`,
    /// which is not pulled; mutter does not implement it). See module DEFERRALS.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn supports_quick_terminal(&self) -> bool {
        false
    }

    /// PORTS: `wayland.zig:80-88` `App.initQuickTerminal`. DEFERRED — returns
    /// [`Error::QuickTerminalUnsupported`]. The sole caller
    /// (`window.rs` `on_notify_quick_terminal`) logs and ignores the error, and it
    /// only reaches here if [`Self::supports_quick_terminal`] is `true` (never),
    /// so this is effectively unreachable today.
    #[allow(clippy::unused_self)]
    pub fn init_quick_terminal(
        &mut self,
        _window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<(), WinprotoError> {
        Err(Error::QuickTerminalUnsupported.into())
    }
}

/// PORTS: `wayland.zig:91-444` `Window`. Per-surface Wayland state.
///
/// Divergence: the reference holds `globals: *Globals` and reads it live at sync
/// time (`wayland.zig:99,250,294`). rustty captures cheap clones of the manager
/// proxies + a snapshot of the two scalar states at `init` instead of a raw
/// pointer — safe Rust, no aliasing with the app's `&mut Globals` during dispatch.
/// `default_deco_mode`/`bg_blur_supported` are set during [`App::init`]'s roundtrip
/// (before any window realizes), so the snapshot is the post-roundtrip truth;
/// live compositor changes to them are part of the post-init DEFERRAL.
pub struct Window {
    /// PORTS: `wayland.zig:93` `apprt_window: *ApprtWindow`. A WEAK ref (the window
    /// owns this winproto `Window` via its private `RefCell<winproto::Window>`, so a
    /// strong ref would be a GObject refcount cycle — the reference's raw pointer is
    /// non-owning). Upgraded at sync time to read config.
    apprt_window: glib::WeakRef<ApprtWindow>,

    /// PORTS: `wayland.zig:96` `surface: *wl.Surface`. GDK owns it (§1.11) — BORROW,
    /// never destroyed here.
    surface: WlSurface,

    /// For flushing our sent requests to the shared display fd.
    conn: Connection,
    /// For creating the per-surface child proxies (blur region, activation token).
    qh: QueueHandle<Globals>,

    // Manager-proxy clones captured from `globals` (None when the global was
    // absent — i.e. everything but `compositor`/`activation` on GNOME).
    compositor: Option<WlCompositor>,
    activation: Option<XdgActivationV1>,
    #[allow(dead_code)] // used by the DEFERRED quick-terminal slide arm
    slide_manager: Option<OrgKdeKwinSlideManager>,
    /// Whether the KDE decoration manager exists — drives the CSD/SSD decision
    /// (`wayland.zig:222`).
    kde_deco_present: bool,
    /// Snapshot of `Globals.state.default_deco_mode` (`wayland.zig:303`).
    default_deco_mode: Option<Mode>,
    /// Snapshot of `Globals.state.bg_effect_capabilities.blur` (`wayland.zig:252`).
    bg_blur_supported: bool,

    // The four owned proxies (`wayland.zig:102-114`), destroyed in `deinit`.
    /// PORTS: `wayland.zig:102-103` `bg_effect`.
    bg_effect: Option<ExtBackgroundEffectSurfaceV1>,
    /// PORTS: `wayland.zig:106-107` `decoration`.
    decoration: Option<OrgKdeKwinServerDecoration>,
    /// PORTS: `wayland.zig:110-111` `slide` (always None until the quick-terminal
    /// arm lands — see module DEFERRALS).
    slide: Option<OrgKdeKwinSlide>,
    /// PORTS: `wayland.zig:114` `activation_token`.
    activation_token: Option<XdgActivationTokenV1>,

    /// PORTS: `wayland.zig:116` `blur_region`.
    blur_region: BlurRegion,
}

impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("wayland::Window")
            .field("has_decoration", &self.decoration.is_some())
            .field("has_bg_effect", &self.bg_effect.is_some())
            .field("kde_deco_present", &self.kde_deco_present)
            .field("default_deco_mode", &self.default_deco_mode)
            .finish()
    }
}

impl Window {
    /// PORTS: `wayland.zig:118-186` `Window.init`.
    pub fn init(
        app: &App,
        window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<Window, WinprotoError> {
        let gtk_window: &gtk4::Window = window.upcast_ref();
        // The concrete rustty window carries the config the reference reads off
        // `*ApprtWindow`. Downcast should always succeed (window.rs passes the
        // rustty `Window` here); if not, `apprt_window` upgrades to `None` and the
        // config-driven paths degrade gracefully.
        let apprt = gtk_window.downcast_ref::<ApprtWindow>();

        // `gtk_native.getSurface() orelse return error.NotWaylandSurface`
        // (`wayland.zig:125-126`).
        let Some(gdk_surface) = gtk_window.surface() else {
            return Err(Error::NoWaylandSurface.into());
        };

        // `gobject.ext.cast(gdk_wayland.WaylandSurface, ...)` +
        // `getWlSurface() orelse return error.NoWaylandSurface`
        // (`wayland.zig:130-137`). We use the RAW accessor (no `wayland_crate`
        // feature) and rebuild the `WlSurface` proxy on OUR connection.
        let Some(ptr) = gdk_surface
            .downcast_ref::<gdk4_wayland::WaylandSurface>()
            .and_then(|s| s.wl_surface_raw())
        else {
            return Err(Error::NoWaylandSurface.into());
        };
        let conn = app.globals.conn().clone();
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), ptr.as_ptr().cast()) }
            .map_err(|_| Error::NoWaylandSurface)?;
        let surface = WlSurface::from_id(&conn, id).map_err(|_| Error::NoWaylandSurface)?;

        let g = &app.globals;
        let qh = g.qh().clone();

        // `mgr.create(wl_surface)` — the decoration object (`wayland.zig:141-153`).
        // wayland-client returns an inert proxy (never errors) if creation fails.
        let decoration = g
            .kde_decoration_manager()
            .map(|mgr| mgr.create(&surface, &qh, ()));

        // `mgr.getBackgroundEffect(wl_surface)` (`wayland.zig:155-167`).
        let bg_effect = g
            .ext_background_effect()
            .map(|mgr| mgr.get_background_effect(&surface, &qh, ()));

        // PORT NOTE: `wayland.zig:169-177` connects `enter_monitor` for the quick
        // terminal only; DEFERRED with the quick-terminal arm.

        Ok(Window {
            apprt_window: apprt.map(ObjectExt::downgrade).unwrap_or_default(),
            surface,
            conn,
            qh,
            compositor: g.compositor().cloned(),
            activation: g.xdg_activation().cloned(),
            slide_manager: g.kde_slide_manager().cloned(),
            kde_deco_present: g.kde_decoration_manager().is_some(),
            default_deco_mode: g.default_deco_mode(),
            bg_blur_supported: g.bg_blur_supported(),
            bg_effect,
            decoration,
            slide: None,
            activation_token: None,
            blur_region: BlurRegion::empty(),
        })
    }

    /// PORTS: `wayland.zig:188-193` `Window.deinit`. Destroy the proxies THIS window
    /// owns (`bg_effect`/`decoration`/`slide`/`activation_token`). The `surface` and
    /// the manager clones are borrows of GDK-/App-owned objects — NOT destroyed
    /// (§1.11).
    pub fn deinit(&mut self) {
        self.blur_region.clear();
        if let Some(bg) = self.bg_effect.take() {
            bg.destroy();
        }
        if let Some(deco) = self.decoration.take() {
            deco.release();
        }
        if let Some(slide) = self.slide.take() {
            slide.release();
        }
        // The reference leaves the token to its `done`/next-`setUrgent`; destroy it
        // here too so teardown does not leak it.
        if let Some(token) = self.activation_token.take() {
            token.destroy();
        }
        let _ = self.conn.flush();
    }

    /// PORTS: `wayland.zig:195-199` `Window.resizeEvent`.
    pub fn resize_event(&mut self) -> Result<(), WinprotoError> {
        self.sync_blur();
        let _ = self.conn.flush();
        Ok(())
    }

    /// PORTS: `wayland.zig:201-214` `Window.syncAppearance`.
    pub fn sync_appearance(&mut self) -> Result<(), WinprotoError> {
        self.sync_blur();
        self.sync_decoration();
        // `if (isQuickTerminal()) syncQuickTerminal()` (`wayland.zig:209-213`) —
        // DEFERRED with the quick-terminal arm.
        let _ = self.conn.flush();
        Ok(())
    }

    /// PORTS: `wayland.zig:216-226` `Window.clientSideDecorationEnabled`. Returns
    /// the ACTUAL present decoration state (the noop contract).
    #[must_use]
    pub fn client_side_decoration_enabled(&self) -> bool {
        match self.decoration_mode() {
            Mode::Client => true,
            // If we support SSDs (KDE deco manager present) do NOT enable CSDs when
            // SSD is preferred; if we do NOT (GNOME) enable CSDs even then
            // (`wayland.zig:219-223`).
            Mode::Server => !self.kde_deco_present,
            Mode::None => false,
            // `Mode` is `#[non_exhaustive]`; the reference's `else => unreachable`.
            _ => false,
        }
    }

    /// PORTS: `wayland.zig:233-246` `Window.setUrgent`.
    pub fn set_urgent(&mut self, urgent: bool) -> Result<(), WinprotoError> {
        // `self.globals.get(.xdg_activation) orelse return` (`wayland.zig:234`).
        let Some(activation) = self.activation.clone() else {
            return Ok(());
        };

        // If there already is a token, destroy and unset it (`wayland.zig:237`).
        if let Some(token) = self.activation_token.take() {
            token.destroy();
        }

        self.activation_token = if urgent {
            // `token = activation.getActivationToken()` + set surface + commit
            // (`wayland.zig:239-244`). The surface is the token's UserData so
            // `globals`' token `done` handler can activate it.
            let token = activation.get_activation_token(&self.qh, self.surface.clone());
            token.set_surface(&self.surface);
            token.commit();
            let _ = self.conn.flush();
            Some(token)
        } else {
            None
        };
        Ok(())
    }

    /// PORTS: `wayland.zig:249-291` `Window.syncBlur`.
    ///
    /// PORT NOTE: faithful port, but on GNOME/mutter `bg_effect`/`compositor`'s blur
    /// capability is absent, so this early-returns — background blur is a non-gap on
    /// the common target (the reference disables it there too). Where the compositor
    /// DOES support `ext_background_effect`, this drives it from the window's
    /// `background-blur` config via [`BlurRegion`].
    fn sync_blur(&mut self) {
        let Some(compositor) = self.compositor.clone() else {
            return;
        };
        let Some(bg) = self.bg_effect.clone() else {
            return;
        };
        if !self.bg_blur_supported {
            return;
        }
        let Some(apprt) = self.apprt_window.upgrade() else {
            return;
        };
        let Some(config) = apprt.get_config() else {
            return;
        };
        let blur = config.get().background_blur;

        if !background_blur_enabled(blur) {
            self.blur_region.clear();
            bg.set_blur_region(None);
            return;
        }

        let csd = self.client_side_decoration_enabled();
        let native: &gtk4::Native = apprt.upcast_ref();
        let region = BlurRegion::calc_for_window(native, csd, false);

        // Region didn't change — don't do anything (`wayland.zig:274-278`).
        if region.eql(&self.blur_region) {
            return;
        }

        // `compositor.createRegion()` + add each slice (`wayland.zig:280-287`).
        let wl_region = compositor.create_region(&self.qh, ());
        for s in region.slices() {
            wl_region.add(s.x as i32, s.y as i32, s.width as i32, s.height as i32);
        }
        bg.set_blur_region(Some(&wl_region));
        // `set_blur_region` has copy semantics (the wl_region may be destroyed
        // immediately per the protocol); the reference keeps the region proxy, we
        // free it to avoid a per-sync leak.
        wl_region.destroy();
        self.blur_region = region;
    }

    /// PORTS: `wayland.zig:293-299` `Window.syncDecoration`.
    fn sync_decoration(&self) {
        let Some(deco) = self.decoration.as_ref() else {
            return;
        };
        // The protocol requests a uint, not the enum (`wayland.zig:296-298`).
        deco.request_mode(self.decoration_mode() as u32);
    }

    /// PORTS: `wayland.zig:301-308` `Window.getDecorationMode`.
    fn decoration_mode(&self) -> Mode {
        let decoration = self.apprt_window.upgrade().map(|w| w.get_window_decoration());
        match decoration {
            // `.auto => default_deco_mode orelse .Client` (`wayland.zig:303`).
            Some(WindowDecoration::Auto) | None => self.default_deco_mode.unwrap_or(Mode::Client),
            Some(WindowDecoration::Client) => Mode::Client,
            Some(WindowDecoration::Server) => Mode::Server,
            Some(WindowDecoration::None) => Mode::None,
        }
    }
}

/// PORTS: `value_types.zig` `BackgroundBlur.enabled` (rustty's
/// `value_types::background_blur_enabled`, which is `pub(crate)` to rustty-config).
/// Replicated here since sync_blur needs it. macOS-glass variants imply blur, so
/// they enable the standard Linux blur (`value_types.rs:2356-2370`).
fn background_blur_enabled(v: BackgroundBlur) -> bool {
    match v {
        BackgroundBlur::False => false,
        BackgroundBlur::True | BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => {
            true
        }
        BackgroundBlur::Radius(r) => r > 0,
    }
}
