//! rustty-apprt::gtk::class::application
//!
//! PORTS: ghostty `src/apprt/gtk/class/application.zig` (`Application` =
//! `GhosttyApplication`, an `adw.Application` subclass, `application.zig:135-140`).
//! This file grew from the **subclass skeleton** (L5.1 T7 — the GObject
//! registration, the `Private` state carrier, class-init, and the process-global
//! singleton accessors `default`/`allocator`/`get_config`). **L5.1 T8** fills the
//! primary entrypoint pair `new` (`application.zig:243-428`) and `deinit`
//! (`:435-459`), plus the file-local `glibLogWriterFunction` (`:48-110`) and
//! `setGtkEnv` (`:2849-2951`), and stitches in the `winproto` union field
//! ([`crate::gtk::winproto`], noop-only). The remaining heavyweight members —
//! the `startup`/`activate`/`dispose`/`finalize` vfunc bodies (Task 9, which also
//! fills the `notify::config` / `parsing-error` handler leaves this task wires),
//! the hand-rolled run loop / `run` (Task 10), and `perform_action` (Task 23) —
//! stay INHABITED placeholders (`unimplemented!()` / no-op leaves, placeholder-leaf
//! doctrine) so the call-graph shape exists and this compiles standalone.
//!
//! # Idiom map (`class.zig` → gtk4-rs, see [`super`])
//!
//! * `Common(Self, Private)` (`application.zig:139` via `class.zig:16`) →
//!   `#[glib::object_subclass]` on the `imp` `ObjectSubclass` impl; the
//!   `Private` struct (`application.zig:163-222`) → the `imp::Application`
//!   instance struct.
//! * `gobject.ext.defineClass(.{ .name = "GhosttyApplication", .classInit =
//!   &Class.init, ... })` (`application.zig:135-140`) → `const NAME =
//!   "GhosttyApplication"` + `type ParentType = adw::Application` + the
//!   `ObjectSubclass::class_init` hook (the faithful analogue of the reference's
//!   `Class.init`, `application.zig:1862-1893`).
//! * the `config` property (`application.zig:141-160`, getter `getConfig`,
//!   `getter_transfer = .full`, type `?*Config`) → a `#[derive(glib::Properties)]`
//!   read-only, nullable property with a custom getter that hands back a fresh
//!   strong ref (`.full`).
//! * the four vfunc overrides `activate`/`startup`/`dispose`/`finalize`
//!   registered in `Class.init` (`application.zig:1889-1892`) → the corresponding
//!   `*Impl` trait methods (`ApplicationImpl::activate`/`::startup`,
//!   `ObjectImpl::dispose`; GObject `finalize` is discharged by the `imp` struct's
//!   `Drop`). Their bodies are Task 9's; the skeleton leaves the default
//!   passthroughs so the type registers cleanly.
//!
//! # Invariant 2 — the ONE canonical allocator
//!
//! `Application.allocator()` returns `core_app.alloc` (`application.zig:464-466`):
//! every other class allocates and frees libghostty-core objects through this ONE
//! allocator so alloc-side ↔ free-side stay symmetric (PREPORT_GTK §2, invariant 2;
//! the load-bearing case is `Surface.core_surface`, allocated at Task 16's create
//! site and `destroy`'d at Task 15's free site with the identical allocator). In
//! rustty the allocator seam is the workspace's global allocator, so [`Application::allocator`]
//! returns [`allocator_api2::alloc::Global`] — a zero-sized handle to the SAME
//! global allocator every `Box<CoreSurface>` in the tier is dropped by. The
//! invariant survives as: the `Box<CoreSurface>` a Surface creates is dropped by
//! this same `Global`.
//!
//! # Invariant 12 — the floating-ref sink
//!
//! gtk4-rs sinks a widget's floating ref automatically when it is parented into a
//! container, but NOT for refs the port holds manually. The manually-held OWNED
//! rows here — `css_provider`, `global_shortcuts` (and, in the Surface tier,
//! `tab_bindings`) — are never parented, so nothing sinks their ref for us; the
//! port holds the strong ref explicitly (a `Some(..)` in the field) and releases
//! it by clearing the field (Task 8's `deinit`, PREPORT_GTK §1.1 free sites).

use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CString};
use std::ptr::NonNull;

use glib::prelude::*;
use glib::subclass::prelude::*;
use rustty_apprt_api::{CoreSurfaceHandle, Target};
use rustty_config::Config as CoreConfig;

use super::config::Config;
use crate::gtk::surface::Surface as ApprtSurface;
use crate::gtk::{build_info, ext, winproto, WeakRef};

mod imp {
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

    /// PORTS: `application.zig:163-222` `Application.Private`. Every pointer-typed
    /// field maps a PREPORT_GTK §1.1 LIFETIMES row; the transfer/ownership mode is
    /// preserved 1:1 (see the field docs). Interior mutability is `RefCell`/`Cell`
    /// because the subclass `imp` is shared `&self` (GObject holds it), matching
    /// the reference's `self.private()` mutable access.
    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::Application)]
    pub struct Application {
        /// PORTS: `application.zig:167` `rt_app: *ApprtApp` (BACKREF, never freed
        /// here). The apprt `App` owns the sole created `Application` ref and
        /// backrefs it; this is the reverse edge. Stored as a raw non-owning
        /// pointer. PORT NOTE: typed as `*mut c_void` in the skeleton because the
        /// apprt `App` type (`ApprtApp`) lands in Task 11 (`src/gtk/app.rs`); Task 8
        /// (`new`) fills this field and Task 11 narrows the pointee type. `None`
        /// until `new` sets it.
        pub rt_app: Cell<Option<NonNull<c_void>>>,

        /// PORTS: `application.zig:170` `core_app: *CoreApp` (BACKREF; libghostty
        /// core owns it, `alloc = core_app.alloc`, `application.zig:247`; not freed
        /// here). Raw non-owning pointer. PORT NOTE: `*mut c_void` in the skeleton —
        /// `CoreApp = rustty_core::App<ApprtApp>` cannot be named until the apprt
        /// `App` type exists (Task 11); Task 8 fills it and Task 11 narrows it.
        pub core_app: Cell<Option<NonNull<c_void>>>,

        /// PORTS: `application.zig:173` `config: *Config` (SHARED — refcounted
        /// [`Config`] GObject shared across the tier). The `config` property is
        /// backed by this field; the getter refs (`.full`), a Task 8 `setConfig`
        /// unrefs the prior and refs the new. `None` until `new`.
        #[property(get = Self::get_config, name = "config", nullable)]
        pub config: RefCell<Option<Config>>,

        /// PORTS: `application.zig:182` `global_shortcuts: *GlobalShortcuts` (OWNED
        /// — created here, sole ref, `unref` at dispose, PREPORT_GTK §1.1). PORT
        /// NOTE: stored as a bare `glib::Object` in the skeleton because the
        /// `GlobalShortcuts` subclass lands in Task 27; `startupGlobalShortcuts`
        /// (Task 9) creates it. Invariant 12: manually held, never parented — the
        /// port holds the strong ref explicitly and clears it to release.
        pub global_shortcuts: RefCell<Option<glib::Object>>,

        /// PORTS: `application.zig:203-205` `config_errors_dialog:
        /// WeakRef(ConfigErrorsDialog)` (SHARED / non-owning — the dialog may close
        /// on its own outside our lifecycle, so a weak ref is correct). PORT NOTE:
        /// `WeakRef<glib::Object>` in the skeleton (the base GObject type) — Task 9
        /// (`showConfigErrorsDialog`) narrows it to `ConfigErrorsDialog` once that
        /// class exists (Task 24). Weak, so narrowing the pointee is safe.
        pub config_errors_dialog: WeakRef<glib::Object>,

        /// PORTS: `application.zig:208` `css_provider: *gtk.CssProvider` (OWNED —
        /// `.new()` here, `unref` at dispose). Invariant 12: manually held, never
        /// parented. `None` until `new`.
        pub css_provider: RefCell<Option<gtk4::CssProvider>>,

        /// PORTS: `application.zig:212` `custom_css_providers:
        /// ArrayListUnmanaged(*gtk.CssProvider)` (OWNED — each element `unref`'d and
        /// the list `deinit`'d at dispose). A `Vec<CssProvider>` whose `Drop`
        /// unrefs each element.
        pub custom_css_providers: RefCell<Vec<gtk4::CssProvider>>,

        /// PORTS: `application.zig:217` `saved_language: ?[:0]const u8` (OWNED — a
        /// heap copy of the launch-time `LANG`, `alloc.free`'d at dispose). A
        /// NUL-terminated [`CString`] (the reference type is a sentinel-terminated
        /// slice). `None` when `LANG` was unset.
        pub saved_language: RefCell<Option<CString>>,

        /// PORTS: `application.zig:206` `signal_source: ?c_uint` (OWNED handle — the
        /// glib source id for the SIGINT/SIGTERM handler; `Source.remove` at
        /// dispose). A [`glib::SourceId`]; dropping/removing it detaches the source
        /// (invariant 11, RAII). `None` when no handler is installed.
        pub signal_source: RefCell<Option<glib::SourceId>>,

        /// PORTS: `application.zig:194-201` `quit_timer` (OWNED handle — see
        /// [`QuitTimer`]). `.off` until armed.
        pub quit_timer: RefCell<QuitTimer>,

        /// PORTS: `application.zig:186` `requested_window: bool` — set true once we
        /// have requested a window exactly once, so we don't quit before showing
        /// one. Control flag; Task 8/Task 10 drive it.
        pub requested_window: Cell<bool>,

        /// PORTS: `application.zig:191` `running: bool` — the hand-rolled run loop's
        /// continue flag; set false to quit. Set only by the main-loop thread
        /// (Task 10). Control flag.
        pub running: Cell<bool>,

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

        /// Test-only instrumentation: how many times the hand-rolled run loop has
        /// executed the inline core tick (`application.zig:552`). The
        /// `application_run_loop_ticks_then_quits` KAT asserts this is ≥ 1 to prove
        /// the tick site actually ran (the reference's `try priv.core_app.tick`
        /// leaves no observable counter, so the port adds one). `#[cfg(test)]` so
        /// production carries no extra field.
        #[cfg(test)]
        pub tick_count: Cell<u64>,
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
            if let Some(_diag) = self.config_errors_dialog.get() {
                // PORT NOTE: `diag.close()` (`:1478`) lands with the
                // `ConfigErrorsDialog` class (Task 24). The strong ref from
                // `get()` is unref'd here by Rust's scope-end drop
                // (`:1479`). Until Task 24, `show_config_errors_dialog` never
                // populates this weak ref, so this arm is dormant.
            }
            self.config_errors_dialog.set(None);

            // Remove the SIGUSR2 source (invariant 11, `:1482-1487`). `take()`
            // gives us the OWNED `SourceId`; we remove it via the raw
            // `g_source_remove` so we can warn-and-continue on failure, exactly
            // like the reference (`SourceId::remove` would instead panic).
            if let Some(source) = self.signal_source.take() {
                let removed = unsafe { glib::ffi::g_source_remove(source.as_raw()) };
                if removed == glib::ffi::GFALSE {
                    eprintln!("warn(gtk): unable to remove signal source");
                }
            }
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
            eprintln!("debug(gtk): startup");

            // `gio.Application.virtual_methods.startup.call(parent)` (`:1297-1300`).
            self.parent_startup();

            let obj = self.obj();

            // Set ourselves as the default application (`:1303`).
            (*obj).set_default();

            // The D-Bus connection is only valid after GApplication startup
            // (`:1305-1308`). PORT NOTE (L6 placeholder): the portal `OpenURI`
            // object lands later; its `set_dbus_connection(app.dbus_connection())`
            // call site is kept INHABITED here as a documented deferral so the
            // startup order is faithful. `app.dbus_connection()` is available now
            // (post parent-startup) but has no consumer until the OpenURI body.

            // Setup our event loop backend (`:1311`).
            self.startup_xev();
            // Setup our style manager — light/dark mode (`:1314`).
            self.startup_style_manager();
            // Setup signal handlers (`:1317`).
            self.startup_signals();
            // Setup our action map (`:1320`).
            self.startup_action_map();
            // Setup our global shortcuts (`:1323`).
            self.startup_global_shortcuts();
            // Show the config-diagnostics dialog if loading produced any (`:1329`).
            self.show_config_errors_dialog();
        }

        /// PORTS: `application.zig:1459-1473` `activate`. Queues a new window onto
        /// the core app mailbox (**the app's only window-creation entry** — it goes
        /// through the mailbox, not a direct call), then chains the parent activate.
        fn activate(&self) {
            eprintln!("debug(gtk): activate");

            // `priv.core_app.mailbox.push(.{ .new_window = {} }, .forever)`
            // (`:1463-1466`), monomorphized into [`new_window_thunk`] by
            // [`super::Application::bind_core_app`] (see that field's doc for why
            // the non-generic vfunc cannot name `App<A>` directly). A `None` thunk
            // — the core app is not yet bound — makes this a no-op push, faithful
            // to a runtime with nothing wired behind the mailbox yet.
            if let Some(thunk) = self.new_window_thunk.borrow().as_ref() {
                thunk();
            }

            // `gio.Application.virtual_methods.activate.call(parent)` (`:1468-1472`).
            self.parent_activate();
        }
    }
    impl gtk4::subclass::prelude::GtkApplicationImpl for Application {}
    impl libadwaita::subclass::prelude::AdwApplicationImpl for Application {}

    impl Application {
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

            // Load our runtime and custom CSS (`:1238-1249`). A failure here just
            // leaves the old CSS in place — it must not fail the whole config
            // change — so both loaders warn-and-continue internally.
            self.load_runtime_css();
            self.load_custom_css();
        }

        /// PORTS: `application.zig:...` `signalCssParsingError` — the
        /// `CssProvider::parsing-error` handler that logs CSS parse errors,
        /// connected in `new` (`application.zig:416-422`).
        ///
        /// **Task 9 fills this.** Placeholder-leaf no-op.
        pub(super) fn signal_css_parsing_error(
            &self,
            section: &gtk4::CssSection,
            error: &glib::Error,
        ) {
            // `css_section.toString()` (`:1259`). gtk4-rs's `CssSection` renders
            // its location via `Display`.
            let location = section.to_str();
            // PORT NOTE: the reference's `gtk_version.atLeast(4, 16, 0)` arm
            // (`:1261-1274`) additionally dumps the offending CSS bytes via
            // `css_section.getBytes()`. `CssSection::bytes` is `#[cfg(v4_16)]` in
            // gtk4-rs and the pinned binding surface is `v4_14` (AD-3 / Cargo.toml),
            // so the byte-dump is not reachable through the bindings here; the
            // location + domain/code/message line (`:1275-1280`) is preserved.
            eprintln!(
                "warn(gtk): css parsing failed at {location}: {:?} {} {}",
                error.domain(),
                error.code(),
                error.message(),
            );
        }

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
                // On Linux `rustty_xev::DYNAMIC` is TRUE, so this branch is LIVE
                // (unlike macOS). `.epoll => if (comptime xev.dynamic)
                // xev.prefer(.epoll) else false` (`:1345-1346`).
                AsyncBackend::Epoll => rustty_xev::Backend::Epoll,
                AsyncBackend::IoUring => rustty_xev::Backend::IoUring,
            };
            let result = if rustty_xev::DYNAMIC {
                rustty_xev::prefer(backend)
            } else {
                false
            };

            if result {
                eprintln!(
                    "info(gtk): libxev manual backend={:?}",
                    rustty_xev::backend()
                );
            } else {
                eprintln!(
                    "warn(gtk): libxev manual backend failed, using default={:?}",
                    rustty_xev::backend()
                );
            }
        }

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

            // Initial idempotent color-scheme sync (`:1393-1396`).
            self.handle_style_manager_dark(&style);
        }

        /// PORTS: `application.zig:1540-1584` `handleStyleManagerDark`. Propagates
        /// the effective light/dark scheme to libghostty-core and every surface.
        fn handle_style_manager_dark(&self, style: &libadwaita::StyleManager) {
            let scheme = if style.is_dark() { "dark" } else { "light" };
            eprintln!("debug(gtk): style manager changed scheme={scheme}");

            // PORT NOTE (placeholder-leaf): the reference now calls
            // `core_app.colorSchemeEvent(rt, scheme)` and, per surface,
            // `surface.core().colorSchemeCallback(scheme)` (`:1551-1563`), plus the
            // GTK ≥ 4.20 `css_provider "prefers-color-scheme"` push (`:1565-1583`).
            // Both reach the type-erased `core_app`/`rt_app` (Task 8 stores them as
            // `*mut c_void`; the concrete `App<A>`/`A` land with `gtk::App`, Task 11)
            // and the surface list (Task 15+), and the 4.20 property type
            // `gtk::InterfaceColorScheme` is `#[cfg(v4_20)]` (outside the pinned
            // `v4_14` surface). The scheme computation + subscription are wired so
            // the propagation body drops in with those dependencies.
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

        /// PORTS: `application.zig:1411-1429` `startupActionMap`. Registers the
        /// seven application actions verbatim (`:1418-1426`).
        fn startup_action_map(&self) {
            // `glib.ext.VariantType.newFor(u64)` (`:1412`) — the `present-surface`
            // parameter is a `t` (u64); `"as"` (`:1415`) is an array-of-strings for
            // `new-window-command`.
            let t_variant_type = glib::VariantTy::UINT64.to_owned();
            let as_variant_type = glib::VariantTy::STRING_ARRAY.to_owned();

            let obj = self.obj();
            let actions = [
                ext::actions::Action::init("new-window", action_new_window, None),
                ext::actions::Action::init(
                    "new-window-command",
                    action_new_window,
                    Some(as_variant_type),
                ),
                ext::actions::Action::init("open-config", action_open_config, None),
                ext::actions::Action::init(
                    "present-surface",
                    action_present_surface,
                    Some(t_variant_type),
                ),
                ext::actions::Action::init("quit", action_quit, None),
                ext::actions::Action::init("reload-config", action_reload_config, None),
                ext::actions::Action::init(
                    "toggle-quick-terminal",
                    action_toggle_quick_terminal,
                    None,
                ),
            ];

            ext::actions::add(&*obj, &actions);
        }

        /// PORTS: `application.zig:1432-1457` `startupGlobalShortcuts`. Wires the
        /// `GlobalShortcuts` object's dbus connection, the `config` property
        /// binding, and the `trigger` signal.
        fn startup_global_shortcuts(&self) {
            // PORT NOTE (placeholder-leaf): the `GlobalShortcuts` GObject
            // (`priv.global_shortcuts`) is created by Task 27, whose class also
            // defines the `set_dbus_connection`, the `config` bind target, and the
            // `trigger` signal (`:1436-1456`). Task 8's `new` leaves the field
            // `None`, so there is nothing to bind yet; the three wire-ups land with
            // that class. `global_shortcut_trigger` (`:1655-1663`) — the `trigger`
            // handler that calls `core.performAllAction` — likewise defers to the
            // action-dispatch surface (Task 23).
        }

        /// PORTS: `application.zig:1608-1653` `showConfigErrorsDialog`. Presents (or
        /// refreshes) the config-diagnostics dialog when the config has diagnostics.
        fn show_config_errors_dialog(&self) {
            let has_diagnostics = {
                let config = self.config.borrow();
                config
                    .as_ref()
                    .is_some_and(|c| c.property::<bool>("has-diagnostics"))
            };

            // No diagnostics, do nothing (`:1634-1635`).
            if !has_diagnostics {
                return;
            }

            // PORT NOTE (placeholder-leaf): building/refreshing the dialog needs the
            // `ConfigErrorsDialog` class (Task 24) — `.new(priv.config)`,
            // `set_property("config", ...)`, `present`/`close`, and the
            // `reload-config` signal connect (`:1614-1652`), plus the
            // `config_errors_dialog` weak-ref narrowing (Task 8 PORT NOTE). Until
            // then we surface the diagnostics on stderr so a bad config is not
            // silent.
            eprintln!(
                "warn(gtk): configuration has diagnostics; errors dialog lands with the ConfigErrorsDialog class (Task 24)"
            );
        }

        /// PORTS: `application.zig:1145-1200` `syncActionAccelerators` /
        /// `syncActionAccelerator`. Re-derives every menu accelerator from the
        /// current keybind set.
        fn sync_action_accelerators(&self) {
            // PORT NOTE (placeholder-leaf): each `syncActionAccelerator` maps a GTK
            // action name to `config.keybind.set.getTrigger(action)` and installs
            // the accelerator via `gtk_app.setAccelsForAction`
            // (`:1169-1199`). That reaches the `input.Binding.Action`
            // trigger→accelerator surface (`key.accelFromTrigger`), which the
            // action-dispatch tier lands (Task 23); the ~24 concrete action rows
            // (`:1150-1166`) come online with the Window/Surface actions they name.
            // Wired here (called on every `notify::config`) so the body drops in.
        }

        /// PORTS: `application.zig` `loadRuntimeCss`. Regenerates the app-level CSS
        /// (font size, window theme, etc.) into the shared `css_provider`.
        fn load_runtime_css(&self) {
            // PORT NOTE (placeholder-leaf): the runtime CSS generator walks the
            // config into a CSS string and feeds `priv.css_provider.loadFromString`.
            // It depends on the config→CSS surface (adwaita styling tier) that lands
            // with the window/surface CSS work. The provider itself is created +
            // registered on the display in Task 8's `new`; this loader is wired into
            // `prop_config` so it activates when that surface exists.
        }

        /// PORTS: `application.zig` `loadCustomCss`. Loads each user
        /// `custom-css` file into a fresh provider appended to
        /// `custom_css_providers`.
        fn load_custom_css(&self) {
            // PORT NOTE (placeholder-leaf): iterates `config.@"custom-css"`,
            // building one `gtk.CssProvider` per file, registering it on the
            // display, and pushing it onto `priv.custom_css_providers` (the OWNED
            // Vec Task 8 declared, freed in `deinit`). Depends on the config
            // `custom-css` path list surface; wired into `prop_config` so it
            // activates when that lands.
        }

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
            // Cancel any previous timer (`:828`). This makes repeated arming
            // idempotent and leak-free.
            self.stop_quit_timer();

            let config = self.config.borrow();
            let Some(config) = config.as_ref() else {
                return;
            };
            let config = config.get();

            // No-op unless configured to quit after the last window closes
            // (`:831`).
            if !config.quit_after_last_window_closed {
                return;
            }

            // With a delay configured, arm a one-shot timeout that marks the timer
            // expired; without one, treat it as already expired (`:834-845`).
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
                // No delay: treat as expired (`:844`).
                *self.quit_timer.borrow_mut() = QuitTimer::Expired;
            }
        }

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
                        eprintln!(
                            "warn(gtk): unable to remove quit timer source={}",
                            source.as_raw()
                        );
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
}

// PORTS: `application.zig:1507-1524` `handleSigusr2`. The SIGUSR2 handler
// installed by [`imp::Application::startup_signals`]. `g_unix_signal_add` calls
// it on the main loop when SIGUSR2 is delivered; returning `G_SOURCE_CONTINUE`
// keeps the handler installed.
unsafe extern "C" fn handle_sigusr2(_ud: glib::ffi::gpointer) -> glib::ffi::gboolean {
    eprintln!("info(gtk): received SIGUSR2, reloading configuration");
    // PORT NOTE (placeholder-leaf): `Action.reloadConfig(self, .app, .{})`
    // (`:1512-1521`) reaches the action-dispatch surface (`performAction`, Task
    // 23) through the `self` userdata. Until then we log the receipt; the reload
    // wires up with that surface. `G_SOURCE_CONTINUE` (`:1523`).
    glib::ffi::GTRUE
}

// SIGUSR2 on Linux (all glibc targets rustty builds for — aarch64/x86_64) is 12.
// The reference reads `std.posix.SIG.USR2`; gtk-rs/glib-sys expose no signal
// constant and `libc` is not a dependency here, so the value is inlined with this
// citation. (`application.zig:1404` `std.posix.SIG.USR2`.)
const SIGUSR2: core::ffi::c_int = 12;

// `g_unix_signal_add` is a GLib macro over `g_unix_signal_add_full`; the gtk4-rs
// bindings wrap neither, so the real symbol is declared here and resolved at link
// time against the already-linked `libglib-2.0` (glib-sys). Signature per
// `glib-unix.h`. (`application.zig:1403` `glib.unixSignalAdd`.)
extern "C" {
    fn g_unix_signal_add_full(
        priority: core::ffi::c_int,
        signum: core::ffi::c_int,
        function: glib::ffi::GSourceFunc,
        data: glib::ffi::gpointer,
        notify: glib::ffi::GDestroyNotify,
    ) -> core::ffi::c_uint;
}

// PORTS: the seven `app.*` action callbacks (`application.zig:1665-1850`). Each is
// the `gio.SimpleAction::activate` handler `ext::actions` wires with a WEAK ref to
// the target `Application` (see `ext/actions.rs`). Their bodies dispatch through
// the core app / action surface (`core_app.performAction`, `Action.newWindow`,
// `core().mailbox.push`), which reaches the type-erased `core_app`/`core()` and
// the `performAction` dispatch (Task 23) — placeholder-leaves until then, but
// REGISTERED verbatim so the action map is complete (the `startup` KAT asserts all
// seven names).

/// PORTS: `application.zig:1699-1809` `actionNewWindow` (`new-window` /
/// `new-window-command`). Parses the optional `as` argv override, then requests a
/// new window.
fn action_new_window(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    _app: &Application,
) {
    eprintln!("debug(gtk): received new window action");
    // PORT NOTE (placeholder-leaf): argv parsing (`-e`, `--command=`,
    // `--working-directory=`, `--title=`) + `Action.newWindow(...)` (`:1716-1808`)
    // land with the action-dispatch surface (Task 23).
}

/// PORTS: `application.zig:1811-1817` `actionOpenConfig` (`open-config`).
fn action_open_config(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    _app: &Application,
) {
    // PORT NOTE (placeholder-leaf): `self.core().mailbox.push(.open_config, ...)`
    // (`:1816`) reaches the type-erased core surface (Task 11+).
}

/// PORTS: `application.zig:1819-1850` `actionPresentSurface` (`present-surface`,
/// `t`).
fn action_present_surface(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    _app: &Application,
) {
    // PORT NOTE (placeholder-leaf): decode the u64 surface id, `findSurfaceByID`,
    // and push `present_surface` (`:1824-1849`) — needs the core surface surface.
}

/// PORTS: `application.zig:1687-1696` `actionQuit` (`quit`).
fn action_quit(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    _app: &Application,
) {
    // PORT NOTE (placeholder-leaf): `core_app.performAction(rt, .quit)` (`:1693`).
}

/// PORTS: `application.zig:1665-1674` `actionReloadConfig` (`reload-config`).
fn action_reload_config(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    _app: &Application,
) {
    // PORT NOTE (placeholder-leaf): `core_app.performAction(rt, .reload_config)`
    // (`:1671`).
}

/// PORTS: `application.zig:1676-1685` `actionToggleQuickTerminal`
/// (`toggle-quick-terminal`).
fn action_toggle_quick_terminal(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    _app: &Application,
) {
    // PORT NOTE (placeholder-leaf): `core_app.performAction(rt,
    // .toggle_quick_terminal)` (`:1682`).
}

/// PORTS: `application.zig:435-459` `deinit` (the field-clearing body). Shared by
/// [`Application::deinit`] (the run loop's explicit force-teardown) and
/// [`imp::Application`]'s `Drop` (the GObject finalize hook). Idempotent: each
/// clear is a no-op once the field is already cleared.
fn deinit_private(priv_: &imp::Application) {
    // `priv.config.unref()` (`application.zig:438`) — clearing drops the ref.
    priv_.config.replace(None);

    // `priv.winproto.deinit()` (`application.zig:439`).
    priv_.winproto.borrow_mut().deinit();

    // PORT NOTE: `priv.open_uri.deinit()` (`:440`) and
    // `priv.global_shortcuts.unref()` (`:441`) free the portal `OpenURI` and the
    // `GlobalShortcuts` GObject; neither is created yet (their types land in later
    // tasks), so there is nothing to free. `global_shortcuts` is cleared
    // defensively (a no-op while `None`).
    priv_.global_shortcuts.replace(None);

    // Drop the queued-window / tick / confirm-quit thunks (rustty-only seams) so
    // they release their captured `core_app`/`rt_app` pointers; the pointers are
    // BACKREFs (never owned here).
    priv_.new_window_thunk.replace(None);
    priv_.tick_thunk.replace(None);
    priv_.needs_confirm_quit_thunk.replace(None);

    // `if (priv.saved_language) |language| alloc.free(language)` (`:442`) —
    // clearing drops the `CString`.
    priv_.saved_language.replace(None);

    // Remove the css provider and each custom provider from the display
    // (`application.zig:443-455`).
    if let Some(display) = gdk4::Display::default() {
        if let Some(provider) = priv_.css_provider.borrow().as_ref() {
            gtk4::style_context_remove_provider_for_display(&display, provider);
        }
        for provider in priv_.custom_css_providers.borrow().iter() {
            gtk4::style_context_remove_provider_for_display(&display, provider);
        }
    }

    // `priv.css_provider.unref()` (`:456`).
    priv_.css_provider.replace(None);

    // Each custom provider `unref` + the list `deinit` (`:457-458`) — clearing the
    // Vec drops (unrefs) each element.
    priv_.custom_css_providers.borrow_mut().clear();
}

/// PORTS: `application.zig:48-110` `glibLogWriterFunction`. Funnels
/// GLib/GObject/GTK log messages through rustty's logging — here `eprintln!`,
/// matching the `gtk_version`/`adw_version` `log_version` convention (no logging
/// framework is wired in this tier) — rather than letting GLib dump them straight
/// to stderr unstructured. Registered once via [`glib::log_set_writer_func`] in
/// [`Application::new`] (`application.zig:251`).
///
/// The reference scans the structured-log fields for `MESSAGE` / `GLIB_DOMAIN`
/// and dispatches on the level flags (`application.zig:60-109`); gtk4-rs hands us
/// a decoded [`glib::LogLevel`] and typed [`glib::LogField`]s, so both port
/// directly.
fn glib_log_writer(level: glib::LogLevel, fields: &[glib::LogField<'_>]) -> glib::LogWriterOutput {
    let mut message: Option<&str> = None;
    let mut domain: Option<&str> = None;
    for field in fields {
        let Some(value) = field.value_str() else {
            continue;
        };
        match field.key() {
            // `if (std.mem.eql(u8, k, "MESSAGE"))` (`application.zig:71`).
            "MESSAGE" => message = Some(value),
            // `if (std.mem.eql(u8, k, "GLIB_DOMAIN"))` (`application.zig:75`).
            "GLIB_DOMAIN" => domain = Some(value),
            _ => continue,
        }
    }

    // `const message = message_ orelse return .unhandled;` (`application.zig:81`).
    let Some(message) = message else {
        return glib::LogWriterOutput::Unhandled;
    };
    // `const domain = domain_ orelse "«unknown»";` (`application.zig:82`).
    let domain = domain.unwrap_or("«unknown»");

    // The level dispatch (`application.zig:84-109`). gtk4-rs decodes the level
    // flags to the highest-severity `LogLevel`, so a single `match` reproduces
    // the reference's ordered flag checks.
    match level {
        // error + critical -> err (`application.zig:84-91`).
        glib::LogLevel::Error => eprintln!("error(glib): ERROR: {domain}: {message}"),
        glib::LogLevel::Critical => eprintln!("error(glib): CRITICAL: {domain}: {message}"),
        // warning -> warn (`application.zig:92-95`).
        glib::LogLevel::Warning => eprintln!("warn(glib): WARNING: {domain}: {message}"),
        // message + info -> info (`application.zig:96-103`).
        glib::LogLevel::Message => eprintln!("info(glib): MESSAGE: {domain}: {message}"),
        glib::LogLevel::Info => eprintln!("info(glib): INFO: {domain}: {message}"),
        // debug -> debug (`application.zig:104-107`).
        glib::LogLevel::Debug => eprintln!("debug(glib): DEBUG: {domain}: {message}"),
    }
    glib::LogWriterOutput::Handled
}

/// PORTS: `application.zig:2849-2951` `setGtkEnv`. Sets the `GDK_DEBUG` /
/// `GDK_DISABLE` environment variables BEFORE GTK initializes, based on the
/// runtime GTK version, so Ghostty gets a GL (never GLES/Vulkan) GDK backend.
/// Must run before `adw::init()` (`application.zig:2850` asserts GTK is not yet
/// initialized).
///
/// PORT NOTE: the reference builds the comma-joined value into a fixed
/// `[1024]u8` and can return `error.NoSpaceLeft` (`application.zig:2917,2928`);
/// rustty joins into a growable `String`, so it cannot overflow and is
/// infallible — the reference's only error arm (`application.zig:296-305`) is
/// therefore unreachable and folds away.
fn set_gtk_env(config: &CoreConfig) {
    use crate::gtk::gtk_version;

    // `assert(gtk.isInitialized() == 0);` (`application.zig:2850`).
    debug_assert!(
        !gtk4::is_initialized(),
        "setGtkEnv must run before GTK is initialized"
    );

    // GDK_DEBUG flags (`application.zig:2852-2867`). `.opengl` requires BOTH
    // stderr logging and `gtk-opengl-debug`, since the flag dumps to stderr
    // (`application.zig:2864-2866`). `state.logging.stderr` reads the process
    // global state; unset (e.g. in a test) reads as `false`.
    let stderr_logging = rustty_core::STATE.get().is_some_and(|s| s.logging.stderr);
    let gdk_debug_opengl = stderr_logging && config.gtk_opengl_debug;
    let mut gdk_debug_gl_disable_gles = false;
    let mut gdk_debug_gl_no_fractional = false;
    let mut gdk_debug_vulkan_disable = false;

    // GDK_DISABLE flags (`application.zig:2869-2879`). `color-mgmt` defaults ON —
    // current GTK color management is not good enough (`application.zig:2871-2874`).
    let mut gdk_disable_gles_api = false;
    let mut gdk_disable_color_mgmt = true;
    let mut gdk_disable_vulkan = false;

    // Version-gated environment block (`application.zig:2881-2914`). Every gate is
    // a LIVE runtime predicate (Cargo.toml AD-3 rule: rustty has no comptime
    // version fold; `runtimeAtLeast`/`runtimeUntil` are unconditionally live).
    'environment: {
        // `if (runtimeAtLeast(4, 18, 0)) color-mgmt = false;` (`:2882-2884`).
        if gtk_version::runtime_at_least(4, 18, 0) {
            gdk_disable_color_mgmt = false;
        }

        // From GTK 4.16, GDK_DEBUG split into GDK_DEBUG + GDK_DISABLE (`:2886-2892`).
        if gtk_version::runtime_at_least(4, 16, 0) {
            gdk_disable_gles_api = true;
            gdk_disable_vulkan = true;
            break 'environment;
        }
        // GTK 4.14+: export GDK_DEBUG to run on Wayland (`:2893-2908`).
        if gtk_version::runtime_at_least(4, 14, 0) {
            gdk_debug_gl_disable_gles = true;
            gdk_debug_vulkan_disable = true;
            // Removed at GTK v4.17.5 (`application.zig:2903-2906`).
            if gtk_version::runtime_until(4, 17, 5) {
                gdk_debug_gl_no_fractional = true;
            }
            break 'environment;
        }
        // Versions prior to 4.14 (`application.zig:2910-2913`).
        gdk_debug_vulkan_disable = true;
    }

    // Build + set GDK_DEBUG (`application.zig:2916-2932`). Field order matches the
    // reference struct's field order.
    {
        let mut parts: Vec<&str> = Vec::new();
        if gdk_debug_opengl {
            parts.push("opengl");
        }
        if gdk_debug_gl_disable_gles {
            parts.push("gl-disable-gles");
        }
        if gdk_debug_gl_no_fractional {
            parts.push("gl-no-fractional");
        }
        if gdk_debug_vulkan_disable {
            parts.push("vulkan-disable");
        }
        let value = parts.join(",");
        eprintln!("warn(gtk): setting GDK_DEBUG={value}");
        if let Ok(v) = CString::new(value) {
            rustty_os::env::setenv(c"GDK_DEBUG", &v);
        }
    }

    // Build + set GDK_DISABLE (`application.zig:2934-2950`).
    {
        let mut parts: Vec<&str> = Vec::new();
        if gdk_disable_gles_api {
            parts.push("gles-api");
        }
        if gdk_disable_color_mgmt {
            parts.push("color-mgmt");
        }
        if gdk_disable_vulkan {
            parts.push("vulkan");
        }
        let value = parts.join(",");
        eprintln!("warn(gtk): setting GDK_DISABLE={value}");
        if let Ok(v) = CString::new(value) {
            rustty_os::env::setenv(c"GDK_DISABLE", &v);
        }
    }
}

/// PORTS: the three error arms of `application.zig:477` `run` (`!void`): the
/// `error.ContextAcquireFailed` (`:483`), `error.ApplicationRegisterFailed`
/// (`:514`), and the propagated `core_app.tick` error (`:552`, `try`). The Zig
/// error set is anonymous; the port names it so [`Application::run`] has a
/// concrete return type.
#[derive(Debug)]
pub enum RunError {
    /// `glib.MainContext.acquire(ctx) == 0` (`application.zig:483`) — another
    /// thread already owns the default main context.
    ContextAcquireFailed,
    /// `self.as(gio.Application).register(...) == 0` (`application.zig:502-514`) —
    /// GApplication registration failed (the warning is logged at the call site).
    ApplicationRegisterFailed,
    /// The inline `core_app.tick(rt_app)` returned an error (`application.zig:552`,
    /// `try`).
    Tick(rustty_core::app::Error),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::ContextAcquireFailed => f.write_str("failed to acquire the main context"),
            RunError::ApplicationRegisterFailed => {
                f.write_str("failed to register the application")
            }
            RunError::Tick(err) => write!(f, "core app tick failed: {err}"),
        }
    }
}

impl std::error::Error for RunError {}

/// The `defer`-as-`Drop` guard for [`Application::run`]'s always-runs cleanup
/// (`application.zig:486-498`). Its `Drop` runs the reference's four deferred
/// steps in order — stop the quit timer, sync settings, drain the loop
/// non-blocking — and then the paired `MainContextAcquireGuard` (declared before
/// this guard in `run`, so it drops AFTER) releases the context. This split is
/// the faithful port of the nested `defer glib.MainContext.release(ctx)`
/// (`:497`), which the reference runs last.
struct RunCleanup<'a> {
    app: &'a Application,
    /// A second handle to the SAME default context (glib contexts are refcounted
    /// singletons) so the drain can iterate without borrowing `run`'s acquire
    /// guard — Rust cannot hold both the context and a guard borrowing it in one
    /// struct.
    ctx: glib::MainContext,
}

impl Drop for RunCleanup<'_> {
    fn drop(&mut self) {
        // Ensure our timer source is removed (`application.zig:488`).
        self.app.imp().stop_quit_timer();

        // Sync any remaining settings (`application.zig:491`).
        gio::Settings::sync();

        // Clear out the event loop, don't block (`application.zig:494`).
        while self.ctx.iteration(false) {}

        // The context release (`application.zig:497`) is the paired
        // `MainContextAcquireGuard`'s `Drop`, which runs after this guard's — see
        // the struct doc.
    }
}

/// PORTS: `application.zig:2953-2960` `findActiveWindow`. Finds the active
/// toplevel window (the one to parent a dialog on). The reference is a
/// `gtk.Window.listToplevels().findCustom` compare function returning 0 on the
/// active window; the behavioral port iterates the toplevels and returns the
/// first active `gtk.Window` (`window.isActive() != 0`).
fn find_active_window() -> Option<gtk4::Window> {
    use gtk4::prelude::GtkWindowExt as _;
    gtk4::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk4::Window>().ok())
        // Confusingly `isActive() == 1` means active; the reference abuses that as
        // the `findCustom` equality (`application.zig:2956-2959`).
        .find(|w| w.is_active())
}

glib::wrapper! {
    /// The `GhosttyApplication` — the application-wide GObject, an
    /// [`libadwaita::Application`] subclass. PORTS: `application.zig:112-140`
    /// `pub const Application`. Process-global singleton: reach it anywhere via
    /// [`Application::default`].
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends libadwaita::Application, gtk4::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Application {
    /// PORTS: `application.zig:230-233` `default`. Returns this application as the
    /// process-global default, giving access to its properties anywhere. Asserts
    /// (like the reference's `gio.Application.getDefault().?` +
    /// `gobject.ext.cast(Self, app).?`) that a default `GApplication` exists AND
    /// that it is a `GhosttyApplication` — the program would be in a very bad state
    /// otherwise.
    ///
    /// This is NOT [`Default::default`]: it fetches the one installed singleton and
    /// panics if there is none, rather than constructing a fresh instance. The name
    /// mirrors the oracle's `Application.default()` 1:1.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn default() -> Self {
        // `gio.Application.getDefault().?` (`application.zig:231`).
        let app = gio::Application::default()
            .expect("no default GApplication is set (Application::default called too early)");
        // `gobject.ext.cast(Self, app).?` (`application.zig:232`).
        app.downcast::<Self>()
            .expect("the default GApplication is not a GhosttyApplication")
    }

    /// PORTS: `application.zig:464-466` `allocator`. The ONE canonical allocator all
    /// other classes must use (invariant 2, see the module doc). In rustty the
    /// allocator seam is the workspace global allocator, so this returns
    /// [`allocator_api2::alloc::Global`] — a zero-sized handle to it. The reference
    /// returns `core_app.alloc`; here every tier allocation goes through this same
    /// `Global`, keeping the alloc-side ↔ free-side symmetry intact.
    #[must_use]
    pub fn allocator(&self) -> allocator_api2::alloc::Global {
        allocator_api2::alloc::Global
    }

    /// PORTS: `application.zig:1208-1210` `getConfig`. Returns a fresh strong ref to
    /// the shared [`Config`] (the reference increases the refcount, `.full`). `None`
    /// before [`Application::new`] has set the config.
    #[must_use]
    pub fn get_config(&self) -> Option<Config> {
        self.property("config")
    }

    /// PORTS: `application.zig` `savedLanguage` — the `LANG` value captured at
    /// startup, applied to child-process environments by
    /// [`crate::gtk::class::surface`]'s `defaultTermioEnv` (`surface.zig:1598`).
    ///
    /// **Placeholder-leaf (language-override tier):** the startup `LANG`
    /// capture/override lands with that work; until then this reports `None`, so
    /// `defaultTermioEnv` removes `LANG` (the reference's `else` branch) rather
    /// than pinning a stale one.
    #[must_use]
    pub fn saved_language(&self) -> Option<String> {
        None
    }

    /// Test-only: build **and register** a default `GhosttyApplication` so widget
    /// tests in other class modules (e.g. `class::surface`) can reach
    /// [`Application::default`] for `init`'s config / allocator fallback. Mirrors
    /// the tests' own `build_test_app` but registers (running `startup`, which
    /// installs us as the default `GApplication`, `application.zig:1303`).
    /// `NON_UNIQUE` keeps registration off the session bus. Must run after
    /// `gtk4::init()`.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn build_default_for_test() -> Self {
        use gio::prelude::ApplicationExt as _;
        let app: Application = glib::Object::builder()
            .property("application-id", "io.airely.rustty.SurfaceTest")
            .property("flags", gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let imp = app.imp();
        imp.config
            .replace(Some(Config::new(&CoreConfig::default_config())));
        imp.core_app.set(NonNull::new(std::ptr::null_mut()));
        app.register(gio::Cancellable::NONE)
            .expect("register default test application");
        app
    }

    // ---- INHABITED placeholder leaves (placeholder-leaf doctrine) ----
    //
    // The following are the class's heavyweight members. They are present so the
    // call-graph shape exists, but their bodies land in later tasks. Each panics if
    // reached before then.

    /// PORTS: `application.zig:243-428` `new`. Creates a new `Application` — the
    /// main program entrypoint's heavy lifting: capture the GLib log writer, load
    /// the configuration (with the degraded-default fallback), capture/override
    /// `LANG`, init i18n + the GTK env vars + libadwaita, select the app id and
    /// application flags, probe the windowing protocol, add the CSS provider, and
    /// construct the instance with the pinned resource base path.
    ///
    /// PORT NOTE (signature): the reference is `new(rt_app: *ApprtApp, core_app:
    /// *CoreApp) Allocator.Error!*Self`. The two backref pointers stay `*mut
    /// c_void` here — `ApprtApp`/`CoreApp` cannot be named until the apprt `App`
    /// lands (Task 11), which narrows the pointee then. They are BACKREFs
    /// (PREPORT_GTK §1.1: "never freed here") that neither `new` nor `deinit`
    /// dereferences — the allocator seam is [`allocator_api2::alloc::Global`], not
    /// `core_app.alloc` — so storing them opaquely is faithful. The return is
    /// `Self`, not `Result`: the reference's ONLY failure mode is early OOM
    /// ("can't be recovered from", `application.zig:240-241`), which in Rust
    /// aborts in the global allocator rather than surfacing an `AllocError`, so
    /// the port is infallible.
    #[must_use]
    pub fn new(rt_app: *mut c_void, core_app: *mut c_void) -> Self {
        // The reference binds `alloc = core_app.alloc` (`application.zig:247`); in
        // rustty the allocator seam is the global allocator (invariant 2), so
        // there is no per-core allocator to bind here.

        // Capture GLib/GObject/GTK log messages (`application.zig:249-251`).
        // gtk4-rs's `log_set_writer_func` panics if set more than once, so guard
        // with a `Once` — repeated `Application::new` (e.g. across tests) must not
        // abort.
        {
            use std::sync::Once;
            static WRITER: Once = Once::new();
            WRITER.call_once(|| glib::log_set_writer_func(glib_log_writer));
        }

        // Log our GTK / libadwaita versions (`application.zig:254-255`).
        crate::gtk::gtk_version::log_version();
        crate::gtk::adw_version::log_version();

        // Load our configuration, with the degraded-default fallback that records
        // the load error as a diagnostic so it can be shown to the user; the
        // default only fails for OOM (which aborts), so we can always start
        // (`application.zig:258-271`).
        let config: CoreConfig = match CoreConfig::load() {
            Ok(config) => config,
            Err(err) => {
                let mut def = CoreConfig::default_config();
                def.add_diagnostic_fmt(
                    format!("error loading user configuration: {err:?}").into_bytes(),
                );
                def
            }
        };

        // Capture the launch-time LANG, then override it from `config.language`
        // (`application.zig:274-284`). The capture happens BEFORE the override.
        let saved_language: Option<CString> = {
            let old_language = match rustty_os::env::getenv("LANG") {
                Ok(Some(result)) => {
                    let dup = CString::new(result.value.clone()).ok();
                    result.deinit();
                    dup
                }
                _ => None,
            };
            if let Some(language) = &config.language {
                if let Ok(lang) = CString::new(language.clone()) {
                    rustty_os::env::setenv(c"LANG", &lang);
                }
            }
            old_language
        };

        // Set the gettext global domain; warn-and-continue on failure — a failure
        // shouldn't stop startup, translations just may not resolve
        // (`application.zig:288-293`).
        if let Err(err) = rustty_os::i18n::initGlobalDomain() {
            eprintln!("warn(gtk): i18n initialization failed error={err:?}");
        }

        // Setup our GTK init env vars (`application.zig:296-305`), then libadwaita
        // (`application.zig:306`). `set_gtk_env` is infallible in rustty (see its
        // doc), so the reference's `error.NoSpaceLeft` warn arm folds away.
        set_gtk_env(&config);
        let _ = libadwaita::init();

        // single-instance -> application flags (`application.zig:308-320`).
        let single_instance = match config.gtk_single_instance {
            rustty_config::value_types::GtkSingleInstance::True => true,
            rustty_config::value_types::GtkSingleInstance::False => false,
            // Resolved to true/false during config finalize (`application.zig:311-312`).
            rustty_config::value_types::GtkSingleInstance::Detect => unreachable!(
                "gtk-single-instance must be resolved to true/false during config load"
            ),
        };
        let mut app_flags = gio::ApplicationFlags::empty();
        if !single_instance {
            app_flags |= gio::ApplicationFlags::NON_UNIQUE;
        }

        // Our app id: `config.class` if a valid GApplication id, else the
        // build-info default (`application.zig:325-335`).
        let app_id: String = 'app_id: {
            if let Some(class) = &config.class {
                if let Ok(class_str) = std::str::from_utf8(class) {
                    if gio::Application::id_is_valid(class_str) {
                        break 'app_id class_str.to_owned();
                    }
                }
                eprintln!("warn(gtk): invalid 'class' in config, ignoring");
            }
            build_info::APPLICATION_ID.to_owned()
        };

        // The GDK display, or exit — there is no scenario we expect this to be
        // null, and we don't want null checks everywhere (`application.zig:337-342`).
        let Some(display) = gdk4::Display::default() else {
            eprintln!("warn(gtk): gdk display is null, exiting");
            std::process::exit(1);
        };

        // Windowing-protocol logic, with the fallback to `.none` on error so we
        // can still launch (`application.zig:345-358`).
        let winproto = match winproto::App::init(&display, &app_id, &config) {
            Ok(wp) => wp,
            Err(err) => {
                eprintln!("warn(gtk): error initializing windowing protocol err={err}");
                winproto::App::default()
            }
        };
        eprintln!("debug(gtk): windowing protocol={:?}", winproto.protocol());

        eprintln!(
            "debug(gtk): creating GTK application id={app_id} single-instance={single_instance}"
        );

        // Wrap the configuration in a GObject (`application.zig:366-368`). This
        // clones the config, so the local `config` may drop at end of scope.
        let config_obj = Config::new(&config);

        // The CSS provider for config-derived styles. GTK ensures only one
        // instance of a given provider exists per display (`application.zig:371-378`).
        let css_provider = gtk4::CssProvider::new();
        gtk4::style_context_add_provider_for_display(
            &display,
            &css_provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 3,
        );

        // Construct the app instance (`application.zig:381-389`). Force the
        // resource base path to a known value ([`build_info::RESOURCE_PATH`]) so
        // it does not depend on the app id (which changes debug/release and can be
        // user-configured) and forces loading the compiled resources
        // (`application.zig:385-387`).
        let app: Self = glib::Object::builder()
            .property("application-id", &app_id)
            .property("flags", app_flags)
            .property("resource-base-path", build_info::RESOURCE_PATH)
            .build();

        // Fill the private state (`application.zig:394-405`). More setup happens in
        // the GObject-called init callbacks, but this is data we can't thread
        // through them.
        let priv_ = app.imp();
        priv_.rt_app.set(NonNull::new(rt_app));
        priv_.core_app.set(NonNull::new(core_app));
        priv_.config.replace(Some(config_obj));
        priv_.winproto.replace(winproto);
        priv_.css_provider.replace(Some(css_provider.clone()));
        // `custom_css_providers` starts empty (`application.zig:401`).
        priv_.saved_language.replace(saved_language);
        // PORT NOTE: the reference also fills `global_shortcuts`
        // (`newInstance(GlobalShortcuts)`, `application.zig:402`) and `open_uri`
        // (`.init(rt_app)`, `application.zig:404`). Both depend on classes that
        // land in later tasks (`GlobalShortcuts` = Task 27; portal `OpenURI` =
        // later): the skeleton `global_shortcuts` field stays `None` (Task 9's
        // `startupGlobalShortcuts` creates it), and there is no `open_uri` field
        // yet. Their frees in `deinit` are guarded accordingly.

        // Signals (`application.zig:408-422`). The `notify::config` handler
        // (re)applies config; the CssProvider `parsing-error` handler logs parse
        // errors. Both handler bodies are Task 9's (placeholder-leaf now).
        app.connect_notify_local(Some("config"), |app: &Application, _pspec| {
            app.imp().prop_config(app);
        });
        css_provider.connect_parsing_error(glib::clone!(
            #[weak]
            app,
            move |_provider, section, error| {
                app.imp().signal_css_parsing_error(section, error);
            }
        ));

        // Trigger initial config changes (`application.zig:425`).
        app.notify("config");

        app
    }

    /// PORTS: `application.zig:435-459` `deinit`. Force-deinitialize the
    /// application. Normally a GObject frees in its finalizer, but the application
    /// is never fully unreferenced (`application.zig:430-434`), so this frees every
    /// OWNED §1.1 row explicitly, in the reference's order.
    ///
    /// PORT NOTE (signature): `&self` (the reference is `*Self`) — the OWNED rows
    /// live behind the `imp`'s `RefCell`/`Cell`s, so clearing a field drops
    /// (unrefs / frees) its contents through `Drop` (Invariant 12 for the manually
    /// held providers).
    pub fn deinit(&self) {
        deinit_private(self.imp());
    }

    /// Installs the monomorphized core-app bindings for the concrete apprt `App`
    /// type `A` — the Rust seam that supplies what Zig gets from comptime
    /// single-backend monomorphization (`priv.core_app: *CoreApp`,
    /// `application.zig:170`).
    ///
    /// The `Application` GObject is non-generic (GObject subclasses cannot be
    /// generic), and its stored `core_app` is a type-erased `*mut c_void` (Task 8),
    /// so the non-generic `activate` vfunc cannot name the concrete
    /// `rustty_core::app::App<A>` the pointer refers to. This method — called once
    /// the backend type is known (the run loop / `gtk::App`, Tasks 10/11; and the
    /// T9 activate test) — casts `core_app` back to `*mut App<A>` and captures it
    /// in the [`imp::Application::new_window_thunk`], so `activate` can push
    /// `Message::NewWindow` faithfully (`application.zig:1464-1466`).
    ///
    /// # Safety-adjacent contract
    ///
    /// `A` MUST be the same apprt `App` type whose `App<A>` was passed to
    /// [`Application::new`] as `core_app`; the stored pointer is reinterpreted as
    /// `*mut App<A>`. This mirrors the reference's single comptime backend: there
    /// is exactly one `CoreApp` type per build. No-op when `core_app` is null.
    pub fn bind_core_app<A>(&self)
    where
        A: rustty_apprt_api::App + 'static,
        // The core `App<A>` self-referential surface bound (mirrors
        // `rustty_core::app::App`'s own `impl` bound) — required to name
        // `App<A>::tick` / `::needsConfirmQuit` in the thunks below.
        A::Surface: rustty_apprt_api::Surface<App = A, Core = rustty_core::surface::Surface<A>>,
    {
        let Some(core_app) = self.imp().core_app.get() else {
            return;
        };
        let core_app = core_app.as_ptr() as *mut rustty_core::app::App<A>;
        self.imp().new_window_thunk.replace(Some(Box::new(move || {
            // `priv.core_app.mailbox.push(.{ .new_window = {} }, .forever)`
            // (`application.zig:1464-1466`). The `core_app` BACKREF is owned by
            // libghostty-core (never freed through this thunk); the app thread
            // validates surface pointers before use (core's threading contract).
            let _ = unsafe { &*core_app }.mailbox.push(
                rustty_core::app::Message::NewWindow(rustty_core::app::NewWindow::default()),
                rustty_datastruct::blocking_queue::Timeout::Forever,
            );
        })));

        // The inline core tick (`try priv.core_app.tick(priv.rt_app)`,
        // `application.zig:552`). `tick` takes `&mut App<A>` and `rt_app: &A`; the
        // apprt `App` (`= A`) is the type-erased `rt_app` backref. Both pointers are
        // BACKREFs owned by libghostty-core / the apprt `App`, never freed here.
        let rt_app = self.imp().rt_app.get().map(|p| p.as_ptr() as *mut A);
        self.imp().tick_thunk.replace(Some(Box::new(move || {
            match rt_app {
                // SAFETY: `core_app`/`rt_app` are the stable heap pointers the caller
                // passed to `new`; the run loop drives this on the app thread only.
                Some(rt_app) => unsafe { &mut *core_app }.tick(unsafe { &*rt_app }),
                None => Ok(()),
            }
        })));

        // The confirm-quit probe (`priv.core_app.needsConfirmQuit()`,
        // `application.zig:604`) that `quit` consults before quitting.
        self.imp()
            .needs_confirm_quit_thunk
            .replace(Some(Box::new(move || {
                // SAFETY: as above — app-thread-only access to the core app backref.
                unsafe { &*core_app }.needsConfirmQuit()
            })));
    }

    /// Binds the `rt_app` BACKREF — the apprt `App`'s stable address — into the
    /// singleton, the DEFERRED half of [`Application::new`] (which stores it null,
    /// see [`crate::gtk::App::init`]'s PORT NOTE). The reference passes `self` (the
    /// App's already-stable address) as `rt_app` at construction (`App.zig:43`); in
    /// rustty the apprt `App` is returned by value from `init`, so its final address
    /// is not known until the caller places it — the run leg ([`crate::gtk::App::run`])
    /// binds it here at that stable-address site, exactly as the `activate`/run-loop
    /// tests drive `rt_app.set` manually.
    ///
    /// Must run BEFORE [`Application::bind_core_app`], which captures the `rt_app`
    /// pointer into the tick / confirm-quit thunks. No-op semantics: a null `rt_app`
    /// clears the cell.
    pub fn bind_rt_app(&self, rt_app: *mut c_void) {
        self.imp().rt_app.set(NonNull::new(rt_app));
    }

    /// PORTS: the active windowing-protocol tag the reference reads via
    /// `@tagName(priv.winproto)` when it logs the selected protocol
    /// (`application.zig:358`). Reports which [`winproto`] backend
    /// [`Application::new`] selected; a noop-only build always reports
    /// [`winproto::Protocol::None`].
    #[must_use]
    pub fn winproto_protocol(&self) -> winproto::Protocol {
        self.imp().winproto.borrow().protocol()
    }

    /// PORTS: `application.zig:468-472` `winproto` — the app-level windowing
    /// protocol accessor the reference returns as `*winprotopkg.App`. rustty holds
    /// it in a [`RefCell`]; the shared-borrow view is what
    /// [`crate::gtk::class::window::Window`]'s `windowRealize` (`window.zig:1305-1309`)
    /// threads into [`winproto::Window::init`].
    ///
    /// [`RefCell`]: std::cell::RefCell
    #[must_use]
    pub fn winproto(&self) -> std::cell::Ref<'_, winproto::App> {
        self.imp().winproto.borrow()
    }

    /// The mutable counterpart of [`Application::winproto`] — the reference's
    /// `*winprotopkg.App` where the callee mutates (e.g. `initQuickTerminal`,
    /// `window.zig:1204`).
    #[must_use]
    pub fn winproto_mut(&self) -> std::cell::RefMut<'_, winproto::App> {
        self.imp().winproto.borrow_mut()
    }

    /// PORTS: `application.zig:474-592` `run`. A hand-rolled replacement for
    /// `gio.Application.run` (**AD-7**, the plan's defining structural choice) so
    /// the event loop can be interleaved with libghostty's core: after every
    /// blocking `MainContext::iteration(true)` the loop inlines
    /// `core_app.tick(rt_app)`. This is the libghostty→GTK bridge's consuming half
    /// — the producing half is [`Application::wakeup`], which the core's
    /// renderer/io threads call to break the blocking iteration so the very next
    /// tick runs promptly (see that method).
    ///
    /// PORT NOTE: **not** `ApplicationExtManual::run` — the whole point is the
    /// inline tick. The reference is `!void`; the port returns [`RunError`], whose
    /// three arms are the reference's `error.ContextAcquireFailed`,
    /// `error.ApplicationRegisterFailed`, and the propagated `core_app.tick` error.
    pub fn run(&self) -> Result<(), RunError> {
        use gio::prelude::ApplicationExt as _;
        use gtk4::prelude::GtkApplicationExt as _;

        let imp = self.imp();

        // Acquire the default context for the application (`:482-483`).
        let ctx = glib::MainContext::default();
        let _acquire = ctx.acquire().map_err(|_| RunError::ContextAcquireFailed)?;

        // The final cleanup that is always required at the end of running — the
        // reference's `defer` block (`:486-498`): stop the quit timer, sync
        // settings, drain the loop (non-blocking), then release the context. In
        // Rust the `defer` is a guard whose `Drop` runs those steps; declaring it
        // AFTER `_acquire` makes it drop FIRST (stop/sync/drain), then `_acquire`
        // drops (release) — the reference's ordering exactly. Declared before
        // `register` so it also covers the register-failure early return.
        let _cleanup = RunCleanup {
            app: self,
            ctx: glib::MainContext::default(),
        };

        // Register the application (`:500-516`). A failure warns and bails; the
        // `_cleanup` guard still runs on the way out.
        self.register(gio::Cancellable::NONE).map_err(|err| {
            eprintln!(
                "warn(gtk): error registering application: {}",
                err.message()
            );
            RunError::ApplicationRegisterFailed
        })?;

        // Call `activate` as part of the normal startup routine, but only if the
        // config allows it (background launch without an initial window,
        // `:518-530`). Scope the config borrow: once the event loop runs it can
        // change out from underneath us (`:526-527`, load-bearing).
        {
            let initial_window = {
                let config = imp.config.borrow();
                config.as_ref().is_some_and(|c| c.get().initial_window)
            };
            if initial_window {
                self.activate();
            }
        }

        // If we are NOT the primary instance, never run the loop — another GTK
        // instance is already running (`:532-540`).
        if self.is_remote() {
            eprintln!("debug(gtk): application is remote, exiting run loop after activation");
            return Ok(());
        }

        // Tell systemd that we are ready (`:543`).
        rustty_os::systemd::notify::ready();

        eprintln!("debug(gtk): entering runloop");
        imp.running.set(true);
        while imp.running.get() {
            // Block until a source is ready OR `wakeup()` breaks us out (`:549`).
            let _ = ctx.iteration(true);

            // Tick the core Rustty terminal app — the inline bridge half
            // (`try priv.core_app.tick(priv.rt_app)`, `:551-552`). Monomorphized
            // into [`imp::Application::tick_thunk`] by [`Application::bind_core_app`]
            // (the non-generic vfunc cannot name `App<A>`); an unbound thunk is a
            // no-op `Ok(())`.
            {
                let result = {
                    let thunk = imp.tick_thunk.borrow();
                    match thunk.as_ref() {
                        Some(tick) => tick(),
                        None => Ok(()),
                    }
                };
                #[cfg(test)]
                imp.tick_count.set(imp.tick_count.get() + 1);
                result.map_err(RunError::Tick)?;
            }

            // Check if we must quit based on the current state (`:554-581`). The
            // predicate ports verbatim; `must_quit` scenarios never need
            // confirmation and may leave app/surface memory unsafe to touch, so we
            // begin our exit immediately via `quitNow` (`:583-590`).
            let must_quit = 'q: {
                let config = imp.config.borrow();
                let Some(config) = config.as_ref() else {
                    break 'q false;
                };
                let config = config.get();

                // Configured to always stay running → don't quit (`:557-558`).
                if !config.quit_after_last_window_closed {
                    break 'q false;
                }

                // Quit timer expired → quit (`:560-564`).
                if matches!(*imp.quit_timer.borrow(), imp::QuitTimer::Expired) {
                    eprintln!("debug(gtk): must_quit due to quit timer expired");
                    break 'q true;
                }

                // No windows attached → quit, but only when no closed-delay is set
                // (with a delay we exit via the timer instead) (`:566-577`).
                if config.quit_after_last_window_closed_delay.is_none()
                    && imp.requested_window.get()
                    && self.windows().is_empty()
                {
                    eprintln!("debug(gtk): must_quit due to no app windows");
                    break 'q true;
                }

                break 'q false;
            };

            if must_quit {
                self.quit_now();
            }
        }
        eprintln!("debug(gtk): exiting runloop");

        Ok(())
    }

    /// PORTS: `application.zig:1286-1289` `wakeup`. **The whole libghostty→GTK
    /// bridge**: the core's renderer/io threads call `rt_app.wakeup()` (→ this),
    /// which breaks the blocking `MainContext::iteration(true)` in [`Application::run`]
    /// so the very next statement — the inline `core_app.tick(rt_app)` — runs
    /// promptly. `glib.MainContext.wakeup(null)` targets the default context (the
    /// one `run` acquires); the gtk4-rs analogue is
    /// `MainContext::default().wakeup()`. Safe to call from any thread.
    pub fn wakeup(&self) {
        glib::MainContext::default().wakeup();
    }

    /// PORTS: `application.zig:594-637` `quit`. Begins stopping the run loop; it
    /// does NOT `posix.exit`. If the run loop already exited we are done; if the
    /// core needs no quit confirmation we exit immediately via [`Application::quit_now`];
    /// otherwise we present a confirmation dialog anchored on the active window.
    pub fn quit(&self) {
        let imp = self.imp();

        // If our run loop has already exited then we are done (`:599-600`).
        if !imp.running.get() {
            return;
        }

        // If the core doesn't need to confirm quit, exit immediately (`:602-607`).
        // An unbound confirm-quit thunk (no core wired) reads as `false`.
        let needs_confirm = imp
            .needs_confirm_quit_thunk
            .borrow()
            .as_ref()
            .is_some_and(|probe| probe());
        if !needs_confirm {
            self.quit_now();
            return;
        }

        // Find the active toplevel to parent the dialog on (`:609-623`). In the
        // rare case there is none we simply skip so we don't crash.
        let _parent = find_active_window();

        // PORT NOTE (INHABITED placeholder): the `CloseConfirmationDialog`
        // (`:625-636`) lands with Task 24; its `.new(.app)` + `close-request`
        // connect (→ [`Application::quit_now`]) + `present(parent)` drop in there.
        // Until then a confirm-quit request is surfaced on stderr and the loop keeps
        // running (the user has not confirmed), which is the faithful pre-dialog
        // behavior (we must NOT quit without confirmation).
        eprintln!(
            "warn(gtk): quit requires confirmation; the CloseConfirmationDialog lands with Task 24"
        );
    }

    /// PORTS: `application.zig:639-665` `quitNow`. Destroys our windows (forcing
    /// them to free, which triggers a proper shutdown sequence) then trips the run
    /// loop's exit flag. All `must_quit` scenarios route here — no confirmation.
    pub fn quit_now(&self) {
        // Get all toplevels and destroy ONLY our own `Window`s (`:640-661`).
        //
        // The fcitx-IME double-free note (`:652-656`, load-bearing): we previously
        // destroyed ALL toplevels, but that double-frees with the fcitx IME, which
        // owns a nested `gtk.Window` (as a property) that we do not own and that it
        // later frees itself. So we destroy only windows that are our own `Window`
        // subclass (`gobject.ext.isA(window, Window)`), which own every other window
        // type and thus trigger a proper cascading shutdown.
        for window in gtk4::Window::list_toplevels() {
            // PORT NOTE (INHABITED placeholder): the `isA(window, Window)` guard +
            // `window.destroy()` need our `Window` GObject subclass, which lands
            // with Task 12. Until then no `Window` toplevels exist (no window is
            // created yet), so the loop body is dormant — but the guard is
            // essential (never destroy foreign toplevels) and drops in with that
            // class. Referencing `window` keeps the port shape explicit.
            let _ = window;
        }

        // Trigger our runloop exit (`:664`).
        self.imp().running.set(false);
    }

    /// PORTS: `application.zig:668-...` `performAction`. Dispatches an apprt action
    /// on a target. **Task 23 fills this.** PORT NOTE: the skeleton has no
    /// parameters; Task 23 re-types it to take the `apprt` `Target`/`Action` once the
    /// action dispatch surface is wired.
    #[must_use]
    pub fn perform_action(&self) -> bool {
        unimplemented!("Application::perform_action lands in L5.1 Task 23 (application.zig:668)")
    }
}

/// PORTS: `application.zig:2481-2486` `Action.render` — the `.render` arm of
/// `performAction` (`:740` `.render => Action.render(target)`). The app/main-thread
/// leg of the **T16 render contract**: the core app, draining a `redraw_surface`
/// mailbox message, calls `rt_app.performAction(.{ .surface = core }, .render, {})`
/// (`rustty_core::app::redrawSurface`, `app.rs:474-479`); that reaches
/// [`crate::gtk::app::App::perform_action`], which dispatches the `.render` action
/// here.
///
/// `.app` is a no-op (nothing to redraw at the app level); `.surface` reaches the
/// core surface's opaque `rt_surface` handle through the [`CoreSurfaceHandle`]
/// trait (`v.rt_surface_ptr()` — the reference's `v.rt_surface`), reconstitutes the
/// apprt [`Surface`](ApprtSurface) it points at, and calls `redraw()` — which forwards
/// to the `GhosttySurface` widget's `gl_area.queue_render()` (`surface.zig:822-826`,
/// the reference's `v.rt_surface.surface.redraw()`). GTK then emits `render` on the
/// app thread → `glareaRender` → `drawFrame(true)` (the final two hops).
///
/// PORT NOTE (T23): the reference routes here through the big `performAction`
/// switch (`application.zig:668-799`), which [`Application::perform_action`] fills
/// in Task 23. Until then [`crate::gtk::app::App::perform_action`] special-cases
/// `.render` to this helper directly, so the render contract is live now — the one
/// action first-light needs — while the full dispatch surface stays deferred.
pub(crate) fn action_render<C: CoreSurfaceHandle>(target: Target<'_, C>) {
    match target {
        // `.app => {}` (`application.zig:2483`).
        Target::App => {}
        // `.surface => |v| v.rt_surface.surface.redraw()` (`:2484`).
        Target::Surface(v) => {
            let rt = v.rt_surface_ptr().cast::<ApprtSurface>();
            if rt.is_null() {
                return;
            }
            // SAFETY: for a `.render` action the core app hands the LIVE core
            // surface whose `rt_surface` points at a pinned, still-alive apprt
            // `Surface` embedded in the `GhosttySurface` widget's GObject private
            // (memory `mirihar-surface-pin`); the render runs synchronously on the
            // app thread while that widget is alive.
            unsafe { (*rt).redraw() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `config` property getter returns `None` on a freshly-built skeleton
    /// instance (before `new` sets the config). Non-display: building the bare
    /// subclass and reading a nullable property needs no `gtk_init`/display — but a
    /// `GhosttyApplication` is an `adw::Application` (a `GApplication`), whose
    /// construction gtk4-rs guards with `assert_initialized_main_thread!` on some
    /// paths; to stay in the headless gate we do NOT instantiate here. Instead this
    /// compile-touches the accessor types (a `refAllDecls` analogue) so the skeleton
    /// API surface is asserted without a display.
    #[test]
    fn application_accessor_types() {
        // Type-level witnesses that the accessors exist with the intended
        // signatures; no instance is constructed (headless gate).
        let _default: fn() -> Application = Application::default;
        let _alloc: fn(&Application) -> allocator_api2::alloc::Global = Application::allocator;
        let _get_config: fn(&Application) -> Option<Config> = Application::get_config;
    }

    // ---- display-dependent lifecycle tests --------------------------------
    //
    // These construct a real `GhosttyApplication` and drive its
    // startup/activate/dispose vfuncs, so they need an initialized GTK + a
    // display. Marked `#[ignore]`; run via
    //   xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored
    // GTK binds its "main thread" to whichever thread first calls `gtk::init`;
    // the cargo harness runs each test on a worker thread, so at most ONE GTK
    // test's body executes per multi-threaded run (the others `gtk::init().err()`
    // and skip — the established pattern, see `ext/actions.rs`
    // `adding_actions_to_an_object`). Run each individually to exercise its body.

    use allocator_api2::alloc::Global;
    use rustty_apprt_api as apprt;
    use rustty_core::app::{App as CoreApp, Message};
    use rustty_core::surface::Surface as CoreSurface;

    /// A minimal apprt `App` double: enough to parametrize `CoreApp<Double>` so a
    /// real core-app mailbox exists for the `activate` push to land in. Mirrors
    /// `rustty-core`'s in-crate `TestApp`/`TestSurface` (app.rs).
    struct Double {
        config: rustty_config::Config,
    }

    impl apprt::App for Double {
        type Surface = DoubleSurface;

        fn perform_action<C: apprt::CoreSurfaceHandle>(
            &self,
            _target: apprt::Target<'_, C>,
            _action: apprt::Action<'_>,
        ) -> Result<bool, apprt::ApprtError> {
            Ok(true)
        }

        fn perform_ipc(
            &self,
            _target: apprt::ipc::Target<'_>,
            _action: apprt::ipc::Action<'_>,
        ) -> Result<bool, apprt::ipc::Errors> {
            unimplemented!("test double: not exercised")
        }

        fn wakeup(&self) {}

        fn config(&self) -> &rustty_config::Config {
            &self.config
        }

        fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
            rustty_input::keyboard::Layout::Unknown
        }
    }

    struct DoubleSurface;

    impl apprt::Surface for DoubleSurface {
        type App = Double;
        type Core = CoreSurface<Double>;

        fn core(&self) -> *mut Self::Core {
            std::ptr::null_mut()
        }
        fn rt_app(&self) -> *mut Double {
            std::ptr::null_mut()
        }
        fn close(&self, _process_alive: bool) {}
        fn deinit(&mut self) {}
        fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
            unimplemented!("test double: not exercised")
        }
        fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
            unimplemented!("test double: not exercised")
        }
        fn get_title(&self) -> Option<&std::ffi::CStr> {
            None
        }
        fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
            unimplemented!("test double: not exercised")
        }
        fn clipboard_request(
            &self,
            _clipboard_type: apprt::Clipboard,
            _state: apprt::ClipboardRequest,
        ) -> Result<bool, apprt::ApprtError> {
            unimplemented!("test double: not exercised")
        }
        fn set_clipboard(
            &self,
            _clipboard_type: apprt::Clipboard,
            _contents: &[apprt::ClipboardContent<'_>],
            _confirm: bool,
        ) -> Result<(), apprt::ApprtError> {
            unimplemented!("test double: not exercised")
        }
        fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
            unimplemented!("test double: not exercised")
        }
        fn default_termio_env(
            &self,
        ) -> Result<std::collections::HashMap<String, String>, apprt::ApprtError> {
            unimplemented!("test double: not exercised")
        }
    }

    /// Build a `GhosttyApplication` for a lifecycle test WITHOUT going through
    /// [`Application::new`]: `new` calls `set_gtk_env`, which `debug_assert!`s GTK
    /// is not yet initialized (`application.zig:2850`) and so can run at most once
    /// per process — unusable across multiple constructing tests. This builds the
    /// subclass directly (the same `glib::Object::builder` shape `new` uses at
    /// `:1178-1182`, minus the resource pin) and fills the two private fields the
    /// startup/activate helpers read (`config`, `core_app`). Flags force
    /// `NON_UNIQUE` so `register` stays local (no session-bus dependency).
    fn build_test_app(core_app: *mut std::ffi::c_void) -> Application {
        let app: Application = glib::Object::builder()
            .property("application-id", "io.airely.rustty.LifecycleTest")
            .property("flags", gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let imp = app.imp();
        imp.config
            .replace(Some(Config::new(&rustty_config::Config::default_config())));
        imp.core_app.set(NonNull::new(core_app));
        app
    }

    /// PORTS: `application.zig:1294-1330` `startup` + `:1411-1429`
    /// `startupActionMap`. Registering the app runs the startup vfunc, which must
    /// install us as the default `GApplication` (`:1303`) and register the seven
    /// application actions (`:1418-1426`).
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn application_startup_sets_default_and_actions() {
        use gio::prelude::{ActionGroupExt, ApplicationExt};

        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        let app = build_test_app(std::ptr::null_mut());

        // `register` emits `startup` synchronously on the primary instance.
        app.register(gio::Cancellable::NONE).expect("register");

        // We are the default application (`:1303`).
        let default = gio::Application::default().expect("a default GApplication exists");
        assert_eq!(
            default.downcast::<Application>().ok().as_ref(),
            Some(&app),
            "startup must set our application as the process-global default"
        );

        // All seven actions are present (`:1418-1426`).
        let names: Vec<String> = app.list_actions().into_iter().map(Into::into).collect();
        for expected in [
            "new-window",
            "new-window-command",
            "open-config",
            "present-surface",
            "quit",
            "reload-config",
            "toggle-quick-terminal",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "startup must register the '{expected}' action; got {names:?}"
            );
        }
    }

    /// PORTS: `application.zig:1459-1473` `activate`. Activating queues exactly one
    /// `Message::NewWindow` onto the core app mailbox — the app's only
    /// window-creation entry (`:1463-1466`).
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn application_activate_pushes_new_window() {
        use gio::subclass::prelude::ApplicationImpl;

        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        // A real core app so the mailbox exists; `create` gives a heap-stable
        // pointer (the `Box` address is stable for the app's lifetime).
        let mut core_app: Box<CoreApp<Double>> =
            CoreApp::<Double>::create(Global).expect("core app");
        let core_ptr: *mut CoreApp<Double> = &mut *core_app;

        let app = build_test_app(core_ptr.cast());
        // Supply the concrete apprt `App` type so the `activate` thunk is
        // monomorphized (the Rust analogue of Zig's comptime `*CoreApp`).
        app.bind_core_app::<Double>();

        // Drive the `activate` vfunc directly (no g_application_activate / no
        // registration needed for the mailbox push).
        <imp::Application as ApplicationImpl>::activate(app.imp());

        // Drain: exactly one NewWindow.
        let mut new_windows = 0usize;
        let mut total = 0usize;
        while let Some(msg) = core_app.mailbox.pop() {
            total += 1;
            if matches!(msg, Message::NewWindow(_)) {
                new_windows += 1;
            }
        }
        assert_eq!(new_windows, 1, "activate must push exactly one NewWindow");
        assert_eq!(total, 1, "activate must push nothing but the NewWindow");
    }

    /// PORTS: `application.zig:1475-1493` `dispose` + `:1400-1408` `startupSignals`.
    /// Startup installs the SIGUSR2 source (`:1403`); dispose removes it and clears
    /// the field (invariant 11, `:1482-1487`).
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn application_dispose_removes_signal_source() {
        use gio::prelude::ApplicationExt;

        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        let app = build_test_app(std::ptr::null_mut());

        // `register` → `startup` → `startupSignals` installs the source.
        app.register(gio::Cancellable::NONE).expect("register");
        assert!(
            app.imp().signal_source.borrow().is_some(),
            "startup must install the SIGUSR2 signal source"
        );

        // `run_dispose` invokes the `dispose` vfunc. `unsafe`: it may run while
        // other references to the object exist; here `app` is the sole owner and
        // is not used for GObject operations after this, so it is sound.
        unsafe { app.run_dispose() };
        assert!(
            app.imp().signal_source.borrow().is_none(),
            "dispose must remove the signal source and clear the field"
        );
    }

    /// PORTS: `application.zig:474-592` `run` + `:1286-1289` `wakeup` + `:639-665`
    /// `quitNow`. The L5.1 gate KAT for the hand-rolled loop: with no initial
    /// window and `quit-after-last-window-closed`, `run` must acquire → register →
    /// (skip activate) → loop `{ iteration(true); tick; check-quit }`, exit cleanly
    /// when `quit_now` sets `running = false`, and have ticked the core at least
    /// once. A background thread exercises the `wakeup` bridge (breaking the
    /// blocking iteration), and a main-thread timeout routes to `quit_now`.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn application_run_loop_ticks_then_quits() {
        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        // A real apprt `App` double + core app so the tick thunk dereferences live
        // backrefs (both heap/stack pointers are stable for the test's duration).
        let mut rt = Double {
            config: rustty_config::Config::default_config(),
        };
        let rt_ptr: *mut Double = &mut rt;
        let mut core_app: Box<CoreApp<Double>> =
            CoreApp::<Double>::create(Global).expect("core app");
        let core_ptr: *mut CoreApp<Double> = &mut *core_app;

        let app = build_test_app(core_ptr.cast());
        app.imp().rt_app.set(NonNull::new(rt_ptr.cast()));

        // No initial window (so `activate`/window-creation stays dormant — no real
        // renderer here), quit after last window closed on. `quit_now` is what
        // actually ends this test's loop.
        let mut cfg = rustty_config::Config::default_config();
        cfg.initial_window = false;
        cfg.quit_after_last_window_closed = true;
        app.imp().config.replace(Some(Config::new(&cfg)));

        // Supply the concrete apprt `App` type so the tick / new-window thunks are
        // monomorphized (the Rust analogue of Zig's comptime `*CoreApp`).
        app.bind_core_app::<Double>();

        // Route a main-thread quit through `quit_now` after the loop has ticked a
        // few times (a glib timeout fires on the acquired context inside `run`).
        let app_weak = app.downgrade();
        glib::timeout_add_local(std::time::Duration::from_millis(120), move || {
            if let Some(app) = app_weak.upgrade() {
                app.quit_now();
            }
            glib::ControlFlow::Break
        });

        // Exercise the libghostty→GTK `wakeup` bridge from another thread: break
        // the blocking `iteration(true)` so the inline tick runs promptly.
        std::thread::spawn(|| {
            for _ in 0..8 {
                std::thread::sleep(std::time::Duration::from_millis(10));
                glib::MainContext::default().wakeup();
            }
        });

        let start = std::time::Instant::now();
        app.run().expect("run loop exits cleanly");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "run loop must exit within the deadline"
        );
        assert!(
            app.imp().tick_count.get() >= 1,
            "the inline core tick must run at least once (got {})",
            app.imp().tick_count.get()
        );
        assert!(
            !app.imp().running.get(),
            "quit_now must have cleared the running flag"
        );
    }

    /// PORTS: `application.zig:823-846` `startQuitTimer` + `:849-864`
    /// `stopQuitTimer`. Arming twice must not leak a source (the second arm cancels
    /// the first), and stopping is idempotent — every path returns the timer to
    /// `.off` (invariant 11).
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn quit_timer_start_stop_is_idempotent() {
        use rustty_config::value_types::Duration;

        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        let app = build_test_app(std::ptr::null_mut());

        // A delay is configured so `start_quit_timer` arms an `.active` source
        // (the leak-prone path) rather than short-circuiting to `.expired`.
        let mut cfg = rustty_config::Config::default_config();
        cfg.quit_after_last_window_closed = true;
        // 10s — long enough it never fires during the test.
        cfg.quit_after_last_window_closed_delay = Some(Duration {
            duration: 10 * 1_000_000_000,
        });
        app.imp().config.replace(Some(Config::new(&cfg)));

        // Arm once → `.active`.
        app.imp().start_quit_timer();
        assert!(
            matches!(*app.imp().quit_timer.borrow(), imp::QuitTimer::Active(_)),
            "start_quit_timer with a delay must arm an active source"
        );

        // Arm again → still `.active`, and the previous source was cancelled (no
        // leak): `start_quit_timer` calls `stop_quit_timer` first.
        app.imp().start_quit_timer();
        assert!(
            matches!(*app.imp().quit_timer.borrow(), imp::QuitTimer::Active(_)),
            "re-arming must leave exactly one active source"
        );

        // Stop → `.off`.
        app.imp().stop_quit_timer();
        assert!(
            matches!(*app.imp().quit_timer.borrow(), imp::QuitTimer::Off),
            "stop_quit_timer must return the timer to off"
        );

        // Stop again → still `.off` (idempotent, no double-remove).
        app.imp().stop_quit_timer();
        assert!(
            matches!(*app.imp().quit_timer.borrow(), imp::QuitTimer::Off),
            "stop_quit_timer must be idempotent"
        );
    }

    /// PORTS: `application.zig:1286-1289` `wakeup`. A `MainContext::wakeup` from
    /// another thread must break a blocking `iteration(true)` on the owning thread
    /// — the mechanism the run loop relies on to interleave the inline core tick.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn wakeup_breaks_blocking_iteration() {
        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        // `Application::wakeup` delegates to `MainContext::default().wakeup()`; build
        // an app to cover the method itself (calling it here is a harmless no-op
        // when nothing is blocking).
        let app = build_test_app(std::ptr::null_mut());
        app.wakeup();

        let ctx = glib::MainContext::default();
        let _acquire = ctx.acquire().expect("acquire the default main context");

        // Another thread wakes us after a short delay.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            glib::MainContext::default().wakeup();
        });

        // Blocks until the cross-thread wakeup; must return rather than hang.
        let start = std::time::Instant::now();
        let _ = ctx.iteration(true);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "wakeup must break the blocking iteration"
        );
    }
}
