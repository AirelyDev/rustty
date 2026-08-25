//! rustty-apprt::gtk::class::inspector_widget
//!
//! PORTS: ghostty `src/apprt/gtk/class/inspector_widget.zig` (245 loc) — the
//! widget for displaying the terminal inspector, a subclass of
//! [`ImguiWidget`](super::imgui_widget::ImguiWidget) that overrides the two
//! Dear ImGui virtual methods (`setup`/`render`) to drive the shared
//! `rustty_inspector::Inspector`, and owns the `surface` it inspects through a
//! weak-notify (NOT a strong ref).
//!
//! ## The virtual seam (`inspector_widget.zig:236-237`)
//!
//! The reference implements `ImguiWidget.virtual_methods.setup`/`render` on the
//! class vtable; gtk4-rs re-expresses that as the real subclassable
//! [`ImguiWidgetImpl`](super::imgui_widget::ImguiWidgetImpl) trait — this widget
//! overrides [`imgui_setup`](ImguiWidgetImpl::imgui_setup) /
//! [`imgui_render`](ImguiWidgetImpl::imgui_render).
//!
//! ## The `surface` weak lifecycle (`inspector_widget.zig:47-50,124-203`)
//!
//! The surface is stored as a RAW pointer that is *not referenced* — instead a
//! GObject weak-notify is attached so that when the surface is disposed (the
//! terminal closes) we deactivate the inspector and null the field. Holding a
//! strong ref here would keep the terminal alive and break the window's
//! auto-close (`inspector_window.zig`'s `notify::surface` teardown), so this
//! faithfully mirrors the reference's non-owning pointer + `weakRef`/`weakUnref`
//! pair, expressed via [`ObjectExt::add_weak_ref_notify_local`].

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use glib::translate::{from_glib_borrow, from_glib_none};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::subclass::prelude::*;

use super::imgui_widget::{ImguiWidget, ImguiWidgetImpl};
use super::surface::{CoreSurface, Surface};

mod imp {
    use super::*;

    /// PORTS: `inspector_widget.zig:47-53` `InspectorWidget.Private`.
    #[derive(CompositeTemplate)]
    #[template(resource = "/io/airely/rustty/ui/1.5/inspector-widget.ui")]
    pub struct InspectorWidget {
        /// PORTS: `inspector_widget.zig:48-50` `surface`. The surface that we are
        /// attached to. This is NOT referenced. We attach a weak notify to the
        /// object. Stored as a raw `GObject*` (the faithful analogue of the
        /// reference's non-owning `?*Surface`), null until [`set_surface`]
        /// (super::InspectorWidget::set_surface).
        pub surface: Cell<*mut glib::gobject_ffi::GObject>,

        /// The weak-notify handle registered on `surface`, kept so that
        /// [`set_surface`](super::InspectorWidget::set_surface) can disconnect it
        /// when the surface changes (the gtk4-rs analogue of the reference's
        /// `weakUnref(surfaceWeakNotify, self)` at `inspector_widget.zig:142-145`).
        pub surface_weak: RefCell<Option<glib::object::WeakRefNotify<Surface>>>,
    }

    impl Default for InspectorWidget {
        fn default() -> Self {
            Self {
                surface: Cell::new(std::ptr::null_mut()),
                surface_weak: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InspectorWidget {
        // PORTS: `inspector_widget.zig:21` `.name = "GhosttyInspectorWidget"`. The
        // GType name must match the vendored `inspector-widget.blp` template class
        // (`$GhosttyInspectorWidget`, extending `$GhosttyImguiWidget`), so it is
        // preserved verbatim.
        const NAME: &'static str = "GhosttyInspectorWidget";
        type Type = super::InspectorWidget;
        type ParentType = ImguiWidget;

        fn class_init(klass: &mut Self::Class) {
            // PORTS: `inspector_widget.zig:218` `ensureType(ImguiWidget)` — ensure
            // the parent type is registered before binding the template that
            // references it. (gtk4-rs registers the parent via `ParentType`, but
            // the explicit ensure mirrors the reference and is harmless.)
            ImguiWidget::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            // PORTS: `inspector_widget.zig:58-60` `init` (`initTemplate`).
            obj.init_template();
        }
    }

    impl ObjectImpl for InspectorWidget {
        /// PORTS: `inspector_widget.zig:229-231` `registerProperties` — the single
        /// `surface` property (`inspector_widget.zig:28-43`).
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES
                .get_or_init(|| vec![glib::ParamSpecObject::builder::<Surface>("surface").build()])
        }

        /// PORTS: `inspector_widget.zig:106-111` `getSurfaceValue`.
        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "surface" => self.obj().get_surface().to_value(),
                _ => unimplemented!(),
            }
        }

        /// PORTS: `inspector_widget.zig:113-118` `setSurfaceValue`.
        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "surface" => {
                    let surface: Option<Surface> = value.get().ok().flatten();
                    self.obj().set_surface(surface.as_ref());
                }
                _ => unimplemented!(),
            }
        }

        /// PORTS: `inspector_widget.zig:62-75` `dispose`. Clear our surface so it
        /// deactivates the inspector, then dispose our template. The parent
        /// (`ImguiWidget`) dispose — which disposes the parent template and chains
        /// to `AdwBin`/`GtkWidget` — runs automatically after this via the gtk4-rs
        /// dispose trampoline (the reference chains up explicitly at `:71-74`).
        fn dispose(&self) {
            // Clear our surface so it deactivates the inspector (`:63-64`).
            self.obj().set_surface(None);
            self.dispose_template();
        }
    }

    impl WidgetImpl for InspectorWidget {}
    impl BinImpl for InspectorWidget {}

    impl ImguiWidgetImpl for InspectorWidget {
        /// Called to do initial setup of the UI. PORTS:
        /// `inspector_widget.zig:77-82` `imguiSetup`.
        fn imgui_setup(&self) {
            rustty_inspector::Inspector::setup();
        }

        /// Called for every frame to draw the UI. PORTS:
        /// `inspector_widget.zig:84-93` `imguiRender`.
        fn imgui_render(&self) {
            // const priv = self.private();
            // const surface = priv.surface orelse return;
            let ptr = self.surface.get();
            if ptr.is_null() {
                return;
            }
            // SAFETY: a non-null pointer is a live `GhosttySurface` — the
            // weak-notify (`surface_weak_notify`) nulls the field when the surface
            // is disposed, so it is never dangling here. Borrowed (no ref taken),
            // faithful to the reference reading the raw `?*Surface`.
            let obj = unsafe { from_glib_borrow::<_, glib::Object>(ptr) };
            let Some(surface) = obj.downcast_ref::<Surface>() else {
                return;
            };
            // const core_surface = surface.core() orelse return;
            let Some(core_surface) = surface.core() else {
                return;
            };
            // const inspector = core_surface.inspector orelse return;
            // SAFETY: `core_surface` is a live, heap-pinned `CoreSurface` for the
            // duration of this call (owned by the live surface).
            let inspector = match unsafe { (*core_surface).inspector } {
                Some(v) => v,
                None => return,
            };
            // inspector.render(core_surface);
            //
            // R4 acyclic `SurfaceData`/`wakeup` seam: `SurfaceData` replaces the
            // reference's `surface: *Surface`; every field is a plain `Copy` value
            // or raw pointer read directly off `core_surface`. Built EXACTLY as the
            // macOS Metal path does in `embedded::Inspector::renderMetal` (none of
            // these are terminal derefs — `Inspector::render`'s own `gui.draw`
            // takes `renderer_state.mutex` internally, INV-1). `wakeup` replaces
            // `surface.renderer_thread` (INV-12).
            let data = rustty_inspector::widgets::surface::SurfaceData {
                renderer_state: unsafe { &raw mut (*core_surface).renderer_state },
                alloc: allocator_api2::alloc::Global,
                size: unsafe { (*core_surface).size },
                font_size_points: unsafe { (*core_surface).font_size.points },
                font_size_pixels: unsafe { (*core_surface).font_size.pixels() },
                mouse_click_state: unsafe { (*core_surface).mouse.click_state },
                selection_gesture: unsafe { &raw const (*core_surface).mouse.selection_gesture },
                // The `ad96613a8` termio Copy/Export seam: the erased core
                // `*Surface` itself plus core's monomorphized trampolines (the
                // reference passes `surface` down into `vt_stream.draw`, which
                // reaches `rt_surface`/`rt_app` through it). Built EXACTLY as
                // the macOS Metal path does in `embedded::Inspector::renderMetal`.
                surface: core_surface.cast(),
                rt_set_clipboard: CoreSurface::inspector_rt_set_clipboard,
                rt_export_terminal_io: CoreSurface::inspector_rt_export_terminal_io,
            };
            let wakeup: &rustty_xev::Async = unsafe { &(*core_surface).renderer_thread.wakeup };
            // SAFETY: `inspector` is non-null (validated above) and owned by
            // `core_surface`'s renderer state for the duration of this call.
            unsafe { (*inspector).render(data, wakeup) };
        }
    }
}

glib::wrapper! {
    /// Widget for displaying the terminal inspector. PORTS:
    /// `inspector_widget.zig:16` `pub const InspectorWidget`.
    pub struct InspectorWidget(ObjectSubclass<imp::InspectorWidget>)
        @extends ImguiWidget, libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for InspectorWidget {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl InspectorWidget {
    /// Queue a render of the Dear ImGui widget. PORTS:
    /// `inspector_widget.zig:98-101` `queueRender` (`self.as(ImguiWidget).queueRender()`).
    pub fn queue_render(&self) {
        self.upcast_ref::<ImguiWidget>().queue_render();
    }

    /// PORTS: `inspector_widget.zig:120-122` `getSurface`. Returns a fresh strong
    /// ref (getter transfer `.none` — GTK refs for the caller); does not retain
    /// the field beyond that.
    #[must_use]
    pub fn get_surface(&self) -> Option<Surface> {
        let ptr = self.imp().surface.get();
        if ptr.is_null() {
            return None;
        }
        // SAFETY: a non-null pointer is a live `GhosttySurface`; `from_glib_none`
        // reconstructs an owned wrapper (+1 ref) that the caller owns.
        let obj = unsafe { from_glib_none::<_, glib::Object>(ptr) };
        obj.downcast::<Surface>().ok()
    }

    /// PORTS: `inspector_widget.zig:124-172` `setSurface`.
    pub fn set_surface(&self, surface_: Option<&Surface>) {
        let imp = self.imp();

        // Do nothing if we're not changing the value (`:128`).
        let cur = imp.surface.get();
        let new = surface_.map_or(std::ptr::null_mut(), |s| {
            s.upcast_ref::<glib::Object>().as_ptr()
        });
        if new == cur {
            return;
        }

        // Setup our notification to happen at the end because we're changing
        // values no matter what (`:130-134`). The `_guard` dropping on EVERY
        // return path replicates the reference's `defer thawNotify`.
        let _guard = self.freeze_notify();
        self.notify("surface");

        // Deactivate the inspector on the old surface if it exists and set our
        // value to null (`:136-150`).
        if !cur.is_null() {
            imp.surface.set(std::ptr::null_mut());

            // Remove our weak ref (`:141-145`).
            if let Some(weak) = imp.surface_weak.borrow_mut().take() {
                weak.disconnect();
            }

            // Deactivate the inspector (`:147-149`).
            // SAFETY: `cur` non-null => a live `GhosttySurface` (the weak-notify
            // would have nulled the field had it been disposed). Borrowed, no ref.
            let obj = unsafe { from_glib_borrow::<_, glib::Object>(cur) };
            if let Some(old) = obj.downcast_ref::<Surface>() {
                if let Some(core_surface) = old.core() {
                    // SAFETY: live core surface for the duration of this call.
                    unsafe { (*core_surface).deactivateInspector() };
                }
            }
        }

        // Activate the inspector on the new surface (`:152-158`).
        let Some(surface) = surface_ else {
            return;
        };
        let Some(core_surface) = surface.core() else {
            return;
        };
        // SAFETY: live core surface.
        if let Err(err) = unsafe { (*core_surface).activateInspector() } {
            eprintln!("warn(gtk): failed to activate inspector err={err:?}");
            return;
        }

        // We use a weak reference on surface to determine if the surface was
        // closed while our inspector was active (`:160-165`).
        let self_weak = self.downgrade();
        let notify = surface.add_weak_ref_notify_local(move || {
            if let Some(this) = self_weak.upgrade() {
                this.surface_weak_notify();
            }
        });
        *imp.surface_weak.borrow_mut() = Some(notify);

        // Store our surface. We don't need to ref this because we setup the weak
        // notify above (`:167-169`).
        imp.surface
            .set(surface.upcast_ref::<glib::Object>().as_ptr());

        self.queue_render();
    }

    /// PORTS: `inspector_widget.zig:177-203` `surfaceWeakNotify`. Invoked (via
    /// [`ObjectExt::add_weak_ref_notify_local`]) when the attached surface is
    /// disposed.
    fn surface_weak_notify(&self) {
        let imp = self.imp();

        // The weak notify has already fired (the object is disposing), so GObject
        // has auto-removed it; drop the now-defunct handle WITHOUT disconnecting.
        let _ = imp.surface_weak.borrow_mut().take();

        // The weak notify docs call out that we can specifically use the pointer
        // values for comparison, but the objects themselves are unsafe. Here the
        // handle is disconnected on every surface change, so only the current
        // surface's notify can reach us — a non-null field is the current surface
        // (`:184-186`).
        let ptr = imp.surface.get();
        if ptr.is_null() {
            return;
        }

        // According to weak notify docs, "surface" is in the "dispose" state. Our
        // surface doesn't clear the core surface until the "finalize" state so we
        // should be able to safely access it here. We need to be really careful
        // though (`:188-196`).
        // SAFETY: the surface is mid-dispose (ref_count still > 0), so a borrowed
        // reconstruction is sound; the core surface is valid until finalize.
        let obj = unsafe { from_glib_borrow::<_, glib::Object>(ptr) };
        let Some(old) = obj.downcast_ref::<Surface>() else {
            return;
        };
        let Some(core_surface) = old.core() else {
            return;
        };
        // SAFETY: live core surface (valid until finalize).
        unsafe { (*core_surface).deactivateInspector() };
        imp.surface.set(std::ptr::null_mut());
        self.notify("surface");

        // Note: in the future we should probably show some content on our window
        // to note that the surface went away in case our embedding widget doesn't
        // close itself. As I type this, our window closes immediately when the
        // surface goes away so you don't see this, but for completeness sake we
        // should clean this up (`:198-202`).
    }
}
