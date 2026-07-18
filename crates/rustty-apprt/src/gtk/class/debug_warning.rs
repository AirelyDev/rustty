//! rustty-apprt::gtk::class::debug_warning
//!
//! PORTS: ghostty `src/apprt/gtk/class/debug_warning.zig` (`GhosttyDebugWarning`).
//!
//! **INHABITED placeholder (created by L5.3 T20 → filled by L5.3 T23).**
//! [`class::window::Window`]'s template instantiates `$GhosttyDebugWarning {
//! visible: bind template.debug; }` (`window.blp:153-155`); for `ui/1.5/window.ui`
//! to build, this GType must be registered and instantiable NOW. The reference's
//! actual "you are running a debug build" banner UI — its `data`/`banner` children
//! and the AD-3-collapsed `adw.Bin` parent (`debug_warning.zig:14`, drop the
//! `gtk.Box` arm) — lands with T23. The placeholder is a bare `adw::Bin` subclass
//! carrying the reference GType name so the window template resolves the child.
//!
//! [`class::window::Window`]: crate::gtk::class::window::Window

use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;

    /// PORTS: `debug_warning.zig` `pub const DebugWarning`. Placeholder instance
    /// struct — the reference `Private` lands with T23.
    #[derive(Default)]
    pub struct DebugWarning;

    #[glib::object_subclass]
    impl ObjectSubclass for DebugWarning {
        // PORTS: `debug_warning.zig` `.name = "GhosttyDebugWarning"`.
        const NAME: &'static str = "GhosttyDebugWarning";
        type Type = super::DebugWarning;
        // PORTS: `debug_warning.zig:14` `pub const Parent = adw.Bin` (AD-3: the
        // `supportsBanner()` version branch collapses to the modern `adw.Bin` arm).
        type ParentType = libadwaita::Bin;
    }

    impl ObjectImpl for DebugWarning {}
    impl WidgetImpl for DebugWarning {}
    impl BinImpl for DebugWarning {}
}

glib::wrapper! {
    /// PORTS: `debug_warning.zig` `pub const DebugWarning`. A registered-GType
    /// placeholder until L5.3 T23 (see the module doc).
    pub struct DebugWarning(ObjectSubclass<imp::DebugWarning>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for DebugWarning {
    fn default() -> Self {
        glib::Object::new()
    }
}
