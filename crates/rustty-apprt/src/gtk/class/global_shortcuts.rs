//! rustty-apprt::gtk::class::global_shortcuts
//!
//! PORTS: ghostty `src/apprt/gtk/class/global_shortcuts.zig` (`GhosttyGlobalShortcuts`).
//!
//! A plain `glib::Object` subclass (`global_shortcuts.zig:19-26`,
//! `pub const Parent = gobject.Object`) that registers Ghostty's `global:`
//! keybinds with the desktop's XDG **GlobalShortcuts** portal and re-emits every
//! portal activation as the [`trigger`] signal. The owning [`Application`] connects
//! `trigger` to `core.performAllAction` so a system-wide hotkey performs its action
//! even when Ghostty is unfocused (`application.zig:1450-1456,1655-1663`).
//!
//! # What is REAL in this tier
//!
//! * The class itself — the `config` + `dbus-connection` properties, the
//!   `notify::config` → refresh wiring (`global_shortcuts.zig:118-126`), the
//!   [`GlobalShortcuts::set_dbus_connection`] setter (`:508-540`), and the `dispose`
//!   teardown (`:598-620`).
//! * The [`trigger`] signal (`global_shortcuts.zig:90-102`), carrying the activated
//!   [`binding::Action`]. It reuses [`ActionBoxed`] — the same boxed newtype the
//!   command palette's `trigger` signal uses — as the gtk4-rs spelling of the
//!   reference's `*const Binding.Action` signal parameter.
//! * The `Application` binding: `startupGlobalShortcuts` (`application.zig:1432-1457`)
//!   creates this object, feeds it the app's D-Bus connection, `sync_create`-binds
//!   `config`, and connects `trigger`. That wiring lives in
//!   [`super::application`]'s `startup_global_shortcuts`.
//!
//! # DEFERRAL — the XDG portal round-trip is L6 (portals)
//!
//! The heart of the reference — [`refresh`](global_shortcuts.zig:159-236) building
//! the trigger→action map from the config keybind set and driving the portal
//! `CreateSession`/`BindShortcuts` D-Bus round-trip (`request`, `Method`,
//! `getRequestPath`, `generateToken`, the `responded`/`gotResponseHandle`
//! callbacks, `shortcutActivated`, `onResponse`; `global_shortcuts.zig:238-560`) —
//! is an **inhabited placeholder**. Desktop portals are an L6 milestone (see
//! `.port/phase-2/DEFERRALS.md`), so [`GlobalShortcuts::refresh`] performs the
//! *real* head (close prior state, require a config) and then **no-ops** rather
//! than panicking: `notify::config` and `set_dbus_connection` drive `refresh` on
//! the hot path at startup, so an `unimplemented!()` here would be a latent abort
//! the moment a user has a single `global:` keybind. When the portal tier lands,
//! the map-build + round-trip + the `handle`/subscription session state + the
//! `close` teardown of those subscriptions all attach to `refresh`/`close`, and
//! `shortcutActivated` becomes the sole emitter of [`trigger`].
//!
//! [`Application`]: super::application::Application
//! [`binding::Action`]: rustty_input::binding::Action
//! [`trigger`]: GlobalShortcuts#signal-trigger
//! [`ActionBoxed`]: super::command_palette::ActionBoxed

use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::glib;
use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use super::command_palette::ActionBoxed;
use super::config::Config;

mod imp {
    use super::*;

    /// PORTS: `global_shortcuts.zig:54-83` `GlobalShortcuts.Private`.
    ///
    /// Only the two SHARED refcounted fields that the REAL code paths touch are
    /// modeled here: `config` (`:56`) and `dbus_connection` (`:59`). The portal
    /// session state — `arena` (`:62`), `map` (`:68`), `handle` (`:72`),
    /// `response_subscription` (`:76`), `activate_subscription` (`:80`) — is part
    /// of the deferred XDG round-trip (module doc) and lands with the portal tier.
    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::GlobalShortcuts)]
    pub struct GlobalShortcuts {
        /// PORTS: `global_shortcuts.zig:30-38,56` the `config` property / private
        /// field (SHARED `?*Config`; obj accessor: get refs `.none`, set unrefs the
        /// prior and refs the new — automatic under gtk4-rs).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// PORTS: `global_shortcuts.zig:40-49,59` the `dbus-connection` property /
        /// private field (SHARED `?*gio.DBusConnection`). The reference's separate
        /// `setDbusConnection` method (`:508-540`) does close/ref/refresh side
        /// effects; here that logic is the property's custom setter so a
        /// `set_property("dbus-connection", ...)` — the only write path — runs it
        /// and the property stays writable (as the reference's field accessor is).
        #[property(name = "dbus-connection", get, set = Self::set_dbus_connection, nullable)]
        pub dbus_connection: RefCell<Option<gio::DBusConnection>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GlobalShortcuts {
        // PORTS: `global_shortcuts.zig:22` `.name = "GhosttyGlobalShortcuts"`.
        const NAME: &'static str = "GhosttyGlobalShortcuts";
        type Type = super::GlobalShortcuts;
        // ParentType defaults to `glib::Object` (`global_shortcuts.zig:19` Parent).

        /// PORTS: `global_shortcuts.zig:622-638` `Class.init`. Registers the
        /// boxed action GType the `trigger` signal param needs interned.
        fn class_init(_klass: &mut Self::Class) {
            ActionBoxed::ensure_type();
        }
    }

    impl GlobalShortcuts {
        /// PORTS: `global_shortcuts.zig:508-540` `setDbusConnection`. The custom
        /// setter for the `dbus-connection` property: swap the connection, closing
        /// any prior portal registrations first, then refresh. Reached through the
        /// generated `set_dbus_connection` wrapper / `set_property` — GObject emits
        /// `notify::dbus-connection` afterward (the reference's trailing
        /// `notifyByPspec`, `:539`).
        fn set_dbus_connection(&self, dbus_connection: Option<gio::DBusConnection>) {
            let obj = self.obj();

            // If we have a prior dbus connection we need to close our prior
            // registrations first, then drop its ref (`:517-524`).
            if self.dbus_connection.borrow().is_some() {
                obj.close();
                self.dbus_connection.replace(None);
            }

            // Adopt the new connection (the stored value holds the ref, mirroring
            // the reference `v.ref()`) and refresh (`:526-537`).
            if let Some(v) = dbus_connection {
                self.dbus_connection.replace(Some(v));
                obj.refresh();
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for GlobalShortcuts {
        /// PORTS: `global_shortcuts.zig:90-102` `signals` — `trigger`, carrying the
        /// activated action (`&.{*const Binding.Action}` → [`ActionBoxed`]).
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![glib::subclass::Signal::builder("trigger")
                    .param_types([ActionBoxed::static_type()])
                    .build()]
            })
        }

        /// PORTS: `global_shortcuts.zig:104-112` `init` — connect `notify::config`
        /// → `propConfig` (`:565-572`), which re-runs [`refresh`] whenever the
        /// bound config changes. The reference connects this in `init`; here it is
        /// deferred to `constructed` because the handler is expressed on the wrapper.
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().connect_notify_local(Some("config"), |gs, _pspec| {
                gs.refresh();
            });
        }

        /// PORTS: `global_shortcuts.zig:598-620` `dispose`. Drops references here so
        /// we lose access to things like the D-Bus connection, so we must close all
        /// connections right away instead of in finalize (`:600-602`). Then the
        /// `config` + `dbus_connection` refs are cleared.
        fn dispose(&self) {
            self.obj().close();
            self.config.replace(None);
            self.dbus_connection.replace(None);
        }
    }
}

glib::wrapper! {
    /// PORTS: `global_shortcuts.zig:19` `pub const GlobalShortcuts`.
    pub struct GlobalShortcuts(ObjectSubclass<imp::GlobalShortcuts>);
}

impl GlobalShortcuts {
    /// PORTS: `application.zig:402` `gobject.ext.newInstance(GlobalShortcuts, .{})`.
    #[must_use]
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// PORTS: `global_shortcuts.zig:159-236` `refresh`. Re-derive the global
    /// shortcut registration from the current config.
    ///
    /// DEFERRAL (portals, L6 — see module doc): the *real* head runs (close prior
    /// state; a config is required to proceed), but building the trigger→action map
    /// from `config.keybind.set` and the XDG `CreateSession`/`BindShortcuts`
    /// round-trip (`:170-236`) is an **inhabited no-op** here — deliberately NOT an
    /// `unimplemented!()`, because `notify::config` / [`Self::set_dbus_connection`]
    /// drive this on the startup hot path and a panic would abort the app the
    /// moment a user has a `global:` keybind.
    fn refresh(&self) {
        // Always close our previous state first (`:161`).
        self.close();

        // We need configuration to proceed (`:166`).
        if self.imp().config.borrow().is_none() {
            return;
        }

        // DEFERRAL: map-build + portal round-trip (`:168-236`) lands with L6.
    }

    /// PORTS: `global_shortcuts.zig:128-157` `close`. Tear down the current portal
    /// session (unsubscribe the response/activate signals, close the session
    /// handle, clear the refresh arena).
    ///
    /// DEFERRAL (portals, L6 — see module doc): the session `handle`, the two D-Bus
    /// signal subscriptions, and the per-refresh arena are all portal-round-trip
    /// state that this tier never creates, so `close` is an inhabited no-op until
    /// that state exists. It is called on every `refresh` and in `dispose` so its
    /// call sites stay faithful.
    fn close(&self) {
        // DEFERRAL: unsubscribe(response/activate), Session.Close(handle), arena
        // deinit (`:131-156`) land with L6.
    }
}

impl Default for GlobalShortcuts {
    fn default() -> Self {
        Self::new()
    }
}
