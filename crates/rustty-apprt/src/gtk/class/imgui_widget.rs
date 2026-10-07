//! rustty-apprt::gtk::class::imgui_widget
//!
//! PORTS: ghostty `src/apprt/gtk/class/imgui_widget.zig` (551 loc) — a widget
//! for embedding a Dear ImGui application, rendered through the OpenGL3 ImGui
//! backend on top of a `GtkGLArea`.
//!
//! ## The virtual seam (`imgui_widget.zig:19-50,477-503`)
//!
//! The reference comments that "It'd be a lot cleaner to use inheritance here
//! but zig-gobject doesn't currently have a way to define virtual methods, so we
//! have to use composition and signals instead" and hand-rolls a class vtable
//! (`setup`/`render` fn pointers on `Class`, dispatched through
//! `getClass()` in the public wrappers). gtk4-rs *does* have real subclassable
//! virtual methods, so this is the single biggest structural simplification vs
//! the oracle: the two vfuncs become a genuine subclassable seam. A subclass
//! (e.g. `InspectorWidget`) implements [`ImguiWidgetImpl`], overriding
//! [`ImguiWidgetImpl::imgui_setup`] / [`ImguiWidgetImpl::imgui_render`]; the base
//! class calls them through the class vtable ([`ImguiWidgetClass`]) exactly where
//! the reference calls `self.setup()` / `self.render()`. The default vfuncs log a
//! "this is a bug" warning, matching `defaultSetup`/`defaultRender`
//! (`imgui_widget.zig:480-488`).
//!
//! ## Scale handling (`imgui_widget.zig:275-293`)
//!
//! `io.DisplaySize` is kept UNSCALED and `io.DisplayFramebufferScale = (1,1)`;
//! the display scale is applied by rebuilding `ImGuiStyle` (via the ext
//! constructor, for proper non-zero defaults) and `ScaleAllSizes(scale_factor)`.
//! This must match the reference exactly or fonts blur (INV-11 scale note).

use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::os::raw::c_int;
use std::ptr;
use std::time::Instant;

use glib::subclass::prelude::*;
use glib::subclass::types::{ClassStruct, IsSubclassable};
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::CompositeTemplate;
use libadwaita::subclass::prelude::*;

use dcimgui_sys as cimgui;
use rustty_vt::input::key::Action;

use crate::gtk::key;

/// The scoped log name — `imgui_widget.zig:18`
/// `std.log.scoped(.gtk_ghostty_imgui_widget)`, R2-renamed.
const LOG_SCOPE: &str = "gtk_rustty_imgui_widget";

/// `io.BackendPlatformName` (`imgui_widget.zig:232`). R2 product-identity rename
/// of the reference's `"ghostty_gtk"`.
const BACKEND_PLATFORM_NAME: &CStr = c"rustty_gtk";

/// PORTS: the reference `setCurrentContext`'s error set,
/// `error{ContextNotInitialized}` (`imgui_widget.zig:129`) — a named tag, not a
/// bare failure, so it is a named type here too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextNotInitialized;

mod imp {
    use super::*;

    /// The class struct for [`ImguiWidget`](super::ImguiWidget), carrying the two
    /// subclassable virtual methods. PORTS: `imgui_widget.zig:498-506`
    /// `ImguiWidget.Class` (the `setup`/`render` fn-pointer slots), re-expressed
    /// as a real gtk4-rs custom class struct so subclasses override the vfuncs
    /// through the `*Impl` trait rather than the reference's hand-rolled vtable.
    #[repr(C)]
    pub struct ImguiWidgetClass {
        pub parent_class: libadwaita::ffi::AdwBinClass,
        /// Function pointers for virtual methods.
        ///
        /// PORTS: `imgui_widget.zig:502` `setup: ?*const fn (*Self) …`.
        pub imgui_setup: fn(&super::ImguiWidget),
        /// PORTS: `imgui_widget.zig:503` `render: ?*const fn (*Self) …`.
        pub imgui_render: fn(&super::ImguiWidget),
    }

    unsafe impl ClassStruct for ImguiWidgetClass {
        type Type = ImguiWidget;
    }

    /// PORTS: `imgui_widget.zig:52-73` `ImguiWidget.Private`.
    #[derive(CompositeTemplate)]
    #[template(resource = "/io/airely/rustty/ui/1.5/imgui-widget.ui")]
    pub struct ImguiWidget {
        /// GL area where we display the Dear ImGui application.
        ///
        /// PORTS: `imgui_widget.zig:53-54` `gl_area`.
        #[template_child]
        pub gl_area: TemplateChild<gtk4::GLArea>,

        /// GTK input method context
        ///
        /// PORTS: `imgui_widget.zig:56-57` `im_context`.
        #[template_child]
        pub im_context: TemplateChild<gtk4::IMMulticontext>,

        /// Dear ImGui context. We create a context per widget so that we can
        /// have multiple active imgui views in the same application.
        ///
        /// PORTS: `imgui_widget.zig:59-61` `ig_context`. Null until `realize`.
        pub ig_context: Cell<*mut cimgui::ImGuiContext>,

        /// Our previous instant used to calculate delta time for animations.
        ///
        /// PORTS: `imgui_widget.zig:63-64` `instant`.
        pub instant: Cell<Option<Instant>>,

        /// Tick callback ID for timed updates.
        ///
        /// PORTS: `imgui_widget.zig:66-67` `tick_callback_id`.
        pub tick_callback_id: Cell<Option<gtk4::TickCallbackId>>,

        /// Last render time for throttling to 30 FPS.
        ///
        /// PORTS: `imgui_widget.zig:69-70` `last_render_time`.
        pub last_render_time: Cell<Option<Instant>>,
    }

    impl Default for ImguiWidget {
        fn default() -> Self {
            Self {
                gl_area: TemplateChild::default(),
                im_context: TemplateChild::default(),
                ig_context: Cell::new(ptr::null_mut()),
                instant: Cell::new(None),
                tick_callback_id: Cell::new(None),
                last_render_time: Cell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ImguiWidget {
        // PORTS: `imgui_widget.zig:29` `.name = "GhosttyImguiWidget"`. The GType
        // name must match the vendored `imgui-widget.blp` template class
        // (`$GhosttyImguiWidget`), so it is preserved verbatim.
        const NAME: &'static str = "GhosttyImguiWidget";
        type Type = super::ImguiWidget;
        type ParentType = libadwaita::Bin;
        // Custom class struct carrying the two virtual methods.
        type Class = ImguiWidgetClass;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            super::ImguiWidget::bind_template_callbacks(klass);

            // Initialize our virtual methods with default functions
            // (`imgui_widget.zig:519-520`).
            klass.imgui_setup = default_imgui_setup;
            klass.imgui_render = default_imgui_render;
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            // PORTS: `imgui_widget.zig:78-80` `init` (`initTemplate`).
            obj.init_template();
        }
    }

    impl ObjectImpl for ImguiWidget {
        /// PORTS: `imgui_widget.zig:82-92` `dispose` (`disposeTemplate` +
        /// chain-up).
        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for ImguiWidget {}
    impl BinImpl for ImguiWidget {}

    /// Default setup function. Does nothing but log a warning.
    ///
    /// PORTS: `imgui_widget.zig:480-483` `defaultSetup`.
    fn default_imgui_setup(_this: &super::ImguiWidget) {
        log::warn!(target: LOG_SCOPE, "default Dear ImGui setup called, this is a bug.");
    }

    /// Default render function. Does nothing but log a warning.
    ///
    /// PORTS: `imgui_widget.zig:485-488` `defaultRender`.
    fn default_imgui_render(_this: &super::ImguiWidget) {
        log::warn!(target: LOG_SCOPE, "default Dear ImGui render called, this is a bug.");
    }
}

glib::wrapper! {
    /// A widget for embedding a Dear ImGui application.
    ///
    /// It'd be a lot cleaner to use inheritance here but zig-gobject doesn't
    /// currently have a way to define virtual methods, so we have to use
    /// composition and signals instead.
    ///
    /// PORTS: `imgui_widget.zig:24` `pub const ImguiWidget` (see the module
    /// doc: gtk4-rs has real virtual methods, so the port uses them).
    pub struct ImguiWidget(ObjectSubclass<imp::ImguiWidget>)
        @extends libadwaita::Bin, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for ImguiWidget {
    fn default() -> Self {
        glib::Object::new()
    }
}

// ---------------------------------------------------------------------------
// The subclassable virtual seam (`imgui_widget.zig:40-50,105-121,498-506`).

/// The subclass contract for [`ImguiWidget`]. A subclass overrides
/// [`imgui_setup`](Self::imgui_setup) / [`imgui_render`](Self::imgui_render); the
/// default impls chain up to the parent vfunc (ultimately the base-class
/// warning). PORTS: `imgui_widget.zig:40-50` `virtual_methods`.
pub trait ImguiWidgetImpl: BinImpl {
    /// This virtual method will be called to allow the Dear ImGui
    /// application to do one-time setup of the context. The correct context
    /// will be current when the virtual method is called.
    ///
    /// PORTS: `imgui_widget.zig:41-44`.
    fn imgui_setup(&self) {
        self.parent_imgui_setup();
    }

    /// This virtual method will be called at each frame to allow the Dear
    /// ImGui application to draw the application. The correct context will
    /// be current when the virtual method is called.
    ///
    /// PORTS: `imgui_widget.zig:46-49`.
    fn imgui_render(&self) {
        self.parent_imgui_render();
    }
}

/// Parent-vfunc chain-up for [`ImguiWidgetImpl`] subclasses — the gtk4-rs
/// equivalent of the reference's `virtual_methods.setup.call(Class.parent, …)`.
pub trait ImguiWidgetImplExt: ObjectSubclass {
    fn parent_imgui_setup(&self);
    fn parent_imgui_render(&self);
}

impl<T: ImguiWidgetImpl> ImguiWidgetImplExt for T {
    fn parent_imgui_setup(&self) {
        unsafe {
            let data = Self::type_data();
            let parent_class = data.as_ref().parent_class() as *const imp::ImguiWidgetClass;
            ((*parent_class).imgui_setup)(self.obj().unsafe_cast_ref::<ImguiWidget>());
        }
    }

    fn parent_imgui_render(&self) {
        unsafe {
            let data = Self::type_data();
            let parent_class = data.as_ref().parent_class() as *const imp::ImguiWidgetClass;
            ((*parent_class).imgui_render)(self.obj().unsafe_cast_ref::<ImguiWidget>());
        }
    }
}

unsafe impl<T: ImguiWidgetImpl> IsSubclassable<T> for ImguiWidget {
    fn class_init(class: &mut glib::Class<Self>) {
        Self::parent_class_init::<T>(class);

        let klass = class.as_mut();
        klass.imgui_setup = imgui_setup_trampoline::<T>;
        klass.imgui_render = imgui_render_trampoline::<T>;
    }
}

fn imgui_setup_trampoline<T: ImguiWidgetImpl>(this: &ImguiWidget) {
    // SAFETY: the trampoline is only installed for `T`, whose instances ARE
    // `ImguiWidget`s; reinterpret the base ref as the subclass to reach its imp.
    let imp = unsafe { this.unsafe_cast_ref::<T::Type>() }.imp();
    ImguiWidgetImpl::imgui_setup(imp);
}

fn imgui_render_trampoline<T: ImguiWidgetImpl>(this: &ImguiWidget) {
    // SAFETY: as above.
    let imp = unsafe { this.unsafe_cast_ref::<T::Type>() }.imp();
    ImguiWidgetImpl::imgui_render(imp);
}

// ---------------------------------------------------------------------------
// Public methods + private helpers.

impl ImguiWidget {
    /// This should be called anytime the underlying data for the UI changes
    /// so that the UI can be refreshed.
    ///
    /// PORTS: `imgui_widget.zig:97-102` `queueRender`.
    pub fn queue_render(&self) {
        self.imp().gl_area.queue_render();
    }

    // Public wrappers for virtual methods

    /// This virtual method will be called to allow the Dear ImGui application
    /// to do one-time setup of the context. The correct context will be current
    /// when the virtual method is called.
    ///
    /// Call the `setup` virtual method through the class vtable. PORTS:
    /// `imgui_widget.zig:107-113` `setup` (the public wrapper).
    pub fn imgui_setup(&self) {
        let klass = self.class();
        (klass.as_ref().imgui_setup)(self);
    }

    /// This virtual method will be called at each frame to allow the Dear ImGui
    /// application to draw the application. The correct context will be current
    /// when the virtual method is called.
    ///
    /// Call the `render` virtual method through the class vtable. PORTS:
    /// `imgui_widget.zig:115-121` `render` (the public wrapper).
    pub fn imgui_render(&self) {
        let klass = self.class();
        (klass.as_ref().imgui_render)(self);
    }

    /// Set our imgui context to be current, or return an error. This must be
    /// called before any Dear ImGui API calls so that they're made against the
    /// proper context. PORTS: `imgui_widget.zig:126-136` `setCurrentContext`.
    pub fn set_current_context(&self) -> Result<(), ContextNotInitialized> {
        let ig_context = self.imp().ig_context.get();
        if ig_context.is_null() {
            log::warn!(target: LOG_SCOPE, "Dear ImGui context not initialized");
            return Err(ContextNotInitialized);
        }
        // SAFETY: non-null context handle owned by this widget.
        unsafe { cimgui::ImGui_SetCurrentContext(ig_context) };
        Ok(())
    }

    /// Initialize the frame. Expects that the context is already current.
    /// PORTS: `imgui_widget.zig:138-153` `newFrame`.
    fn new_frame(&self) {
        // SAFETY: the context is current (asserted by the caller).
        let io = unsafe { cimgui::ImGui_GetIO() };

        // Determine our delta time (`:143-152`).
        let now = Instant::now();
        let delta_time: f32 = if let Some(prev) = self.imp().instant.get() {
            let since_s = now.duration_since(prev).as_secs_f32();
            since_s.max(0.00001)
        } else {
            1.0 / 60.0
        };
        // SAFETY: `io` is the current context's non-null IO struct.
        unsafe { (*io).DeltaTime = delta_time };

        self.imp().instant.set(Some(now));
    }

    /// Get the scale factor that the display is operating at. PORTS:
    /// `imgui_widget.zig:203-206` `getScaleFactor`.
    fn scale_factor_f64(&self) -> f64 {
        f64::from(self.imp().gl_area.scale_factor())
    }

    /// Handle key press/release events. PORTS: `imgui_widget.zig:155-190`
    /// `keyEvent`.
    fn key_event(
        &self,
        action: Action,
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        _keycode: u32,
        gtk_mods: gdk4::ModifierType,
    ) -> bool {
        self.queue_render();

        if self.set_current_context().is_err() {
            return false;
        }

        // SAFETY: the context is now current.
        let io = unsafe { cimgui::ImGui_GetIO() };

        let mods = key::translate_mods(gtk_mods);
        // SAFETY: `io` non-null; `ImGuiKey_*` are valid key enum values.
        unsafe {
            cimgui::ImGuiIO_AddKeyEvent(
                io,
                cimgui::ImGuiKey_LeftShift as cimgui::ImGuiKey,
                mods.shift,
            );
            cimgui::ImGuiIO_AddKeyEvent(
                io,
                cimgui::ImGuiKey_LeftCtrl as cimgui::ImGuiKey,
                mods.ctrl,
            );
            cimgui::ImGuiIO_AddKeyEvent(io, cimgui::ImGuiKey_LeftAlt as cimgui::ImGuiKey, mods.alt);
            cimgui::ImGuiIO_AddKeyEvent(
                io,
                cimgui::ImGuiKey_LeftSuper as cimgui::ImGuiKey,
                mods.super_,
            );
        }

        // If our keyval has a key, then we send that key event (`:176-181`).
        if let Some(inputkey) = key::key_from_keyval(keyval) {
            if let Some(imgui_key) = inputkey.imgui_key() {
                // SAFETY: `io` non-null; `imgui_key` is a valid `ImGuiKey`.
                unsafe {
                    cimgui::ImGuiIO_AddKeyEvent(io, imgui_key, action == Action::Press);
                }
            }
        }

        // Try to process the event as text (`:183-187`).
        if let Some(event) = ec_key.current_event() {
            let _ = self.imp().im_context.filter_keypress(&event);
        }

        true
    }
}

/// Translate a GTK mouse button to a Dear ImGui mouse button. PORTS:
/// `imgui_widget.zig:192-200` `translateMouseButton`.
fn translate_mouse_button(button: u32) -> Option<c_int> {
    match button {
        1 => Some(cimgui::ImGuiMouseButton__ImGuiMouseButton_Left as c_int),
        2 => Some(cimgui::ImGuiMouseButton__ImGuiMouseButton_Middle as c_int),
        3 => Some(cimgui::ImGuiMouseButton__ImGuiMouseButton_Right as c_int),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Signal handlers (the `imgui-widget.blp` template callbacks). None are
// `swapped`, so — per the gtk4-rs composite-template convention — the emitting
// widget is the FIRST explicit parameter and the template instance (`this`) is
// LAST (sourced from the instance-callback user data), matching how the
// non-swapped `Surface` controller callbacks are ported.

impl ImguiWidget {
    /// PORTS: `imgui_widget.zig` `Class.init` — the `bindTemplateCallback` calls of `Class.init`,
    /// bound through GTK's C builder scope (NEW-FINDINGS N516; see
    /// [`template_callback`](crate::gtk::class::template_callback)).
    fn bind_template_callbacks(klass: &mut <imp::ImguiWidget as ObjectSubclass>::Class) {
        crate::gtk::class::template_callback::bind_template_callbacks!(klass;
            c"realize" => ImguiWidget::realize, (area: obj(gtk4::GLArea), this: obj(ImguiWidget));
            c"unrealize" => ImguiWidget::unrealize, (area: obj(gtk4::GLArea), this: obj(ImguiWidget));
            c"resize" => ImguiWidget::resize,
                (area: obj(gtk4::GLArea), width: prim(i32), height: prim(i32), this: obj(ImguiWidget));
            c"render" => ImguiWidget::render,
                (area: obj(gtk4::GLArea), context: obj(gdk4::GLContext), this: obj(ImguiWidget)) -> glib(bool);
            c"focus_enter" => ImguiWidget::focus_enter, (ec: obj(gtk4::EventControllerFocus), this: obj(ImguiWidget));
            c"focus_leave" => ImguiWidget::focus_leave, (ec: obj(gtk4::EventControllerFocus), this: obj(ImguiWidget));
            c"key_pressed" => ImguiWidget::key_pressed, (
                ec: obj(gtk4::EventControllerKey), keyval: prim(u32), keycode: prim(u32),
                mods: glib(gdk4::ModifierType), this: obj(ImguiWidget),
            ) -> glib(bool);
            c"key_released" => ImguiWidget::key_released, (
                ec: obj(gtk4::EventControllerKey), keyval: prim(u32), keycode: prim(u32),
                mods: glib(gdk4::ModifierType), this: obj(ImguiWidget),
            );
            c"mouse_pressed" => ImguiWidget::mouse_pressed, (
                gesture: obj(gtk4::GestureClick), n_press: prim(i32), x: prim(f64), y: prim(f64),
                this: obj(ImguiWidget),
            );
            c"mouse_released" => ImguiWidget::mouse_released, (
                gesture: obj(gtk4::GestureClick), n_press: prim(i32), x: prim(f64), y: prim(f64),
                this: obj(ImguiWidget),
            );
            c"mouse_motion" => ImguiWidget::mouse_motion,
                (ec: obj(gtk4::EventControllerMotion), x: prim(f64), y: prim(f64), this: obj(ImguiWidget));
            c"scroll" => ImguiWidget::scroll,
                (ec: obj(gtk4::EventControllerScroll), x: prim(f64), y: prim(f64), this: obj(ImguiWidget))
                -> glib(bool);
            c"im_commit" => ImguiWidget::im_commit,
                (ctx: obj(gtk4::IMMulticontext), bytes: bytes(), this: obj(ImguiWidget));
        );
    }
}

impl ImguiWidget {
    /// PORTS: `imgui_widget.zig:214-247` `glAreaRealize`.
    fn realize(_area: &gtk4::GLArea, this: &ImguiWidget) {
        let imp = this.imp();
        debug_assert!(imp.ig_context.get().is_null());

        imp.gl_area.make_current();
        if let Some(err) = imp.gl_area.error() {
            log::warn!(target: LOG_SCOPE, "GLArea for Dear ImGui widget failed to realize: {err}");
            return;
        }

        // SAFETY: the GL context is current; create the per-widget ImGui context.
        let ig_context = unsafe { cimgui::ImGui_CreateContext(ptr::null_mut()) };
        if ig_context.is_null() {
            log::warn!(target: LOG_SCOPE, "unable to initialize Dear ImGui context");
            return;
        }
        imp.ig_context.set(ig_context);
        if this.set_current_context().is_err() {
            return;
        }

        // Setup some basic config (`:230-232`).
        // SAFETY: context current; `io` non-null; the name is a `'static` CStr.
        let io = unsafe { cimgui::ImGui_GetIO() };
        unsafe { (*io).BackendPlatformName = BACKEND_PLATFORM_NAME.as_ptr() };

        // Realize means that our OpenGL context is ready, so we can now
        // initialize the ImgUI OpenGL backend for our context.
        // GTK may use GLES for its shared context. ImGui's default shader
        // version is for desktop GL, so select GLSL ES explicitly in that case.
        // (`:236-250`, `a93a8b03b`.)
        //
        // const context = priv.gl_area.getContext().?;
        let context = imp
            .gl_area
            .context()
            .expect("a realized GLArea without an error has a GL context");
        let glsl_version: Option<&CStr> = if context.uses_es() {
            Some(c"#version 300 es")
        } else {
            None
        };
        // SAFETY: OpenGL context current; `glsl_version` is null (the backend's
        // default) or a NUL-terminated `'static` string.
        let initialized = unsafe {
            cimgui::ImGui_ImplOpenGL3_Init(glsl_version.map_or(ptr::null(), CStr::as_ptr))
        };
        if !initialized {
            log::warn!(target: LOG_SCOPE, "unable to initialize Dear ImGui OpenGL backend");
            // SAFETY: `ig_context` is the context created above and not yet
            // handed to anything else.
            unsafe { cimgui::ImGui_DestroyContext(ig_context) };
            imp.ig_context.set(ptr::null_mut());
            return;
        }

        // Call the virtual method to setup the UI (`:238-239`).
        this.imgui_setup();

        // Add a tick callback to drive timed updates via the frame clock
        // (`:241-246`).
        let id = this.add_tick_callback(Self::tick_callback);
        imp.tick_callback_id.set(Some(id));
    }

    /// Handle a request to unrealize the GLArea. PORTS:
    /// `imgui_widget.zig:250-272` `glAreaUnrealize`.
    fn unrealize(_area: &gtk4::GLArea, this: &ImguiWidget) {
        let imp = this.imp();
        debug_assert!(!imp.ig_context.get().is_null());

        // Remove the tick callback if it was registered (`:254-258`).
        if let Some(id) = imp.tick_callback_id.take() {
            id.remove();
        }

        // Unrealize is not guaranteed to be called with a current GL context,
        // so we make it current for ImGui cleanup (`:260-266`).
        imp.gl_area.make_current();
        if let Some(err) = imp.gl_area.error() {
            log::warn!(target: LOG_SCOPE, "GLArea for Dear ImGui widget failed to realize: {err}");
            return;
        }

        if this.set_current_context().is_err() {
            return;
        }
        // SAFETY: context current; shut down the GL backend + destroy the context.
        unsafe {
            cimgui::ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup();
            cimgui::ImGui_DestroyContext(imp.ig_context.get());
        }
        imp.ig_context.set(ptr::null_mut());
    }

    /// Handle a request to resize the GLArea. PORTS: `imgui_widget.zig:275-293`
    /// `glAreaResize`.
    fn resize(area: &gtk4::GLArea, width: i32, height: i32, this: &ImguiWidget) {
        if this.set_current_context().is_err() {
            return;
        }
        // SAFETY: context current.
        let io = unsafe { cimgui::ImGui_GetIO() };
        let scale_factor = area.scale_factor();

        // Our display size is always unscaled. We'll do the scaling in the style
        // instead. This creates crisper looking fonts (`:280-283`).
        // SAFETY: `io` non-null.
        unsafe {
            (*io).DisplaySize = cimgui::ImVec2_t {
                x: width as f32,
                y: height as f32,
            };
            (*io).DisplayFramebufferScale = cimgui::ImVec2_t { x: 1.0, y: 1.0 };
        }

        // Setup a new style and scale it appropriately. We must use the
        // ImGuiStyle constructor to get proper default values (e.g.,
        // CurveTessellationTol) rather than zero-initialized values (`:285-292`).
        // SAFETY: `ImGuiStyle_ImGuiStyle` in-place default-constructs the whole
        // struct (ext.cpp), so the byte-zeroed placeholder is fully overwritten.
        unsafe {
            let mut style: cimgui::ImGuiStyle = std::mem::zeroed();
            cimgui::ImGuiStyle_ImGuiStyle(&mut style);
            cimgui::ImGuiStyle_ScaleAllSizes(&mut style, scale_factor as f32);
            let active_style = cimgui::ImGui_GetStyle();
            *active_style = style;
        }
    }

    /// Handle a request to render the contents of our GLArea. PORTS:
    /// `imgui_widget.zig:296-324` `glAreaRender`.
    ///
    /// GTK guarantees the GL context is current for the `render` signal, so —
    /// unlike realize/unrealize — we do NOT call `make_current` here (`:296`).
    fn render(_area: &gtk4::GLArea, _context: &gdk4::GLContext, this: &ImguiWidget) -> bool {
        if this.set_current_context().is_err() {
            return false;
        }

        // Update last render time for tick callback throttling (`:299-301`).
        this.imp().last_render_time.set(Some(Instant::now()));

        // Setup our frame. We render twice because some ImGui behaviors take
        // multiple renders to process. I don't know how to make this more
        // efficient (`:303-316`).
        for _ in 0..2 {
            // SAFETY: context current.
            unsafe { cimgui::ImGui_ImplOpenGL3_NewFrame() };
            this.new_frame();
            unsafe { cimgui::ImGui_NewFrame() };

            // Call the virtual method to draw the UI (`:311-312`).
            this.imgui_render();

            // Render (`:314-315`).
            unsafe { cimgui::ImGui_Render() };
        }

        // (`0fca3d34a` deleted the "OpenGL final render" `gl.clearColor(0x28,
        // 0x2C, 0x34)`/`gl.clear` that preceded this: nothing loads a GL function
        // table on the GTK main thread any more.)
        // SAFETY: context current; `ImGui_GetDrawData` returns the current
        // frame's draw data (valid until the next `NewFrame`).
        unsafe { cimgui::ImGui_ImplOpenGL3_RenderDrawData(cimgui::ImGui_GetDrawData()) };

        true
    }

    /// PORTS: `imgui_widget.zig:326-331` `ecFocusEnter`.
    fn focus_enter(_ec: &gtk4::EventControllerFocus, this: &ImguiWidget) {
        if this.set_current_context().is_err() {
            return;
        }
        // SAFETY: context current.
        let io = unsafe { cimgui::ImGui_GetIO() };
        unsafe { cimgui::ImGuiIO_AddFocusEvent(io, true) };
        this.queue_render();
    }

    /// PORTS: `imgui_widget.zig:333-338` `ecFocusLeave`.
    fn focus_leave(_ec: &gtk4::EventControllerFocus, this: &ImguiWidget) {
        if this.set_current_context().is_err() {
            return;
        }
        // SAFETY: context current.
        let io = unsafe { cimgui::ImGui_GetIO() };
        unsafe { cimgui::ImGuiIO_AddFocusEvent(io, false) };
        this.queue_render();
    }

    /// PORTS: `imgui_widget.zig:340-354` `ecKeyPressed`.
    fn key_pressed(
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        keycode: u32,
        gtk_mods: gdk4::ModifierType,
        this: &ImguiWidget,
    ) -> bool {
        this.key_event(Action::Press, ec_key, keyval, keycode, gtk_mods)
    }

    /// PORTS: `imgui_widget.zig:356-370` `ecKeyReleased`.
    fn key_released(
        ec_key: &gtk4::EventControllerKey,
        keyval: u32,
        keycode: u32,
        gtk_mods: gdk4::ModifierType,
        this: &ImguiWidget,
    ) {
        let _ = this.key_event(Action::Release, ec_key, keyval, keycode, gtk_mods);
    }

    /// PORTS: `imgui_widget.zig:372-386` `ecMousePressed`.
    fn mouse_pressed(
        gesture: &gtk4::GestureClick,
        _n_press: i32,
        _x: f64,
        _y: f64,
        this: &ImguiWidget,
    ) {
        this.queue_render();
        if this.set_current_context().is_err() {
            return;
        }
        // SAFETY: context current.
        let io = unsafe { cimgui::ImGui_GetIO() };
        let gdk_button = gesture.current_button();
        if let Some(button) = translate_mouse_button(gdk_button) {
            unsafe { cimgui::ImGuiIO_AddMouseButtonEvent(io, button, true) };
        }
    }

    /// PORTS: `imgui_widget.zig:388-402` `ecMouseReleased`.
    fn mouse_released(
        gesture: &gtk4::GestureClick,
        _n_press: i32,
        _x: f64,
        _y: f64,
        this: &ImguiWidget,
    ) {
        this.queue_render();
        if this.set_current_context().is_err() {
            return;
        }
        // SAFETY: context current.
        let io = unsafe { cimgui::ImGui_GetIO() };
        let gdk_button = gesture.current_button();
        if let Some(button) = translate_mouse_button(gdk_button) {
            unsafe { cimgui::ImGuiIO_AddMouseButtonEvent(io, button, false) };
        }
    }

    /// PORTS: `imgui_widget.zig:404-419` `ecMouseMotion`.
    fn mouse_motion(_ec: &gtk4::EventControllerMotion, x: f64, y: f64, this: &ImguiWidget) {
        this.queue_render();
        if this.set_current_context().is_err() {
            return;
        }
        // SAFETY: context current.
        let io = unsafe { cimgui::ImGui_GetIO() };
        let scale_factor = this.scale_factor_f64();
        unsafe {
            cimgui::ImGuiIO_AddMousePosEvent(
                io,
                (x * scale_factor) as f32,
                (y * scale_factor) as f32,
            );
        }
    }

    /// PORTS: `imgui_widget.zig:421-436` `ecMouseScroll`.
    fn scroll(_ec: &gtk4::EventControllerScroll, x: f64, y: f64, this: &ImguiWidget) -> bool {
        this.queue_render();
        if this.set_current_context().is_err() {
            return false;
        }
        // SAFETY: context current.
        let io = unsafe { cimgui::ImGui_GetIO() };
        unsafe { cimgui::ImGuiIO_AddMouseWheelEvent(io, x as f32, (-y) as f32) };
        true
    }

    /// PORTS: `imgui_widget.zig:438-447` `imCommit`.
    fn im_commit(_ctx: &gtk4::IMMulticontext, bytes: &[u8], this: &ImguiWidget) {
        this.queue_render();
        if this.set_current_context().is_err() {
            return;
        }
        // SAFETY: context current.
        let io = unsafe { cimgui::ImGui_GetIO() };
        // `AddInputCharactersUTF8` takes a NUL-terminated UTF-8 string; the GTK
        // commit payload never contains an interior NUL, so a failed `CString`
        // conversion is simply dropped.
        if let Ok(cstr) = CString::new(bytes) {
            unsafe { cimgui::ImGuiIO_AddInputCharactersUTF8(io, cstr.as_ptr()) };
        }
    }

    /// Tick callback for timed updates. This drives periodic redraws.
    /// Redraws are limited to 30 FPS max since our imgui widgets don't
    /// usually need higher frame rates than that.
    ///
    /// PORTS: `imgui_widget.zig:449-475` `tickCallback`.
    fn tick_callback(widget: &ImguiWidget, _clock: &gdk4::FrameClock) -> glib::ControlFlow {
        let now = Instant::now();

        // Throttle to 30 FPS (~33ms between frames) (`:465-472`).
        let frame_time = std::time::Duration::from_nanos(
            std::time::Duration::from_secs(1).as_nanos() as u64 / 30,
        );
        let should_render = match widget.imp().last_render_time.get() {
            Some(last) => now.duration_since(last) >= frame_time,
            None => true,
        };

        if should_render {
            widget.queue_render();
        }

        // Continue the tick callback (`:474`).
        glib::ControlFlow::Continue
    }
}
