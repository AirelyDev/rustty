//! rustty-apprt::gtk::class::tab
//!
//! PORTS: ghostty `src/apprt/gtk/class/tab.zig` (`GhosttyTab`).
//!
//! **INHABITED placeholder (created by L5.3 T20 → filled by L5.3 T21).**
//! [`class::window::Window`]'s `Class.init` `ensureType(Tab)`s this GType and
//! `ui/1.5/window.ui` casts through it — `tab_view.selected-page.child as
//! <$GhosttyTab>.active-surface` (`window.blp:50`) — so the type must register
//! NOW with an `active-surface` property for the window subtitle expression to
//! build. The window also reads a tab's `active-surface`/`needs-confirm-quit`/
//! surface-tree on live code paths (`getActiveSurface`, `getNeedsConfirmQuit`,
//! `tabViewClosePage`). The real tab — its `config`/`title`/`title_override`/
//! `tooltip`/`split_tree` (PREPORT_GTK §1.4) and the tab UI — lands with T21; the
//! window's tab-*creation* paths (`newTabPage`) stay INHABITED leaves until then,
//! so this placeholder implements only the read surface the window's live paths
//! touch.
//!
//! [`class::window::Window`]: crate::gtk::class::window::Window

use std::cell::RefCell;
use std::sync::OnceLock;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use libadwaita::subclass::prelude::*;

use super::split_tree::Tree;
use super::surface::Surface;

mod imp {
    use super::*;

    /// PORTS: `tab.zig:158` `Tab.Private`. Placeholder instance struct — only the
    /// `active-surface` binding surface the window template + live paths need. The
    /// reference `config`/`title`/`title_override`/`tooltip`/`split_tree` (§1.4)
    /// land with T21.
    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::Tab)]
    pub struct Tab {
        /// The tab's active surface (`tab.zig` `active-surface`). Bound through by
        /// the window subtitle expression (`window.blp:50`) and read by the
        /// window's `getActiveSurface` (`window.zig:903-906`). Placeholder storage
        /// (T21 makes it computed from the split tree).
        #[property(name = "active-surface", get, set, nullable)]
        pub active_surface: RefCell<Option<Surface>>,

        /// `tab.zig:163` `title` — the string the window's `tab_bindings`
        /// BindingGroup syncs to the window title (`window.zig:322`). Placeholder
        /// storage until T21.
        #[property(get, set, nullable)]
        pub title: RefCell<Option<String>>,

        /// `tab.zig:169` `tooltip` — bound to the tab page tooltip
        /// (`window.zig:480-485`). Placeholder storage until T21.
        #[property(get, set, nullable)]
        pub tooltip: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Tab {
        // PORTS: `tab.zig` `.name = "GhosttyTab"`.
        const NAME: &'static str = "GhosttyTab";
        type Type = super::Tab;
        // PORTS: `tab.zig` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Tab {
        /// PORTS: `tab.zig` the `close-request` signal — connected by the window's
        /// `tabViewPageAttached` (`window.zig:1533-1539`, an INHABITED leaf until
        /// T21). Registered now so the type is complete.
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("close-request").build()])
        }
    }
    impl WidgetImpl for Tab {}
    impl BinImpl for Tab {}
}

glib::wrapper! {
    /// PORTS: `tab.zig` `pub const Tab`. A registered-GType placeholder
    /// (`active-surface`/`title`/`tooltip` + `close-request`) until L5.3 T21.
    pub struct Tab(ObjectSubclass<imp::Tab>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Tab {
    /// PORTS: `tab.zig` `getActiveSurface` (`window.zig:905` consumes it). Reads
    /// the `active-surface` property. Does not add a ref beyond the property clone.
    #[must_use]
    pub fn get_active_surface(&self) -> Option<Surface> {
        self.active_surface()
    }

    /// PORTS: `tab.zig` `getNeedsConfirmQuit` (`window.zig:980`, `:1471` consume
    /// it). **Placeholder (T21):** a tab with no live surfaces never needs
    /// confirmation, so this reports `false` until the real per-surface check lands.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn get_needs_confirm_quit(&self) -> bool {
        false
    }

    /// PORTS: `tab.zig` `getSurfaceTree` (`window.zig:1560`, `:1585` consume it).
    /// **Placeholder (T21):** returns `None` (no live surface tree) so the window's
    /// `connectSurfaceHandlers`/`disconnectSurfaceHandlers` leaves are never reached.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn get_surface_tree(&self) -> Option<Tree> {
        None
    }
}

impl Default for Tab {
    fn default() -> Self {
        glib::Object::new()
    }
}
