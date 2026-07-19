//! rustty-apprt::gtk::winproto::wayland::queue_source
//!
//! A custom GLib `GSource` that cranks the registry [`Connection`]'s PRIVATE
//! libwayland event queue from the main loop, so post-init inbound events arrive.
//!
//! # Why this exists (the `set_urgent` gap)
//!
//! `Backend::from_foreign_display` gives the registry [`Connection`] its OWN
//! libwayland queue (`wl_display_create_queue`); every rustty proxy — the registry,
//! the bound globals, and the per-surface `xdg_activation_token_v1` — dispatches
//! into that queue. GDK's own event source reads the shared `wl_display` fd and
//! demarshals each incoming event onto the queue its target proxy belongs to
//! (including ours), then dispatches only GDK's DEFAULT queue. Our queue's buffered
//! events therefore sit undispatched: [`super::App::init`]'s two-phase roundtrip
//! captures all bind-time state, but events that arrive LATER never fire their
//! handlers. The one with a user-visible effect is `xdg_activation_token_v1.done` —
//! the reply that carries the activation token [`super::Window::set_urgent`] needs
//! to call `xdg_activation_v1.activate` and flash the taskbar.
//!
//! The reference sidesteps this by putting its registry on the `wl_display`'s
//! DEFAULT queue, which GDK dispatches for free (`Globals.zig:82`). wayland-rs
//! dispatch handlers can only be invoked by wayland-rs' own machinery, not
//! libwayland's C default-dispatch, so rustty must crank its private queue itself.
//!
//! # How it cranks the queue
//!
//! A `GSource` watches the connection fd for readability. GLib runs a single poll
//! per main-loop iteration and dispatches EVERY source made ready by it, in
//! priority order — so on any iteration where the fd is readable, GDK's source
//! (priority `G_PRIORITY_DEFAULT` + 10) AND this source both dispatch. This source
//! runs at `G_PRIORITY_DEFAULT_IDLE` (a larger number → lower priority) so GDK's
//! read has already demarshalled the incoming events onto our queue by the time our
//! callback runs; the callback then [`EventQueue::dispatch_pending`]s our queue and
//! [`flush`](EventQueue::flush)es the requests those handlers send. We deliberately
//! do NOT `read_events` ourselves — a second reader on GDK's `wl_display` would fight
//! GDK's own prepare/read cycle; letting GDK own the socket read and cranking only
//! `dispatch_pending` is the safe libwayland multi-queue pattern.

use std::cell::RefCell;
use std::ffi::{c_int, c_uint};
use std::mem::size_of;
use std::os::fd::{AsFd, AsRawFd};
use std::ptr;
use std::rc::Rc;

use glib::translate::ToGlibPtr;
use wayland_client::EventQueue;

use super::globals::Globals;

/// The private event queue plus the state its handlers dispatch against, shared
/// (`Rc<RefCell<..>>`) between [`super::App`] — which reads `globals` at
/// [`super::Window::init`] time — and this source's callback, which drains the
/// queue. Both live on the GTK main thread; the `RefCell` borrows never nest (the
/// callback runs only from the main loop, never re-entrantly with `Window::init`).
pub struct Pump {
    /// The registry's private libwayland queue (from `Connection::new_event_queue`).
    pub event_queue: EventQueue<Globals>,
    /// The registry binder + bound-global store the queue's handlers dispatch into.
    /// Boxed for a stable address, matching the reference's heap allocation
    /// (`Globals.zig:72-80`).
    pub globals: Box<Globals>,
}

/// A `GSource` subclass: `GSource` header followed by the `g_source_add_unix_fd`
/// tag, so [`check`] can query the fd's readiness. `#[repr(C)]` with `GSource`
/// first is the standard GLib source-subclassing layout.
#[repr(C)]
struct WlGSource {
    source: glib::ffi::GSource,
    /// The tag returned by `g_source_add_unix_fd`, used by [`check`].
    tag: glib::ffi::gpointer,
}

/// `GSourceFuncs.prepare`: never ready without a poll — block in poll until the fd
/// signals (`*timeout = -1`).
unsafe extern "C" fn prepare(
    _source: *mut glib::ffi::GSource,
    timeout: *mut c_int,
) -> glib::ffi::gboolean {
    if !timeout.is_null() {
        unsafe { *timeout = -1 };
    }
    glib::ffi::GFALSE
}

/// `GSourceFuncs.check`: ready iff the connection fd is readable.
unsafe extern "C" fn check(source: *mut glib::ffi::GSource) -> glib::ffi::gboolean {
    let wl = source.cast::<WlGSource>();
    let cond = unsafe { glib::ffi::g_source_query_unix_fd(source, (*wl).tag) };
    if cond & glib::ffi::G_IO_IN != 0 {
        glib::ffi::GTRUE
    } else {
        glib::ffi::GFALSE
    }
}

/// `GSourceFuncs.dispatch`: invoke the registered callback (the [`trampoline`]).
unsafe extern "C" fn dispatch(
    _source: *mut glib::ffi::GSource,
    callback: glib::ffi::GSourceFunc,
    user_data: glib::ffi::gpointer,
) -> glib::ffi::gboolean {
    match callback {
        Some(cb) => unsafe { cb(user_data) },
        // No callback set — remove the source (should never happen; we always set it).
        None => glib::ffi::G_SOURCE_REMOVE,
    }
}

static FUNCS: glib::ffi::GSourceFuncs = glib::ffi::GSourceFuncs {
    prepare: Some(prepare),
    check: Some(check),
    dispatch: Some(dispatch),
    finalize: None,
    closure_callback: None,
    closure_marshal: None,
};

/// The `GSourceFunc` GLib calls on each dispatch: crank the private queue, then
/// flush any requests the handlers sent (e.g. the `xdg_activation_v1.activate` that
/// [`Globals`]' token-`done` handler issues). Returns `G_SOURCE_CONTINUE` to keep
/// the source alive for the app's lifetime.
unsafe extern "C" fn trampoline(user_data: glib::ffi::gpointer) -> glib::ffi::gboolean {
    let pump = unsafe { &*(user_data as *const RefCell<Pump>) };
    // Not nested with Window::init's shared borrow (both main-thread, non-reentrant).
    if let Ok(mut p) = pump.try_borrow_mut() {
        let p = &mut *p;
        // dispatch_pending processes events GDK already demarshalled onto our queue.
        let _ = p.event_queue.dispatch_pending(p.globals.as_mut());
        // Push out anything the handlers queued (activation token activate, etc.).
        let _ = p.event_queue.flush();
    }
    glib::ffi::G_SOURCE_CONTINUE
}

/// `GDestroyNotify` for the callback data: drop the `Rc` clone the source held.
unsafe extern "C" fn destroy_user_data(user_data: glib::ffi::gpointer) {
    drop(unsafe { Box::from_raw(user_data as *mut Rc<RefCell<Pump>>) });
}

/// Owns the attached `GSource`; destroys + unrefs it on drop (which detaches it from
/// the main context and runs [`destroy_user_data`], releasing the `Rc` clone). Held
/// by [`super::App`] for the app's lifetime.
pub struct WlQueueSource {
    source: *mut glib::ffi::GSource,
}

impl WlQueueSource {
    /// Create the source over the [`Pump`]'s connection fd and attach it to the
    /// default main context. `pump` is cloned into the source's callback data.
    pub fn attach(pump: &Rc<RefCell<Pump>>) -> Self {
        // The connection fd is stable for the app's lifetime (GDK owns the display).
        let fd = pump
            .borrow()
            .event_queue
            .as_fd()
            .as_raw_fd();

        unsafe {
            // Allocate the GSource subclass (GSource header + our `tag`).
            let source = glib::ffi::g_source_new(
                ptr::addr_of!(FUNCS) as *mut glib::ffi::GSourceFuncs,
                size_of::<WlGSource>() as c_uint,
            );

            // Watch the connection fd for readability; stash the tag for `check`.
            let tag = glib::ffi::g_source_add_unix_fd(source, fd, glib::ffi::G_IO_IN);
            (*source.cast::<WlGSource>()).tag = tag;

            // Lower priority than GDK's event source so GDK reads the socket first
            // and our `dispatch_pending` sees the already-demarshalled events.
            glib::ffi::g_source_set_priority(source, glib::ffi::G_PRIORITY_DEFAULT_IDLE);

            // Callback data = an owning `Rc<RefCell<Pump>>` clone, freed on finalize.
            let data = Box::into_raw(Box::new(pump.clone()));
            glib::ffi::g_source_set_callback(
                source,
                Some(trampoline),
                data as glib::ffi::gpointer,
                Some(destroy_user_data),
            );

            let ctx = glib::MainContext::default();
            glib::ffi::g_source_attach(source, ctx.to_glib_none().0);

            WlQueueSource { source }
        }
    }
}

impl Drop for WlQueueSource {
    fn drop(&mut self) {
        unsafe {
            // Detach from the context, then release our ref → finalize → destroy the
            // callback data (drops the Rc clone). GLib removes the unix fd on finalize.
            glib::ffi::g_source_destroy(self.source);
            glib::ffi::g_source_unref(self.source);
        }
    }
}
