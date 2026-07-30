//! rustty-apprt::gtk::class::surface_child_exited
//!
//! PORTS: ghostty `src/apprt/gtk/class/surface_child_exited.zig`
//! (`GhosttySurfaceChildExited`).
//!
//! The "child exited" banner overlay (`surface_child_exited.zig:20-202`, the
//! `adw.Banner`-based `SurfaceChildExitedBanner` — Adwaita 1.3+, which this port
//! targets). `Surface`'s template instantiates `$GhosttySurfaceChildExited
//! child_exited_overlay { … }` (`surface.blp:151-154`), binding `visible: bind
//! template.child-exited` and connecting `close-request`. This is a
//! `CompositeTemplate` over `ui/1.3/surface-child-exited.ui`: the `adw.Banner`
//! child whose title reflects the exit code, plus the `data` boxed property that
//! drives it (`notify::data`) and the `close-request` outward signal.
//!
//! Mirrors the `CompositeTemplate` + `#[gtk4::template_callbacks]` idiom of
//! `search_overlay.rs` (the `clicked` signal callback is NON-swapped, so `this`
//! comes LAST; the `notify_data` handler's arg0 genuinely IS Self, so it is
//! `&self`).

use std::cell::RefCell;
use std::sync::OnceLock;

use glib::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::subclass::prelude::*;

use crate::gtk::i18n::tr;

/// PORTS: `apprt.surface.Message.ChildExited` boxed as `"GhosttyApprtChildExited"`
/// (`surface_child_exited.zig:40`, `gobject.ext.defineBoxed`). The apprt-api crate
/// defines the plain struct (its `getGObjectType` is "deferred to rustty-apprt's
/// gtk backend"); this newtype is that deferred boxed registration, so the `data`
/// property can carry the child-exit payload.
#[derive(Debug, Clone, Copy, glib::Boxed)]
#[boxed_type(name = "GhosttyApprtChildExited", nullable)]
pub struct ChildExited(pub rustty_apprt_api::surface::ChildExited);

mod imp {
    use super::*;

    /// PORTS: `surface_child_exited.zig:22` `pub const SurfaceChildExitedBanner`
    /// and its `Private` (`:62-70`). A `CompositeTemplate` over
    /// `ui/1.3/surface-child-exited.ui`; binds the `banner` child (`:67,181`).
    #[derive(CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::SurfaceChildExited)]
    #[template(resource = "/io/airely/rustty/ui/1.3/surface-child-exited.ui")]
    pub struct SurfaceChildExited {
        /// PORTS: `surface_child_exited.zig:67` `banner: *adw.Banner` — the banner
        /// whose title reflects the exit status (`surface-child-exited.blp:12`).
        #[template_child]
        pub banner: TemplateChild<libadwaita::Banner>,

        /// PORTS: `surface_child_exited.zig:34-46,64` property/field `data`
        /// (`?*apprt.surface.Message.ChildExited`, OWNED boxed;
        /// `privateBoxedFieldAccessor`). The child-exit payload sent by the apprt;
        /// its `notify::data` drives [`Self::on_notify_data`].
        #[property(get, set, nullable)]
        pub data: RefCell<Option<ChildExited>>,
    }

    impl Default for SurfaceChildExited {
        fn default() -> Self {
            Self {
                banner: TemplateChild::default(),
                data: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SurfaceChildExited {
        // PORTS: `surface_child_exited.zig:27` `.name = "GhosttySurfaceChildExited"`.
        const NAME: &'static str = "GhosttySurfaceChildExited";
        type Type = super::SurfaceChildExited;
        // PORTS: `surface_child_exited.zig:25` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;

        /// PORTS: `surface_child_exited.zig:170-196` `Class.init` — load the
        /// template, bind the `banner` child + the two template callbacks.
        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        /// PORTS: `surface_child_exited.zig:72-74` `init` — `initTemplate`.
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for SurfaceChildExited {
        /// PORTS: `surface_child_exited.zig:48-60` signal `close-request` — the one
        /// the surface template connects (`surface.blp:153`).
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("close-request").build()])
        }

        /// PORTS: `surface_child_exited.zig:134-144` `dispose` — dispose the
        /// template. The `banner` is nested (not the Bin's root child), so no
        /// double-free workaround is needed. The OWNED `data` (reference
        /// `finalize`, `:146-157`) drops with the `RefCell`.
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for SurfaceChildExited {}
    impl BinImpl for SurfaceChildExited {}
}

glib::wrapper! {
    /// PORTS: `surface_child_exited.zig:22` `pub const SurfaceChildExitedBanner`.
    pub struct SurfaceChildExited(ObjectSubclass<imp::SurfaceChildExited>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for SurfaceChildExited {
    fn default() -> Self {
        glib::Object::new()
    }
}

#[gtk4::template_callbacks]
impl SurfaceChildExited {
    /// PORTS: `surface_child_exited.zig:96-117` `propData`. Update the banner
    /// title (and the widget's `normal`/`abnormal` CSS classes) from the current
    /// `data`. The `notify::data` handler's arg0 genuinely IS Self, so this takes
    /// `&self` (`surface-child-exited.blp:9` `notify::data => $notify_data()`).
    /// Renamed `on_notify_data` (blueprint name kept via `name = …`) so it does
    /// not clash with the `glib::Properties`-generated `notify_data()` emitter.
    #[template_callback(name = "notify_data")]
    fn on_notify_data(&self) {
        let imp = self.imp();
        let banner = &imp.banner;
        let widget = self.upcast_ref::<gtk4::Widget>();

        let data = imp.data.borrow();
        let Some(ChildExited(data)) = data.as_ref() else {
            // Not localized on purpose (`:104-106`). Matches the blp default
            // (`surface-child-exited.blp:16`, which the port keeps verbatim).
            banner.set_title("This is a bug in Rustty. Please report it.");
            return;
        };

        // `data.exit_code == 0` (`:108-116`). CSS classes toggle on the widget,
        // not the banner. PORTS the reference's `i18n._(...)` wrapping of both
        // banner titles (`:109`, `:113`) via [`crate::gtk::i18n::tr`].
        if data.exit_code == 0 {
            banner.set_title(tr(c"Command succeeded"));
            widget.add_css_class("normal");
            widget.remove_css_class("abnormal");
        } else {
            banner.set_title(tr(c"Command failed"));
            widget.remove_css_class("normal");
            widget.add_css_class("abnormal");
        }
    }

    /// PORTS: `surface_child_exited.zig:119-129` `closeButtonClicked`. Emit
    /// `close-request` so the surface closes. NON-swapped signal
    /// (`surface-child-exited.blp:13` `button-clicked => $clicked()`): the emitting
    /// `adw.Banner` is arg 0, `this: &SurfaceChildExited` is LAST.
    #[template_callback]
    fn clicked(_banner: &libadwaita::Banner, this: &SurfaceChildExited) {
        this.emit_by_name::<()>("close-request", &[]);
    }
}

impl SurfaceChildExited {
    /// PORTS: `surface_child_exited.zig:76-91` `setData`. Store the child-exit
    /// payload (the reference copies onto the heap; here the `Copy` value rides
    /// the boxed [`ChildExited`] newtype). The generated property setter
    /// auto-emits `notify::data`, driving [`Self::on_notify_data`].
    pub fn set_child_exited_data(&self, data: Option<rustty_apprt_api::surface::ChildExited>) {
        self.set_data(data.map(ChildExited));
    }
}
