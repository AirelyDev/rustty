//! rustty-apprt::gtk::class::search_overlay
//!
//! PORTS: ghostty `src/apprt/gtk/class/search_overlay.zig` (`GhosttySearchOverlay`).
//!
//! **INHABITED placeholder (L5.2 T13 → filled by L5.3 T24).** `Surface`'s
//! template instantiates `$GhosttySearchOverlay search_overlay { … }`
//! (`surface.blp:191-196`) which **binds four signals** (`stop-search`,
//! `search-changed`, `next-match`, `previous-match`) and **reads the `active`
//! property** (`surface.blp:208` `search_overlay.active`). For the template to
//! load, this class must register that exact signal + property surface now; the
//! search UI itself (the `search_entry` child, match counts, `grabFocus`) lands
//! with T24. The placeholder is an `adw::Bin` subclass with the reference GType
//! name plus those four signals (`search_overlay.zig:159-204`) and the `active`
//! property (`:38-56`).

use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;
    use std::cell::Cell;
    use std::sync::OnceLock;

    /// PORTS: `search_overlay.zig:24` `pub const SearchOverlay`. Placeholder
    /// instance struct carrying only the `active` flag the surface template reads;
    /// the reference `Private` (`:207-226`) lands with T24.
    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::SearchOverlay)]
    pub struct SearchOverlay {
        /// PORTS: `search_overlay.zig:38-56,212` property/field `active`. "True
        /// when a search is active, meaning we should show the overlay."
        #[property(get, set)]
        active: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SearchOverlay {
        // PORTS: `search_overlay.zig:30` `.name = "GhosttySearchOverlay"`.
        const NAME: &'static str = "GhosttySearchOverlay";
        type Type = super::SearchOverlay;
        // PORTS: `search_overlay.zig:28` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;
    }

    #[glib::derived_properties]
    impl ObjectImpl for SearchOverlay {
        // PORTS the placeholder's default-hidden state from the reference
        // `search-overlay.blp:6` (`visible: bind template.active`, which defaults to
        // inactive/false). Without it, a bare `adw::Bin` overlay child defaults to
        // visible + FILL + can-target, so GtkOverlay allocates it the FULL terminal
        // rectangle and it swallows every pointer/keyboard event before the GLArea's
        // controllers see it. T24 replaces this with the real revealed-on-search template.
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().set_visible(false);
        }

        /// PORTS: `search_overlay.zig:157-204` `signals`. The four the surface
        /// template connects (`surface.blp:192-195`).
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // `stop-search` (`:159-168`), no params.
                    glib::subclass::Signal::builder("stop-search").build(),
                    // `search-changed` (`:171-180`), one nullable-string param
                    // (`&.{?[*:0]const u8}`).
                    glib::subclass::Signal::builder("search-changed")
                        .param_types([String::static_type()])
                        .build(),
                    // `next-match` (`:183-192`), no params.
                    glib::subclass::Signal::builder("next-match").build(),
                    // `previous-match` (`:195-204`), no params.
                    glib::subclass::Signal::builder("previous-match").build(),
                ]
            })
        }
    }
    impl WidgetImpl for SearchOverlay {}
    impl BinImpl for SearchOverlay {}
}

glib::wrapper! {
    /// PORTS: `search_overlay.zig:24-...` `pub const SearchOverlay`. See the module
    /// doc — a registered-GType placeholder (signals + `active`) until L5.3 T24.
    pub struct SearchOverlay(ObjectSubclass<imp::SearchOverlay>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SearchOverlay {
    fn default() -> Self {
        glib::Object::new()
    }
}
