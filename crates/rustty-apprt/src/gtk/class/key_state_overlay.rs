//! rustty-apprt::gtk::class::key_state_overlay
//!
//! PORTS: ghostty `src/apprt/gtk/class/key_state_overlay.zig`
//! (`GhosttyKeyStateOverlay`).
//!
//! An overlay that displays the current key table stack and pending key sequence
//! (`key_state_overlay.zig:14-17`). `Surface`'s template instantiates
//! `$GhosttyKeyStateOverlay key_state_overlay { … }` and binds two properties —
//! `tables: bind template.key-table` and `sequence: bind template.key-sequence`
//! (`surface.blp:199-202`). This is a `CompositeTemplate` over
//! `ui/1.2/key-state-overlay.ui` (`key-state-overlay.blp`): the horizontal box of
//! keyboard icon + `tables_label` + chevron + `sequence_label` + pending spinner,
//! whose visibility/text is driven by the bind-closures below.
//!
//! Mirrors the `CompositeTemplate` + `#[gtk4::template_callbacks]` idiom of
//! `search_overlay.rs` (bind-closures take `&self` first; the `drag-end` signal
//! callback is NON-swapped, so `this` comes LAST).

use std::cell::{Cell, RefCell};

use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::subclass::prelude::*;

use crate::gtk::ext::StringList;

mod imp {
    use super::*;

    /// PORTS: `key_state_overlay.zig:17` `pub const KeyStateOverlay` + its
    /// `Private` (`:120-131`). A `CompositeTemplate` over
    /// `ui/1.2/key-state-overlay.ui`.
    #[derive(CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::KeyStateOverlay)]
    #[template(resource = "/io/airely/rustty/ui/1.2/key-state-overlay.ui")]
    pub struct KeyStateOverlay {
        /// PORTS: `key_state_overlay.zig:30-49,122` property/field `tables`
        /// (`?*ext.StringList`, OWNED boxed). Get transfer `.none`, set transfer
        /// `.full` (`:41-44`) — the gtk4-rs `RefCell<Option<Boxed>>` get clones,
        /// set replaces/drops-prior, matching those transfer modes. The custom
        /// setter also notifies the computed `has-tables` (`:156`).
        #[property(get, set = Self::set_tables, nullable)]
        pub tables: RefCell<Option<StringList>>,

        /// PORTS: `key_state_overlay.zig:51-66` read-only computed property
        /// `has-tables` (getter `getHasTables`, `:173-176`): true iff `tables` is
        /// non-null and non-empty.
        #[property(name = "has-tables", get = Self::get_has_tables)]
        pub has_tables_prop: std::marker::PhantomData<bool>,

        /// PORTS: `key_state_overlay.zig:68-87,125` property `sequence`
        /// (`?*ext.StringList`, OWNED boxed). Custom setter notifies
        /// `has-sequence` (`:170`).
        #[property(get, set = Self::set_sequence, nullable)]
        pub sequence: RefCell<Option<StringList>>,

        /// PORTS: `key_state_overlay.zig:89-104` read-only computed property
        /// `has-sequence` (getter `getHasSequence`, `:178-181`).
        #[property(name = "has-sequence", get = Self::get_has_sequence)]
        pub has_sequence_prop: std::marker::PhantomData<bool>,

        /// Target vertical alignment for the overlay.
        ///
        /// PORTS: `key_state_overlay.zig:106-117,128` property/field
        /// `valign-target` (default `.end`) — the target vertical alignment; the
        /// template binds `valign: bind template.valign-target`
        /// (`key-state-overlay.blp:8`) and `on_drag_end` flips it (`:238-263`).
        #[property(get, set, name = "valign-target", builder(gtk4::Align::End))]
        pub valign_target: Cell<gtk4::Align>,
    }

    impl Default for KeyStateOverlay {
        /// gtk4-rs needs a `Default` for `object_subclass`; `gtk.Align` has no
        /// `Default` so it cannot be derived. The `valign-target` default mirrors
        /// the reference pspec default (`key_state_overlay.zig:112`); the template
        /// (`key-state-overlay.blp:6`) sets it at load anyway.
        fn default() -> Self {
            Self {
                tables: RefCell::new(None),
                has_tables_prop: std::marker::PhantomData,
                sequence: RefCell::new(None),
                has_sequence_prop: std::marker::PhantomData,
                valign_target: Cell::new(gtk4::Align::End),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KeyStateOverlay {
        // PORTS: `key_state_overlay.zig:22` `.name = "GhosttyKeyStateOverlay"`.
        // Preserved 1:1 so the surface template's `$GhosttyKeyStateOverlay` resolves.
        const NAME: &'static str = "GhosttyKeyStateOverlay";
        type Type = super::KeyStateOverlay;
        // PORTS: `key_state_overlay.zig:20` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;

        /// PORTS: `key_state_overlay.zig:307-336` `Class.init` — load the template
        /// and bind the five template callbacks.
        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        /// PORTS: `key_state_overlay.zig:133-135` `init` — `gtk.Widget.initTemplate`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for KeyStateOverlay {
        /// PORTS: `key_state_overlay.zig:268-278` `dispose` — dispose the template.
        /// The box children are deeply nested (not the Bin's root child), so no
        /// double-free workaround is needed (cf. `resize_overlay.rs`). The OWNED
        /// `tables`/`sequence` (reference `finalize`, `:280-294`) drop with the
        /// `RefCell`s.
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for KeyStateOverlay {}
    impl BinImpl for KeyStateOverlay {}

    impl KeyStateOverlay {
        /// PORTS: `key_state_overlay.zig:145-157` `setTables`. Replace the stored
        /// list (dropping the prior, the reference's `old.destroy()`), then notify
        /// `tables` and `has-tables`, in that order.
        ///
        /// CORRECTION (task 20, S52-F3): this notified only `has-tables`, relying
        /// on GObject's automatic post-`set_property` notify for `tables` ("auto-
        /// fires via the property machinery"). The reference notifies both
        /// explicitly (`:155-156`); inside `g_object_set_property`'s freeze the
        /// automatic `tables` notify then de-duplicates against the queued one,
        /// exactly as it does for the reference's own accessor (which, like this
        /// pspec, is not `EXPLICIT_NOTIFY`). Measured on GLib 2.88.2, whose frozen
        /// queue dispatches in REVERSE: through `g_object_set_property` the OLD
        /// setter delivered `tables, has-tables` and this one (the reference's
        /// calls) delivers `has-tables, tables`. So the old port and the reference
        /// disagreed, but not in the direction the audit reported.
        ///
        /// CORRECTION (task 20 fix round 1): this note first said the automatic
        /// notify "is queued AFTER the explicit one, so handlers saw `has-tables`
        /// first" — backwards for this GLib, and contradicting the test's own doc.
        pub(super) fn set_tables(&self, value: Option<StringList>) {
            self.tables.replace(value);
            self.obj().notify("tables");
            self.obj().notify("has-tables");
        }

        /// PORTS: `key_state_overlay.zig:159-171` `setSequence`. Same ordering as
        /// [`Self::set_tables`] (`:169-170`); same CORRECTION.
        pub(super) fn set_sequence(&self, value: Option<StringList>) {
            self.sequence.replace(value);
            self.obj().notify("sequence");
            self.obj().notify("has-sequence");
        }

        /// PORTS: `key_state_overlay.zig:173-176` `getHasTables`.
        fn get_has_tables(&self) -> bool {
            self.tables.borrow().as_ref().is_some_and(|v| !v.is_empty())
        }

        /// PORTS: `key_state_overlay.zig:178-181` `getHasSequence`.
        fn get_has_sequence(&self) -> bool {
            self.sequence
                .borrow()
                .as_ref()
                .is_some_and(|v| !v.is_empty())
        }
    }
}

glib::wrapper! {
    /// An overlay that displays the current key table stack and pending key sequence.
    /// This helps users understand what key bindings are active and what keys they've
    /// pressed in a multi-key sequence.
    ///
    /// PORTS: `key_state_overlay.zig:17` `pub const KeyStateOverlay`. The overlay
    /// showing the pending key-table stack + key sequence for leader-key feedback.
    pub struct KeyStateOverlay(ObjectSubclass<imp::KeyStateOverlay>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for KeyStateOverlay {
    fn default() -> Self {
        glib::Object::new()
    }
}

#[gtk4::template_callbacks]
impl KeyStateOverlay {
    // -- bind-closures (key-state-overlay.blp) -----------------------------
    // PORT NOTE (arg order): these are GtkExpression bind-closures, so gtk4-rs
    // passes the template instance as the implicit FIRST argument (`&self`),
    // then the bound arguments — matching the reference's `closure*(_: *Self,
    // ...)` signatures (`key_state_overlay.zig:183-233`).

    /// PORTS: `key_state_overlay.zig:191-197` `closureHasState`. The overlay is
    /// visible when there is any key state (`key-state-overlay.blp:5`).
    #[template_callback]
    fn has_state(&self, has_tables: bool, has_sequence: bool) -> bool {
        has_tables || has_sequence
    }

    /// PORTS: `key_state_overlay.zig:183-189` `closureShowChevron`. The chevron
    /// separator shows only when both a table AND a sequence are present
    /// (`key-state-overlay.blp:38`).
    #[template_callback]
    fn show_chevron(&self, has_tables: bool, has_sequence: bool) -> bool {
        has_tables && has_sequence
    }

    /// PORTS: `key_state_overlay.zig:199-215` `closureTablesText`. Joins the key
    /// table stack with `" > "` (`key-state-overlay.blp:33`); `None` when empty.
    #[template_callback]
    fn tables_text(&self, tables: Option<StringList>) -> Option<String> {
        let list = tables?;
        if list.is_empty() {
            return None;
        }
        Some(list.strings().join(" > "))
    }

    /// PORTS: `key_state_overlay.zig:217-233` `closureSequenceText`. Joins the
    /// pending key sequence with `" "` (`key-state-overlay.blp:48`); `None` when
    /// empty.
    #[template_callback]
    fn sequence_text(&self, sequence: Option<StringList>) -> Option<String> {
        let list = sequence?;
        if list.is_empty() {
            return None;
        }
        Some(list.strings().join(" "))
    }

    // -- drag gesture (key-state-overlay.blp:10-14) ------------------------
    /// PORTS: `key_state_overlay.zig:238-263` `onDragEnd`. The key-state overlay
    /// only moves between top-center and bottom-center (horizontal alignment is
    /// always center), so it flips `valign-target` to whichever half the overlay's
    /// center crossed. NON-swapped signal (`key-state-overlay.blp:13`
    /// `drag-end => $on_drag_end()`): the emitting `GestureDrag` is arg 0, `this:
    /// &KeyStateOverlay` is LAST (a wrong `&self` shape aborts at runtime).
    #[template_callback]
    fn on_drag_end(
        _gesture: &gtk4::GestureDrag,
        _offset_x: f64,
        offset_y: f64,
        this: &KeyStateOverlay,
    ) {
        // Key state overlay only moves between top-center and bottom-center.
        // Horizontal alignment is always center.
        let widget = this.upcast_ref::<gtk4::Widget>();
        let Some(parent) = widget.parent() else {
            return;
        };

        let parent_height = f64::from(parent.height());
        let self_height = f64::from(widget.height());

        let self_y = if this.valign_target() == gtk4::Align::Start {
            0.0
        } else {
            parent_height - self_height
        };
        let new_y = self_y + offset_y + (self_height / 2.0);

        let new_valign = if new_y > parent_height / 2.0 {
            gtk4::Align::End
        } else {
            gtk4::Align::Start
        };

        if new_valign != this.valign_target() {
            this.set_valign_target(new_valign);
            widget.queue_resize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S52-F3: `setTables` / `setSequence` notify the property first and its
    /// computed `has-*` companion second (`key_state_overlay.zig:155-156,169-170`).
    ///
    /// The setter is called directly, outside `g_object_set_property`, because that
    /// is the only place the CALL order is observable as-is. Inside
    /// `g_object_set_property` (how the reference's only caller, the
    /// `tables: bind template.key-table` binding, reaches it) GLib queues
    /// notifications during a freeze and dispatches the queue in REVERSE (measured on
    /// GLib 2.88.2: the fixed setter dispatches `has-tables` then `tables`, the old
    /// one `tables` then `has-tables`), so an assertion there would pin GLib's queue
    /// rather than the port. Every notify of the four properties is recorded, so a
    /// missing, extra or reordered emission fails, and the non-empty record is the
    /// proof the setter ran.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib key_state_overlay -- --ignored"]
    fn key_state_overlay_setters_notify_value_before_has() {
        crate::gtk::init_for_display_test("key_state_overlay_setters_notify_value_before_has");
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        let overlay = KeyStateOverlay::default();
        let seen = std::rc::Rc::new(RefCell::new(Vec::<String>::new()));
        let sink = seen.clone();
        // Only the four properties under test: the template binds the overlay's
        // own `visible` to `has-*`, so a change there is expected and not ours.
        overlay.connect_notify_local(None, move |_, pspec| {
            let name = pspec.name();
            if ["tables", "has-tables", "sequence", "has-sequence"].contains(&name) {
                sink.borrow_mut().push(name.to_string());
            }
        });

        overlay.imp().set_tables(Some(StringList::new(&["a"])));
        assert_eq!(*seen.borrow(), ["tables", "has-tables"]);
        seen.borrow_mut().clear();

        overlay.imp().set_sequence(Some(StringList::new(&["b"])));
        assert_eq!(*seen.borrow(), ["sequence", "has-sequence"]);
    }
}
