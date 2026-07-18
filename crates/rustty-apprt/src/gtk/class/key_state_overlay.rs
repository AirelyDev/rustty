//! rustty-apprt::gtk::class::key_state_overlay
//!
//! PORTS: ghostty `src/apprt/gtk/class/key_state_overlay.zig`
//! (`GhosttyKeyStateOverlay`).
//!
//! **INHABITED placeholder (L5.2 T13 → filled by L5.3 T24).** `Surface`'s
//! template instantiates `$GhosttyKeyStateOverlay key_state_overlay { … }` and
//! **binds two properties** — `tables: bind template.key-table` and
//! `sequence: bind template.key-sequence` (`surface.blp:200-201`). For those
//! bindings to resolve at template-load, this class must register the `tables`
//! and `sequence` properties (both `?*ext.StringList`,
//! `key_state_overlay.zig:30-87`) now; the actual overlay rendering lands with
//! T24. The placeholder is an `adw::Bin` subclass with the reference GType name
//! and those two nullable boxed-`StringList` properties.

use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::gtk::ext::StringList;

mod imp {
    use super::*;
    use std::cell::RefCell;

    /// PORTS: `key_state_overlay.zig:19` `pub const KeyStateOverlay`. Placeholder
    /// instance struct carrying the two `StringList` fields the surface template
    /// binds; the reference `Private` (`:120-...`) lands with T24.
    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::KeyStateOverlay)]
    pub struct KeyStateOverlay {
        /// PORTS: `key_state_overlay.zig:30-49,121` property/field `tables`
        /// (`?*ext.StringList`, OWNED boxed). Get transfer `.none`, set transfer
        /// `.full` (`:41-44`) — the gtk4-rs `RefCell<Option<Boxed>>` get clones,
        /// set replaces/drops-prior, matching those transfer modes.
        #[property(get, set, nullable)]
        tables: RefCell<Option<StringList>>,

        /// PORTS: `key_state_overlay.zig:68-87` property `sequence`
        /// (`?*ext.StringList`, OWNED boxed).
        #[property(get, set, nullable)]
        sequence: RefCell<Option<StringList>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KeyStateOverlay {
        // PORTS: `key_state_overlay.zig:22` `.name = "GhosttyKeyStateOverlay"`.
        const NAME: &'static str = "GhosttyKeyStateOverlay";
        type Type = super::KeyStateOverlay;
        // PORTS: `key_state_overlay.zig:20` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;
    }

    #[glib::derived_properties]
    impl ObjectImpl for KeyStateOverlay {
        // PORTS the placeholder's default-hidden state from the reference
        // `key-state-overlay.blp:5` (`visible: bind $has_state(...)`, which defaults to
        // no-state/false). Without it, a bare `adw::Bin` overlay child defaults to
        // visible + FILL + can-target, so GtkOverlay allocates it the FULL terminal
        // rectangle and it swallows every pointer/keyboard event before the GLArea's
        // controllers see it. T24 replaces this with the real key-state template.
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_visible(false);
        }
    }
    impl WidgetImpl for KeyStateOverlay {}
    impl BinImpl for KeyStateOverlay {}
}

glib::wrapper! {
    /// PORTS: `key_state_overlay.zig:19-...` `pub const KeyStateOverlay`. See the
    /// module doc — a registered-GType placeholder (the `tables`/`sequence`
    /// properties) until L5.3 T24.
    pub struct KeyStateOverlay(ObjectSubclass<imp::KeyStateOverlay>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for KeyStateOverlay {
    fn default() -> Self {
        glib::Object::new()
    }
}
