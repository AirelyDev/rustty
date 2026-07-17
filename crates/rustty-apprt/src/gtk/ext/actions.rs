//! rustty-apprt::gtk::ext::actions
//!
//! PORTS: ghostty `src/apprt/gtk/ext/actions.zig` (198 loc, 2 tests @ `:35,:139`).
//!
//! Helpers for attaching `gio.SimpleAction`s to widgets. The oracle threads the
//! target widget as an untyped `*T` userdata pointer into each action's
//! `activate` callback (`actions.zig:44,109-115`); the group it builds is
//! inserted into the widget and OWNED by it, so the raw back-pointer is valid
//! for as long as any action can fire (Invariant 10: the group's lifetime ⊆ the
//! widget's).
//!
//! rustty cannot hold a raw `*T`; the faithful, memory-safe re-expression is to
//! capture a WEAK reference to the target in the `activate` closure and upgrade
//! it on each invocation (the standard gtk4-rs `#[weak]` idiom). This preserves
//! Invariant 10's shape — no strong cycle from the widget-owned group back to the
//! widget — and is behavior-identical inside the supported envelope: the widget
//! is always alive when one of its own actions fires, so the upgrade always
//! succeeds; if the widget is gone, the callback is a no-op instead of a
//! use-after-free.

use glib::prelude::*;

/// PORTS: `actions.zig:20` `gActionNameIsValid`. "Reimplementation of
/// `g_action_name_is_valid()` so that it can be used at comptime."
/// (`actions.zig:16-17`). In rustty this is a plain `const fn` — usable in
/// `const` context exactly as the oracle uses it at comptime.
#[must_use]
pub const fn g_action_name_is_valid(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return false;
    }

    // PORTS: `actions.zig:23-30` — the allowed set is `-`, `.`, `0-9`, `a-z`,
    // `A-Z`. `const fn` forbids `for`/iterator adapters, so this is a `while`
    // over the bytes (ASCII-only, exactly like the oracle's byte switch).
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let ok = matches!(c, b'-' | b'.' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z');
        if !ok {
            return false;
        }
        i += 1;
    }

    true
}

/// PORTS: `actions.zig:44` `Action(T).Callback`. The oracle's C callback
/// `*const fn (*gio.SimpleAction, ?*glib.Variant, *T) callconv(.c) void` becomes
/// a Rust `fn` pointer with the borrow that GObject actually provides: a
/// borrowed action, a borrowed optional parameter, and a borrowed target.
pub type Callback<T> = fn(&gio::SimpleAction, Option<&glib::Variant>, &T);

/// PORTS: `actions.zig:41` `pub fn Action(comptime T: type) type` — the struct
/// "describing an action" (`:40`). `T` is the target widget type.
pub struct Action<T> {
    /// PORTS: `actions.zig:47` `name`.
    pub name: &'static str,
    /// PORTS: `actions.zig:48` `callback`.
    pub callback: Callback<T>,
    /// PORTS: `actions.zig:49` `parameter_type: ?*const glib.VariantType`.
    pub parameter_type: Option<glib::VariantType>,
    /// PORTS: `actions.zig:50` `state: ?*glib.Variant = null`.
    pub state: Option<glib::Variant>,
}

impl<T> Action<T> {
    /// PORTS: `actions.zig:53` `init`. "initialize a new action so that we can
    /// comptime check the name." (`:52-53`). The oracle's
    /// `comptime assert(gActionNameIsValid(name))` (`:58`) becomes a
    /// `debug_assert!` — the same check, active in dev builds.
    #[must_use]
    pub fn init(
        name: &'static str,
        callback: Callback<T>,
        parameter_type: Option<glib::VariantType>,
    ) -> Self {
        debug_assert!(g_action_name_is_valid(name), "invalid action name: {name}");
        Self {
            name,
            callback,
            parameter_type,
            state: None,
        }
    }

    /// PORTS: `actions.zig:69` `initStateful`. As [`Action::init`] but carries
    /// initial `state` (`:74-81`).
    #[must_use]
    pub fn init_stateful(
        name: &'static str,
        callback: Callback<T>,
        parameter_type: Option<glib::VariantType>,
        state: glib::Variant,
    ) -> Self {
        debug_assert!(g_action_name_is_valid(name), "invalid action name: {name}");
        Self {
            name,
            callback,
            parameter_type,
            state: Some(state),
        }
    }
}

/// PORTS: `actions.zig:87` `add`. "Add actions to a widget that implements
/// `gio.ActionMap`." Delegates to [`add_to_map`] on `self` upcast to
/// `gio.ActionMap` (`:88`).
pub fn add<T>(self_obj: &T, actions: &[Action<T>])
where
    T: IsA<gio::ActionMap>,
{
    add_to_map(self_obj, self_obj.upcast_ref::<gio::ActionMap>(), actions);
}

/// PORTS: `actions.zig:92` `addToMap`. For each action: build a `SimpleAction`
/// (stateful or not, `:95-107`), wire its `activate` signal to the callback with
/// the target as userdata (`:109-115`), and register it on `map` (`:116`). The
/// oracle's `defer action.unref()` (`:108`) is the Rust temporary drop: the local
/// `SimpleAction` is unref'd at loop-iteration end, after `add_action` has taken
/// its own reference.
fn add_to_map<T>(self_obj: &T, map: &gio::ActionMap, actions: &[Action<T>])
where
    T: ObjectType,
{
    use gio::prelude::ActionMapExt;

    for entry in actions {
        debug_assert!(
            g_action_name_is_valid(entry.name),
            "invalid action name: {}",
            entry.name
        );

        let parameter_type = entry.parameter_type.as_ref().map(AsRef::as_ref);
        let action = match &entry.state {
            Some(state) => gio::SimpleAction::new_stateful(entry.name, parameter_type, state),
            None => gio::SimpleAction::new(entry.name, parameter_type),
        };

        // Invariant 10: weak back-reference to the target (see module doc).
        let weak = self_obj.downgrade();
        let callback = entry.callback;
        action.connect_activate(move |action, parameter| {
            if let Some(target) = weak.upgrade() {
                callback(action, parameter, &target);
            }
        });

        map.add_action(&action);
    }
}

/// PORTS: `actions.zig:121` `addAsGroup`. "Add actions to a widget that doesn't
/// implement ActionGroup directly." Builds a fresh `SimpleActionGroup`, fills it
/// via [`add_to_map`], inserts it into the widget under `name` (`:131-134`), and
/// returns it. The caller MUST store the returned group: `insert_action_group`
/// keeps it alive through the widget, and the actions' closures hold a weak
/// back-reference into that same widget (Invariant 10 — the group's lifetime ⊆
/// the widget's).
pub fn add_as_group<T>(self_obj: &T, name: &str, actions: &[Action<T>]) -> gio::SimpleActionGroup
where
    T: IsA<gtk4::Widget>,
{
    use gtk4::prelude::WidgetExt;

    debug_assert!(g_action_name_is_valid(name), "invalid group name: {name}");

    let group = gio::SimpleActionGroup::new();
    add_to_map(self_obj, group.upcast_ref::<gio::ActionMap>(), actions);
    self_obj.insert_action_group(name, Some(&group));
    group
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: `actions.zig:35` `test "gActionNameIsValid"`. Non-display: pure
    /// predicate, runs in the normal gate.
    #[test]
    fn g_action_name_is_valid() {
        assert!(super::g_action_name_is_valid("ring-bell"));
        assert!(!super::g_action_name_is_valid("ring_bell"));
    }

    /// PORTS: `actions.zig:139` `test "adding actions to an object"`.
    ///
    /// Display-dependent — the oracle guards with `if (gtk.initCheck() == 0)
    /// return error.SkipZigTest` (`:141`), and gtk4-rs's `gtk4::Box::new`
    /// asserts an initialized main thread. Marked `#[ignore]`; run via
    /// `xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored`.
    ///
    /// Exercises the full path: build a group with one `i32`-parameterized
    /// action whose callback writes the parameter into the box's `spacing`
    /// property, activate it, pump the main loop, and read `spacing` back.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn adding_actions_to_an_object() {
        use gtk4::prelude::*;

        if gtk4::init().is_err() {
            return;
        }

        fn callback(
            _action: &gio::SimpleAction,
            parameter: Option<&glib::Variant>,
            this: &gtk4::Box,
        ) {
            let Some(parameter) = parameter else {
                return;
            };
            let value = parameter.get::<i32>().expect("parameter is an i32 variant");
            this.set_property("spacing", value);
        }

        let boxed = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let actions = [Action::<gtk4::Box>::init(
            "test",
            callback,
            Some(glib::VariantTy::INT32.to_owned()),
        )];
        let _group = add_as_group(&boxed, "test", &actions);

        let expected: i32 = 4242;
        let parameter = expected.to_variant();
        assert!(boxed.activate_action("test.test", Some(&parameter)).is_ok());

        let context = glib::MainContext::default();
        while context.iteration(false) {}

        let actual: i32 = boxed.property("spacing");
        assert_eq!(actual, expected);
    }
}
