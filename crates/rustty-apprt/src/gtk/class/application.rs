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
use rustty_config::Config as CoreConfig;

use super::config::Config;
use crate::gtk::{build_info, winproto, WeakRef};

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

        /// PORTS: `application.zig:176` `winproto: winprotopkg.App` — the
        /// per-application windowing-protocol state (the [`winproto`] tagged
        /// union). `.none` (the noop backend) until [`super::Application::new`]
        /// runs [`winproto::App::init`]; a noop-only build (L5.1 T8, no
        /// `wayland`/`x11` feature) always lands on `.none` (PREPORT_GTK §1.11 /
        /// §5.4). Freed in `deinit` via [`winproto::App::deinit`]
        /// (`application.zig:439`). `RefCell` for the shared-`&self` `imp` access.
        pub winproto: RefCell<winproto::App>,
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
            // Task 9: apply the configuration (updateConfig / CSS reload / etc.).
        }

        /// PORTS: `application.zig:...` `signalCssParsingError` — the
        /// `CssProvider::parsing-error` handler that logs CSS parse errors,
        /// connected in `new` (`application.zig:416-422`).
        ///
        /// **Task 9 fills this.** Placeholder-leaf no-op.
        pub(super) fn signal_css_parsing_error(
            &self,
            _section: &gtk4::CssSection,
            _error: &glib::Error,
        ) {
            // Task 9: log the CSS parsing error (section location + message).
        }
    }
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
        let priv_ = self.imp();

        // `priv.config.unref()` (`application.zig:438`) — clearing drops the ref.
        priv_.config.replace(None);

        // `priv.winproto.deinit()` (`application.zig:439`).
        priv_.winproto.borrow_mut().deinit();

        // PORT NOTE: `priv.open_uri.deinit()` (`application.zig:440`) and
        // `priv.global_shortcuts.unref()` (`application.zig:441`) free the portal
        // `OpenURI` and the `GlobalShortcuts` GObject. Neither is created by this
        // task's `new` (their types land in later tasks — see `new`'s PORT NOTE),
        // so there is nothing to free yet; the frees wire up when those creators
        // land. `global_shortcuts` is cleared defensively (a no-op while `None`).
        priv_.global_shortcuts.replace(None);

        // `if (priv.saved_language) |language| alloc.free(language)`
        // (`application.zig:442`) — clearing drops the `CString`.
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

        // `priv.css_provider.unref()` (`application.zig:456`).
        priv_.css_provider.replace(None);

        // Each custom provider `unref` + the list `deinit` (`application.zig:457-458`)
        // — clearing the Vec drops (unrefs) each element.
        priv_.custom_css_providers.borrow_mut().clear();
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
