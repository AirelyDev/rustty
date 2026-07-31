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
//! # How it cranks the queue (queue-pending prepare/check, NOT fd revents)
//!
//! `prepare`/`check` report the source ready iff OUR private queue has pending
//! events, tested with the libwayland read protocol:
//! [`EventQueue::prepare_read`] returns `None` exactly when events are already
//! pending (`wl_display_prepare_read_queue` fails with non-empty queue); when it
//! returns a guard the queue is empty and dropping the guard cancels the read
//! intention (`wl_display_cancel_read`) — we NEVER read the socket ourselves,
//! GDK owns the read (a second reader would fight GDK's own prepare/read cycle).
//! `dispatch` then runs [`EventQueue::dispatch_pending`] and
//! [`flush`](EventQueue::flush)es the requests the handlers sent.
//!
//! The source needs NO fd watch: events for our queue arrive on the shared
//! `wl_display` fd that GDK's source already polls, so any such arrival wakes the
//! main loop, GDK reads + demarshals onto our queue during ITS dispatch, and the
//! very next loop iteration our `prepare` sees the pending events and dispatches.
//!
//! A previous revision of this source instead watched the connection fd
//! (`g_source_add_unix_fd`) and keyed `check` on `G_IO_IN`. That NEVER fires: GLib
//! dispatches only the highest-priority READY band per iteration (`gmain.c`
//! `g_main_context_check` stops collecting once a higher band is ready), and GDK's
//! source — same fd, higher priority — is ready in exactly the iterations ours
//! would be, wins the band, and drains the socket; next poll the revents are gone.
//! The live-session KAT (`gtk_wayland_winproto.rs`,
//! `wayland_set_urgent_dispatches_activation_token`) caught this: the token `done`
//! sat demarshalled-but-undispatched on the private queue. Queue-pending readiness
//! is level-triggered, so being band-skipped one iteration just defers dispatch to
//! the next one.

use std::cell::RefCell;
use std::ffi::{c_int, c_uint};
use std::mem::size_of;
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

/// A `GSource` subclass: `GSource` header followed by a raw pointer to the shared
/// [`Pump`], so `prepare`/`check` (which receive only the source pointer, no user
/// data) can test whether the private queue has pending events. `#[repr(C)]` with
/// `GSource` first is the standard GLib source-subclassing layout. The pointee is
/// kept alive by the `Rc` clone stored as the source's callback data (freed by
/// [`destroy_user_data`] at finalize, after which GLib never calls these funcs).
#[repr(C)]
struct WlGSource {
    source: glib::ffi::GSource,
    pump: *const RefCell<Pump>,
}

/// Ready iff the private queue has pending events. `prepare_read()` returns `None`
/// exactly then; a returned guard (empty queue) is dropped immediately, cancelling
/// the read intention — the socket read stays GDK's alone.
unsafe fn queue_has_pending(source: *mut glib::ffi::GSource) -> glib::ffi::gboolean {
    let pump = unsafe { (*source.cast::<WlGSource>()).pump };
    if pump.is_null() {
        return glib::ffi::GFALSE;
    }
    // Never nested with the trampoline (GLib calls prepare/check/dispatch
    // sequentially) nor with App's shared borrows (main-thread, non-reentrant);
    // fail closed (not-ready) if a borrow is somehow live.
    match unsafe { &*pump }.try_borrow_mut() {
        Ok(mut p) => match p.event_queue.prepare_read() {
            None => glib::ffi::GTRUE,
            Some(guard) => {
                drop(guard);
                glib::ffi::GFALSE
            }
        },
        Err(_) => glib::ffi::GFALSE,
    }
}

/// `GSourceFuncs.prepare`: ready iff our queue has pending events; otherwise poll
/// without a timeout on our account (`*timeout = -1`) — GDK's fd source wakes the
/// loop when wayland data arrives.
unsafe extern "C" fn prepare(
    source: *mut glib::ffi::GSource,
    timeout: *mut c_int,
) -> glib::ffi::gboolean {
    if !timeout.is_null() {
        unsafe { *timeout = -1 };
    }
    unsafe { queue_has_pending(source) }
}

/// `GSourceFuncs.check`: same level-triggered test as [`prepare`] (GDK's dispatch
/// this iteration may have demarshalled new events onto our queue).
unsafe extern "C" fn check(source: *mut glib::ffi::GSource) -> glib::ffi::gboolean {
    unsafe { queue_has_pending(source) }
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
    // `user_data` is the `Box<Rc<RefCell<Pump>>>` raw pointer installed by
    // `attach` (and freed by `destroy_user_data`) — deref the OUTER Rc first.
    // Casting it straight to `*const RefCell<Pump>` would misread the Rc's heap
    // pointer as the RefCell borrow flag and fail every borrow.
    let pump: &Rc<RefCell<Pump>> = unsafe { &*(user_data as *const Rc<RefCell<Pump>>) };
    // Not nested with Window::init's shared borrow (both main-thread, non-reentrant).
    let emissions = if let Ok(mut p) = pump.try_borrow_mut() {
        let p = &mut *p;
        // dispatch_pending processes events GDK already demarshalled onto our queue.
        let _ = p.event_queue.dispatch_pending(p.globals.as_mut());
        // Push out anything the handlers queued (activation token activate, etc.).
        let _ = p.event_queue.flush();
        // Collect (do NOT run) the global-shortcut signal emissions the
        // `vicinae_hotkey_v1` handlers produced.
        p.globals.hotkeys_mut().take_pending()
    } else {
        Vec::new()
    };

    // Run them with the pump borrow RELEASED. A `trigger` emission performs an
    // arbitrary keybind action, and some of those (`toggle_quick_terminal` →
    // `winproto::Window::init`) re-enter this very `RefCell`; emitting inside the
    // borrow above would turn a bound global hotkey into a panic. See
    // `hotkeys.rs`'s "emissions are DEFERRED out of the dispatch borrow".
    for e in emissions {
        e.emit();
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
    /// Create the source over the [`Pump`]'s private queue and attach it to the
    /// default main context. `pump` is cloned into the source's callback data (which
    /// keeps the pointee of the `WlGSource.pump` pointer alive until finalize).
    pub fn attach(pump: &Rc<RefCell<Pump>>) -> Self {
        unsafe {
            // Allocate the GSource subclass (GSource header + our `pump` pointer).
            let source = glib::ffi::g_source_new(
                ptr::addr_of!(FUNCS) as *mut glib::ffi::GSourceFuncs,
                size_of::<WlGSource>() as c_uint,
            );

            // Callback data = an owning `Rc<RefCell<Pump>>` clone, freed on finalize;
            // `prepare`/`check` reach the same RefCell through the struct pointer.
            let data = Box::into_raw(Box::new(pump.clone()));
            (*source.cast::<WlGSource>()).pump = Rc::as_ptr(&*data);

            // Lower priority than GDK's event source so GDK reads the socket first
            // and our `dispatch_pending` sees the already-demarshalled events. GLib
            // dispatches one priority band per iteration, but our readiness is
            // level-triggered (queue-pending), so we run on the next iteration.
            glib::ffi::g_source_set_priority(source, glib::ffi::G_PRIORITY_DEFAULT_IDLE);

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
            // callback data (drops the Rc clone; prepare/check are never called on a
            // destroyed source, so the struct's pump pointer cannot dangle in use).
            glib::ffi::g_source_destroy(self.source);
            glib::ffi::g_source_unref(self.source);
        }
    }
}
