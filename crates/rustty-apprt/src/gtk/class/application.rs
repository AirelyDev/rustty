//! rustty-apprt::gtk::class::application
//!
//! PORTS: ghostty `src/apprt/gtk/class/application.zig` (`Application` =
//! `GhosttyApplication`, an `adw.Application` subclass, `application.zig:135-140`).
//! This file is the **subclass skeleton** (L5.1 T7): the GObject registration, the
//! `Private` state carrier, class-init (resource registration + the `config`
//! property + the vfunc override slots), and the process-global singleton
//! accessors `default`/`allocator`/`get_config`. The heavyweight members
//! (`new`/`deinit` — Task 8, the `startup`/`activate`/`dispose`/`finalize` vfunc
//! bodies — Task 9, the hand-rolled run loop / `run` — Task 10, `perform_action` —
//! Task 23) land in later tasks; here they are INHABITED placeholders
//! (`unimplemented!()` leaves, placeholder-leaf doctrine) so the call-graph shape
//! exists and this compiles standalone.
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

use super::config::Config;
use crate::gtk::WeakRef;

mod imp {
    use super::*;

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
        // PORT NOTE: `dispose` (`application.zig:1475-1493`) — the vfunc override
        // slot exists here; its body (unref the OWNED rows) is Task 9's. The
        // skeleton keeps the default (no-op) dispose so the type registers cleanly.
    }

    // PORTS: the parent-chain `*Impl` traits for an `adw.Application` subclass. Each
    // is the gtk4-rs slot where the reference's registered vfunc overrides live:
    // `activate`/`startup` (`application.zig:1459-1473`/`:1294-1330`, registered
    // `:1889-1890`) are `ApplicationImpl` methods filled by Task 9; the skeleton
    // keeps the default passthroughs so `GhosttyApplication` registers over the full
    // `Object → GApplication → GtkApplication → AdwApplication` chain.
    impl gio::subclass::prelude::ApplicationImpl for Application {}
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
    }
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

    // ---- INHABITED placeholder leaves (placeholder-leaf doctrine) ----
    //
    // The following are the class's heavyweight members. They are present so the
    // call-graph shape exists, but their bodies land in later tasks. Each panics if
    // reached before then.

    /// PORTS: `application.zig:243-428` `new`. Creates a new `Application` — the main
    /// program entrypoint's heavy lifting (config load, i18n, adw init, winproto
    /// init, css provider, instance construction with the pinned resource base
    /// path). **Task 8 fills this.** PORT NOTE: the skeleton signature takes the two
    /// backref pointers as `*mut c_void`; Task 8 re-types them to `*mut ApprtApp` /
    /// `*mut CoreApp` and returns `Result<Self, AllocError>` (the reference's
    /// `Allocator.Error!*Self`) once those types exist (Task 11).
    #[must_use]
    pub fn new(_rt_app: *mut c_void, _core_app: *mut c_void) -> Self {
        unimplemented!("Application::new lands in L5.1 Task 8 (application.zig:243-428)")
    }

    /// PORTS: `application.zig:477-592` `run`. The hand-rolled main loop
    /// (acquire → register → activate → `while (running) { MainContext::iteration;
    /// core_app.tick; check-quit }` with the `MainContext::wakeup` bridge).
    /// **Task 10 fills this.**
    pub fn run(&self) {
        unimplemented!("Application::run lands in L5.1 Task 10 (application.zig:477-592)")
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
}
