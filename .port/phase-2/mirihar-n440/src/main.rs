//! N440 — the Metal completion handler vs the render thread's whole-renderer
//! `&mut` under the draw mutex. Isolated, non-FFI model; see `README.md` for
//! the candidates and results. Each candidate is one executed path (Miri checks
//! only what runs), selected by argv[1].
//!
//! The REAL renderer mutex and semaphore (`rustty_zigstd::thread::{Mutex,
//! Semaphore}`) and the REAL `rustty_renderer::latest_frame::DrawMutex` are
//! included by `#[path]`; `extern crate self as rustty_zigstd` lets
//! `latest_frame.rs`'s own `use rustty_zigstd::thread::Mutex` resolve to the
//! included copy.
//!
//! Threads, in both candidates:
//! - the render thread: per round, a locked entry that forms a whole-renderer
//!   `&mut` (`Renderer::with_locked`: `set_font_grid_locked`, the locked tail of
//!   `update_frame_impl`, the search-match writes), then an async draw
//!   (`draw_frame_raw(sync = false)`): under the lock, `next_frame` (waits on
//!   `frame_sema`) and `Frame::complete(false)`, which hands the frame to
//!   Metal;
//! - the Metal completion thread: runs each committed frame's completion
//!   handler (`metal_buffer_completed`), WITHOUT the draw mutex;
//! - the main thread (fixed candidate only): the CoreAnimation sync draw
//!   (`display_callback` → `draw_frame_raw(sync = true)`), narrow, running the
//!   same completion handler inline under the lock.

extern crate self as rustty_zigstd;

#[path = "../../../../crates/rustty-zigstd/src/thread.rs"]
#[allow(dead_code)]
pub mod thread;

#[path = "../../../../crates/rustty-renderer/src/latest_frame.rs"]
#[allow(dead_code)]
mod latest_frame;

use latest_frame::DrawMutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering::SeqCst};
use std::sync::mpsc;
use std::sync::{Arc, Mutex as StdMutex};
use thread::Semaphore;

const ROUNDS: usize = 4;
/// Fewer frames than rounds, so `next_frame` really waits for a completion.
const SWAP_CHAIN_COUNT: usize = 2;

struct P<T>(*mut T);
impl<T> Clone for P<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for P<T> {}
unsafe impl<T> Send for P<T> {}

/// The CALayer: an ObjC object with its own (thread-safe) state.
struct Layer {
    contents: StdMutex<usize>,
}

/// `Target`: `surface` is the `CFRetained<IOSurfaceRef>`.
struct Target {
    surface: Arc<Vec<u8>>,
}

/// The core mailbox sink behind `SurfaceMailbox` (an `Arc`ed queue).
#[derive(Clone)]
struct Mailbox(Arc<StdMutex<Vec<i32>>>);
impl Mailbox {
    fn push(&self, health: i32) {
        self.0.lock().unwrap().push(health);
    }

    /// `.instant`: refused (false) once `MAILBOX_CAPACITY` reports are queued.
    fn push_instant(&self, health: i32) -> bool {
        let mut q = self.0.lock().unwrap();
        if q.len() >= MAILBOX_CAPACITY {
            return false;
        }
        q.push(health);
        true
    }

    /// The main thread drains the app mailbox.
    fn drain(&self) {
        self.0.lock().unwrap().clear();
    }
}

/// `HealthReport` (round 2): what the surface was last told, and whether a
/// report is owed.
struct HealthReport {
    reported: StdMutex<i32>,
    pending: AtomicBool,
}

/// A small app mailbox, so the fixed model also takes the refusal path.
const MAILBOX_CAPACITY: usize = 1;

fn health_of(round: usize) -> i32 {
    (round % 2) as i32
}

// ===========================================================================
// SHIPPED (pre-N440): `metal_buffer_completed` recovers the renderer from the
// context's type-erased pointer and calls `(*renderer).api.present(&*ctx.target,
// ..)` (a `&mut` reborrow of `api`, a `&Target` into `swap_chain.frames[i]`) and
// `frame_completed_raw(renderer, ..)` (places on `health`, `surface_mailbox`,
// `swap_chain`), all renderer bytes, without the draw mutex.
// ===========================================================================
struct ShippedApi {
    layer: Option<Layer>,
    blending: u8,
}

impl ShippedApi {
    /// `Metal::present(&mut self, target: &Target, sync)`.
    fn present(&mut self, target: &Target) {
        if let Some(layer) = &self.layer {
            *layer.contents.lock().unwrap() = target.surface.len();
        }
    }
}

struct ShippedSwapChain {
    frames: [Target; SWAP_CHAIN_COUNT],
    frame_index: usize,
    frame_sema: Semaphore,
}

struct Shipped {
    draw_mutex: DrawMutex,
    health: AtomicI32,
    surface_mailbox: Mailbox,
    api: ShippedApi,
    swap_chain: Option<ShippedSwapChain>,
    uniforms: [f32; 4],
    cells: Vec<u32>,
}

/// `CompletionContext { renderer: *mut (), target: *mut Target, sync }`.
struct ShippedCtx {
    renderer: *mut Shipped,
    target: *mut Target,
}
unsafe impl Send for ShippedCtx {}

impl Shipped {
    fn new() -> Self {
        Shipped {
            draw_mutex: DrawMutex::new(),
            health: AtomicI32::new(0),
            surface_mailbox: Mailbox(Arc::new(StdMutex::new(Vec::new()))),
            api: ShippedApi {
                layer: Some(Layer {
                    contents: StdMutex::new(0),
                }),
                blending: 0,
            },
            swap_chain: Some(ShippedSwapChain {
                frames: std::array::from_fn(|i| Target {
                    surface: Arc::new(vec![0; i + 1]),
                }),
                frame_index: 0,
                frame_sema: Semaphore::with_permits(SWAP_CHAIN_COUNT),
            }),
            uniforms: [0.0; 4],
            cells: vec![0; 8],
        }
    }

    unsafe fn with_locked<T>(p: *mut Shipped, lock: &DrawMutex, f: impl FnOnce(&mut Shipped) -> T) -> T {
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

    /// `draw_frame_raw(sync = false)`: narrow places under the lock; the frame
    /// goes to Metal with the raw context.
    unsafe fn draw_async(p: *mut Shipped, lock: &DrawMutex, metal: &mpsc::Sender<ShippedCtx>) {
        lock.lock();
        unsafe {
            let chain: *mut ShippedSwapChain = (*p).swap_chain.as_mut().unwrap();
            (*chain).frame_sema.wait();
            (*chain).frame_index = ((*chain).frame_index + 1) % SWAP_CHAIN_COUNT;
            let target: *mut Target = &raw mut (*chain).frames[(*chain).frame_index];
            (*p).uniforms[1] += 1.0;
            metal.send(ShippedCtx { renderer: p, target }).unwrap();
        }
        lock.unlock();
    }
}

/// `metal_buffer_completed`, shipped shape.
unsafe fn shipped_buffer_completed(ctx: &ShippedCtx, health: i32) {
    let renderer = ctx.renderer;
    unsafe {
        if health == 0 {
            (*renderer).api.present(&*ctx.target);
        }
        // `frame_completed_raw`.
        if (*renderer).health.load(SeqCst) != health {
            (*renderer).health.store(health, SeqCst);
            (*renderer).surface_mailbox.push(health);
        }
        (*renderer).swap_chain.as_ref().unwrap().frame_sema.post();
    }
}

fn run_shipped() {
    let p = P(Box::into_raw(Box::new(Shipped::new())));
    let lock = unsafe { (*p.0).draw_mutex.clone() };
    let (tx, rx) = mpsc::channel::<ShippedCtx>();
    let metal = std::thread::spawn(move || {
        let mut round = 0;
        while let Ok(ctx) = rx.recv() {
            unsafe { shipped_buffer_completed(&ctx, health_of(round)) };
            round += 1;
        }
    });
    let render = std::thread::spawn(move || {
        let p = p;
        for round in 0..ROUNDS {
            unsafe {
                Shipped::with_locked(p.0, &lock, |r| {
                    r.cells.push(round as u32);
                    r.api.blending ^= 1;
                    r.uniforms[0] += 1.0;
                });
                Shipped::draw_async(p.0, &lock, &tx);
            }
            std::thread::yield_now();
        }
        drop(tx);
    });
    render.join().unwrap();
    metal.join().unwrap();
    drop(unsafe { Box::from_raw(p.0) });
}

// ===========================================================================
// FIXED (N440, P7-W7 T5a): the completion context carries clones of what the
// handler needs, each in its own allocation: `health: Arc<AtomicHealth>`,
// `frame_sema: Arc<Semaphore>` (the swap chain's), a `SurfaceMailbox` clone,
// the presenter cell `Arc<LayerCell>` (Metal's `layer`), the health-report
// reports (`Arc<HealthReport>`) and the target's retained IOSurface. The
// handler forms no reference to any renderer byte. Fix rounds 1 and 2 (rulings
// 39, 41): the reference's order, store → push → release, with an `.instant`
// push decided against `reported` (the health the surface last heard) under its
// mutex; a refusal leaves the report owed (UD362), so `SwapChain::deinit`'s wait
// for every release also waits for every push. Teardown (wait on the semaphore, take the chain, free the renderer)
// happens BEFORE the completion thread is joined: after its release a
// completion may only drop its clones.
// ===========================================================================
struct FixedApi {
    layer: Arc<Option<Layer>>,
    blending: u8,
}

struct FixedSwapChain {
    frames: [Target; SWAP_CHAIN_COUNT],
    frame_index: usize,
    frame_sema: Arc<Semaphore>,
}

struct Fixed {
    draw_mutex: DrawMutex,
    health: Arc<AtomicI32>,
    health_report: Arc<HealthReport>,
    surface_mailbox: Mailbox,
    api: FixedApi,
    swap_chain: Option<FixedSwapChain>,
    uniforms: [f32; 4],
    cells: Vec<u32>,
}

/// The fixed `CompletionContext`: owned clones only.
struct FixedCtx {
    health: Arc<AtomicI32>,
    health_report: Arc<HealthReport>,
    frame_sema: Arc<Semaphore>,
    surface_mailbox: Mailbox,
    layer: Arc<Option<Layer>>,
    surface: Arc<Vec<u8>>,
    sync: bool,
}

impl Fixed {
    fn new() -> Self {
        Fixed {
            draw_mutex: DrawMutex::new(),
            health: Arc::new(AtomicI32::new(0)),
            health_report: Arc::new(HealthReport {
                reported: StdMutex::new(0),
                pending: AtomicBool::new(false),
            }),
            surface_mailbox: Mailbox(Arc::new(StdMutex::new(Vec::new()))),
            api: FixedApi {
                layer: Arc::new(Some(Layer {
                    contents: StdMutex::new(0),
                })),
                blending: 0,
            },
            swap_chain: Some(FixedSwapChain {
                frames: std::array::from_fn(|i| Target {
                    surface: Arc::new(vec![0; i + 1]),
                }),
                frame_index: 0,
                frame_sema: Arc::new(Semaphore::with_permits(SWAP_CHAIN_COUNT)),
            }),
            uniforms: [0.0; 4],
            cells: vec![0; 8],
        }
    }

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

    /// `draw_frame_locked`'s frame acquisition and `begin_frame`: narrow places
    /// under the lock; the context's clones are taken here.
    unsafe fn begin_frame(p: *mut Fixed, sync: bool) -> FixedCtx {
        unsafe {
            let chain: *mut FixedSwapChain = (*p).swap_chain.as_mut().unwrap();
            (*chain).frame_sema.wait();
            (*chain).frame_index = ((*chain).frame_index + 1) % SWAP_CHAIN_COUNT;
            (*p).uniforms[1] += 1.0;
            FixedCtx {
                health: (*p).health.clone(),
                health_report: (*p).health_report.clone(),
                frame_sema: (*chain).frame_sema.clone(),
                surface_mailbox: (*p).surface_mailbox.clone(),
                layer: (*p).api.layer.clone(),
                surface: (*chain).frames[(*chain).frame_index].surface.clone(),
                sync,
            }
        }
    }
}

/// `metal_buffer_completed` + `frame_completed`, fixed shape: context only.
fn fixed_buffer_completed(ctx: &FixedCtx, health: i32) {
    if health == 0 {
        if let Some(layer) = &*ctx.layer {
            *layer.contents.lock().unwrap() = ctx.surface.len() + usize::from(ctx.sync);
        }
    }
    if ctx.health.load(SeqCst) != health {
        ctx.health.store(health, SeqCst);
    }
    // `HealthReport`: decide against what the surface was last told, under its
    // mutex; a refusal leaves the report owed (`pending`).
    {
        let mut reported = ctx.health_report.reported.lock().unwrap();
        if *reported == health {
            ctx.health_report.pending.store(false, SeqCst);
        } else if ctx.surface_mailbox.push_instant(health) {
            *reported = health;
            ctx.health_report.pending.store(false, SeqCst);
        } else {
            ctx.health_report.pending.store(true, SeqCst);
        }
    }
    ctx.frame_sema.post();
}

fn run_fixed(with_main_sync_draw: bool) {
    let p = P(Box::into_raw(Box::new(Fixed::new())));
    let main_lock = unsafe { (*p.0).draw_mutex.clone() };
    let mailbox = unsafe { (*p.0).surface_mailbox.clone() };
    let render_lock = main_lock.clone();
    let (tx, rx) = mpsc::channel::<FixedCtx>();
    let metal = std::thread::spawn(move || {
        let mut round = 0;
        while let Ok(ctx) = rx.recv() {
            fixed_buffer_completed(&ctx, health_of(round));
            round += 1;
        }
    });
    let render = std::thread::spawn(move || {
        let p = p;
        for round in 0..ROUNDS {
            unsafe {
                Fixed::with_locked(p.0, &render_lock, |r| {
                    r.cells.push(round as u32);
                    r.api.blending ^= 1;
                    r.uniforms[0] += 1.0;
                });
                // `draw_frame_raw(sync = false)`: an owed report is an animation wake.
                render_lock.lock();
                let report: &Arc<HealthReport> = &(*p.0).health_report;
                let _owed = report.pending.load(SeqCst);
                let ctx = Fixed::begin_frame(p.0, false);
                tx.send(ctx).unwrap();
                render_lock.unlock();
            }
            std::thread::yield_now();
        }
        drop(tx);
    });
    if with_main_sync_draw {
        for round in 0..ROUNDS {
            // `draw_frame_raw(sync = true)`: the handler inline, under the lock.
            main_lock.lock();
            let ctx = unsafe { Fixed::begin_frame(p.0, true) };
            fixed_buffer_completed(&ctx, health_of(round + 1));
            main_lock.unlock();
            drop(ctx);
            // The main thread drains the app mailbox between its draws.
            mailbox.drain();
            std::thread::yield_now();
        }
    }
    render.join().unwrap();
    // `SwapChain::deinit` (`Renderer::deinit`): wait for every in-flight frame's
    // release, then free the chain and the renderer, all BEFORE the completion
    // thread is joined: a completion that has released its frame may still be
    // running, dropping its clones.
    unsafe {
        let chain = (*p.0).swap_chain.take().unwrap();
        for _ in 0..SWAP_CHAIN_COUNT {
            chain.frame_sema.wait();
        }
        drop(chain);
    }
    drop(main_lock);
    drop(unsafe { Box::from_raw(p.0) });
    metal.join().unwrap();
    drop(mailbox);
}

fn main() {
    let which = std::env::args().nth(1).expect("candidate name");
    match which.as_str() {
        // UB expected: the shipped handler reaches renderer bytes unlocked.
        "shipped_async_handler" => run_shipped(),
        // CLEAN expected: the handler touches only its context's clones.
        "fixed_async_handler" => run_fixed(false),
        "fixed_async_and_sync_draw" => run_fixed(true),
        other => panic!("unknown candidate {other}"),
    }
}
