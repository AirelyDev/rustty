//! rustty-apprt::gtk::class::resize_overlay
//!
//! PORTS: ghostty `src/apprt/gtk/class/resize_overlay.zig` (`GhosttyResizeOverlay`).
//!
//! **INHABITED placeholder (L5.2 T13 → filled by L5.3 T24).** `Surface`'s
//! template instantiates `$GhosttyResizeOverlay resize_overlay {}`
//! (`surface.blp:157`) and its `class_init` `ensure_type`s this class
//! (`surface.zig:3594`), so the GType must be **registered** now for the template
//! to load. The reference is a scheduled resize-dimension overlay (label +
//! three glib source ids, `resize_overlay.zig:127-153`); none of that behavior is
//! reachable until a surface renders, so the placeholder is a bare `adw::Bin`
//! subclass registered under the exact reference GType name. T24 ports the label,
//! the `duration`/`first-delay`/`overlay-{h,v}align` properties, and the timers.

use gtk4::subclass::prelude::*;
use libadwaita::subclass::prelude::*;

mod imp {
    use super::*;

    /// PORTS: `resize_overlay.zig:24` `pub const ResizeOverlay`. Placeholder
    /// instance struct — the reference `Private` (`:127-153`) lands with T24.
    #[derive(Default)]
    pub struct ResizeOverlay;

    #[glib::object_subclass]
    impl ObjectSubclass for ResizeOverlay {
        // PORTS: `resize_overlay.zig:29` `.name = "GhosttyResizeOverlay"`.
        // Preserved 1:1 so the template's `$GhosttyResizeOverlay` resolves.
        const NAME: &'static str = "GhosttyResizeOverlay";
        type Type = super::ResizeOverlay;
        // PORTS: `resize_overlay.zig:27` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;
    }

    impl ObjectImpl for ResizeOverlay {}
    impl WidgetImpl for ResizeOverlay {}
    impl BinImpl for ResizeOverlay {}
}

glib::wrapper! {
    /// PORTS: `resize_overlay.zig:24-...` `pub const ResizeOverlay`. See the module
    /// doc — a registered-GType placeholder until L5.3 T24.
    pub struct ResizeOverlay(ObjectSubclass<imp::ResizeOverlay>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ResizeOverlay {
    fn default() -> Self {
        glib::Object::new()
    }
}
