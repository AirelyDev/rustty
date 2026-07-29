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

use rustty_input::binding::{Action, Value};

use crate::gtk::key;

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

        /// PORTS: `global_shortcuts.zig:64-68` `map`. A mapping from the portal
        /// shortcut id to the action it performs. The id is the serialized XDG
        /// spelling of the trigger, which is unique in the keymap. Insertion
        /// order is preserved (the reference uses a `StringArrayHashMapUnmanaged`),
        /// because that is the order the shortcuts are presented to the portal.
        ///
        /// PORT NOTE: the reference allocates the keys in a per-refresh arena
        /// (`:61-62`) and drops the whole arena in `close`; Rust owns the strings
        /// in the `Vec`, so `close` just clears it and the arena field has no
        /// counterpart.
        pub map: RefCell<Vec<(String, rustty_input::binding::Action)>>,

        /// PORTS: `global_shortcuts.zig:70-72` `handle` — the current portal
        /// session's D-Bus object path, or `None` when no session exists.
        pub handle: RefCell<Option<String>>,

        /// PORTS: `global_shortcuts.zig:74-76` `response_subscription` — the
        /// subscription to the in-flight request's `Response` signal. The
        /// reference uses `0` as the null id; gio's `SignalSubscriptionId` is a
        /// non-zero newtype, so `Option` is the faithful spelling.
        pub response_subscription: RefCell<Option<gio::SignalSubscriptionId>>,

        /// PORTS: `global_shortcuts.zig:78-80` `activate_subscription` — the
        /// subscription to the session's `Activated` signal.
        pub activate_subscription: RefCell<Option<gio::SignalSubscriptionId>>,
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

    /// PORTS: `global_shortcuts.zig:148-213` `refresh`. Re-derive the global
    /// shortcut registration from the current config: close the previous session,
    /// rebuild the trigger→action map, and — if anything is registered — start the
    /// portal round-trip with `CreateSession`.
    fn refresh(&self) {
        // Always close our previous state first (`:150`).
        self.close();

        let imp = self.imp();

        // We need configuration to proceed (`:155`).
        let map = {
            let config = imp.config.borrow();
            let Some(config) = config.as_ref() else {
                return;
            };
            build_shortcut_map(config.get())
        };

        let count = map.len();
        imp.map.replace(map);

        // Create our session if we have global shortcuts (`:212`).
        if count > 0 {
            self.request(Method::CreateSession);
        }
    }

    /// PORTS: `global_shortcuts.zig:109-146` `close`. Tear down the current portal
    /// session: drop both signal subscriptions, ask the portal to close the
    /// session, and clear the map.
    fn close(&self) {
        let imp = self.imp();

        // `const dbus = priv.dbus_connection orelse return;` (`:111`)
        let dbus = { imp.dbus_connection.borrow().clone() };
        let Some(dbus) = dbus else {
            // Without a connection there are no subscriptions and no session, but
            // the map may still hold state from a headless refresh.
            imp.map.replace(Vec::new());
            imp.handle.replace(None);
            return;
        };

        // `:113-121`
        if let Some(id) = imp.response_subscription.borrow_mut().take() {
            dbus.signal_unsubscribe(id);
        }
        if let Some(id) = imp.activate_subscription.borrow_mut().take() {
            dbus.signal_unsubscribe(id);
        }

        // Close the existing session (`:123-139`).
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

        self.emit_by_name::<()>("trigger", &[&ActionBoxed(action)]);
    }

    /// The current portal shortcut map (id → action), in registration order.
    /// rustty-only accessor: the reference reads `priv.map` directly in-file.
    pub(crate) fn shortcut_map(&self) -> Vec<(String, Action)> {
        self.imp().map.borrow().clone()
    }

    /// PORTS: `global_shortcuts.zig:329-460` `request`. Filled in WS15 T6.
    fn request(&self, _method: Method) {}
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

/// PORTS: `global_shortcuts.zig:625-633` `generateToken`. A random token suitable
/// for a portal `handle_token` / `session_handle_token`.
///
/// R2: the reference prefixes `ghostty_`; rustty prefixes `rustty_`. The reference
/// draws a `u28` and formats `{x:0<7}` (hex, fill `0`, LEFT-aligned to width 7 —
/// i.e. zeros are appended, not prepended); Rust's `{:0<7x}` is the same spec.
pub(crate) fn generate_token() -> String {
    let mut buf = [0u8; 4];
    rustty_os::file::crypto_random_bytes(&mut buf);
    // `std.crypto.random.int(u28)`
    let v = u32::from_ne_bytes(buf) & 0x0fff_ffff;
    format!("rustty_{v:0<7x}")
}

/// PORTS: `global_shortcuts.zig:468-497` `getRequestPath`.
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
    let sender = unique_name
        .strip_prefix(':')
        .unwrap_or(unique_name)
        .replace('.', "_");
    format!("{PORTAL_OBJECT_PATH}/request/{sender}/{token}")
}

/// PORTS: `global_shortcuts.zig:234-241` — the `CreateSession` payload.
///
/// D-Bus signature `(a{sv})`: one options vardict carrying the request's
/// `handle_token` and the session's `session_handle_token`.
/// See <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html>
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

/// PORTS: `global_shortcuts.zig:286-293` — read `session_handle` out of a
/// `Response` results vardict.
///
/// PORT NOTE: the reference looks the key up as `&s`. Portal implementations
/// disagree on whether this field is `s` or `o`, so we accept either
/// (`glib::Variant::str` covers `s`/`o`/`g`).
pub(crate) fn session_handle_from_results(results: &glib::Variant) -> Option<String> {
    let dict = glib::VariantDict::new(Some(results));
    let value = dict.lookup_value("session_handle", None)?;
    value.str().map(str::to_owned)
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
pub(crate) fn build_shortcut_map(config: &rustty_config::Config) -> Vec<(String, Action)> {
    let mut map: Vec<(String, Action)> = Vec::new();

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

        // "We only allow global keybinds that map to exactly a single action
        // for now." (`:179-181`)
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

    /// PORTS: `global_shortcuts.zig:286-293` — pull `session_handle` out of the
    /// `Response` results vardict. Portals in the wild type this field as either
    /// `s` or `o`; both must be accepted.
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
        assert_eq!(
            session_handle_from_results(&as_path.end()).as_deref(),
            Some("/session/b")
        );

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
}
