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
use super::tab::TabOverrides;
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

    /// Storage for the type-erased `present-surface` seam
    /// ([`Application::present_surface_thunk`]), named so the field type stays
    /// under the type-complexity threshold.
    type PresentSurfaceThunk = RefCell<Option<Box<dyn Fn(u64)>>>;

    /// Storage for the type-erased color-scheme seam
    /// ([`Application::color_scheme_thunk`]), named for the same reason as
    /// [`PresentSurfaceThunk`].
    type ColorSchemeThunk = RefCell<Option<Box<dyn Fn(bool)>>>;

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

        /// PORTS: `application.zig:203-205` `config_errors_dialog:
        /// WeakRef(ConfigErrorsDialog)` (SHARED / non-owning — the dialog may close
        /// on its own outside our lifecycle, so a weak ref is correct).
        ///
        /// PORT NOTE (WS20): narrowed from the skeleton's `WeakRef<glib::Object>`
        /// to the real `WeakRef<ConfigErrorsDialog>` now that the class exists.
        pub config_errors_dialog: WeakRef<ConfigErrorsDialog>,

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
        pub update_config_thunk: RefCell<
            Option<Box<dyn Fn(&rustty_config::Config) -> Result<(), rustty_core::app::Error>>>,
        >,

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

            // The D-Bus connection is only valid after GApplication startup, so the
            // portal `OpenURI` object is handed it here (`:1305-1308`).
            {
                use gio::prelude::ApplicationExt as _;
                let dbus = obj.upcast_ref::<gio::Application>().dbus_connection();
                self.open_uri.set_dbus_connection(dbus.as_ref());
            }

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
            // `self.showConfigErrorsDialog()` (`:1229`).
            self.show_config_errors_dialog();
        }

        /// PORTS: the `.app` arm of `reloadConfig` — `self.core().updateConfig(
        /// self.rt(), config)` (`application.zig:2472-2475`). Drives the freshly
        /// loaded config through the type-erased core app via
        /// [`Self::update_config_thunk`], which iterates every surface's config and
        /// (synchronously) re-dispatches the `.app` `config_change` action back into
        /// [`Self::set_config`]. A `None` thunk (core not yet bound) is a no-op.
        pub(super) fn update_config_app(
            &self,
            config: &rustty_config::Config,
        ) -> Result<(), rustty_apprt_api::ApprtError> {
            // Follow the `new_window_thunk` idiom: borrow-and-call the thunk inline.
            if let Some(thunk) = self.update_config_thunk.borrow().as_ref() {
                thunk(config).map_err(|err| {
                    log::warn!(target: LOG_SCOPE, "error updating app config err={err:?}");
                    rustty_apprt_api::ApprtError
                })?;
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
        /// The reference's `{s}` of `err.f_message orelse "«unknown»"`
        /// (`application.zig:1272,1279`).
        pub(super) fn signal_css_parsing_error(
            &self,
            section: &gtk4::CssSection,
            error: &glib::Error,
        ) {
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
                log::info!(target: LOG_SCOPE, "libxev manual backend={:?}",
                    rustty_xev::backend());
            } else {
                log::warn!(target: LOG_SCOPE, "libxev manual backend failed, using default={:?}",
                    rustty_xev::backend());
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
                ext::actions::Action::init(
                    "toggle-quick-terminal",
                    action_toggle_quick_terminal,
                    None,
                ),
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

            let (title, body) = super::bind_failed_notification(failure);
            // `Action.desktopNotification(self, .app, .{ .title, .body })`
            // (`:1721-1724`).
            action::desktop_notification(
                &self.obj(),
                Target::<super::AppTargetHandle>::App,
                rustty_apprt_api::action::DesktopNotification {
                    title: title.as_bytes(),
                    body: body.as_bytes(),
                },
            );
        }

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
                // log.warn("error reloading config: {}", .{err});
                log::warn!(target: LOG_SCOPE, "error reloading config: {err:?}");
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
            self.sync_action_accelerator(
                "win.toggle-command-palette",
                &Action::ToggleCommandPalette,
            );
            self.sync_action_accelerator("win.close", &Action::CloseWindow);
            self.sync_action_accelerator("win.new-window", &Action::NewWindow);
            self.sync_action_accelerator("win.new-tab", &Action::NewTab);
            self.sync_action_accelerator(
                "win.close-tab::this",
                &Action::CloseTab(CloseTabMode::This),
            );
            self.sync_action_accelerator("tab.close::this", &Action::CloseTab(CloseTabMode::This));
            self.sync_action_accelerator(
                "win.split-right",
                &Action::NewSplit(SplitDirection::Right),
            );
            self.sync_action_accelerator("win.split-down", &Action::NewSplit(SplitDirection::Down));
            self.sync_action_accelerator("win.split-left", &Action::NewSplit(SplitDirection::Left));
            self.sync_action_accelerator("win.split-up", &Action::NewSplit(SplitDirection::Up));
            self.sync_action_accelerator(
                "win.copy",
                &Action::CopyToClipboard(CopyToClipboard::Mixed),
            );
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
        fn sync_action_accelerator(
            &self,
            gtk_action: &str,
            action: &rustty_input::binding::Action,
        ) {
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

        /// PORTS: `application.zig:1066-1140` `loadCustomCss`. Loads each
        /// `gtk-custom-css` file into a fresh provider appended to
        /// `custom_css_providers` and registered on the display at USER priority.
        fn load_custom_css(&self) {
            use std::os::unix::ffi::OsStrExt;

            // `gdk.Display.getDefault() orelse { warn; return }` (`:1069-1072`).
            let Some(display) = gtk4::gdk::Display::default() else {
                log::warn!(target: LOG_SCOPE, "unable to get display");
                return;
            };

            // Unload the previously loaded style providers, then clear the list
            // (`:1074-1081`). Each was registered at USER priority below; drop it
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
                return;
            };
            let config = cfg.get();

            // 5 MB per-file limit (`:1101`).
            const CSS_FILE_SIZE_LIMIT: u64 = 5 * 1024 * 1024;

            // `for (config.@"gtk-custom-css".value.items) |p|` (`:1083`). Each path
            // was already expanded to absolute at config load.
            for p in &config.gtk_custom_css.value {
                // `.optional => .{ path, true }` / `.required => .{ path, false }`
                // (`:1084-1087`).
                let (path_bytes, optional) = match p {
                    rustty_config::path::Path::Optional(path) => (path, true),
                    rustty_config::path::Path::Required(path) => (path, false),
                };
                let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path_bytes));

                // Open + read the file (`:1088-1113`). A missing OPTIONAL file is
                // silent; any other open error (or a missing REQUIRED file) warns;
                // either way we skip to the next path.
                let contents = match std::fs::metadata(path) {
                    Ok(meta) if meta.len() > CSS_FILE_SIZE_LIMIT => {
                        log::warn!(target: LOG_SCOPE, "gtk-custom-css file {} was larger than {CSS_FILE_SIZE_LIMIT} bytes",
                            path.display());
                        continue;
                    }
                    Ok(_) => match std::fs::read(path) {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            let missing = err.kind() == std::io::ErrorKind::NotFound;
                            if !missing || !optional {
                                log::warn!(target: LOG_SCOPE, "error opening gtk-custom-css file {}: {err}",
                                    path.display());
                            }
                            continue;
                        }
                    },
                    Err(err) => {
                        let missing = err.kind() == std::io::ErrorKind::NotFound;
                        if !missing || !optional {
                            log::warn!(target: LOG_SCOPE, "error opening gtk-custom-css file {}: {err}",
                                path.display());
                        }
                        continue;
                    }
                };

                log::info!(target: LOG_SCOPE, "loading gtk-custom-css path={}", path.display());

                // Fresh provider, wire the parse-error logger, own it in the Vec,
                // load, and register on the display at USER priority (`:1115-1139`).
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
                css_provider.load_from_string(&String::from_utf8_lossy(&contents));
                gtk4::style_context_add_provider_for_display(
                    &display,
                    &css_provider,
                    gtk4::STYLE_PROVIDER_PRIORITY_USER,
                );
            }
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

    /// PORTS: `application.zig:930-964` `loadRuntimeCss414`. Load runtime CSS for
    /// older than GTK 4.16.
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

    /// PORTS: `application.zig:966-1064` `loadRuntimeCss416`. Load runtime CSS
    /// for GTK 4.16 and newer.
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
}

// PORTS: `application.zig:1507-1524` `handleSigusr2`. The SIGUSR2 handler
// installed by [`imp::Application::startup_signals`]. `g_unix_signal_add` calls
// it on the main loop when SIGUSR2 is delivered; returning `G_SOURCE_CONTINUE`
// keeps the handler installed.
unsafe extern "C" fn handle_sigusr2(ud: glib::ffi::gpointer) -> glib::ffi::gboolean {
    // `const self: *Self = @ptrCast(@alignCast(ud orelse return CONTINUE))` (`:1508`).
    // The userdata is the `Application` GObject pointer (`startup_signals`); a null
    // means the source outlived the app, so keep the handler installed and bail.
    if ud.is_null() {
        return glib::ffi::GTRUE;
    }

    log::info!(target: LOG_SCOPE, "received SIGUSR2, reloading configuration");

    // `Action.reloadConfig(self, .app, .{})` (`:1512-1521`). Reach the app singleton
    // (rustty's blessed `self`-equivalent accessor; the `ud` pointer is that same
    // object) and drive an `.app`-target reload directly — the reference calls the
    // `Action` helper straight, NOT through `core.performAction`. On failure we log;
    // "we should show another GUI" (`:1516-1519`) is future work.
    let app = Application::default();
    if let Err(err) = action::reload_config(
        &app,
        Target::<AppTargetHandle>::App,
        rustty_apprt_api::action::ReloadConfig::default(),
    ) {
        // log.warn("error reloading config: {}", .{err});
        // (task-14d-3: the error VALUE was dropped — the `.is_err()` discarded it.)
        log::warn!(target: LOG_SCOPE, "error reloading config: {err:?}");
    }

    // `return @intFromBool(glib.SOURCE_CONTINUE)` (`:1523`).
    glib::ffi::GTRUE
}

/// PORTS: `application.zig:1700-1719` — the pure string-assembly half of
/// `globalShortcutBindFailed`, split out so it is testable with no GApplication, no
/// D-Bus and no compositor. Returns `(title, body)`.
///
/// The body is `"<trigger label>: <detail>"`, where the detail is the
/// compositor-supplied `message` when it is non-empty and one of two translated
/// fallbacks otherwise (`:1707-1713`). The trigger label falls back to `"?"` when
/// the trigger cannot be rendered (`:1701-1705`).
///
/// PORT NOTE (byte fidelity): `failure.message` is compositor wire data
/// ([`Vec<u8>`]); it is decoded lossily HERE, at the one place it becomes
/// user-facing display text, and nowhere earlier.
///
/// PORT NOTE: the reference formats into a fixed 512-byte buffer and RETURNS
/// (raising no notification at all) when the body does not fit (`:1715-1720`,
/// `catch return`). rustty's `String` grows, so the notification is always raised;
/// a notification silently dropped for length is a defect, not behaviour worth
/// reproducing.
pub(crate) fn bind_failed_notification(failure: &BindFailed) -> (String, String) {
    // `key.labelFromTrigger(&writer, failure.trigger) catch false` → `"?"`
    // (`:1700-1705`).
    let label = {
        let mut buf = String::new();
        if crate::gtk::key::label_from_trigger(&mut buf, failure.trigger) {
            buf
        } else {
            "?".to_owned()
        }
    };

    // `if (failure.message[0] != 0) break :detail failure.message;` (`:1708`).
    let detail: String = if failure.message.is_empty() {
        if failure.revoked {
            tr(c"The keybind was revoked by the system.").to_owned()
        } else {
            tr(c"The keybind was denied by the system.").to_owned()
        }
    } else {
        String::from_utf8_lossy(&failure.message).into_owned()
    };

    (
        tr(c"Global keybind unavailable").to_owned(),
        format!("{label}: {detail}"),
    )
}

/// A zero-sized [`CoreSurfaceHandle`] used only to name the `C` type parameter of
/// [`Target::App`] from the non-generic reload entry points ([`handle_sigusr2`] and
/// [`action_reload_config`]). The `.app` target never dereferences a surface handle
/// (the reference's `.app` arms take no `core`), so this handle is never read — it
/// exists purely to satisfy the generic `Target<'_, C>`.
struct AppTargetHandle;

impl CoreSurfaceHandle for AppTargetHandle {
    fn rt_surface_ptr(&self) -> *mut c_void {
        std::ptr::null_mut()
    }
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

// PORTS: the nine `app.*` action callbacks (`application.zig:1854-2202`; B5's
// `new-tab` and B7's `ring-bell` joined the original seven). Each is
// the `gio.SimpleAction::activate` handler `ext::actions` wires with a WEAK ref to
// the target `Application` (see `ext/actions.rs`). Their bodies dispatch through
// the core app / action surface (`core_app.performAction`, `Action.newWindow`,
// `core().mailbox.push`), which reaches the type-erased `core_app`/`core()` and
// the `performAction` dispatch (Task 23) — placeholder-leaves until then, but
// REGISTERED verbatim so the action map is complete (the `startup` KAT asserts all
// nine names).

/// PORTS: `application.zig:1899-1936` `actionNewWindow` (`new-window` /
/// `new-window-command`; B5 end state — the argv parse is factored into
/// [`parse_overrides`]). Parses the optional `as` argv override, then requests
/// a new window.
fn action_new_window(
    _action: &gio::SimpleAction,
    parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `log.debug("received new window action")` (`:1904`).

    // `const overrides = overrides: { ... }` (`:1911-1930`) — parse the
    // optional `as` parameter; a parse failure collapses to null (`catch
    // null`, `:1929`).
    let overrides: Option<TabOverrides> = 'overrides: {
        // were we given a parameter? (`:1913`.)
        let Some(arguments) = parameter else {
            break 'overrides None;
        };

        // ensure that the supplied parameter is an array of strings
        // (`:1918-1925`).
        if !arguments.is_type(glib::VariantTy::STRING_ARRAY) {
            log::warn!(target: LOG_SCOPE, "parameter is of type '{}', not 'as'",
                arguments.type_().as_str(),);
            break 'overrides None;
        }

        parse_overrides(arguments.iter())
    };

    // `Action.newWindow(self, null, if (overrides) |o| .{ ... } else .none)`
    // (`:1932-1944`). Unlike `activate` (which queues a `.new_window` on the
    // core mailbox), the received action opens the window directly so the
    // overrides are honored; `parent` is null → `.app` target.
    if let Err(err) = action::new_window(
        app,
        Target::<AppTargetHandle>::App,
        overrides.unwrap_or_default(),
    ) {
        log::warn!(target: LOG_SCOPE, "unable to create new window: {err:?}");
    }
}

/// Handle `app.new-tab` GTK action
///
/// PORTS: `application.zig:1946-2011` `actionNewTab` (B5, 034506f14): the
/// `(tas)` parameter carries the target surface ID (0 == focused) and the CLI
/// argument array. The surface lookup + `Action.newTab`/`Action.newWindow`
/// fan-out needs the typed core app, so it lives in the monomorphized
/// `new_tab_thunk` installed by [`Application::bind_core_app`] (the
/// `present_surface_thunk` precedent); unbound core → no-op
/// (placeholder-leaf doctrine).
fn action_new_tab(
    _action: &gio::SimpleAction,
    parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `log.debug("received new tab action")` (`:1951`).

    // `const surface_id_, const overrides = result: { ... }` (`:1959-1993`).
    let (surface_id, overrides) = {
        // were we given a parameter? (`:1961-1964`.)
        let Some(parameter) = parameter else {
            log::warn!(target: LOG_SCOPE, "app.new-tab did not receive a parameter");
            return;
        };

        // `log.warn("got parameter: {s}", .{parameter.getTypeString()})`
        // (`:1966` — the reference logs this unconditionally).
        log::warn!(target: LOG_SCOPE, "got parameter: {}", parameter.type_().as_str());

        // ensure that the supplied parameter is an array of strings
        // (`:1968-1978` — the `(tas)` tuple check).
        let Ok(tas) = glib::VariantTy::new("(tas)") else {
            return;
        };
        if !parameter.is_type(tas) {
            log::warn!(target: LOG_SCOPE, "parameter is of type '{}', not '(tas)'",
                parameter.type_().as_str(),);
            return;
        }

        // `parameter.get("(tas)", &surface_id, &arguments_it_)` (`:1980-1986`).
        let surface_id = parameter.child_value(0).get::<u64>().unwrap_or(0);
        let arguments = parameter.child_value(1);

        // `const overrides = parseOverrides(alloc, arguments_it) catch return;`
        // (`:1988`).
        let Some(overrides) = parse_overrides(arguments.iter()) else {
            return;
        };

        // `break :result .{ if (surface_id == 0) null else surface_id,
        // overrides };` (`:1990`).
        (
            if surface_id == 0 {
                None
            } else {
                Some(surface_id)
            },
            overrides,
        )
    };

    // The surface lookup + dispatch (`:1995-2010`), monomorphized in
    // [`imp::Application::new_tab_thunk`].
    if let Some(thunk) = app.imp().new_tab_thunk.borrow().as_ref() {
        thunk(surface_id, overrides);
    }
}

/// PORTS: `application.zig:2013-2098` `parseOverrides` (B5, 3650bef4e — the
/// factored `new-window`/`new-tab` argv override parser; adds
/// `--shell-integration=`). Returns `None` where the reference returns an
/// error (`Allocator.Error || error{ValueRequired}`); the infallible-alloc
/// arms keep only the shape.
fn parse_overrides(arguments_it: glib::VariantIter) -> Option<TabOverrides> {
    use rustty_config::command::Command;
    use rustty_config::value_types::ShellIntegration;

    // `var args: std.ArrayList([:0]const u8) = .empty;` (`:2019`).
    let mut args: Vec<Vec<u8>> = Vec::new();

    let mut working_directory: Option<String> = None;
    let mut title: Option<String> = None;
    let mut command: Option<Command> = None;
    // `var parsed_shell_integration: struct { @"shell-integration":
    // ?configpkg.Config.ShellIntegration = null } = .{};` (`:2024-2026`) — the
    // one-field struct exists only to give `cli.args.parseIntoField` a target;
    // the bare optional is its Rust collapse.
    let mut parsed_shell_integration: Option<ShellIntegration> = None;

    // `var e_seen: bool = false;` (`:2031`.)
    let mut e_seen = false;

    // `while (arguments_it.nextValue()) |value|` (`:2034`) — iterate the array
    // elements.
    for (i, value) in arguments_it.enumerate() {
        // just to be sure (`:2037-2038`).
        let Some(str) = value.str() else {
            continue;
        };

        // `log.debug("argument: {d} {s}", .{ i, str });` (`:2044`).
        let _ = i;

        if e_seen {
            // `args.append(arena_alloc, copy)` (`:2046-2056`) — everything
            // after `-e` is a direct-exec argument. The two alloc-failure
            // `return err` arms (`:2048,2053`) are infallible here.
            args.push(str.as_bytes().to_vec());
            continue;
        }

        // `if (std.mem.eql(u8, str, "-e"))` (`:2058-2061`).
        if str == "-e" {
            e_seen = true;
            continue;
        }

        // `if (std.mem.cutPrefix(u8, str, "--command="))` (`:2063-2071`) —
        // a parse failure warns and RETURNS the error (the B5 rework; the old
        // handler continued).
        if let Some(v) = str.strip_prefix("--command=") {
            let mut cmd = Command::Shell(Vec::new());
            match cmd.parse_cli(Some(v.as_bytes())) {
                Ok(()) => command = Some(cmd),
                Err(err) => {
                    log::warn!(target: LOG_SCOPE, "unable to parse command: {err:?}");
                    return None;
                }
            }
            continue;
        }
        // `if (std.mem.cutPrefix(u8, str, "--shell-integration="))`
        // (`:2072-2084`): `cli.args.parseIntoField(..., "shell-integration",
        // trim(v))` — for a bare optional enum field that is
        // `std.meta.stringToEnum` (== `string_to_enum`, the config name-map);
        // a parse failure warns and CONTINUES (`:2079-2082`), unlike the
        // command arm.
        if let Some(v) = str.strip_prefix("--shell-integration=") {
            match ShellIntegration::string_to_enum(v.trim().as_bytes()) {
                Some(parsed) => parsed_shell_integration = Some(parsed),
                None => {
                    log::warn!(target: LOG_SCOPE, "unable to parse shell integration {v}: InvalidValue")
                }
            }
            continue;
        }
        // `if (std.mem.cutPrefix(u8, str, "--working-directory="))`
        // (`:2085-2091`).
        if let Some(v) = str.strip_prefix("--working-directory=") {
            working_directory = Some(v.trim().to_string());
            continue;
        }
        // `if (std.mem.cutPrefix(u8, str, "--title="))` (`:2092-2097`).
        if let Some(v) = str.strip_prefix("--title=") {
            title = Some(v.trim().to_string());
            continue;
        }
    }

    // `if (args.items.len > 0) command = .{ .direct = args.items };`
    // (`:2101-2105`).
    if !args.is_empty() {
        command = Some(Command::Direct(args));
    }

    // `return .{ .command, .shell_integration, .working_directory, .title };`
    // (`:2107-2112`).
    Some(TabOverrides {
        command,
        shell_integration: parsed_shell_integration,
        working_directory,
        title,
    })
}

/// PORTS: `application.zig:2114-2148` `actionOpenConfig` (`open-config`;
/// 0a183c923 — the string parameter selects the `os-open` / `new-window`
/// target).
fn action_open_config(
    _action: &gio::SimpleAction,
    parameter: Option<&glib::Variant>,
    app: &Application,
) {
    use rustty_apprt_api::action::OpenConfig;

    // `const target: CoreApp.Message.OpenConfig = target: { ... }` (`:2119-
    // 2146`): the `StaticStringMap` lookup over the two detailed-action target
    // strings, defaulting to `.os_open` on every malformed-parameter path.
    //
    // PORT NOTE: the target type is the apprt-api `action::OpenConfig` (the
    // same two-variant `os_open`/`new_window` enum as the reference's
    // `CoreApp.Message.OpenConfig`) because rustty routes the parsed target
    // straight to [`action::open_config`] rather than through the core
    // mailbox; see the routing note below.
    let target: OpenConfig = 'target: {
        let Some(parameter) = parameter else {
            log::warn!(target: LOG_SCOPE, "no parameter provided to app.open-config action");
            break 'target OpenConfig::OsOpen;
        };

        if !parameter.is_type(glib::VariantTy::STRING) {
            log::warn!(target: LOG_SCOPE, "parameter to app.open-config action is of type '{}' not 's'",
                parameter.type_().as_str(),);
            break 'target OpenConfig::OsOpen;
        }

        let Some(str) = parameter.str() else {
            break 'target OpenConfig::OsOpen;
        };

        // `target_map.get(str) orelse { warn; break :target .os_open; }`
        // (`:2120-2123,2141-2145`).
        match str {
            "os-open" => OpenConfig::OsOpen,
            "new-window" => OpenConfig::NewWindow,
            _ => {
                log::warn!(target: LOG_SCOPE, "'{str}' is not configured as a target for the \
                     app.open-config action");
                OpenConfig::OsOpen
            }
        }
    };

    // `self.core().mailbox.push(global.io(), .{ .open_config = target },
    // .forever)` (`:2147`) is drained to `rt_app.performAction(.app,
    // .open_config, <target>)` — a pure passthrough. rustty's
    // `action::open_config` is self-contained (config path + `open_url` /
    // edit-command window; needs no core mailbox/surface), so route it
    // directly WITH the parsed target, the same precedent as
    // [`action_reload_config`].
    let _ = action::open_config(app, target);
}

/// PORTS: `application.zig:2170-2202` `actionRingBell` (B7, f0f3f4d8d — the
/// application-owned audio bell; the surface's `ringBell` activates
/// `app.ring-bell`, `surface.rs` `ring_bell`).
fn action_ring_bell(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    app: &Application,
) {
    use std::os::unix::ffi::OsStrExt;

    let Some(config) = app.get_config() else {
        return;
    };
    let config = config.get();

    // Do our sound
    // `if (config.@"bell-features".audio) audio: { ... }` (`:2178-2201`).
    if !config.bell_features.audio {
        return;
    }

    // `const config_path = config.@"bell-audio-path" orelse break :audio;`
    // (`:2179-2183`): the `Path` union carries the optional/required flag.
    let Some(config_path) = config.bell_audio_path.as_ref() else {
        return;
    };
    let (path_bytes, required) = match config_path {
        rustty_config::path::Path::Optional(p) => (p.as_slice(), false),
        rustty_config::path::Path::Required(p) => (p.as_slice(), true),
    };

    // `std.math.clamp(config.@"bell-audio-volume", 0.0, 1.0)` (`:2185-2189`).
    let volume = config.bell_audio_volume.clamp(0.0, 1.0);

    let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path_bytes));

    // Reuse one MediaFile per application (rebuilt only when the path
    // changes) so each bell replays the same pipeline instead of
    // leaking a fresh one. Assign unconditionally: bellMediaFile frees
    // any stale MediaFile and returns the current slot value (possibly
    // null if the path is now inaccessible), so priv.bell_media never
    // dangles. (`:2191-2200`.)
    let imp = app.imp();
    let current = imp.bell_media.borrow_mut().take();
    let media_file = crate::gtk::media::bell_media_file(current, path, required);
    if let Some(media_file) = &media_file {
        crate::gtk::media::play_bell(media_file, volume);
    }
    imp.bell_media.replace(media_file);
}

/// PORTS: `application.zig:1819-1850` `actionPresentSurface` (`present-surface`,
/// `t`).
fn action_present_surface(
    _action: &gio::SimpleAction,
    parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `const parameter = parameter_ orelse return;` (`:1824`).
    let Some(parameter) = parameter else {
        return;
    };

    // Make sure that we've received a u64 from the system (`:1826-1832`). Zig
    // builds a `VariantType.newFor(u64)` and checks `Variant.isOfType`; the glib
    // Rust binding folds both into a typed `get::<u64>()` that returns `None`
    // when the variant is not of type `t` (`"t"`, the u64 type string).
    let Some(surface_id) = parameter.get::<u64>() else {
        return;
    };

    // Convert the u64 to a core surface by using it as a surface ID. A value of
    // zero means that there was no target surface for the notification so we
    // don't focus any surface (`:1834-1838`).
    if surface_id == 0 {
        return;
    }

    // `self.core().findSurfaceByID(surface_id)` + push a `.surface_message`/
    // `.present_surface` envelope (`:1839-1849`). The monomorphized lookup+push
    // lives in the [`imp::Application::present_surface_thunk`], installed by
    // [`Application::bind_core_app`]; unbound core → no-op (placeholder-leaf).
    if let Some(thunk) = app.imp().present_surface_thunk.borrow().as_ref() {
        thunk(surface_id);
    }
}

/// PORTS: `application.zig:1687-1696` `actionQuit` (`quit`).
fn action_quit(_action: &gio::SimpleAction, _parameter: Option<&glib::Variant>, app: &Application) {
    // `priv.core_app.performAction(self.rt(), .quit)` (`:1693`). The core `.quit`
    // action is a pure passthrough back to the apprt (`App.zig:425`), so it
    // collapses to the direct `Application::quit` the `Action::Quit` performAction
    // arm takes (cf. [`action_reload_config`]).
    app.quit();
}

/// PORTS: `application.zig:1665-1674` `actionReloadConfig` (`reload-config`).
fn action_reload_config(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `priv.core_app.performAction(self.rt(), .reload_config)` (`:1671`). The core
    // app's `.reload_config` scoped action is a pure passthrough back to the apprt
    // (`App.zig:428`, `rt_app.performAction(.app, .reload_config, .{})`), so it
    // collapses to the same `.app`-target reload the SIGUSR2 handler performs. Route
    // it directly. On failure, log (`:1672`).
    if let Err(err) = action::reload_config(
        app,
        Target::<AppTargetHandle>::App,
        rustty_apprt_api::action::ReloadConfig::default(),
    ) {
        // log.warn("error reloading config: {}", .{err});
        // (task-14d-3: the error VALUE was dropped — the `.is_err()` discarded it.)
        log::warn!(target: LOG_SCOPE, "error reloading config: {err:?}");
    }
}

/// PORTS: `application.zig:1676-1685` `actionToggleQuickTerminal`
/// (`toggle-quick-terminal`).
fn action_toggle_quick_terminal(
    _action: &gio::SimpleAction,
    _parameter: Option<&glib::Variant>,
    app: &Application,
) {
    // `priv.core_app.performAction(self.rt(), .toggle_quick_terminal)` (`:1682`).
    // The core `.toggle_quick_terminal` action is a pure passthrough back to the
    // apprt (`application.zig:762` → `Action.toggleQuickTerminal(self)`), so it
    // collapses to the same direct `action::toggle_quick_terminal` route the
    // `Action::ToggleQuickTerminal` performAction arm takes (cf.
    // [`action_reload_config`]). On failure the toggle just returns `false`.
    let _ = action::toggle_quick_terminal(app);
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

    // `priv.open_uri.deinit()` (`:440`) — cancels the cleanup timer, unsubscribes
    // every in-flight `Response`, drops the entries and releases the D-Bus
    // connection. Idempotent (its own `alive` flag).
    priv_.open_uri.deinit();

    // `priv.global_shortcuts.unref()` (`:441`) — clearing the field drops the ref.
    priv_.global_shortcuts.replace(None);

    // Drop the queued-window / tick / confirm-quit thunks (rustty-only seams) so
    // they release their captured `core_app`/`rt_app` pointers; the pointers are
    // BACKREFs (never owned here).
    priv_.new_window_thunk.replace(None);
    priv_.new_tab_thunk.replace(None);
    priv_.tick_thunk.replace(None);
    priv_.needs_confirm_quit_thunk.replace(None);
    priv_.update_config_thunk.replace(None);
    priv_.color_scheme_thunk.replace(None);
    priv_.perform_all_action_thunk.replace(None);

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
/// GLib/GObject/GTK log messages through rustty's logging — the `log::` facade
/// under the reference's own `glib` scope (`GLIB_LOG_SCOPE`) — rather than
/// letting GLib dump them straight to stderr unstructured. Registered once via
/// [`glib::log_set_writer_func`] in
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
        glib::LogLevel::Error => {
            log::error!(target: GLIB_LOG_SCOPE, "ERROR: {domain}: {message}");
        }
        glib::LogLevel::Critical => {
            log::error!(target: GLIB_LOG_SCOPE, "CRITICAL: {domain}: {message}");
        }
        // warning -> warn (`application.zig:92-95`).
        glib::LogLevel::Warning => {
            log::warn!(target: GLIB_LOG_SCOPE, "WARNING: {domain}: {message}");
        }
        // message + info -> info (`application.zig:96-103`).
        glib::LogLevel::Message => {
            log::info!(target: GLIB_LOG_SCOPE, "MESSAGE: {domain}: {message}");
        }
        glib::LogLevel::Info => {
            log::info!(target: GLIB_LOG_SCOPE, "INFO: {domain}: {message}");
        }
        // debug -> debug (`application.zig:104-107`).
        glib::LogLevel::Debug => {
            log::debug!(target: GLIB_LOG_SCOPE, "DEBUG: {domain}: {message}");
        }
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
        log::warn!(target: LOG_SCOPE, "setting GDK_DEBUG={value}");
        if let Ok(v) = CString::new(value) {
            env_setenv(c"GDK_DEBUG", &v);
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
        log::warn!(target: LOG_SCOPE, "setting GDK_DISABLE={value}");
        if let Ok(v) = CString::new(value) {
            env_setenv(c"GDK_DISABLE", &v);
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
            let old_language = env_getenv(c"LANG").and_then(|v| CString::new(v).ok());
            if let Some(language) = &config.language {
                if let Ok(lang) = CString::new(language.clone()) {
                    env_setenv(c"LANG", &lang);
                }
            }
            old_language
        };

        // Set the gettext global domain; warn-and-continue on failure — a failure
        // shouldn't stop startup, translations just may not resolve
        // (`application.zig:288-293`).
        if let Err(err) = rustty_os::i18n::initGlobalDomain() {
            log::warn!(target: LOG_SCOPE, "i18n initialization failed error={err:?}");
        }

        // Setup our GTK init env vars (`application.zig:296-305`), then libadwaita
        // UNRESTORED SITES (task-14d-3) — this file's residual deficit, each a
        // `catch` arm of a call that is INFALLIBLE in this port, so restoring
        // any of them would put a diagnostic on a success path:
        //   `:114` `UNKNOWN: {s}: {s}` — gtk4-rs decodes the level flags into a
        //          closed `glib::LogLevel` with no unknown variant.
        //   `:314` `error setting GTK environment variables err={}` — see below.
        //   `:1258` `accelerator somehow longer than 1024 bytes: {f}` — the
        //          reference formats into a fixed 1024-byte stack buffer; this
        //          port formats into a `String`.
        //   `:1304` `out of memory loading runtime CSS, no runtime CSS applied`
        //          and `:1310` `failed to load custom CSS, ...` —
        //          `load_runtime_css`/`load_custom_css` return `()`.
        //   `:1804` `error toggling quick terminal err={}` and `:1815`
        //          `error quitting err={}` — both collapse to a direct call
        //          that cannot fail (see each action fn's own PORT NOTE).
        //   `:1998 :2002 :2037 :2044 :2814` — Zig `dupeZ`/`ArrayList.append`/
        //          `bufPrint` allocation failures; the Rust equivalents are
        //          infallible.
        //   `:2197` `unable to load resources` — gresource registration is
        //          compiled in and cannot fail at runtime here.
        //   `:3024` `unable to resize split, out of memory` — same class.
        //
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
                log::warn!(target: LOG_SCOPE, "invalid 'class' in config, ignoring");
            }
            build_info::APPLICATION_ID.to_owned()
        };

        // The GDK display, or exit — there is no scenario we expect this to be
        // null, and we don't want null checks everywhere (`application.zig:337-342`).
        let Some(display) = gdk4::Display::default() else {
            log::warn!(target: LOG_SCOPE, "gdk display is null, exiting");
            std::process::exit(1);
        };

        // Windowing-protocol logic, with the fallback to `.none` on error so we
        // can still launch (`application.zig:345-358`).
        let winproto = match winproto::App::init(&display, &app_id, &config) {
            Ok(wp) => wp,
            Err(err) => {
                log::warn!(target: LOG_SCOPE, "error initializing windowing protocol err={err}");
                winproto::App::default()
            }
        };
        log::debug!(target: LOG_SCOPE, "windowing protocol={:?}", winproto.protocol());

        log::debug!(target: LOG_SCOPE, "creating GTK application id={app_id} single-instance={single_instance}");

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
                        if !action::new_tab(Target::Surface(unsafe { &*surface }), overrides) {
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
                    if let Err(_err) =
                        unsafe { &mut *core_app }.colorSchemeEvent(unsafe { &*rt_app }, scheme)
                    {
                        // log.warn("error updating app color scheme err={}", .{err});
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
                    if let Err(_err) = unsafe { &mut *core }.colorSchemeCallback(scheme) {
                        // log.warn(
                        //     "unable to tell surface about color scheme change err={}",
                        //     .{err},
                        // );
                    }
                }
            })));

        // The global-shortcut dispatch (`globalShortcutTrigger`,
        // `application.zig:1655-1663`): `core_app.performAllAction(rt_app,
        // action)`. `performAllAction` takes `&mut App<A>` and `rt_app: &A`; both
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
                if let Err(err) =
                    unsafe { &mut *core_app }.performAllAction(unsafe { &*rt_app }, action)
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

    /// PORTS: `application.zig:817-820` `openUri` — returns the open-URI portal
    /// implementation. The reference returns `*OpenURI` (a pointer into the private
    /// struct); [`OpenUri`] is itself an `Rc` handle, so a clone is the same shared
    /// object, not a copy of its state.
    #[must_use]
    pub fn open_uri(&self) -> OpenUri {
        self.imp().open_uri.clone()
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
            log::warn!(target: LOG_SCOPE, "error registering application: {}",
                err.message());
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
            log::debug!(target: LOG_SCOPE, "application is remote, exiting run loop after activation");
            return Ok(());
        }

        // Tell systemd that we are ready (`:543`).
        rustty_os::systemd::notify::ready();

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

                // Configured to always stay running → don't quit (`:557-558`).
                if !config.quit_after_last_window_closed {
                    break 'q false;
                }

                // Quit timer expired → quit (`:560-564`).
                if matches!(*imp.quit_timer.borrow(), imp::QuitTimer::Expired) {
                    log::debug!(target: LOG_SCOPE, "must_quit due to quit timer expired");
                    break 'q true;
                }

                // No windows attached → quit, but only when no closed-delay is set
                // (with a delay we exit via the timer instead) (`:566-577`).
                if config.quit_after_last_window_closed_delay.is_none()
                    && imp.requested_window.get()
                    && self.windows().is_empty()
                {
                    log::debug!(target: LOG_SCOPE, "must_quit due to no app windows");
                    break 'q true;
                }

                break 'q false;
            };

            if must_quit {
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
        // rare case there is none we present unparented so we don't crash.
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
        // Get all toplevels and destroy ONLY our own `Window`s (`:640-661`).
        //
        // The fcitx-IME double-free note (`:652-656`, load-bearing): we previously
        // destroyed ALL toplevels, but that double-frees with the fcitx IME, which
        // owns a nested `gtk.Window` (as a property) that we do not own and that it
        // later frees itself. So we destroy only windows that are our own `Window`
        // subclass (`gobject.ext.isA(window, Window)`), which own every other window
        // type and thus trigger a proper cascading shutdown.
        for window in gtk4::Window::list_toplevels() {
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

mod action {
    use glib::prelude::*;
    use glib::subclass::prelude::*;
    use gtk4::prelude::*;
    use rustty_apprt_api::action::{
        CloseTabMode, GotoSplit, GotoTab, GotoWindow, PromptTitle, ResizeSplitDirection,
        SplitDirection,
    };
    use rustty_apprt_api::{ApprtError, CoreSurfaceHandle, Surface as _, Target};
    use rustty_datastruct::split_tree::{Direction, Goto, SpatialDirection};

    use super::super::split_tree::SplitTree;
    use super::super::surface::{CoreSurface, Size, Surface as GhosttySurface};
    use super::super::tab::{Tab, TabOverrides};
    use super::super::window::{SelectTab, Window};
    use super::{ext, AppTargetHandle, Application, Config, CoreConfig, LOG_SCOPE};
    use crate::gtk::i18n::tr;
    use crate::gtk::surface::Surface as ApprtSurface;

    /// Reach the [`GhosttySurface`] widget for a `.surface` target — the reference's
    /// `v.rt_surface.surface` / `v.rt_surface.gobj()`. Returns `None` if the backref
    /// is not yet wired (pre-`constructed`). Same liveness contract as
    /// [`action_render`](super::action_render): the widget is alive during the
    /// synchronous app-thread dispatch.
    fn widget<C: CoreSurfaceHandle>(v: &C) -> Option<GhosttySurface> {
        let rt = v.rt_surface_ptr().cast::<ApprtSurface>();
        if rt.is_null() {
            return None;
        }
        // SAFETY: for a `.surface` action the core app hands the LIVE core surface
        // whose `rt_surface` points at the pinned apprt `Surface` embedded in the
        // widget's private (memory `mirihar-surface-pin`), read on the app thread.
        unsafe { (*rt).widget() }
    }

    /// `pub fn closeTab(target, value) bool` (`:1910-1923`). Activates the
    /// `tab.close` widget action (installed by `Tab::init_action_map`, `tab.rs:200`)
    /// with the mode tag-name string.
    pub(super) fn close_tab<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: CloseTabMode,
    ) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let mode = match value {
                    CloseTabMode::This => "this",
                    CloseTabMode::Other => "other",
                    CloseTabMode::Right => "right",
                };
                w.activate_action("tab.close", Some(&mode.to_variant()))
                    .is_ok()
            }
        }
    }

    /// `pub fn closeWindow(target) bool` (`:1925-1933`). Activates `win.close`
    /// (`window.rs:660`).
    pub(super) fn close_window<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                w.activate_action("win.close", None).is_ok()
            }
        }
    }

    /// `pub fn copyTitleToClipboard(target) bool` (`:1935-1941`).
    pub(super) fn copy_title_to_clipboard<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => false,
            // `.surface => |v| v.rt_surface.gobj().copyTitleToClipboard()` (`:1939`).
            Target::Surface(v) => match widget(v) {
                Some(w) => w.copy_title_to_clipboard(),
                None => false,
            },
        }
    }

    /// `pub fn configChange(self, target, new_config) !void` (`:1943-1957`). Wraps
    /// the config in a [`Config`] GObject (which clones it) and applies it to the
    /// target.
    pub(super) fn config_change<C: CoreSurfaceHandle>(
        app: &Application,
        target: Target<'_, C>,
        new_config: &CoreConfig,
    ) -> Result<(), ApprtError> {
        // Wrap our config in a GObject. This clones it (`:1949-1951`).
        let config_obj = Config::new(new_config);
        match target {
            // `.surface => |core| core.rt_surface.surface.setConfig(config_obj)`
            // (`:1953-1954`). Push the new config onto the LIVE surface widget via its
            // `config` property setter (generated by `#[property(get, set)]` on
            // `Surface::config`), which emits `notify::config` → `Surface::on_notify_config`
            // so the surface re-applies (colors, font size, palette, overlays).
            Target::Surface(v) => {
                if let Some(w) = widget(v) {
                    w.set_config(Some(config_obj));
                }
            }
            // `.app => self.setConfig(config_obj)` (`:1955`).
            Target::App => {
                app.imp().set_config(config_obj);
            }
        }
        Ok(())
    }

    /// `pub fn desktopNotification(self, target, n) void` (`:1959-2001`).
    pub(super) fn desktop_notification<C: CoreSurfaceHandle>(
        app: &Application,
        target: Target<'_, C>,
        n: rustty_apprt_api::action::DesktopNotification<'_>,
    ) {
        use gio::prelude::ApplicationExt as _;

        // `.surface => |v| { v.rt_surface.gobj().sendDesktopNotification(n.title,
        // n.body); return; }` (`:1965-1968`).
        if let Target::Surface(v) = target {
            if let Some(w) = widget(v) {
                w.send_desktop_notification(n.title, n.body);
            }
            return;
        }

        // `.app => {}` then the app-level fallback (`:1963,1971-2000`): build a
        // standalone `gio.Notification` presenting no specific surface (target id
        // 0).
        //
        // Set a default title if we don't already have one (`:1971-1974`). PORT
        // NOTE (R2 identity): the reference's literal `"Ghostty"` becomes the
        // product display name [`rustty_identity::APP_NAME_TITLECASE`].
        let title_owned;
        let title_str: &str = if n.title.is_empty() {
            rustty_identity::APP_NAME_TITLECASE
        } else {
            title_owned = String::from_utf8_lossy(n.title);
            &title_owned
        };
        let body_str = String::from_utf8_lossy(n.body);

        let notification = gio::Notification::new(title_str);
        notification.set_body(Some(&body_str));

        // `gio.ThemedIcon.new("com.mitchellh.ghostty")` (`:1980`). PORT NOTE (R2):
        // the themed-icon name IS the application id
        // ([`build_info::BASE_APPLICATION_ID`], the single source of truth).
        let icon = gio::ThemedIcon::new(crate::gtk::build_info::BASE_APPLICATION_ID);
        notification.set_icon(&icon);

        // `setDefaultActionAndTargetValue("app.present-surface",
        // glib.Variant.newUint64(0))` (`:1983-1986`).
        notification
            .set_default_action_and_target_value("app.present-surface", Some(&0u64.to_variant()));

        // Notification ID = body so an identical follow-up replaces the prior one
        // (`:1988-1999`).
        app.upcast_ref::<gio::Application>()
            .send_notification(Some(&body_str), &notification);
    }

    /// `pub fn equalizeSplits(target) bool` (`:2003-2015`). Activates
    /// `split-tree.equalize` (`split_tree.rs:307`).
    pub(super) fn equalize_splits<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "equalize splits to app is unexpected");
                false
            }
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                w.activate_action("split-tree.equalize", None).is_ok()
            }
        }
    }

    /// `pub fn gotoSplit(target, to) bool` (`:2017-2039`). Reaches the enclosing
    /// [`SplitTree`] and calls [`SplitTree::goto`], mapping the apprt direction to a
    /// tree [`Goto`] exactly as the reference does (`:2028-2036`).
    pub(super) fn goto_split<C: CoreSurfaceHandle>(target: Target<'_, C>, to: GotoSplit) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(tree) = ext::ancestor::<SplitTree>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a split tree, ignoring goto_split");
                    return false;
                };
                tree.goto(match to {
                    GotoSplit::Previous => Goto::PreviousWrapped,
                    GotoSplit::Next => Goto::NextWrapped,
                    GotoSplit::Up => Goto::Spatial(SpatialDirection::Up),
                    GotoSplit::Down => Goto::Spatial(SpatialDirection::Down),
                    GotoSplit::Left => Goto::Spatial(SpatialDirection::Left),
                    GotoSplit::Right => Goto::Spatial(SpatialDirection::Right),
                })
            }
        }
    }

    /// `pub fn gotoTab(target, tab) bool` (`:2049-2064`).
    pub(super) fn goto_tab<C: CoreSurfaceHandle>(target: Target<'_, C>, tab: GotoTab) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(window) = ext::ancestor::<Window>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a window, ignoring goto_tab");
                    return false;
                };
                // `window.selectTab(switch (tab) { ... })` (`:2058-2063`). The special
                // negative sentinels map to the directional variants; any other value is
                // the (1-indexed) tab number carried by `GotoTab`'s raw `i32`.
                window.select_tab(match tab {
                    GotoTab::PREVIOUS => SelectTab::Previous,
                    GotoTab::NEXT => SelectTab::Next,
                    GotoTab::LAST => SelectTab::Last,
                    GotoTab(n) => SelectTab::N(n as usize),
                })
            }
        }
    }

    /// `pub fn gotoWindow(direction) bool` (`:2067-2101`). Iterates our toplevel
    /// windows forward/backward (wrapping) and presents+focuses the first visible,
    /// non-active `Window`. Behavioral port of the `glib.List` prev/next walk.
    pub(super) fn goto_window(direction: GotoWindow) -> bool {
        // Collect our own `Window` toplevels in toplevel order.
        let windows: Vec<Window> = gtk4::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk4::Window>().ok())
            .filter_map(|w| w.downcast::<Window>().ok())
            .collect();
        if windows.is_empty() {
            return false;
        }

        // The window we start from is the active one (else the list head, `:2073-2075`).
        let start = windows
            .iter()
            .position(|w| w.upcast_ref::<gtk4::Window>().is_active())
            .unwrap_or(0);

        let n = windows.len();
        // Walk forward/backward once around the ring (skipping the start slot).
        for step in 1..=n {
            let idx = match direction {
                GotoWindow::Next => (start + step) % n,
                GotoWindow::Previous => (start + n - (step % n)) % n,
            };
            if idx == start {
                continue;
            }
            if goto_window_maybe(&windows[idx]) {
                return true;
            }
        }
        false
    }

    /// `fn gotoWindowMaybe(gtk_window) bool` (`:2103-2119`). Presents + focuses the
    /// active surface of a visible, non-active `Window`.
    fn goto_window_maybe(window: &Window) -> bool {
        let gtk_window = window.upcast_ref::<gtk4::Window>();
        if gtk_window.is_active() {
            return false;
        }
        if !window.is_visible() {
            return false;
        }
        let Some(surface) = window.get_active_surface() else {
            return false;
        };
        gtk_window.present();
        surface.grab_focus();
        true
    }

    /// `pub fn initialSize(target, value) bool` (`:2121-2135`).
    pub(super) fn initial_size<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::InitialSize,
    ) -> bool {
        match target {
            Target::App => false,
            // `.surface => |core| { surface.setDefaultSize(.{ .width, .height });
            // return true; }` (`:2128-2133`). Sets the `default-size` property; the
            // owning window reads it when presenting.
            Target::Surface(v) => match widget(v) {
                Some(w) => {
                    w.set_default_size(Some(Size {
                        width: value.width,
                        height: value.height,
                    }));
                    true
                }
                None => false,
            },
        }
    }

    /// `pub fn mouseOverLink(target, value) void` (`:2138-2148`).
    pub(super) fn mouse_over_link<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::MouseOverLink<'_>,
    ) {
        match target {
            Target::App => log::warn!(target: LOG_SCOPE, "mouse over link to app is unexpected"),
            Target::Surface(v) => {
                // PORTS: `application.zig:2151-2153`
                // `surface.rt_surface.gobj().setMouseHoverUrl(if (value.url.len > 0) value.url else null)`.
                // Sets the widget's `mouse-hover-url` property (the generated setter fires
                // `notify::mouse-hover-url`, driving `on_notify_mouse_hover_url` which
                // reveals the URL overlay label). Empty url clears it (`null`).
                if let Some(w) = widget(v) {
                    let url = if value.url.is_empty() {
                        None
                    } else {
                        Some(String::from_utf8_lossy(value.url).into_owned())
                    };
                    w.set_mouse_hover_url(url);
                }
            }
        }
    }

    /// `pub fn mouseShape(target, shape) void` (`:2158-2166`).
    pub(super) fn mouse_shape<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        shape: rustty_vt::mouse::Shape,
    ) {
        match target {
            Target::App => log::warn!(target: LOG_SCOPE, "mouse shape to app is unexpected"),
            Target::Surface(v) => {
                // PORTS: `application.zig:2164` `surface.rt_surface.gobj().setMouseShape(shape)`.
                // Sets the widget's `mouse-shape` property (carried as the shape's `i32`
                // discriminant, see the `mouse-shape` field PORT NOTE). The generated setter
                // fires `notify::mouse-shape`, driving `on_notify_mouse_shape` which sets the
                // GLArea cursor.
                if let Some(w) = widget(v) {
                    w.set_mouse_shape(shape as i32);
                }
            }
        }
    }

    /// `pub fn mouseVisibility(target, visibility) void` (`:2168-2179`).
    pub(super) fn mouse_visibility<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        visibility: rustty_apprt_api::action::MouseVisibility,
    ) {
        use rustty_apprt_api::action::MouseVisibility;
        match target {
            Target::App => log::warn!(target: LOG_SCOPE, "mouse visibility to app is unexpected"),
            Target::Surface(v) => {
                // PORTS: `application.zig:2175-2178`
                // `setMouseHidden(switch (visibility) { .visible => false, .hidden => true })`.
                // The generated setter fires `notify::mouse-hidden`, driving
                // `on_notify_mouse_hidden` which hides/shows the cursor.
                if let Some(w) = widget(v) {
                    w.set_mouse_hidden(matches!(visibility, MouseVisibility::Hidden));
                }
            }
        }
    }

    /// `pub fn moveTab(target, value) bool` (`:2173-2196`).
    pub(super) fn move_tab<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::MoveTab,
    ) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(window) = ext::ancestor::<Window>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a window, ignoring move_tab");
                    return false;
                };
                // `window.moveTab(surface, @intCast(value.amount))` (`:2196-2199`).
                window.move_tab(&w, value.amount)
            }
        }
    }

    /// `pub fn moveTabToNewWindow(target) bool` (`:2615-2633`, f6fca9aab).
    pub(super) fn move_tab_to_new_window<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(window) = ext::ancestor::<Window>(&w) else {
                    // `log.warn("surface is not in a window, ignoring
                    // new_tab_to_new_window")` (`:2626` — the reference's
                    // "new_tab_to_new_window" text, verbatim).
                    log::warn!(target: LOG_SCOPE, "surface is not in a window, ignoring new_tab_to_new_window");
                    return false;
                };

                // `window.moveTabToNewWindow(surface)` (`:2630`).
                window.move_tab_to_new_window(&w)
            }
        }
    }

    /// `pub fn newSplit(target, direction) bool` (`:2205-2226`). Activates
    /// `split-tree.new-split` with the direction tag-name (`split_tree.rs:306`).
    pub(super) fn new_split<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        direction: SplitDirection,
    ) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "new split to app is unexpected");
                false
            }
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let dir = match direction {
                    SplitDirection::Right => "right",
                    SplitDirection::Down => "down",
                    SplitDirection::Left => "left",
                    SplitDirection::Up => "up",
                };
                w.activate_action("split-tree.new-split", Some(&dir.to_variant()))
                    .is_ok()
            }
        }
    }

    /// `pub fn newTab(target, overrides) bool` (`:2657-2690`; B5, 034506f14
    /// added the overrides struct). Reaches the enclosing [`Window`] and calls
    /// [`Window::new_tab`] with the target's core surface as the parent
    /// (`window.rs` `new_tab` → `new_tab_page`, the Tab + SplitTree model).
    pub(super) fn new_tab<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        overrides: TabOverrides,
    ) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "new tab to app is unexpected");
                false
            }
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(window) = ext::ancestor::<Window>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a window, ignoring new_tab");
                    return false;
                };
                // `window.newTab(core, .{ .command, .shell_integration,
                // .working_directory, .title })` (`:2686-2691`) — the parent core
                // surface for font/dir inheritance, read off the widget (its
                // `core()`), not the generic handle.
                window.new_tab(w.core(), overrides);
                true
            }
        }
    }

    /// `pub fn newWindow(self, parent, overrides) !void` (`:2253-2331`, folding in
    /// `initAndShowWindow` `:2285-2328`). Creates a [`Window`], binds its `config` to
    /// the app's, opens its first tab (`.window` context), and presents.
    ///
    /// `overrides` carries the optional `command`/`working-directory`/`title` applied
    /// to the first surface (`.none` on the `performAction` path, `:717`; parsed from
    /// the `new-window-command` argv by [`super::action_new_window`], `:1802`).
    pub(super) fn new_window<C: CoreSurfaceHandle>(
        app: &Application,
        target: Target<'_, C>,
        overrides: TabOverrides,
    ) -> Result<(), ApprtError> {
        // Note that we've requested a window at least once (`:2262`). Used to trigger
        // quit-on-no-windows.
        app.imp().requested_window.set(true);

        // The parent core surface (target-dependent): `.app => null`, `.surface => v`
        // (`:714-717`), read off the widget for the surface arm. The type is unified by
        // the `win.new_tab` callsite below (`Window::new_tab` takes `Option<*mut CoreSurface>`).
        let parent = match target {
            Target::App => None,
            Target::Surface(v) => widget(v).and_then(|w| w.core()),
        };

        // `Window.new(self, .{ .title = overrides.title })` (`:2270-2272`) — the window
        // title override is applied up front; it also flows to the first surface via
        // `new_tab_for_window` in the shared helper (`:2309-2313`).
        let win = Window::new(app, overrides.title.as_deref());

        // Bind config, create the first tab, size, and present (`:2285-2329`).
        init_and_show_window(app, &win, parent, overrides);
        Ok(())
    }

    /// PORTS: `application.zig:2285-2329` `initAndShowWindow`. The shared init-and-show
    /// path invoked on an already-constructed [`Window`] — binds the app config to the
    /// window, creates the first tab with window context, estimates the initial size,
    /// and presents. rustty folded this into [`new_window`]; it is split back out here
    /// so [`toggle_quick_terminal`] can drive it with a pre-built quick-terminal window
    /// (the reference's `initAndShowWindow(self, win, null, .none)`, `:2667`).
    pub(super) fn init_and_show_window(
        app: &Application,
        win: &Window,
        parent: Option<*mut CoreSurface>,
        overrides: TabOverrides,
    ) {
        // Setup a binding so that whenever our config changes so does the window.
        // There's never a time when the window config should be out of sync with the
        // application config (`:2295-2306`).
        app.bind_property("config", win, "config").build();

        // Create a new tab with window context (first tab in new window) (`:2308-2313`,
        // `win.newTabForWindow(parent, ...)`), applying the argv override.
        win.new_tab_for_window(parent, overrides);

        // Estimate the initial window size before presenting so the window manager
        // can position it correctly (`surface.estimateInitialSize()` + the
        // `getDefaultSize`/`setDefaultSize` read, `:2315-2325`).
        if let Some(surface) = win.get_active_surface() {
            surface.estimate_initial_size();
            if let Some(size) = surface.default_size() {
                win.set_default_size(size.width as i32, size.height as i32);
            }
        }

        // Show the window (`:2327`).
        win.upcast_ref::<gtk4::Window>().present();
    }

    /// `pub fn openConfig(self, value) bool` (`:2780-2831`; 0a183c923 added the
    /// `apprt.action.OpenConfig` parameter and the `.new_window` arm).
    pub(super) fn open_config(
        app: &Application,
        value: rustty_apprt_api::action::OpenConfig,
    ) -> bool {
        use rustty_apprt_api::action::OpenConfig;

        // Get the config file path (`:2783-2788`, `configpkg.edit.openPath`).
        let path = match rustty_config::edit::open_path() {
            Ok(p) => p,
            Err(err) => {
                log::warn!(target: LOG_SCOPE, "error getting config file path: {err:?}");
                return false;
            }
        };

        match value {
            OpenConfig::OsOpen => {
                // Open it using openUrl. "path" isn't actually a URL but
                // at the time of writing that works just fine for GTK
                // (`:2791-2795`).
                open_url(
                    app,
                    rustty_apprt_api::action::OpenUrl {
                        kind: rustty_apprt_api::action::OpenUrlKind::Text,
                        url: &path,
                    },
                );
            }

            OpenConfig::NewWindow => {
                // `internal_os.getConfigEditCommand(alloc, path, .default)`
                // (`:2798-2802`). The returned command carries its trailing
                // NUL (the `[:0]` sentinel modeled in-band, `os/edit.rs` PORT
                // NOTE); strip it for the logical argv bytes.
                let mut cmd = match rustty_os::getConfigEditCommand(
                    &path,
                    &rustty_os::edit::Options::default(),
                ) {
                    Ok(c) => c,
                    Err(err) => {
                        log::warn!(target: LOG_SCOPE, "unable to get command to edit the config: {err:?}");
                        return false;
                    }
                };
                if cmd.last() == Some(&0) {
                    cmd.pop();
                }

                // `const command: configpkg.Command = .{ .direct = &.{
                // "/bin/sh", "-c", cmd } };` (`:2805-2807`).
                let command = rustty_config::command::Command::Direct(vec![
                    b"/bin/sh".to_vec(),
                    b"-c".to_vec(),
                    cmd,
                ]);

                // `std.fmt.allocPrintSentinel("{s} {s}", .{ i18n._("Editing
                // configuration file"), path }, 0)` (`:2809-2817`) — the
                // format itself is infallible in Rust, matching the
                // `catch ... break :t null` only in shape.
                let title = format!(
                    "{} {}",
                    tr(c"Editing configuration file"),
                    String::from_utf8_lossy(&path)
                );

                // `Action.newWindow(self, null, .{ .command, .title })`
                // (`:2820-2826`).
                if let Err(err) = new_window(
                    app,
                    Target::<AppTargetHandle>::App,
                    TabOverrides {
                        command: Some(command),
                        title: Some(title),
                        ..Default::default()
                    },
                ) {
                    log::warn!(target: LOG_SCOPE, "unable to create new window: {err:?}");
                    return false;
                }
            }
        }

        true
    }

    /// `pub fn openUrl(self, value) void` (`application.zig:2346-2364`).
    pub(super) fn open_url(app: &Application, value: rustty_apprt_api::action::OpenUrl<'_>) {
        // Local file paths and `file://` URLs bypass the URI launcher and go
        // straight to the OS `open` fallback (`:2350-2356`).
        if value.url.starts_with(b"/") || value.url.starts_with(b"file://") {
            open_url_fallback(value.kind, value.url);
            return;
        }

        // `self.openUri().start(value) catch |err| { log; openUrlFallback }`
        // (`:2358-2363`) — the XDG-Desktop-Portal `OpenURI` D-Bus launcher
        // (`portal/OpenURI.zig`, ported as [`crate::gtk::portal::OpenUri`]). Any
        // synchronous failure to even ISSUE the request falls back to the OS opener;
        // an asynchronous failure reaches the same fallback from the portal
        // callbacks.
        if let Err(err) = app.open_uri().start(value) {
            log::error!(target: LOG_SCOPE, "unable to open uri err={err}");
            open_url_fallback(value.kind, value.url);
        }
    }

    /// `pub fn openUrlFallback(self, kind, url) void` (`application.zig:1896-1905`).
    /// The minimal cross-platform URL opener (`internal_os.open`, ported as
    /// [`rustty_os::open::open`]).
    pub(super) fn open_url_fallback(kind: rustty_apprt_api::action::OpenUrlKind, url: &[u8]) {
        use rustty_apprt_api::action::OpenUrlKind;
        // `apprt.action.OpenUrl.Kind` -> the `rustty_os::open::Kind` seam (`open.rs:31`).
        let kind = match kind {
            OpenUrlKind::Unknown => rustty_os::open::Kind::Unknown,
            OpenUrlKind::Text => rustty_os::open::Kind::Text,
            OpenUrlKind::Html => rustty_os::open::Kind::Html,
            // PORT NOTE (P4 W6-tail, 77537c806): the reference passes the ONE
            // `apprt.action.OpenUrl.Kind` enum straight through to
            // `internal_os.open`; this seam mapping is arm-for-arm, so the new
            // `osc8` value maps too (the macOS fail-closed guard lives inside
            // `rustty_os::open::open`; on Linux the generic opener handles it,
            // exactly as upstream).
            OpenUrlKind::Osc8 => rustty_os::open::Kind::Osc8,
        };
        if let Err(err) = rustty_os::open::open(kind, url) {
            log::warn!(target: LOG_SCOPE, "unable to open url: {err:?}");
        }
    }

    /// `pub fn pwd(target, value) void` (`:2369-2378`).
    pub(super) fn pwd<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::Pwd<'_>,
    ) {
        match target {
            Target::App => log::warn!(target: LOG_SCOPE, "pwd to app is unexpected"),
            Target::Surface(v) => {
                // PORTS: `application.zig:2372` `surface.rt_surface.gobj().setPwd(value.pwd)`
                // — set the surface widget's `pwd` property (the tab tooltip + the window
                // subtitle bind to active-surface `pwd`, tab.blp:12 / window.blp:50).
                if let Some(w) = widget(v) {
                    w.set_pwd(Some(String::from_utf8_lossy(value.pwd).into_owned()));
                }
            }
        }
    }

    /// `pub fn quitTimer(self, mode) !void` (`:2379-2388`). Starts/stops the app's
    /// quit timer (`application.rs` `start_quit_timer`/`stop_quit_timer`).
    pub(super) fn quit_timer(app: &Application, mode: rustty_apprt_api::action::QuitTimer) {
        use rustty_apprt_api::action::QuitTimer;
        match mode {
            QuitTimer::Start => app.imp().start_quit_timer(),
            QuitTimer::Stop => app.imp().stop_quit_timer(),
        }
    }

    /// `pub fn presentTerminal(target) bool` (`:2389-2400`).
    pub(super) fn present_terminal<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                // `v.rt_surface.surface.present()` (`:2395-2396`) — emits `present-request`;
                // the owning window selects+focuses the surface's tab and raises the window
                // (`window.rs` `surface_present_request`).
                match widget(v) {
                    Some(w) => {
                        w.present();
                        true
                    }
                    None => false,
                }
            }
        }
    }

    /// `pub fn progressReport(target, value) bool` (`:2401-2412`).
    pub(super) fn progress_report<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_vt::osc::ProgressReport,
    ) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                // `v.rt_surface.surface.setProgressReport(value)` (`:2405`).
                match widget(v) {
                    Some(w) => {
                        w.set_progress_report(value);
                        true
                    }
                    None => false,
                }
            }
        }
    }

    /// `pub fn promptTitle(target, value) bool` (`:2414-2441`). The `.tab` arm reaches
    /// the enclosing [`Tab`] and prompts (`tab.rs:241`); the `.surface` arm defers.
    pub(super) fn prompt_title<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: PromptTitle,
    ) -> bool {
        match value {
            PromptTitle::Surface => match target {
                Target::App => false,
                Target::Surface(v) => {
                    // `v.rt_surface.surface.promptTitle()` (`:2419`).
                    match widget(v) {
                        Some(w) => {
                            w.prompt_title();
                            true
                        }
                        None => false,
                    }
                }
            },
            PromptTitle::Tab => match target {
                Target::App => false,
                Target::Surface(v) => {
                    let Some(w) = widget(v) else { return false };
                    let Some(tab) = ext::ancestor::<Tab>(&w) else {
                        log::warn!(target: LOG_SCOPE, "surface is not in a tab, ignoring prompt_tab_title");
                        return false;
                    };
                    tab.prompt_tab_title();
                    true
                }
            },
            // `.window => { ... win.promptWindowTitle(); return true; }`
            // (`:2923-2939`, 8b7c57c75).
            PromptTitle::Window => match target {
                Target::App => false,
                Target::Surface(v) => {
                    let Some(w) = widget(v) else { return false };
                    let Some(win) = ext::ancestor::<Window>(&w) else {
                        log::warn!(target: LOG_SCOPE, "surface is not in a window, \
                             ignoring prompt_window_title");
                        return false;
                    };
                    win.prompt_window_title();
                    true
                }
            },
        }
    }

    /// `pub fn reloadConfig(self, target, opts) !void` (`:2443-2482`).
    pub(super) fn reload_config<C: CoreSurfaceHandle>(
        app: &Application,
        target: Target<'_, C>,
        opts: rustty_apprt_api::action::ReloadConfig,
    ) -> Result<(), ApprtError> {
        // Tell systemd that reloading has started (`:2448`), and — via the trailing
        // `notify::ready()` after the body — that it has finished. The reference's
        // `defer systemd.notify.ready()` (`:2451`) must run on EVERY exit path
        // (including the `?` early-returns of the hard-reload load), so the body runs
        // in an inner closure and `ready()` fires unconditionally afterwards.
        rustty_os::systemd::notify::reloading();
        let result = reload_config_inner(app, target, opts);
        rustty_os::systemd::notify::ready();
        result
    }

    /// The body of [`reload_config`], factored out so the caller's
    /// `systemd.notify.ready()` runs on every exit path (the reference's
    /// `defer`, `application.zig:2451`).
    fn reload_config_inner<C: CoreSurfaceHandle>(
        app: &Application,
        target: Target<'_, C>,
        opts: rustty_apprt_api::action::ReloadConfig,
    ) -> Result<(), ApprtError> {
        // Get our config object (`:2453-2472`).
        let config: Config = if opts.soft {
            // Soft reload: apply conditional logic to the existing loaded config, so
            // return it as-is but take a reference (`:2458-2461`,
            // `self.private().config.ref()`).
            match app.get_config() {
                Some(c) => c,
                // No config set yet (pre-construction) — nothing to reload.
                None => return Ok(()),
            }
        } else {
            // Hard reload: load a new config completely (`:2464-2470`). The reference
            // wraps the loaded `CoreConfig` in a `Config` GObject (which clones it),
            // so the local load result may drop here.
            let core_config = CoreConfig::load().map_err(|err| {
                log::warn!(target: LOG_SCOPE, "error loading config on reload err={err:?}");
                ApprtError
            })?;
            Config::new(&core_config)
        };

        // Update the proper target. This triggers a `config_change` apprt action
        // which propagates the config to our property system (`:2477-2481`).
        match target {
            // `.app => try self.core().updateConfig(self.rt(), config.get())`
            // (`:2472-2475`).
            Target::App => app.imp().update_config_app(config.get())?,
            // `.surface => |core| try core.updateConfig(config.get())` (`:2477`).
            Target::Surface(v) => {
                let rt = v.rt_surface_ptr().cast::<ApprtSurface>();
                if !rt.is_null() {
                    // SAFETY: same liveness contract as `widget`: for a `.surface`
                    // action the core app hands the LIVE core surface whose
                    // `rt_surface` points at the pinned apprt `Surface`, read on the
                    // app thread. `core()` reads the embedded `*CoreSurface` backref.
                    let core = unsafe { (*rt).core() };
                    unsafe { &mut *core }
                        .updateConfig(config.get())
                        .map_err(|err| {
                            log::warn!(target: LOG_SCOPE, "error updating surface config err={err:?}");
                            ApprtError
                        })?;
                }
            }
        }
        Ok(())
    }

    /// `pub fn resizeSplit(target, value) bool` (`:2489-2519`). Reaches the enclosing
    /// [`SplitTree`], no-ops if the tree has no split (so the key passes through), then
    /// calls [`SplitTree::resize`].
    pub(super) fn resize_split<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::ResizeSplit,
    ) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "resize_split to app is unexpected");
                false
            }
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(tree) = ext::ancestor::<SplitTree>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a split tree, ignoring resize_split");
                    return false;
                };
                // If the tree has no splits (one leaf), the action is not performable
                // (`:2500-2501`).
                if !tree.property::<bool>("is-split") {
                    return false;
                }
                let dir = match value.direction {
                    ResizeSplitDirection::Up => Direction::Up,
                    ResizeSplitDirection::Down => Direction::Down,
                    ResizeSplitDirection::Left => Direction::Left,
                    ResizeSplitDirection::Right => Direction::Right,
                };
                match tree.resize(dir, value.amount) {
                    Ok(handled) => handled,
                    Err(_) => {
                        log::warn!(target: LOG_SCOPE, "unable to resize split");
                        false
                    }
                }
            }
        }
    }

    /// `pub fn ringBell(target) void` (`application.zig:2521-2526`) →
    /// `surface.setBellRinging(true)` (`surface.zig:2208-2222`).
    ///
    /// `setBellRinging(true)` first runs `ringBell` (`surface.zig:2548-2563`, B7
    /// end state), which activates `tab.ring-bell` / `win.ring-bell` (system
    /// beep and urgency) and `app.ring-bell` (the application-owned audio bell,
    /// [`super::action_ring_bell`]), then sets the `bell-ringing` property
    /// unless the surface is focused (f5419b9b1). The property drives the
    /// bind-time `should_border_be_shown` closure (`surface.rs`) so the bell
    /// border lights up. We reproduce that here on the `Surface` widget: the
    /// generated `set_bell_ringing` setter (from `#[property(get, set)]` on
    /// `bell_ringing`) is the property write and the three `activate_action`s
    /// are `ringBell`'s action fan-out (`surface.zig:2558-2560`).
    pub(super) fn ring_bell<C: CoreSurfaceHandle>(target: Target<'_, C>) {
        match target {
            Target::App => {}
            // `.surface => |v| v.rt_surface.surface.setBellRinging(true)` (`:2521`).
            // rustty hosts the full bell (signal emit, `tab`/`win.ring-bell` action
            // fan-out, audio, and the `bell-ringing` border property) in
            // [`Surface::ring_bell`], which both this action and `commandFinished`
            // funnel through.
            Target::Surface(v) => {
                if let Some(w) = widget(v) {
                    w.ring_bell();
                }
            }
        }
    }

    /// `pub fn scrollbar(target, value) void` (`:2528-2536`).
    pub(super) fn scrollbar<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_vt::pagelist::grow::Scrollbar,
    ) {
        match target {
            Target::App => {}
            // `.surface => |v| v.rt_surface.surface.setScrollbar(value)` (`:2534`).
            Target::Surface(v) => {
                if let Some(w) = widget(v) {
                    w.set_scrollbar(value);
                }
            }
        }
    }

    /// `pub fn startSearch(target, value) void` (`:2538-2543`).
    pub(super) fn start_search<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::StartSearch<'_>,
    ) {
        match target {
            Target::App => {}
            Target::Surface(v) => {
                // `v.rt_surface.surface.setSearchActive(true, value.needle)` (`:2549`).
                if let Some(w) = widget(v) {
                    w.set_search_active(true, value.needle);
                }
            }
        }
    }

    /// `pub fn endSearch(target) void` (`:2545-2550`).
    pub(super) fn end_search<C: CoreSurfaceHandle>(target: Target<'_, C>) {
        match target {
            Target::App => {}
            Target::Surface(v) => {
                // `v.rt_surface.surface.setSearchActive(false, "")` (`:2556`).
                if let Some(w) = widget(v) {
                    w.set_search_active(false, b"");
                }
            }
        }
    }

    /// `pub fn searchTotal(target, value) void` (`:2552-2557`).
    pub(super) fn search_total<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::SearchTotal,
    ) {
        match target {
            Target::App => {}
            Target::Surface(v) => {
                // `v.rt_surface.surface.setSearchTotal(value.total)` (`:2563`).
                if let Some(w) = widget(v) {
                    w.set_search_total(value.total);
                }
            }
        }
    }

    /// `pub fn searchSelected(target, value) void` (`:2559-2564`).
    pub(super) fn search_selected<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::SearchSelected,
    ) {
        match target {
            Target::App => {}
            Target::Surface(v) => {
                // `v.rt_surface.surface.setSearchSelected(value.selected)` (`:2570`).
                if let Some(w) = widget(v) {
                    w.set_search_selected(value.selected);
                }
            }
        }
    }

    /// `pub fn setTitle(target, value) void` (`:2566-2574`).
    pub(super) fn set_title<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::SetTitle<'_>,
    ) {
        match target {
            Target::App => log::warn!(target: LOG_SCOPE, "set_title to app is unexpected"),
            Target::Surface(v) => {
                // PORTS: `application.zig:2576-2583` `setTitle` — set the surface widget's
                // `title` property. The tab's `computed_title` binds this active-surface
                // `title` as the `terminal` title (tab.blp:11), which flows to the tab
                // title and, via the window's tab BindingGroup, to the window title.
                if let Some(w) = widget(v) {
                    w.set_title(Some(String::from_utf8_lossy(value.title).into_owned()));
                }
            }
        }
    }

    /// `pub fn setTabTitle(target, value) bool` (`:2576-2597`). Reaches the enclosing
    /// [`Tab`] and applies the title override (`tab.rs:235`).
    pub(super) fn set_tab_title<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::SetTitle<'_>,
    ) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "set_tab_title to app is unexpected");
                false
            }
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(tab) = ext::ancestor::<Tab>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a tab, ignoring set_tab_title");
                    return false;
                };
                // `setTitleOverride(if len==0 null else title)` (`:2594`).
                let title = if value.title.is_empty() {
                    None
                } else {
                    Some(String::from_utf8_lossy(value.title).into_owned())
                };
                tab.apply_title_override(title.as_deref());
                true
            }
        }
    }

    /// `pub fn setWindowTitle(target, value) bool` (`:3111-3133`, 8b7c57c75 —
    /// the window-title renaming pair).
    pub(super) fn set_window_title<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::SetTitle<'_>,
    ) -> bool {
        match target {
            Target::App => {
                // `log.warn("set_tab_title to app is unexpected")` (`:3117` —
                // the reference's copy-pasted "set_tab_title" text, verbatim).
                log::warn!(target: LOG_SCOPE, "set_tab_title to app is unexpected");
                false
            }
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(window) = ext::ancestor::<Window>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a window, ignoring set_window_title");
                    return false;
                };
                // `window.setTitleOverride(if (value.title.len == 0) null
                // else value.title)` (`:3129`).
                let title = if value.title.is_empty() {
                    None
                } else {
                    Some(String::from_utf8_lossy(value.title).into_owned())
                };
                window.set_title_override(title);
                true
            }
        }
    }

    /// `pub fn showChildExited(target, value) bool` (`:2600-2608`).
    pub(super) fn show_child_exited<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::surface::ChildExited,
    ) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                // `v.rt_surface.surface.childExited(value)` (`:2614`).
                match widget(v) {
                    Some(w) => w.child_exited_show(value),
                    None => false,
                }
            }
        }
    }

    /// `pub fn showGtkInspector() void` (`:2610-2612`).
    pub(super) fn show_gtk_inspector() {
        gtk4::Window::set_interactive_debugging(true);
    }

    /// `pub fn sizeLimit(target, value) bool` (`:2614-2632`).
    pub(super) fn size_limit<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::SizeLimit,
    ) -> bool {
        match target {
            Target::App => false,
            // `.surface => |core| { surface.setMinSize(.{ .width = min_width,
            // .height = min_height }); return true; }` (`:2621-2630`). The max size
            // is ignored — we have no mechanism to enforce it (`:2622-2623`).
            Target::Surface(v) => match widget(v) {
                Some(w) => {
                    w.set_min_size(Some(Size {
                        width: value.min_width,
                        height: value.min_height,
                    }));
                    true
                }
                None => false,
            },
        }
    }

    /// `pub fn toggleFullscreen(target) void` (`:2634-2639`).
    pub(super) fn toggle_fullscreen<C: CoreSurfaceHandle>(target: Target<'_, C>) {
        match target {
            Target::App => {}
            Target::Surface(v) => {
                // `v.rt_surface.surface.toggleFullscreen()` (`:2638`) — emits
                // `toggle-fullscreen`; the owning window fullscreens/unfullscreens
                // (`window.rs` `surface_toggle_fullscreen`).
                if let Some(w) = widget(v) {
                    w.toggle_fullscreen();
                }
            }
        }
    }

    /// `pub fn toggleQuickTerminal(self) bool` (`:2649-2669`).
    pub(super) fn toggle_quick_terminal(app: &Application) -> bool {
        // If we already have a quick terminal window, we just toggle the
        // visibility of it (`:2650-2655`).
        if let Some(win) = get_quick_terminal_window() {
            win.toggle_visibility();
            return true;
        }

        // If we don't support quick terminals then we do nothing (`:2657-2659`).
        if !app.winproto().supports_quick_terminal() {
            return false;
        }

        // Create our new window as a quick terminal (`:2661-2665`,
        // `gobject.ext.newInstance(Window, .{ .application, .@"quick-terminal" = true })`).
        let win: Window = glib::Object::builder()
            .property("application", app)
            .property("quick-terminal", true)
            .build();
        assert!(win.is_quick_terminal());

        // Bind config, create the first tab, size, and present (`:2667`,
        // `initAndShowWindow(self, win, null, .none)`).
        init_and_show_window(app, &win, None, TabOverrides::default());
        true
    }

    /// `fn getQuickTerminalWindow() ?*Window` (`:2713-2731`). Finds a quick terminal
    /// window: scans the toplevels and returns the first that downcasts to our
    /// [`Window`] subclass with `is_quick_terminal()` set.
    fn get_quick_terminal_window() -> Option<Window> {
        gtk4::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<Window>().ok())
            .find(Window::is_quick_terminal)
    }

    /// `pub fn toggleSplitZoom(target) bool` (`:2654-2680`). Reaches the enclosing
    /// [`SplitTree`], no-ops if unsplit, then activates `split-tree.zoom`
    /// (`split_tree.rs:308`).
    pub(super) fn toggle_split_zoom<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "toggle_split_zoom to app is unexpected");
                false
            }
            Target::Surface(v) => {
                // TODO: pass surface ID when we have that
                let Some(w) = widget(v) else { return false };
                let Some(tree) = ext::ancestor::<SplitTree>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a split tree, ignoring toggle_split_zoom");
                    return false;
                };
                if !tree.property::<bool>("is-split") {
                    return false;
                }
                w.activate_action("split-tree.zoom", None).is_ok()
            }
        }
    }

    /// `pub fn toggleMaximize(target) void` (`:2725-2730`).
    pub(super) fn toggle_maximize<C: CoreSurfaceHandle>(target: Target<'_, C>) {
        match target {
            Target::App => {}
            Target::Surface(v) => {
                // `v.rt_surface.surface.toggleMaximize()` (`:2729`) — emits
                // `toggle-maximize`; the owning window maximizes/unmaximizes
                // (`window.rs` `surface_toggle_maximize`).
                if let Some(w) = widget(v) {
                    w.toggle_maximize();
                }
            }
        }
    }

    /// `pub fn toggleTabOverview(target) bool` (`:2732-2749`). Reaches the enclosing
    /// [`Window`] and toggles its tab overview (`window.rs:973`).
    pub(super) fn toggle_tab_overview<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(window) = ext::ancestor::<Window>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a window, ignoring toggle_tab_overview");
                    return false;
                };
                window.toggle_tab_overview();
                true
            }
        }
    }

    /// `pub fn toggleWindowDecorations(target) bool` (`:2751-2768`). Reaches the
    /// enclosing [`Window`] and toggles its decorations (`window.rs:963`).
    pub(super) fn toggle_window_decorations<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                let Some(window) = ext::ancestor::<Window>(&w) else {
                    log::warn!(target: LOG_SCOPE, "surface is not in a window, ignoring toggle_window_decorations");
                    return false;
                };
                window.toggle_window_decorations();
                true
            }
        }
    }

    /// `pub fn toggleCommandPalette(target) bool` (`:2778-2785`). Delegates to the
    /// surface widget's `toggleCommandPalette` (`surface.zig:880-883`), which
    /// activates the `win.toggle-command-palette` action installed on the window.
    pub(super) fn toggle_command_palette<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => false,
            Target::Surface(v) => {
                let Some(w) = widget(v) else { return false };
                // TODO: pass the surface with the action (`surface.zig:881`)
                //
                // `surface.rt_surface.gobj().toggleCommandPalette()` →
                // `activateAction("win.toggle-command-palette", null)`.
                w.activate_action("win.toggle-command-palette", None)
                    .is_ok()
            }
        }
    }

    /// `pub fn controlInspector(target, value) bool` (`:2779-2786`).
    pub(super) fn control_inspector<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::Inspector,
    ) -> bool {
        match target {
            Target::App => false,
            // `.surface => |surface| surface.rt_surface.gobj().controlInspector(value)`
            // (`:2783`).
            Target::Surface(v) => match widget(v) {
                Some(w) => w.control_inspector(value),
                None => false,
            },
        }
    }

    /// `pub fn exportTerminalIO(self, target, value) Allocator.Error!bool`
    /// (`:2259-2301`, B9 — ad96613a8's export-terminal-io consumer with
    /// 2c0d2588a's type-safe FileChooser cast folded in). Duplicates the
    /// borrowed contents, opens a native save dialog, and writes the file on
    /// accept.
    ///
    /// PORT NOTE: `contents` is `value.contents` (the apprt-api
    /// `Action::ExportTerminalIO` variant's one-field payload, passed as raw
    /// bytes). `_app` is the reference's `self` — used only for
    /// `self.allocator()`, which Rust ownership absorbs. `allow(deprecated)`:
    /// `gtk.FileChooserNative` is deprecated since GTK 4.10; the reference
    /// uses it deliberately on GTK4.
    #[allow(deprecated)]
    pub(super) fn export_terminal_io<C: CoreSurfaceHandle>(
        _app: &Application,
        target: Target<'_, C>,
        contents: &[u8],
    ) -> bool {
        use std::cell::RefCell;
        use std::rc::Rc;

        // `.app => return false, .surface => |v| v.rt_surface.gobj()`
        // (`:2264-2267`).
        let surface = match target {
            Target::App => return false,
            Target::Surface(v) => match widget(v) {
                Some(w) => w,
                None => return false,
            },
        };

        // `const contents = try alloc.dupe(u8, value.contents);` + the heap
        // `ExportTerminalIORequest` (`:2269-2277`): the request owns the copy;
        // the closure capture is the reference's `destroyData` lifetime.
        let request = ExportTerminalIORequest {
            contents: contents.to_vec(),
        };

        // `ext.getAncestor(gtk.Window, surface.as(gtk.Widget))` (`:2279`).
        let parent = ext::ancestor::<gtk4::Window>(&surface);

        // `gtk.FileChooserNative.new(...)` (`:2280-2286`).
        let dialog = gtk4::FileChooserNative::new(
            Some(tr(c"Export Terminal IO Events")),
            parent.as_ref(),
            gtk4::FileChooserAction::Save,
            Some(tr(c"Export")),
            Some(tr(c"Cancel")),
        );
        // `chooser.setCreateFolders(1)` + `chooser.setCurrentName(...)`
        // (`:2287-2289`; R2: the default file name carries the rustty
        // identity).
        dialog.set_create_folders(true);
        dialog.set_current_name("rustty-terminal-io.txt");

        // `gtk.NativeDialog.signals.response.connect(dialog, ..., .{
        // .destroyData = ExportTerminalIORequest.destroy })` (`:2291-2297`).
        // The keep-alive slot models the reference's ownership: `new` hands us
        // one strong ref which the response handler releases (`defer
        // dialog.unref()`, `:2311`); a native dialog is not a widget, so
        // nothing else holds it. `take()` breaks the dialog → closure → slot
        // → dialog cycle when the response fires.
        let keep_alive: Rc<RefCell<Option<gtk4::FileChooserNative>>> =
            Rc::new(RefCell::new(Some(dialog.clone())));
        dialog.connect_response(move |dialog, response_id| {
            let _keep = keep_alive.borrow_mut().take();
            request.response(dialog, response_id);
        });

        // `dialog.as(gtk.NativeDialog).show()` (`:2298`).
        dialog.show();
        true
    }

    /// PORTS: `application.zig:2303-2352` `ExportTerminalIORequest` (B9): the
    /// heap context for one export dialog. `destroy` (`:2307-2310`, the signal
    /// `destroyData`) is the struct's `Drop` — the closure capture frees the
    /// contents when the handler is released.
    struct ExportTerminalIORequest {
        contents: Vec<u8>,
    }

    impl ExportTerminalIORequest {
        /// `fn response(dialog, response_id, self)` (`:2312-2351`).
        #[allow(deprecated)]
        fn response(&self, dialog: &gtk4::FileChooserNative, response_id: gtk4::ResponseType) {
            // (`defer dialog.unref()` (`:2316`) is the caller's keep-alive
            // `take()`.)

            // `if (response_id != @intFromEnum(gtk.ResponseType.accept))
            // return;` (`:2318`).
            if response_id != gtk4::ResponseType::Accept {
                return;
            }

            // `dialog.as(gtk.FileChooser).getFile() orelse { warn; return; }`
            // (`:2320-2324`).
            let Some(file) = dialog.file() else {
                log::warn!(target: LOG_SCOPE, "inspector export dialog returned no file");
                return;
            };

            // `file.replaceContents(contents.ptr, contents.len, null, 0, .{},
            // null, null, &gerr)` (`:2326-2336`): etag null, no backup, no
            // flags, synchronous. The gio binding folds the `gerr` out-param
            // and the `replaced == 0` result into one `Result`, so the
            // reference's two failure logs (`:2337-2349`) collapse to the
            // `Err` arm.
            if let Err(err) = file.replace_contents(
                &self.contents,
                None,
                false,
                gio::FileCreateFlags::empty(),
                gio::Cancellable::NONE,
            ) {
                log::error!(target: LOG_SCOPE, "failed to export terminal IO events err={err}");
            }
        }
    }

    /// `pub fn commandFinished(target, value) bool` (`:2788-2795`).
    pub(super) fn command_finished<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::CommandFinished,
    ) -> bool {
        match target {
            Target::App => false,
            // `.surface => |surface| surface.rt_surface.gobj().commandFinished(value)`
            // (`:2792`).
            Target::Surface(v) => match widget(v) {
                Some(w) => w.command_finished(value),
                None => false,
            },
        }
    }

    /// `pub fn setReadonly(target, value) bool` (`:2797-2804`).
    pub(super) fn set_readonly<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::Readonly,
    ) -> bool {
        match target {
            Target::App => false,
            // `.surface => |surface| surface.rt_surface.gobj().setReadonly(value)`
            // (`:2801`).
            Target::Surface(v) => match widget(v) {
                Some(w) => w.set_readonly(value),
                None => false,
            },
        }
    }

    /// `pub fn keySequence(target, value) bool` (`:2806-2820`).
    pub(super) fn key_sequence<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::KeySequence,
    ) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "key_sequence action to app is unexpected");
                false
            }
            Target::Surface(v) => {
                // `core.rt_surface.gobj().keySequenceAction(value)` (`:2821`). Errors
                // only log; the action returns true unconditionally (`:2824`).
                if let Some(w) = widget(v) {
                    w.key_sequence_action(value);
                }
                true
            }
        }
    }

    /// `pub fn keyTable(target, value) bool` (`:2822-2836`).
    pub(super) fn key_table<C: CoreSurfaceHandle>(
        target: Target<'_, C>,
        value: rustty_apprt_api::action::KeyTable<'_>,
    ) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "key_table action to app is unexpected");
                false
            }
            Target::Surface(v) => {
                // `core.rt_surface.gobj().keyTableAction(value)` (`:2836`). Errors only
                // log; the action returns true unconditionally (`:2839`).
                if let Some(w) = widget(v) {
                    w.key_table_action(value);
                }
                true
            }
        }
    }

    /// `pub fn showOnScreenKeyboard(target) bool` (`:2699-2712`). Forwards to the
    /// surface widget's on-screen-keyboard request (`surface.rs:1334`).
    pub(super) fn show_on_screen_keyboard<C: CoreSurfaceHandle>(target: Target<'_, C>) -> bool {
        match target {
            Target::App => {
                log::warn!(target: LOG_SCOPE, "show_on_screen_keyboard to app is unexpected");
                false
            }
            Target::Surface(v) => match widget(v) {
                Some(w) => w.show_on_screen_keyboard(None),
                None => false,
            },
        }
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
        ) -> Result<apprt::ClipboardReadResult, apprt::ApprtError> {
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
        fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
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

        // All nine actions are present (`:1483-1492` — B5's `new-tab` and B7's
        // `ring-bell` joined the original seven).
        let names: Vec<String> = app.list_actions().into_iter().map(Into::into).collect();
        for expected in [
            "new-window",
            "new-window-command",
            "new-tab",
            "open-config",
            "present-surface",
            "quit",
            "reload-config",
            "toggle-quick-terminal",
            "ring-bell",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "startup must register the '{expected}' action; got {names:?}"
            );
        }
    }

    /// PORTS: `application.zig:1655-1663` `globalShortcutTrigger`. A `trigger`
    /// from the GlobalShortcuts portal performs the action on the core app via
    /// `performAllAction`. Requires a display only because `build_test_app`
    /// constructs a real `GtkApplication`.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn application_global_shortcut_trigger_performs_action() {
        // F2: NOT `if gtk4::init().is_err() { return; }`. This is the test whose
        // own vacuity taught this stage the lesson; a silent skip here is
        // indistinguishable from a pass, and a reviewer hit exactly that.
        crate::gtk::init_for_display_test("application_global_shortcut_trigger_performs_action");
        let _ = libadwaita::init();

        let mut core_app: Box<CoreApp<Double>> =
            CoreApp::<Double>::create(Global).expect("core app");
        let core_ptr: *mut CoreApp<Double> = &mut *core_app;

        let app = build_test_app(core_ptr.cast());
        assert!(
            app.imp().perform_all_action_thunk.borrow().is_none(),
            "unbound core app installs no dispatch thunk"
        );

        // task-14d-3: the `rt_app` backref MUST be bound, and BEFORE
        // `bind_core_app`. The thunk opens with
        // `let Some(rt_app) = rt_app else { return; }`, and `bind_core_app`
        // SNAPSHOTS `imp.rt_app` into the closure at install time
        // (`let rt_app = self.imp().rt_app.get()...` — application.rs:2913), so
        // binding it afterwards has no effect on an already-installed thunk.
        // With it unbound the thunk returns BEFORE `performAllAction` and this
        // test never reaches the dispatch it claims to prove. Caught by running
        // the S50-F4 negative control on Linux: re-adding the removed success
        // emission left the test GREEN, which is the definition of a vacuous
        // assertion.
        let mut rt_app = Double {
            config: rustty_config::Config::default_config(),
        };
        let rt_ptr: *mut Double = &raw mut rt_app;
        app.bind_rt_app(rt_ptr.cast());
        assert!(
            app.imp().rt_app.get().is_some(),
            "bind_rt_app must publish the rt-app backref the thunk dereferences"
        );

        app.bind_core_app::<Double>();
        assert!(
            app.imp().perform_all_action_thunk.borrow().is_some(),
            "bind_core_app must install the performAllAction thunk"
        );

        // S50-F4 (task-14d-3): the SUCCESS path must emit NOTHING. The port
        // used to log `global shortcut action performed ok` here; the reference
        // (`application.zig:1741-1749`) is a bare `catch` with no success arm,
        // and an emission the reference does not make is as much a parity
        // defect as a missing one. Asserting the FULL captured record vector is
        // empty is what makes the regression fail — a `.any()`-style assertion
        // could not see an EXTRA record at all.
        let reached_before = PERFORM_ALL_ACTION_REACHED.load(std::sync::atomic::Ordering::Relaxed);
        let cap = crate::log_capture::start();
        app.imp()
            .global_shortcut_trigger(&ActionBoxed(rustty_input::binding::Action::NewWindow));
        let messages = cap.messages();
        drop(cap);

        assert_eq!(
            app.imp().global_shortcut_trigger_count.get(),
            1,
            "the trigger handler must reach the core dispatch exactly once"
        );
        assert_eq!(
            PERFORM_ALL_ACTION_REACHED.load(std::sync::atomic::Ordering::Relaxed) - reached_before,
            1,
            "the thunk must have reached performAllAction — without this the \
             empty-capture assertion below is vacuous, which is exactly what the \
             S50-F4 negative control caught on Linux"
        );
        assert_eq!(
            Vec::<(log::Level, String, String)>::new(),
            messages,
            "globalShortcutTrigger's SUCCESS path logs nothing in the reference"
        );
    }

    /// PORTS: `application.zig:1700-1719` — the notification body assembly. Pure
    /// string work: no display, no D-Bus, no compositor, so NOT `#[ignore]`d. This
    /// is where the user-visible content of the whole `bind-failed` channel lives.
    #[test]
    fn application_bind_failed_notification_body() {
        use rustty_input::binding::{Trigger, TriggerKey};
        use rustty_vt::input::key_mods::Mods;

        let trigger = Trigger {
            key: TriggerKey::Unicode('q' as u32),
            mods: Mods {
                super_: true,
                ..Default::default()
            },
        };

        // A non-empty compositor message is used verbatim (`:1708`).
        let (title, body) = bind_failed_notification(&BindFailed {
            trigger,
            action: rustty_input::binding::Action::NewWindow,
            message: b"already held by vicinae".to_vec(),
            revoked: false,
        });
        assert_eq!(title, "Global keybind unavailable");
        assert_eq!(body, "Super+Q: already held by vicinae");

        // Empty message + denied -> the "denied" fallback (`:1710-1712`).
        let (_, body) = bind_failed_notification(&BindFailed {
            trigger,
            action: rustty_input::binding::Action::NewWindow,
            message: Vec::new(),
            revoked: false,
        });
        assert_eq!(body, "Super+Q: The keybind was denied by the system.");

        // Empty message + revoked -> the "revoked" fallback (`:1710-1711`). This
        // pair is the reason the `revoked` flag is carried at all: a test of only
        // one arm passes with the flag ignored.
        let (_, body) = bind_failed_notification(&BindFailed {
            trigger,
            action: rustty_input::binding::Action::NewWindow,
            message: Vec::new(),
            revoked: true,
        });
        assert_eq!(body, "Super+Q: The keybind was revoked by the system.");

        // An unformattable trigger degrades to "?" rather than dropping the
        // notification (`:1701-1705`).
        let (_, body) = bind_failed_notification(&BindFailed {
            trigger: Trigger {
                key: TriggerKey::CatchAll,
                mods: Mods::default(),
            },
            action: rustty_input::binding::Action::NewWindow,
            message: b"nope".to_vec(),
            revoked: false,
        });
        assert_eq!(body, "?: nope");
    }

    /// PORTS: `application.zig:1475-1481` — the `bind-failed` connection. Emitting
    /// the signal on the app's own `GlobalShortcuts` must reach
    /// `globalShortcutBindFailed`. Asserts the HANDLER FIRED (the counter), not
    /// merely that emitting did not error.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn application_bind_failed_signal_reaches_the_handler() {
        use rustty_input::binding::{Trigger, TriggerKey};
        use rustty_vt::input::key_mods::Mods;

        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        use gio::prelude::ApplicationExt as _;

        let app = build_test_app(std::ptr::null_mut());
        // `startup` is what runs `startupGlobalShortcuts` and makes the connection.
        app.register(gio::Cancellable::NONE).expect("register");

        let gs: GlobalShortcuts = app
            .imp()
            .global_shortcuts
            .borrow()
            .clone()
            .expect("startup created the GlobalShortcuts object")
            .downcast()
            .expect("it is a GhosttyGlobalShortcuts");

        assert_eq!(app.imp().global_shortcut_bind_failed_count.get(), 0);

        gs.emit_bind_failed(&BindFailed {
            trigger: Trigger {
                key: TriggerKey::Unicode('q' as u32),
                mods: Mods {
                    super_: true,
                    ..Default::default()
                },
            },
            action: rustty_input::binding::Action::NewWindow,
            message: Vec::new(),
            revoked: false,
        });

        assert_eq!(
            app.imp().global_shortcut_bind_failed_count.get(),
            1,
            "the bind-failed signal must reach globalShortcutBindFailed exactly once"
        );
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

    /// PORTS: `application.zig:1565-1583` (the GTK ≥ 4.20 `prefers-color-scheme`
    /// push inside `handleStyleManagerDark`) — the RUNTIME KAT for the opt-in
    /// `gtk-v4_20` feature. Ghostty's branch is comptime-folded below 4.20 and
    /// runtime-guarded above; rustty mirrors that as `#[cfg(feature =
    /// "gtk-v4_20")]` + `gtk_version::at_least(4, 20, 0)`. This test only exists
    /// when the feature is on, and only proves anything on a ≥ 4.20 GTK runtime
    /// (validated 2026-07-19 on GTK 4.22.4): forcing the Adwaita color scheme
    /// must synchronously drive `handle_style_manager_dark` (via the
    /// `notify::dark` connection made in `startup_style_manager`,
    /// `application.zig:1386-1396`) and push the matching
    /// `InterfaceColorScheme` onto the startup-installed `css_provider`
    /// (`application.zig:1570-1582`).
    #[cfg(feature = "gtk-v4_20")]
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk,gtk-v4_20 -- --ignored"]
    fn application_v4_20_color_scheme_css_push() {
        use gio::prelude::ApplicationExt;
        use libadwaita::prelude::AdwApplicationExt;

        if gtk4::init().is_err() {
            eprintln!("skip: no display");
            return;
        }
        let _ = libadwaita::init();
        assert!(
            crate::gtk::gtk_version::at_least(4, 20, 0),
            "the gtk-v4_20 KAT needs a GTK >= 4.20 shared library at runtime \
             (found {}.{}.{})",
            gtk4::major_version(),
            gtk4::minor_version(),
            gtk4::micro_version()
        );

        // A real core app so `build_test_app` has a heap-stable pointer (same
        // shape as `application_activate_pushes_new_window`).
        let mut core_app: Box<CoreApp<Double>> =
            CoreApp::<Double>::create(Global).expect("core app");
        let core_ptr: *mut CoreApp<Double> = &mut *core_app;
        let app = build_test_app(core_ptr.cast());
        // `register` emits `startup` synchronously → `startup_style_manager`
        // connects `notify::dark` (`application.zig:1386-1396`).
        app.register(gio::Cancellable::NONE).expect("register");
        // The shared `css_provider` is installed by `Application::new`
        // (`application.zig:401` → `new` at `:1844`), which `build_test_app`
        // bypasses (it builds the bare NON_UNIQUE test instance to avoid the
        // primary-instance/session-bus hazard). Install a bare provider on the
        // imp directly — the unit under test is `handle_style_manager_dark`'s
        // ≥ 4.20 push into whatever provider the imp holds, not `new`'s
        // installation (exercised by the real binary).
        let provider = gtk4::CssProvider::new();
        app.imp().css_provider.replace(Some(provider.clone()));

        // `notify::dark` only fires on a real transition, and the session may
        // already be dark (or light) — so force the OPPOSITE of the current
        // state first, then flip back: two guaranteed transitions, each driving
        // `handleStyleManagerDark`'s ≥ 4.20 push, independent of session theme.
        let style = app.style_manager();
        let started_dark = style.is_dark();
        let (first, first_expect, second, second_expect) = if started_dark {
            (
                libadwaita::ColorScheme::ForceLight,
                gtk4::InterfaceColorScheme::Light,
                libadwaita::ColorScheme::ForceDark,
                gtk4::InterfaceColorScheme::Dark,
            )
        } else {
            (
                libadwaita::ColorScheme::ForceDark,
                gtk4::InterfaceColorScheme::Dark,
                libadwaita::ColorScheme::ForceLight,
                gtk4::InterfaceColorScheme::Light,
            )
        };

        style.set_color_scheme(first);
        while glib::MainContext::default().iteration(false) {}
        assert_eq!(
            provider.property::<gtk4::InterfaceColorScheme>("prefers-color-scheme"),
            first_expect,
            "the first forced transition must push the matching \
             InterfaceColorScheme onto the css_provider (application.zig:1570-1582)"
        );

        style.set_color_scheme(second);
        while glib::MainContext::default().iteration(false) {}
        assert_eq!(
            provider.property::<gtk4::InterfaceColorScheme>("prefers-color-scheme"),
            second_expect,
            "the second forced transition must push the matching \
             InterfaceColorScheme onto the css_provider"
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

    /// F-011 KAT: `saved_language` reports the launch-time `LANG` stored in
    /// `priv.saved_language` (`application.zig:475-478`) — not a hardcoded `None`
    /// (the regressed placeholder), which made `defaultTermioEnv`
    /// (`surface.zig:1602-1606`) strip `LANG` from every default child
    /// environment.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn saved_language_reads_the_captured_launch_lang() {
        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();

        let app = build_test_app(std::ptr::null_mut());

        // `LANG` unset at launch → `None` (the consumer then removes `LANG`).
        app.imp().saved_language.replace(None);
        assert_eq!(app.saved_language(), None);

        // Captured at launch → returned verbatim (a copy of the stored bytes).
        let lang = CString::new("en_US.UTF-8").unwrap();
        app.imp().saved_language.replace(Some(lang.clone()));
        assert_eq!(app.saved_language(), Some(lang));
    }

    /// Recursively find a presented [`CloseConfirmationDialog`] in a widget tree
    /// (presented unparented, the dialog floats inside its own toplevel host).
    fn find_close_confirmation(widget: &gtk4::Widget) -> Option<CloseConfirmationDialog> {
        use gtk4::prelude::WidgetExt as _;
        if let Some(dialog) = widget.downcast_ref::<CloseConfirmationDialog>() {
            return Some(dialog.clone());
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            if let Some(found) = find_close_confirmation(&c) {
                return Some(found);
            }
            child = c.next_sibling();
        }
        None
    }

    /// F-009 KAT (`application.zig:603-644` `quit` + `:1552-1558`
    /// `handleCloseConfirmation`): when the core needs quit confirmation, `quit`
    /// must NOT stop the loop; it must present a `CloseConfirmationDialog(.app)`
    /// whose `close-request` routes to `quit_now`. Against the pre-fix
    /// placeholder (a stderr warning, no dialog) the toplevel scan finds nothing
    /// and this fails.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn quit_needing_confirmation_presents_dialog_and_close_request_quits() {
        use libadwaita::prelude::AdwDialogExt as _;

        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        let app = build_test_app(std::ptr::null_mut());
        app.imp().running.set(true);
        app.imp()
            .needs_confirm_quit_thunk
            .replace(Some(Box::new(|| true)));

        app.quit();
        while glib::MainContext::default().iteration(false) {}

        // Not confirmed yet → the loop keeps running (`:609` comment: we must
        // NOT quit without confirmation).
        assert!(
            app.imp().running.get(),
            "quit must not stop the loop before the dialog is confirmed"
        );

        // The dialog was presented (unparented here: no active window exists).
        let dialog = gtk4::Window::list_toplevels()
            .iter()
            .find_map(find_close_confirmation)
            .expect("quit presents a CloseConfirmationDialog");
        assert_eq!(dialog.target(), DialogTarget::App);

        // Confirming — the dialog's `close-request` — routes to `quit_now`
        // (`handleCloseConfirmation`, `application.zig:1552-1558`).
        dialog.emit_by_name::<()>("close-request", &[]);
        assert!(
            !app.imp().running.get(),
            "close-request must route to quit_now"
        );

        dialog.close();
        while glib::MainContext::default().iteration(false) {}
    }

    /// F-010 KAT (`application.zig:646-666` `quitNow`): destroys the toplevels
    /// that are OUR `Window` subclass (forcing the ordered widget-teardown
    /// cascade), spares foreign toplevels (the fcitx-IME double-free guard,
    /// `:652-656`), then trips the run-loop exit flag. Against the pre-fix
    /// placeholder (`let _ = window;`) the `Window` survives and this fails.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn quit_now_destroys_our_windows_and_spares_foreign_toplevels() {
        use gio::prelude::ApplicationExt as _;
        use gtk4::prelude::GtkWindowExt as _;

        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        // A registered app: `Window::new` binds `application`, and `startup`
        // installs us as the default `GApplication` for the template machinery.
        let app = build_test_app(std::ptr::null_mut());
        app.register(gio::Cancellable::NONE)
            .expect("register the application");
        app.imp().running.set(true);

        // One of OUR windows, and a foreign toplevel we must NOT destroy (the
        // fcitx-IME stand-in).
        let ours = Window::new(&app, None);
        let foreign = gtk4::Window::new();
        let foreign_widget = foreign.upcast_ref::<gtk4::Widget>().clone();
        assert!(
            gtk4::Window::list_toplevels()
                .iter()
                .any(|w| w.downcast_ref::<Window>().is_some()),
            "precondition: our Window is a live toplevel"
        );

        app.quit_now();

        let toplevels = gtk4::Window::list_toplevels();
        assert!(
            !toplevels
                .iter()
                .any(|w| w.downcast_ref::<Window>().is_some()),
            "quit_now destroys our Window toplevels (application.zig:657-660)"
        );
        assert!(
            toplevels.contains(&foreign_widget),
            "foreign toplevels are spared (the fcitx guard, application.zig:652-656)"
        );
        assert!(
            !app.imp().running.get(),
            "quit_now clears the running flag (application.zig:664)"
        );

        foreign.destroy();
        drop(ours);
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

    /// V14 (`.port/phase-2/VERIFICATION_DEBT.md`): drive
    /// [`imp::Application::signal_css_parsing_error`] ITSELF, not just the
    /// `CssSection::bytes()` binding it calls.
    ///
    /// PORTS the acceptance surface of `application.zig:1261-1274`: on a >= 4.16
    /// build box the handler logs the OFFENDING CSS SOURCE BYTES after the
    /// location/domain/code/message; on the 4.14 floor the `if (comptime
    /// gtk_version.atLeast(4, 16, 0))` arm folds away and the log carries only the
    /// fallback line (`:1275-1280`).
    ///
    /// This is a LIB test because `signal_css_parsing_error` is `pub(super)` and
    /// therefore unreachable from `tests/gtk_css_parse_error.rs`, which can only
    /// prove the binding.
    ///
    /// The assertion is a DIFFERENTIAL, not a one-sided claim: the expected
    /// presence of the bytes is `cfg!(feature = "gtk-v4_16")`, so the same test
    /// body asserts PRESENT under `--features gtk,gtk-v4_16` and ABSENT under
    /// `--features gtk`. Either arm fails if the handler's gate does not match its
    /// feature.
    ///
    /// G-GTKINIT: this reuses the lib-test binary's ONE `gtk4::init()` the same way
    /// every other headed test in this module does (soft-skip when GTK is already
    /// bound to a different libtest thread) — it does not add a new init to a
    /// `tests/` binary.
    #[test]
    #[ignore = "headed GTK. run: \
                cargo test -p rustty-apprt --features gtk[,gtk-v4_16] --lib \
                css_parsing_error -- --ignored"]
    fn signal_css_parsing_error_byte_dump_follows_the_feature_ceiling() {
        if gtk4::init().is_err() {
            // Another headed test in this binary already bound GTK to its thread.
            // Loud, so a filtered run can never be mistaken for evidence.
            println!(
                "V14: SKIPPED — GTK is bound to another test thread; \
                 re-run with `--lib css_parsing_error`"
            );
            return;
        }
        let _ = libadwaita::init();

        // The runtime half of the two-part gate: with the feature on but a < 4.16
        // shared library the arm self-disables and there is nothing to prove.
        #[cfg(feature = "gtk-v4_16")]
        assert!(
            crate::gtk::gtk_version::at_least(4, 16, 0),
            "this KAT needs a >= 4.16 GTK runtime; the byte-dump arm self-disables below it"
        );

        const BAD_CSS: &str = "window { color: ###notacolor; }";

        let app = build_test_app(std::ptr::null_mut());

        // The production wiring shape (`:1201-1207` / `Application::new`): a real
        // `CssProvider::parsing-error` emission, forwarded straight into the
        // handler under test with the section and error GTK itself produced.
        let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let provider = gtk4::CssProvider::new();
        provider.connect_parsing_error(glib::clone!(
            #[weak]
            app,
            #[strong]
            calls,
            move |_provider, section, error| {
                calls.set(calls.get() + 1);
                app.imp().signal_css_parsing_error(section, error);
            }
        ));

        // task-14d-3: this used to dup(2)-redirect fd 2 and grep the bytes,
        // because the handler wrote through a raw `eprintln!`. It now goes
        // through the `log::` facade, so the records are read from the capture
        // logger instead — which is strictly better evidence (level and scope
        // are pinned too) and removes this test's `--nocapture` dependency for
        // the log half. The `capture_stderr` fd-2 helper had no other caller
        // and is deleted with it.
        let cap = crate::log_capture::start();
        provider.load_from_string(BAD_CSS);
        let logged = cap.messages();
        drop(cap);

        assert!(
            calls.get() > 0,
            "a malformed stylesheet must raise parsing-error and reach the handler"
        );
        assert!(
            !logged.is_empty(),
            "the handler emitted NOTHING — the assertions below would be vacuous"
        );

        println!(
            "V14 EVIDENCE (gtk-v4_16 = {}): {logged:?}",
            cfg!(feature = "gtk-v4_16"),
        );

        // Both arms log the fallback fields (`:1275-1280`), at warn, under this
        // file's own reference scope.
        assert!(
            logged.iter().any(|(level, target, msg)| {
                *level == log::Level::Warn
                    && target == LOG_SCOPE
                    && msg.starts_with("css parsing failed at")
            }),
            "the handler must log the fallback line in BOTH arms, at warn under \
             {LOG_SCOPE}; got {logged:?}"
        );

        // The differential: the raw source bytes appear IFF the ceiling is raised.
        let has_bytes = logged.iter().any(|(_, _, msg)| msg.contains(BAD_CSS));
        assert_eq!(
            has_bytes,
            cfg!(feature = "gtk-v4_16"),
            "the `application.zig:1261-1274` byte dump must be present IFF \
             `gtk-v4_16` is on (feature on = {}); logged: {logged:?}",
            cfg!(feature = "gtk-v4_16"),
        );
    }
}

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
