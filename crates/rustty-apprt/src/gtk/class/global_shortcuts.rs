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
//! # The XDG portal round-trip
//!
//! [`GlobalShortcuts::refresh`] rebuilds the trigger→action map from the config's
//! `global:` keybinds and, when the map is non-empty, drives the portal
//! handshake: `CreateSession` → (on `Response` code 0) subscribe to `Activated`
//! and issue `BindShortcuts`. Each request pre-computes its own `Request` object
//! path from the connection's unique name plus the `handle_token` it is about to
//! send, and subscribes to `Response` on that path BEFORE issuing the call,
//! because a portal may emit `Response` before a naive client has subscribed and
//! the signal is delivered exactly once (`global_shortcuts.zig:340-360`).
//!
//! [`GlobalShortcuts::close`] is the exact inverse: unsubscribe both signals, ask
//! the portal to `Session.Close` the handle, and drop the map. It runs at the head
//! of every `refresh`, on a `dbus-connection` swap, and in `dispose`.
//!
//! An `Activated` signal carries the shortcut id; it is looked up in the map and
//! the bound action is re-emitted on the [`trigger`] signal, which
//! [`super::application`] connects to `core.performAllAction`.
//!
//! # Environment requirements (empirically established, WS15 T1)
//!
//! The portal refuses `CreateSession` with
//! `org.freedesktop.portal.Error.NotAllowed: "An app id is required"` unless
//! `xdg-desktop-portal` can determine the caller's app id. For a non-sandboxed
//! host process that means being launched inside an `app.slice` systemd unit
//! named after the application id (`app-<appid>-<tag>.scope|.service`) with a
//! matching `<appid>.desktop` present. A desktop-session launch satisfies this;
//! a bare shell launch does not. On GNOME 50 `BindShortcuts` additionally blocks
//! on a one-time user confirmation dialog, after which the grant is remembered in
//! the `org.gnome.settings-daemon.global-shortcuts` `applications` list.
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

use rustty_input::binding::{Action, Trigger, Value};

use crate::gtk::key;

/// `const log = std.log.scoped(.gtk_ghostty_global_shortcuts)`
/// (`global_shortcuts.zig:15`). REBRANDED per the task-14d-2 precedent (see
/// `.port/tools/PARITY_NAMING.md`); the GType registered NAME is not.
const LOG_SCOPE: &str = "gtk_rustty_global_shortcuts";

/// A global shortcut that failed to bind or was revoked.
/// Only valid for the duration of the signal emission.
///
/// PORTS: `global_shortcuts.zig:85-101` `GlobalShortcuts.BindFailed`.
///
/// The per-binding failure channel the `vicinae_hotkey_v1` winproto path needs and
/// the XDG portal path has no analogue for: the compositor answers every bind
/// request individually with `bound` / `denied(reason, message)` / `revoked(reason,
/// message)`, and a denial that went nowhere would leave the user with a keybind
/// that silently does nothing.
///
/// PORT NOTE (`:88` "Only valid for the duration of the signal emission"): the
/// reference's `message` is a borrowed `[*:0]const u8` pointing into the Wayland
/// event's buffer, which is why its lifetime is the emission. A `#[glib::Boxed]`
/// type must be `Clone`, and the boxed value outlives the emission in gtk4-rs'
/// `Value` machinery, so the port OWNS the message. The observable contract is
/// unchanged and strictly safer.
///
/// PORT NOTE (byte fidelity): `message` is compositor-supplied wire data, so it is
/// kept as [`Vec<u8>`] rather than a lossy `String`. It is decoded (lossily, for
/// display only) at the one place it reaches a user — the desktop notification
/// body in [`super::application`].
// No `Eq`: `Binding.Action` carries `f64` payloads, so it is `PartialEq` only.
#[derive(Clone, Debug, PartialEq, glib::Boxed)]
#[boxed_type(name = "GhosttyGlobalShortcutsBindFailed", nullable)]
pub struct BindFailed {
    /// PORTS: `global_shortcuts.zig:86` `trigger: Binding.Trigger`.
    pub trigger: Trigger,
    /// PORTS: `global_shortcuts.zig:87` `action: Binding.Action`.
    pub action: Action,
    /// May be empty.
    ///
    /// PORTS: `global_shortcuts.zig:89-90` `message`.
    pub message: Vec<u8>,
    /// Revoked after binding rather than denied up front.
    ///
    /// PORTS: `global_shortcuts.zig:92-93` `revoked`.
    pub revoked: bool,
}

mod imp {
    use super::*;

    /// PORTS: `global_shortcuts.zig:54-83` `GlobalShortcuts.Private`.
    ///
    /// The two SHARED refcounted fields — `config` (`:56`) and `dbus_connection`
    /// (`:59`) — plus the portal session state: `map` (`:68`), `handle` (`:72`),
    /// `response_subscription` (`:76`), `activate_subscription` (`:80`). The
    /// reference's `arena` (`:62`) has no counterpart: it exists only to own the
    /// map's key strings, which Rust owns in the `Vec` itself. Its reference doc:
    /// "An arena allocator that is present for each refresh."
    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::GlobalShortcuts)]
    pub struct GlobalShortcuts {
        /// The configuration that this is using.
        ///
        /// PORTS: `global_shortcuts.zig:30-38,56` the `config` property / private
        /// field (SHARED `?*Config`; obj accessor: get refs `.none`, set unrefs the
        /// prior and refs the new — automatic under gtk4-rs).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// The dbus connection.
        ///
        /// PORTS: `global_shortcuts.zig:40-49,59` the `dbus-connection` property /
        /// private field (SHARED `?*gio.DBusConnection`). The reference's separate
        /// `setDbusConnection` method (`:508-540`) does close/ref/refresh side
        /// effects; here that logic is the property's custom setter so a
        /// `set_property("dbus-connection", ...)` — the only write path — runs it
        /// and the property stays writable (as the reference's field accessor is).
        #[property(name = "dbus-connection", get, set = Self::set_dbus_connection, nullable)]
        pub dbus_connection: RefCell<Option<gio::DBusConnection>>,

        /// A mapping from a unique ID to an action.
        /// Currently the unique ID is simply the serialized representation of the
        /// trigger that was used for the action as triggers are unique in the keymap,
        /// but this may change in the future.
        ///
        /// PORTS: `global_shortcuts.zig:64-68` `map`. The id is the serialized XDG
        /// spelling of the trigger. Insertion order is preserved (the reference uses a `StringArrayHashMapUnmanaged`),
        /// because that is the order the shortcuts are presented to the portal.
        ///
        /// PORT NOTE: the reference allocates the keys in a per-refresh arena
        /// (`:61-62`) and drops the whole arena in `close`; Rust owns the strings
        /// in the `Vec`, so `close` just clears it and the arena field has no
        /// counterpart.
        pub map: RefCell<Vec<(String, rustty_input::binding::Action)>>,

        /// The handle of the current global shortcuts portal session,
        /// as a D-Bus object path.
        ///
        /// PORTS: `global_shortcuts.zig:70-72` `handle` (`None` when no session
        /// exists).
        pub handle: RefCell<Option<String>>,

        /// The D-Bus signal subscription for the response signal on requests.
        /// The ID is guaranteed to be non-zero, so we can use 0 to indicate null.
        ///
        /// PORTS: `global_shortcuts.zig:74-76` `response_subscription`. The
        /// reference uses `0` as the null id; gio's `SignalSubscription` is an
        /// RAII guard (dropping it is the `signalUnsubscribe` call), so `Option`
        /// is the faithful spelling and `close`'s explicit unsubscribe is a
        /// `.take()`.
        pub response_subscription: RefCell<Option<gio::SignalSubscription>>,

        /// The D-Bus signal subscription for the keybind activate signal.
        /// The ID is guaranteed to be non-zero, so we can use 0 to indicate null.
        ///
        /// PORTS: `global_shortcuts.zig:78-80` `activate_subscription` (an
        /// `Option` guard, as above).
        pub activate_subscription: RefCell<Option<gio::SignalSubscription>>,
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
            // The `bind-failed` signal param needs its boxed GType interned too
            // (`global_shortcuts.zig:97-100` `defineBoxed`).
            super::BindFailed::ensure_type();
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
            // the reference `v.ref()`) and refresh (`:526-537`). The reference
            // remarks on that call:
            //   v.ref(); // Weird this doesn't return self
            // (a gobject-zig binding quirk; the owned `v` here IS the ref).
            if let Some(v) = dbus_connection {
                self.dbus_connection.replace(Some(v));
                obj.refresh();
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for GlobalShortcuts {
        /// PORTS: `global_shortcuts.zig:103-127` `signals` — `trigger`, carrying the
        /// activated action (`&.{*const Binding.Action}` → [`ActionBoxed`]), and
        /// `bind-failed`, carrying the failed binding (`&.{*const BindFailed}` →
        /// [`super::BindFailed`]).
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // Emitted whenever a global shortcut is triggered.
                    glib::subclass::Signal::builder("trigger")
                        .param_types([ActionBoxed::static_type()])
                        .build(),
                    // Emitted when a global shortcut fails to bind or is revoked.
                    // `pub const @"bind-failed"` (`:117-126`).
                    glib::subclass::Signal::builder("bind-failed")
                        .param_types([super::BindFailed::static_type()])
                        .build(),
                ]
            })
        }

        /// PORTS: `global_shortcuts.zig:104-112` `init` — connect `notify::config`
        /// → `propConfig` (`:565-572`), which re-runs [`refresh`] whenever the
        /// bound config changes. The reference connects this in `init`; here it is
        /// deferred to `constructed` because the handler is expressed on the wrapper.
        fn constructed(&self) {
            self.parent_constructed();
            self.obj()
                .connect_notify_local(Some("config"), |gs, _pspec| {
                    gs.refresh();
                });
        }

        /// PORTS: `global_shortcuts.zig:598-620` `dispose`. Close, then clear the
        /// `config` + `dbus_connection` refs.
        fn dispose(&self) {
            // Since we drop references here we may lose access to things like
            // dbus connections, so we need to close all our connections right
            // away instead of in finalize.
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

    /// PORTS: `global_shortcuts.zig:148-213` `refresh`. Re-derive the global
    /// shortcut registration from the current config: close the previous session,
    /// rebuild the trigger→action map, and — if anything is registered — start the
    /// portal round-trip with `CreateSession`.
    // UNRESTORED SITES (task-14d-3), all five of this file's residual deficit,
    // all the same cause — the reference's fallible call is INFALLIBLE here, so
    // there is no `catch` arm and restoring the diagnostic would put it on a
    // success path:
    //   `:239` `buffer too small to translate trigger, ignoring={f}` — the
    //          reference formats the trigger into a fixed 1024-byte stack
    //          buffer that can overflow; this port formats into a `String`.
    //   `:345` `out of memory: failed to clone session handle` — the handle is
    //          an owned `String`; the reference's `dupeZ` can fail.
    //   `:365` `failed to bind shortcuts={}` — [`Self::request`] returns `()`.
    //   `:567` and `:580` `error refreshing global shortcuts: {}` — this
    //          function returns `()`.
    fn refresh(&self) {
        // Always close our previous state first.
        // (`:150`)
        self.close();

        let imp = self.imp();

        // We need a configuration to proceed.
        // (`:155`)
        let map = {
            let config = imp.config.borrow();
            let Some(config) = config.as_ref() else {
                return;
            };

            // Prefer a windowing protocol native mechanism over the
            // XDG desktop portal when available.
            //
            // (`:184-190`, oracle `88bd4fdce`) Returns
            // false on `noop`/`x11` and on a Wayland compositor that does not
            // advertise `vicinae_hotkey_manager_v1`, in which case we fall through
            // to the portal path below — the two mechanisms are ADDITIVE WITH
            // PREFERENCE, not a replacement.
            //
            // IMPLEMENTATION HAZARD (`RefCell`): `Application::winproto()` hands out
            // a `Ref<'_, winproto::App>` while `bind_global_shortcuts`/
            // `clear_global_shortcuts` need `&mut`, so BOTH call sites go through
            // `winproto_mut()` and neither may straddle a live `winproto()` borrow.
            // `close()` above took (and dropped) its own `RefMut` inside its own
            // statement; this one is likewise a statement temporary. Holding either
            // across the other turns the upstream crash fix into a `RefCell`
            // double-borrow panic.
            if let Some(app) = app_default() {
                if app.winproto_mut().bind_global_shortcuts(self, config.get()) {
                    // log.debug("global shortcuts bound via winproto", .{});
                    log::debug!(target: LOG_SCOPE, "global shortcuts bound via winproto");
                    return;
                }
            }

            // DELIBERATE DIVERGENCE (recorded, `P3-W11 T4` / `P3-W8 T6 Step 2`):
            // upstream `0a117e079` adds, here (`:192-193`), before the map is built:
            //
            //     // The portal fallback requires a dbus connection.
            //     if (priv.dbus_connection == null) return;
            // rustty KEEPS building the
            // map without a bus: two KATs assert exactly that
            // (`global_shortcuts_refresh_builds_map_without_dbus`,
            // `…_clears_prior_map`), and upstream's early return is only an
            // optimisation — `request()` already returns instead of unwrapping a
            // null connection (`:349-360`), so once a bus exists the observable
            // behaviour is identical.
            build_shortcut_map(config.get())
        };

        let count = map.len();
        imp.map.replace(map);

        // Create our session if we have global shortcuts.
        // (`:212`)
        if count > 0 {
            self.request(Method::CreateSession);
        }
    }

    /// PORTS: `global_shortcuts.zig:109-146` `close`. Tear down the current portal
    /// session: drop both signal subscriptions, ask the portal to close the
    /// session, and clear the map.
    fn close(&self) {
        let imp = self.imp();

        // This is safe to call even if the winproto was already
        // deinitialized, which happens during application teardown.
        //
        // `Application.default().winproto().clearGlobalShortcuts();` (`:141-146`,
        // oracle `0a117e079`'s tail) — UNCONDITIONAL, i.e. not behind the
        // `dbus_connection` check, because the winproto bindings exist precisely
        // when the portal session does not. In rustty the `RefCell<winproto::App>` always holds a live
        // (possibly `.none`) arm, so the same call is safe for the same reason.
        //
        // Routed through `winproto_mut()`: `clear_global_shortcuts` needs `&mut`
        // (see the hazard note in `refresh`). Taken and dropped within this one
        // statement so `refresh`'s own `winproto_mut()` cannot double-borrow.
        if let Some(app) = app_default() {
            app.winproto_mut().clear_global_shortcuts();
        }

        // `:113-121` — dropping a `SignalSubscription` unsubscribes it.
        //
        // PORT NOTE (ordering, WS15 review): these run BEFORE the
        // `dbus_connection` early-return. Each guard owns its own ref to the
        // connection it was created on, so a `close()` reached after the
        // connection field was already cleared must still unsubscribe rather than
        // leak two live subscriptions. The reference's `orelse return` cannot hit
        // this case because Zig's `priv.dbus_connection` and its subscriptions are
        // cleared together; hoisting the drops makes the port unconditionally
        // correct with no behavioural difference on the reachable paths.
        drop(imp.response_subscription.borrow_mut().take());
        drop(imp.activate_subscription.borrow_mut().take());

        // `const dbus = priv.dbus_connection orelse return;` (`:111`)
        let dbus = { imp.dbus_connection.borrow().clone() };
        let Some(dbus) = dbus else {
            // Without a connection there is no session to close, but the map may
            // still hold state from a headless refresh.
            imp.map.replace(Vec::new());
            imp.handle.replace(None);
            return;
        };

        // Close existing session
        // (`:123-139`)
        let handle = imp.handle.borrow_mut().take();
        if let Some(handle) = handle {
            dbus.call(
                Some(PORTAL_BUS_NAME),
                &handle,
                SESSION_IFACE,
                "Close",
                None,
                None,
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
                |_res| {},
            );
        }

        // `arena.deinit(); priv.map = .{}` (`:141-145`) — Rust owns the map's
        // strings, so clearing the vec is the whole teardown.
        imp.map.replace(Vec::new());
    }

    /// PORTS: `global_shortcuts.zig:558-564` — the display-free half of
    /// `shortcutActivated`: look the portal's shortcut id up in the map and
    /// re-emit the bound action on the `trigger` signal. Unknown ids are ignored.
    ///
    /// Split out from the D-Bus callback so it can be exercised without a bus.
    pub(crate) fn dispatch_activation(&self, shortcut_id: &str) {
        let action = {
            let map = self.imp().map.borrow();
            map.iter()
                .find(|(k, _)| k == shortcut_id)
                .map(|(_, a)| a.clone())
        };
        let Some(action) = action else {
            return;
        };

        // PORT NOTE (rustty-only instrumentation, KEPT — unlike S50-F4's
        // success line, which was removed): the reference logs nothing here.
        // This line is reachable only AFTER the map lookup hit, so it separates
        // "signal arrived" from "signal resolved to a bound action" for
        // `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh`, which has no
        // other way to tell those two apart — the reference's own
        // `activated={s}` fires before the lookup. It is deliberately NOT
        // emitted under this file's reference `LOG_SCOPE`, so it cannot be
        // mistaken for a restored reference diagnostic; see the task-14d-3
        // report's inverse-defect heading, which lists it as a candidate a
        // later stage may still want to remove.
        log::debug!(
            target: "rustty_gtk_instrumentation",
            "global shortcut dispatch map-hit id={shortcut_id}"
        );

        // `self.emitTrigger(&action)` (`:605`).
        self.emit_trigger(&action);
    }

    /// Emit the trigger signal. Public for winproto shortcut backends.
    ///
    /// PORTS: `global_shortcuts.zig:607-616` `emitTrigger`.
    pub fn emit_trigger(&self, action: &Action) {
        self.emit_by_name::<()>("trigger", &[&ActionBoxed(action.clone())]);
    }

    /// Emit the bind-failed signal. Public for winproto shortcut backends.
    ///
    /// PORTS: `global_shortcuts.zig:618-626` `emitBindFailed`.
    pub fn emit_bind_failed(&self, failure: &BindFailed) {
        self.emit_by_name::<()>("bind-failed", &[&failure.clone()]);
    }

    /// The current portal shortcut map (id → action), in registration order.
    /// rustty-only accessor: the reference reads `priv.map` directly in-file, so
    /// this exists only for the headless KATs.
    #[cfg(test)]
    pub(crate) fn shortcut_map(&self) -> Vec<(String, Action)> {
        self.imp().map.borrow().clone()
    }

    /// Submit a request to the global shortcuts portal.
    ///
    /// PORTS: `global_shortcuts.zig:329-460` `request`.
    fn request(&self, method: Method) {
        // NOTE(pluiedev):
        // How the protocol is _initially_ designed to work is as follows:
        //
        // 1. The client calls a method which returns the path of a Request object;
        // 2. The client waits for the Response signal under said object path;
        // 3. When the signal arrives, the actual return value and status code
        //    become available for the client for further processing.
        //
        // THIS DOES NOT WORK. Once the first two steps are complete, the client
        // needs to immediately start listening for the third step, but an overeager
        // server implementation could easily send the Response signal before the
        // client is even ready, causing communications to break down over a simple
        // race condition/two generals' problem that even _TCP_ had figured out
        // decades ago. Worse yet, you get exactly _one_ chance to listen for the
        // signal, or else your communication attempt so far has all been in vain.
        //
        // Instead of fixing the protocol, the portal documentation asks clients
        // to manually construct the expected object path and subscribe
        // to the request signal beforehand, making the whole response value of
        // the original call COMPLETELY MEANINGLESS.
        //
        // Furthermore, this is _entirely undocumented_ aside from one tiny
        // paragraph under the documentation for the Request interface, and
        // anyone would be forgiven for missing it without reading the libportal
        // source code.
        //
        // (`:379-406`; the note's editorializing is stripped per the port's
        // prose policy, `strip-ghostty-gag-prose` — the three dropped lines,
        // `:380`, `:396` and `:406`, are declared in
        // `.port/parity-closure/COMMENT-NOT-CARRIED.tsv`.)
        let imp = self.imp();

        let dbus = { imp.dbus_connection.borrow().clone() };
        let Some(dbus) = dbus else {
            // The reference reaches `priv.dbus_connection.?` here; `refresh` is
            // driven by `notify::config` too, which can fire before the connection
            // is set. Returning is the LATENT-CRASH-safe equivalent of a
            // round-trip that cannot be started.
            return;
        };

        let request_token = generate_token();

        // Construct the payload expected by the XDG portal call.
        // `method.makePayload(self, request_token) orelse return` (`:428`).
        let payload = match method {
            Method::CreateSession => create_session_payload(&request_token, &generate_token()),
            Method::BindShortcuts => {
                let handle = { imp.handle.borrow().clone() };
                let Some(handle) = handle else {
                    return;
                };
                let map = imp.map.borrow();
                let Some(payload) = bind_shortcuts_payload(&handle, map.as_slice(), &request_token)
                else {
                    return;
                };
                payload
            }
        };

        // `getRequestPath` (`:429`).
        // `getUniqueName` should never return null here as we're using an ordinary
        // message bus connection. If it doesn't, something is very wrong
        // (`:476-478`).
        let Some(unique_name) = dbus.unique_name() else {
            // PORT NOTE: the reference's `getUniqueName` null arm
            // (`global_shortcuts.zig:476-478`) returns `error.NoDBusUniqueName`
            // without logging; this warn is rustty-only and, like the map-hit
            // line, is not emitted under the reference scope.
            log::warn!(
                target: "rustty_gtk_instrumentation",
                "global shortcuts: dbus connection has no unique name"
            );
            return;
        };
        let request_path = request_object_path(unique_name.as_str(), &request_token);

        // `assert(priv.response_subscription == 0)` (`:434`).
        debug_assert!(
            imp.response_subscription.borrow().is_none(),
            "a global shortcuts request is already in flight"
        );

        // `dbus.signalSubscribe(.., Request, Response, request_path, ..)`
        // (`:435-445`) → `callbacks.responded` (`:390-422`).
        let weak = self.downgrade();
        let subscription = dbus.subscribe_to_signal(
            None,
            Some(REQUEST_IFACE),
            Some("Response"),
            Some(&request_path),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let Some(this) = weak.upgrade() else {
                    return;
                };

                // Unsubscribe from the response signal (`:402-406`).
                drop(this.imp().response_subscription.borrow_mut().take());

                this.responded(method, signal.parameters);
            },
        );
        imp.response_subscription.replace(Some(subscription));

        // The method call itself (`:447-459`). Its reply is informational (see the
        // PROTOCOL NOTE); we only log a failure, as `gotResponseHandle` does
        // (`:363-387`).
        let method_name = method.name();
        dbus.call(
            Some(PORTAL_BUS_NAME),
            PORTAL_OBJECT_PATH,
            GLOBAL_SHORTCUTS_IFACE,
            method_name,
            Some(&payload),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
            move |res| {
                // `callbacks.gotResponseHandle` (`:363-387`): the reply carries
                // the portal's ACTUAL request object path.
                //
                // TODO: XDG recommends updating the signal subscription if the actual
                // returned request path is not the same as the expected request
                // path, to retain compatibility with older versions of XDG portals.
                // Although it suffers from the race condition outlined above,
                // we should still implement this at some point.
                //
                // (Not implemented here either, as in the reference.)
                if let Err(err) = res {
                    // log.warn("request failed={s} ({})", .{ err_.f_message orelse "(unknown)", err_.f_code });
                    let _ = method_name;
                    log::warn!(
                        target: LOG_SCOPE,
                        "request failed={} ({})",
                        err.message(),
                        err.code(),
                    );
                }
            },
        );
    }

    /// See <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html#org-freedesktop-portal-request-response>
    ///
    /// PORTS: `global_shortcuts.zig:442-469` `callbacks.responded`, after its
    /// unsubscribe: read the `Response` body and dispatch on the code. A separate
    /// method (the reference's is a nested callback) so a malformed body can be
    /// driven without a portal.
    fn responded(&self, method: Method, params: &glib::Variant) {
        // var response: u32 = 0;
        // var vardict: ?*glib.Variant = null;
        // defer if (vardict) |v| v.unref();
        // params_.get("(u@a{sv})", &response, &vardict);
        //
        // CORRECTION (task 20, S51-F7): this used to read
        // `params.child_value(0).get().unwrap_or(2)` then
        // `params.child_value(1)`, under a note that said "DELIBERATE
        // DIVERGENCE: the fallback is 2 ("ended unexpectedly"), not 0". Both
        // `child_value` calls ASSERT the index is in range, so a `()` or `(u)`
        // body panicked before the default could apply. See
        // [`response_params`].
        let (response, vardict) = response_params(params);

        // PORT NOTE: reference defect (R13) — `.port/parity-closure/UPSTREAM-DEFECTS.md`
        // UD1. A deliberate divergence (task 20, S51-F7), not an oversight: a body that is
        // not `(ua{sv})` leaves `response` 0 and `vardict` null, and the reference then
        // runs `method.onResponse(self_cb, vardict.?)` — a panic in a safe build, undefined
        // behaviour in ReleaseFast. It is reachable from ANY session-bus peer: this
        // subscription, like the reference's (`:481-491`), has a null sender. Under R13 the
        // port does not reproduce it: code 0 without a vardict takes the `2` arm — the
        // failed response it evidently is, with the reference's own "ended unexpectedly"
        // warning — and `onResponse` is not called.
        match (response, vardict) {
            // `.{0 => method.onResponse(..)}` (`:462-465`)
            (0, Some(vardict)) => {
                // log.debug("request successful", .{});
                log::debug!(target: LOG_SCOPE, "request successful");
                self.on_response(method, &vardict);
            }
            // 1 => log.debug("request was cancelled by user", .{}),
            (1, _) => log::debug!(target: LOG_SCOPE, "request was cancelled by user"),
            // 2 => log.warn("request ended unexpectedly", .{}),
            (2, _) | (0, None) => log::warn!(target: LOG_SCOPE, "request ended unexpectedly"),
            // else => log.warn("unrecognized response code={}", .{response}),
            (other, _) => {
                log::warn!(target: LOG_SCOPE, "unrecognized response code={other}");
            }
        }
    }

    /// PORTS: `global_shortcuts.zig:283-325` `Method.onResponse` — what to do once
    /// a request's `Response` arrives with code 0.
    fn on_response(&self, method: Method, results: &glib::Variant) {
        match method {
            Method::CreateSession => {
                // `vardict.lookup("session_handle", "&s", &handle)` (`:287-293`).
                let Some(handle) = session_handle_from_results(results) else {
                    // log.warn("session handle not found in response={s}", .{vardict.print(true)});
                    log::warn!(
                        target: LOG_SCOPE,
                        "session handle not found in response={}",
                        results.print(true)
                    );
                    return;
                };
                // log.debug("session_handle={?s}", .{handle});
                log::debug!(target: LOG_SCOPE, "session_handle={handle}");
                self.imp().handle.replace(Some(handle.clone()));

                let dbus = { self.imp().dbus_connection.borrow().clone() };
                let Some(dbus) = dbus else {
                    return;
                };

                // Subscribe to keybind activations
                // (`:304-316`). `arg0` is the
                // session handle, matched as an object path.
                debug_assert!(self.imp().activate_subscription.borrow().is_none());
                let weak = self.downgrade();
                let subscription = dbus.subscribe_to_signal(
                    None,
                    Some(GLOBAL_SHORTCUTS_IFACE),
                    Some("Activated"),
                    Some(PORTAL_OBJECT_PATH),
                    Some(&handle),
                    gio::DBusSignalFlags::MATCH_ARG0_PATH,
                    move |signal| {
                        let Some(this) = weak.upgrade() else {
                            return;
                        };
                        // 2nd value in the tuple is the activated shortcut ID
                        // See https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html#org-freedesktop-portal-globalshortcuts-activated
                        // (`:598-601`). See [`activated_shortcut_id`].
                        let Some(id) = activated_shortcut_id(signal.parameters) else {
                            return;
                        };
                        // log.debug("activated={s}", .{shortcut_id});
                        log::debug!(target: LOG_SCOPE, "activated={id}");
                        this.dispatch_activation(&id);
                    },
                );
                self.imp().activate_subscription.replace(Some(subscription));

                // `shortcuts.request(.bind_shortcuts)` (`:318-321`).
                self.request(Method::BindShortcuts);
            }

            // `.bind_shortcuts => {}` (`:323`).
            //
            // PORT NOTE (rustty-only instrumentation, KEPT): the reference's arm
            // is empty. We log the accepted binding set, because this is the
            // single observable that proves the compositor took the shortcuts —
            // it is what `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh`
            // asserts. task-14d-3 moved it OFF this file's reference `LOG_SCOPE`
            // onto a `rustty_gtk_instrumentation` target, so it cannot be
            // mistaken for a restored reference diagnostic and does not inflate
            // this file's reconciliation count.
            Method::BindShortcuts => {
                log::debug!(
                    target: "rustty_gtk_instrumentation",
                    "global shortcuts bound results={}",
                    results.print(true)
                );
            }
        }
    }
}

impl Default for GlobalShortcuts {
    fn default() -> Self {
        Self::new()
    }
}

//---------------------------------------------------------------
// Portal protocol helpers (free functions so they are unit-testable with no
// display and no bus — the oracle keeps them as private methods/`Method` arms
// on the class, which Zig can test in-file and Rust cannot).

/// PORTS: `global_shortcuts.zig:215-326` `Method` — the two portal methods this
/// class drives, in the order it drives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    /// `CreateSession`
    CreateSession,
    /// `BindShortcuts`
    BindShortcuts,
}

impl Method {
    /// PORTS: `global_shortcuts.zig:219-224` `Method.name`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Method::CreateSession => "CreateSession",
            Method::BindShortcuts => "BindShortcuts",
        }
    }
}

/// The XDG desktop portal's well-known bus name.
pub(crate) const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";
/// The XDG desktop portal's root object path.
pub(crate) const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
/// `global_shortcuts.zig:450` — the interface every method call targets.
pub(crate) const GLOBAL_SHORTCUTS_IFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
/// `global_shortcuts.zig:437` — the interface carrying the `Response` signal.
pub(crate) const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";
/// `global_shortcuts.zig:128` — the interface carrying `Close`.
pub(crate) const SESSION_IFACE: &str = "org.freedesktop.portal.Session";

/// Generate a random token suitable for use in requests.
///
/// PORTS: `global_shortcuts.zig:625-633` `generateToken` (a portal
/// `handle_token` / `session_handle_token`).
///
/// R2: the reference prefixes `ghostty_`; rustty prefixes `rustty_`. The reference
/// draws a `u28` and formats `{x:0<7}` (hex, fill `0`, LEFT-aligned to width 7 —
/// i.e. zeros are appended, not prepended); Rust's `{:0<7x}` is the same spec.
pub(crate) fn generate_token() -> String {
    let mut buf = [0u8; 4];
    rustty_os::file::crypto_random_bytes(&mut buf);
    // `std.crypto.random.int(u28)`
    let v = u32::from_ne_bytes(buf) & 0x0fff_ffff;
    // u28 takes up 7 bytes in hex, 8 bytes for "ghostty_" and 1 byte for NUL
    // 7 + 8 + 1 = 16
    // (PORT NOTE: that sizes the reference's `[16]u8` `Token` buffer. R2: the
    // rustty prefix is 7 bytes, and Rust formats into a `String`, so there is
    // no fixed buffer to size; the reference arithmetic is kept as written.)
    format!("rustty_{v:0<7x}")
}

/// Get the XDG portal request path for the current Rustty instance.
///
/// If this sounds like nonsense, see `request` for an explanation as to
/// why we need to do this.
///
/// Precondition: dbus connection exists, arena setup
///
/// PORTS: `global_shortcuts.zig:468-497` `getRequestPath` (a pure function of
/// the unique name and token here; the caller holds the connection).
///
/// The portal's `Request` object path must be computed by the CLIENT and
/// subscribed to BEFORE the method call is made: an eager portal implementation
/// can emit `Response` before a client that waited for the method's return value
/// has subscribed, and the signal is delivered exactly once. The path is
/// `/org/freedesktop/portal/desktop/request/<SENDER>/<TOKEN>`, where `<SENDER>` is
/// the connection's unique name with the leading `:` removed and every `.`
/// replaced by `_` (`:1.192` → `1_192`); D-Bus object-path components may not
/// contain `.`.
pub(crate) fn request_object_path(unique_name: &str, token: &str) -> String {
    // See https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html
    // for the syntax XDG portals expect.

    let sender = unique_name
        // Remove leading `:`
        .strip_prefix(':')
        .unwrap_or(unique_name)
        // Sanitize the unique name by replacing every `.` with `_`.
        // In effect, this will turn a unique name like `:1.192` into `1_192`.
        // Valid D-Bus object path components never contain `.`s anyway, so we're
        // free to replace all instances of `.` here and avoid extra allocation.
        .replace('.', "_");
    format!("{PORTAL_OBJECT_PATH}/request/{sender}/{token}")
}

/// PORTS: `global_shortcuts.zig:234-241` — the `CreateSession` payload.
///
/// D-Bus signature `(a{sv})`: one options vardict carrying the request's
/// `handle_token` and the session's `session_handle_token`.
///
/// See <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html#org-freedesktop-portal-globalshortcuts-createsession>
pub(crate) fn create_session_payload(request_token: &str, session_token: &str) -> glib::Variant {
    use glib::prelude::ToVariant as _;

    let options = glib::VariantDict::new(None);
    options.insert_value("handle_token", &request_token.to_variant());
    options.insert_value("session_handle_token", &session_token.to_variant());
    glib::Variant::tuple_from_iter([options.end()])
}

/// PORTS: `global_shortcuts.zig:244-279` — the `BindShortcuts` payload.
///
/// D-Bus signature `(oa(sa{sv})sa{sv})`: the session handle, the shortcut list
/// (each `(id, {description, preferred_trigger})`), an empty parent-window
/// identifier, and the request options carrying `handle_token`.
///
/// See <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html#org-freedesktop-portal-globalshortcuts-bindshortcuts>
///
/// Returns `None` when `handle` is not a valid D-Bus object path — the reference
/// reaches this point only with a handle the portal itself just gave us
/// (`:246` `priv.handle orelse return null`), so `None` is the same "skip the
/// request" outcome without a panic.
pub(crate) fn bind_shortcuts_payload(
    handle: &str,
    map: &[(String, Action)],
    request_token: &str,
) -> Option<glib::Variant> {
    use glib::prelude::ToVariant as _;

    let session = glib::variant::ObjectPath::try_from(handle).ok()?;
    let entry_ty = glib::VariantTy::new("(sa{sv})").ok()?;

    let entries: Vec<glib::Variant> = map
        .iter()
        .map(|(trigger, action)| {
            let opts = glib::VariantDict::new(None);
            opts.insert_value("description", &action_description(action).to_variant());
            opts.insert_value("preferred_trigger", &trigger.to_variant());
            glib::Variant::tuple_from_iter([trigger.to_variant(), opts.end()])
        })
        .collect();
    let shortcuts = glib::Variant::array_from_iter_with_type(entry_ty, entries);

    let options = glib::VariantDict::new(None);
    options.insert_value("handle_token", &request_token.to_variant());

    Some(glib::Variant::tuple_from_iter([
        session.to_variant(),
        shortcuts,
        "".to_variant(),
        options.end(),
    ]))
}

/// PORTS: `global_shortcuts.zig:598-601` —
/// `var shortcut_id: [*:0]const u8 = undefined; params.getChild(1, "&s", &shortcut_id);`
/// in `shortcutActivated`: the second child of the `Activated` body, typed `s`.
///
/// PORT NOTE: reference defect (R13) — `.port/parity-closure/UPSTREAM-DEFECTS.md` UD2. A
/// deliberate divergence (task 20), not an oversight. The reference fails two ways:
/// - a body with NO second child (`(o)`, `()`): `g_variant_get_child` hits GLib's fatal
///   "Attempt to access item 1 in a container with only 1 items" and the process ABORTS
///   (exit 134, every build mode — measured by the task-20 reviewer on GLib 2.88.2);
/// - a second child of the wrong type (`(oo)`): GLib writes nothing and the reference goes
///   on to log and look up the UNDEFINED `shortcut_id` pointer.
///
/// The subscription matches only `arg0` against the session handle and has a null sender,
/// so such a body from a session-bus peer reaches it. The port used to PANIC on the first
/// shape (`child_value(1)` asserts the index). Under R13 this returns `None` and the
/// handler does nothing, which is what the reference evidently intends for an activation
/// that names no shortcut.
///
/// CORRECTION (task 20 fix round 1): this note first said that on a body with no second
/// child `g_variant_get_child` "writes nothing" — true only of the wrong-typed shape.
pub(crate) fn activated_shortcut_id(params: &glib::Variant) -> Option<String> {
    let child = params.try_child_value(1)?;
    if !child.is_type(glib::VariantTy::STRING) {
        return None;
    }
    child.str().map(str::to_owned)
}

/// PORTS: `global_shortcuts.zig:454-457` — `params_.get("(u@a{sv})", &response,
/// &vardict)` in `callbacks.responded`, returning `(response, vardict)`.
///
/// The reference's `get` is `g_variant_get`, and so is this: on a body that is not
/// `(ua{sv})` GLib emits a critical and writes neither out-parameter, so `response`
/// stays at its initial 0 and `vardict` stays null. The `@a{sv}` reference GLib
/// hands back is owned and is released when the returned `Variant` drops — the
/// reference's `defer if (vardict) |v| v.unref()`.
pub(crate) fn response_params(params: &glib::Variant) -> (u32, Option<glib::Variant>) {
    use glib::translate::{from_glib_full, ToGlibPtr};

    let mut response: u32 = 0;
    let mut vardict: *mut glib::ffi::GVariant = std::ptr::null_mut();
    // SAFETY: `params` is a live `GVariant`. The format is a valid `g_variant_get`
    // format whose conversions (`u`, `@a{sv}`) take a `*mut u32` and a
    // `*mut *mut GVariant`, which is what is passed.
    unsafe {
        glib::ffi::g_variant_get(
            ToGlibPtr::<*mut glib::ffi::GVariant>::to_glib_none(params).0,
            c"(u@a{sv})".as_ptr(),
            &mut response as *mut u32,
            &mut vardict as *mut *mut glib::ffi::GVariant,
        );
    }
    let vardict = if vardict.is_null() {
        None
    } else {
        // SAFETY: `@a{sv}` returns a new (full) reference.
        Some(unsafe { from_glib_full(vardict) })
    };
    (response, vardict)
}

/// PORTS: `global_shortcuts.zig:333-338` — `vardict.lookup("session_handle", "&s",
/// &handle)`.
///
/// CORRECTION (task 20, S51-F6): this used to say "Portal implementations disagree on
/// whether this field is `s` or `o`, so we accept either (`glib::Variant::str` covers
/// `s`/`o`/`g`)". The reference's `&s` is `g_variant_lookup`, which asks for type `s`
/// exactly and fails on an `o`; the oracle then warns `session handle not found` and
/// abandons the session. The lookup is typed `s` here for the same result.
pub(crate) fn session_handle_from_results(results: &glib::Variant) -> Option<String> {
    let dict = glib::VariantDict::new(Some(results));
    let value = dict.lookup_value("session_handle", Some(glib::VariantTy::STRING))?;
    value.str().map(str::to_owned)
}

/// The default [`Application`](super::application::Application), or `None`.
///
/// PORT NOTE: the reference reaches the app with `Application.default()`
/// (`global_shortcuts.zig:143`), which asserts a default `GApplication` exists —
/// true for every reachable runtime path, because this class is only ever created
/// by `startupGlobalShortcuts`. rustty's headless KATs construct a
/// `GlobalShortcuts` standalone (no `GApplication` at all), and rustty's
/// `Application::default()` panics in that case, so the winproto hops go through
/// this non-asserting probe. On the live path it is exactly `Application::default()`;
/// the `None` arm is reachable only from a test that has no application.
fn app_default() -> Option<super::application::Application> {
    use gtk4::prelude::Cast as _;
    gio::Application::default().and_then(|a| a.downcast::<super::application::Application>().ok())
}

/// PORTS: `global_shortcuts.zig:168-206` — the map-build half of `refresh`.
///
/// Walks the config's keybind set and returns the portal shortcut map: an
/// insertion-ordered list of `(xdg shortcut id, action)`. Only `global:` leaves
/// that carry exactly one action are registered; leader (sequence) entries and
/// chained leaves are skipped, matching the reference's `TODO: remove this
/// restriction`. The key is the trigger's XDG spelling because triggers are
/// unique in the keymap (`:64-67`).
///
/// PORT NOTE: the reference formats the trigger into a 1024-byte stack buffer and
/// has a `error.WriteFailed` arm that warns and skips; `xdg_shortcut_from_trigger`
/// returns a growable `String` here so only the `null` arm (an untranslatable
/// trigger — `catch_all`, or a key with no GDK keysym name) can skip a bind.
/// The reference's comment on that arm (`:235-237`):
///
/// ```text
/// If there isn't space to translate the trigger, then our
/// buffer might be too small (but 1024 is insane!). In any case
/// we don't want to stop registering globals.
/// ```
pub(crate) fn build_shortcut_map(config: &rustty_config::Config) -> Vec<(String, Action)> {
    // Setup our new arena that we'll use for memory allocations.
    // (PORT NOTE: `:203-206` — no arena here, see below.)
    //
    // Our map starts out empty again. We don't need to worry about
    // memory because its part of the arena we clear.
    // (PORT NOTE: no arena here — the `Vec` owns its strings.)
    let mut map: Vec<(String, Action)> = Vec::new();

    // Update map

    for (trigger, value) in config.keybind.set.iter_bindings() {
        // `.leader => continue`, `inline .leaf, .leaf_chained => |l| l.generic()`.
        let leaf = match value {
            Value::Leader(_) => continue,
            Value::Leaf(l) => l.generic(),
            Value::LeafChained(l) => l.generic(),
        };

        // `if (!leaf.flags.global) continue;`
        if !leaf.flags.global {
            continue;
        }

        // We only allow global keybinds that map to exactly a single
        // action for now. TODO: remove this restriction
        // (`:179-181`)
        let actions = leaf.actions_slice();
        if actions.len() != 1 {
            continue;
        }

        let Some(shortcut) = key::xdg_shortcut_from_trigger(*trigger) else {
            continue;
        };

        // `priv.map.put(alloc, trigger, actions[0])` — last write wins.
        match map.iter_mut().find(|(k, _)| *k == shortcut) {
            Some(entry) => entry.1 = actions[0].clone(),
            None => map.push((shortcut, actions[0].clone())),
        }
    }

    map
}

/// PORTS: `global_shortcuts.zig:259-263` — the `description` field of a
/// `BindShortcuts` entry: the action in its parse/format spelling (`"{f}"` of
/// `Binding.Action`), e.g. `new_window`, `goto_tab:3`.
///
/// PORT NOTE: the reference formats into a 256-byte stack buffer and `catch
/// continue`s a binding whose action does not fit. `AllocatingWriter` grows, so
/// no binding is ever dropped for length here. `Action::format`'s writer error is
/// `Infallible` for this writer.
pub(crate) fn action_description(action: &Action) -> String {
    let mut w = rustty_cfg_fmt::AllocatingWriter::init();
    let _ = action.format(&mut w);
    String::from_utf8_lossy(w.written()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_input::binding::Action;

    /// Build a config carrying the given keybind lines.
    fn config_with(binds: &[&str]) -> rustty_config::Config {
        let mut config = rustty_config::Config::default_config();
        for b in binds {
            config
                .keybind
                .set
                .parse_and_put(b.as_bytes())
                .expect("keybind parses");
        }
        config
    }

    /// PORTS: `global_shortcuts.zig:168-206` — only `global:` leaves with exactly
    /// one action reach the portal map, keyed by the XDG shortcut spelling.
    #[test]
    fn global_shortcuts_map_only_global_single_action_binds() {
        let config = config_with(&[
            "global:super+q=new_window",
            "ctrl+shift+t=new_tab",
            "global:ctrl+alt+shift+f12=toggle_quick_terminal",
        ]);

        let map = build_shortcut_map(&config);

        assert_eq!(
            map,
            vec![
                ("LOGO+q".to_owned(), Action::NewWindow),
                ("SHIFT+CTRL+ALT+F12".to_owned(), Action::ToggleQuickTerminal),
            ],
            "non-global binds are skipped; global binds keep insertion order"
        );
    }

    /// PORTS: `global_shortcuts.zig:179-181` — multi-action (chained) leaves are
    /// skipped for now ("We only allow global keybinds that map to exactly a
    /// single action"). `chain=<action>` appends to the immediately-preceding
    /// binding, turning its leaf into a `LeafChained` with two actions.
    #[test]
    fn global_shortcuts_map_skips_chained_actions() {
        let config = config_with(&["global:super+q=new_window", "chain=new_tab"]);
        let map = build_shortcut_map(&config);
        assert!(
            map.is_empty(),
            "a chained (2-action) global leaf is skipped, got {map:?}"
        );
    }

    /// A default config has no `global:` binds at all, so the map is empty and
    /// (per `refresh`) no portal session is ever created.
    #[test]
    fn global_shortcuts_map_empty_by_default() {
        let config = rustty_config::Config::default_config();
        assert!(build_shortcut_map(&config).is_empty());
    }

    /// PORTS: `global_shortcuts.zig:259-263` — the shortcut `description` is the
    /// action rendered in its parse/format spelling.
    #[test]
    fn global_shortcuts_action_description_is_format_spelling() {
        assert_eq!(action_description(&Action::NewWindow), "new_window");
        assert_eq!(
            action_description(&Action::ToggleQuickTerminal),
            "toggle_quick_terminal"
        );
    }

    /// PORTS: `global_shortcuts.zig:625-633` `generateToken`. R2: the reference's
    /// `ghostty_` prefix becomes `rustty_`; the value is a 28-bit random hex,
    /// right-padded with `0` to 7 columns (Zig `{x:0<7}` == Rust `{:0<7x}`).
    #[test]
    fn global_shortcuts_token_shape() {
        for _ in 0..64 {
            let t = generate_token();
            assert_eq!(t.len(), "rustty_".len() + 7, "token={t}");
            assert!(t.starts_with("rustty_"), "token={t}");
            assert!(
                t["rustty_".len()..].bytes().all(|b| b.is_ascii_hexdigit()),
                "token tail must be hex: {t}"
            );
        }
        assert_ne!(
            generate_token(),
            generate_token(),
            "tokens must not be constant"
        );
    }

    /// PORTS: `global_shortcuts.zig:468-497` `getRequestPath`. The unique name
    /// loses its leading `:` and every `.` becomes `_`, e.g. `:1.192` → `1_192`.
    #[test]
    fn global_shortcuts_request_object_path() {
        assert_eq!(
            request_object_path(":1.192", "rustty_abc1230"),
            "/org/freedesktop/portal/desktop/request/1_192/rustty_abc1230"
        );
        assert_eq!(
            request_object_path(":1.2.3", "rustty_0000000"),
            "/org/freedesktop/portal/desktop/request/1_2_3/rustty_0000000"
        );
    }

    /// The generated path must be a valid D-Bus object path (the portal will
    /// reject a malformed one, and `signal_subscribe` would silently never fire).
    #[test]
    fn global_shortcuts_request_object_path_is_valid() {
        let path = request_object_path(":1.192", &generate_token());
        assert!(
            gtk4::glib::Variant::is_object_path(&path),
            "not a valid object path: {path}"
        );
    }

    /// PORTS: `global_shortcuts.zig:234-241` — the `CreateSession` payload is a
    /// 1-tuple holding the options vardict with both tokens.
    #[test]
    fn global_shortcuts_create_session_payload() {
        let v = create_session_payload("rustty_1111111", "rustty_2222222");
        assert_eq!(v.type_().as_str(), "(a{sv})");

        let dict = gtk4::glib::VariantDict::new(Some(&v.child_value(0)));
        assert_eq!(
            dict.lookup::<String>("handle_token").unwrap().as_deref(),
            Some("rustty_1111111")
        );
        assert_eq!(
            dict.lookup::<String>("session_handle_token")
                .unwrap()
                .as_deref(),
            Some("rustty_2222222")
        );
    }

    /// PORTS: `global_shortcuts.zig:244-279` — the `BindShortcuts` payload is
    /// `(session_handle, [(id, {description, preferred_trigger})], parent_window,
    /// {handle_token})`.
    #[test]
    fn global_shortcuts_bind_shortcuts_payload() {
        let map = vec![
            ("LOGO+q".to_owned(), Action::NewWindow),
            ("SHIFT+CTRL+ALT+F12".to_owned(), Action::ToggleQuickTerminal),
        ];
        let v = bind_shortcuts_payload(
            "/org/freedesktop/portal/desktop/session/1_192/rustty_2222222",
            &map,
            "rustty_1111111",
        )
        .expect("valid session handle builds a payload");

        assert_eq!(v.type_().as_str(), "(oa(sa{sv})sa{sv})");
        assert_eq!(
            v.child_value(0).str(),
            Some("/org/freedesktop/portal/desktop/session/1_192/rustty_2222222")
        );

        let shortcuts = v.child_value(1);
        assert_eq!(shortcuts.n_children(), 2);

        let first = shortcuts.child_value(0);
        assert_eq!(first.child_value(0).str(), Some("LOGO+q"));
        let opts = gtk4::glib::VariantDict::new(Some(&first.child_value(1)));
        assert_eq!(
            opts.lookup::<String>("description").unwrap().as_deref(),
            Some("new_window")
        );
        assert_eq!(
            opts.lookup::<String>("preferred_trigger")
                .unwrap()
                .as_deref(),
            Some("LOGO+q")
        );

        // parent_window is always empty (`:274` `''`).
        assert_eq!(v.child_value(2).str(), Some(""));

        let tail = gtk4::glib::VariantDict::new(Some(&v.child_value(3)));
        assert_eq!(
            tail.lookup::<String>("handle_token").unwrap().as_deref(),
            Some("rustty_1111111")
        );
    }

    /// A malformed session handle can never produce a `(o…)` payload — the
    /// reference would have already asserted; here we return `None` and skip.
    #[test]
    fn global_shortcuts_bind_shortcuts_payload_rejects_bad_handle() {
        assert!(bind_shortcuts_payload("not a path", &[], "rustty_1111111").is_none());
    }

    /// S51-F7: the `Response` body is read the way the reference reads it. A
    /// well-formed `(ua{sv})` yields its code and dict; the malformed shapes that
    /// used to PANIC in `child_value` (`()`, `(u)`) and the wrong-typed one that
    /// used to read as 2 (`(sa{sv})`) all yield `(0, None)`, which is
    /// `g_variant_get`'s untouched out-parameters.
    #[test]
    fn global_shortcuts_response_params_follow_g_variant_get() {
        use gtk4::glib::prelude::ToVariant;

        let dict = gtk4::glib::VariantDict::new(None);
        dict.insert_value("session_handle", &"/session/a".to_variant());
        let good = gtk4::glib::Variant::tuple_from_iter([1u32.to_variant(), dict.end()]);
        let (code, vardict) = response_params(&good);
        assert_eq!(code, 1);
        assert_eq!(
            session_handle_from_results(&vardict.expect("well-formed body has a vardict"))
                .as_deref(),
            Some("/session/a")
        );

        let empty = gtk4::glib::Variant::tuple_from_iter(std::iter::empty::<gtk4::glib::Variant>());
        let code_only = gtk4::glib::Variant::tuple_from_iter([2u32.to_variant()]);
        let wrong_first = gtk4::glib::Variant::tuple_from_iter([
            "x".to_variant(),
            gtk4::glib::VariantDict::new(None).end(),
        ]);
        for (what, body) in [("()", empty), ("(u)", code_only), ("(sa{sv})", wrong_first)] {
            let (code, vardict) = response_params(&body);
            assert_eq!(code, 0, "{what}: response keeps its initial 0");
            assert!(vardict.is_none(), "{what}: vardict stays null");
        }
    }

    /// The `Activated` sibling of the `Response` defect (`UPSTREAM-DEFECTS.md` UD2): a body
    /// without a string second child yields no shortcut id instead of panicking. `(os)` is
    /// the well-formed positive half; `(o)`, `()` and an `(oo)` whose id is an object path
    /// all yield `None` (`&s` asks for type `s` exactly).
    #[test]
    fn global_shortcuts_activated_shortcut_id_is_safe_on_malformed_bodies() {
        use gtk4::glib::prelude::ToVariant;
        use gtk4::glib::variant::ObjectPath;

        let handle = ObjectPath::try_from("/session/a").unwrap().to_variant();
        let good = gtk4::glib::Variant::tuple_from_iter([handle.clone(), "LOGO+q".to_variant()]);
        assert_eq!(activated_shortcut_id(&good).as_deref(), Some("LOGO+q"));

        let short = gtk4::glib::Variant::tuple_from_iter([handle.clone()]);
        let empty = gtk4::glib::Variant::tuple_from_iter(std::iter::empty::<gtk4::glib::Variant>());
        let as_path = gtk4::glib::Variant::tuple_from_iter([handle.clone(), handle]);
        for body in [short, empty, as_path] {
            assert_eq!(activated_shortcut_id(&body), None, "{}", body.print(true));
        }
    }

    /// S51-F7 and the reference defect it met (`UPSTREAM-DEFECTS.md` UD1): a code-0
    /// `Response` whose body is not `(ua{sv})` must not panic. The reference unwraps the
    /// null vardict (`vardict.?`); the port handles it as a failed response with the
    /// reference's own "request ended unexpectedly" warning and never calls `onResponse`.
    /// The well-formed body first is the positive half: it reaches `onResponse` (its
    /// `session_handle=` debug line) through the same method.
    #[test]
    fn global_shortcuts_malformed_response_does_not_panic() {
        use gtk4::glib::prelude::ToVariant;

        let cap = crate::log_capture::start();
        let gs = GlobalShortcuts::new();

        let dict = gtk4::glib::VariantDict::new(None);
        dict.insert_value("session_handle", &"/session/a".to_variant());
        let good = gtk4::glib::Variant::tuple_from_iter([0u32.to_variant(), dict.end()]);
        gs.responded(Method::CreateSession, &good);

        let empty = gtk4::glib::Variant::tuple_from_iter(std::iter::empty::<gtk4::glib::Variant>());
        gs.responded(Method::CreateSession, &empty);

        let messages = cap.messages();
        drop(cap);
        let rec = |level, msg: &str| (level, LOG_SCOPE.to_string(), msg.to_string());
        assert_eq!(
            messages,
            vec![
                rec(log::Level::Debug, "request successful"),
                rec(log::Level::Debug, "session_handle=/session/a"),
                rec(log::Level::Warn, "request ended unexpectedly"),
            ]
        );
    }

    /// PORTS: `global_shortcuts.zig:333-338` — pull `session_handle` out of the
    /// `Response` results vardict with `&s`.
    ///
    /// CORRECTION (task 20, S51-F6): this test used to assert that an `o`-typed
    /// handle is ACCEPTED ("Portals in the wild type this field as either `s` or
    /// `o`; both must be accepted"). The reference's `&s` lookup rejects it, so the
    /// assertion is inverted.
    #[test]
    fn global_shortcuts_session_handle_from_results() {
        use gtk4::glib::prelude::ToVariant;

        let as_string = gtk4::glib::VariantDict::new(None);
        as_string.insert_value("session_handle", &"/session/a".to_variant());
        assert_eq!(
            session_handle_from_results(&as_string.end()).as_deref(),
            Some("/session/a")
        );

        let as_path = gtk4::glib::VariantDict::new(None);
        as_path.insert_value(
            "session_handle",
            &gtk4::glib::variant::ObjectPath::try_from("/session/b")
                .unwrap()
                .to_variant(),
        );
        assert_eq!(session_handle_from_results(&as_path.end()).as_deref(), None);

        let empty = gtk4::glib::VariantDict::new(None);
        assert_eq!(session_handle_from_results(&empty.end()), None);
    }

    /// `refresh` rebuilds the shortcut map from the bound config. With no D-Bus
    /// connection there is no portal session to create, so the map is built and
    /// the round-trip is simply not attempted (this is the headless path).
    #[test]
    fn global_shortcuts_refresh_builds_map_without_dbus() {
        let core = config_with(&["global:super+q=new_window"]);
        let cfg = Config::new(&core);
        let gs = GlobalShortcuts::new();
        gs.set_config(Some(&cfg));

        assert_eq!(
            gs.shortcut_map(),
            vec![("LOGO+q".to_owned(), Action::NewWindow)],
            "notify::config must drive refresh and populate the map"
        );
    }

    /// `close` (which `refresh` always runs first, and `dispose` runs) clears the
    /// portal session state. Re-setting a config with no global binds empties it.
    #[test]
    fn global_shortcuts_refresh_clears_prior_map() {
        let with_global = Config::new(&config_with(&["global:super+q=new_window"]));
        let without_global = Config::new(&config_with(&["ctrl+shift+t=new_tab"]));

        let gs = GlobalShortcuts::new();
        gs.set_config(Some(&with_global));
        assert_eq!(gs.shortcut_map().len(), 1);

        gs.set_config(Some(&without_global));
        assert!(
            gs.shortcut_map().is_empty(),
            "a config with no global binds leaves an empty map"
        );
    }

    /// PORTS: `global_shortcuts.zig:541-565` `shortcutActivated` — the portal's
    /// shortcut id is looked up in the map and the matching action is re-emitted
    /// on the `trigger` signal. This is the whole point of the class.
    #[test]
    fn global_shortcuts_activation_emits_trigger() {
        use std::cell::RefCell as StdRefCell;
        use std::rc::Rc;

        let cfg = Config::new(&config_with(&["global:super+q=new_window"]));
        let gs = GlobalShortcuts::new();
        gs.set_config(Some(&cfg));

        let seen: Rc<StdRefCell<Vec<Action>>> = Rc::new(StdRefCell::new(Vec::new()));
        let seen_cb = seen.clone();
        gs.connect_local("trigger", false, move |args| {
            let boxed: ActionBoxed = args[1].get().expect("trigger carries an ActionBoxed");
            seen_cb.borrow_mut().push(boxed.0);
            None
        });

        gs.dispatch_activation("LOGO+q");
        gs.dispatch_activation("LOGO+never-bound");

        assert_eq!(
            *seen.borrow(),
            vec![Action::NewWindow],
            "only a mapped shortcut id emits trigger"
        );
    }

    /// PORTS: `global_shortcuts.zig:117-126,618-626` — `emitBindFailed` puts the
    /// failure on the `bind-failed` signal, which is the ONLY channel a denied or
    /// revoked winproto binding has. Written in the style of
    /// `global_shortcuts_activation_emits_trigger`: the assertion is that the
    /// signal actually FIRED with the payload intact, not that nothing errored —
    /// a keybind that silently no-ops is the failure mode this whole series exists
    /// to prevent.
    #[test]
    fn global_shortcuts_emit_bind_failed_reaches_the_signal() {
        use std::cell::RefCell as StdRefCell;
        use std::rc::Rc;

        let gs = GlobalShortcuts::new();

        let seen: Rc<StdRefCell<Vec<BindFailed>>> = Rc::new(StdRefCell::new(Vec::new()));
        let seen_cb = seen.clone();
        gs.connect_local("bind-failed", false, move |args| {
            let boxed: BindFailed = args[1].get().expect("bind-failed carries a BindFailed");
            seen_cb.borrow_mut().push(boxed);
            None
        });

        let trigger = Trigger {
            key: rustty_input::binding::TriggerKey::Unicode('q' as u32),
            mods: rustty_vt::input::key_mods::Mods {
                super_: true,
                ..Default::default()
            },
        };
        let failure = BindFailed {
            trigger,
            action: Action::NewWindow,
            message: b"already bound".to_vec(),
            revoked: true,
        };
        gs.emit_bind_failed(&failure);

        assert_eq!(
            *seen.borrow(),
            vec![failure],
            "emit_bind_failed must deliver the whole failure (trigger, action, \
             message bytes and the revoked flag) to the signal"
        );
    }
}
