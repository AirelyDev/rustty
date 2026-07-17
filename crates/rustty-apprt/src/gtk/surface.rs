//! rustty-apprt::gtk::surface
//!
//! PORTS: ghostty `src/apprt/gtk/Surface.zig` (the per-surface apprt backend —
//! a `GtkWidget` subclass wrapping a libghostty-core `Surface`). **This file is
//! an INHABITED placeholder skeleton (L5.1 T11).** Task 11 needs a concrete type
//! to satisfy `gtk::App`'s `type Surface: apprt_api::Surface<App = App>`
//! associated-type bound (`App.zig:14` `const Surface = @import("Surface.zig")`),
//! so the type exists NOW with its seam methods as placeholder-leaves; **L5.1
//! T18 ports the real `Surface.zig`** (the `GtkWidget` subclass, the core-surface
//! embedding, the event/clipboard/render wiring) into this file.
//!
//! Per the placeholder-leaf doctrine, every method that reaches a Task-18
//! dependency is present (so the seam's call-graph shape exists and the crate
//! compiles standalone) but `unimplemented!()`s or returns a safe default until
//! that task fills it. No `Surface` is constructed anywhere in this tier yet, so
//! none of these bodies is reachable at runtime.

use std::collections::HashMap;
use std::ffi::CStr;

use rustty_apprt_api::{
    ApprtError, Clipboard, ClipboardContent, ClipboardRequest, ContentScale, CursorPos,
    Surface as ApprtSurface, SurfaceSize,
};

use super::app::App;

/// PORTS: `Surface.zig` `pub const Surface` — the GTK per-surface apprt backend.
///
/// **Placeholder skeleton (L5.1 T11 → filled by T18).** The real type is a
/// `GtkWidget` subclass embedding a `rustty_core::surface::Surface<App>`; the
/// skeleton is field-less because Task 11 only requires the type to *exist* and
/// satisfy the [`ApprtSurface`] seam so [`App`]'s associated `Surface` resolves.
#[derive(Debug, Default)]
pub struct Surface {
    // PORT NOTE (T18): the real fields — the embedded `core_surface`, the
    // `GtkWidget`/`GLArea` handles, the IME/clipboard state — land with the
    // `Surface.zig` port. The skeleton carries none so it constructs trivially
    // (it never is, in this tier) and so T18 defines the storage from scratch.
    _private: (),
}

impl ApprtSurface for Surface {
    // PORTS: `Surface.zig` `apprt.runtime.Surface`/`apprt.runtime.App` mutual
    // reference — the GTK `Surface` names the GTK `App` (`App.zig:14`).
    type App = App;

    // PORTS: the embedded core surface. rustty-core constrains this to
    // `core::Surface<App>` at its impl blocks (`seam.rs` `Surface::Core` note),
    // so the GTK backend's core is `rustty_core::surface::Surface<App>` — the
    // faithful analogue of `Surface.core_surface: CoreSurface`.
    type Core = rustty_core::surface::Surface<App>;

    fn core(&self) -> *mut Self::Core {
        // PORT NOTE (placeholder-leaf, T18): `pub fn core(self: *Surface)
        // *CoreSurface` returns `&self.core_surface`. No core surface exists in
        // the skeleton; T18 embeds it and returns its address.
        unimplemented!("gtk::Surface::core lands with the Surface.zig port (L5.1 T18)")
    }

    fn rt_app(&self) -> *mut Self::App {
        // PORT NOTE (placeholder-leaf, T18): `pub fn rtApp(...) *App` returns the
        // owning apprt `App` backref.
        unimplemented!("gtk::Surface::rt_app lands with the Surface.zig port (L5.1 T18)")
    }

    fn close(&self, _process_alive: bool) {
        // PORT NOTE (placeholder-leaf, T18): `pub fn close(...)` requests the
        // window/tab close (confirming when a process is alive).
        unimplemented!("gtk::Surface::close lands with the Surface.zig port (L5.1 T18)")
    }

    fn deinit(&mut self) {
        // PORT NOTE (placeholder-leaf, T18): tears down the core surface + GTK
        // widgets. No-op while the skeleton owns nothing.
    }

    fn get_content_scale(&self) -> Result<ContentScale, ApprtError> {
        unimplemented!("gtk::Surface::get_content_scale lands with the Surface.zig port (L5.1 T18)")
    }

    fn get_size(&self) -> Result<SurfaceSize, ApprtError> {
        unimplemented!("gtk::Surface::get_size lands with the Surface.zig port (L5.1 T18)")
    }

    fn get_title(&self) -> Option<&CStr> {
        // PORT NOTE (placeholder-leaf, T18): returns the stored surface title.
        None
    }

    fn supports_clipboard(&self, _clipboard_type: Clipboard) -> bool {
        unimplemented!(
            "gtk::Surface::supports_clipboard lands with the Surface.zig port (L5.1 T18)"
        )
    }

    fn clipboard_request(
        &self,
        _clipboard_type: Clipboard,
        _state: ClipboardRequest,
    ) -> Result<bool, ApprtError> {
        unimplemented!("gtk::Surface::clipboard_request lands with the Surface.zig port (L5.1 T18)")
    }

    fn set_clipboard(
        &self,
        _clipboard_type: Clipboard,
        _contents: &[ClipboardContent<'_>],
        _confirm: bool,
    ) -> Result<(), ApprtError> {
        unimplemented!("gtk::Surface::set_clipboard lands with the Surface.zig port (L5.1 T18)")
    }

    fn get_cursor_pos(&self) -> Result<CursorPos, ApprtError> {
        unimplemented!("gtk::Surface::get_cursor_pos lands with the Surface.zig port (L5.1 T18)")
    }

    fn default_termio_env(&self) -> Result<HashMap<String, String>, ApprtError> {
        unimplemented!(
            "gtk::Surface::default_termio_env lands with the Surface.zig port (L5.1 T18)"
        )
    }
}

impl Surface {
    /// PORTS: `Surface.zig` `redrawInspector` — requests a redraw of this
    /// surface's terminal inspector overlay.
    ///
    /// **Placeholder-leaf (T18 / the inspector tier).** Reached only through
    /// [`App::redraw_inspector`](super::app::App::redraw_inspector), which is
    /// itself dormant until a surface is created (no surface exists in this
    /// tier). The body drops in with the inspector-rendering work.
    pub fn redraw_inspector(&self) {
        // PORT NOTE (placeholder-leaf): `surface.redrawInspector()` queues the
        // inspector render. No inspector/render surface exists yet (T18+).
    }
}
