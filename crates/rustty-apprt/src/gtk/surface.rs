//! rustty-apprt::gtk::surface
//!
//! PORTS: ghostty `src/apprt/gtk/Surface.zig` (the per-surface apprt backend —
//! a thin backref wrapper around the `GhosttySurface` GObject widget, 105 loc).
//!
//! # Scope (L5.2 T17, co-landed with the T15 GLArea lifecycle)
//!
//! This is the **apprt `Surface` shim**: `impl `[`rustty_apprt_api::Surface`] for
//! the GTK backend. Every method forwards to the owning [`class::Surface`]
//! (`Surface.zig`'s `self.surface.<method>()`). The struct is a BACKREF —
//! `Surface.zig:12` `surface: *Surface` — embedded in the widget's private data
//! (`class::surface::imp::Surface::rt_surface`, `surface.zig:639`), which is
//! heap-pinned by the GObject instance, so a raw unowned pointer back to the
//! widget is exactly the reference's model (no ref, no cycle).
//!
//! # The self-referential pin (memory `mirihar-surface-pin`)
//!
//! [`class::surface::initSurface`] hands `&priv.rt_surface` (a `*mut
//! gtk::Surface`) to `rustty_core::Surface::init`, which stores it and hands it
//! to the renderer/io threads. That pointer MUST stay stable for the surface's
//! life — it does, because `rt_surface` lives inside the GObject private, and
//! **the GObject instance IS the pin**: GObject heap-allocates the private and
//! never moves it. The backref [`Surface::surface`] is likewise stable; it is
//! set exactly once in `constructed` (`surface.zig:1793` `priv.rt_surface = .{
//! .surface = self }`) and never rewritten.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};

use glib::prelude::*;
use gtk4::subclass::prelude::*;

use rustty_apprt_api::{
    ApprtError, Clipboard, ClipboardContent, ClipboardRequest, ContentScale, CursorPos,
    Surface as ApprtSurface, SurfaceSize,
};

use super::app::App;
use super::class::application::Application;
use super::class::surface::Surface as GhosttySurface;

/// PORTS: `Surface.zig` `pub const Surface` — the GTK per-surface apprt backend.
///
/// Holds an unowned backref to the [`class::Surface`] GObject that embeds it
/// (`Surface.zig:12` `surface: *Surface`). Stored as the raw `GObject` pointer;
/// [`Surface::gobj`] reconstitutes the typed widget for each forward. `Default`
/// yields a null backref (set in the widget's `constructed`).
#[derive(Debug)]
pub struct Surface {
    /// PORTS: `Surface.zig:12` `surface: *Surface`. The owning `GhosttySurface`
    /// GObject, as a raw unowned `*mut GObject`. Null until the widget's
    /// `constructed` wires it (`surface.zig:1793`); never rewritten after.
    surface: std::cell::Cell<*mut glib::gobject_ffi::GObject>,

    /// NUL-terminated cache backing [`Surface::get_title`]. The apprt trait
    /// returns `Option<&CStr>` borrowed from `&self`, but the widget stores its
    /// title as a Rust `String`; this owns a stable heap buffer to lend the
    /// `&CStr` from. Refilled only when the title changes (matching the
    /// reference's `[:0]const u8` pointer into the surface's own title storage,
    /// which is likewise invalidated on the next title update).
    title_cache: RefCell<Option<CString>>,
}

impl Default for Surface {
    fn default() -> Self {
        Self {
            surface: std::cell::Cell::new(std::ptr::null_mut()),
            title_cache: RefCell::new(None),
        }
    }
}

impl Surface {
    /// Wire the backref to the owning widget. Called once from the widget's
    /// `constructed` (`surface.zig:1793` `priv.rt_surface = .{ .surface = self }`).
    pub(crate) fn set_widget(&self, widget: &GhosttySurface) {
        self.surface
            .set(widget.upcast_ref::<glib::Object>().as_ptr());
    }

    /// Reconstitute the owning [`class::Surface`] from the backref, or `None`
    /// before `constructed` wires it. Returns a temporary strong ref (dropped at
    /// the caller's scope end); the widget is owned by the GObject tree so this
    /// is never the last ref.
    ///
    /// NOTE: only used by the forwards that run while the widget is fully alive
    /// (refcount > 0). The [`ApprtSurface::core`] read is on the TEARDOWN path
    /// (`deleteSurface` calls it during the widget's own finalize, refcount 0), so
    /// it must NOT go through this refcounting reconstitution — see there.
    fn gobj(&self) -> Option<GhosttySurface> {
        let ptr = self.surface.get();
        if ptr.is_null() {
            return None;
        }
        // SAFETY: `ptr` is a live `GhosttySurface` GObject set by `set_widget`
        // from `constructed`; the widget outlives every apprt-surface forward
        // (it embeds this `Surface`). `from_glib_none` adds a temporary ref.
        let obj: glib::Object = unsafe { glib::translate::from_glib_none(ptr) };
        obj.downcast::<GhosttySurface>().ok()
    }

    /// The owning [`GhosttySurface`] widget, or `None` before `constructed` wires
    /// the backref. This is the crate-visible form of [`Surface::gobj`] — the
    /// reference's `rt_surface.surface` / `rt_surface.gobj()` accessor that the
    /// `Application` `performAction` `Action` namespace (`application.zig:1909-2843`)
    /// reads to reach the widget for a `.surface` target. Same liveness contract as
    /// [`Surface::gobj`]: only call while the widget is fully alive (refcount > 0),
    /// which is exactly the case during synchronous app-thread action dispatch.
    pub(crate) fn widget(&self) -> Option<GhosttySurface> {
        self.gobj()
    }
}

impl ApprtSurface for Surface {
    // PORTS: `Surface.zig` `apprt.runtime.Surface`/`apprt.runtime.App` mutual
    // reference — the GTK `Surface` names the GTK `App` (`App.zig:14`).
    type App = App;

    // PORTS: the embedded core surface. rustty-core constrains this to
    // `core::Surface<App>` at its impl blocks (`seam.rs` `Surface::Core` note),
    // so the GTK backend's core is `rustty_core::surface::Surface<App>`.
    type Core = rustty_core::surface::Surface<App>;

    fn core(&self) -> *mut Self::Core {
        // PORTS: `Surface.zig:24-28` `return self.surface.core().?` — a RAW read
        // through the `*Surface` backref, asserting non-null "because libghostty
        // should only be calling this for initialized surfaces".
        //
        // SOUNDNESS / TEARDOWN: this must NOT reconstruct a refcounting GObject
        // wrapper (`gobj`), because it is called on the widget's own finalize path
        // (`deleteSurface` compares `focused == rt_surface.core()`, app.rs), when
        // the instance refcount is already 0 — where `from_glib_none`/`_borrow`
        // trip glib's `ref_count != 0` debug assertion. Instead we read the
        // widget's `core_surface` directly from the raw instance pointer (the
        // reference's raw `*Surface` deref), which touches no refcount and is sound
        // both live (render thread) and during finalize (main thread).
        let ptr = self.surface.get();
        assert!(
            !ptr.is_null(),
            "gtk::Surface::core called on an uninitialized surface"
        );
        // SAFETY: `ptr` is a live `GhosttySurface` instance (set by `set_widget`
        // from `constructed`; its GObject header + private data are allocated even
        // mid-finalize). `core_surface_ptr` performs only a plain C-struct borrow +
        // an atomic load — no refcount, no wrapper resurrection.
        let core = unsafe { GhosttySurface::core_surface_ptr(ptr) };
        assert!(
            !core.is_null(),
            "gtk::Surface::core called on an uninitialized surface"
        );
        core
    }

    fn rt_app(&self) -> *mut Self::App {
        // PORTS: `Surface.zig:30-33` `return Application.default().rt()`. The
        // apprt `App` (`= gtk::App`) is stored type-erased in the singleton's
        // `rt_app` cell (installed by the run loop); reconstitute it here.
        let app = Application::default();
        match app.imp().rt_app.get() {
            Some(p) => p.as_ptr().cast::<App>(),
            None => std::ptr::null_mut(),
        }
    }

    fn close(&self, _process_alive: bool) {
        // PORTS: `Surface.zig:35-38`. The reference DISCARDS `process_active`
        // (`_ = process_active;`) and emits the widget's close-request signal.
        if let Some(s) = self.gobj() {
            s.close();
        }
    }

    fn deinit(&mut self) {
        // PORTS: `Surface.zig:14-16` — a no-op. The GObject finalize
        // (`class::surface`'s `Drop`, Task 14) does the real teardown.
    }

    fn get_content_scale(&self) -> Result<ContentScale, ApprtError> {
        // PORTS: `Surface.zig:48-50`.
        self.gobj().map(|s| s.get_content_scale()).ok_or(ApprtError)
    }

    fn get_size(&self) -> Result<SurfaceSize, ApprtError> {
        // PORTS: `Surface.zig:52-54`.
        self.gobj().map(|s| s.get_size()).ok_or(ApprtError)
    }

    fn get_title(&self) -> Option<&CStr> {
        // PORTS: `Surface.zig:44-46` `return self.surface.getTitle()`.
        //
        // The reference returns `?[:0]const u8` — a NUL-terminated pointer into
        // the surface's own title storage, valid until the title next changes.
        // The widget stores its title as a Rust `String`, so we build a
        // NUL-terminated `CString` and cache it in this backref at a stable heap
        // address to lend the `&CStr` from. The cache is refilled only when the
        // title actually changes, so the returned pointer stays valid until the
        // next title update — the same contract as the reference.
        let title = self.gobj()?.get_title()?;
        // A title with an interior NUL cannot be a C string; drop it (report no
        // title) rather than truncate.
        let cstring = CString::new(title).ok()?;

        let mut cache = self.title_cache.borrow_mut();
        if cache.as_ref() != Some(&cstring) {
            *cache = Some(cstring);
        }
        // A stable pointer into the cached `CString`'s heap buffer. The buffer
        // lives as long as this `Surface` backref (owned by the widget's pinned
        // private data) and is only ever replaced by a *different* title above.
        let ptr: *const CStr = cache.as_deref()?;
        drop(cache);
        // SAFETY: `ptr` points into the `CString` owned by `self.title_cache`,
        // whose heap buffer is stable for `&self`'s lifetime (only replaced on a
        // title change). The sole consumer (`Surface.report_title`, the CSI 21 t
        // path) copies the bytes immediately and does not retain the borrow
        // across another `get_title` call, so no aliased overwrite can dangle it.
        Some(unsafe { &*ptr })
    }

    fn supports_clipboard(&self, _clipboard_type: Clipboard) -> bool {
        // PORTS: `Surface.zig:60-71` — `true` for all three variants
        // (`.standard`, `.selection`, `.primary`).
        true
    }

    fn clipboard_request(
        &self,
        clipboard_type: Clipboard,
        state: ClipboardRequest,
    ) -> Result<bool, ApprtError> {
        // PORTS: `Surface.zig:73-82`.
        match self.gobj() {
            Some(s) => s.clipboard_request(clipboard_type, state),
            None => Err(ApprtError),
        }
    }

    fn set_clipboard(
        &self,
        clipboard_type: Clipboard,
        contents: &[ClipboardContent<'_>],
        confirm: bool,
    ) -> Result<(), ApprtError> {
        // PORTS: `Surface.zig:84-95`.
        match self.gobj() {
            Some(s) => {
                s.set_clipboard(clipboard_type, contents, confirm);
                Ok(())
            }
            None => Err(ApprtError),
        }
    }

    fn get_cursor_pos(&self) -> Result<CursorPos, ApprtError> {
        // PORTS: `Surface.zig:56-58`.
        self.gobj().map(|s| s.get_cursor_pos()).ok_or(ApprtError)
    }

    fn default_termio_env(&self) -> Result<rustty_global::EnvMap, ApprtError> {
        // PORTS: `Surface.zig:97-99`.
        self.gobj()
            .map(|s| s.default_termio_env())
            .ok_or(ApprtError)
    }
}

impl Surface {
    /// PORTS: the widget leg of `Action.render` — `v.rt_surface.surface.redraw()`
    /// (`class/application.zig:2485`). The reference reaches the `GhosttySurface`
    /// widget through the apprt surface's `.surface` field and calls `redraw()`;
    /// rustty's widget backref ([`Surface::gobj`]) is private, so this `pub(crate)`
    /// forwarder is the faithful analogue. Dormant (no-op) before `constructed`
    /// wires the backref. This is the app/main-thread leg of the **T16 render
    /// contract**, driven by `performAction(.render)`
    /// ([`super::class::application::action_render`]).
    pub(crate) fn redraw(&self) {
        if let Some(s) = self.gobj() {
            s.redraw();
        }
    }

    /// PORTS: `Surface.zig:102-104` `redrawInspector` → `self.surface.redrawInspector()`,
    /// which forwards to the widget's `redrawInspector` (`surface.zig:909-913`):
    /// redraw the inspector window attached to this surface, if there is one.
    ///
    /// Reached through [`App::redraw_inspector`](super::app::App::redraw_inspector).
    pub fn redraw_inspector(&self) {
        // `if let Some(win) = self.inspector() { win.queueRender() }`
        // (`surface.zig:911-912`).
        if let Some(win) = self.gobj().and_then(|w| w.inspector()) {
            win.queue_render();
        }
    }
}
