//! PORTS: ghostty src/apprt/gtk/class/application.zig — `Application.Private`
//! (the `imp::Application` instance struct, `application.zig:163-222`), the
//! `ObjectSubclass` registration and its `ObjectImpl`/`ApplicationImpl` vfuncs,
//! and the private `imp::Application` methods (startup, CSS, signals, ...).

use super::*;
// The GTK-tier traits the startup/activate/dispose vfuncs and their helpers
// reach through: `set_default` (gio `ApplicationExt`), the
// `parent_startup`/`parent_activate` chain-up (gio `ApplicationImplExt`), and
// the style-manager surface (`AdwApplicationExt::style_manager`,
// `StyleManagerExt::{set_color_scheme, is_dark, connect_dark_notify}`). `as _`
// so they enable method resolution without competing for a name.
use gio::prelude::ApplicationExt as _;
use gio::subclass::prelude::ApplicationImplExt as _;
use libadwaita::prelude::AdwApplicationExt as _;

/// PORTS: `application.zig:194-201` `quit_timer`. The tagged union that tracks
/// the "quit after the last window closes" timer. `active` carries the glib
/// timeout source id (`c_uint`, OWNED handle — removed via `glib.Source.remove`
/// at the dispose site, PREPORT_GTK §1.1); `off`/`expired` carry nothing.
#[derive(Debug, Default)]
pub enum QuitTimer {
    /// `.off` (`application.zig:195`) — no timer armed.
    #[default]
    Off,
    /// `.active: c_uint` (`application.zig:196`) — a live glib timeout source.
    /// The [`glib::SourceId`] is the OWNED handle; dropping/removing it cancels
    /// the timeout (invariant 11, RAII per PREPORT_GTK §5.2).
    Active(glib::SourceId),
    /// `.expired` (`application.zig:197`) — the timer fired.
    Expired,
}

/// Storage for the type-erased `present-surface` seam
/// ([`Application::present_surface_thunk`]), named so the field type stays
/// under the type-complexity threshold.
type PresentSurfaceThunk = RefCell<Option<Box<dyn Fn(u64)>>>;

/// Storage for the type-erased color-scheme seam
/// ([`Application::color_scheme_thunk`]), named for the same reason as
/// [`PresentSurfaceThunk`].
type ColorSchemeThunk = RefCell<Option<Box<dyn Fn(bool)>>>;

/// Storage for the type-erased `open-config` seam
/// ([`Application::open_config_thunk`]), named for the same reason as
/// [`PresentSurfaceThunk`].
type OpenConfigThunk = RefCell<Option<Box<dyn Fn(rustty_core::app::OpenConfig)>>>;

/// Storage for the type-erased `new-tab` seam
/// ([`Application::new_tab_thunk`]), named for the same reason as
/// [`PresentSurfaceThunk`]. The `Option<u64>` is the reference's
/// `surface_id_` (`None` == the `surface_id == 0` "no target" case).
type NewTabThunk = RefCell<Option<Box<dyn Fn(Option<u64>, super::TabOverrides)>>>;

/// PORTS: `application.zig:163-222` `Application.Private`. Every pointer-typed
/// field maps a PREPORT_GTK §1.1 LIFETIMES row; the transfer/ownership mode is
/// preserved 1:1 (see the field docs). Interior mutability is `RefCell`/`Cell`
/// because the subclass `imp` is shared `&self` (GObject holds it), matching
/// the reference's `self.private()` mutable access.
#[derive(glib::Properties, Default)]
#[properties(wrapper_type = super::Application)]
pub struct Application {
    /// The apprt App. This is annoying that we need this it'd be
    /// nicer to just make THIS the apprt app but the current librustty
    /// API doesn't allow that.
    ///
    /// PORTS: `application.zig:167` `rt_app: *ApprtApp` (BACKREF, never freed
    /// here). The apprt `App` owns the sole created `Application` ref and
    /// backrefs it; this is the reverse edge. Stored as a raw non-owning
    /// pointer. PORT NOTE: typed as `*mut c_void` in the skeleton because the
    /// apprt `App` type (`ApprtApp`) lands in Task 11 (`src/gtk/app.rs`); Task 8
    /// (`new`) fills this field and Task 11 narrows the pointee type. `None`
    /// until `new` sets it.
    pub rt_app: Cell<Option<NonNull<c_void>>>,

    /// The librustty App instance.
    ///
    /// PORTS: `application.zig:170` `core_app: *CoreApp` (BACKREF; libghostty
    /// core owns it, `alloc = core_app.alloc`, `application.zig:247`; not freed
    /// here). Raw non-owning pointer. PORT NOTE: `*mut c_void` in the skeleton —
    /// `CoreApp = rustty_core::App<ApprtApp>` cannot be named until the apprt
    /// `App` type exists (Task 11); Task 8 fills it and Task 11 narrows it.
    pub core_app: Cell<Option<NonNull<c_void>>>,

    /// The configuration for the application.
    ///
    /// PORTS: `application.zig:173` `config: *Config` (SHARED — refcounted
    /// [`Config`] GObject shared across the tier). The `config` property is
    /// backed by this field; the getter refs (`.full`), a Task 8 `setConfig`
    /// unrefs the prior and refs the new. `None` until `new`.
    #[property(get = Self::get_config, name = "config", nullable)]
    pub config: RefCell<Option<Config>>,

    /// The global shortcut logic.
    ///
    /// PORTS: `application.zig:182` `global_shortcuts: *GlobalShortcuts` (OWNED
    /// — created here, sole ref, `unref` at dispose, PREPORT_GTK §1.1). PORT
    /// NOTE: stored as a bare `glib::Object` in the skeleton because the
    /// `GlobalShortcuts` subclass lands in Task 27; `startupGlobalShortcuts`
    /// (Task 9) creates it. Invariant 12: manually held, never parented — the
    /// port holds the strong ref explicitly and clears it to release.
    pub global_shortcuts: RefCell<Option<glib::Object>>,

    /// PORTS: `application.zig:219` `open_uri: OpenURI = undefined` (OWNED —
    /// an inline struct, `.init(rt_app)` in `new` (`:404`) and `deinit()` at
    /// dispose (`:440`)). The port stores it inline as well: [`OpenUri`] is a
    /// cheap `Rc` handle, created eagerly so `openUri()` is always valid, and
    /// `deinit_private` calls its `deinit`.
    pub open_uri: OpenUri,

    /// The audio bell's MediaFile, reused across bells so we don't leak a
    /// GStreamer pipeline (and its GL threads) on every ring. Built lazily
    /// on the first audio bell and rebuilt when `bell-audio-path` changes;
    /// unref'd on dispose. See ringBell and media.zig.
    ///
    /// PORTS: `application.zig:227-232` `bell_media: ?*gtk.MediaFile` (B7,
    /// f0f3f4d8d — moved here from the surface; OWNED, dropped at dispose).
    pub bell_media: RefCell<Option<gtk4::MediaFile>>,

    /// If non-null, we're currently showing a config errors dialog.
    /// This is a WeakRef because the dialog can close on its own
    /// outside of our own lifecycle and that's okay.
    ///
    /// PORTS: `application.zig:203-205` `config_errors_dialog:
    /// WeakRef(ConfigErrorsDialog)` (SHARED / non-owning — the dialog may close
    /// on its own outside our lifecycle, so a weak ref is correct).
    ///
    /// PORT NOTE (WS20): narrowed from the skeleton's `WeakRef<glib::Object>`
    /// to the real `WeakRef<ConfigErrorsDialog>` now that the class exists.
    pub config_errors_dialog: WeakRef<ConfigErrorsDialog>,

    /// CSS Provider for any styles based on Rustty configuration values.
    ///
    /// PORTS: `application.zig:208` `css_provider: *gtk.CssProvider` (OWNED —
    /// `.new()` here, `unref` at dispose). Invariant 12: manually held, never
    /// parented. `None` until `new`.
    pub css_provider: RefCell<Option<gtk4::CssProvider>>,

    /// Providers for loading custom stylesheets defined by user
    ///
    /// PORTS: `application.zig:212` `custom_css_providers:
    /// ArrayListUnmanaged(*gtk.CssProvider)` (OWNED — each element `unref`'d and
    /// the list `deinit`'d at dispose). A `Vec<CssProvider>` whose `Drop`
    /// unrefs each element.
    pub custom_css_providers: RefCell<Vec<gtk4::CssProvider>>,

    /// A copy of the LANG environment variable that was provided to Rustty
    /// by the system. If this is null, the LANG environment variable did
    /// not exist in Rustty's environment variable.
    ///
    /// PORTS: `application.zig:217` `saved_language: ?[:0]const u8` (OWNED — a
    /// heap copy of the launch-time `LANG`, `alloc.free`'d at dispose). A
    /// NUL-terminated [`CString`] (the reference type is a sentinel-terminated
    /// slice). `None` when `LANG` was unset.
    pub saved_language: RefCell<Option<CString>>,

    /// glib source for our signal handler.
    ///
    /// PORTS: `application.zig:206` `signal_source: ?c_uint` (OWNED handle — the
    /// glib source id for the SIGINT/SIGTERM handler; `Source.remove` at
    /// dispose). A [`glib::SourceId`]; dropping/removing it detaches the source
    /// (invariant 11, RAII). `None` when no handler is installed.
    pub signal_source: RefCell<Option<glib::SourceId>>,

    /// The timer used to quit the application after the last window is
    /// closed. Even if there is no quit delay set, this is the state
    /// used to determine to close the app.
    ///
    /// PORTS: `application.zig:194-201` `quit_timer` (OWNED handle — see
    /// [`QuitTimer`]). `.off` until armed.
    pub quit_timer: RefCell<QuitTimer>,

    /// This is set to true so long as we request a window exactly
    /// once. This prevents quitting the app before we've shown one
    /// window.
    ///
    /// PORTS: `application.zig:186` `requested_window: bool` — set true once we
    /// have requested a window exactly once, so we don't quit before showing
    /// one. Control flag; Task 8/Task 10 drive it.
    pub requested_window: Cell<bool>,

    /// This is set to false internally when the event loop
    /// should exit and the application should quit. This must
    /// only be set by the main loop thread.
    ///
    /// PORTS: `application.zig:191` `running: bool` — the hand-rolled run loop's
    /// continue flag; set false to quit. Set only by the main-loop thread
    /// (Task 10). Control flag.
    pub running: Cell<bool>,

    /// True once we've told systemd that startup is complete. Reloads
    /// before then (e.g. syncing the color scheme during startup) must
    /// not notify systemd, or it sees READY=1 before we're ready.
    ///
    /// PORTS: `application.zig:198-201` `systemd_ready: bool = false` (added
    /// `4eb308831`, P7-W8 T3). Reloads before then (e.g. syncing the color
    /// scheme during startup) must not notify systemd, or it sees `READY=1`
    /// before we're ready. Set true once, at the same point the reference
    /// sets it (`:564`, right after the first `systemd.notify.ready()`).
    pub systemd_ready: Cell<bool>,

    /// State and logic for the underlying windowing protocol.
    ///
    /// PORTS: `application.zig:176` `winproto: winprotopkg.App` — the
    /// per-application windowing-protocol state (the [`winproto`] tagged
    /// union). `.none` (the noop backend) until [`super::Application::new`]
    /// runs [`winproto::App::init`]; a noop-only build (L5.1 T8, no
    /// `wayland`/`x11` feature) always lands on `.none` (PREPORT_GTK §1.11 /
    /// §5.4). Freed in `deinit` via [`winproto::App::deinit`]
    /// (`application.zig:439`). `RefCell` for the shared-`&self` `imp` access.
    pub winproto: RefCell<winproto::App>,

    /// rustty-only seam carrying the monomorphized "queue a new window" push
    /// (`priv.core_app.mailbox.push(.{ .new_window = {} }, .forever)`,
    /// `application.zig:1464-1466`). The Zig `activate` vfunc names `*CoreApp`
    /// directly because the whole program is comptime-monomorphized to one
    /// apprt backend; the Rust `Application` GObject is non-generic and its
    /// stored `core_app` is a type-erased `*mut c_void` (Task 8 PORT NOTE),
    /// so the concrete `rustty_core::app::App<A>` it points at cannot be named
    /// from the non-generic vfunc. [`super::Application::bind_core_app`]
    /// installs this thunk once the concrete apprt `App` type `A` is known
    /// (the run loop / `gtk::App` — Tasks 10/11, and the T9 test), and the
    /// `activate` vfunc invokes it. `None` until bound; `activate` is then a
    /// no-op push (faithful to a not-yet-wired core, placeholder-leaf doctrine).
    pub new_window_thunk: RefCell<Option<Box<dyn Fn()>>>,

    /// rustty-only seam carrying the monomorphized "present a surface by ID"
    /// push (`self.core().findSurfaceByID(surface_id)` +
    /// `self.core().mailbox.push(.{ .surface_message = .{ .surface = surface,
    /// .message = .present_surface } }, .forever)`,
    /// `application.zig:1839-1849`, `actionPresentSurface`). Same type-erasure
    /// rationale as [`Self::new_window_thunk`]: the non-generic `Application`
    /// cannot name the concrete `rustty_core::app::App<A>` its type-erased
    /// `core_app` points at, so [`super::Application::bind_core_app`] captures
    /// it here. `None` until bound; the `present-surface` handler is then a
    /// no-op (placeholder-leaf doctrine).
    pub present_surface_thunk: PresentSurfaceThunk,

    /// rustty-only seam carrying the monomorphized `open-config` push
    /// (`self.core().mailbox.push(global.io(), .{ .open_config = target },
    /// .forever)`, `application.zig:2102`, `actionOpenConfig`). Same
    /// type-erasure rationale as [`Self::present_surface_thunk`]. `None` until
    /// [`super::Application::bind_core_app`]; the `open-config` handler is then a
    /// no-op (placeholder-leaf doctrine).
    pub open_config_thunk: OpenConfigThunk,

    /// rustty-only seam carrying the monomorphized `new-tab` surface
    /// resolution + dispatch (`actionNewTab`'s
    /// `core_app.findSurfaceByID(surface_id)` / `core_app.focusedSurface()`
    /// lookup and the `Action.newTab`-or-`Action.newWindow` fan-out,
    /// `application.zig:1858-1907`, B5/034506f14). Same type-erasure
    /// rationale as [`Self::present_surface_thunk`]: the non-generic
    /// `Application` cannot name the concrete `rustty_core::app::App<A>`,
    /// so [`super::Application::bind_core_app`] captures it here. `None`
    /// until bound; the `new-tab` handler is then a no-op
    /// (placeholder-leaf doctrine).
    pub new_tab_thunk: NewTabThunk,

    /// rustty-only seam carrying the monomorphized color-scheme propagation
    /// (`core_app.colorSchemeEvent(self.rt(), scheme)` plus, per surface,
    /// `surface.core().colorSchemeCallback(scheme)`,
    /// `application.zig:1553-1563`, `handleStyleManagerDark`). The `bool` is
    /// `style.is_dark()` (light = `false`, dark = `true`); the thunk maps it to
    /// `apprt::ColorScheme` before reaching the core. Same type-erasure
    /// rationale as [`Self::new_window_thunk`]: the non-generic `Application`
    /// cannot name the concrete `rustty_core::app::App<A>` its type-erased
    /// `core_app`/`rt_app` pointers refer to, nor the concrete surface type, so
    /// [`super::Application::bind_core_app`] captures both backrefs here.
    /// `None` until bound; [`Self::handle_style_manager_dark`] then propagates
    /// nothing (faithful to a not-yet-wired core, placeholder-leaf doctrine).
    pub color_scheme_thunk: ColorSchemeThunk,

    /// rustty-only seam carrying the monomorphized inline core tick
    /// (`try priv.core_app.tick(priv.rt_app)`, `application.zig:552`) — **the
    /// libghostty→GTK bridge's other half**: the run loop breaks its blocking
    /// `MainContext::iteration(true)` when [`super::Application::wakeup`] fires,
    /// and the very next statement is this tick. Like [`Self::new_window_thunk`],
    /// the non-generic `Application` GObject cannot name the concrete
    /// `rustty_core::app::App<A>` its type-erased `core_app`/`rt_app` pointers
    /// refer to, so [`super::Application::bind_core_app`] captures both here.
    /// `None` until bound; the run loop then treats a tick as a no-op `Ok(())`.
    #[allow(clippy::type_complexity)]
    pub tick_thunk: RefCell<Option<Box<dyn Fn() -> Result<(), rustty_core::app::Error>>>>,

    /// rustty-only seam carrying the monomorphized
    /// `priv.core_app.needsConfirmQuit()` probe (`application.zig:604`) that
    /// [`super::Application::quit`] consults. Same type-erasure rationale as
    /// [`Self::tick_thunk`]; installed by [`super::Application::bind_core_app`].
    /// `None` until bound — [`super::Application::quit`] then treats "needs
    /// confirmation" as `false` and quits immediately (the safe default: an
    /// unbound core has no surfaces holding unsaved state).
    #[allow(clippy::type_complexity)]
    pub needs_confirm_quit_thunk: RefCell<Option<Box<dyn Fn() -> bool>>>,

    /// rustty-only seam carrying the monomorphized
    /// `priv.core_app.updateConfig(rt_app, config)` call
    /// (`application.zig:2473-2474`, the `.app` arm of `reloadConfig`). Same
    /// type-erasure rationale as [`Self::tick_thunk`]: the non-generic
    /// `Application` cannot name the concrete `rustty_core::app::App<A>` its
    /// type-erased `core_app`/`rt_app` pointers refer to, so
    /// [`super::Application::bind_core_app`] captures both here. Invoked by
    /// [`super::Application::update_config_app`] (the `.app` reload target).
    /// `None` until bound — the reload is then a no-op (faithful to a
    /// not-yet-wired core, placeholder-leaf doctrine).
    #[allow(clippy::type_complexity)]
    pub update_config_thunk:
        RefCell<Option<Box<dyn Fn(&rustty_config::Config) -> Result<(), rustty_core::app::Error>>>>,

    /// rustty-only seam carrying the monomorphized global-shortcut dispatch
    /// (`self.core().performAllAction(self.rt(), action)`,
    /// `application.zig:1660`, `globalShortcutTrigger`). Same type-erasure
    /// rationale as [`Self::tick_thunk`]: the non-generic `Application` cannot
    /// name the concrete `rustty_core::app::App<A>` its type-erased
    /// `core_app`/`rt_app` pointers refer to, so
    /// [`super::Application::bind_core_app`] captures both here. `None` until
    /// bound — a global shortcut then performs nothing (faithful to a
    /// not-yet-wired core).
    #[allow(clippy::type_complexity)]
    pub perform_all_action_thunk: RefCell<Option<Box<dyn Fn(rustty_input::binding::Action)>>>,

    /// Test-only instrumentation: how many times the hand-rolled run loop has
    /// executed the inline core tick (`application.zig:552`). The
    /// `application_run_loop_ticks_then_quits` KAT asserts this is ≥ 1 to prove
    /// the tick site actually ran (the reference's `try priv.core_app.tick`
    /// leaves no observable counter, so the port adds one). `#[cfg(test)]` so
    /// production carries no extra field.
    #[cfg(test)]
    pub tick_count: Cell<u64>,

    /// Test-only instrumentation: how many times a portal `trigger` reached
    /// the core dispatch (`application.zig:1660` leaves no observable, so the
    /// port adds one — the `tick_count` precedent). `#[cfg(test)]` so
    /// production carries no extra field.
    #[cfg(test)]
    pub global_shortcut_trigger_count: Cell<u64>,

    /// TEST-ONLY counter: how many times `bind-failed` reached
    /// [`Self::global_shortcut_bind_failed`]. Lets the KAT assert the failure
    /// SIGNAL actually fired, rather than that nothing errored.
    #[cfg(test)]
    pub global_shortcut_bind_failed_count: Cell<u64>,
}

#[glib::object_subclass]
impl ObjectSubclass for Application {
    // PORTS: `application.zig:136` `.name = "GhosttyApplication"`. Preserve the
    // GType name verbatim so the registered type is identical to the reference.
    const NAME: &'static str = "GhosttyApplication";
    type Type = super::Application;
    // PORTS: `application.zig:135` `pub const Parent = adw.Application`.
    type ParentType = libadwaita::Application;

    /// PORTS: `application.zig:1862-1893` `Class.init`. gtk4-rs's per-class
    /// `class_init` hook is the faithful analogue of the reference's hand-rolled
    /// `Class.init`. The reference does three things here; in the gtk4-rs idiom
    /// they split as:
    ///   1. register the compiled gresources exactly once
    ///      (`application.zig:1867-1881`) → the [`crate::gtk::register_resources`]
    ///      call below (idempotent `Once`, warn-and-continue on a null blob).
    ///   2. `registerProperties(class, &.{properties.config.impl})`
    ///      (`application.zig:1884-1886`) → handled by `#[glib::derived_properties]`
    ///      (installs the `config` property into `ObjectImpl::properties`).
    ///   3. the four vfunc overrides `activate`/`startup`/`dispose`/`finalize`
    ///      (`application.zig:1889-1892`) → handled by the `*Impl` trait method
    ///      slots below (bodies are Task 9's; the skeleton keeps the defaults).
    fn class_init(_klass: &mut Self::Class) {
        // Register our compiled resources exactly once.
        //
        // PORT NOTE: `register_resources` owns the reference's failure arm
        // (`application.zig:2194-2197`):
        //
        //     // If we fail to load resources then things will
        //     // probably look really bad but it shouldn't stop our
        //     // app from loading.
        crate::gtk::register_resources();
    }
}

#[glib::derived_properties]
impl ObjectImpl for Application {
    /// PORTS: `application.zig:1475-1493` `dispose`. Runs the two teardown
    /// steps the reference does BEFORE chaining the parent dispose; the
    /// gtk4-rs dispose glue chains the parent automatically after this returns
    /// (`glib::subclass::object::dispose` — "Chain up to the parent's
    /// dispose"), so the explicit `gobject.Object.virtual_methods.dispose.call`
    /// (`:1489-1492`) is discharged for us.
    fn dispose(&self) {
        // Close + release the config-errors dialog (invariant 4,
        // PREPORT_GTK §1.1 / `:1477-1480`): `get()` hands back a strong ref,
        // which we would `close()` and then drop (unref).
        if let Some(diag) = self.config_errors_dialog.get() {
            // `diag.close()` (`:1478`). The strong ref from `get()` is
            // unref'd here by Rust's scope-end drop (`:1479`).
            diag.close();
        }
        // `priv.config_errors_dialog.deinit()` (1dbc8ca30: the named
        // clear-before-free; see `weak_ref.rs` `deinit`).
        self.config_errors_dialog.deinit();

        // Remove the SIGUSR2 source (invariant 11, `:1482-1487`). `take()`
        // gives us the OWNED `SourceId`; we remove it via the raw
        // `g_source_remove` so we can warn-and-continue on failure, exactly
        // like the reference (`SourceId::remove` would instead panic).
        if let Some(source) = self.signal_source.take() {
            let removed = unsafe { glib::ffi::g_source_remove(source.as_raw()) };
            if removed == glib::ffi::GFALSE {
                log::warn!(target: LOG_SCOPE, "unable to remove signal source");
            }
        }

        // `if (priv.bell_media) |v| { v.unref(); priv.bell_media = null; }`
        // (`:1570-1573`, B7/f0f3f4d8d) — the scope-end drop is the unref.
        drop(self.bell_media.take());
    }
}

impl Drop for Application {
    /// PORTS: `application.zig:1495-1501` `finalize`. GObject frees in its
    /// finalizer; in gtk4-rs the `imp` struct's `Drop` IS the finalize hook
    /// (the framework runs it then chains the parent finalize), so the
    /// reference's `self.deinit()` + parent-`finalize` chain-up maps here.
    /// [`super::Application::deinit`] is idempotent (every field-clear is a
    /// no-op once cleared), so a prior explicit `deinit` (the run loop's
    /// force-teardown, `application.zig:430-434`) leaves this a clean sweep.
    fn drop(&mut self) {
        deinit_private(self);
    }
}

// PORTS: the parent-chain `*Impl` traits for an `adw.Application` subclass. Each
// is the gtk4-rs slot where the reference's registered vfunc overrides live:
// `activate`/`startup` (`application.zig:1459-1473`/`:1294-1330`, registered
// `:1889-1890`) are `ApplicationImpl` methods filled by Task 9; the skeleton
// keeps the default passthroughs so `GhosttyApplication` registers over the full
// `Object → GApplication → GtkApplication → AdwApplication` chain.
impl gio::subclass::prelude::ApplicationImpl for Application {
    /// PORTS: `application.zig:1294-1330` `startup`. Chains the parent startup,
    /// installs us as the process-global default `GApplication`, then runs the
    /// six startup helpers in the reference's exact order.
    fn startup(&self) {
        log::debug!(target: LOG_SCOPE, "startup");

        // `gio.Application.virtual_methods.startup.call(parent)` (`:1297-1300`).
        self.parent_startup();

        let obj = self.obj();

        // Set ourselves as the default application (`:1303`).
        (*obj).set_default();

        // The D-Bus connection is only valid after GApplication startup.
        //
        // (The portal `OpenURI` object is handed it here, `:1305-1308`.)
        {
            use gio::prelude::ApplicationExt as _;
            let dbus = obj.upcast_ref::<gio::Application>().dbus_connection();
            self.open_uri.set_dbus_connection(dbus.as_ref());
        }

        // Setup our event loop
        //
        // (`:1311`.)
        self.startup_xev();
        // Setup some signal handlers. This must happen before anything
        // that might notify systemd, since with Type=notify-reload
        // systemd refuses to start us if our reload signal has no
        // handler.
        //
        // PORTS: `application.zig:1390` (order swapped with the style
        // manager by `4eb308831`, P7-W8 T3): signals now install BEFORE
        // the style manager, because syncing the color scheme during
        // startup can trigger a config reload, which must not race
        // ahead of the SIGUSR2 handler.
        self.startup_signals();
        // Setup our style manager (light/dark mode)
        //
        // (`:1314` before `4eb308831`; now runs after signals, `:1394`.)
        self.startup_style_manager();
        // Setup our action map (`:1320`).
        self.startup_action_map();
        // Setup our global shortcuts (`:1323`).
        self.startup_global_shortcuts();
        // If we have any config diagnostics from loading, then we
        // show the diagnostics dialog. We show this one as a general
        // modal (not to any specific window) because we don't even
        // know if the window will load.
        //
        // (`:1329`.)
        self.show_config_errors_dialog();
    }

    /// PORTS: `application.zig:1459-1473` `activate`. Queues a new window onto
    /// the core app mailbox (**the app's only window-creation entry** — it goes
    /// through the mailbox, not a direct call), then chains the parent activate.
    fn activate(&self) {
        log::debug!(target: LOG_SCOPE, "activate");

        // `priv.core_app.mailbox.push(.{ .new_window = {} }, .forever)`
        // (`:1463-1466`), monomorphized into [`new_window_thunk`] by
        // [`super::Application::bind_core_app`] (see that field's doc for why
        // the non-generic vfunc cannot name `App<A>` directly). A `None` thunk
        // — the core app is not yet bound — makes this a no-op push, faithful
        // to a runtime with nothing wired behind the mailbox yet.
        if let Some(thunk) = self.new_window_thunk.borrow().as_ref() {
            thunk();
        }

        // Call the parent activate method.
        //
        // (`gio.Application.virtual_methods.activate.call(parent)`, `:1468-1472`.)
        self.parent_activate();
    }
}
impl gtk4::subclass::prelude::GtkApplicationImpl for Application {}
impl libadwaita::subclass::prelude::AdwApplicationImpl for Application {}

impl Application {
    /// Returns the configuration for this application.
    ///
    /// The reference count is increased.
    ///
    /// PORTS: `application.zig:1208-1210` `getConfig` (the `config` property
    /// getter, `getter_transfer = .full`). Hands back a fresh strong ref to the
    /// shared [`Config`] (cloning a gtk4-rs wrapper increments the refcount), so
    /// the caller owns the returned ref — exactly the reference's
    /// `self.private().config.ref()`. `None` before `new` sets the config
    /// (the reference's field is non-null post-construction, but the property
    /// type is `?*Config`, so nullable is faithful).
    fn get_config(&self) -> Option<Config> {
        self.config.borrow().clone()
    }

    /// Set the configuration for this application. The reference count
    /// is increased on the new configuration and the old one is
    /// unreferenced.
    ///
    /// If the config has errors this may show the config errors dialog.
    ///
    /// PORTS: `application.zig:1211-1226` `setConfig`. Sets the configuration for
    /// this application: unref the prior, ref the new, emit `notify::config` (so
    /// [`Self::prop_config`] re-applies CSS / accelerators), then show the config
    /// errors dialog if the new config has diagnostics. The reference's explicit
    /// `priv.config.unref()` + `priv.config = config.ref()` is the `RefCell`
    /// `replace` (dropping the old `Config` GObject wrapper unrefs it; storing the
    /// new one holds its ref); `notifyByPspec(config)` is `notify("config")`.
    ///
    /// Reached from [`super::action::config_change`]'s `.app` arm — the config
    /// property setter path the reference's `configChange` takes
    /// (`application.zig:1953`).
    pub(super) fn set_config(&self, config: Config) {
        // `priv.config.unref(); priv.config = config.ref();` (`:1224-1225`).
        self.config.replace(Some(config));
        // `self.as(gobject.Object).notifyByPspec(...config...)` (`:1226`).
        self.obj().notify("config");
        // Show our errors if we have any
        //
        // (`self.showConfigErrorsDialog()`, `:1229`.)
        self.show_config_errors_dialog();
    }

    /// PORTS: the `.app` arm of `reloadConfig` — `self.core().updateConfig(
    /// self.rt(), config)` (`application.zig:2472-2475`). Drives the freshly
    /// loaded config through the type-erased core app via
    /// [`Self::update_config_thunk`], which iterates every surface's config and
    /// (synchronously) re-dispatches the `.app` `config_change` action back into
    /// [`Self::set_config`]. A `None` thunk (core not yet bound) is a no-op.
    ///
    /// The error is returned, not logged: the reference `try`s the call and
    /// logs once, at `reloadConfig`'s caller (N311).
    pub(super) fn update_config_app(
        &self,
        config: &rustty_config::Config,
    ) -> Result<(), rustty_core::app::Error> {
        // Follow the `new_window_thunk` idiom: borrow-and-call the thunk inline.
        if let Some(thunk) = self.update_config_thunk.borrow().as_ref() {
            thunk(config)?;
        }
        Ok(())
    }

    /// PORTS: `application.zig:530-...` `propConfig` — the `notify::config`
    /// signal handler that (re)applies the configuration to the running
    /// application (CSS reload, action/env sync, config-errors dialog, ...),
    /// connected in `new` (`application.zig:408-414`) and triggered once at
    /// the end of `new` (`application.zig:425`).
    ///
    /// **Task 9 fills this.** Placeholder-leaf (placeholder-leaf doctrine): a
    /// safe no-op now, so the initial `notify("config")` trigger in
    /// [`super::Application::new`] runs cleanly before the config-application
    /// body lands with the startup/activate vfuncs (Task 9).
    pub(super) fn prop_config(&self, _app: &super::Application) {
        // Sync our accelerators for menu items (`:1233`).
        self.sync_action_accelerators();

        // Load our runtime and custom CSS. If this fails then our window is
        // just stuck with the old CSS but we don't want to fail the entire
        // config change operation (`:1300-1314`).
        self.load_runtime_css();
        // self.loadCustomCss() catch |err| {
        //     log.warn("failed to load custom CSS, no custom CSS applied, err={}", .{err});
        // };
        if let Err(err) = self.load_custom_css() {
            log::warn!(target: LOG_SCOPE, "failed to load custom CSS, no custom CSS applied, err={err}");
        }
    }

    /// PORTS: `application.zig:...` `signalCssParsingError` — the
    /// `CssProvider::parsing-error` handler that logs CSS parse errors,
    /// connected in `new` (`application.zig:416-422`).
    ///
    /// The reference's `{s}` of `err.f_message orelse "«unknown»"`
    /// (`application.zig:1272,1279`).
    pub(super) fn signal_css_parsing_error(&self, section: &gtk4::CssSection, error: &glib::Error) {
        // `css_section.toString()` (`:1259`). gtk4-rs's `CssSection` renders
        // its location via `Display`.
        let location = section.to_str();

        // The GTK >= 4.16 byte-dump arm (`:1261-1274`). Ghostty writes it
        // `if (comptime gtk_version.atLeast(4, 16, 0)) bytes: { ... }`, whose
        // leading `comptime` fold collapses the branch away when built
        // against a pre-4.16 header (`gtk_version.zig:58-62`). rustty mirrors
        // that fold FAITHFULLY as the same two-part gate the `:1565`
        // `prefers-color-scheme` push uses: `#[cfg(feature = "gtk-v4_16")]`
        // is the comptime analog (`CssSection::bytes` is
        // `#[cfg(feature = "v4_16")]` in gtk4-rs — gtk4-0.11.4
        // `src/auto/css_section.rs:57`, outside the pinned `v4_14` ceiling,
        // so the DEFAULT build simply omits the dump exactly like ghostty
        // built on the 4.14 floor), and a live `at_least(4, 16, 0)` probe is
        // the runtime analog (this port has no `comptime_version` header
        // constant, so the runtime check is never folded out). See the AD-3
        // BASELINE PIN note in `Cargo.toml` and the `L5 GTK baseline pin`
        // DEFERRALS row.
        #[cfg(feature = "gtk-v4_16")]
        if crate::gtk::gtk_version::at_least(4, 16, 0) {
            // `const bytes = css_section.getBytes() orelse break :bytes;`
            // (`:1262`) — and `const ptr = bytes.getData(&len) orelse break
            // :bytes; const data = ptr[0..len];` (`:1263-1265`). The
            // reference logs the raw CSS BYTES; we keep them as `&[u8]` and
            // render them with the byte-preserving `{:?}`-free lossy view
            // only at the very last step, never converting the payload.
            if let Some(bytes) = section.bytes() {
                let data: &[u8] = &bytes;
                log::warn!(target: LOG_SCOPE, "css parsing failed at {location}: {:?} {} {}\n{}",
                    error.domain(),
                    error.code(),
                    error.message(),
                    String::from_utf8_lossy(data),);
                // `return;` (`:1274`).
                return;
            }
        }

        // The fallback line (`:1275-1280`).
        log::warn!(target: LOG_SCOPE, "css parsing failed at {location}: {:?} {} {}",
            error.domain(),
            error.code(),
            error.message(),);
    }

    /// Configure libxev to use a specific backend.
    ///
    /// This must be called before any other xev APIs are used.
    ///
    /// PORTS: `application.zig:1335-1360` `startupXev`. Configures libxev to a
    /// specific backend when `async-backend` is not `.auto`. Must run before
    /// any other xev API is used.
    fn startup_xev(&self) {
        use rustty_config::value_types::AsyncBackend;

        let config = self.config.borrow();
        let Some(config) = config.as_ref() else {
            return;
        };
        let config = config.get();

        // If our backend is auto then we have no setup to do (`:1340`).
        let backend = match config.async_backend {
            AsyncBackend::Auto => return,
            // Setup our event loop backend to the preferred method
            //
            // On Linux `rustty_xev::DYNAMIC` is TRUE, so this branch is LIVE
            // (unlike macOS). `.epoll => if (comptime xev.dynamic)
            // xev.prefer(.epoll) else false` (`:1345-1346`).
            AsyncBackend::Epoll => rustty_xev::Backend::Epoll,
            AsyncBackend::IoUring => rustty_xev::Backend::IoUring,
        };
        // `#[cfg]` is the image of `comptime xev.dynamic`, as in `lib.rs`'s
        // `detect` call: only Linux has the Dynamic layer. On FreeBSD (kqueue
        // only) `xev.dynamic` is false and the reference's `prefer` call is
        // comptime-elided, so the result is `false` (N555).
        #[cfg(target_os = "linux")]
        let result = if rustty_xev::DYNAMIC {
            rustty_xev::prefer(backend)
        } else {
            false
        };
        #[cfg(not(target_os = "linux"))]
        let result = {
            let _ = backend;
            false
        };

        if result {
            log::info!(target: LOG_SCOPE, "libxev manual backend={:?}",
                rustty_xev::backend());
        } else {
            log::warn!(target: LOG_SCOPE, "libxev manual backend failed, using default={:?}",
                rustty_xev::backend());
        }
    }

    /// Setup the style manager on startup. The primary task here is to
    /// setup our initial light/dark mode based on the configuration and
    /// setup listeners for changes to the style manager.
    ///
    /// PORTS: `application.zig:1365-1397` `startupStyleManager`. Sets the
    /// initial light/dark scheme from `window-theme`, subscribes to scheme
    /// changes, and does one idempotent initial sync.
    fn startup_style_manager(&self) {
        use rustty_config::value_types::WindowTheme;

        let obj = self.obj();
        let style = (*obj).style_manager();

        // Setup our initial light/dark (`:1370-1382`).
        let scheme = {
            let config = self.config.borrow();
            let Some(config) = config.as_ref() else {
                return;
            };
            let config = config.get();
            match config.window_theme {
                // `.auto, .ghostty` — perceived-luminance branch (`:1372-1378`).
                // (`WindowTheme::Rustty` is this port's rename of `.ghostty`.)
                WindowTheme::Auto | WindowTheme::Rustty => {
                    let lum = config.background.to_terminal_rgb().perceived_luminance();
                    if lum > 0.5 {
                        libadwaita::ColorScheme::PreferLight
                    } else {
                        libadwaita::ColorScheme::PreferDark
                    }
                }
                WindowTheme::System => libadwaita::ColorScheme::PreferLight,
                WindowTheme::Dark => libadwaita::ColorScheme::ForceDark,
                WindowTheme::Light => libadwaita::ColorScheme::ForceLight,
            }
        };
        style.set_color_scheme(scheme);

        // Setup color change notifications (`:1385-1391`) — the reference
        // connects `notify::dark`.
        style.connect_dark_notify(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            move |style| {
                this.imp().handle_style_manager_dark(style);
            }
        ));

        // Do an initial color scheme sync. This is idempotent and does nothing
        // if our current theme matches what librustty has so its safe to
        // call.
        //
        // (`:1393-1396`.)
        self.handle_style_manager_dark(&style);
    }

    /// PORTS: `application.zig:1540-1584` `handleStyleManagerDark`. Propagates
    /// the effective light/dark scheme to libghostty-core and every surface.
    fn handle_style_manager_dark(&self, style: &libadwaita::StyleManager) {
        let is_dark = style.is_dark();
        let scheme = if is_dark { "dark" } else { "light" };
        log::debug!(target: LOG_SCOPE, "style manager changed scheme={scheme}");

        // `core_app.colorSchemeEvent(self.rt(), scheme)` + per-surface
        // `surface.core().colorSchemeCallback(scheme)` (`:1551-1563`). Both reach
        // the type-erased `core_app`/`rt_app` (`*mut c_void`) and the concrete
        // surface type, so [`super::Application::bind_core_app`] monomorphizes the
        // propagation into [`color_scheme_thunk`] once the apprt `App` type `A`
        // is known. `None` until bound → no-op push (placeholder-leaf doctrine).
        if let Some(thunk) = self.color_scheme_thunk.borrow().as_ref() {
            thunk(is_dark);
        }

        // The GTK ≥ 4.20 `prefers-color-scheme` push (`:1565-1583`). Ghostty
        // gates this behind `if (gtk_version.atLeast(4, 20, 0))`, whose leading
        // `comptime` fold collapses the whole branch away when built against a
        // pre-4.20 header (`gtk_version.zig:58-62`). rustty mirrors that fold
        // FAITHFULLY as a two-part gate: `#[cfg(feature = "gtk-v4_20")]` is the
        // comptime analog (the `gtk::InterfaceColorScheme` property type is
        // `#[cfg(feature = "v4_20")]` in gtk4-rs, outside the pinned `v4_14`
        // ceiling — the DEFAULT build simply omits the push, exactly like
        // ghostty built on the 4.14 floor), and the runtime `at_least(4, 20, 0)`
        // guard is the runtime analog (a live shared-library probe — this port
        // has no `comptime_version` header constant, so the runtime check is
        // never folded out). See the AD-3 BASELINE PIN note in `Cargo.toml` and
        // the `L5 GTK baseline pin` DEFERRALS row.
        #[cfg(feature = "gtk-v4_20")]
        if crate::gtk::gtk_version::at_least(4, 20, 0) {
            let gtk_scheme = if is_dark {
                gtk4::InterfaceColorScheme::Dark
            } else {
                gtk4::InterfaceColorScheme::Light
            };
            // `var value = gobject.ext.Value.newFrom(gtk_scheme);` — build the
            // `Value` once and reuse it across every provider (`:1570-1582`).
            let value = gtk_scheme.to_value();
            if let Some(css_provider) = self.css_provider.borrow().as_ref() {
                css_provider.set_property_from_value("prefers-color-scheme", &value);
            }
            for css_provider in self.custom_css_providers.borrow().iter() {
                css_provider.set_property_from_value("prefers-color-scheme", &value);
            }
        }
    }

    /// PORTS: `application.zig:1400-1408` `startupSignals`. Installs the SIGUSR2
    /// handler (config reload) via `g_unix_signal_add` and stores its source id
    /// (invariant 11 — removed at dispose).
    fn startup_signals(&self) {
        debug_assert!(
            self.signal_source.borrow().is_none(),
            "startup_signals installs the signal source exactly once"
        );

        // The gtk4-rs bindings do not wrap `g_unix_signal_add`; call the GLib
        // symbol directly (linked via glib-sys). The `self` userdata is the raw
        // `GObject` pointer, valid until dispose removes this source.
        let obj = self.obj();
        let user_data = obj.as_ptr() as glib::ffi::gpointer;
        let raw = unsafe {
            g_unix_signal_add_full(
                glib::ffi::G_PRIORITY_DEFAULT,
                SIGUSR2,
                Some(handle_sigusr2),
                user_data,
                None,
            )
        };
        // `g_unix_signal_add` returns a non-zero source id on success; wrap it
        // as the OWNED `SourceId` handle.
        let source: glib::SourceId = unsafe { glib::translate::from_glib(raw) };
        self.signal_source.replace(Some(source));
    }

    /// PORTS: `application.zig:1473-1497` `startupActionMap`. Registers the
    /// nine application actions verbatim (`:1483-1492`).
    fn startup_action_map(&self) {
        // `glib.ext.VariantType.newFor(u64)` (`:1477`) — the `present-surface`
        // parameter is a `t` (u64); `"as"` (`:1480`) is an array-of-strings for
        // `new-window-command`; `"(tas)"` (`:1483-1484`) is B5's `new-tab`
        // surface-id + argument tuple; `glib.ext.VariantType.newFor([:0]const
        // u8)` (`:1486-1487`) is the `open-config` target string.
        let t_variant_type = glib::VariantTy::UINT64.to_owned();
        let as_variant_type = glib::VariantTy::STRING_ARRAY.to_owned();
        let tas_variant_type = glib::VariantTy::new("(tas)")
            .expect("valid variant type")
            .to_owned();
        let s_variant_type = glib::VariantTy::STRING.to_owned();

        let obj = self.obj();
        let actions = [
            ext::actions::Action::init("new-window", action_new_window, None),
            ext::actions::Action::init(
                "new-window-command",
                action_new_window,
                Some(as_variant_type),
            ),
            ext::actions::Action::init("new-tab", action_new_tab, Some(tas_variant_type)),
            ext::actions::Action::init("open-config", action_open_config, Some(s_variant_type)),
            ext::actions::Action::init(
                "present-surface",
                action_present_surface,
                Some(t_variant_type),
            ),
            ext::actions::Action::init("quit", action_quit, None),
            ext::actions::Action::init("reload-config", action_reload_config, None),
            ext::actions::Action::init("toggle-quick-terminal", action_toggle_quick_terminal, None),
            ext::actions::Action::init("ring-bell", action_ring_bell, None),
        ];

        ext::actions::add(&*obj, &actions);
    }

    /// PORTS: `application.zig:1432-1457` `startupGlobalShortcuts`. Wires the
    /// `GlobalShortcuts` object's dbus connection, the `config` property
    /// binding, and the `trigger` signal.
    fn startup_global_shortcuts(&self) {
        use gio::prelude::ApplicationExt as _;

        let obj = self.obj();

        // Create the `GlobalShortcuts` object (`application.zig:402`
        // `newInstance(GlobalShortcuts)`; the reference builds it in `new` and
        // holds the sole ref, freeing it at dispose — here the manually-held
        // `global_shortcuts` field owns that ref, cleared in `deinit_private`).
        let global_shortcuts = GlobalShortcuts::new();

        // On startup, our dbus connection should be available (`:1435-1437`).
        let dbus = obj.upcast_ref::<gio::Application>().dbus_connection();
        global_shortcuts.set_dbus_connection(dbus.as_ref());

        // Setup a binding so that the shortcut config always matches the app
        // (`:1440-1447`).
        obj.bind_property("config", &global_shortcuts, "config")
            .sync_create()
            .build();

        // Setup the signal handler for global shortcut triggers
        // (`:1450-1456`) → `globalShortcutTrigger` (`:1655-1663`).
        let this_weak = obj.downgrade();
        global_shortcuts.connect_local("trigger", false, move |args| {
            // args[0] is the emitting `GlobalShortcuts`; args[1] is the action.
            let boxed: ActionBoxed = args[1].get().expect("trigger action is an ActionBoxed");
            if let Some(app) = this_weak.upgrade() {
                app.imp().global_shortcut_trigger(&boxed);
            }
            None
        });

        // `_ = GlobalShortcuts.signals.@"bind-failed".connect(...)`
        // (`:1475-1481`) -> `globalShortcutBindFailed` (`:1691-1725`).
        let this_weak = obj.downgrade();
        global_shortcuts.connect_local("bind-failed", false, move |args| {
            let failure: BindFailed = args[1].get().expect("bind-failed carries a BindFailed");
            if let Some(app) = this_weak.upgrade() {
                app.imp().global_shortcut_bind_failed(&failure);
            }
            None
        });

        // Store the owned strong ref (Invariant 12: manually held, never
        // parented; cleared to release).
        self.global_shortcuts
            .replace(Some(global_shortcuts.upcast()));
    }

    /// PORTS: `application.zig:1655-1663` `globalShortcutTrigger`. Perform the
    /// activated global shortcut's action across all surfaces, via the
    /// monomorphized [`Self::perform_all_action_thunk`]
    /// ([`super::Application::bind_core_app`] installs it). An unbound core
    /// performs nothing.
    ///
    /// `pub(super)` (the reference's method is file-private) so the KAT in the
    /// outer module's `mod tests` can drive it directly; `mod imp`'s own call
    /// site in `startup_global_shortcuts` is unaffected.
    pub(super) fn global_shortcut_trigger(&self, action: &ActionBoxed) {
        #[cfg(test)]
        self.global_shortcut_trigger_count
            .set(self.global_shortcut_trigger_count.get() + 1);

        if let Some(thunk) = self.perform_all_action_thunk.borrow().as_ref() {
            thunk(action.0.clone());
        }
    }

    /// PORTS: `application.zig:1691-1725` `globalShortcutBindFailed` — "May fire
    /// before any window exists, hence a desktop notification rather than a
    /// toast."
    ///
    /// The visible half of the per-binding failure channel: without it a keybind
    /// the compositor denied or revoked is a keybind that silently does nothing.
    ///
    /// `pub(super)` (the reference's method is file-private) so the KAT in the
    /// outer module's `mod tests` can drive it directly.
    pub(super) fn global_shortcut_bind_failed(&self, failure: &BindFailed) {
        #[cfg(test)]
        self.global_shortcut_bind_failed_count
            .set(self.global_shortcut_bind_failed_count.get() + 1);

        // `... catch return;` (`:1773-1778`): no notification when the body
        // does not fit the reference's 512-byte buffer (S49-F4 / S50-F3).
        let Some((title, body)) = super::bind_failed_notification(failure) else {
            return;
        };
        // `Action.desktopNotification(self, .app, .{ .title, .body })`
        // (`:1721-1724`).
        action::desktop_notification(
            &self.obj(),
            Target::<super::AppTargetHandle>::App,
            rustty_apprt_api::action::DesktopNotification {
                title: title.as_bytes(),
                body: &body,
            },
        );
    }

    /// Show the config errors dialog if the config on our application
    /// has diagnostics.
    ///
    /// PORTS: `application.zig:1608-1653` `showConfigErrorsDialog`. Presents (or
    /// refreshes) the config-diagnostics dialog when the config has diagnostics.
    fn show_config_errors_dialog(&self) {
        // `const priv = self.private();` (`:1609`). `priv.config` is the
        // reference's non-null `*Config`; ours is an `Option<Config>` that is
        // populated by `new`, so a `None` here means the app is not fully
        // constructed and there is nothing to show.
        let Some(config) = self.config.borrow().clone() else {
            return;
        };
        let has_diagnostics = config.property::<bool>("has-diagnostics");

        // If we already have a dialog, just update the config.
        //
        // `if (priv.config_errors_dialog.get()) |diag| { ... return; }`
        // (`:1612-1631`). The strong ref `get()` hands back (`:1613` `defer
        // diag.unref()`) is dropped at the end of this block.
        if let Some(diag) = self.config_errors_dialog.get() {
            // `gobject.Object.setProperty(diag, "config", &value)`
            // (`:1615-1622`).
            diag.set_property("config", &config);

            // `if (!priv.config.hasDiagnostics()) diag.close() else
            // diag.present(null);` (`:1624-1628`).
            if has_diagnostics {
                diag.present(None);
            } else {
                diag.close();
            }

            return;
        }

        // No diagnostics, do nothing (`:1634-1635`).
        if !has_diagnostics {
            return;
        }

        // "No dialog yet, initialize a new one. There's no need to unref here
        // because the widget that it becomes a part of takes ownership."
        // (`:1637-1640`).
        let dialog = ConfigErrorsDialog::new(&config);
        self.config_errors_dialog.set(Some(&dialog));

        // "Connect to the reload signal so we know to reload our config."
        // (`:1642-1649`) — `ConfigErrorsDialog.signals.@"reload-config"
        // .connect(dialog, *Application, handleReloadConfig, self, .{})`.
        dialog.connect_closure(
            "reload-config",
            false,
            glib::closure_local!(
                #[watch(rename_to = app)]
                self.obj(),
                move |_dialog: &ConfigErrorsDialog| {
                    app.imp().handle_reload_config();
                }
            ),
        );

        // "Show it" (`:1651-1652`).
        dialog.present(None);
    }

    /// PORTS: `application.zig:1585-1606` `handleReloadConfig`. The
    /// `reload-config` signal handler connected in
    /// [`Self::show_config_errors_dialog`].
    fn handle_reload_config(&self) {
        // "We clear our dialog reference because its going to close after
        // response handling and we don't want to reuse it." (`:1589-1592`).
        self.config_errors_dialog.set(None);

        // "Reload our config as if the app reloaded." — `Action.reloadConfig(
        // self, .app, .{})` (`:1594-1605`). On failure "we want the user to
        // know it. For now we log but we should show another GUI."
        let app = self.obj();
        if let Err(err) = action::reload_config(
            &app,
            Target::<AppTargetHandle>::App,
            rustty_apprt_api::action::ReloadConfig::default(),
        ) {
            // If we fail to reload the configuration, then we want the
            // user to know it. For now we log but we should show another
            // GUI.
            //
            // log.warn("error reloading config: {}", .{err});
            log::warn!(target: LOG_SCOPE, "error reloading config: {err}");
        }
    }

    /// PORTS: `application.zig:1145-1200` `syncActionAccelerators` /
    /// `syncActionAccelerator`. Re-derives every menu accelerator from the
    /// current keybind set.
    fn sync_action_accelerators(&self) {
        // PORTS: `application.zig:1142-1167` `syncActionAccelerators`. Maps each
        // menu GTK action to the `input.Binding.Action` it performs, so the
        // accelerator GTK renders next to the menu item is re-derived from the
        // current keybind set on every `notify::config`.
        use rustty_input::binding::{
            Action, CloseTabMode, CopyToClipboard, InspectorMode, OpenConfig, SplitDirection,
        };
        self.sync_action_accelerator("app.quit", &Action::Quit);
        // The two parameterized `open-config` targets (`application.zig:
        // 1208-1209`, 0a183c923): each detailed action name carries its
        // target string, and each maps to the binding action with the
        // matching payload.
        self.sync_action_accelerator(
            "app.open-config::os-open",
            &Action::OpenConfig(OpenConfig::OsOpen),
        );
        self.sync_action_accelerator(
            "app.open-config::new-window",
            &Action::OpenConfig(OpenConfig::NewWindow),
        );
        self.sync_action_accelerator("app.reload-config", &Action::ReloadConfig);
        self.sync_action_accelerator(
            "win.toggle-inspector",
            &Action::Inspector(InspectorMode::Toggle),
        );
        self.sync_action_accelerator("app.show-gtk-inspector", &Action::ShowGtkInspector);
        self.sync_action_accelerator("win.toggle-command-palette", &Action::ToggleCommandPalette);
        self.sync_action_accelerator("win.close", &Action::CloseWindow);
        self.sync_action_accelerator("win.new-window", &Action::NewWindow);
        self.sync_action_accelerator("win.new-tab", &Action::NewTab);
        self.sync_action_accelerator("win.close-tab::this", &Action::CloseTab(CloseTabMode::This));
        self.sync_action_accelerator("tab.close::this", &Action::CloseTab(CloseTabMode::This));
        self.sync_action_accelerator("win.split-right", &Action::NewSplit(SplitDirection::Right));
        self.sync_action_accelerator("win.split-down", &Action::NewSplit(SplitDirection::Down));
        self.sync_action_accelerator("win.split-left", &Action::NewSplit(SplitDirection::Left));
        self.sync_action_accelerator("win.split-up", &Action::NewSplit(SplitDirection::Up));
        self.sync_action_accelerator("win.copy", &Action::CopyToClipboard(CopyToClipboard::Mixed));
        self.sync_action_accelerator("win.paste", &Action::PasteFromClipboard);
        self.sync_action_accelerator("win.reset", &Action::Reset);
        self.sync_action_accelerator("win.clear", &Action::ClearScreen);
        self.sync_action_accelerator("win.prompt-title", &Action::PromptSurfaceTitle);
        self.sync_action_accelerator(
            "split-tree.new-split::left",
            &Action::NewSplit(SplitDirection::Left),
        );
        self.sync_action_accelerator(
            "split-tree.new-split::right",
            &Action::NewSplit(SplitDirection::Right),
        );
        self.sync_action_accelerator(
            "split-tree.new-split::up",
            &Action::NewSplit(SplitDirection::Up),
        );
        self.sync_action_accelerator(
            "split-tree.new-split::down",
            &Action::NewSplit(SplitDirection::Down),
        );
    }

    /// PORTS: `application.zig:1169-1200` `syncActionAccelerator`. Resets the GTK
    /// action's accelerators, then installs the one derived from the current
    /// keybind trigger for `action` (if any) via `gtk.Application.setAccelsForAction`.
    fn sync_action_accelerator(&self, gtk_action: &str, action: &rustty_input::binding::Action) {
        use gtk4::prelude::*;
        let obj = self.obj();
        let gtk_app = obj.upcast_ref::<gtk4::Application>();

        // Reset it initially (`:1169-1170`).
        gtk_app.set_accels_for_action(gtk_action, &[]);

        let config = self.config.borrow();
        let Some(config) = config.as_ref() else {
            return;
        };
        // First trigger bound to this action, if any (`:1174-1175`).
        let Some(trigger) = config.get().keybind.set.get_trigger(action) else {
            return;
        };
        // Trigger → GTK accelerator string (`:1176-1193`).
        //
        // PORT NOTE: the reference formats into a fixed 1024-byte buffer and
        // has an overflow arm that `accel_from_trigger` (a `String`) cannot
        // reach:
        //
        //     // This should really never, never happen. Its not critical enough
        //     // to actually crash, but this is a bug somewhere. An accelerator
        //     // for a trigger can't possibly be more than 1024 bytes.
        let Some(accel) = crate::gtk::key::accel_from_trigger(trigger) else {
            return;
        };
        gtk_app.set_accels_for_action(gtk_action, &[accel.as_str()]);
    }

    /// PORTS: `application.zig:867-928` `loadRuntimeCss`. Regenerates the
    /// app-level runtime CSS (window theme, split styling, title font) into
    /// the shared `css_provider`, replacing any previously loaded CSS. This
    /// is what paints the `windowhandle` (headerbar) opaque on an RGBA
    /// window.
    fn load_runtime_css(&self) {
        use std::fmt::Write as _;

        // Read the current config (`:870`); nothing to do without one
        // (mirrors `start_quit_timer`).
        let cfg = self.config.borrow();
        let Some(cfg) = cfg.as_ref() else {
            return;
        };
        let config = cfg.get();

        // `std.Io.Writer.Allocating` seeded at 2048 bytes (`:872`).
        let mut css = String::with_capacity(2048);

        // Load standard css first as it can override some of the user
        // configured styling (`:877-879`).
        load_runtime_css_414(config, &mut css);
        load_runtime_css_416(config, &mut css);

        // `widget.unfocused-split` (`:881-894`).
        let unfocused_fill = config.unfocused_split_fill.unwrap_or(config.background);
        write!(
            css,
            "widget.unfocused-split {{\n opacity: {:.2};\n background-color: rgb({},{},{});\n}}\n",
            1.0 - config.unfocused_split_opacity,
            unfocused_fill.r,
            unfocused_fill.g,
            unfocused_fill.b,
        )
        .unwrap();

        // `.window .split paned > separator` divider color, when set
        // (`:896-908`).
        if let Some(color) = config.split_divider_color {
            write!(
                css,
                ".window .split paned > separator {{\n  color: rgb({r},{g},{b});\n  background: rgb({r},{g},{b});\n}}\n",
                r = color.r,
                g = color.g,
                b = color.b,
            )
            .unwrap();
        }

        // `.window headerbar` title font family, when set (`:910-917`).
        // `window-title-font-family` is `Option<Vec<u8>>` here, so render it
        // via `String::from_utf8_lossy`.
        if let Some(font_family) = config.window_title_font_family.as_ref() {
            let font_family = String::from_utf8_lossy(font_family);
            write!(
                css,
                ".window headerbar {{\n  font-family: \"{font_family}\";\n}}\n",
            )
            .unwrap();
        }

        // log.debug("runtime CSS is {d} bytes", .{contents.len});
        //
        // S49-F5: this was dropped entirely on the grounds that the port had
        // "no debug-logging shim". It has one now (`DECISION-logging.md`
        // §6r4). The reference's `contents` is the built CSS byte slice, so
        // the count is `css.len()`.
        log::debug!(target: LOG_SCOPE, "runtime CSS is {} bytes", css.len());

        // Clears any previously loaded CSS from this provider (`:923-927`).
        // gtk4-rs 0.11 `CssProvider::load_from_string` has the same
        // clear-then-load semantics as the oracle's `loadFromBytes`. If the
        // provider isn't installed yet, return without loading (placeholder-
        // leaf doctrine — the provider is created in `new`).
        let provider = self.css_provider.borrow();
        let Some(provider) = provider.as_ref() else {
            return;
        };
        provider.load_from_string(&css);
    }

    /// PORTS: `application.zig:1119-1199` `loadCustomCss`. Loads each
    /// `gtk-custom-css` file into a fresh provider appended to
    /// `custom_css_providers` and registered on the display at USER priority.
    ///
    /// Returns the reference's `LoadCustomCssError` arm: a read error other
    /// than `FileTooBig` on a file that OPENED stops the whole load (the files
    /// after it are not loaded; the ones before it stay applied), and the caller
    /// logs `failed to load custom CSS, no custom CSS applied, err={}`
    /// (`:1309-1314`). Measured on the VM with the reference's own open + read
    /// code (Zig 0.16, aarch64-linux): a DIRECTORY named in `gtk-custom-css`
    /// opens, then its first read fails with `error.IsDir`.
    ///
    /// CORRECTION (task T-D, S49-F3): this returned `()`. It checked the size
    /// with `fs::metadata` and read with `fs::read`, so a read error was logged
    /// as "error opening gtk-custom-css file" and skipped, and the files after
    /// it still loaded. It also logged `loading gtk-custom-css path=` only
    /// after a successful read (the reference logs it between the open and the
    /// read), printed the size limit as `5242880 bytes` (the reference's `{Bi}`
    /// prints `5MiB`, measured), and parsed `String::from_utf8_lossy(contents)`
    /// where the reference hands GTK the file's bytes (`glib.Bytes` +
    /// `loadFromBytes`). GTK treats those differently: measured on GTK 4.22.4,
    /// a stylesheet whose comment holds one Latin-1 byte parses to NOTHING from
    /// raw bytes, and to its rules after U+FFFD substitution.
    pub(super) fn load_custom_css(&self) -> std::io::Result<()> {
        use std::os::unix::ffi::OsStrExt;

        // `gdk.Display.getDefault() orelse { warn; return }` (`:1126-1129`).
        let Some(display) = gtk4::gdk::Display::default() else {
            log::warn!(target: LOG_SCOPE, "unable to get display");
            return Ok(());
        };

        // Unload the previously loaded style providers, then clear the list
        // (`:1131-1138`). Each was registered at USER priority below; drop it
        // off the display before releasing our reference.
        {
            let mut providers = self.custom_css_providers.borrow_mut();
            for provider in providers.iter() {
                gtk4::style_context_remove_provider_for_display(&display, provider);
            }
            providers.clear();
        }

        let cfg = self.config.borrow();
        let Some(cfg) = cfg.as_ref() else {
            return Ok(());
        };
        let config = cfg.get();

        // `for (config.@"gtk-custom-css".value.items) |p|` (`:1141`). Each path
        // was already expanded to absolute at config load.
        for p in &config.gtk_custom_css.value {
            // `.optional => .{ path, true }` / `.required => .{ path, false }`
            // (`:1142-1145`).
            let (path_bytes, optional) = match p {
                rustty_config::path::Path::Optional(path) => (path, true),
                rustty_config::path::Path::Required(path) => (path, false),
            };
            let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path_bytes));

            // const file = std.Io.Dir.openFileAbsolute(global.io(), path, .{})
            //     catch |err| { if (err != error.FileNotFound or !optional) log.warn(...); continue; };
            // (`:1146-1158`). A missing OPTIONAL file is silent.
            let file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(err) => {
                    if err.kind() != std::io::ErrorKind::NotFound || !optional {
                        log::warn!(target: LOG_SCOPE, "error opening gtk-custom-css file {}: {err}",
                            path.display());
                    }
                    continue;
                }
            };

            // const css_file_size_limit = 5 * 1024 * 1024; // 5MB
            const CSS_FILE_SIZE_LIMIT: usize = 5 * 1024 * 1024;

            // log.info("loading gtk-custom-css path={s}", .{path});
            log::info!(target: LOG_SCOPE, "loading gtk-custom-css path={}", path.display());

            // const contents = compat_file.readToEndAlloc(file, alloc, css_file_size_limit)
            //     catch |err| switch (err) {
            //         error.FileTooBig => { log.warn(...); continue; },
            //         else => |e| return e,
            //     };
            // (`:1166-1176`).
            let contents = match super::read_to_end_alloc(file, CSS_FILE_SIZE_LIMIT) {
                Ok(contents) => contents,
                Err(super::ReadToEndAllocError::FileTooBig) => {
                    // `{Bi}` of `css_file_size_limit` (5 * 1024 * 1024) prints
                    // `5MiB` (Zig 0.16, measured on the VM).
                    const CSS_FILE_SIZE_LIMIT_BI: &str = "5MiB";
                    // log.warn("gtk-custom-css file {s} was larger than {Bi}", .{ path, css_file_size_limit });
                    log::warn!(target: LOG_SCOPE, "gtk-custom-css file {} was larger than {CSS_FILE_SIZE_LIMIT_BI}",
                        path.display());
                    continue;
                }
                Err(super::ReadToEndAllocError::Io(err)) => return Err(err),
            };

            // const bytes = glib.Bytes.new(contents.ptr, contents.len); (`:1180`)
            let bytes = glib::Bytes::from_owned(contents);

            // Fresh provider, wire the parse-error logger, own it in the Vec,
            // load, and register on the display at USER priority (`:1183-1197`).
            let css_provider = gtk4::CssProvider::new();
            let obj = self.obj();
            css_provider.connect_parsing_error(glib::clone!(
                #[weak]
                obj,
                move |_provider, section, error| {
                    obj.imp().signal_css_parsing_error(section, error);
                }
            ));
            self.custom_css_providers
                .borrow_mut()
                .push(css_provider.clone());
            // css_provider.loadFromBytes(bytes); (`:1196`)
            css_provider.load_from_bytes(&bytes);
            gtk4::style_context_add_provider_for_display(
                &display,
                &css_provider,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );
        }
        Ok(())
    }

    /// This will get called when there are no more open surfaces.
    ///
    /// PORTS: `application.zig:823-846` `startQuitTimer`. Called when the last
    /// surface closes: arms (or immediately expires) the "quit after the last
    /// window closed" timer. Cancels any prior timer first, so calling it more
    /// than once never leaks a source (invariant 11).
    ///
    /// PORT NOTE (placeholder-leaf): the production call site is surface
    /// open/close (`window.zig`/`surface.zig`, Task 15+), which does not exist
    /// yet — so in a non-test build this is currently only reachable from the
    /// `quit_timer_start_stop_is_idempotent` KAT. `stop_quit_timer` (its
    /// counterpart) IS live now via [`RunCleanup`]. `#[allow(dead_code)]` until
    /// the surface-lifecycle callers land.
    #[allow(dead_code)]
    pub(super) fn start_quit_timer(&self) {
        // Cancel any previous timer.
        //
        // (`:828`.) This makes repeated arming
        // idempotent and leak-free.
        self.stop_quit_timer();

        let config = self.config.borrow();
        let Some(config) = config.as_ref() else {
            return;
        };
        let config = config.get();

        // This is a no-op unless we are configured to quit after last window is closed.
        //
        // (`:831`.)
        if !config.quit_after_last_window_closed {
            return;
        }

        // If a delay is configured, set a timeout function to quit after the delay.
        //
        // (`:834-845`.)
        if let Some(delay) = config.quit_after_last_window_closed_delay {
            // `glib.timeoutAdd(v.asMilliseconds(), handleQuitTimerExpired, self)`
            // (`:836-840`). The reference passes a raw `self` userdata (no
            // refcount); the gtk4-rs analogue is a weak clone, so the timeout
            // never keeps the application alive on its own. The closure returns
            // `ControlFlow::Break` to fire exactly once, matching the
            // reference's `return 0` (`G_SOURCE_REMOVE`, `:1537`).
            let millis = u64::from(delay.as_milliseconds());
            let obj = self.obj();
            let source = glib::timeout_add_local(
                std::time::Duration::from_millis(millis),
                glib::clone!(
                    #[weak]
                    obj,
                    #[upgrade_or]
                    glib::ControlFlow::Break,
                    move || {
                        obj.imp().handle_quit_timer_expired();
                        glib::ControlFlow::Break
                    }
                ),
            );
            *self.quit_timer.borrow_mut() = QuitTimer::Active(source);
        } else {
            // If no delay is configured, treat it as expired.
            //
            // (`:844`.)
            *self.quit_timer.borrow_mut() = QuitTimer::Expired;
        }
    }

    /// This will get called when a new surface gets opened.
    ///
    /// PORTS: `application.zig:849-864` `stopQuitTimer`. Cancels the quit timer
    /// and returns it to `.off`. Idempotent for every state (invariant 11).
    pub(super) fn stop_quit_timer(&self) {
        // Take the current state so we can drop/remove the OWNED source id
        // without holding the borrow across `g_source_remove`.
        let prev = std::mem::replace(&mut *self.quit_timer.borrow_mut(), QuitTimer::Off);
        match prev {
            // `.off => {}` (`:852`).
            QuitTimer::Off => {}
            // `.expired => priv.quit_timer = .off` (`:853`) — already reset above.
            QuitTimer::Expired => {}
            // `.active => |source| { if remove(source) == 0 warn; .off }`
            // (`:854-863`). Use the raw `g_source_remove` so a failure warns and
            // continues (the reference logs), rather than [`glib::SourceId::remove`]
            // which panics on failure.
            QuitTimer::Active(source) => {
                let removed = unsafe { glib::ffi::g_source_remove(source.as_raw()) };
                if removed == glib::ffi::GFALSE {
                    log::warn!(target: LOG_SCOPE, "unable to remove quit timer source={}",
                        source.as_raw());
                }
                // `source` is consumed here (moved out of the timer); dropping it
                // does NOT double-remove — `SourceId`'s own `Drop` is a no-op.
            }
        }
    }

    /// PORTS: `application.zig:1533-1538` `handleQuitTimerExpired`. The one-shot
    /// timeout callback: flags the timer expired so the run loop's `must_quit`
    /// check (`:561-564`) trips on its next iteration, then removes itself
    /// (`return 0`). Reachable only from [`Self::start_quit_timer`]'s timeout
    /// closure, so it inherits that method's placeholder-leaf status until the
    /// surface-lifecycle callers land (Task 15+).
    #[allow(dead_code)]
    pub(super) fn handle_quit_timer_expired(&self) {
        *self.quit_timer.borrow_mut() = QuitTimer::Expired;
    }
}

/// Load runtime CSS for older than GTK 4.16
///
/// PORTS: `application.zig:930-964` `loadRuntimeCss414`.
fn load_runtime_css_414(config: &CoreConfig, css: &mut String) {
    use rustty_config::value_types::WindowTheme;
    use std::fmt::Write as _;

    // GTK 4.16+ uses the CSS-variable path in `load_runtime_css_416`
    // instead (`:935`).
    if crate::gtk::gtk_version::runtime_at_least(4, 16, 0) {
        return;
    }

    let headerbar_background = config
        .window_titlebar_background
        .unwrap_or(config.background);
    let headerbar_foreground = config
        .window_titlebar_foreground
        .unwrap_or(config.foreground);

    // Only the rustty (ghostty) window theme paints the headerbar; every
    // other theme defers to the system/libadwaita styling (`:941-963`).
    // (`WindowTheme::Rustty` is this port's rename of the oracle's
    // `.ghostty`.)
    //
    // PORT NOTE: the reference is `switch (window_theme) { .ghostty => ...,
    // else => {} }`; a two-arm match whose second arm is the catch-all IS an
    // `if` (clippy::single_match), so it is spelled as one.
    if config.window_theme == WindowTheme::Rustty {
        write!(
            css,
            "windowhandle {{\n  background-color: rgb({},{},{});\n  color: rgb({},{},{});\n}}\nwindowhandle:backdrop {{\n background-color: oklab(from rgb({},{},{}) calc(l * 0.9) a b / alpha);\n}}\n",
            headerbar_background.r,
            headerbar_background.g,
            headerbar_background.b,
            headerbar_foreground.r,
            headerbar_foreground.g,
            headerbar_foreground.b,
            headerbar_background.r,
            headerbar_background.g,
            headerbar_background.b,
        )
        .unwrap();
    }
}

/// Load runtime for GTK 4.16 and newer
///
/// PORTS: `application.zig:966-1064` `loadRuntimeCss416`.
fn load_runtime_css_416(config: &CoreConfig, css: &mut String) {
    use rustty_config::value_types::WindowTheme;
    use std::fmt::Write as _;

    // Older than GTK 4.16 uses the direct-rgb path in
    // `load_runtime_css_414` instead (`:971`).
    if crate::gtk::gtk_version::runtime_until(4, 16, 0) {
        return;
    }

    let headerbar_background = config
        .window_titlebar_background
        .unwrap_or(config.background);
    let headerbar_foreground = config
        .window_titlebar_foreground
        .unwrap_or(config.foreground);

    // Static block, emitted verbatim (`writeAll`, `:977-1030`).
    css.push_str(
        "/*\n\
         \x20* Child Exited Overlay\n\
         \x20*/\n\
         \n\
         .child-exited.normal revealer widget {\n\
         \x20 background-color: color-mix(\n\
         \x20   in srgb,\n\
         \x20   var(--success-bg-color),\n\
         \x20   transparent 50%\n\
         \x20 );\n\
         }\n\
         \n\
         .child-exited.abnormal revealer widget {\n\
         \x20 background-color: color-mix(\n\
         \x20   in srgb,\n\
         \x20   var(--error-bg-color),\n\
         \x20   transparent 50%\n\
         \x20 );\n\
         }\n\
         \n\
         /*\n\
         \x20* Surface\n\
         \x20*/\n\
         \n\
         .surface progressbar.error trough progress {\n\
         \x20 background-color: color-mix(\n\
         \x20   in srgb,\n\
         \x20   var(--error-bg-color),\n\
         \x20   transparent 50%\n\
         \x20 );\n\
         }\n\
         \n\
         .surface .bell-overlay {\n\
         \x20 border-color: color-mix(\n\
         \x20   in srgb,\n\
         \x20   var(--accent-color),\n\
         \x20   transparent 50%\n\
         \x20 );\n\
         }\n\
         \n\
         /*\n\
         \x20* Drag and Drop Overlay\n\
         \x20*/\n\
         .drop-overlay.drop-left {\n\
         \x20 background: linear-gradient(\n\
         \x20   to left,\n\
         \x20   transparent, 50%,\n\
         \x20   color-mix(in srgb, var(--accent-bg-color), transparent 80%) 50%\n\
         \x20 );\n\
         }\n\
         .drop-overlay.drop-right {\n\
         \x20 background: linear-gradient(\n\
         \x20   to right,\n\
         \x20   transparent, 50%,\n\
         \x20   color-mix(in srgb, var(--accent-bg-color), transparent 80%) 50%\n\
         \x20 );\n\
         }\n\
         .drop-overlay.drop-top {\n\
         \x20 background: linear-gradient(\n\
         \x20   to top,\n\
         \x20   transparent, 50%,\n\
         \x20   color-mix(in srgb, var(--accent-bg-color), transparent 80%) 50%\n\
         \x20 );\n\
         }\n\
         .drop-overlay.drop-bottom {\n\
         \x20 background: linear-gradient(\n\
         \x20   to bottom,\n\
         \x20   transparent, 50%,\n\
         \x20   color-mix(in srgb, var(--accent-bg-color), transparent 80%) 50%\n\
         \x20 );\n\
         }\n\
         \n\
         /*\n\
         \x20* Splits\n\
         \x20*/\n\
         \n\
         .window .split paned > separator {\n\
         \x20 background-color: color-mix(\n\
         \x20   in srgb,\n\
         \x20   var(--window-bg-color),\n\
         \x20   transparent 0%\n\
         \x20 );\n\
         }\n",
    );

    // Only the rustty (ghostty) window theme overrides the libadwaita
    // color variables + headerbar (`:1032-1063`). (`WindowTheme::Rustty`
    // is this port's rename of the oracle's `.ghostty`.) Note: this block
    // intentionally has no trailing newline, matching the oracle bytes.
    //
    // PORT NOTE: the reference is `switch (window_theme) { .ghostty => ...,
    // else => {} }`; a two-arm match whose second arm is the catch-all IS an
    // `if` (clippy::single_match), so it is spelled as one.
    if config.window_theme == WindowTheme::Rustty {
        write!(
                css,
                ":root {{\n  \
                 --ghostty-fg: rgb({},{},{});\n  \
                 --ghostty-bg: rgb({},{},{});\n  \
                 --headerbar-fg-color: var(--ghostty-fg);\n  \
                 --headerbar-bg-color: var(--ghostty-bg);\n  \
                 --headerbar-backdrop-color: oklab(from var(--headerbar-bg-color) calc(l * 0.9) a b / alpha);\n  \
                 --overview-fg-color: var(--ghostty-fg);\n  \
                 --overview-bg-color: var(--ghostty-bg);\n  \
                 --popover-fg-color: var(--ghostty-fg);\n  \
                 --popover-bg-color: var(--ghostty-bg);\n  \
                 --window-fg-color: var(--ghostty-fg);\n  \
                 --window-bg-color: var(--ghostty-bg);\n\
                 }}\n\
                 windowhandle {{\n  \
                 background-color: var(--headerbar-bg-color);\n  \
                 color: var(--headerbar-fg-color);\n\
                 }}\n\
                 windowhandle:backdrop {{\n \
                 background-color: var(--headerbar-backdrop-color);\n\
                 }}",
                headerbar_foreground.r,
                headerbar_foreground.g,
                headerbar_foreground.b,
                headerbar_background.r,
                headerbar_background.g,
                headerbar_background.b,
            )
            .unwrap();
    }
}
