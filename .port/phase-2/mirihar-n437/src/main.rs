//! N437 — the render thread's whole-`Renderer` references vs the main thread's
//! raw draw. Isolated, non-FFI model; see `README.md` for the candidates and
//! results. Each candidate is one executed path (Miri checks only what runs),
//! selected by argv[1].
//!
//! The REAL renderer mutex (`rustty_zigstd::thread::Mutex`) and the REAL
//! `rustty_renderer::latest_frame` (`DrawMutex`, `LatestFrame`) are included by
//! `#[path]`; `extern crate self as rustty_zigstd` lets `latest_frame.rs`'s own
//! `use rustty_zigstd::thread::Mutex` resolve to the included copy.

extern crate self as rustty_zigstd;

#[path = "../../../../crates/rustty-zigstd/src/thread.rs"]
pub mod thread;

#[path = "../../../../crates/rustty-renderer/src/latest_frame.rs"]
#[allow(dead_code)]
mod latest_frame;

use latest_frame::{DrawMutex, ExportedFrameType, LatestFrame};
use std::sync::Arc;
use thread::Mutex;

const ROUNDS: usize = 3;

/// An exported frame that owns memory (the OpenGL `ExportedFrame::Memory`).
struct Frame(#[allow(dead_code)] Vec<u8>);
impl ExportedFrameType for Frame {
    const IS_VOID: bool = false;
}

struct P<T>(*mut T);
impl<T> Clone for P<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for P<T> {}
unsafe impl<T> Send for P<T> {}

fn spawn_render<T: 'static>(p: P<T>, f: impl FnOnce(P<T>) + Send + 'static) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || f(p))
}

// ===========================================================================
// SHIPPED (pre-N437): the mutex inline in the renderer; render-thread entries
// with whole-renderer receivers (`renderer_seam.rs` called
// `(*(*self_).renderer).0.update_frame(..)`, `.animation_wake()`,
// `.set_focus(..)`, ...); the main thread's raw sync draw (`draw_frame_raw`)
// locks the inline mutex and reborrows the whole renderer inside it.
// ===========================================================================
struct Shipped {
    draw_mutex: Mutex,
    focused: bool,
    cells_rebuilt: bool,
    cells: Vec<u32>,
    terminal_state: u64,
    uniforms: [f32; 4],
}

impl Shipped {
    fn new() -> Self {
        Shipped {
            draw_mutex: Mutex::new(),
            focused: true,
            cells_rebuilt: false,
            cells: vec![0; 8],
            terminal_state: 0,
            uniforms: [0.0; 4],
        }
    }

    /// `update_frame(&mut self, ..)`: unlocked work, then the locked rebuild.
    fn update_frame(&mut self) {
        self.terminal_state += 1;
        std::thread::yield_now();
        self.draw_mutex.lock();
        self.cells.push(1);
        self.cells_rebuilt = true;
        self.draw_mutex.unlock();
    }

    /// `animation_wake(&self)`: an unlocked read (`armAnimationTimer`).
    fn animation_wake(&self) -> bool {
        std::thread::yield_now();
        self.focused && !self.cells.is_empty()
    }

    /// `set_focus(&mut self, ..)`: unlocked writes.
    fn set_focus(&mut self, focus: bool) {
        self.focused = focus;
        std::thread::yield_now();
    }

    /// A whole-`&mut self` helper `draw_frame_raw` calls under the lock
    /// (`update_screen_size_uniforms`, `init_shaders`, ...).
    fn update_uniforms(&mut self) {
        self.uniforms[0] += 1.0;
    }

    /// `draw_frame_raw(self_ptr, sync)`: raw entry, inline lock, whole `&mut`
    /// helpers inside.
    unsafe fn draw_frame_raw(p: *mut Shipped) {
        unsafe {
            (*p).draw_mutex.lock();
            (*p).update_uniforms();
            (*p).cells_rebuilt = false;
            let _ = (*p).focused;
            (*p).draw_mutex.unlock();
        }
    }
}

fn run_shipped(render: fn(P<Shipped>)) {
    let p = P(Box::into_raw(Box::new(Shipped::new())));
    let t = spawn_render(p, move |p| {
        for _ in 0..ROUNDS {
            render(p)
        }
    });
    for _ in 0..ROUNDS {
        unsafe { Shipped::draw_frame_raw(p.0) };
        std::thread::yield_now();
    }
    t.join().unwrap();
    drop(unsafe { Box::from_raw(p.0) });
}

// ===========================================================================
// FIXED (N437): `draw_mutex` in its own allocation (`DrawMutex`), each thread
// holding its own handle; every cross-thread entry locks through that handle
// BEFORE it forms any reference to the renderer (`Renderer::with_locked`,
// `*_raw`); the display-link apply runs after the unlock on a copy; the
// latest-frame slot is behind an `Arc` the main thread holds its own handle to.
// ===========================================================================
struct Fixed {
    draw_mutex: DrawMutex,
    latest_frame: Arc<LatestFrame<Frame>>,
    focused: bool,
    visible: bool,
    display_realized: bool,
    cells_rebuilt: bool,
    cells: Vec<u32>,
    terminal_state: u64,
    uniforms: [f32; 4],
    swap_chain: Option<Vec<u8>>,
}

impl Fixed {
    fn new() -> Self {
        Fixed {
            draw_mutex: DrawMutex::new(),
            latest_frame: Arc::new(LatestFrame::new()),
            focused: true,
            visible: true,
            display_realized: true,
            cells_rebuilt: false,
            cells: vec![0; 8],
            terminal_state: 0,
            uniforms: [0.0; 4],
            swap_chain: None,
        }
    }

    /// `Renderer::with_locked`.
    unsafe fn with_locked<T>(p: *mut Fixed, lock: &DrawMutex, f: impl FnOnce(&mut Fixed) -> T) -> T {
        lock.lock();
        struct Unlock<'a>(&'a DrawMutex);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                self.0.unlock();
            }
        }
        let _u = Unlock(lock);
        f(unsafe { &mut *p })
    }

    /// `update_frame_locked(&mut self, ..)`: the whole body under the lock.
    fn update_frame_locked(&mut self) -> bool {
        self.terminal_state += 1;
        std::thread::yield_now();
        self.cells.push(1);
        self.cells_rebuilt = true;
        self.visible && self.cells_rebuilt // the display-link sync, applied after
    }

    fn animation_wake(&self) -> bool {
        self.focused && !self.cells.is_empty()
    }

    fn set_focus_locked(&mut self, focus: bool) {
        self.focused = focus;
    }

    fn update_uniforms(&mut self) {
        self.uniforms[0] += 1.0;
    }

    /// `draw_frame_locked`: whole `&mut` helpers, a swap chain built lazily,
    /// a frame pushed into the latest-frame slot (`opengl_frame_completed`).
    fn draw_frame_locked(&mut self) {
        if !self.display_realized {
            return;
        }
        self.swap_chain.get_or_insert_with(|| vec![0; 16]);
        self.update_uniforms();
        self.cells_rebuilt = false;
        self.latest_frame.push(Frame(vec![1, 2, 3]));
    }

    /// `release_if_unrealized_raw`'s body.
    fn release_if_unrealized(&mut self) -> bool {
        if self.display_realized {
            return false;
        }
        self.swap_chain = None;
        true
    }
}

/// The render thread's loop body: every entry through its own handle.
fn fixed_render_round(p: P<Fixed>, lock: &DrawMutex, round: usize) {
    unsafe {
        // renderCallback: unrealized release, update, draw.
        if Fixed::with_locked(p.0, lock, |r| r.release_if_unrealized()) {
            return;
        }
        let sync = Fixed::with_locked(p.0, lock, |r| r.update_frame_locked());
        let _ = sync; // DisplayLinkSync::apply after the unlock touches no renderer byte
        Fixed::with_locked(p.0, lock, |r| r.draw_frame_locked());
        // armAnimationTimer, setFocus, setVisible, the search-match writes.
        let _ = Fixed::with_locked(p.0, lock, |r| r.animation_wake());
        Fixed::with_locked(p.0, lock, |r| r.set_focus_locked(round % 2 == 0));
        Fixed::with_locked(p.0, lock, |r| {
            r.visible = true;
            r.cells.truncate(8);
        });
    }
}

fn run_fixed(main_side: fn(P<Fixed>, &DrawMutex, &LatestFrame<Frame>)) {
    let p = P(Box::into_raw(Box::new(Fixed::new())));
    // Both handles taken before the render thread starts (`RendererHandles`,
    // `Thread::draw_mutex`).
    let main_lock = unsafe { (*p.0).draw_mutex.clone() };
    let main_slot = unsafe { (*p.0).latest_frame.clone() };
    let render_lock = main_lock.clone();
    let t = spawn_render(p, move |p| {
        for round in 0..ROUNDS {
            fixed_render_round(p, &render_lock, round);
        }
    });
    for _ in 0..ROUNDS {
        main_side(p, &main_lock, &main_slot);
        std::thread::yield_now();
    }
    t.join().unwrap();
    drop(main_slot);
    drop(unsafe { Box::from_raw(p.0) });
}

/// The macOS main thread: CoreAnimation's sync draw (`display_callback`).
fn main_sync_draw(p: P<Fixed>, lock: &DrawMutex, _slot: &LatestFrame<Frame>) {
    unsafe { Fixed::with_locked(p.0, lock, |r| r.draw_frame_locked()) };
}

/// The GTK main thread: `displayUnrealized`/`displayRealized` flags and the
/// `RenderSurface` snapshot's `takeFrame`.
fn main_gtk(p: P<Fixed>, lock: &DrawMutex, slot: &LatestFrame<Frame>) {
    unsafe {
        Fixed::with_locked(p.0, lock, |r| r.display_realized = false);
        drop(slot.take());
        Fixed::with_locked(p.0, lock, |r| r.display_realized = true);
        drop(slot.take());
    }
}

// ===========================================================================
// NEGATIVE controls for the two halves of the fix.
// ===========================================================================

/// Lock-first but with the mutex INLINE: the waiter's `&Mutex` over renderer
/// bytes is clobbered by the holder's whole `&mut` retag.
fn run_inline_lock_whole_mut() {
    let p = P(Box::into_raw(Box::new(Shipped::new())));
    let t = spawn_render(p, |p| {
        for _ in 0..ROUNDS {
            unsafe {
                let m: *const Mutex = &raw const (*p.0).draw_mutex;
                (*m).lock();
                (*p.0).update_uniforms();
                (*m).unlock();
            }
        }
    });
    for _ in 0..ROUNDS {
        unsafe { Shipped::draw_frame_raw(p.0) };
        std::thread::yield_now();
    }
    t.join().unwrap();
    drop(unsafe { Box::from_raw(p.0) });
}

/// The external lock, but the latest-frame slot INLINE: the main thread's
/// `take` reads renderer bytes the render thread's locked whole `&mut` covers.
struct InlineSlot {
    draw_mutex: DrawMutex,
    latest: LatestFrame<Frame>,
    uniforms: [f32; 4],
}

fn run_latest_frame_inline() {
    let p = P(Box::into_raw(Box::new(InlineSlot {
        draw_mutex: DrawMutex::new(),
        latest: LatestFrame::new(),
        uniforms: [0.0; 4],
    })));
    let main_lock = unsafe { (*p.0).draw_mutex.clone() };
    let render_lock = main_lock.clone();
    let t = spawn_render(p, move |p| {
        for _ in 0..ROUNDS {
            render_lock.lock();
            let r = unsafe { &mut *p.0 };
            fn draw(r: &mut InlineSlot) {
                r.uniforms[0] += 1.0;
                std::thread::yield_now();
                r.latest.push(Frame(vec![1]));
            }
            draw(r);
            render_lock.unlock();
        }
    });
    for _ in 0..ROUNDS {
        // `takeFrame` through the renderer (`(*core).renderer.latest_frame`).
        drop(unsafe { (*p.0).latest.take() });
        std::thread::yield_now();
    }
    t.join().unwrap();
    drop(main_lock);
    drop(unsafe { Box::from_raw(p.0) });
}

fn main() {
    let which = std::env::args().nth(1).expect("candidate name");
    match which.as_str() {
        // UB expected (the shipped shapes N437 names).
        "shipped_update_frame" => run_shipped(|p| unsafe { (*p.0).update_frame() }),
        "shipped_animation_wake" => run_shipped(|p| {
            let _ = unsafe { (*p.0).animation_wake() };
        }),
        "shipped_set_focus" => run_shipped(|p| unsafe { (*p.0).set_focus(false) }),
        // UB expected (negative controls for each half of the fix).
        "inline_lock_whole_mut" => run_inline_lock_whole_mut(),
        "latest_frame_inline" => run_latest_frame_inline(),
        // CLEAN expected (the fix).
        "fixed_vs_sync_draw" => run_fixed(main_sync_draw),
        "fixed_vs_gtk_main" => run_fixed(main_gtk),
        other => panic!("unknown candidate {other}"),
    }
}
