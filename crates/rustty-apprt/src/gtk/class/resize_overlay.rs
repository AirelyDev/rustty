//! rustty-apprt::gtk::class::resize_overlay
//!
//! PORTS: ghostty `src/apprt/gtk/class/resize_overlay.zig` (`GhosttyResizeOverlay`).
//!
//! The overlay that shows the current `cols x rows` size while a surface is
//! resizing (`resize_overlay.zig:11-21`). `Surface`'s template instantiates
//! `$GhosttyResizeOverlay resize_overlay {}` (`surface.blp:157`) and its
//! `class_init` `ensure_type`s this class (`surface.zig:3594`).
//!
//! General usage (`resize_overlay.zig:14-21`):
//!   1. Add it to an overlay (done in `surface.blp`).
//!   2. Set the label with [`ResizeOverlay::set_label`].
//!   3. Schedule to show it with [`ResizeOverlay::schedule`].
//!
//! Mirrors the `CompositeTemplate`-with-child idiom of `title_dialog.rs` (loads
//! `ui/1.2/resize-overlay.ui`, binds the `label` template child) and the
//! `SourceId` RAII of the port (three glib source ids removed in `dispose`,
//! **invariant 11**).

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gtk4::glib::Properties;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::subclass::prelude::*;

/// Removes a glib source id, warning (not panicking) on failure — the faithful
/// shape of the reference's `if (glib.Source.remove(id) == 0) log.warn(...)`
/// (`resize_overlay.zig:196-267`, **invariant 11**). `SourceId`'s own `Drop`
/// does NOT remove the source, so `g_source_remove` is called directly (not
/// [`glib::SourceId::remove`], which would panic on a stale id).
fn remove_source_id(id: Option<glib::SourceId>, what: &str) {
    if let Some(source) = id {
        // SAFETY: `source.as_raw()` is a source id previously returned by glib;
        // passing it to `g_source_remove` is always valid (returns FALSE if gone).
        let removed = unsafe { glib::ffi::g_source_remove(source.as_raw()) };
        if removed == glib::ffi::GFALSE {
            eprintln!("warn(gtk): unable to remove {what} source={}", source.as_raw());
        }
    }
}

mod imp {
    use super::*;

    /// PORTS: `resize_overlay.zig:24` `pub const ResizeOverlay` + its `Private`
    /// (`:127-153`). A `CompositeTemplate` over `ui/1.2/resize-overlay.ui`;
    /// binds the `label` child (`resize_overlay.zig:129,346`).
    #[derive(Properties, gtk4::CompositeTemplate)]
    #[properties(wrapper_type = super::ResizeOverlay)]
    #[template(resource = "/io/airely/rustty/ui/1.2/resize-overlay.ui")]
    pub struct ResizeOverlay {
        /// PORTS: `resize_overlay.zig:129` the `label` template child (the
        /// `gtk.Label` with the text).
        #[template_child]
        pub label: TemplateChild<gtk4::Label>,

        /// PORTS: `resize_overlay.zig:78-91,132` the `label`/`label_text`
        /// property + private field — the text set on the label when scheduled.
        #[property(get, set, nullable)]
        pub label_text: RefCell<Option<String>>,

        /// PORTS: `resize_overlay.zig:37-54,135` the `duration` property /
        /// private field — how long the overlay stays visible (ms).
        #[property(get, set, minimum = 250, default = 750)]
        pub duration: Cell<u32>,

        /// PORTS: `resize_overlay.zig:56-76,139` the `first-delay` property /
        /// private field — the first delay before any overlay is shown (ms).
        /// Must be specified during construction otherwise it has no effect.
        #[property(get, set, name = "first-delay", minimum = 250, default = 250)]
        pub first_delay: Cell<u32>,

        /// PORTS: `resize_overlay.zig:93-108,150` the `overlay-halign` property /
        /// private field — the horizontal alignment of the label.
        #[property(get, set, name = "overlay-halign", builder(gtk4::Align::Center))]
        pub halign: Cell<gtk4::Align>,

        /// PORTS: `resize_overlay.zig:110-125,151` the `overlay-valign` property /
        /// private field — the vertical alignment of the label.
        #[property(get, set, name = "overlay-valign", builder(gtk4::Align::Center))]
        pub valign: Cell<gtk4::Align>,

        /// PORTS: `resize_overlay.zig:142` the `idler` source id — the idle
        /// source that updates the label (**invariant 11**).
        pub idler: RefCell<Option<glib::SourceId>>,

        /// PORTS: `resize_overlay.zig:145` the `timer` source id — the timer for
        /// dismissing the overlay (**invariant 11**).
        pub timer: RefCell<Option<glib::SourceId>>,

        /// PORTS: `resize_overlay.zig:148` the `delay_timer` source id — the
        /// first-delay timer (**invariant 11**).
        pub delay_timer: RefCell<Option<glib::SourceId>>,
    }

    impl Default for ResizeOverlay {
        /// gtk4-rs needs a `Default` for `object_subclass`; `gtk.Align` has no
        /// `Default` so it cannot be derived. The numeric/align defaults mirror
        /// the reference pspec defaults (`resize_overlay.zig:41,60,98,115`), but
        /// the template (`resize-overlay.blp:12-15`) overrides them all at load.
        fn default() -> Self {
            Self {
                label: TemplateChild::default(),
                label_text: RefCell::new(None),
                duration: Cell::new(750),
                first_delay: Cell::new(250),
                halign: Cell::new(gtk4::Align::Center),
                valign: Cell::new(gtk4::Align::Center),
                idler: RefCell::new(None),
                timer: RefCell::new(None),
                delay_timer: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ResizeOverlay {
        // PORTS: `resize_overlay.zig:29` `.name = "GhosttyResizeOverlay"`.
        // Preserved 1:1 so the surface template's `$GhosttyResizeOverlay` resolves.
        const NAME: &'static str = "GhosttyResizeOverlay";
        type Type = super::ResizeOverlay;
        // PORTS: `resize_overlay.zig:27` `pub const Parent = adw.Bin`.
        type ParentType = libadwaita::Bin;

        /// PORTS: `resize_overlay.zig:326-360` `Class.init` — load the template
        /// and bind the `label` child.
        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        /// PORTS: `resize_overlay.zig:157-158` `init` — `gtk.Widget.initTemplate`.
        /// (The `first_delay` timer setup, `resize_overlay.zig:159-167`, runs in
        /// [`ObjectImpl::constructed`] once the template has applied its props.)
        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ResizeOverlay {
        /// PORTS: `resize_overlay.zig:155-168` `init` — after the template is
        /// applied (so `first-delay` has its blp value), schedule the first-delay
        /// timer if it is non-zero (`:160-166`). We do this in `constructed`
        /// rather than `instance_init` because gtk4-rs applies the template's root
        /// properties during `init_template` (in `instance_init`), so `first_delay`
        /// is only settled by the time `constructed` runs.
        fn constructed(&self) {
            self.parent_constructed();

            let first_delay = self.first_delay.get();
            if first_delay > 0 {
                let id = glib::timeout_add_local(
                    Duration::from_millis(u64::from(first_delay)),
                    glib::clone!(
                        #[weak(rename_to = this)]
                        self,
                        #[upgrade_or]
                        glib::ControlFlow::Break,
                        move || this.obj().on_delay_timer()
                    ),
                );
                self.delay_timer.replace(Some(id));
            }
        }

        /// PORTS: `resize_overlay.zig:270-311` `dispose` — remove the three glib
        /// source ids (warn-on-failure, **invariant 11**), then dispose the
        /// template. The parent `dispose` chains automatically in gtk4-rs. Every
        /// field is `take()`n so `dispose` is idempotent.
        fn dispose(&self) {
            remove_source_id(self.idler.take(), "resize overlay idler");
            remove_source_id(self.timer.take(), "resize overlay timer");
            remove_source_id(self.delay_timer.take(), "resize overlay delay timer");

            // `gtk.Widget.disposeTemplate(self, getGObjectType())` (`:307-310`).
            // The `label` child is nested (not the Bin's root child), so no
            // double-free workaround is needed (cf. `surface.rs` invariant 6).
            self.dispose_template();
        }
    }

    impl WidgetImpl for ResizeOverlay {}
    impl BinImpl for ResizeOverlay {}
}

glib::wrapper! {
    /// PORTS: `resize_overlay.zig:24` `pub const ResizeOverlay`.
    pub struct ResizeOverlay(ObjectSubclass<imp::ResizeOverlay>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ResizeOverlay {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ResizeOverlay {
    /// PORTS: `resize_overlay.zig:170-178` `setLabel`. Sets the text used the
    /// next time the overlay is shown. This does NOT show the overlay if it is
    /// currently hidden — the caller must also call [`Self::schedule`]. The
    /// generated `set_label_text` setter dupes the string and notifies the
    /// `label` pspec (`:177`).
    pub fn set_label(&self, label: Option<&str>) {
        self.set_label_text(label.map(std::string::ToString::to_string));
    }

    /// PORTS: `resize_overlay.zig:180-193` `schedule`. Schedule the overlay to be
    /// shown on the next idle tick (to avoid flickering during resizes).
    pub fn schedule(&self) {
        let imp = self.imp();

        // If we have a delay timer then we're not showing anything yet, so do
        // nothing (`:185-186`).
        if imp.delay_timer.borrow().is_some() {
            return;
        }

        // When updating a widget, wait until GTK is "idle" — since we are called
        // in the middle of resizing, doing it now results in warnings and horrible
        // flickering (`:188-192`).
        if imp.idler.borrow().is_some() {
            return;
        }
        let id = glib::idle_add_local(glib::clone!(
            #[weak(rename_to = this)]
            self,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move || this.on_idle()
        ));
        imp.idler.replace(Some(id));
    }

    /// PORTS: `resize_overlay.zig:195-241` `onIdle`.
    fn on_idle(&self) -> glib::ControlFlow {
        let imp = self.imp();

        // No matter what our idler is complete with this callback (`:200-201`).
        imp.idler.replace(None);

        // Cancel our previous show timer no matter what (`:203-209`).
        remove_source_id(imp.timer.take(), "size overlay timer");

        // If we have a label to show, show ourselves; if we don't have label
        // text, then hide our label (`:211-217`).
        let Some(text) = self.label_text() else {
            self.set_visible(false);
            return glib::ControlFlow::Break;
        };

        // Set our label and show it (`:219-221`).
        imp.label.set_label(&text);
        self.set_visible(true);

        // Setup the new timer to hide ourselves after the delay (`:223-228`).
        let id = glib::timeout_add_local(
            Duration::from_millis(u64::from(self.duration())),
            glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || this.on_timer()
            ),
        );
        imp.timer.replace(Some(id));

        glib::ControlFlow::Break
    }

    /// PORTS: `resize_overlay.zig:243-249` `onTimer`.
    fn on_timer(&self) -> glib::ControlFlow {
        self.imp().timer.replace(None);
        self.set_visible(false);
        glib::ControlFlow::Break
    }

    /// PORTS: `resize_overlay.zig:251-256` `onDelayTimer`.
    fn on_delay_timer(&self) -> glib::ControlFlow {
        self.imp().delay_timer.replace(None);
        glib::ControlFlow::Break
    }
}
