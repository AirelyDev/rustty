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
use rustty_apprt_api::{Action, ApprtError, CoreSurfaceHandle, Target};
use rustty_config::Config as CoreConfig;

use super::close_confirmation_dialog::{CloseConfirmationDialog, Target as DialogTarget};
use super::command_palette::ActionBoxed;
use super::config::Config;
use super::config_errors_dialog::ConfigErrorsDialog;
use super::global_shortcuts::{BindFailed, GlobalShortcuts};
use super::overrides::{parse_overrides, TabOverrides};
use super::window::Window;
use crate::gtk::i18n::tr;
use crate::gtk::portal::OpenUri;
use crate::gtk::surface::Surface as ApprtSurface;
use crate::gtk::{build_info, ext, winproto, WeakRef};

/// `const log = std.log.scoped(.gtk_ghostty_application)` (`application.zig:50`).
/// REBRANDED per the task-14d-2 precedent (`.port/tools/PARITY_NAMING.md`); the
/// GType registered NAME in this file is deliberately NOT rebranded.
const LOG_SCOPE: &str = "gtk_rustty_application";

/// `const glib_log = std.log.scoped(.glib)` (`application.zig:62`) — the SECOND
/// scoped alias in this reference file, used only by `glibLogWriterFunction` so
/// that GLib's own messages keep their own scope tag. No brand token.
const GLIB_LOG_SCOPE: &str = "glib";

/// TEST-ONLY: how many times the installed global-shortcut dispatch thunk got
/// PAST its `rt_app` guard and actually called `performAllAction`.
///
/// A process-global rather than an imp field because the thunk is a
/// `Box<dyn Fn>` that captures only the two raw backrefs — capturing the
/// `Application` to reach a field would risk an `Rc` cycle with a `'static`
/// closure.
///
/// It exists because `global_shortcut_trigger_count` does NOT prove the dispatch
/// happened: that counter is bumped at the top of `global_shortcut_trigger`,
/// while the thunk it invokes opens with
/// `let Some(rt_app) = rt_app else { return; }`. With `rt_app` unbound the
/// trigger count still reaches 1 and `performAllAction` is never called — which
/// is exactly how the S50-F4 empty-capture assertion was VACUOUS until the Linux
/// negative control caught it (task-14d-3).
#[cfg(test)]
static PERFORM_ALL_ACTION_REACHED: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

// The `imp` subclass, the file-local free functions (gio `app.*` action
// callbacks and the other helpers), the `Action` namespace and the tests live in
// `application/` (P7-W13 K). `imp` and `action` keep their module names, so every
// `pub(super)` keeps its meaning.
mod imp;

mod gio_actions;
pub use gio_actions::RunError;
use gio_actions::{
    action_new_tab, action_new_window, action_open_config, action_present_surface, action_quit,
    action_reload_config, action_ring_bell, action_toggle_quick_terminal, deinit_private,
    find_active_window, g_unix_signal_add_full, glib_log_writer, handle_sigusr2, set_gtk_env,
    AppTargetHandle, RunCleanup, SIGUSR2,
};
pub(crate) use gio_actions::{bind_failed_notification, read_to_end_alloc, ReadToEndAllocError};

glib::wrapper! {
    /// The primary entrypoint for the Rustty GTK application.
    ///
    /// This requires a `rustty.App` and `rustty.Config` and takes
    /// care of the rest. Call `run` to run the application to completion.
    ///
    /// This type creates a new GObject class. Since the Application is
    /// the primary entrypoint I'm going to use this as a place to document
    /// how this all works and where you can find resources for it, but
    /// this applies to any other GObject class within this apprt.
    ///
    /// The various fields (parent_instance) and constants (Parent,
    /// getGObjectType, etc.) are mandatory "interfaces" for zig-gobject
    /// to create a GObject class.
    ///
    /// I found these to be the best resources:
    ///
    ///   * https://github.com/ianprime0509/zig-gobject/blob/d7f1edaf50193d49b56c60568dfaa9f23195565b/extensions/gobject2.zig
    ///   * https://github.com/ianprime0509/zig-gobject/blob/d7f1edaf50193d49b56c60568dfaa9f23195565b/example/src/custom_class.zig
    ///
    /// PORT NOTE: the gtk4-rs `glib::wrapper!` + `#[glib::object_subclass]`
    /// idiom replaces zig-gobject's mandatory fields and constants.
    ///
    /// The `GhosttyApplication` — the application-wide GObject, an
    /// [`libadwaita::Application`] subclass. PORTS: `application.zig:112-140`
    /// `pub const Application`. Process-global singleton: reach it anywhere via
    /// [`Application::default`].
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends libadwaita::Application, gtk4::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Application {
    /// Get this application as the default, allowing access to its
    /// properties globally.
    ///
    /// This asserts that there is a default application and that the
    /// default application is a GhosttyApplication. The program would have
    /// to be in a very bad state for this to be violated.
    ///
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

    /// The global allocator that all other classes should use by
    /// calling `Application.default().allocator()`. Zig code should prefer
    /// this wherever possible so we get leak detection in debug/tests.
    ///
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

    /// Get the original language that Rustty was launched with. This returns a
    /// pointer to internal memory so it must be copied by callers.
    ///
    /// PORTS: `application.zig:475-478` `savedLanguage` — the launch-time `LANG`
    /// captured by [`Application::new`] BEFORE the `config.language` override
    /// (`application.zig:279-290`), applied to child-process environments by
    /// [`crate::gtk::class::surface`]'s `defaultTermioEnv` (`surface.zig:1602-1606`,
    /// the sole consumer). `None` when `LANG` was unset at launch — the consumer
    /// then removes `LANG` from the child env (the reference's `else` branch).
    ///
    /// PORT NOTE: the reference returns a pointer to internal memory that "must be
    /// copied by callers"; the [`CString`] clone here IS that copy (the stored
    /// field is the reference's sentinel-terminated `?[:0]const u8`).
    #[must_use]
    pub fn saved_language(&self) -> Option<CString> {
        self.imp().saved_language.borrow().clone()
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

    /// Creates a new Application instance.
    ///
    /// This does a lot more work than a typical class instantiation,
    /// because we expect that this is the main program entrypoint.
    ///
    /// The only failure mode of initializing the application is early OOM.
    /// Early OOM can't be recovered from. Every other error is mapped to
    /// some degraded state where we can at least show a window with an error.
    ///
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

        // Capture GLib/GObject/GTK log messages and funnel them through Zig's
        // logging system rather than just getting dumped directly to stderr.
        //
        // (`application.zig:249-251`.) gtk4-rs's `log_set_writer_func` panics if set more than once, so guard
        // with a `Once` — repeated `Application::new` (e.g. across tests) must not
        // abort.
        {
            use std::sync::Once;
            static WRITER: Once = Once::new();
            WRITER.call_once(|| glib::log_set_writer_func(glib_log_writer));
        }

        // Log our GTK versions
        //
        // (GTK and libadwaita, `application.zig:254-255`.)
        crate::gtk::gtk_version::log_version();
        crate::gtk::adw_version::log_version();

        // Load our configuration.
        //
        // (`application.zig:258-271`; the default only fails for OOM, which
        // aborts in Rust.)
        let config: CoreConfig = match CoreConfig::load() {
            Ok(config) => config,
            Err(err) => {
                // If we fail to load the configuration, then we should log
                // the error in the diagnostics so it can be shown to the user.
                // We can still load a default which only fails for OOM, allowing
                // us to startup.
                let mut def = CoreConfig::default_config();
                def.add_diagnostic_fmt(
                    format!("error loading user configuration: {err:?}").into_bytes(),
                );
                def
            }
        };

        // Set the old language,
        //
        // (Capture the launch-time LANG, then override it from `config.language`,
        // `application.zig:274-284`. The capture happens BEFORE the override.)
        let saved_language: Option<CString> = {
            let old_language = env_getenv(c"LANG").and_then(|v| CString::new(v).ok());
            if let Some(language) = &config.language {
                // Override LANG if we need to (sync global environs if so)
                if let Ok(lang) = CString::new(language.clone()) {
                    env_setenv(c"LANG", &lang);
                }
            }
            old_language
        };

        // Set gettext global domain to be our app so that our unqualified
        // translations map to our translations.
        //
        // (`application.zig:288-293`.)
        if let Err(err) = rustty_os::i18n::initGlobalDomain() {
            // Failures shuldn't stop application startup. Our app may
            // not translate correctly but it should still work. In the
            // future we may want to add this to the GUI to show.
            log::warn!(target: LOG_SCOPE, "i18n initialization failed error={err:?}");
        }

        // Setup our GTK init env vars
        //
        // (`application.zig:296-305`), then libadwaita
        // UNRESTORED SITES (task-14d-3) — this file's residual deficit, each a
        // `catch` arm of a call that is INFALLIBLE in this port, so restoring
        // any of them would put a diagnostic on a success path:
        //   `:114` `UNKNOWN: {s}: {s}` — gtk4-rs decodes the level flags into a
        //          closed `glib::LogLevel` with no unknown variant.
        //   `:314` `error setting GTK environment variables err={}` — see below.
        //   `:1258` `accelerator somehow longer than 1024 bytes: {f}` — the
        //          reference formats into a fixed 1024-byte stack buffer; this
        //          port formats into a `String`.
        //   `:1304` `out of memory loading runtime CSS, no runtime CSS applied` —
        //          `load_runtime_css` returns `()`. (CORRECTION, task T-D: this
        //          entry also listed `:1310` `failed to load custom CSS, ...` as
        //          unrestorable because `load_custom_css` returned `()`; it returns
        //          the reference's read error now and `:1310` is live, S49-F3.)
        //   `:1804` `error toggling quick terminal err={}` and `:1815`
        //          `error quitting err={}` — both collapse to a direct call
        //          that cannot fail (see each action fn's own PORT NOTE).
        //   `:1998 :2002 :2037 :2044 :2814` — Zig `dupeZ`/`ArrayList.append`/
        //          `bufPrint` allocation failures; the Rust equivalents are
        //          infallible.
        //   (`:2197` `unable to load resources` is live: `gtk.rs`
        //          `register_resources`, task S-1c.)
        //   (`:3024` `unable to resize split, out of memory` is live in
        //          `resize_split`; its text had lost ", out of memory".)
        //   Each remaining site is a machine-checked row in
        //   `.port/parity-closure/LOG-ABSENT-SITES.tsv` (task S-1c).
        //
        // (`application.zig:306`). `set_gtk_env` is infallible in rustty (see its
        // doc), so the reference's `error.NoSpaceLeft` warn arm folds away. That
        // arm reads:
        //
        //     // If we fail to set GTK environment variables then we still
        //     // try to start the application...
        set_gtk_env(&config);
        let _ = libadwaita::init();

        // single-instance -> application flags (`application.zig:308-320`).
        let single_instance = match config.gtk_single_instance {
            rustty_config::value_types::GtkSingleInstance::True => true,
            rustty_config::value_types::GtkSingleInstance::False => false,
            // This should have been resolved to true/false during config loading.
            //
            // (`application.zig:311-312`.)
            rustty_config::value_types::GtkSingleInstance::Detect => unreachable!(
                "gtk-single-instance must be resolved to true/false during config load"
            ),
        };
        // Setup the flags for our application.
        let mut app_flags = gio::ApplicationFlags::empty();
        if !single_instance {
            app_flags |= gio::ApplicationFlags::NON_UNIQUE;
        }

        // Our app ID determines uniqueness and maps to our desktop file.
        // We append "-debug" to the ID if we're in debug mode so that we
        // can develop Rustty in Rustty.
        //
        // (`config.class` if a valid GApplication id, else the build-info
        // default, `application.zig:325-335`.)
        let app_id: String = 'app_id: {
            if let Some(class) = &config.class {
                if let Ok(class_str) = std::str::from_utf8(class) {
                    if gio::Application::id_is_valid(class_str) {
                        break 'app_id class_str.to_owned();
                    }
                }
                log::warn!(target: LOG_SCOPE, "invalid 'class' in config, ignoring");
            }
            build_info::APPLICATION_ID.to_owned()
        };

        // (`application.zig:337-342`.)
        let Some(display) = gdk4::Display::default() else {
            // I'm unsure of any scenario where this happens. Because we don't
            // want to litter null checks everywhere, we just exit here.
            log::warn!(target: LOG_SCOPE, "gdk display is null, exiting");
            std::process::exit(1);
        };

        // Setup our windowing protocol logic
        //
        // (`application.zig:345-358`.)
        let winproto = match winproto::App::init(&display, &app_id, &config) {
            Ok(wp) => wp,
            Err(err) => {
                // If we fail to detect or setup the windowing protocol
                // specifies, we fallback to a noop implementation so we can
                // still launch.
                log::warn!(target: LOG_SCOPE, "error initializing windowing protocol err={err}");
                winproto::App::default()
            }
        };
        log::debug!(target: LOG_SCOPE, "windowing protocol={:?}", winproto.protocol());

        // Create our GTK Application which encapsulates our process.
        log::debug!(target: LOG_SCOPE, "creating GTK application id={app_id} single-instance={single_instance}");

        // Wrap our configuration in a GObject.
        //
        // (`application.zig:366-368`.) This clones the config, so the local `config` may drop at end of scope.
        let config_obj = Config::new(&config);

        // Internally, GTK ensures that only one instance of this provider
        // exists in the provider list for the display.
        //
        // (`application.zig:371-378`.)
        let css_provider = gtk4::CssProvider::new();
        gtk4::style_context_add_provider_for_display(
            &display,
            &css_provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION + 3,
        );

        // Initialize the app.
        //
        // (`application.zig:381-389`.)
        let app: Self = glib::Object::builder()
            .property("application-id", &app_id)
            .property("flags", app_flags)
            // Force the resource path to a known value so it doesn't depend
            // on the app id (which changes between debug/release and can be
            // user-configured) and force it to load in compiled resources.
            //
            // ([`build_info::RESOURCE_PATH`], `application.zig:385-387`.)
            .property("resource-base-path", build_info::RESOURCE_PATH)
            .build();

        // Setup our private state. More setup is done in the init
        // callback that GObject calls, but we can't pass this data through
        // to there (and we don't need it there directly) so this is here.
        //
        // (`application.zig:394-405`.)
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

        // Signals
        //
        // (`application.zig:408-422`.) The `notify::config` handler
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

        // Trigger initial config changes
        //
        // (`application.zig:425`.)
        app.notify("config");

        app
    }

    /// Force deinitialize the application.
    ///
    /// Normally in a GObject lifecycle, this would be called by the
    /// finalizer. But applications are never fully unreferenced so this
    /// ensures that our memory is cleaned up properly.
    ///
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

        // The "present a surface by ID" push (`actionPresentSurface`,
        // `application.zig:1834-1849`): use the u64 as a surface ID, look it up in
        // the core app, and push a `.surface_message`/`.present_surface` envelope.
        self.imp()
            .present_surface_thunk
            .replace(Some(Box::new(move |id: u64| {
                // SAFETY: `core_app` is the stable heap pointer the caller passed to
                // `new`; the app thread drives this and validates surface pointers
                // before use (core's threading contract).
                if let Some(surface) = unsafe { &*core_app }.findSurfaceByID(id) {
                    let _ = unsafe { &*core_app }.mailbox.push(
                        rustty_core::app::Message::SurfaceMessage(
                            rustty_apprt_api::SurfaceMessageEnvelope {
                                surface,
                                message: rustty_apprt_api::surface::Message::PresentSurface,
                            },
                        ),
                        rustty_datastruct::blocking_queue::Timeout::Forever,
                    );
                }
            })));

        // The `open-config` push (`actionOpenConfig`, `application.zig:2102`):
        // `self.core().mailbox.push(global.io(), .{ .open_config = target }, .forever)`.
        self.imp().open_config_thunk.replace(Some(Box::new(
            move |target: rustty_core::app::OpenConfig| {
                // SAFETY: `core_app` is the stable heap pointer the caller passed to
                // `new`; the push is the mailbox's own thread-safe enqueue.
                let _ = unsafe { &*core_app }.mailbox.push(
                    rustty_core::app::Message::OpenConfig(target),
                    rustty_datastruct::blocking_queue::Timeout::Forever,
                );
            },
        )));

        // The `new-tab` surface resolution + dispatch (`actionNewTab`,
        // `application.zig:1858-1907`, B5/034506f14): resolve the target
        // surface (by ID, else the focused surface), then `Action.newTab` on
        // it — falling back to `Action.newWindow` when no surface is found.
        {
            let weak = self.downgrade();
            self.imp()
                .new_tab_thunk
                .replace(Some(Box::new(move |surface_id_, overrides| {
                    // `const surface_ = surface: { ... }` (`:1858-1876`): an
                    // ID lookup that misses FALLS THROUGH to the focused
                    // surface; a focused-surface miss yields null.
                    //
                    // SAFETY: `core_app` is the stable heap pointer the caller
                    // passed to `new`; the app thread drives this and
                    // validates surface pointers before use (core's threading
                    // contract).
                    let surface_ = 'surface: {
                        if let Some(surface_id) = surface_id_ {
                            match unsafe { &*core_app }.findSurfaceByID(surface_id) {
                                Some(s) => break 'surface Some(s),
                                None => log::warn!(target: LOG_SCOPE, "new-tab: unable to find surface 0x{surface_id:016x}"),
                            }
                        }
                        match unsafe { &*core_app }.focusedSurface() {
                            Some(s) => Some(s),
                            None => {
                                log::warn!(target: LOG_SCOPE, "new-tab: unable to find surface");
                                None
                            }
                        }
                    };

                    let Some(app) = weak.upgrade() else { return };

                    if let Some(surface) = surface_ {
                        // `Action.newTab(.{ .surface = surface }, .{ ... })`
                        // (`:1878-1893`).
                        //
                        // SAFETY: `findSurfaceByID`/`focusedSurface` return
                        // live app-thread surfaces (validated above).
                        if !action::new_tab(unsafe { Target::surface_ptr(surface) }, overrides) {
                            log::warn!(target: LOG_SCOPE, "new-tab: unable to create tab");
                        }
                    } else {
                        // `Action.newWindow(self, null, .{ ... })`
                        // (`:1895-1906`).
                        if let Err(err) =
                            action::new_window(&app, Target::<AppTargetHandle>::App, overrides)
                        {
                            log::warn!(target: LOG_SCOPE, "new-tab: unable to create new window: {err:?}");
                        }
                    }
                })));
        }

        // The inline core tick (`try priv.core_app.tick(priv.rt_app)`,
        // `application.zig:552`). `tick` takes `&mut App<A>` and `rt_app: &A`; the
        // apprt `App` (`= A`) is the type-erased `rt_app` backref. Both pointers are
        // BACKREFs owned by libghostty-core / the apprt `App`, never freed here.
        let rt_app = self.imp().rt_app.get().map(|p| p.as_ptr() as *mut A);
        self.imp().tick_thunk.replace(Some(Box::new(move || {
            match rt_app {
                // SAFETY: `core_app`/`rt_app` are the stable heap pointers the caller
                // passed to `new`; the run loop drives this on the app thread only.
                // (N443: raw-self; a drained message can re-enter the core app.)
                Some(rt_app) => unsafe { rustty_core::app::App::tick(core_app, &*rt_app) },
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

        // The `.app`-target config reload (`self.core().updateConfig(self.rt(),
        // config)`, `application.zig:2473-2474`). `updateConfig` takes `&mut App<A>`
        // and `rt_app: &A`; both `core_app` and `rt_app` are BACKREFs owned by
        // libghostty-core / the apprt `App`, never freed here. Re-entrant: core's
        // `updateConfig` synchronously calls back `rt_app.perform_action(.app,
        // .config_change, ...)` which lands in `action::config_change` →
        // `Application::set_config`.
        self.imp().update_config_thunk.replace(Some(Box::new(
            move |config: &rustty_config::Config| {
                match rt_app {
                    // SAFETY: as above — app-thread-only access to the two backrefs.
                    Some(rt_app) => {
                        unsafe { &mut *core_app }.updateConfig(unsafe { &*rt_app }, config)
                    }
                    None => Ok(()),
                }
            },
        )));

        // The color-scheme propagation (`handleStyleManagerDark`,
        // `application.zig:1553-1563`): `core_app.colorSchemeEvent(self.rt(),
        // scheme)` followed by, for each surface, `surface.core()
        // .colorSchemeCallback(scheme)`. `colorSchemeEvent` takes `&mut App<A>` and
        // `rt_app: &A`; both `core_app` and `rt_app` are BACKREFs owned by
        // libghostty-core / the apprt `App`, never freed here. `.core()` is the
        // apprt `Surface` seam (in scope via the `use` below) yielding the
        // `*mut CoreSurface`.
        use rustty_apprt_api::Surface as _;
        self.imp()
            .color_scheme_thunk
            .replace(Some(Box::new(move |is_dark: bool| {
                let scheme = if is_dark {
                    rustty_apprt_api::ColorScheme::Dark
                } else {
                    rustty_apprt_api::ColorScheme::Light
                };

                // `core_app.colorSchemeEvent(self.rt(), scheme) catch |err|
                // log.warn("error updating app color scheme err={}", ...)`.
                if let Some(rt_app) = rt_app {
                    // SAFETY: as above — app-thread-only access to the two backrefs.
                    if let Err(err) = unsafe {
                        rustty_core::app::App::colorSchemeEvent(core_app, &*rt_app, scheme)
                    } {
                        // log.warn("error updating app color scheme err={}", .{err});
                        // (task S-1c: ported live; it was a comment.)
                        log::warn!(
                            target: LOG_SCOPE,
                            "error updating app color scheme err={err:?}"
                        );
                    }
                }

                // `for (core_app.surfaces.items) |surface|
                // surface.core().colorSchemeCallback(scheme) catch |err|
                // log.warn("unable to tell surface about color scheme change
                // err={}", ...)`.
                for &surface in unsafe { &*core_app }.surfaces.iter() {
                    // SAFETY: as above — app-thread-only access to the surface
                    // backrefs the core app owns.
                    let core = unsafe { &*surface }.core();
                    // N456 (ruling 57): raw-self, because the soft
                    // `reload_config` it performs re-enters this surface
                    // (`reload_config_inner` -> `updateConfig`).
                    if let Err(err) =
                        unsafe { rustty_core::surface::Surface::colorSchemeCallback(core, scheme) }
                    {
                        // log.warn(
                        //     "unable to tell surface about color scheme change err={}",
                        //     .{err},
                        // );
                        log::warn!(
                            target: LOG_SCOPE,
                            "unable to tell surface about color scheme change err={err:?}"
                        );
                    }
                }
            })));

        // The global-shortcut dispatch (`globalShortcutTrigger`,
        // `application.zig:1655-1663`): `core_app.performAllAction(rt_app,
        // action)`. `performAllAction` takes the raw `*mut App<A>` and `rt_app: &A`; both
        // are BACKREFs owned by libghostty-core / the apprt `App`, never freed
        // here. It runs on the app thread (the GTK main loop delivers the portal
        // `Activated` signal there).
        self.imp().perform_all_action_thunk.replace(Some(Box::new(
            move |action: rustty_input::binding::Action| {
                let Some(rt_app) = rt_app else {
                    return;
                };
                #[cfg(test)]
                PERFORM_ALL_ACTION_REACHED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // SAFETY: as above — app-thread-only access to the two backrefs.
                // self.core().performAllAction(self.rt(), action.*) catch |err| {
                //     log.warn("failed to perform action={}", .{err});
                // };
                //
                // S50-F4 (INVERSE PARITY DEFECT, fixed task-14d-3): there used to
                // be a success arm here emitting
                // `debug(gtk): global shortcut action performed ok`. The
                // reference (`application.zig:1741-1749`) has NO success-side log
                // — it is a bare `catch`. An emission the reference does not make
                // is as much a parity defect as a missing one, so it is gone.
                //
                // Its stated purpose was to give
                // `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh` an
                // observable for "the chain completed". That gate did not lose
                // coverage: its very next assertion counts a NEW TOPLEVEL WINDOW
                // on the session bus, and the bound action is `new_window` — a
                // window appearing entails `performAllAction` having run and
                // returned Ok, so the retained assertion implies the removed one.
                // (N443: raw-self; the action's host callback re-enters the app.)
                if let Err(err) =
                    unsafe { rustty_core::app::App::performAllAction(core_app, &*rt_app, action) }
                {
                    // log.warn("failed to perform action={}", .{err});
                    log::warn!(target: LOG_SCOPE, "failed to perform action={err:?}");
                }
            },
        )));
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
    ///
    /// PORT NOTE: the reference's two raw accessors over these backrefs have no
    /// Rust counterpart: the non-generic `Application` cannot name `CoreApp` /
    /// `ApprtApp`, so their callers go through the `*_thunk` seams instead
    /// (`application.zig:803-811`):
    ///
    /// ```text
    /// /// Returns the core app associated with this application. This is
    /// /// not a reference-counted type so you should not store this.
    /// pub fn core(self: *Self) *CoreApp
    ///
    /// /// Returns the apprt application associated with this application.
    /// pub fn rt(self: *Self) *ApprtApp
    /// ```
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

    /// Returns the app winproto implementation.
    ///
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

    /// Returns the open URI portal implementation.
    ///
    /// PORTS: `application.zig:817-820` `openUri` — returns the open-URI portal
    /// implementation. The reference returns `*OpenURI` (a pointer into the private
    /// struct); [`OpenUri`] is itself an `Rc` handle, so a clone is the same shared
    /// object, not a copy of its state.
    #[must_use]
    pub fn open_uri(&self) -> OpenUri {
        self.imp().open_uri.clone()
    }

    /// Run the application. This is a replacement for `gio.Application.run`
    /// because we want more tight control over our event loop so we can
    /// integrate it with librustty.
    ///
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

        // Based on the actual `gio.Application.run` implementation:
        // https://github.com/GNOME/glib/blob/a8e8b742e7926e33eb635a8edceac74cf239d6ed/gio/gapplication.c#L2533

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
            log::warn!(target: LOG_SCOPE, "error registering application: {}",
                err.message());
            RunError::ApplicationRegisterFailed
        })?;

        // This just calls the `activate` signal but its part of the normal startup
        // routine so we just call it, but only if the config allows it (this allows
        // for launching Rustty in the "background" without immediately opening
        // a window).
        //
        // https://gitlab.gnome.org/GNOME/glib/-/blob/bd2ccc2f69ecfd78ca3f34ab59e42e2b462bad65/gio/gapplication.c#L2302
        //
        // (`:518-530`.)
        {
            // We need to scope any config access because once we run our
            // event loop, this can change out from underneath us.
            let initial_window = {
                let config = imp.config.borrow();
                config.as_ref().is_some_and(|c| c.get().initial_window)
            };
            if initial_window {
                self.activate();
            }
        }

        // If we are NOT the primary instance, then we never want to run.
        // This means that another instance of the GTK app is running.
        //
        // (`:532-540`.)
        if self.is_remote() {
            log::debug!(target: LOG_SCOPE, "application is remote, exiting run loop after activation");
            return Ok(());
        }

        // Tell systemd that we are ready (`:543`).
        rustty_os::systemd::notify::ready();
        // PORTS: `application.zig:564` `priv.systemd_ready = true;` (added
        // `4eb308831`, P7-W8 T3). Reloads before this point (e.g. syncing the
        // color scheme during startup) must not notify systemd — see
        // `reload_config`'s `systemd_ready` gate.
        imp.systemd_ready.set(true);

        log::debug!(target: LOG_SCOPE, "entering runloop");
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

                // If we are configured to always stay running, don't quit.
                //
                // (`:557-558`.)
                if !config.quit_after_last_window_closed {
                    break 'q false;
                }

                // If the quit timer has expired, quit.
                //
                // (`:560-564`.)
                if matches!(*imp.quit_timer.borrow(), imp::QuitTimer::Expired) {
                    log::debug!(target: LOG_SCOPE, "must_quit due to quit timer expired");
                    break 'q true;
                }

                // If we have no windows attached to our app, also quit.
                // We only do this if we don't have the closed delay set,
                // because with the closed delay set we'll exit eventually.
                //
                // (`:566-577`.)
                if config.quit_after_last_window_closed_delay.is_none()
                    && imp.requested_window.get()
                    && self.windows().is_empty()
                {
                    log::debug!(target: LOG_SCOPE, "must_quit due to no app windows");
                    break 'q true;
                }

                // No quit conditions met
                break 'q false;
            };

            if must_quit {
                // All must quit scenarios do not need confirmation.
                // Furthermore, must quit scenarios may result in a situation
                // where its unsafe to even access the app/surface memory
                // since its in the process of being freed. We must simply
                // begin our exit immediately.
                self.quit_now();
            }
        }
        log::debug!(target: LOG_SCOPE, "exiting runloop");

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

    /// Quit the application. This will start the process to stop the
    /// run loop. It will not `posix.exit`.
    ///
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

        // If our core app doesn't need to confirm quit then we
        // can exit immediately.
        //
        // (`:602-607`.)
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

        // Get the parent for our dialog
        //
        // (`:609-623`.) When there is none:
        //
        //     // If we have an active surface then we should have
        //     // a window available but in the rare case we don't we
        //     // should exit so we don't crash.
        //
        // (the reference then presents unparented, as here).
        let parent = find_active_window();

        // Show a confirmation dialog whose `close-request` routes to
        // [`Application::quit_now`] (`:625-637`; `handleCloseConfirmation`,
        // `:1552-1558`). Mirrors the window-close wiring in
        // `window.rs` `on_close_request` (`window.zig:1400-1422`).
        let dialog = CloseConfirmationDialog::new(DialogTarget::App);
        let app_weak = self.downgrade();
        dialog.connect_local("close-request", false, move |_args| {
            if let Some(app) = app_weak.upgrade() {
                app.quit_now();
            }
            None
        });

        // Show it (`:639-640`). Present keeps the dialog referenced by GTK until
        // dismissed.
        dialog.present(parent.as_ref().map(|w| w.upcast_ref::<gtk4::Widget>()));
    }

    /// PORTS: `application.zig:639-665` `quitNow`. Destroys our windows (forcing
    /// them to free, which triggers a proper shutdown sequence) then trips the run
    /// loop's exit flag. All `must_quit` scenarios route here — no confirmation.
    pub fn quit_now(&self) {
        // Get all our windows and destroy them, forcing them to free.
        //
        // (`:640-661`.)
        for window in gtk4::Window::list_toplevels() {
            // We only want to destroy our windows. These windows own
            // every other type of window that is possible so this will
            // trigger a proper shutdown sequence.
            //
            // We previously just destroyed ALL windows but this leads to
            // a double-free with the fcitx ime, because it has a nested
            // gtk.Window as a property that we don't own and it later
            // tries to free on its own. I think this is probably a bug in
            // the fcitx ime widget but still, we don't want a double free!
            //
            // `if (gobject.ext.isA(window, Window)) window.destroy();` (`:657-660`)
            // — the `downcast_ref` succeeds exactly when the toplevel is (or
            // derives) our `Window` subclass, the gtk4-rs analogue of `isA`.
            if let Some(window) = window.downcast_ref::<Window>() {
                use gtk4::prelude::GtkWindowExt as _;
                window.destroy();
            }
        }

        // Trigger our runloop exit (`:664`).
        self.imp().running.set(false);
    }

    /// apprt API to perform an action.
    ///
    /// PORTS: `application.zig:668-799` `performAction`. Dispatches an apprt action
    /// on a target through the [`action`] namespace (`application.zig:1909-2843`).
    ///
    /// The `match` is EXHAUSTIVE over every [`Action`] variant — Rust's
    /// exhaustiveness check is the faithful analogue of the reference's exhaustive
    /// `switch (action)`: a new action key is a compile error, never a silent
    /// fallthrough (`:796-798` "The unhandled case must be explicit in the switch
    /// above"). The `apprt` seam is GENERIC over the surface-handle `C`, not `dyn`
    /// (`seam.rs:11-16`), so the switch is a monomorphized `match` here.
    ///
    /// Returns whether the action was handled (reference `!bool`). The void-returning
    /// arms fall through to the trailing `Ok(true)` ("Assume it was handled",
    /// `:796-798`); the bool-returning arms return their result; the Unimplemented
    /// block warns and returns `Ok(false)` (`:777-793`).
    pub fn perform_action<C: CoreSurfaceHandle>(
        &self,
        target: Target<'_, C>,
        action: Action<'_>,
    ) -> Result<bool, ApprtError> {
        match action {
            // `.close_tab => return Action.closeTab(target, value)` (`:674`).
            Action::CloseTab(mode) => return Ok(action::close_tab(target, mode)),
            // `.close_window => return Action.closeWindow(target)` (`:675`).
            Action::CloseWindow => return Ok(action::close_window(target)),

            // `.copy_title_to_clipboard => return ...` (`:677`).
            Action::CopyTitleToClipboard => return Ok(action::copy_title_to_clipboard(target)),

            // `.config_change => try Action.configChange(self, target, value.config)` (`:679`).
            Action::ConfigChange(v) => action::config_change(self, target, v.config)?,

            // `.desktop_notification => Action.desktopNotification(self, target, value)` (`:685`).
            Action::DesktopNotification(v) => action::desktop_notification(self, target, v),

            // `.equalize_splits => return Action.equalizeSplits(target)` (`:687`).
            Action::EqualizeSplits => return Ok(action::equalize_splits(target)),

            // `.goto_split => return Action.gotoSplit(target, value)` (`:689`).
            Action::GotoSplit(v) => return Ok(action::goto_split(target, v)),
            // `.goto_window => return Action.gotoWindow(value)` (`:691`).
            Action::GotoWindow(v) => return Ok(action::goto_window(v)),
            // `.goto_tab => return Action.gotoTab(target, value)` (`:693`).
            Action::GotoTab(v) => return Ok(action::goto_tab(target, v)),

            // `.initial_size => return Action.initialSize(target, value)` (`:695`).
            Action::InitialSize(v) => return Ok(action::initial_size(target, v)),

            // `.inspector => return Action.controlInspector(target, value)` (`:697`).
            Action::Inspector(v) => return Ok(action::control_inspector(target, v)),

            // `.export_terminal_io => return try Action.exportTerminalIO(self,
            // target, value)` (`:713-717`, B9/ad96613a8).
            Action::ExportTerminalIO(v) => {
                return Ok(action::export_terminal_io(self, target, v.contents))
            }

            // `.key_sequence => return Action.keySequence(target, value)` (`:699`).
            Action::KeySequence(v) => return Ok(action::key_sequence(target, v)),
            // `.key_table => return Action.keyTable(target, value)` (`:700`).
            Action::KeyTable(v) => return Ok(action::key_table(target, v)),

            // `.mouse_over_link => Action.mouseOverLink(target, value)` (`:702`).
            Action::MouseOverLink(v) => action::mouse_over_link(target, v),
            // `.mouse_shape => Action.mouseShape(target, value)` (`:703`).
            Action::MouseShape(v) => action::mouse_shape(target, v),
            // `.mouse_visibility => Action.mouseVisibility(target, value)` (`:704`).
            Action::MouseVisibility(v) => action::mouse_visibility(target, v),

            // `.move_tab => return Action.moveTab(target, value)` (`:706`).
            Action::MoveTab(v) => return Ok(action::move_tab(target, v)),

            // `.move_tab_to_new_window => return Action.moveTabToNewWindow(
            // target)` (`:727`, f6fca9aab).
            Action::MoveTabToNewWindow => return Ok(action::move_tab_to_new_window(target)),

            // `.new_split => return Action.newSplit(target, value)` (`:708`).
            Action::NewSplit(v) => return Ok(action::new_split(target, v)),

            // `.new_tab => return Action.newTab(target, .none)` (`:728`, B5).
            Action::NewTab => return Ok(action::new_tab(target, TabOverrides::default())),

            // `.new_window => try Action.newWindow(self, <parent>, .none)` (`:712-719`).
            Action::NewWindow => action::new_window(self, target, TabOverrides::default())?,

            // `.open_config => return Action.openConfig(self, value)` (`:742`,
            // 0a183c923).
            Action::OpenConfig(v) => return Ok(action::open_config(self, v)),

            // `.open_url => Action.openUrl(self, value)` (`:723`).
            Action::OpenUrl(v) => action::open_url(self, v),

            // `.pwd => Action.pwd(target, value)` (`:725`).
            Action::Pwd(v) => action::pwd(target, v),

            // `.present_terminal => return Action.presentTerminal(target)` (`:727`).
            Action::PresentTerminal => return Ok(action::present_terminal(target)),

            // `.progress_report => return Action.progressReport(target, value)` (`:729`).
            Action::ProgressReport(v) => return Ok(action::progress_report(target, v)),

            // `.prompt_title => return Action.promptTitle(target, value)` (`:731`).
            Action::PromptTitle(v) => return Ok(action::prompt_title(target, v)),

            // `.quit => self.quit()` (`:733`).
            Action::Quit => self.quit(),

            // `.quit_timer => try Action.quitTimer(self, value)` (`:735`).
            Action::QuitTimer(v) => action::quit_timer(self, v),

            // `.reload_config => try Action.reloadConfig(self, target, value)` (`:737`).
            Action::ReloadConfig(v) => action::reload_config(self, target, v)?,

            // `.render => Action.render(target)` (`:740`).
            Action::Render => action_render(target),

            // `.resize_split => return Action.resizeSplit(target, value)` (`:742`).
            Action::ResizeSplit(v) => return Ok(action::resize_split(target, v)),

            // `.resize_window => return Action.resizeWindow(target, value)`
            // (`:771`, 219cca932).
            Action::ResizeWindow(v) => return Ok(action::resize_window(target, v)),

            // `.ring_bell => Action.ringBell(target)` (`:744`).
            Action::RingBell => action::ring_bell(target),

            // GTK has no accessibility consumer for this yet (`:746-747`). A HANDLED
            // no-op — falls through to the trailing `Ok(true)`.
            Action::SelectionChanged => {}

            // `.scrollbar => Action.scrollbar(target, value)` (`:749`).
            Action::Scrollbar(v) => action::scrollbar(target, v),

            // `.set_title => Action.setTitle(target, value)` (`:751`).
            Action::SetTitle(v) => action::set_title(target, v),
            // `.set_tab_title => return Action.setTabTitle(target, value)` (`:752`).
            Action::SetTabTitle(v) => return Ok(action::set_tab_title(target, v)),

            // `.set_window_title => return Action.setWindowTitle(target,
            // value)` (`:773`, 8b7c57c75).
            Action::SetWindowTitle(v) => return Ok(action::set_window_title(target, v)),

            // `.show_child_exited => return Action.showChildExited(target, value)` (`:754`).
            Action::ShowChildExited(v) => return Ok(action::show_child_exited(target, v)),

            // `.show_gtk_inspector => Action.showGtkInspector()` (`:756`).
            Action::ShowGtkInspector => action::show_gtk_inspector(),

            // `.size_limit => return Action.sizeLimit(target, value)` (`:758`).
            Action::SizeLimit(v) => return Ok(action::size_limit(target, v)),

            // `.toggle_maximize => Action.toggleMaximize(target)` (`:760`).
            Action::ToggleMaximize => action::toggle_maximize(target),
            // `.toggle_fullscreen => Action.toggleFullscreen(target)` (`:761`).
            Action::ToggleFullscreen(_) => action::toggle_fullscreen(target),
            // `.toggle_quick_terminal => return Action.toggleQuickTerminal(self)` (`:762`).
            Action::ToggleQuickTerminal => return Ok(action::toggle_quick_terminal(self)),
            // `.toggle_tab_overview => return Action.toggleTabOverview(target)` (`:763`).
            Action::ToggleTabOverview => return Ok(action::toggle_tab_overview(target)),
            // `.toggle_window_decorations => return ...` (`:764`).
            Action::ToggleWindowDecorations => {
                return Ok(action::toggle_window_decorations(target))
            }
            // `.toggle_command_palette => return Action.toggleCommandPalette(target)` (`:765`).
            Action::ToggleCommandPalette => return Ok(action::toggle_command_palette(target)),
            // `.toggle_split_zoom => return Action.toggleSplitZoom(target)` (`:766`).
            Action::ToggleSplitZoom => return Ok(action::toggle_split_zoom(target)),
            // `.show_on_screen_keyboard => return ...` (`:767`).
            Action::ShowOnScreenKeyboard => return Ok(action::show_on_screen_keyboard(target)),
            // `.command_finished => return Action.commandFinished(target, value)` (`:768`).
            Action::CommandFinished(v) => return Ok(action::command_finished(target, v)),
            // `.readonly => return Action.setReadonly(target, value)` (`:769`).
            Action::Readonly(v) => return Ok(action::set_readonly(target, v)),

            // `.start_search => Action.startSearch(target, value)` (`:771`).
            Action::StartSearch(v) => action::start_search(target, v),
            // `.end_search => Action.endSearch(target)` (`:772`).
            Action::EndSearch => action::end_search(target),
            // `.search_total => Action.searchTotal(target, value)` (`:773`).
            Action::SearchTotal(v) => action::search_total(target, v),
            // `.search_selected => Action.searchSelected(target, value)` (`:774`).
            Action::SearchSelected(v) => action::search_selected(target, v),

            // Unimplemented (`:777-793`): warn + `return false`. One arm per
            // reference line.
            Action::SecureInput(_)
            | Action::CloseAllWindows
            | Action::FloatWindow(_)
            | Action::ToggleVisibility
            | Action::ToggleBackgroundOpacity
            | Action::CellSize(_)
            | Action::RenderInspector
            | Action::RendererHealth(_)
            | Action::ColorChange(_)
            | Action::ResetWindowSize
            | Action::CheckForUpdates
            | Action::Undo
            | Action::Redo => {
                // log.warn("unimplemented action={}", .{action});
                //
                // S49-F5: the action NAME was omitted. `{}` on the reference's
                // `apprt.Action.Key` enum prints its tag; `Action::key()` is the
                // port's equivalent (`ActionKey` derives `Debug`).
                log::warn!(target: LOG_SCOPE, "unimplemented action={:?}", action.key());
                return Ok(false);
            }
        }

        // "Assume it was handled. The unhandled case must be explicit in the switch
        // above." (`:796-798`).
        Ok(true)
    }
}

/// PORTS: `application.zig:1909-2843` `const Action = struct { ... }` — the
/// namespace of per-action handlers that [`Application::perform_action`] dispatches
/// into. Each function reproduces the reference's `target` switch and its return
/// value one-for-one.
///
/// # INHABITED placeholders (the class family lands incrementally)
///
/// Many handlers dispatch into `GhosttySurface`/`Window`/`Tab` methods that land
/// with later L5.3 tasks (the surface-widget content methods, the dialog family
/// T23, overlays T24, the command palette T25) or with the L6 inspector. Where the
/// leaf method is not yet available, the handler still reproduces the reference's
/// `target` switch and reaches the widget/target (keeping the shape live), then
/// DEFERS the specific leaf call with a PORT NOTE and returns the reference's
/// static result. Where the leaf already exists (the `activate_action` widget
/// actions installed by T20/T21, `Window`/`Tab`/`SplitTree` navigation, the render
/// contract), the handler is fully wired.
/// `openUrlFallback` re-exported for the portal module: `portal/OpenURI.zig` reaches
/// it through its `app` backref (`OpenURI.zig:253`), which the port does not store
/// (the fallback is app-independent — see [`crate::gtk::portal::OpenUri`]'s docs), so
/// the portal calls this free function instead.
pub(crate) fn open_url_fallback(kind: rustty_apprt_api::action::OpenUrlKind, url: &[u8]) {
    action::open_url_fallback(kind, url);
}

mod action;

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
mod tests;

// PORT NOTE (P3-W7 T4): `src/os/env.zig` was DELETED upstream (`e8525c0fd`) and
// with it `internal_os.setenv`/`getenv`. The GDK/LANG mutations below are
// GENUINE process-environment mutations with no replacement in `os/`, so they
// follow upstream's own `locale.zig` precedent: file-local `extern "c"`
// declarations. Every mutation is followed by `rustty_global::sync_environ()`,
// which is what upstream's `global.init` calls after `ensureLocale()` to
// re-capture the block.
//
// These run BEFORE `gtk_init` and before any surface exists, i.e. before any
// other thread can be reading the environment — which is the only reason a
// process-wide `setenv` is defensible here at all (see
// `.port/tools/gates/check_no_test_setenv.sh` for why it is banned in tests).
mod env_c {
    use std::ffi::{c_char, c_int};

    extern "C" {
        pub fn getenv(name: *const c_char) -> *mut c_char;
        #[cfg(not(windows))]
        pub fn setenv(name: *const c_char, value: *const c_char, overwrite: c_int) -> c_int;
        #[cfg(windows)]
        pub fn _putenv_s(varname: *const c_char, value_string: *const c_char) -> c_int;
    }
}

/// `posix.getenv(key)` — the raw bytes, or `None` when unset.
fn env_getenv(key: &std::ffi::CStr) -> Option<Vec<u8>> {
    // SAFETY: `key` is NUL-terminated; the returned pointer, when non-null,
    // points at a NUL-terminated string owned by the C runtime that stays valid
    // until the next environment mutation, and we copy it immediately.
    unsafe {
        let p = env_c::getenv(key.as_ptr());
        if p.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(p).to_bytes().to_vec())
    }
}

/// `setenv(key, value, 1)` followed by `global.syncEnviron()`.
fn env_setenv(key: &std::ffi::CStr, value: &std::ffi::CStr) -> std::ffi::c_int {
    // SAFETY: both pointers are NUL-terminated C strings.
    let rc = unsafe {
        #[cfg(windows)]
        {
            env_c::_putenv_s(key.as_ptr(), value.as_ptr())
        }
        #[cfg(not(windows))]
        {
            env_c::setenv(key.as_ptr(), value.as_ptr(), 1)
        }
    };
    // We need to re-sync the environment after this completes.
    rustty_global::sync_environ();
    rc
}
