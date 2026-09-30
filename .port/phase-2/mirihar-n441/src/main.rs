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

// ---------------------------------------------------------------------------
// N441 fix round 1: freeing the out-of-line renderer. Every owner frees the
// core `Surface` WITHOUT drop glue (embedded `closeSurface` via
// `ManuallyDrop`, GTK via `dealloc`, init-error callers via the `MaybeUninit`
// slot), so a `Drop` on the box never runs: a leak (Miri's leak check). The fix
// frees the box explicitly right after `deinit`, also without drop glue.
// ---------------------------------------------------------------------------
struct RendererBoxDrop(*mut Renderer);
impl Drop for RendererBoxDrop {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.0) });
    }
}

struct RendererBoxExplicit(*mut Renderer);
impl RendererBoxExplicit {
    unsafe fn dealloc_after_deinit(&self) {
        drop(unsafe { Box::from_raw(self.0.cast::<std::mem::ManuallyDrop<Renderer>>()) });
    }
}

/// `Renderer::deinit`: tears down what the renderer owns (the port frees the
/// owned buffers explicitly; drop glue afterwards would free them twice).
unsafe fn renderer_deinit(r: *mut Renderer) {
    unsafe {
        let cells = std::ptr::read(&raw const (*r).cells);
        drop(cells);
    }
}

struct SurfaceWithDropBox {
    size: (u32, u32),
    renderer: RendererBoxDrop,
}

struct SurfaceWithExplicitBox {
    size: (u32, u32),
    renderer: RendererBoxExplicit,
}

/// Round 0: `Surface::deinit` runs `renderer.deinit()`, then the owner frees
/// the surface through `ManuallyDrop` (embedded `closeSurface`).
fn run_box_drop_leak() {
    let s = Box::into_raw(Box::new(SurfaceWithDropBox {
        size: (1, 1),
        renderer: RendererBoxDrop(Box::into_raw(Box::new(Renderer::new()))),
    }));
    unsafe {
        (*s).size = (2, 2);
        renderer_deinit((*s).renderer.0);
        drop(Box::from_raw(
            s.cast::<std::mem::ManuallyDrop<SurfaceWithDropBox>>(),
        ));
    }
}

/// Fix round 1: `deinit` then `dealloc_after_deinit`; the owner's
/// `ManuallyDrop` free is unchanged.
fn run_box_dealloc_after_deinit() {
    let s = Box::into_raw(Box::new(SurfaceWithExplicitBox {
        size: (1, 1),
        renderer: RendererBoxExplicit(Box::into_raw(Box::new(Renderer::new()))),
    }));
    unsafe {
        (*s).size = (2, 2);
        renderer_deinit((*s).renderer.0);
        (*s).renderer.dealloc_after_deinit();
        drop(Box::from_raw(
            s.cast::<std::mem::ManuallyDrop<SurfaceWithExplicitBox>>(),
        ));
    }
}

// ---------------------------------------------------------------------------
// N441 fix round 2 (ruling 46): the backend teardown. The renderer's fields that
// only `Drop` frees (the Metal backend with its display-callback context, the
// search matches, the `Arc`s) and the layer the host view keeps alive with the
// callback pointing into the renderer. Round 1 freed the renderer WITHOUT drop
// glue and never cleared the callback: those fields leak, and a later display
// pass on the view-retained layer runs the callback on the freed renderer. Round
// 2: `Renderer::deinit` calls `Metal::deinit` (clear the callback, free its
// context, as the reference's `layer.release()`), then the owner drops the
// renderer normally.
// ---------------------------------------------------------------------------
mod backend {
    use std::cell::Cell;
    use std::rc::Rc;

    /// The CALayer the host view retains: a display callback + context.
    pub struct Layer {
        pub cb: Cell<Option<unsafe fn(*mut Ctx)>>,
        pub ctx: Cell<*mut Ctx>,
    }

    impl Layer {
        /// Core Animation's display pass.
        pub fn display(&self) {
            if let Some(cb) = self.cb.get() {
                unsafe { cb(self.ctx.get()) };
            }
        }
    }

    /// `DisplayCallbackCtx { renderer, lock }`.
    pub struct Ctx {
        pub renderer: *mut Renderer,
    }

    /// `Metal`: the layer reference and the callback context.
    pub struct Metal {
        pub layer: Rc<Layer>,
        pub display_ctx: *mut Ctx,
    }

    impl Metal {
        /// Round 2 `Metal::deinit`: clear the callback, free the context.
        pub fn deinit(&mut self) {
            self.layer.cb.set(None);
            self.layer.ctx.set(core::ptr::null_mut());
            if !self.display_ctx.is_null() {
                drop(unsafe { Box::from_raw(self.display_ctx) });
                self.display_ctx = core::ptr::null_mut();
            }
        }
    }

    impl Drop for Metal {
        fn drop(&mut self) {
            self.deinit();
        }
    }

    #[allow(dead_code)]
    pub struct Renderer {
        pub api: Metal,
        pub cells: Vec<u32>,
        /// `search_matches`: freed only by `Drop` (the reference frees its arena
        /// in `deinit`; the port's `Option` drops).
        pub search_matches: Option<Vec<u32>>,
        pub latest_frame: std::sync::Arc<u32>,
        pub drawn: u32,
    }

    /// `display_callback` → `draw_frame_raw(renderer, sync = true)`.
    pub unsafe fn display_callback(ctx: *mut Ctx) {
        unsafe {
            let r = (*ctx).renderer;
            (*r).drawn += 1;
        }
    }

    /// `Renderer::init` + `loopEnter` (the render thread installs the callback).
    pub fn new(layer: &Rc<Layer>) -> *mut Renderer {
        let r = Box::into_raw(Box::new(Renderer {
            api: Metal {
                layer: layer.clone(),
                display_ctx: core::ptr::null_mut(),
            },
            cells: vec![0; 4],
            search_matches: Some(vec![1, 2, 3]),
            latest_frame: std::sync::Arc::new(0),
            drawn: 0,
        }));
        let ctx = Box::into_raw(Box::new(Ctx { renderer: r }));
        unsafe {
            (*r).api.display_ctx = ctx;
            layer.cb.set(Some(display_callback));
            layer.ctx.set(ctx);
        }
        r
    }

    /// `Renderer::deinit` without `api.deinit()` (round 1): the explicit
    /// teardown of what `deinit` owns.
    pub unsafe fn renderer_deinit_round1(r: *mut Renderer) {
        unsafe { (*r).cells = Vec::new() };
    }

    /// `Renderer::deinit` with `api.deinit()` (round 2).
    pub unsafe fn renderer_deinit_round2(r: *mut Renderer) {
        unsafe {
            (*r).cells = Vec::new();
            (*r).api.deinit();
        }
    }

    pub fn host_layer() -> Rc<Layer> {
        Rc::new(Layer {
            cb: Cell::new(None),
            ctx: Cell::new(core::ptr::null_mut()),
        })
    }
}

/// Round 1: `deinit`, then the box freed as `ManuallyDrop` (no glue).
fn run_r1_backend(display_after: bool) {
    let layer = backend::host_layer();
    let r = backend::new(&layer);
    unsafe {
        backend::renderer_deinit_round1(r);
        drop(Box::from_raw(
            r.cast::<std::mem::ManuallyDrop<backend::Renderer>>(),
        ));
    }
    if display_after {
        // A resize / display pass on the view-retained layer.
        layer.display();
    }
}

/// Round 2: `deinit` (with `Metal::deinit`), then the renderer dropped normally.
fn run_r2_backend() {
    let layer = backend::host_layer();
    let r = backend::new(&layer);
    unsafe {
        backend::renderer_deinit_round2(r);
        drop(Box::from_raw(r));
    }
    layer.display();
}

// ===========================================================================
// N442 / N443 / N448
// ===========================================================================
mod n442 {
    use std::cell::{Cell, UnsafeCell};

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Action {
        NewSplit,
        NewTab,
        NewWindow,
        /// App-scoped `reload_config`.
        ReloadConfig,
        /// App-scoped `quit`.
        Quit,
        /// Any arm whose host callback re-enters nothing.
        Other,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Context {
        Split,
        Tab,
        Window,
    }

    /// Which code the model runs.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Mode {
        /// Before P7-W7 T5b: `&mut self` everywhere.
        Shipped,
        /// T5b round 0: the surface entries raw-self; the app entries still
        /// references (`App::newWindow(&self)`, `App::performAction(&mut
        /// self)`, embedded `keyEvent(&self)` + `newSurface(&mut self)`).
        Round0,
        /// T5b fix round 1: the app entries raw-self too.
        Fixed,
    }

    thread_local! {
        pub static MODE: Cell<Mode> = const { Cell::new(Mode::Fixed) };
        /// The host's own app pointer (`Rustty.App.app`, the `rustty_app_new`
        /// result). The host passes THIS to `rustty_surface_new`,
        /// `rustty_app_update_config` and `rustty_app_needs_confirm_quit`, not
        /// the callback's `app` argument (which it only hands to
        /// `rustty_app_userdata`).
        pub static ROOT: Cell<*mut RtApp> = const { Cell::new(core::ptr::null_mut()) };
    }
    fn mode() -> Mode {
        MODE.with(|m| m.get())
    }

    // ------------------------------------------------------------------ types

    /// The core `App`.
    pub struct CoreApp {
        pub alloc: u64,
        pub focused: bool,
        pub theme: u8,
        pub surfaces: Vec<*mut Embedded>,
    }

    /// `apprt.Target`: the surface arm holds a shared reference, as the port's
    /// `apprt::Target<'_, C>` does.
    pub enum Target<'a> {
        App,
        Surface(&'a Core),
    }

    /// The embedded `App`: plain fields plus an `UnsafeCell` config (written
    /// through `&self` by `config_change`).
    pub struct RtApp {
        pub core_app: *mut CoreApp,
        pub cb: unsafe fn(*const RtApp, *mut Embedded, Action) -> bool,
        pub config: UnsafeCell<u32>,
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

    /// The embedded `Surface`: `core_surface` inline.
    pub struct Embedded {
        pub app: *mut RtApp,
        pub core_surface: Core,
        pub inherited_working_directory: UnsafeCell<Option<Vec<u8>>>,
    }

    // ----------------------------------------------------------- embedded app

    impl RtApp {
        /// `ApprtApp::perform_action(&self, ..)` (a trait method: `&self`).
        pub fn perform_action(&self, target: Target<'_>, action: Action) -> bool {
            let t = match target {
                Target::App => core::ptr::null_mut(),
                Target::Surface(c) => c.rt_surface,
            };
            unsafe { (self.cb)(self, t, action) }
        }

        /// SHIPPED / ROUND 0 `newSurface(&mut self, ..)`.
        pub fn new_surface_mut(&mut self) -> *mut Embedded {
            let app: *mut RtApp = self;
            unsafe { new_surface_body(app) }
        }

        /// FIXED `newSurface(self_: *mut App, ..)`.
        pub unsafe fn new_surface_raw(app: *mut RtApp) -> *mut Embedded {
            unsafe { new_surface_body(app) }
        }

        /// SHIPPED / ROUND 0 `keyEvent(&self, target, ..)`.
        pub fn key_event_ref(&self, surface: *mut Embedded, key: u32) -> bool {
            if surface.is_null() {
                return unsafe { (*self.core_app).key_event_mut(self, key) };
            }
            unsafe { surface_key_callback(&raw mut (*surface).core_surface, key) }
        }

        /// FIXED `keyEvent(self_: *const App, target, ..)`.
        pub unsafe fn key_event_raw(app: *const RtApp, surface: *mut Embedded, key: u32) -> bool {
            if surface.is_null() {
                return unsafe { CoreApp::key_event_raw((*app).core_app, &*app, key) };
            }
            unsafe { surface_key_callback(&raw mut (*surface).core_surface, key) }
        }
    }

    /// `newSurface`'s body: allocate, `Surface.init` (`app.core_app.addSurface`).
    unsafe fn new_surface_body(app: *mut RtApp) -> *mut Embedded {
        let core_app = unsafe { (*app).core_app };
        let s = Box::into_raw(Box::new(Embedded {
            app,
            core_surface: Core {
                font_size: 13.0,
                pwd: b"/tmp".to_vec(),
                last_trigger: None,
                sequence_set: None,
                size: (800, 600),
                pressed_key: None,
                rt_app: app,
                app: core_app,
                rt_surface: core::ptr::null_mut(),
            },
            inherited_working_directory: UnsafeCell::new(None),
        }));
        unsafe {
            (*s).core_surface.rt_surface = s;
            // `try app.core_app.addSurface(self)`: `&mut CoreApp`, a push.
            (*core_app).add_surface(s);
        }
        s
    }

    impl Embedded {
        /// `newSurfaceOptions(&self, context)`.
        pub fn new_surface_options(&self, context: Context) -> f32 {
            let font_size = self.core_surface.font_size;
            if context == Context::Window || context == Context::Tab {
                let _alloc = unsafe { (*(*self.app).core_app).alloc };
                let cwd = self.core_surface.pwd.clone();
                unsafe { *self.inherited_working_directory.get() = Some(cwd) };
            }
            font_size
        }
    }

    // --------------------------------------------------------------- core app

    impl CoreApp {
        pub fn add_surface(&mut self, s: *mut Embedded) {
            self.surfaces.push(s);
        }

        pub fn needs_confirm_quit(&self) -> bool {
            !self.surfaces.is_empty() && self.alloc == 0
        }

        /// `updateConfig(&mut self, rt_app, config)`: every surface gets
        /// `change_config` (writes it), then the app's own state.
        pub fn update_config(&mut self, rt_app: &RtApp) {
            for &s in self.surfaces.iter() {
                unsafe { (*s).core_surface.font_size += 1.0 };
            }
            self.theme = self.theme.wrapping_add(1);
            unsafe { *rt_app.config.get() += 1 };
        }

        /// SHIPPED / ROUND 0 `keyEvent(&mut self, rt_app, ..)` (global keybind).
        pub fn key_event_mut(&mut self, rt_app: &RtApp, key: u32) -> bool {
            let Some(action) = action_for(key) else {
                return false;
            };
            if !self.focused {
                return false;
            }
            self.perform_all_action_mut(rt_app, action);
            true
        }

        /// FIXED `keyEvent(self_: *mut Self, rt_app, ..)`.
        pub unsafe fn key_event_raw(self_: *mut CoreApp, rt_app: &RtApp, key: u32) -> bool {
            let Some(action) = action_for(key) else {
                return false;
            };
            if !unsafe { (*self_).focused } {
                return false;
            }
            unsafe { CoreApp::perform_all_action_raw(self_, rt_app, action) };
            true
        }

        /// SHIPPED / ROUND 0 `performAction(&mut self, ..)`.
        pub fn perform_action_mut(&mut self, rt_app: &RtApp, action: Action) {
            match action {
                Action::NewWindow => {
                    let _ = rt_app.perform_action(Target::App, Action::NewWindow);
                }
                a => {
                    let _ = rt_app.perform_action(Target::App, a);
                }
            }
        }

        /// FIXED `performAction(self_: *mut Self, ..)`.
        pub unsafe fn perform_action_raw(self_: *mut CoreApp, rt_app: &RtApp, action: Action) {
            let _ = self_;
            let _ = rt_app.perform_action(Target::App, action);
        }

        /// ROUND 0 `newWindow(&self, ..)`.
        pub fn new_window_ref(&self, rt_app: &RtApp, parent: Option<*mut Core>) {
            let target = match parent {
                Some(p)
                    if self
                        .surfaces
                        .iter()
                        .any(|&s| unsafe { &raw mut (*s).core_surface } == p) =>
                {
                    Target::Surface(unsafe { &*p })
                }
                _ => Target::App,
            };
            let _ = rt_app.perform_action(target, Action::NewWindow);
        }

        /// SHIPPED `newWindow(&mut self, ..)`.
        pub fn new_window_mut(&mut self, rt_app: &RtApp, parent: Option<*mut Core>) {
            let target = match parent {
                Some(p)
                    if self
                        .surfaces
                        .iter()
                        .any(|&s| unsafe { &raw mut (*s).core_surface } == p) =>
                {
                    Target::Surface(unsafe { &*p })
                }
                _ => Target::App,
            };
            let _ = rt_app.perform_action(target, Action::NewWindow);
        }

        /// FIXED `newWindow(self_: *mut Self, ..)`.
        pub unsafe fn new_window_raw(
            self_: *mut CoreApp,
            rt_app: &RtApp,
            parent: Option<*mut Core>,
        ) {
            let has = |p: *mut Core| unsafe {
                (*self_)
                    .surfaces
                    .iter()
                    .any(|&s| &raw mut (*s).core_surface == p)
            };
            let target = match parent {
                Some(p) if has(p) => Target::Surface(unsafe { &*p }),
                _ => Target::App,
            };
            let _ = rt_app.perform_action(target, Action::NewWindow);
        }

        /// SHIPPED `performAllAction(&mut self, ..)`: surface-scoped actions over
        /// the live `surfaces` slice.
        pub fn perform_all_action_mut(&mut self, rt_app: &RtApp, action: Action) {
            if matches!(action, Action::ReloadConfig | Action::Quit) {
                self.perform_action_mut(rt_app, action);
                return;
            }
            for &s in self.surfaces.iter() {
                unsafe { surface_perform_binding_action(&raw mut (*s).core_surface, action) };
            }
        }

        /// ROUND 0 `performAllAction`: raw app, but iterating the live list.
        pub unsafe fn perform_all_action_live_iter(
            self_: *mut CoreApp,
            rt_app: &RtApp,
            action: Action,
        ) {
            if matches!(action, Action::ReloadConfig | Action::Quit) {
                unsafe { CoreApp::perform_action_raw(self_, rt_app, action) };
                return;
            }
            for &s in unsafe { (*self_).surfaces.iter() } {
                unsafe { surface_perform_binding_action(&raw mut (*s).core_surface, action) };
            }
        }

        /// FIXED `performAllAction(self_: *mut Self, ..)` (N448): a snapshot
        /// of the list, skipping surfaces that are gone.
        pub unsafe fn perform_all_action_raw(self_: *mut CoreApp, rt_app: &RtApp, action: Action) {
            if matches!(action, Action::ReloadConfig | Action::Quit) {
                unsafe { CoreApp::perform_action_raw(self_, rt_app, action) };
                return;
            }
            let surfaces: Vec<*mut Embedded> = unsafe { (*self_).surfaces.clone() };
            for s in surfaces {
                if !unsafe { (*self_).surfaces.contains(&s) } {
                    continue;
                }
                unsafe { surface_perform_binding_action(&raw mut (*s).core_surface, action) };
            }
        }
    }

    // ------------------------------------------------------ the C exports

    /// `rustty_surface_inherited_config(surface, source)`.
    pub unsafe fn c_inherited_config(surface: *mut Embedded, source: Context) -> f32 {
        unsafe { (*surface).new_surface_options(source) }
    }

    /// `rustty_surface_new(app, opts)`.
    pub unsafe fn c_surface_new(app: *mut RtApp) -> *mut Embedded {
        match mode() {
            Mode::Shipped | Mode::Round0 => unsafe { (*app).new_surface_mut() },
            Mode::Fixed => unsafe { RtApp::new_surface_raw(app) },
        }
    }

    /// `rustty_app_update_config(app, config)`:
    /// `(*core_app).updateConfig(&*app, config)`.
    pub unsafe fn c_app_update_config(app: *const RtApp) {
        unsafe { (*(*app).core_app).update_config(&*app) }
    }

    /// `rustty_app_needs_confirm_quit(app)`.
    pub unsafe fn c_app_needs_confirm_quit(app: *const RtApp) -> bool {
        unsafe { (*(*app).core_app).needs_confirm_quit() }
    }

    /// The macOS host's action callback (`Rustty.App.swift`).
    /// - `new_split` / `new_tab` / `new_window`: `inherited_config` on the
    ///   target surface, then the notification observers create the new view
    ///   synchronously: `rustty_surface_new`.
    /// - `reload_config`: `rustty_app_update_config`.
    /// - `quit`: `NSApplication.terminate` -> `rustty_app_needs_confirm_quit`.
    pub unsafe fn host_action(_app: *const RtApp, surface: *mut Embedded, action: Action) -> bool {
        let app: *mut RtApp = ROOT.with(|r| r.get());
        let ctx = match action {
            Action::NewSplit => Context::Split,
            Action::NewTab => Context::Tab,
            Action::NewWindow => Context::Window,
            Action::ReloadConfig => {
                unsafe { c_app_update_config(app) };
                return true;
            }
            Action::Quit => {
                let _ = unsafe { c_app_needs_confirm_quit(app) };
                return true;
            }
            Action::Other => return true,
        };
        if !surface.is_null() {
            let _ = unsafe { c_inherited_config(surface, ctx) };
        }
        let _ = unsafe { c_surface_new(app) };
        true
    }

    pub fn action_for(key: u32) -> Option<Action> {
        match key {
            1 => Some(Action::NewSplit),
            2 => Some(Action::NewTab),
            3 => Some(Action::NewWindow),
            4 => Some(Action::Other),
            5 => Some(Action::ReloadConfig),
            6 => Some(Action::Quit),
            _ => None,
        }
    }

    // ---------------------------------------------------------- core surface

    /// SHIPPED core-surface methods (`&mut self`).
    impl Core {
        pub fn key_callback(&mut self, key: u32) -> bool {
            self.pressed_key = None;
            if let Some(v) = self.maybe_handle_binding(key) {
                return v;
            }
            self.pressed_key = Some(key);
            false
        }

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
                Action::ReloadConfig | Action::Quit => {
                    let rt_app = unsafe { &*self.rt_app };
                    unsafe { &mut *self.app }.perform_action_mut(rt_app, action);
                    true
                }
                Action::Other => {
                    self.size.0 += 1;
                    true
                }
            }
        }
    }

    /// `Surface::keyCallback(self_: *mut Self, ..)` (T5b, both rounds).
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

    /// `Surface::performBindingAction(self_: *mut Self, ..)`; the app-scoped
    /// arms per `MODE` (round 0: app references; fixed: raw-self).
    pub unsafe fn perform_binding_action_raw(self_: *mut Core, action: Action) -> bool {
        let this = unsafe { &mut *self_ };
        match action {
            Action::NewWindow => unsafe {
                match mode() {
                    Mode::Fixed => {
                        CoreApp::new_window_raw((*self_).app, &*(*self_).rt_app, Some(self_))
                    }
                    _ => (*(*self_).app).new_window_ref(&*(*self_).rt_app, Some(self_)),
                }
                true
            },
            Action::NewTab => unsafe {
                (*(*self_).rt_app).perform_action(Target::Surface(&*self_), Action::NewTab)
            },
            Action::NewSplit => unsafe {
                let _right = (*self_).size.0 > (*self_).size.1;
                (*(*self_).rt_app).perform_action(Target::Surface(&*self_), Action::NewSplit)
            },
            Action::ReloadConfig | Action::Quit => unsafe {
                let rt_app = &*(*self_).rt_app;
                match mode() {
                    Mode::Fixed => CoreApp::perform_action_raw((*self_).app, rt_app, action),
                    _ => (*(*self_).app).perform_action_mut(rt_app, action),
                }
                true
            },
            Action::Other => {
                this.size.0 += 1;
                true
            }
        }
    }

    /// Dispatch into a surface per `MODE`.
    unsafe fn surface_key_callback(s: *mut Core, key: u32) -> bool {
        match mode() {
            Mode::Shipped => unsafe { (*s).key_callback(key) },
            _ => unsafe { key_callback_raw(s, key) },
        }
    }

    unsafe fn surface_perform_binding_action(s: *mut Core, action: Action) -> bool {
        match mode() {
            Mode::Shipped => unsafe { (*s).perform_binding_action(action) },
            _ => unsafe { perform_binding_action_raw(s, action) },
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

    // ------------------------------------------------------------- the world

    /// `rustty_app_new` + one `rustty_surface_new` (with spare capacity 1 in
    /// the surface list, so the first append reallocates it, as a grown list
    /// eventually does).
    pub struct World {
        pub core_app: *mut CoreApp,
        pub rt_app: *mut RtApp,
        pub surface: *mut Embedded,
    }

    impl World {
        pub fn new(m: Mode) -> World {
            MODE.with(|c| c.set(m));
            let mut surfaces = Vec::with_capacity(1);
            surfaces.shrink_to_fit();
            let core_app = Box::into_raw(Box::new(CoreApp {
                alloc: 7,
                focused: true,
                theme: 0,
                surfaces,
            }));
            let rt_app = Box::into_raw(Box::new(RtApp {
                core_app,
                cb: host_action,
                config: UnsafeCell::new(0),
            }));
            ROOT.with(|r| r.set(rt_app));
            let surface = unsafe { new_surface_body(rt_app) };
            World {
                core_app,
                rt_app,
                surface,
            }
        }
    }

    impl World {
        /// A second surface, with the list left exactly full (`len ==
        /// capacity`), so the next append reallocates it.
        pub fn add_surface_tight(&self) {
            unsafe {
                let _ = new_surface_body(self.rt_app);
                (*self.core_app).surfaces.shrink_to_fit();
            }
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            unsafe {
                for s in std::mem::take(&mut (*self.core_app).surfaces) {
                    drop(Box::from_raw(s));
                }
                drop(Box::from_raw(self.rt_app));
                drop(Box::from_raw(self.core_app));
            }
        }
    }

    // --------------------------------------------------------- the C routes

    /// `rustty_surface_key(surface, ev)` → `App::keyEvent(.surface)`.
    pub unsafe fn c_surface_key(surface: *mut Embedded, key: u32) -> bool {
        unsafe {
            match mode() {
                Mode::Fixed => RtApp::key_event_raw((*surface).app, surface, key),
                _ => (*(*surface).app).key_event_ref(surface, key),
            }
        }
    }

    /// `rustty_app_key(app, ev)` → `App::keyEvent(.app)` (global keybinds).
    pub unsafe fn c_app_key(app: *mut RtApp, key: u32) -> bool {
        unsafe {
            match mode() {
                Mode::Fixed => RtApp::key_event_raw(app, core::ptr::null_mut(), key),
                _ => (*app).key_event_ref(core::ptr::null_mut(), key),
            }
        }
    }

    /// `rustty_surface_binding_action` / `_split` / `_split_focus` /
    /// `_request_close`.
    pub unsafe fn c_surface_binding_action(ptr: *mut Embedded, action: Action) -> bool {
        unsafe { surface_perform_binding_action(&raw mut (*ptr).core_surface, action) }
    }

    /// `rustty_app_tick` draining an app-scoped message (`performAction`).
    pub unsafe fn c_app_tick_perform(app: *mut RtApp, action: Action) {
        unsafe {
            let core_app = (*app).core_app;
            match mode() {
                Mode::Fixed => CoreApp::perform_action_raw(core_app, &*app, action),
                _ => (*core_app).perform_action_mut(&*app, action),
            }
        }
    }

    /// `rustty_app_key` with a global surface-scoped binding (`all:`/`global:`)
    /// in round-0 shape: raw app, live-list iteration (N448).
    pub unsafe fn c_app_key_live_iter(app: *mut RtApp, action: Action) {
        unsafe { CoreApp::perform_all_action_live_iter((*app).core_app, &*app, action) }
    }
}

fn n442_keys(m: n442::Mode, f: unsafe fn(*mut n442::Embedded, u32) -> bool, keys: &[u32]) {
    for &key in keys {
        let w = n442::World::new(m);
        unsafe { f(w.surface, key) };
        drop(w);
    }
}

fn n442_actions(m: n442::Mode, actions: &[n442::Action]) {
    for &a in actions {
        let w = n442::World::new(m);
        unsafe { n442::c_surface_binding_action(w.surface, a) };
        drop(w);
    }
}

fn main() {
    use n442::{Action as A, Mode as M};
    let which = std::env::args().nth(1).expect("candidate name");
    match which.as_str() {
        // ---- N441 ----
        "n441_inline_renderer" => run_inline(),
        "n441_boxed_renderer" => run_boxed(),
        "n441_residual_terminal_inline" => run_terminal_inline(),
        "n441_residual_terminal_boxed" => run_terminal_boxed(),
        // Fix round 1: the box freed without drop glue.
        "n441_box_drop_leak" => run_box_drop_leak(),
        "n441_box_dealloc_after_deinit" => run_box_dealloc_after_deinit(),
        // Fix round 2: the backend teardown and the fields only `Drop` frees.
        "n441_r1_drop_only_fields_leak" => run_r1_backend(false),
        "n441_r1_display_after_free" => run_r1_backend(true),
        "n441_r2_deinit_then_drop" => run_r2_backend(),

        // ---- N442: before T5b ----
        "n442_shipped_key_route" => n442_keys(M::Shipped, n442::c_surface_key, &[4, 1, 2, 3]),
        "n442_shipped_binding_new_split" => n442_actions(M::Shipped, &[A::NewSplit]),
        "n442_shipped_binding_new_tab" => n442_actions(M::Shipped, &[A::NewTab]),
        "n442_shipped_binding_new_window" => n442_actions(M::Shipped, &[A::NewWindow]),
        // NEGATIVE: raw-self, but the local `this` spans the callback.
        "n442_raw_this_spans" => {
            let w = n442::World::new(M::Fixed);
            unsafe {
                n442::perform_binding_action_this_spans(
                    &raw mut (*w.surface).core_surface,
                    A::NewSplit,
                )
            };
        }

        // ---- fix round 1: round-0 shapes (app references) ----
        // `App::newWindow(&self)` while the host's `rustty_surface_new` pushes
        // to `surfaces`.
        "r0_binding_new_window" => n442_actions(M::Round0, &[A::NewWindow]),
        // embedded `keyEvent(&self)` + `newSurface(&mut self)` on the key route.
        "r0_key_route_new_split" => n442_keys(M::Round0, n442::c_surface_key, &[1]),
        // N443: `App::performAction(&mut self)`: reload_config / quit.
        "r0_binding_reload_config" => n442_actions(M::Round0, &[A::ReloadConfig]),
        "r0_binding_quit" => n442_actions(M::Round0, &[A::Quit]),
        "r0_app_tick_reload_config" => {
            let w = n442::World::new(M::Round0);
            unsafe { n442::c_app_tick_perform(w.rt_app, A::ReloadConfig) };
        }
        "r0_app_key_global_reload_config" => {
            let w = n442::World::new(M::Round0);
            unsafe { n442::c_app_key(w.rt_app, 5) };
        }
        // N448: the live-list iteration while a surface's host callback appends.
        "r0_all_new_split_live_iter" => {
            let w = n442::World::new(M::Fixed);
            w.add_surface_tight();
            unsafe { n442::c_app_key_live_iter(w.rt_app, A::NewSplit) };
        }
        // The fix for N448 on the same two-surface, full-list world.
        "fixed_all_new_split_snapshot" => {
            let w = n442::World::new(M::Fixed);
            w.add_surface_tight();
            unsafe { n442::c_app_key(w.rt_app, 1) };
        }

        // ---- the fix (round 1) ----
        "fixed_key_route_all" => n442_keys(M::Fixed, n442::c_surface_key, &[4, 1, 2, 3, 5, 6]),
        "fixed_binding_all" => n442_actions(
            M::Fixed,
            &[
                A::Other,
                A::NewSplit,
                A::NewTab,
                A::NewWindow,
                A::ReloadConfig,
                A::Quit,
            ],
        ),
        "fixed_app_tick_all" => {
            for a in [A::ReloadConfig, A::Quit, A::NewWindow] {
                let w = n442::World::new(M::Fixed);
                unsafe { n442::c_app_tick_perform(w.rt_app, a) };
            }
        }
        "fixed_app_key_global_all" => {
            for key in [1, 2, 3, 5, 6] {
                let w = n442::World::new(M::Fixed);
                unsafe { n442::c_app_key(w.rt_app, key) };
            }
        }
        other => panic!("unknown candidate {other}"),
    }
}
