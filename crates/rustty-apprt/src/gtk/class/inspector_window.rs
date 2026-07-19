//! rustty-apprt::gtk::class::inspector_window
//!
//! PORTS: ghostty `src/apprt/gtk/class/inspector_window.zig` (218 loc) — the
//! window for displaying the Ghostty/Rustty terminal inspector, an
//! [`adw::ApplicationWindow`] subclass that hosts one
//! [`InspectorWidget`](super::inspector_widget::InspectorWidget) and auto-closes
//! when the inspected terminal (surface) goes away.
//!
//! ## The `surface` weak lifecycle + auto-close (`inspector_window.zig:69-71,160-171`)
//!
//! The window holds the surface it inspects through a
//! [`WeakRef`](crate::gtk::WeakRef) (NOT a strong ref) and binds it into the
//! embedded [`InspectorWidget`](super::inspector_widget::InspectorWidget) via the
//! template (`inspector-window.blp:34`). The widget attaches its own weak-notify
//! to that surface; when the surface is disposed the widget nulls its `surface`
//! and notifies the `surface` pspec, which fires the template callback
//! [`notify_inspector_surface`](InspectorWindow::on_notify_inspector_surface) —
//! seeing the widget's surface gone, the window destroys itself.
//!
//! ## Valgrind-found corruption in `dispose` (`inspector_window.zig:95-101`)
//!
//! `dispose` MUST clear the weak ref ([`set_surface`](InspectorWindow::set_surface)
//! `None`); the oracle notes that failing to do so is not a leak but an invalid
//! memory READ on the target (weak-referenced) object at dispose time — caught
//! only via Valgrind, and in practice observed to hang. Ported verbatim.

use std::marker::PhantomData;

use gtk4::glib;
use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita as adw;
use libadwaita::subclass::prelude::*;

use crate::gtk::WeakRef;

use super::debug_warning::DebugWarning;
use super::inspector_widget::InspectorWidget;
use super::surface::Surface;

mod imp {
    use super::*;

    /// PORTS: `inspector_window.zig:69-77` `InspectorWindow.Private`.
    #[derive(Default, CompositeTemplate, Properties)]
    #[template(resource = "/io/airely/rustty/ui/1.5/inspector-window.ui")]
    #[properties(wrapper_type = super::InspectorWindow)]
    pub struct InspectorWindow {
        /// PORTS: `inspector_window.zig:31-45` `surface` property — the surface
        /// this window is attached to. The backing store is the WEAK reference
        /// (`inspector_window.zig:70-71` `surface: WeakRef(Surface) = .empty`);
        /// the getter refs (transfer `.none`), the setter replaces the weak.
        #[property(name = "surface", get = Self::get_surface_value, set = Self::set_surface_value, nullable)]
        pub surface_prop: PhantomData<Option<Surface>>,

        /// PORTS: `inspector_window.zig:47-64` `debug` property — const
        /// `build_config.is_debug`. Bound by the template (`inspector-window.blp:29`)
        /// to the `DebugWarning`'s visibility. Read-only (getter only).
        #[property(name = "debug", get = Self::debug_prop)]
        pub debug_prop: PhantomData<bool>,

        /// PORTS: `inspector_window.zig:70-71` `surface` — the surface that we are
        /// attached to, held WEAKLY (**invariant 4**: the referent can finalize out
        /// from under us). The `surface` property's backing store.
        pub surface: WeakRef<Surface>,

        /// PORTS: `inspector_window.zig:73-74` `inspector_widget` — the embedded
        /// inspector widget (BORROW_FIELD; `disposeTemplate` frees it).
        #[template_child]
        pub inspector_widget: TemplateChild<InspectorWidget>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InspectorWindow {
        // PORTS: `inspector_window.zig:24` `.name = "GhosttyInspectorWindow"`. The
        // GType name must match the vendored `inspector-window.blp` template class
        // (`template $GhosttyInspectorWindow`), so it is preserved verbatim.
        const NAME: &'static str = "GhosttyInspectorWindow";
        type Type = super::InspectorWindow;
        // PORTS: `inspector_window.zig:22` `pub const Parent = adw.ApplicationWindow`.
        type ParentType = adw::ApplicationWindow;

        /// PORTS: `inspector_window.zig:185-206` `Class.init`. `ensureType`s the two
        /// template subclasses so `ui/1.5/inspector-window.ui` resolves them
        /// (`:186-187`), then binds the template + the instance callbacks
        /// (`notify_inspector_surface`, `:201`).
        fn class_init(klass: &mut Self::Class) {
            DebugWarning::ensure_type();
            InspectorWidget::ensure_type();

            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            // PORTS: `inspector_window.zig:83` `gtk.Widget.initTemplate`.
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for InspectorWindow {
        /// PORTS: `inspector_window.zig:82-93` `init` (instance-init tail).
        /// `init_template` already ran in [`ObjectSubclass::instance_init`]; the
        /// remainder — the debug CSS class and the window icon — runs here.
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Add our dev CSS class if we're in debug mode
            // (`inspector_window.zig:86-88`).
            if cfg!(debug_assertions) {
                obj.add_css_class("devel");
            }

            // Set our window icon. We can't set this in the blueprint file because
            // it's dependent on the build config (`inspector_window.zig:90-92`). R2:
            // the rustty identity, not the hardcoded reference id.
            obj.set_icon_name(Some(rustty_identity::BUNDLE_ID));
        }

        /// PORTS: `inspector_window.zig:95-112` `dispose`. You MUST clear all weak
        /// refs in dispose, otherwise it causes memory corruption on dispose on the
        /// TARGET (weak referenced) object. The only way we caught this is via
        /// Valgrind. It's not a leak, it's an invalid memory read. In practice, we
        /// found this sometimes caused hanging! The parent
        /// (`AdwApplicationWindow` → `GtkWidget` → `GObject`) dispose chains after
        /// we return via the gtk4-rs dispose trampoline (`:103-111`).
        fn dispose(&self) {
            self.set_surface(None);
            self.dispose_template();
        }
    }

    impl WidgetImpl for InspectorWindow {}
    impl WindowImpl for InspectorWindow {}
    impl ApplicationWindowImpl for InspectorWindow {}
    impl AdwApplicationWindowImpl for InspectorWindow {}

    // -- Properties (`inspector_window.zig:134-155`) -------------------------
    impl InspectorWindow {
        /// PORTS: `inspector_window.zig:137-140` `setSurface`. Sets the weak ref
        /// backing the `surface` property (the shared setter used by the property
        /// accessor, `new`, and `dispose`).
        pub fn set_surface(&self, newvalue: Option<&Surface>) {
            self.surface.set(newvalue);
        }

        /// PORTS: `inspector_window.zig:142-148` `getSurfaceValue`. `get()` refs, so
        /// we hand the caller that fresh strong ref (getter transfer `.none`) — the
        /// gtk4-rs `to_value` on the returned `Option<Surface>` performs the same
        /// single-ref-for-the-caller the reference's `Value.take` does.
        fn get_surface_value(&self) -> Option<Surface> {
            self.surface.get()
        }

        /// PORTS: `inspector_window.zig:150-155` `setSurfaceValue`.
        fn set_surface_value(&self, value: Option<Surface>) {
            self.set_surface(value.as_ref());
        }

        /// PORTS: `inspector_window.zig:56-61` `debug` getter — `build_config.is_debug`.
        #[allow(clippy::unused_self)]
        fn debug_prop(&self) -> bool {
            cfg!(debug_assertions)
        }
    }
}

glib::wrapper! {
    /// Window for displaying the Ghostty/Rustty inspector. PORTS:
    /// `inspector_window.zig:19` `pub const InspectorWindow`.
    pub struct InspectorWindow(ObjectSubclass<imp::InspectorWindow>)
        @extends adw::ApplicationWindow, gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gtk4::gio::ActionGroup, gtk4::gio::ActionMap, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl InspectorWindow {
    /// PORTS: `inspector_window.zig:117-121` `new`. Constructs over the surface to
    /// inspect, binding it into the `surface` property (which the template
    /// forwards to the embedded [`InspectorWidget`]).
    #[must_use]
    pub fn new(surface: &Surface) -> Self {
        glib::Object::builder().property("surface", surface).build()
    }

    /// Present the window. PORTS: `inspector_window.zig:124-126` `present`
    /// (`self.as(gtk.Window).present()`).
    pub fn present(&self) {
        self.upcast_ref::<gtk4::Window>().present();
    }

    /// Queue a render of the embedded widget. PORTS:
    /// `inspector_window.zig:129-132` `queueRender`.
    pub fn queue_render(&self) {
        self.imp().inspector_widget.queue_render();
    }

    // PORTS: `inspector_window.zig:137-140` `setSurface`. Exposed as the derived
    // `surface`-property setter `set_surface(&self, Option<&Surface>)` (generated by
    // `#[derive(Properties)]`), which routes through `set_surface_value` →
    // `imp::set_surface`; wiring seams and `dispose` use that / the imp method.
}

#[gtk4::template_callbacks]
impl InspectorWindow {
    /// PORTS: `inspector_window.zig:160-171` `propInspectorSurface`. Bound to the
    /// embedded widget's `notify::surface` (`inspector-window.blp:33`). If the
    /// inspector's surface went away, we destroy the window. The inspector has a
    /// weak notify on the surface so it knows if it goes nil.
    #[template_callback(name = "notify_inspector_surface")]
    fn on_notify_inspector_surface(
        inspector: &InspectorWidget,
        _pspec: &glib::ParamSpec,
        this: &InspectorWindow,
    ) {
        if inspector.get_surface().is_none() {
            this.destroy();
        }
    }
}
