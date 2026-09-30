//! N456 — the shared `apprt::Target::Surface(&C)` across `rt_app.perform_action`.
//! Isolated, non-FFI model; see `README.md` for the candidates and results. Each
//! candidate is one executed path (Miri checks only what runs), selected by
//! argv[1].
//!
//! The shipped port passes `Target::Surface(&core_surface)` BY VALUE to
//! `perform_action`. A reference field of a by-value argument is retagged at
//! function entry with a PROTECTOR over every non-`UnsafeCell` byte of the core
//! `Surface`, live for the whole synchronous host action callback. The macOS host
//! resizes the window inside that callback for `reset_window_size` (live) and
//! `resize_window` (P7-W9 T2): `setFrame(display: true)` lays the view out
//! synchronously, `rustty_surface_set_size` -> embedded `updateSize(&mut self)`
//! -> core `sizeCallback(&mut self)`, a WRITE to the protected bytes.
//!
//! The fix (P7-W6 T3, ruling 55): `Target::Surface(SurfacePtr<'s, C>)`, a
//! `NonNull<C>` with no protector (the reference's `*CoreSurface`). Consumers
//! dereference it only for the one field read they need.

#![allow(clippy::missing_safety_doc)]

use core::marker::PhantomData;
use core::ptr::NonNull;
use std::cell::Cell;

// ------------------------------------------------------------------ types

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// `.reset_window_size` (a surface binding).
    ResetWindowSize,
    /// `.resize_window` (a program's CSI 8 t, via `handleMessage`).
    ResizeWindow { width: u32, height: u32 },
    /// `.set_title`: the embedded `performPreAction` reads `rt_surface` through
    /// the target, then the host does nothing re-entrant.
    SetTitle,
}

/// The core `Surface` (the fields the paths touch).
pub struct Core {
    pub size: (u32, u32),
    pub cell: (u32, u32),
    pub vt_window_resize_allowed: bool,
    pub readonly: bool,
    pub last_binding: u32,
    pub rt_surface: *mut Embedded,
    pub rt_app: *const RtApp,
}

/// The embedded apprt `Surface`: `core_surface` inline, as in
/// `rustty_apprt::embedded::Surface`.
pub struct Embedded {
    pub size: (u32, u32),
    pub title: Option<String>,
    pub core_surface: Core,
}

/// The embedded `App`.
pub struct RtApp {
    pub cb: unsafe fn(*const RtApp, TargetC, Action) -> bool,
}

/// `rustty_target_s`: the C projection (unchanged by the fix).
#[derive(Clone, Copy)]
pub struct TargetC {
    pub surface: *mut Embedded,
}

/// SHIPPED: `Target<'s, C> { App, Surface(&'s C) }`.
pub enum TargetRef<'a> {
    #[allow(dead_code)]
    App,
    Surface(&'a Core),
}

/// FIXED: `SurfacePtr<'s, C>` — `NonNull<C>` + `PhantomData<&'s C>`.
pub struct SurfacePtr<'a> {
    ptr: NonNull<Core>,
    _life: PhantomData<&'a Core>,
}
impl Clone for SurfacePtr<'_> {
    fn clone(&self) -> Self {
        *self
    }
}
impl Copy for SurfacePtr<'_> {}

impl<'a> SurfacePtr<'a> {
    /// `Target::surface_ref(&C)`: the reference dies at return; the stored
    /// pointer carries its tag but no protector.
    pub fn from_ref(v: &'a Core) -> Self {
        SurfacePtr {
            ptr: NonNull::from(v),
            _life: PhantomData,
        }
    }
    /// `unsafe Target::surface_ptr(*const C)`: the raw root's provenance.
    pub unsafe fn from_raw(p: *const Core) -> Self {
        SurfacePtr {
            ptr: unsafe { NonNull::new_unchecked(p.cast_mut()) },
            _life: PhantomData,
        }
    }
    /// `CoreSurfaceHandle::rt_surface_ptr` through a transient `&C`.
    pub fn rt_surface_ptr(self) -> *mut Embedded {
        unsafe { (*self.ptr.as_ptr()).rt_surface }
    }
    /// NEGATIVE only: a consumer that materialises the whole `&C` and keeps it
    /// across the host call (what the fix's contract forbids).
    pub unsafe fn get(self) -> &'a Core {
        unsafe { &*self.ptr.as_ptr() }
    }
}

/// FIXED: `Target<'s, C> { App, Surface(SurfacePtr<'s, C>) }`.
pub enum TargetPtr<'a> {
    #[allow(dead_code)]
    App,
    Surface(SurfacePtr<'a>),
}

/// Which target shape the core builds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Before T3: `Target::Surface(&*this)` / `Target::Surface(&*self_)`.
    Shipped,
    /// T3: `Target::surface_ref(&*this)` / `Target::surface_ptr(self_)`.
    Fixed,
    /// NEGATIVE: the fixed target, but the embedded `perform_action` keeps a
    /// whole `&C` from it alive across the host callback.
    FixedRefHeld,
}

thread_local! {
    static MODE: Cell<Mode> = const { Cell::new(Mode::Fixed) };
}
fn mode() -> Mode {
    MODE.with(|m| m.get())
}

// ------------------------------------------------------------ embedded app

impl RtApp {
    /// SHIPPED `ApprtApp::perform_action(&self, target: Target<'_, C>, ..)`:
    /// the `&Core` inside the by-value `target` is protected for the call.
    pub fn perform_action_ref(&self, target: TargetRef<'_>, action: Action) -> bool {
        self.perform_pre_action_ref(&target, action);
        let c = match target {
            TargetRef::App => TargetC {
                surface: core::ptr::null_mut(),
            },
            TargetRef::Surface(v) => TargetC {
                surface: v.rt_surface,
            },
        };
        unsafe { (self.cb)(self, c, action) }
    }

    fn perform_pre_action_ref(&self, target: &TargetRef<'_>, action: Action) {
        if let (Action::SetTitle, TargetRef::Surface(s)) = (action, target) {
            let rt = s.rt_surface;
            unsafe { (*rt).title = Some("t".into()) };
        }
    }

    /// FIXED `perform_action(&self, target: Target<'_, C>, ..)` over the raw
    /// pointer.
    pub fn perform_action_ptr(&self, target: TargetPtr<'_>, action: Action) -> bool {
        self.perform_pre_action_ptr(&target, action);
        let c = match target {
            TargetPtr::App => TargetC {
                surface: core::ptr::null_mut(),
            },
            TargetPtr::Surface(v) => TargetC {
                surface: v.rt_surface_ptr(),
            },
        };
        if mode() == Mode::FixedRefHeld {
            // NEGATIVE: a `&Core` held across the callback and used after it.
            let held: Option<&Core> = match target {
                TargetPtr::Surface(v) => Some(unsafe { v.get() }),
                TargetPtr::App => None,
            };
            let r = unsafe { (self.cb)(self, c, action) };
            if let Some(h) = held {
                std::hint::black_box(h.size);
            }
            return r;
        }
        unsafe { (self.cb)(self, c, action) }
    }

    fn perform_pre_action_ptr(&self, target: &TargetPtr<'_>, action: Action) {
        if let (Action::SetTitle, TargetPtr::Surface(s)) = (action, target) {
            let rt = s.rt_surface_ptr();
            unsafe { (*rt).title = Some("t".into()) };
        }
    }
}

impl Embedded {
    /// `updateSize(&mut self, w, h)`: records the size, then
    /// `core_surface.sizeCallback(self.size)`.
    pub fn update_size(&mut self, w: u32, h: u32) {
        if self.size == (w, h) {
            return;
        }
        self.size = (w, h);
        self.core_surface.size_callback(w, h);
    }
}

impl Core {
    /// `sizeCallback(&mut self, size)`: writes the core surface.
    pub fn size_callback(&mut self, w: u32, h: u32) {
        self.size = (w, h);
    }
}

// --------------------------------------------------------------- the C API

/// `rustty_surface_set_size(surface, w, h)`: `(*surface).updateSize(w, h)`.
pub unsafe fn c_surface_set_size(surface: *mut Embedded, w: u32, h: u32) {
    unsafe { (*surface).update_size(w, h) }
}

/// The macOS host's action callback (`Rustty.App.swift`).
/// - `reset_window_size`: the NotificationCenter post runs `onResetWindowSize`
///   -> `returnToDefaultSize` -> `apply(to:)` -> `setFrame(display: true)`,
///   which lays the view out: `rustty_surface_set_size` on the same surface.
/// - `resize_window` (`TerminalController.resizeWindow`, 714fe9b90):
///   `setFrame(frame, display: true)`, the same re-entry.
pub unsafe fn host_action(_app: *const RtApp, target: TargetC, action: Action) -> bool {
    match action {
        Action::ResetWindowSize => {
            unsafe { c_surface_set_size(target.surface, 640, 480) };
            true
        }
        Action::ResizeWindow { width, height } => {
            unsafe { c_surface_set_size(target.surface, width, height) };
            true
        }
        Action::SetTitle => true,
    }
}

// ------------------------------------------------------------ core surface

/// `Surface::performBindingAction(self_: *mut Surface, ..)` (raw-self since
/// N442): an unprotected local reborrow `this`, dead after the dispatch.
pub unsafe fn perform_binding_action(self_: *mut Core, action: Action) -> bool {
    let this = unsafe { &mut *self_ };
    this.last_binding += 1;
    let rt_app: &RtApp = unsafe { &*this.rt_app };
    match mode() {
        // `Target::Surface(&*this)`
        Mode::Shipped => rt_app.perform_action_ref(TargetRef::Surface(&*this), action),
        // `Target::surface_ref(&*this)`
        Mode::Fixed | Mode::FixedRefHeld => {
            rt_app.perform_action_ptr(TargetPtr::Surface(SurfacePtr::from_ref(&*this)), action)
        }
    }
}

/// `Surface::keyCallback(self_: *mut Surface, ..)`: binding dispatch, then the
/// surface is re-read by place (N442's lazy re-take).
pub unsafe fn key_callback(self_: *mut Core, action: Action) -> bool {
    if unsafe { (*self_).readonly } {
        return false;
    }
    let consumed = unsafe { perform_binding_action(self_, action) };
    let _ = unsafe { (*self_).size };
    consumed
}

/// `Surface::resizeWindow(self_: *mut Surface, size)` (raw-self since P7-W6
/// T1): by-place reads, then `perform_action`.
pub unsafe fn resize_window(self_: *mut Core, rows: u32, columns: u32) {
    if !unsafe { (*self_).vt_window_resize_allowed } {
        return;
    }
    let cell = unsafe { (*self_).cell };
    let width = columns * cell.0;
    let height = rows * cell.1;
    let rt_app: &RtApp = unsafe { &*(*self_).rt_app };
    let action = Action::ResizeWindow { width, height };
    match mode() {
        // `Target::Surface(unsafe { &*self_ })`
        Mode::Shipped => {
            rt_app.perform_action_ref(TargetRef::Surface(unsafe { &*self_ }), action);
        }
        // `unsafe { Target::surface_ptr(self_) }`
        Mode::Fixed | Mode::FixedRefHeld => {
            rt_app.perform_action_ptr(
                TargetPtr::Surface(unsafe { SurfacePtr::from_raw(self_) }),
                action,
            );
        }
    }
}

/// `Surface::handleMessage(self_: *mut Surface, msg)`: `.resize_window` goes to
/// the raw-self `resizeWindow`.
pub unsafe fn handle_message_resize_window(self_: *mut Core, rows: u32, columns: u32) {
    unsafe { resize_window(self_, rows, columns) }
}

/// `rustty_surface_key(surface, ..)`: `keyCallback(&raw mut (*surface).core_surface, ..)`.
pub unsafe fn c_surface_key(surface: *mut Embedded, action: Action) -> bool {
    unsafe { key_callback(&raw mut (*surface).core_surface, action) }
}

/// `rustty_surface_binding_action(surface, ..)`:
/// `performBindingAction(&raw mut (*ptr).core_surface, ..)`.
pub unsafe fn c_surface_binding_action(surface: *mut Embedded, action: Action) -> bool {
    unsafe { perform_binding_action(&raw mut (*surface).core_surface, action) }
}

/// `rustty_app_tick` -> `App::surfaceMessage` -> `Surface::handleMessage(&raw mut ..)`.
pub unsafe fn c_app_tick_resize_window(surface: *mut Embedded, rows: u32, columns: u32) {
    unsafe { handle_message_resize_window(&raw mut (*surface).core_surface, rows, columns) }
}

// ------------------------------------------------------------------ setup

fn new_surface(app: *const RtApp) -> *mut Embedded {
    let s = Box::into_raw(Box::new(Embedded {
        size: (800, 600),
        title: None,
        core_surface: Core {
            size: (800, 600),
            cell: (10, 20),
            vt_window_resize_allowed: true,
            readonly: false,
            last_binding: 0,
            rt_surface: core::ptr::null_mut(),
            rt_app: app,
        },
    }));
    unsafe { (*s).core_surface.rt_surface = s };
    s
}

fn run(m: Mode, f: impl FnOnce(*mut Embedded)) {
    MODE.with(|c| c.set(m));
    let app = Box::into_raw(Box::new(RtApp { cb: host_action }));
    let s = new_surface(app);
    f(s);
    unsafe {
        // The host read the resized surface back (`rustty_surface_size`).
        let _ = (*s).core_surface.size;
        drop(Box::from_raw(s));
        drop(Box::from_raw(app));
    }
}

fn key_reset(s: *mut Embedded) {
    assert!(unsafe { c_surface_key(s, Action::ResetWindowSize) });
    assert_eq!(unsafe { (*s).core_surface.size }, (640, 480));
}

fn binding_reset(s: *mut Embedded) {
    assert!(unsafe { c_surface_binding_action(s, Action::ResetWindowSize) });
    assert_eq!(unsafe { (*s).core_surface.size }, (640, 480));
}

fn tick_resize(s: *mut Embedded) {
    unsafe { c_app_tick_resize_window(s, 24, 80) };
    assert_eq!(unsafe { (*s).core_surface.size }, (800, 480));
}

fn all(s: *mut Embedded) {
    assert!(unsafe { c_surface_binding_action(s, Action::SetTitle) });
    assert!(unsafe { (*s).title.is_some() });
    key_reset(s);
    tick_resize(s);
    binding_reset(s);
    // A second key binding after the re-entries: the surface is still usable
    // through the raw roots.
    assert!(unsafe { c_surface_key(s, Action::SetTitle) });
    assert_eq!(unsafe { (*s).core_surface.last_binding }, 4);
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    match which.as_str() {
        // ---- shipped (before T3) ----
        "n456_shipped_key_reset_window_size" => run(Mode::Shipped, key_reset),
        "n456_shipped_binding_reset_window_size" => run(Mode::Shipped, binding_reset),
        "n456_shipped_resize_window" => run(Mode::Shipped, tick_resize),
        // ---- negative: the raw target, but a whole &C held across the callback ----
        "n456_fixed_ref_held_reset_window_size" => run(Mode::FixedRefHeld, key_reset),
        "n456_fixed_ref_held_resize_window" => run(Mode::FixedRefHeld, tick_resize),
        // ---- fixed (T3) ----
        "n456_fixed_key_reset_window_size" => run(Mode::Fixed, key_reset),
        "n456_fixed_binding_reset_window_size" => run(Mode::Fixed, binding_reset),
        "n456_fixed_resize_window" => run(Mode::Fixed, tick_resize),
        "n456_fixed_all" => run(Mode::Fixed, all),
        other => panic!("unknown candidate {other:?}"),
    }
}
