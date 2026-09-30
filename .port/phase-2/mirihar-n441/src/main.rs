//! N441 / N442 — the main thread's whole-core-`Surface` references. Isolated,
//! non-FFI model; see `README.md` for the candidates and results. Each
//! candidate is one executed path (Miri checks only what runs), selected by
//! argv[1].
//!
//! N441 (two threads): the main thread's `&mut Surface` callbacks
//! (`sizeCallback`, `contentScaleCallback`, key/mouse/focus/clipboard) vs the
//! render thread's locked writes to the renderer.
//!
//! N442 (one thread): a protected `&mut Surface` in `performBindingAction` (and
//! its `keyCallback` / `maybeHandleBinding` callers) across the embedder's
//! action callback, which re-enters `rustty_surface_inherited_config` →
//! `newSurfaceOptions` through the parent pointer.
//!
//! The REAL renderer mutex (`rustty_zigstd::thread::Mutex`) and the REAL
//! `rustty_renderer::latest_frame::DrawMutex` are included by `#[path]`;
//! `extern crate self as rustty_zigstd` lets `latest_frame.rs`'s own
//! `use rustty_zigstd::thread::Mutex` resolve to the included copy.

#![allow(clippy::missing_safety_doc)]

extern crate self as rustty_zigstd;

#[path = "../../../../crates/rustty-zigstd/src/thread.rs"]
#[allow(dead_code)]
pub mod thread;

#[path = "../../../../crates/rustty-renderer/src/latest_frame.rs"]
#[allow(dead_code)]
mod latest_frame;

use latest_frame::DrawMutex;
use thread::Mutex;

const ROUNDS: usize = 3;

struct P<T>(*mut T);
impl<T> Clone for P<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for P<T> {}
unsafe impl<T> Send for P<T> {}

// ===========================================================================
// N441
// ===========================================================================

/// The renderer (`rustty_renderer::Renderer`): fields the render thread writes
/// under the draw mutex (N437's fix: lock first, then the whole `&mut`).
struct Renderer {
    cells: Vec<u32>,
    uniforms: [f32; 4],
    display_realized: bool,
}

impl Renderer {
    fn new() -> Self {
        Renderer {
            cells: vec![0; 8],
            uniforms: [0.0; 4],
            display_realized: true,
        }
    }

    /// `Renderer::with_locked` (the render thread's shape after N437).
    unsafe fn with_locked<T>(
        p: *mut Renderer,
        lock: &DrawMutex,
        f: impl FnOnce(&mut Renderer) -> T,
    ) -> T {
        lock.lock();
        let out = f(unsafe { &mut *p });
        lock.unlock();
        out
    }

    fn update_frame_locked(&mut self) {
        self.cells[0] += 1;
        std::thread::yield_now();
        self.uniforms[0] += 1.0;
    }

    /// `display_realized_raw`: the main thread's narrow, locked write.
    unsafe fn display_realized_raw(p: *mut Renderer, lock: &DrawMutex) {
        lock.lock();
        unsafe { (*p).display_realized = true };
        lock.unlock();
    }
}

/// The render thread's rounds: every renderer write under the draw mutex,
/// through the raw pointer it was handed at `Thread.init`.
fn render_thread(r: P<Renderer>, lock: DrawMutex) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let r = r;
        for _ in 0..ROUNDS {
            unsafe { Renderer::with_locked(r.0, &lock, |r| r.update_frame_locked()) };
            std::thread::yield_now();
        }
    })
}

/// SHIPPED (pre-fix): the renderer inline in the core `Surface` allocation.
struct SurfaceInline {
    size: (u32, u32),
    content_scale: f32,
    focused: bool,
    renderer: Renderer,
    mouse_mods: u8,
}

impl SurfaceInline {
    /// `sizeCallback(&mut self, ..)` — the GTK `render_surface_resize` /
    /// embedded `rustty_surface_set_size` entry; never touches the renderer.
    fn size_callback(&mut self, w: u32, h: u32) {
        self.size = (w, h);
        std::thread::yield_now();
    }

    /// `contentScaleCallback(&mut self, ..)`.
    fn content_scale_callback(&mut self, s: f32) {
        self.content_scale = s;
    }

    /// `focusCallback` / `keyCallback` / mouse callbacks: whole `&mut self`.
    fn focus_callback(&mut self, f: bool) {
        self.focused = f;
        self.mouse_mods = 0;
    }
}

fn run_inline() {
    let s = P(Box::into_raw(Box::new(SurfaceInline {
        size: (0, 0),
        content_scale: 1.0,
        focused: true,
        renderer: Renderer::new(),
        mouse_mods: 0,
    })));
    let lock = DrawMutex::new();
    // `Thread.init(.., &self.renderer, ..)`: a raw pointer into the surface.
    let r = P(unsafe { &raw mut (*s.0).renderer });
    let t = render_thread(r, lock.clone());
    for i in 0..ROUNDS as u32 {
        // `unsafe { &mut *core }.contentScaleCallback(..)` / `.sizeCallback(..)`.
        unsafe { &mut *s.0 }.content_scale_callback(2.0);
        unsafe { &mut *s.0 }.size_callback(i, i);
        unsafe { &mut *s.0 }.focus_callback(i % 2 == 0);
        std::thread::yield_now();
    }
    t.join().unwrap();
    drop(unsafe { Box::from_raw(s.0) });
}

/// FIXED: the renderer in its own allocation; the surface holds the raw
/// pointer, which the render thread shares. A whole-`Surface` retag covers the
/// pointer's bytes only.
struct SurfaceBoxed {
    size: (u32, u32),
    content_scale: f32,
    focused: bool,
    renderer: *mut Renderer,
    mouse_mods: u8,
}

impl SurfaceBoxed {
    fn size_callback(&mut self, w: u32, h: u32) {
        self.size = (w, h);
        std::thread::yield_now();
    }

    fn content_scale_callback(&mut self, s: f32) {
        self.content_scale = s;
    }

    fn focus_callback(&mut self, f: bool) {
        self.focused = f;
        self.mouse_mods = 0;
    }

    /// A main-thread entry that reaches the renderer (`displayRealized`,
    /// the sync `drawFrame`): through the pointer, locked, narrow.
    fn display_realized(&mut self, lock: &DrawMutex) {
        unsafe { Renderer::display_realized_raw(self.renderer, lock) };
    }
}

fn run_boxed() {
    let r = P(Box::into_raw(Box::new(Renderer::new())));
    let s = P(Box::into_raw(Box::new(SurfaceBoxed {
        size: (0, 0),
        content_scale: 1.0,
        focused: true,
        renderer: r.0,
        mouse_mods: 0,
    })));
    let main_lock = DrawMutex::new();
    let t = render_thread(r, main_lock.clone());
    for i in 0..ROUNDS as u32 {
        unsafe { &mut *s.0 }.content_scale_callback(2.0);
        unsafe { &mut *s.0 }.size_callback(i, i);
        unsafe { &mut *s.0 }.focus_callback(i % 2 == 0);
        unsafe { &mut *s.0 }.display_realized(&main_lock);
        std::thread::yield_now();
    }
    t.join().unwrap();
    // `Surface.deinit`: the render thread is joined, then the renderer freed.
    let s = unsafe { Box::from_raw(s.0) };
    drop(unsafe { Box::from_raw(s.renderer) });
}

// ---------------------------------------------------------------------------
// The same class beyond the renderer (NEW finding, not N441's scope): the
// terminal sits inline in `Surface.io` and the io thread writes it under
// `renderer_state.mutex` (itself out of line, `*mut Mutex`).
// ---------------------------------------------------------------------------
struct Terminal {
    rows: Vec<u32>,
    cursor: u32,
}

struct SurfaceTermInline {
    size: (u32, u32),
    terminal: Terminal,
    state_mutex: *mut Mutex,
}

impl SurfaceTermInline {
    fn size_callback(&mut self, w: u32, h: u32) {
        self.size = (w, h);
        std::thread::yield_now();
    }
}

struct SurfaceTermBoxed {
    size: (u32, u32),
    terminal: *mut Terminal,
    state_mutex: *mut Mutex,
}

impl SurfaceTermBoxed {
    fn size_callback(&mut self, w: u32, h: u32) {
        self.size = (w, h);
        std::thread::yield_now();
    }
}

fn io_thread(t: P<Terminal>, m: P<Mutex>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let (t, m) = (t, m);
        for _ in 0..ROUNDS {
            unsafe {
                (*m.0).lock();
                (*t.0).cursor += 1;
                (&mut (*t.0).rows)[0] += 1;
                (*m.0).unlock();
            }
            std::thread::yield_now();
        }
    })
}

fn run_terminal_inline() {
    let m = P(Box::into_raw(Box::new(Mutex::new())));
    let s = P(Box::into_raw(Box::new(SurfaceTermInline {
        size: (0, 0),
        terminal: Terminal {
            rows: vec![0; 4],
            cursor: 0,
        },
        state_mutex: m.0,
    })));
    let t = io_thread(P(unsafe { &raw mut (*s.0).terminal }), m);
    for i in 0..ROUNDS as u32 {
        unsafe { &mut *s.0 }.size_callback(i, i);
        std::thread::yield_now();
    }
    t.join().unwrap();
    let s = unsafe { Box::from_raw(s.0) };
    drop(unsafe { Box::from_raw(s.state_mutex) });
}

fn run_terminal_boxed() {
    let m = P(Box::into_raw(Box::new(Mutex::new())));
    let term = P(Box::into_raw(Box::new(Terminal {
        rows: vec![0; 4],
        cursor: 0,
    })));
    let s = P(Box::into_raw(Box::new(SurfaceTermBoxed {
        size: (0, 0),
        terminal: term.0,
        state_mutex: m.0,
    })));
    let t = io_thread(term, m);
    for i in 0..ROUNDS as u32 {
        unsafe { &mut *s.0 }.size_callback(i, i);
        std::thread::yield_now();
    }
    t.join().unwrap();
    let s = unsafe { Box::from_raw(s.0) };
    drop(unsafe { Box::from_raw(s.terminal) });
    drop(unsafe { Box::from_raw(s.state_mutex) });
}

// ===========================================================================
// N442
// ===========================================================================
mod n442 {
    use std::cell::UnsafeCell;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Action {
        NewSplit,
        NewTab,
        NewWindow,
        /// Any arm that does not call `rt_app.perform_action` with a
        /// re-entrant host callback (`scroll_to_top`, `copy_to_clipboard`, ..).
        Other,
        /// N443: the app-scoped `quit` (`App::performAction`).
        Quit,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Context {
        Split,
        Tab,
        Window,
    }

    /// The core `App` (`rustty_core::App`).
    pub struct CoreApp {
        pub alloc: u64,
        pub surfaces: Vec<*mut Core>,
    }

    /// `apprt.Target`: `Target::Surface(&C)` is a shared reference, as in the
    /// port (`apprt::Target<'_, C>`).
    pub enum Target<'a> {
        App,
        Surface(&'a Core),
    }

    /// The embedded `App`: the C action callback and the core app pointer.
    pub struct RtApp {
        pub core_app: *mut CoreApp,
        pub cb: unsafe fn(*const RtApp, *mut Embedded, Action) -> bool,
    }

    impl RtApp {
        /// `perform_action(&self, target, action)`: `target.cval()` is the
        /// core surface's `rt_surface` (the embedded surface pointer).
        pub fn perform_action(&self, target: Target<'_>, action: Action) -> bool {
            let t = match target {
                Target::App => core::ptr::null_mut(),
                Target::Surface(c) => c.rt_surface,
            };
            unsafe { (self.cb)(self, t, action) }
        }
    }

    impl CoreApp {
        /// SHIPPED `App::newWindow(&mut self, ..)`: reads only.
        pub fn new_window_mut(&mut self, rt_app: &RtApp, parent: Option<*mut Core>) {
            let target = match parent {
                Some(p) if self.surfaces.contains(&p) => Target::Surface(unsafe { &*p }),
                _ => Target::App,
            };
            let _ = rt_app.perform_action(target, Action::NewWindow);
        }

        /// `App::needsConfirmQuit(&self)`.
        pub fn needs_confirm_quit(&self) -> bool {
            !self.surfaces.is_empty() && self.alloc == 0
        }

        /// SHIPPED (N443, OPEN) `App::performAction(&mut self, ..)`'s `.quit`
        /// arm.
        pub fn perform_action_quit_mut(&mut self, rt_app: &RtApp) {
            let _ = rt_app.perform_action(Target::App, Action::Quit);
        }

        /// N443 fix direction: the `.quit` arm without a protected `&mut App`.
        pub fn perform_action_quit_ref(&self, rt_app: &RtApp) {
            let _ = rt_app.perform_action(Target::App, Action::Quit);
        }

        /// FIXED `App::newWindow(&self, ..)`.
        pub fn new_window_ref(&self, rt_app: &RtApp, parent: Option<*mut Core>) {
            let target = match parent {
                Some(p) if self.surfaces.contains(&p) => Target::Surface(unsafe { &*p }),
                _ => Target::App,
            };
            let _ = rt_app.perform_action(target, Action::NewWindow);
        }
    }

    /// The core `Surface`.
    pub struct Core {
        pub font_size: f32,
        pub pwd: Vec<u8>,
        pub last_trigger: Option<u64>,
        pub sequence_set: Option<u32>,
        pub size: (u32, u32),
        pub pressed_key: Option<u32>,
        pub rt_app: *const RtApp,
        pub app: *mut CoreApp,
        pub rt_surface: *mut Embedded,
    }

    /// The embedded `Surface`: `core_surface` inline, the handed-out working
    /// directory in an `UnsafeCell` (written through `&self`).
    pub struct Embedded {
        pub app: *const RtApp,
        pub core_surface: Core,
        pub inherited_working_directory: UnsafeCell<Option<Vec<u8>>>,
    }

    impl Embedded {
        /// `newSurfaceOptions(&self, context)`.
        pub fn new_surface_options(&self, context: Context) -> f32 {
            let font_size = self.core_surface.font_size;
            if context == Context::Window || context == Context::Tab {
                // `self.core_surface.pwd(self.app.core_app.alloc)`
                let _alloc = unsafe { (*(*self.app).core_app).alloc };
                let cwd = self.core_surface.pwd.clone();
                unsafe { *self.inherited_working_directory.get() = Some(cwd) };
            }
            font_size
        }
    }

    /// `rustty_app_needs_confirm_quit(app)`: `(*core_app).needsConfirmQuit()`.
    pub unsafe fn c_app_needs_confirm_quit(app: *const RtApp) -> bool {
        unsafe { (*(*app).core_app).needs_confirm_quit() }
    }

    /// `rustty_surface_inherited_config(surface, source)`.
    pub unsafe fn c_inherited_config(surface: *mut Embedded, source: Context) -> f32 {
        unsafe { (*surface).new_surface_options(source) }
    }

    /// The macOS host's action callback (`Rustty.App.swift` `newSplit`,
    /// `newTab`, `newWindow`): re-enters the target surface synchronously.
    pub unsafe fn host_action(app: *const RtApp, surface: *mut Embedded, action: Action) -> bool {
        if action == Action::Quit {
            // `NSApplication.terminate` -> `applicationShouldTerminate` ->
            // `rustty_app_needs_confirm_quit(app)` -> `core_app.needsConfirmQuit()`.
            let _ = unsafe { c_app_needs_confirm_quit(app) };
            return true;
        }
        if surface.is_null() {
            return true;
        }
        let ctx = match action {
            Action::NewSplit => Context::Split,
            Action::NewTab => Context::Tab,
            Action::NewWindow => Context::Window,
            Action::Other | Action::Quit => return true,
        };
        let _ = unsafe { c_inherited_config(surface, ctx) };
        true
    }

    // ---------------------------------------------------------------- SHIPPED
    impl Core {
        /// `keyCallback(&mut self, ..)`.
        pub fn key_callback(&mut self, key: u32) -> bool {
            self.pressed_key = None;
            if let Some(v) = self.maybe_handle_binding(key) {
                return v;
            }
            self.pressed_key = Some(key);
            false
        }

        /// `maybeHandleBinding(&mut self, ..)`.
        pub fn maybe_handle_binding(&mut self, key: u32) -> Option<bool> {
            let action = action_for(key)?;
            self.last_trigger = None;
            self.sequence_set = None;
            let performed = self.perform_binding_action(action);
            if performed {
                self.last_trigger = Some(u64::from(key));
            }
            Some(performed)
        }

        /// `performBindingAction(&mut self, ..)`.
        pub fn perform_binding_action(&mut self, action: Action) -> bool {
            match action {
                Action::NewWindow => {
                    let self_ptr: *mut Core = self;
                    let rt_app = unsafe { &*self.rt_app };
                    unsafe { &mut *self.app }.new_window_mut(rt_app, Some(self_ptr));
                    true
                }
                Action::NewTab => {
                    unsafe { &*self.rt_app }.perform_action(Target::Surface(&*self), Action::NewTab)
                }
                Action::NewSplit => {
                    let _right = self.size.0 > self.size.1;
                    unsafe { &*self.rt_app }
                        .perform_action(Target::Surface(&*self), Action::NewSplit)
                }
                Action::Other | Action::Quit => {
                    self.size.0 += 1;
                    true
                }
            }
        }
    }

    pub fn action_for(key: u32) -> Option<Action> {
        match key {
            1 => Some(Action::NewSplit),
            2 => Some(Action::NewTab),
            3 => Some(Action::NewWindow),
            4 => Some(Action::Other),
            _ => None,
        }
    }

    // ------------------------------------------------------------------ FIXED
    // Raw-self dispatch: `*mut Core` receivers; each stretch of work takes a
    // fresh, UNPROTECTED local `&mut` that is dead before the next call that
    // can reach the host callback; the re-entrant arms touch the surface only
    // through place expressions on the raw pointer and a shared `Target`.

    /// `Surface::keyCallback(self_: *mut Self, ..)`.
    pub unsafe fn key_callback_raw(self_: *mut Core, key: u32) -> bool {
        {
            let this = unsafe { &mut *self_ };
            this.pressed_key = None;
        }
        if let Some(v) = unsafe { maybe_handle_binding_raw(self_, key) } {
            return v;
        }
        let this = unsafe { &mut *self_ };
        this.pressed_key = Some(key);
        false
    }

    /// `Surface::maybeHandleBinding(self_: *mut Self, ..)`.
    pub unsafe fn maybe_handle_binding_raw(self_: *mut Core, key: u32) -> Option<bool> {
        let action = action_for(key)?;
        {
            let this = unsafe { &mut *self_ };
            this.last_trigger = None;
            this.sequence_set = None;
        }
        let performed = unsafe { perform_binding_action_raw(self_, action) };
        let this = unsafe { &mut *self_ };
        if performed {
            this.last_trigger = Some(u64::from(key));
        }
        Some(performed)
    }

    /// `Surface::performBindingAction(self_: *mut Self, ..)`: the body keeps a
    /// local `this` for the non-re-entrant arms; the re-entrant arms never use
    /// it (a local `&mut` is not protected, and it is dead after them).
    pub unsafe fn perform_binding_action_raw(self_: *mut Core, action: Action) -> bool {
        let this = unsafe { &mut *self_ };
        match action {
            Action::NewWindow => unsafe {
                (*(*self_).app).new_window_ref(&*(*self_).rt_app, Some(self_));
                true
            },
            Action::NewTab => unsafe {
                (*(*self_).rt_app).perform_action(Target::Surface(&*self_), Action::NewTab)
            },
            Action::NewSplit => unsafe {
                let _right = (*self_).size.0 > (*self_).size.1;
                (*(*self_).rt_app).perform_action(Target::Surface(&*self_), Action::NewSplit)
            },
            Action::Other | Action::Quit => {
                this.size.0 += 1;
                true
            }
        }
    }

    /// NEGATIVE: raw-self, but the local `&mut` written before the callback
    /// is used again after it.
    pub unsafe fn perform_binding_action_this_spans(self_: *mut Core, action: Action) -> bool {
        let this = unsafe { &mut *self_ };
        this.sequence_set = None;
        let v = unsafe { (*(*self_).rt_app).perform_action(Target::Surface(&*self_), action) };
        this.sequence_set = Some(1);
        v
    }

    /// Build one app + embedded surface as `rustty_app_new` +
    /// `rustty_surface_new` do (the C host holds the raw roots).
    pub struct World {
        pub core_app: *mut CoreApp,
        pub rt_app: *mut RtApp,
        pub surface: *mut Embedded,
    }

    impl World {
        pub fn new() -> World {
            let core_app = Box::into_raw(Box::new(CoreApp {
                alloc: 7,
                surfaces: Vec::new(),
            }));
            let rt_app = Box::into_raw(Box::new(RtApp {
                core_app,
                cb: host_action,
            }));
            let surface = Box::into_raw(Box::new(Embedded {
                app: rt_app,
                core_surface: Core {
                    font_size: 13.0,
                    pwd: b"/tmp".to_vec(),
                    last_trigger: None,
                    sequence_set: None,
                    size: (800, 600),
                    pressed_key: None,
                    rt_app,
                    app: core_app,
                    rt_surface: core::ptr::null_mut(),
                },
                inherited_working_directory: UnsafeCell::new(None),
            }));
            unsafe {
                (*surface).core_surface.rt_surface = surface;
                (*core_app).surfaces.push(&raw mut (*surface).core_surface);
            }
            World {
                core_app,
                rt_app,
                surface,
            }
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            unsafe {
                drop(Box::from_raw(self.surface));
                drop(Box::from_raw(self.rt_app));
                drop(Box::from_raw(self.core_app));
            }
        }
    }

    // --- the C routes -------------------------------------------------------

    /// SHIPPED `rustty_surface_key` → `App::keyEvent` → `keyCallback`.
    pub unsafe fn c_surface_key(surface: *mut Embedded, key: u32) -> bool {
        unsafe { (*surface).core_surface.key_callback(key) }
    }

    /// SHIPPED `rustty_surface_binding_action` / `rustty_surface_split`.
    pub unsafe fn c_surface_binding_action(ptr: *mut Embedded, action: Action) -> bool {
        unsafe { (*ptr).core_surface.perform_binding_action(action) }
    }

    /// FIXED `rustty_surface_key`.
    pub unsafe fn c_surface_key_raw(surface: *mut Embedded, key: u32) -> bool {
        unsafe { key_callback_raw(&raw mut (*surface).core_surface, key) }
    }

    /// FIXED `rustty_surface_binding_action` / `_split` / `_split_focus` /
    /// `_request_close`.
    pub unsafe fn c_surface_binding_action_raw(ptr: *mut Embedded, action: Action) -> bool {
        unsafe { perform_binding_action_raw(&raw mut (*ptr).core_surface, action) }
    }
}

fn n442_all(f: unsafe fn(*mut n442::Embedded, u32) -> bool) {
    use n442::World;
    for key in [4, 1, 2, 3] {
        let w = World::new();
        unsafe { f(w.surface, key) };
        drop(w);
    }
}

fn main() {
    let which = std::env::args().nth(1).expect("candidate name");
    match which.as_str() {
        // ---- N441 ----
        // UB expected: whole-`&mut Surface` main-thread callbacks over the
        // inline renderer vs the render thread's locked writes.
        "n441_inline_renderer" => run_inline(),
        // CLEAN expected: the renderer out of the `Surface` allocation.
        "n441_boxed_renderer" => run_boxed(),
        // UB expected (residual class, a NEW finding): the inline terminal vs
        // the io thread's locked writes.
        "n441_residual_terminal_inline" => run_terminal_inline(),
        // CLEAN expected: the same shape of fix applied to the terminal.
        "n441_residual_terminal_boxed" => run_terminal_boxed(),

        // ---- N442 ----
        // UB expected: the shipped protected `&mut` routes.
        "n442_shipped_key_route" => n442_all(n442::c_surface_key),
        "n442_shipped_binding_new_split" => {
            let w = n442::World::new();
            unsafe { n442::c_surface_binding_action(w.surface, n442::Action::NewSplit) };
        }
        "n442_shipped_binding_new_tab" => {
            let w = n442::World::new();
            unsafe { n442::c_surface_binding_action(w.surface, n442::Action::NewTab) };
        }
        "n442_shipped_binding_new_window" => {
            let w = n442::World::new();
            unsafe { n442::c_surface_binding_action(w.surface, n442::Action::NewWindow) };
        }
        // UB expected: the new-window arm raw-self, but `App::newWindow` still
        // `&mut self` while the host reads `core_app.alloc`.
        "n442_new_window_app_mut" => {
            let w = n442::World::new();
            unsafe {
                let s: *mut n442::Core = &raw mut (*w.surface).core_surface;
                (*(*s).app).new_window_mut(&*(*s).rt_app, Some(s));
            }
        }
        // UB expected: raw-self, but a local `&mut` spans the callback.
        "n442_raw_this_spans" => {
            let w = n442::World::new();
            unsafe {
                n442::perform_binding_action_this_spans(
                    &raw mut (*w.surface).core_surface,
                    n442::Action::NewSplit,
                )
            };
        }
        // N443 (OPEN, recorded here): UB expected under SB with the shipped
        // `&mut App` quit arm; CLEAN with the `&App` direction.
        "n443_quit_app_mut" => {
            let w = n442::World::new();
            unsafe { (*w.core_app).perform_action_quit_mut(&*w.rt_app) };
        }
        "n443_quit_app_ref" => {
            let w = n442::World::new();
            unsafe { (*w.core_app).perform_action_quit_ref(&*w.rt_app) };
        }
        // CLEAN expected: the fix, every arm, both route shapes.
        "n442_raw_key_route" => n442_all(n442::c_surface_key_raw),
        "n442_raw_binding_all" => {
            for a in [
                n442::Action::Other,
                n442::Action::NewSplit,
                n442::Action::NewTab,
                n442::Action::NewWindow,
            ] {
                let w = n442::World::new();
                unsafe { n442::c_surface_binding_action_raw(w.surface, a) };
            }
        }
        other => panic!("unknown candidate {other}"),
    }
}
