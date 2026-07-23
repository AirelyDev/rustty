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
//! * **Quick terminal / `wlr-layer-shell`.** [`App::supports_quick_terminal`] gates
//!   on the runtime `gtk4-layer-shell` capability and [`App::init_quick_terminal`]
//!   turns the window into a layer surface (`wayland.zig:70-88`); mutter does not
//!   implement `wlr-layer-shell`, so ghostty (and rustty) disables it on GNOME
//!   anyway. The per-sync `syncQuickTerminal` (layer/anchor/margin + the KDE slide)
//!   and `enteredMonitor` sizing (`wayland.zig:310-421`) are the remaining arm; the
//!   [`Window`] keeps the `slide` field + destroy path for the KDE slide.
//! * **Post-init async event delivery — CRANKED via a GSource (RESOLVED
//!   2026-07-19, runtime-verified).** The registry [`Connection`] gets its own
//!   libwayland event queue (`Backend::from_foreign_display` calls
//!   `wl_display_create_queue`), which GDK's main loop does not dispatch. The
//!   initial two-phase roundtrip in [`App::init`] captures all bind-time state
//!   (default decoration mode, blur caps, output order); events that arrive LATER
//!   (notably an `xdg_activation` token `done`, the reply [`Window::set_urgent`]
//!   needs to flash the taskbar) are dispatched only if the queue is cranked.
//!   [`queue_source::WlQueueSource`] is a GLib `GSource` whose prepare/check report
//!   ready exactly when the private queue has pending events (GDK reads the shared
//!   fd and demarshals onto our queue; we never read the socket) and whose dispatch
//!   runs `dispatch_pending` — the wayland-rs analogue of the reference relying on
//!   GDK's default-queue dispatch (`Globals.zig:82`). Verified end-to-end on a live
//!   wlroots compositor (headless sway; xdg_activation, same protocol mutter
//!   implements) by the `wayland_set_urgent_dispatches_activation_token` KAT
//!   (`tests/gtk_wayland_winproto.rs`): set_urgent → real token `done` → `activate`.
//!   Requests we SEND still go out on the shared display fd via
//!   [`Connection::flush`]. (A live compositor decoration-mode change would ALSO
//!   crank now, but [`Window`] still snapshots the decoration mode at init — making
//!   that read live is a separate, tracked deferral.)

mod globals;
mod queue_source;

use std::cell::RefCell;
use std::rc::Rc;

use gdk4::prelude::*;
use gdk4_wayland::prelude::WaylandSurfaceExtManual;
use gtk4::prelude::*;

use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, QueueHandle};
use wayland_backend::client::ObjectId;
use wayland_client::Proxy;

use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use wayland_protocols::xdg::activation::v1::client::xdg_activation_token_v1::XdgActivationTokenV1;
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;
use wayland_protocols_plasma::server_decoration::client::org_kde_kwin_server_decoration::OrgKdeKwinServerDecoration;
use wayland_protocols_plasma::server_decoration::client::org_kde_kwin_server_decoration_manager::Mode;
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide::OrgKdeKwinSlide;
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide::Location as SlideLocation;
use wayland_protocols_plasma::slide::client::org_kde_kwin_slide_manager::OrgKdeKwinSlideManager;
use wayland_client::protocol::wl_compositor::WlCompositor;

use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use rustty_config::value_types::{
    BackgroundBlur, QuickTerminalKeyboardInteractivity, QuickTerminalLayer, QuickTerminalPosition,
    QuickTerminalScreen, QuickTerminalSizeDimensions, WindowDecoration,
};
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

    /// PORTS: `wayland.zig:29` `globals: *Globals` + the private event queue. Held
    /// in an `Rc<RefCell<..>>` shared with [`queue_source::WlQueueSource`]'s callback
    /// so the main-loop `GSource` can crank the queue while [`Window::init`] still
    /// reads the bound globals. `globals` stays boxed inside [`queue_source::Pump`]
    /// for a stable address (`Globals.zig:72-80`).
    pump: Rc<RefCell<queue_source::Pump>>,

    /// The `GSource` that cranks the private queue from the GLib main loop so
    /// post-init events (e.g. the `xdg_activation` token `done` [`Window::set_urgent`]
    /// awaits) are dispatched. Detaches on drop; see the module-level DEFERRAL.
    _source: queue_source::WlQueueSource,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("wayland::App");
        // May be borrowed by the queue GSource callback; avoid a Debug-time panic.
        match self.pump.try_borrow() {
            Ok(p) => s.field("globals", &p.globals),
            Err(_) => s.field("globals", &"<borrowed>"),
        }
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

        // Hand the queue + globals to a shared Pump and attach the main-loop GSource
        // that cranks it, so post-init events (the `xdg_activation` token `done` that
        // `set_urgent` awaits) are dispatched. GDK does not dispatch this queue; the
        // reference gets the same for free from GDK's default-queue dispatch
        // (`Globals.zig:82`).
        let pump = Rc::new(RefCell::new(queue_source::Pump {
            event_queue,
            globals: boxed,
        }));
        let source = queue_source::WlQueueSource::attach(&pump);

        Ok(Some(App {
            _display: display,
            pump,
            _source: source,
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

    /// PORTS: `wayland.zig:70-78` `App.supportsQuickTerminal`. Gated on the runtime
    /// `wlr-layer-shell` capability: `gtk4-layer-shell` reports whether the running
    /// compositor implements the protocol (mutter does not → warn + `false`, a
    /// graceful non-gap; sway/wlroots/KDE → `true`).
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn supports_quick_terminal(&self) -> bool {
        if !gtk4_layer_shell::is_supported() {
            eprintln!(
                "warn(gtk): your compositor does not support the wlr-layer-shell protocol; disabling quick terminal"
            );
            return false;
        }

        true
    }

    /// The connector name of the primary output (`Globals.state.primary_output_name`),
    /// used to resolve the quick-terminal target monitor.
    ///
    /// Divergence: the reference reads `self.globals.state.primary_output_name` live
    /// (`wayland.zig:471`); rustty's winproto [`Window`] deliberately holds no
    /// `globals` pointer (it snapshots only proxy clones + scalars), so both the
    /// [`Window`] method and the `enter_monitor` callback reach the primary output
    /// through this App accessor via `apprt_window.application().winproto()`. Returns
    /// an owned clone (the `globals` live behind the shared `Rc<RefCell<Pump>>`, so a
    /// borrow can't escape); re-read each sync so primary-output/config changes take
    /// effect (matching the reference's live re-resolve).
    #[must_use]
    pub fn primary_output_name(&self) -> Option<String> {
        self.pump.borrow().globals.state.primary_output_name.clone()
    }

    /// OBSERVABILITY (not in the reference): the number of `xdg_activation_token_v1`
    /// `done` replies the [`queue_source::WlQueueSource`] has dispatched so far. Lets
    /// the live-session KAT prove the post-init dispatch seam actually cranks — after
    /// [`Window::set_urgent(true)`](Window::set_urgent) and a few main-loop turns, the
    /// compositor's token `done` must have run [`globals`]' handler, bumping this. It
    /// exists ONLY to give that round-trip an assertion point (the reference relies on
    /// GDK's default-queue dispatch and needs no such proof, `Globals.zig:82`).
    #[must_use]
    pub fn activation_done_count(&self) -> u32 {
        self.pump.borrow().globals.activation_done_count
    }

    /// PORTS: `wayland.zig:80-88` `App.initQuickTerminal`. Turn the quick-terminal
    /// window into a layer surface (`initForWindow`) and pin it to the configured
    /// monitor (`setMonitor`; `None` lets the compositor decide). The layer,
    /// namespace, keyboard mode, anchor, and margins are set later in
    /// `syncQuickTerminal` on every appearance sync.
    pub fn init_quick_terminal(
        &mut self,
        window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<(), WinprotoError> {
        let gtk_window: &gtk4::Window = window.upcast_ref();
        gtk_window.init_layer_shell();

        // Set target monitor based on config (null lets compositor decide).
        let monitor = gtk_window
            .downcast_ref::<ApprtWindow>()
            .and_then(|apprt_window| {
                resolve_quick_terminal_monitor(
                    self.primary_output_name().as_deref(),
                    apprt_window,
                )
            });
        // The reference unrefs the returned monitor here; the `gdk::Monitor` owned by
        // `monitor` is dropped (unref'd) at the end of this scope.
        gtk_window.set_monitor(monitor.as_ref());
        Ok(())
    }
}

/// Resolve the `quick-terminal-screen` config to a specific monitor.
/// Returns `None` to let the compositor decide (used for `.mouse` mode).
///
/// PORTS: `wayland.zig:449-483` `resolveQuickTerminalMonitor`. Divergence: the
/// reference takes `*Globals` and reads `globals.state.primary_output_name`; rustty
/// passes the connector name in (see [`App::primary_output_name`]). The caller owns
/// the returned ref (Rust drops/unrefs it).
fn resolve_quick_terminal_monitor(
    primary_output_name: Option<&str>,
    apprt_window: &ApprtWindow,
) -> Option<gdk4::Monitor> {
    let config = apprt_window.get_config()?;
    let config = config.get();

    match config.quick_terminal_screen {
        QuickTerminalScreen::Mouse => return None,
        QuickTerminalScreen::Main | QuickTerminalScreen::MacosMenuBar => {}
    }

    let display = apprt_window.upcast_ref::<gtk4::Widget>().display();
    let monitors = display.monitors();

    // Try to find the monitor matching the primary output name.
    if let Some(stored_name) = primary_output_name {
        let mut i = 0;
        while let Some(item) = monitors.item(i) {
            i += 1;
            let Some(monitor) = item.downcast_ref::<gdk4::Monitor>() else {
                continue;
            };
            if let Some(connector) = monitor.connector() {
                if connector.as_str() == stored_name {
                    return Some(monitor.clone());
                }
            }
        }
    }

    // Fall back to the first monitor in the list.
    monitors.item(0)?.downcast::<gdk4::Monitor>().ok()
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
    /// The KDE `org_kde_kwin_slide_manager`, used by `sync_quick_terminal` to
    /// create the quick-terminal slide animation (absent on GNOME/sway/wlroots).
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
    /// PORTS: `wayland.zig:108-111` `slide`. Object that controls the
    /// slide-in/slide-out animations of the quick terminal. Always `None` for
    /// windows other than the quick terminal (and when the compositor lacks the
    /// KDE slide manager). Set in `sync_quick_terminal`.
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
        // Read the bound globals through the shared Pump. Every value pulled out is
        // an owned clone / Copy, so the borrow does not escape (and never nests with
        // the queue GSource's `borrow_mut`, which runs only from the main loop).
        let pump = app.pump.borrow();
        let g = &pump.globals;
        let conn = g.conn().clone();
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), ptr.as_ptr().cast()) }
            .map_err(|_| Error::NoWaylandSurface)?;
        let surface = WlSurface::from_id(&conn, id).map_err(|_| Error::NoWaylandSurface)?;

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

        // `wayland.zig:169-177`: for the quick terminal only, connect the GDK
        // surface `enter-monitor` signal so the window is (re)sized to the monitor
        // it lands on. The callback reads the primary output through the App
        // accessor via `apprt_window.application().winproto()`, so we hand it a weak
        // ref to the apprt window (avoiding a surface->closure->window ref cycle).
        if apprt.is_some_and(|w| w.is_quick_terminal()) {
            let apprt_weak = apprt.map(ObjectExt::downgrade).unwrap_or_default();
            gdk_surface.connect_enter_monitor(move |surface, monitor| {
                if let Some(apprt_window) = apprt_weak.upgrade() {
                    entered_monitor(surface, monitor, &apprt_window);
                }
            });
        }

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

        // `if (isQuickTerminal()) syncQuickTerminal()` (`wayland.zig:209-213`).
        if self
            .apprt_window
            .upgrade()
            .is_some_and(|w| w.is_quick_terminal())
        {
            if let Err(err) = self.sync_quick_terminal() {
                eprintln!("warn(gtk): failed to sync quick terminal appearance={err}");
            }
        }

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

    /// PORTS: `wayland.zig:310-389` `Window.syncQuickTerminal`. Push the
    /// quick-terminal's layer, namespace, target monitor, keyboard mode, anchor,
    /// and margins to the `wlr-layer-shell` surface, then (re)create the KDE
    /// `org_kde_kwin_slide` so anchored edges get the compositor slide animation.
    /// Invoked from [`Window::sync_appearance`] on every appearance sync (not once),
    /// so config reloads and primary-output changes take effect.
    fn sync_quick_terminal(&mut self) -> Result<(), WinprotoError> {
        let Some(apprt_window) = self.apprt_window.upgrade() else {
            return Ok(());
        };
        let window: &gtk4::Window = apprt_window.upcast_ref();
        let Some(config) = apprt_window.get_config() else {
            return Ok(());
        };
        let config = config.get();

        window.set_layer(match config.gtk_quick_terminal_layer {
            QuickTerminalLayer::Overlay => Layer::Overlay,
            QuickTerminalLayer::Top => Layer::Top,
            QuickTerminalLayer::Bottom => Layer::Bottom,
            QuickTerminalLayer::Background => Layer::Background,
        });
        window.set_namespace(Some(&String::from_utf8_lossy(
            &config.gtk_quick_terminal_namespace,
        )));

        // Re-resolve the target monitor on every sync so that config reloads
        // and primary-output changes take effect without recreating the window.
        let primary_output_name = quick_terminal_primary_output_name(&apprt_window);
        let target_monitor =
            resolve_quick_terminal_monitor(primary_output_name.as_deref(), &apprt_window);
        // The reference unrefs the returned monitor here; the owned `gdk::Monitor`
        // is dropped (unref'd) at the end of this scope.
        window.set_monitor(target_monitor.as_ref());

        window.set_keyboard_mode(match config.quick_terminal_keyboard_interactivity {
            QuickTerminalKeyboardInteractivity::None => KeyboardMode::None,
            QuickTerminalKeyboardInteractivity::OnDemand => {
                if gtk4_layer_shell::protocol_version() < 4 {
                    eprintln!(
                        "warn(gtk): your compositor does not support on-demand keyboard access; falling back to exclusive access"
                    );
                    KeyboardMode::Exclusive
                } else {
                    KeyboardMode::OnDemand
                }
            }
            QuickTerminalKeyboardInteractivity::Exclusive => KeyboardMode::Exclusive,
        });

        let anchored_edge: Option<Edge> = match config.quick_terminal_position {
            QuickTerminalPosition::Left => Some(Edge::Left),
            QuickTerminalPosition::Right => Some(Edge::Right),
            QuickTerminalPosition::Top => Some(Edge::Top),
            QuickTerminalPosition::Bottom => Some(Edge::Bottom),
            QuickTerminalPosition::Center => None,
        };

        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            if let Some(anchored) = anchored_edge {
                if edge == anchored {
                    window.set_margin(edge, 0);
                    window.set_anchor(edge, true);
                    continue;
                }
            }

            // Arbitrary margin - could be made customizable?
            window.set_margin(edge, 20);
            window.set_anchor(edge, false);
        }

        if let Some(slide) = self.slide.take() {
            slide.release();
        }

        self.slide = if let Some(anchored) = anchored_edge {
            if let Some(mgr) = self.slide_manager.clone() {
                let slide = mgr.create(&self.surface, &self.qh, ());

                let slide_location = match anchored {
                    Edge::Top => SlideLocation::Top,
                    Edge::Bottom => SlideLocation::Bottom,
                    Edge::Left => SlideLocation::Left,
                    Edge::Right => SlideLocation::Right,
                    // `Edge` is `#[non_exhaustive]`; the loop only produces the four
                    // real edges above.
                    _ => SlideLocation::Top,
                };

                slide.set_location(slide_location as u32);
                slide.commit();
                Some(slide)
            } else {
                None
            }
        } else {
            None
        };

        Ok(())
    }
}

/// Update the size of the quick terminal based on monitor dimensions.
///
/// PORTS: `wayland.zig:392-421` `Window.enteredMonitor` (the GDK `enter_monitor`
/// signal callback). Divergence: the reference reaches the globals through
/// `apprt_window.winproto().wayland.globals`; rustty resolves the primary output
/// via [`quick_terminal_primary_output_name`] (`apprt_window.application().winproto()`).
fn entered_monitor(_surface: &gdk4::Surface, monitor: &gdk4::Monitor, apprt_window: &ApprtWindow) {
    let window: &gtk4::Window = apprt_window.upcast_ref();
    let Some(config) = apprt_window.get_config() else {
        return;
    };
    let config = config.get();

    let primary_output_name = quick_terminal_primary_output_name(apprt_window);
    let resolved_monitor =
        resolve_quick_terminal_monitor(primary_output_name.as_deref(), apprt_window);
    // The reference unrefs the resolved monitor at scope end; the owned
    // `gdk::Monitor` is dropped (unref'd) when this function returns.

    // Use the configured monitor for sizing if not in mouse mode.
    let size_monitor = resolved_monitor.as_ref().unwrap_or(monitor);

    let monitor_size = size_monitor.geometry();

    let dims = config.quick_terminal_size.calculate(
        config.quick_terminal_position,
        QuickTerminalSizeDimensions {
            width: monitor_size.width() as u32,
            height: monitor_size.height() as u32,
        },
    );

    window.set_default_size(dims.width as i32, dims.height as i32);
}

/// Resolve the `Globals.state.primary_output_name` for the quick terminal by
/// reaching the wayland winproto [`App`] through the owning [`Application`].
///
/// PORTS: the reference reads `self.globals.state.primary_output_name` directly
/// (the winproto `Window` holds the `*Globals`, `wayland.zig:327,464`). rustty's
/// winproto [`Window`] deliberately holds no `globals` pointer, so both this sync
/// path and the `enter_monitor` callback reach the primary output through
/// [`App::primary_output_name`] via `apprt_window.application().winproto()`.
///
/// [`Application`]: crate::gtk::class::application::Application
fn quick_terminal_primary_output_name(apprt_window: &ApprtWindow) -> Option<String> {
    let app = apprt_window
        .upcast_ref::<gtk4::Window>()
        .application()?
        .downcast::<crate::gtk::class::application::Application>()
        .ok()?;
    let name = match &*app.winproto() {
        super::App::Wayland(wp) => wp.primary_output_name(),
        _ => None,
    };
    name
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
