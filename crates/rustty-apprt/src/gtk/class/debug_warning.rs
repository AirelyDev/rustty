//! rustty-apprt::gtk::class::debug_warning
//!
//! PORTS: ghostty `src/apprt/gtk/class/debug_warning.zig` (`GhosttyDebugWarning`).
//!
//! The "you are running a debug build" warning banner. [`class::window::Window`]'s
//! template instantiates `$GhosttyDebugWarning { visible: bind template.debug; }`
//! (`window.blp:153-155`); the widget itself is a template over
//! `ui/1.3/debug-warning.blp` — an `Adw.Banner` child carrying the warning title
//! (`revealed: true`) inside the AD-3-collapsed `adw.Bin` parent
//! (`debug_warning.zig:14`, the `supportsBanner()` branch — the legacy `gtk.Box`
//! arm is dropped).
//!
//! [`class::window::Window`]: crate::gtk::class::window::Window

use gtk4::subclass::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::gtk::gresource;

mod imp {
    use super::*;

    /// PORTS: `debug_warning.zig` `pub const DebugWarning`. No `Private`: the
    /// banner and its title live entirely in the `.blp` (no template children or
    /// callbacks here).
    #[derive(Default, gtk4::CompositeTemplate)]
    #[template(resource = "/io/airely/rustty/ui/1.3/debug-warning.ui")]
    pub struct DebugWarning;

    #[glib::object_subclass]
    impl ObjectSubclass for DebugWarning {
        // PORTS: `debug_warning.zig` `.name = "GhosttyDebugWarning"`.
        const NAME: &'static str = "GhosttyDebugWarning";
        type Type = super::DebugWarning;
        // PORTS: `debug_warning.zig:14` `pub const Parent = adw.Bin` (AD-3: the
        // `supportsBanner()` version branch collapses to the modern `adw.Bin` arm).
        type ParentType = libadwaita::Bin;

        /// PORTS: `debug_warning.zig:37-46` `Class.init` —
        /// `setTemplateFromResource(gresource.blueprint(.{ .major = 1, .minor = 3
        /// (the `supportsBanner()` arm), .name = "debug-warning" }))`.
        fn class_init(klass: &mut Self::Class) {
            klass.set_template_from_resource(gresource::blueprint(1, 3, "debug-warning"));
        }

        /// PORTS: `debug_warning.zig:22-24` `init` — `gtk.Widget.initTemplate`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DebugWarning {}
    impl WidgetImpl for DebugWarning {}
    impl BinImpl for DebugWarning {}
}

glib::wrapper! {
    /// PORTS: `debug_warning.zig` `pub const DebugWarning`.
    pub struct DebugWarning(ObjectSubclass<imp::DebugWarning>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for DebugWarning {
    fn default() -> Self {
        glib::Object::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `#[template]` literal must equal the const-folded resolver
    /// (the `surface.rs` drift-guard idiom), so the two never drift.
    #[test]
    fn template_resource_matches_blueprint() {
        assert_eq!(
            "/io/airely/rustty/ui/1.3/debug-warning.ui",
            gresource::blueprint(1, 3, "debug-warning")
        );
    }

    /// F-019 KAT: the template must actually BIND (`debug_warning.zig:22-24,
    /// 37-46`) — constructing a `DebugWarning` inflates the `Adw.Banner` child
    /// from `ui/1.3/debug-warning.blp` into the Bin. Against the pre-fix
    /// placeholder (no `class_init`/`instance_init`) the Bin renders empty —
    /// `first_child()` is `None` — and this fails.
    #[test]
    #[ignore = "requires a display; run via xvfb-run -a cargo test -p rustty-apprt --features gtk -- --ignored"]
    fn template_inflates_banner_child() {
        use glib::prelude::Cast as _;
        use gtk4::prelude::WidgetExt as _;
        // `Banner::is_revealed()`/`title()` are inherent in libadwaita 0.9.2
        // (`src/auto/banner.rs:75,81`) — there is no `BannerExt` to import.

        if gtk4::init().is_err() {
            return;
        }
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        let warning = DebugWarning::default();
        let child = warning
            .first_child()
            .expect("the template inflates a child into the Bin");
        let banner = child
            .downcast::<libadwaita::Banner>()
            .expect("the child is the Adw.Banner (debug-warning.blp:5-8)");
        assert!(banner.is_revealed(), "revealed: true (debug-warning.blp:7)");
        assert!(
            banner.title().contains("debug build"),
            "the banner carries the debug-build warning title, got {:?}",
            banner.title()
        );
    }
}
