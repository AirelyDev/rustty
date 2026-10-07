//! rustty-apprt::gtk::class::surface_scrolled_window
//!
//! PORTS: ghostty `src/apprt/gtk/class/surface_scrolled_window.zig`
//! (`GhostttySurfaceScrolledWindow`).
//!
//! A wrapper widget that embeds a [`Surface`] inside a `GtkScrolledWindow`,
//! providing scrollbar functionality for the terminal surface. The `surface`
//! property can be set at construction (`buildTree`'s `.initNew`,
//! `split_tree.zig:967-970`) or changed dynamically.
//!
//! PREPORT_GTK §1.8 lifetimes: `config` SHARED (`RefCell<Option<Config>>`,
//! unref'd in dispose), `config_binding` OWNED (`RefCell<Option<glib::Binding>>`,
//! our ref released in `dispose`/`propSurface`), `surface` BORROW_FIELD (the
//! surface is owned by the split tree; this field is a non-owning-by-intent
//! clone that dispose drops), `scrolled_window` TEMPLATE_CHILD (released en
//! masse by `dispose_template`).

use std::cell::RefCell;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::subclass::prelude::*;

use super::config::Config;
use super::surface::Surface;
use crate::gtk::gtk_version;

mod imp {
    use super::*;

    /// PORTS: `surface_scrolled_window.zig:59` `SurfaceScrolledWindow.Private`.
    #[derive(CompositeTemplate, Properties)]
    #[template(resource = "/io/airely/rustty/ui/1.5/surface-scrolled-window.ui")]
    #[properties(wrapper_type = super::SurfaceScrolledWindow)]
    pub struct SurfaceScrolledWindow {
        /// PORTS: `surface_scrolled_window.zig:31-41,60` `config` (`?*Config`,
        /// SHARED; obj accessor).
        #[property(get, set, nullable)]
        pub config: RefCell<Option<Config>>,

        /// PORTS: `surface_scrolled_window.zig:61` `config_binding`. OWNED: our
        /// ref on the `GBinding` created by `bindProperty`, released (dropped) in
        /// `propSurface`/`dispose`.
        pub config_binding: RefCell<Option<glib::Binding>>,

        /// PORTS: `surface_scrolled_window.zig:43-56,62` `surface`. Custom
        /// getter/setter (see [`SurfaceScrolledWindow::get_surface_value`] /
        /// `set_surface_value`).
        #[property(name = "surface", get = Self::get_surface_value, set = Self::set_surface_value, nullable, explicit_notify)]
        pub surface: RefCell<Option<Surface>>,

        /// PORTS: `surface_scrolled_window.zig:63` `scrolled_window` template
        /// child.
        #[template_child]
        pub scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    }

    impl Default for SurfaceScrolledWindow {
        fn default() -> Self {
            Self {
                config: RefCell::new(None),
                config_binding: RefCell::new(None),
                surface: RefCell::new(None),
                scrolled_window: TemplateChild::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SurfaceScrolledWindow {
        // PORTS: `surface_scrolled_window.zig:23` `.name`. NOTE: the reference
        // name has a three-`t` typo ("GhostttySurfaceScrolledWindow"); the
        // vendored `surface-scrolled-window.blp` template uses the identical
        // spelling, so it is preserved verbatim (parity — the GType name must
        // match the `.ui` `template` class).
        const NAME: &'static str = "GhostttySurfaceScrolledWindow";
        type Type = super::SurfaceScrolledWindow;
        type ParentType = libadwaita::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            super::SurfaceScrolledWindow::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for SurfaceScrolledWindow {
        /// PORTS: `surface_scrolled_window.zig:67-70` `init`.
        fn constructed(&self) {
            self.parent_constructed();
            // Until gtk 4.20.1 trackpads have kinetic scrolling behavior
            // regardless of `Gtk.ScrolledWindow.kinetic_scrolling`
            // (`:69,72-85`).
            if gtk_version::runtime_until(4, 20, 1) {
                self.obj().disable_kinetic_scroll();
            }
        }

        /// PORTS: `surface_scrolled_window.zig:88-111` `dispose`.
        fn dispose(&self) {
            // Unbind, then release our ref on the config binding (`:91-95`;
            // `6cd684d5d` added the `unbind()` — the binding's own ref goes with
            // it, ours with the drop).
            if let Some(binding) = self.config_binding.borrow_mut().take() {
                binding.unbind();
            }
            // Unref the config (`:96-99`).
            self.config.borrow_mut().take();
            self.dispose_template();
        }
    }

    impl WidgetImpl for SurfaceScrolledWindow {}
    impl BinImpl for SurfaceScrolledWindow {}

    impl SurfaceScrolledWindow {
        /// PORTS: `surface_scrolled_window.zig:119-124` `getSurfaceValue`.
        fn get_surface_value(&self) -> Option<Surface> {
            self.surface.borrow().clone()
        }

        /// PORTS: `surface_scrolled_window.zig:126-131` `setSurfaceValue`.
        fn set_surface_value(&self, surface: Option<Surface>) {
            self.obj().set_surface_inner(surface.as_ref());
        }
    }
}

glib::wrapper! {
    /// A wrapper widget that embeds a Surface inside a GtkScrolledWindow.
    /// This provides scrollbar functionality for the terminal surface.
    /// The surface property can be set during initialization or changed
    /// dynamically via the surface property.
    ///
    /// PORTS: `surface_scrolled_window.zig:18` `pub const SurfaceScrolledWindow`.
    pub struct SurfaceScrolledWindow(ObjectSubclass<imp::SurfaceScrolledWindow>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl SurfaceScrolledWindow {
    /// PORTS: `surface_scrolled_window.zig` `Class.init` — the `bindTemplateCallback` calls of `Class.init`,
    /// bound through GTK's C builder scope (NEW-FINDINGS N516; see
    /// [`template_callback`](crate::gtk::class::template_callback)).
    fn bind_template_callbacks(klass: &mut <imp::SurfaceScrolledWindow as ObjectSubclass>::Class) {
        crate::gtk::class::template_callback::bind_template_callbacks!(klass;
            c"scrollbar_policy" => SurfaceScrolledWindow::closure_scrollbar_policy,
                (this: obj(SurfaceScrolledWindow), config: opt_obj(Config)) -> glib(gtk4::PolicyType);
            c"notify_surface" => SurfaceScrolledWindow::on_notify_surface,
                (emitter: obj(libadwaita::Bin), pspec: obj(glib::ParamSpec), this: obj(SurfaceScrolledWindow));
        );
    }
}

impl SurfaceScrolledWindow {
    /// PORTS: `surface_scrolled_window.zig:133-135` `getSurface`. This does not
    /// add a ref beyond the property clone.
    #[must_use]
    pub fn get_surface(&self) -> Option<Surface> {
        self.imp().surface.borrow().clone()
    }

    /// PORTS: `surface_scrolled_window.zig:137-147` `setSurface`. (Named
    /// `_inner` to avoid clashing with the generated `surface` property setter,
    /// which forwards here via `set_surface_value`.)
    pub fn set_surface_inner(&self, surface: Option<&Surface>) {
        let imp = self.imp();

        // If unchanged, do nothing (`:140`).
        {
            let cur = imp.surface.borrow();
            if cur.as_ref() == surface {
                return;
            }
        }

        // The reference notifies BEFORE assigning and relies on freeze/thaw
        // batching so `propSurface` (the notify handler) observes the *new*
        // value once thaw flushes (`:142-146`). We reproduce that exactly.
        let guard = self.freeze_notify();
        self.notify("surface");
        *imp.surface.borrow_mut() = surface.cloned();
        drop(guard);
    }

    /// PORTS: `surface_scrolled_window.zig:72-86` `disableKineticScroll`.
    /// Disable `EventControllerScroll.kinetic` on the scrolled window's
    /// controllers as a workaround for the pre-4.20.1 trackpad behavior.
    fn disable_kinetic_scroll(&self) {
        // Until gtk 4.20.1 trackpads have kinetic scrolling behavior regardless
        // of `Gtk.ScrolledWindow.kinetic_scrolling`. As a workaround, disable
        // EventControllerScroll.kinetic
        let controllers = self.imp().scrolled_window.observe_controllers();
        let mut i = 0u32;
        while let Some(obj) = controllers.item(i) {
            i += 1;
            if let Ok(controller) = obj.downcast::<gtk4::EventControllerScroll>() {
                let mut flags = controller.flags();
                flags.remove(gtk4::EventControllerScrollFlags::KINETIC);
                controller.set_flags(flags);
            }
        }
    }

    /// PORTS: `surface_scrolled_window.zig:149-158` `closureScrollbarPolicy`.
    /// Bound in the template (`surface-scrolled-window.blp:13`). The template
    /// scope passes the instance first.
    fn closure_scrollbar_policy(
        _this: &SurfaceScrolledWindow,
        config: Option<Config>,
    ) -> gtk4::PolicyType {
        let Some(config) = config else {
            return gtk4::PolicyType::Automatic;
        };
        match config.get().scrollbar {
            rustty_config::value_types::Scrollbar::Never => gtk4::PolicyType::Never,
            rustty_config::value_types::Scrollbar::System => gtk4::PolicyType::Automatic,
        }
    }

    /// PORTS: `surface_scrolled_window.zig:160-184` `propSurface`. Reparents the
    /// surface into the scrolled window and (re)binds `config` from the surface.
    fn on_notify_surface(
        _emitter: &libadwaita::Bin,
        _pspec: &glib::ParamSpec,
        this: &SurfaceScrolledWindow,
    ) {
        let imp = this.imp();

        // Move the surface into the scrolled window (`:166-167`).
        let surface = imp.surface.borrow().clone();
        imp.scrolled_window
            .set_child(surface.as_ref().map(|s| s.upcast_ref::<gtk4::Widget>()));

        // Unbind old config binding if it exists (`:170-175`): `unbind()` then
        // release our ref (the drop) — `6cd684d5d` added the `unbind()`.
        if let Some(binding) = imp.config_binding.borrow_mut().take() {
            binding.unbind();
        }

        // Bind config from surface to our config property (`:178-189`).
        if let Some(surface) = surface {
            let binding = surface
                .bind_property("config", this, "config")
                .sync_create()
                .build();
            // Keep another ref, otherwise the binding would be freed and
            // our pointer become stale if the surface gets finalized.
            //
            // PORT NOTE (`6cd684d5d`): the reference's explicit `binding.ref()`
            // is what gtk-rs's `build()` already does (`from_glib_none` over the
            // (transfer none) `g_object_bind_property` result), so the strong
            // `glib::Binding` stored here IS that extra ref — see
            // `Surface::bind_is_split`.
            *imp.config_binding.borrow_mut() = Some(binding);
        }
    }
}

impl Default for SurfaceScrolledWindow {
    fn default() -> Self {
        glib::Object::new()
    }
}
