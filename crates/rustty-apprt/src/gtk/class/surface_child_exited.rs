//! rustty-apprt::gtk::class::surface_child_exited
//!
//! PORTS: ghostty `src/apprt/gtk/class/surface_child_exited.zig`
//! (`GhosttySurfaceChildExited`).
//!
//! **INHABITED placeholder (L5.2 T13 → filled by L5.3 T24).** `Surface`'s
//! template instantiates `$GhosttySurfaceChildExited child_exited_overlay { … }`
//! (`surface.blp:151-154`), binding `visible: bind template.child-exited`
//! (inherited `GtkWidget:visible`) and connecting the `close-request` signal.
//! For the template to load, this class must register that exact signal now; the
//! reference's "child exited" banner UI (the `data` message + `banner` child,
//! `surface_child_exited.zig:36-...`) lands with T24. The placeholder is an
//! `adw::Bin` subclass with the reference GType name plus the `close-request`
//! signal (`surface_child_exited.zig:51-...`).

use gtk4::subclass::prelude::*;
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;
    use std::sync::OnceLock;

    /// PORTS: `surface_child_exited.zig:24` `pub const SurfaceChildExited`.
    /// Placeholder instance struct — the reference `Private` (`data`/`banner`)
    /// lands with T24.
    #[derive(Default)]
    pub struct SurfaceChildExited;

    #[glib::object_subclass]
    impl ObjectSubclass for SurfaceChildExited {
        // PORTS: `surface_child_exited.zig:27` `.name = "GhosttySurfaceChildExited"`.
        const NAME: &'static str = "GhosttySurfaceChildExited";
        type Type = super::SurfaceChildExited;
        // PORTS: `surface_child_exited.zig:25` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;
    }

    impl ObjectImpl for SurfaceChildExited {
        /// PORTS: `surface_child_exited.zig:51-...` signal `close-request` — the
        /// one the surface template connects (`surface.blp:153`).
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("close-request").build()])
        }
    }
    impl WidgetImpl for SurfaceChildExited {}
    impl BinImpl for SurfaceChildExited {}
}

glib::wrapper! {
    /// PORTS: `surface_child_exited.zig:24-...` `pub const SurfaceChildExited`. See
    /// the module doc — a registered-GType placeholder (`close-request`) until
    /// L5.3 T24.
    pub struct SurfaceChildExited(ObjectSubclass<imp::SurfaceChildExited>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SurfaceChildExited {
    fn default() -> Self {
        glib::Object::new()
    }
}
