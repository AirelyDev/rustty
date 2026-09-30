//! N465 — `Surface::close(&mut self)` held a protected `&mut Surface` across the
//! runtime's close callback, which re-enters the surface synchronously.
//! Isolated, non-FFI model; see `README.md` for the candidates and results. Each
//! candidate is one executed path (Miri checks only what runs), selected by
//! argv[1].
//!
//! Shipped port (before P7-W12 T1):
//!
//! - core `close(&mut self)`: `self.rt_surface.close(self.needsConfirmQuit())`,
//!   with `childExited(&mut self)` and `handleMessageArms(&mut self)` above it;
//! - the seam's `fn close(&self, process_alive)`; the embedded surface holds
//!   the core surface INLINE, so that `&self` is a protector over the core
//!   bytes too.
//!
//! Host re-entry inside the close callback:
//!
//! - macOS: `close_surface_cb` posts `rusttyCloseSurface`; the observer runs
//!   synchronously, `closeSurface(withConfirmation: false)` ->
//!   `removeSurfaceNode` -> `surfaceTree` didSet -> `surfaceTreeDidChange` ->
//!   `cancelPendingClipboardConfirmation` -> `pendingClipboardConfirmation =
//!   nil` -> didSet -> `previous.cancel(from:)` -> the completion ->
//!   `rustty_surface_deny_clipboard_request` -> embedded
//!   `denyClipboardRequest(&mut self)` -> core `denyClipboardRequest` (a
//!   WRITE). Only when a clipboard confirmation is pending on that surface.
//! - GTK: `close` emits `close-request`; `SplitTree.surfaceCloseRequest` takes
//!   `&mut *core` for `needsConfirmQuit` (a retag + read). Every close.
//!
//! The fix: core `close`/`childExited` raw-self (dispatched raw by
//! `handleMessage`), and the seam's `unsafe fn close(self_: *const Self, ..)`
//! reaching the runtime surface by place.

#![allow(clippy::missing_safety_doc)]

use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Macos,
    Gtk,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Before the fix: `&mut self` core receivers, `&self` seam receiver.
    Shipped,
    /// NEGATIVE: the core half fixed (raw-self), the seam still `&self`.
    CoreOnly,
    /// The fix.
    Fixed,
}

#[derive(Clone, Copy)]
pub enum Msg {
    Close,
    ChildExited,
}

thread_local! {
    static HOST: Cell<Host> = const { Cell::new(Host::Macos) };
    static MODE: Cell<Mode> = const { Cell::new(Mode::Fixed) };
    /// The host's own state: a clipboard confirmation is pending on the surface.
    static HOST_PENDING: Cell<bool> = const { Cell::new(true) };
    static CLOSED: Cell<u32> = const { Cell::new(0) };
}
fn host() -> Host {
    HOST.with(|h| h.get())
}
fn mode() -> Mode {
    MODE.with(|m| m.get())
}

// ------------------------------------------------------------------ core

/// The core `Surface` (the fields the paths touch).
pub struct Core {
    pub child_exited: bool,
    pub process_alive: bool,
    pub wait_after_command: bool,
    /// Live clipboard requests (`denyClipboardRequest` writes it).
    pub clipboard_requests: u32,
    /// `*apprt.runtime.Surface`: an `Embedded` or a `GtkRt`, per `HOST`.
    pub rt_surface: *mut u8,
}

impl Core {
    fn needs_confirm_quit(&mut self) -> bool {
        self.process_alive
    }

    fn deny_clipboard_request(&mut self) {
        self.clipboard_requests -= 1;
    }

    // ---- shipped receivers ----

    fn close_ref(&mut self) {
        let alive = self.needs_confirm_quit();
        unsafe { rt_close(self.rt_surface, alive) };
    }

    fn child_exited_ref(&mut self) {
        self.child_exited = true;
        if self.wait_after_command {
            return;
        }
        self.close_ref();
    }

    fn handle_message_arms(&mut self, msg: Msg) {
        match msg {
            Msg::Close => self.close_ref(),
            Msg::ChildExited => self.child_exited_ref(),
        }
    }

    // ---- fixed: raw-self ----

    pub unsafe fn close_raw(self_: *mut Core) {
        let alive = unsafe { &mut *self_ }.needs_confirm_quit();
        let rt = unsafe { (*self_).rt_surface };
        unsafe { rt_close(rt, alive) };
    }

    pub unsafe fn child_exited_raw(self_: *mut Core) {
        let this = unsafe { &mut *self_ };
        this.child_exited = true;
        if this.wait_after_command {
            return;
        }
        unsafe { Core::close_raw(self_) };
    }
}

/// `Surface::handleMessage(self_: *mut Surface, msg)`.
pub unsafe fn handle_message(self_: *mut Core, msg: Msg) {
    match mode() {
        Mode::Shipped => unsafe { &mut *self_ }.handle_message_arms(msg),
        Mode::CoreOnly | Mode::Fixed => match msg {
            Msg::Close => unsafe { Core::close_raw(self_) },
            Msg::ChildExited => unsafe { Core::child_exited_raw(self_) },
        },
    }
}

/// `rustty_surface_request_close`'s unhandled fallback.
pub unsafe fn request_close_fallback(emb: *mut Embedded) {
    match mode() {
        Mode::Shipped => unsafe { (*emb).core_surface.close_ref() },
        Mode::CoreOnly | Mode::Fixed => unsafe { Core::close_raw(&raw mut (*emb).core_surface) },
    }
}

// --------------------------------------------------------------- runtimes

/// The embedded apprt `Surface`: `core_surface` inline.
pub struct Embedded {
    /// The host's handle (`SurfaceView.surface`): the allocation root.
    pub userdata: *mut Embedded,
    pub close_surface: unsafe fn(*mut Embedded, bool),
    pub core_surface: Core,
}

impl Embedded {
    /// SHIPPED seam: `fn close(&self, process_alive)`.
    fn close_ref(&self, alive: bool) {
        unsafe { (self.close_surface)(self.userdata, alive) };
    }

    /// FIXED seam: `unsafe fn close(self_: *const Self, process_alive)`.
    unsafe fn close_raw(self_: *const Embedded, alive: bool) {
        let func = unsafe { (*self_).close_surface };
        let userdata = unsafe { (*self_).userdata };
        unsafe { func(userdata, alive) };
    }

    /// `denyClipboardRequest(&mut self, state)`.
    fn deny_clipboard_request(&mut self) {
        self.core_surface.deny_clipboard_request();
    }
}

/// The GTK apprt `Surface`: a separate allocation pointing at the core.
pub struct GtkRt {
    pub core: *mut Core,
}

impl GtkRt {
    fn close_ref(&self, _alive: bool) {
        unsafe { gtk_emit_close_request(self.core) };
    }
    unsafe fn close_raw(self_: *const GtkRt, _alive: bool) {
        let core = unsafe { &*self_ }.core;
        unsafe { gtk_emit_close_request(core) };
    }
}

/// `apprt.runtime.Surface.close`, per host and mode.
unsafe fn rt_close(rt: *mut u8, alive: bool) {
    let raw_seam = mode() == Mode::Fixed;
    match host() {
        Host::Macos => {
            let rt = rt.cast::<Embedded>();
            if raw_seam {
                unsafe { Embedded::close_raw(rt, alive) }
            } else {
                unsafe { &*rt }.close_ref(alive)
            }
        }
        Host::Gtk => {
            let rt = rt.cast::<GtkRt>();
            if raw_seam {
                unsafe { GtkRt::close_raw(rt, alive) }
            } else {
                unsafe { &*rt }.close_ref(alive)
            }
        }
    }
}

// ------------------------------------------------------------------ hosts

/// macOS `close_surface_cb` -> the synchronous notification observer.
unsafe fn macos_close_surface_cb(userdata: *mut Embedded, process_alive: bool) {
    CLOSED.with(|c| c.set(c.get() + 1));
    // withConfirmation == process_alive; the alert path is asynchronous.
    if process_alive {
        return;
    }
    // removeSurfaceNode -> ... -> cancelPendingClipboardConfirmation.
    if HOST_PENDING.with(|p| p.replace(false)) {
        // rustty_surface_deny_clipboard_request(surface, state)
        unsafe { (&mut *userdata).deny_clipboard_request() };
    }
}

/// GTK `close-request` -> `SplitTree.surfaceCloseRequest`.
unsafe fn gtk_emit_close_request(core: *mut Core) {
    CLOSED.with(|c| c.set(c.get() + 1));
    if !unsafe { &mut *core }.needs_confirm_quit() {
        // closeConfirmationClose: the tree drops the widget (not freed here;
        // the emission holds a reference, see N442's audit).
    }
}

// ---------------------------------------------------------------- scenarios

fn run(h: Host, m: Mode, f: fn()) {
    HOST.with(|x| x.set(h));
    MODE.with(|x| x.set(m));
    HOST_PENDING.with(|p| p.set(true));
    CLOSED.with(|c| c.set(0));
    f();
    assert!(CLOSED.with(|c| c.get()) >= 1, "the close callback never ran");
    println!("ok");
}

fn new_core(rt: *mut u8) -> Core {
    Core {
        child_exited: false,
        process_alive: false,
        wait_after_command: false,
        clipboard_requests: 1,
        rt_surface: rt,
    }
}

fn macos_surface() -> *mut Embedded {
    let emb = Box::into_raw(Box::new(Embedded {
        userdata: core::ptr::null_mut(),
        close_surface: macos_close_surface_cb,
        core_surface: new_core(core::ptr::null_mut()),
    }));
    unsafe {
        (*emb).userdata = emb;
        (*emb).core_surface.rt_surface = emb.cast();
    }
    emb
}

fn free_macos(emb: *mut Embedded) {
    drop(unsafe { Box::from_raw(emb) });
}

/// `rustty_app_tick` -> `App::drainMailbox` -> `handleMessage(.child_exited)`.
fn macos_child_exited() {
    let emb = macos_surface();
    unsafe { handle_message(&raw mut (*emb).core_surface, Msg::ChildExited) };
    assert_eq!(unsafe { (*emb).core_surface.clipboard_requests }, 0);
    free_macos(emb);
}

/// Every route on macOS: `.close`, `.child_exited`, the C request_close fallback.
fn macos_all() {
    for route in 0..3 {
        HOST_PENDING.with(|p| p.set(true));
        let emb = macos_surface();
        unsafe {
            match route {
                0 => handle_message(&raw mut (*emb).core_surface, Msg::Close),
                1 => handle_message(&raw mut (*emb).core_surface, Msg::ChildExited),
                _ => request_close_fallback(emb),
            }
        }
        assert_eq!(unsafe { (*emb).core_surface.clipboard_requests }, 0);
        free_macos(emb);
    }
}

fn gtk_surface() -> (*mut Core, *mut GtkRt) {
    let core = Box::into_raw(Box::new(new_core(core::ptr::null_mut())));
    let rt = Box::into_raw(Box::new(GtkRt { core }));
    unsafe { (*core).rt_surface = rt.cast() };
    (core, rt)
}

fn free_gtk(core: *mut Core, rt: *mut GtkRt) {
    drop(unsafe { Box::from_raw(rt) });
    drop(unsafe { Box::from_raw(core) });
}

/// The GTK tick -> `handleMessage(.child_exited)`.
fn gtk_child_exited() {
    let (core, rt) = gtk_surface();
    unsafe { handle_message(core, Msg::ChildExited) };
    free_gtk(core, rt);
}

fn gtk_all() {
    for msg in [Msg::Close, Msg::ChildExited] {
        let (core, rt) = gtk_surface();
        unsafe { handle_message(core, msg) };
        free_gtk(core, rt);
    }
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    match which.as_str() {
        // ---- shipped ----
        "n465_shipped_macos_child_exited" => run(Host::Macos, Mode::Shipped, macos_child_exited),
        "n465_shipped_gtk_child_exited" => run(Host::Gtk, Mode::Shipped, gtk_child_exited),
        // ---- negative: core raw-self, seam still `&self` ----
        "n465_core_only_macos_child_exited" => run(Host::Macos, Mode::CoreOnly, macos_child_exited),
        // ---- fixed ----
        "n465_fixed_macos_child_exited" => run(Host::Macos, Mode::Fixed, macos_child_exited),
        "n465_fixed_gtk_child_exited" => run(Host::Gtk, Mode::Fixed, gtk_child_exited),
        "n465_fixed_macos_all" => run(Host::Macos, Mode::Fixed, macos_all),
        "n465_fixed_gtk_all" => run(Host::Gtk, Mode::Fixed, gtk_all),
        other => panic!("unknown candidate {other:?}"),
    }
}
