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

/// `const log = std.log.scoped(.gtk_ghostty_resize_overlay)` (`resize_overlay.zig:10`).
///
/// REBRANDED per the precedent task-14d-2 set at `gtk/class/config.rs` and now
/// recorded in `.port/tools/PARITY_NAMING.md`: a log scope tag is a runtime
/// identity string emitted into user-visible diagnostic text. The GType
/// registered NAME in this file is deliberately NOT rebranded.
const LOG_SCOPE: &str = "gtk_rustty_resize_overlay";

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
            // log.warn("unable to remove {s}", .{what});
            //
            // task-14d-3: the `source={}` suffix is gone — no pinned reference
            // site prints it (see this file's `unable to remove ...` sites);
            // `what` carries the reference's own wording, so each caller
            // reproduces its own reference message verbatim.
            log::warn!(target: LOG_SCOPE, "unable to remove {what}");
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

        /// The text to set on the label when scheduled.
        ///
        /// PORTS: `resize_overlay.zig:77-91,132` the `label` property, backed by
        /// the `label_text` private field.
        ///
        /// CORRECTION (task 20, S52-F1): the attribute was `#[property(get, set,
        /// nullable)]`, so glib-macros registered the property under the FIELD's
        /// name, `label-text`. The reference registers it as `label`
        /// (`pub const name = "label"`, `:79`), and `setLabel` notifies that pspec.
        /// The field keeps its name (the `label` template child owns the other).
        #[property(name = "label", get, set, nullable)]
        pub label_text: RefCell<Option<String>>,

        /// The time that the overlay appears.
        ///
        /// PORTS: `resize_overlay.zig:37-54,135` the `duration` property /
        /// private field (ms).
        #[property(get, set, minimum = 250, default = 750)]
        pub duration: Cell<u32>,

        /// The first delay before any overlay is shown. Must be specified
        /// during construction otherwise it has no effect.
        ///
        /// PORTS: `resize_overlay.zig:56-76,139` the `first-delay` property /
        /// private field (ms).
        #[property(get, set, name = "first-delay", minimum = 250, default = 250)]
        pub first_delay: Cell<u32>,

        /// The alignment of the label
        ///
        /// PORTS: `resize_overlay.zig:93-108,150` the `overlay-halign` property /
        /// private field — the horizontal alignment of the label.
        #[property(get, set, name = "overlay-halign", builder(gtk4::Align::Center))]
        pub halign: Cell<gtk4::Align>,

        /// PORTS: `resize_overlay.zig:110-125,151` the `overlay-valign` property /
        /// private field — the vertical alignment of the label.
        #[property(get, set, name = "overlay-valign", builder(gtk4::Align::Center))]
        pub valign: Cell<gtk4::Align>,

        /// The idle source that we use to update the label.
        ///
        /// PORTS: `resize_overlay.zig:142` the `idler` source id (**invariant 11**).
        pub idler: RefCell<Option<glib::SourceId>>,

        /// The timer for dismissing the overlay.
        ///
        /// PORTS: `resize_overlay.zig:145` the `timer` source id (**invariant 11**).
        pub timer: RefCell<Option<glib::SourceId>>,

        /// The first delay timer.
        ///
        /// PORTS: `resize_overlay.zig:148` the `delay_timer` source id
        /// (**invariant 11**).
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
    /// The overlay that shows the current size while a surface is resizing.
    /// This can be used generically to show pretty much anything with a
    /// disappearing overlay, but we have no other use at this point so it
    /// is named specifically for what it does.
    ///
    /// General usage:
    ///
    ///   1. Add it to an overlay
    ///   2. Set the label with `setLabel`
    ///   3. Schedule to show it with `schedule`
    ///
    /// Set any properties to change the behavior.
    ///
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
    // Set the label for the overlay. This will not show the
    // overlay if it is currently hidden; you must call schedule.
    //
    // PORTS: `resize_overlay.zig:170-178` `setLabel` is the `label` property's
    // generated `set_label` setter: it dupes the string and notifies the `label`
    // pspec (`:177`). It does NOT show the overlay if it is currently hidden — the
    // caller must also call [`Self::schedule`]. CORRECTION (task 20, S52-F1): a
    // hand-written `set_label` used to forward to the generated `set_label_text`;
    // with the property correctly named `label` the generated setter has that
    // name, so the wrapper is gone.

    /// Schedule the overlay to be shown. To avoid flickering during
    /// resizes we schedule the overlay to be shown on the next idle tick.
    ///
    /// PORTS: `resize_overlay.zig:180-193` `schedule`.
    pub fn schedule(&self) {
        let imp = self.imp();

        // If we have a delay timer then we're not showing anything
        // yet so do nothing.
        // (`:185-186`)
        if imp.delay_timer.borrow().is_some() {
            return;
        }

        // When updating a widget, wait until GTK is "idle", i.e. not in the middle
        // of doing any other updates. Since we are called in the middle of resizing
        // GTK is doing a lot of work rearranging all of the widgets. Not doing this
        // results in a lot of warnings from GTK and _horrible_ flickering of the
        // resize overlay.
        // (`:188-192`)
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

        // If we have a label to show, show ourselves. If we don't have
        // label text, then hide our label.
        // (`:211-217`)
        let Some(text) = self.label() else {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// S52-F1: the text property is registered as `label` (`resize_overlay.zig:79`)
    /// and setting it notifies `notify::label`. The notify counter is the positive
    /// half: a property that exists but is never written would leave it at 0.
    #[test]
    #[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib resize_overlay -- --ignored"]
    fn resize_overlay_text_property_is_named_label() {
        crate::gtk::init_for_display_test("resize_overlay_text_property_is_named_label");
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        let overlay = ResizeOverlay::default();
        assert!(overlay.find_property("label").is_some());
        assert!(overlay.find_property("label-text").is_none());

        let notified = std::rc::Rc::new(Cell::new(0u32));
        let counter = notified.clone();
        overlay.connect_notify_local(Some("label"), move |_, _| {
            counter.set(counter.get() + 1);
        });
        overlay.set_label(Some("80 x 24"));
        assert_eq!(notified.get(), 1);
        assert_eq!(
            overlay.property::<Option<String>>("label").as_deref(),
            Some("80 x 24")
        );
    }
}
