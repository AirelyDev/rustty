//! N444: the core `Surface`'s other cross-thread state (P7-W7 T5c). Isolated,
//! non-FFI model; see `README.md` for the candidates and results. Each
//! candidate is one executed path (Miri checks only what runs), selected by
//! argv[1].
//!
//! The core `Surface` embedded by value five blocks that another thread
//! writes through a raw pointer into the surface:
//!
//! - `io: Termio` — the io read thread writes `io.terminal` under
//!   `renderer_state.mutex` (`processOutput`), the io writer thread drains
//!   `io.mailbox`;
//! - `io_thread: termio::Thread` — the io thread's loop state;
//! - `renderer_thread: renderer::Thread` — the render thread's loop state;
//! - `renderer_state: renderer::State` — the D4 handoff atomics (`demand`,
//!   `handoff_gen`) the io and render threads bump;
//! - `search.state: search::Thread` — the search thread's loop state.
//!
//! Every main-thread `&mut Surface` / `&Surface` the apprt forms retags every
//! byte of the surface, so it races with those writes. The fix: each block in
//! its own allocation (`ThreadSharedBox`), shared by one raw root, with the
//! main thread's own accesses narrow (by place) and locked where the reference
//! locks.
//!
//! The REAL renderer mutex (`rustty_zigstd::thread::Mutex`) is included by
//! `#[path]`.

#![allow(clippy::missing_safety_doc)]

#[path = "../../../../crates/rustty-zigstd/src/thread.rs"]
#[allow(dead_code)]
pub mod thread;

use std::collections::VecDeque;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex as StdMutex;
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
// The blocks
// ===========================================================================

struct Terminal {
    rows: Vec<u32>,
    cursor: u32,
}

/// `termio::Mailbox::Spsc { queue: Box<Queue>, wakeup }`. The queue is its own
/// allocation with an internal lock (`BlockingQueue`); `wakeup` is an
/// `xev::Async` whose `notify` is `&self`.
struct IoMailbox {
    queue: Box<StdMutex<VecDeque<u32>>>,
    wakeup: AtomicBool,
}

impl IoMailbox {
    /// `Mailbox::send(&mut self, ..)` / `notify(&mut self)` (the shipped
    /// receivers).
    fn send_mut(&mut self, v: u32) {
        self.queue.lock().unwrap().push_back(v);
        self.wakeup.store(true, Ordering::Release);
    }

    /// The same body with `&self` receivers (`push` and `Async::notify` are
    /// `&self`).
    fn send(&self, v: u32) {
        self.queue.lock().unwrap().push_back(v);
        self.wakeup.store(true, Ordering::Release);
    }

    fn pop(&self) -> Option<u32> {
        self.queue.lock().unwrap().pop_front()
    }
}

/// `termio::Termio` (the fields the model's threads touch).
struct Termio {
    terminal: Terminal,
    mailbox: IoMailbox,
    size: (u32, u32),
    last_cursor_reset: u32,
    renderer_state: *mut State,
}

impl Termio {
    /// SHIPPED `queueMessage(&mut self, ..)`: a protected Unique over the
    /// whole Termio, terminal included.
    fn queue_message_mut(&mut self, v: u32) {
        let _m = unsafe { (*self.renderer_state).mutex };
        self.mailbox.send_mut(v);
    }

    /// FIXED `queueMessage(self_: *mut Termio, ..)` (the reference's
    /// `self: *Termio`): only the fields the reference touches, by place.
    unsafe fn queue_message(self_: *mut Termio, v: u32) {
        let _m = unsafe { (*(*self_).renderer_state).mutex };
        unsafe { (*self_).mailbox.send(v) };
    }

    /// Same, but still reaching `Mailbox::send(&mut self)`.
    unsafe fn queue_message_send_mut(self_: *mut Termio, v: u32) {
        let _m = unsafe { (*(*self_).renderer_state).mutex };
        unsafe { (*self_).mailbox.send_mut(v) };
    }

    /// `processOutput(io: *mut Termio, ..)` (the read thread, raw-self):
    /// the terminal written under the renderer mutex.
    unsafe fn process_output(io: *mut Termio) {
        unsafe {
            let rs = (*io).renderer_state;
            (*(*rs).mutex).lock();
            (*io).terminal.cursor += 1;
            (&mut (*io).terminal.rows)[0] += 1;
            (*io).last_cursor_reset += 1;
            (*rs).demand.fetch_add(1, Ordering::Relaxed);
            (*(*rs).mutex).unlock();
        }
    }

    /// N449: an io writer-thread `&mut self` method (`focusGained`,
    /// `resize`, `changeConfig`, `sizeReport`, ...): a protected Unique over
    /// the whole Termio; its locked terminal read goes through the state's
    /// terminal pointer, as the port does.
    fn focus_gained_mut(&mut self) {
        unsafe {
            let rs = self.renderer_state;
            (*(*rs).mutex).lock();
            let _mode = (*(*rs).terminal).cursor;
            (*(*rs).mutex).unlock();
        }
        std::thread::yield_now();
        self.size.0 += 0;
    }

    /// N449 fix direction: the same method raw-self, narrow.
    unsafe fn focus_gained_raw(io: *mut Termio) {
        unsafe {
            let rs = (*io).renderer_state;
            (*(*rs).mutex).lock();
            let _mode = (*(*rs).terminal).cursor;
            (*(*rs).mutex).unlock();
        }
        std::thread::yield_now();
    }
}

/// `termio::Thread`: the io thread's own loop state.
struct IoThread {
    loop_ticks: u32,
    stop: AtomicBool,
}

/// `renderer::Thread`: the render thread's own loop state.
struct RenderThread {
    flags: u32,
    wakeup: AtomicBool,
    stop: AtomicBool,
}

/// `renderer::State`.
struct State {
    mutex: *mut Mutex,
    terminal: *mut Terminal,
    preedit: Option<Vec<u32>>,
    demand: AtomicU32,
}

/// `terminal::search::Thread`: the search thread's own loop state.
struct SearchThread {
    refresh_active: bool,
    ticks: u32,
    stop: AtomicBool,
}

struct Search {
    state: *mut SearchThread,
}

// ===========================================================================
// The other threads (each touches only its own block, through its raw root)
// ===========================================================================

#[derive(Clone, Copy)]
struct Roots {
    io: P<Termio>,
    io_thread: P<IoThread>,
    renderer_thread: P<RenderThread>,
    state: P<State>,
    search: P<SearchThread>,
}

#[derive(Clone, Copy, Default)]
struct Which {
    /// The io read thread's `processOutput` (terminal + state atomics).
    io_read: bool,
    /// The io writer thread: its own `io_thread` loop state + the mailbox drain.
    io_writer: bool,
    /// N449: the io writer thread also runs a `&mut self` Termio method.
    io_writer_termio_mut: bool,
    /// N449 fix direction: the same method raw-self.
    io_writer_termio_raw: bool,
    /// The render thread: its own `renderer_thread` state + locked terminal
    /// reads + the state atomics.
    render: bool,
    /// The search thread: its own `search.state`.
    search: bool,
    /// Only the D4 handoff atomics on `renderer_state` (`lock_demand` /
    /// `unlock_demand` / `yield_to_demand` on the io and render threads).
    state_atomics: bool,
}

fn spawn_threads(r: Roots, w: Which) -> Vec<std::thread::JoinHandle<()>> {
    let mut out = Vec::new();
    if w.io_read {
        out.push(std::thread::spawn(move || {
            let r = r;
            for _ in 0..ROUNDS {
                unsafe { Termio::process_output(r.io.0) };
                std::thread::yield_now();
            }
        }));
    }
    if w.io_writer || w.io_writer_termio_mut || w.io_writer_termio_raw {
        out.push(std::thread::spawn(move || {
            let r = r;
            for _ in 0..ROUNDS {
                unsafe {
                    if w.io_writer {
                        // `threadMain(self_: *mut Thread, io: *mut Termio)`:
                        // its own loop state, then `drainMailbox`'s
                        // `match &(*io).mailbox { .. queue.pop() }`.
                        (*r.io_thread.0).loop_ticks += 1;
                        while (*r.io.0).mailbox.pop().is_some() {}
                        let _ = (*r.io_thread.0).stop.load(Ordering::Acquire);
                    }
                    if w.io_writer_termio_mut {
                        (*r.io.0).focus_gained_mut();
                    }
                    if w.io_writer_termio_raw {
                        Termio::focus_gained_raw(r.io.0);
                    }
                }
                std::thread::yield_now();
            }
        }));
    }
    if w.state_atomics {
        out.push(std::thread::spawn(move || {
            let r = r;
            for _ in 0..ROUNDS {
                unsafe {
                    (*r.state.0).demand.fetch_add(1, Ordering::Relaxed);
                    std::thread::yield_now();
                    (*r.state.0).demand.fetch_sub(1, Ordering::Relaxed);
                }
                std::thread::yield_now();
            }
        }));
    }
    if w.render {
        out.push(std::thread::spawn(move || {
            let r = r;
            for _ in 0..ROUNDS {
                unsafe {
                    // `threadMain(self_: *mut Thread)`: its own loop state.
                    (*r.renderer_thread.0).flags += 1;
                    let _ = (*r.renderer_thread.0).wakeup.swap(false, Ordering::AcqRel);
                    // `updateFrame`: the terminal read under the state mutex.
                    let s = r.state.0;
                    (*s).demand.fetch_add(1, Ordering::Relaxed);
                    (*(*s).mutex).lock();
                    let _c = (*(*s).terminal).cursor;
                    (*(*s).mutex).unlock();
                    (*s).demand.fetch_sub(1, Ordering::Relaxed);
                }
                std::thread::yield_now();
            }
        }));
    }
    if w.search {
        out.push(std::thread::spawn(move || {
            let r = r;
            for _ in 0..ROUNDS {
                unsafe {
                    // `search.Thread.threadMain(self: *Thread)`.
                    (*r.search.0).ticks += 1;
                    (*r.search.0).refresh_active = !(*r.search.0).refresh_active;
                    let _ = (*r.search.0).stop.load(Ordering::Acquire);
                }
                std::thread::yield_now();
            }
        }));
    }
    out
}

fn new_blocks(state_mutex: *mut Mutex) -> (Termio, IoThread, RenderThread, State, SearchThread) {
    (
        Termio {
            terminal: Terminal {
                rows: vec![0; 4],
                cursor: 0,
            },
            mailbox: IoMailbox {
                queue: Box::new(StdMutex::new(VecDeque::new())),
                wakeup: AtomicBool::new(false),
            },
            size: (0, 0),
            last_cursor_reset: 0,
            renderer_state: std::ptr::null_mut(),
        },
        IoThread {
            loop_ticks: 0,
            stop: AtomicBool::new(false),
        },
        RenderThread {
            flags: 0,
            wakeup: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        },
        State {
            mutex: state_mutex,
            terminal: std::ptr::null_mut(),
            preedit: None,
            demand: AtomicU32::new(0),
        },
        SearchThread {
            refresh_active: false,
            ticks: 0,
            stop: AtomicBool::new(false),
        },
    )
}

// ===========================================================================
// SHIPPED: every block inline in the core `Surface` allocation
// ===========================================================================

struct SurfaceInline {
    size: (u32, u32),
    focused: bool,
    io: Termio,
    io_thread: IoThread,
    renderer_thread: RenderThread,
    renderer_state: State,
    search: Option<SearchInline>,
    mouse_mods: u8,
}

struct SearchInline {
    state: SearchThread,
}

impl SurfaceInline {
    /// `sizeCallback(&mut self, ..)` / `contentScaleCallback` — the GTK
    /// `render_surface_resize` / embedded `rustty_surface_set_size` entry;
    /// touches none of the shared blocks.
    fn size_callback(&mut self, w: u32, h: u32) {
        self.size = (w, h);
        std::thread::yield_now();
    }

    /// `focusCallback` / key / mouse callbacks: whole `&mut self`.
    fn focus_callback(&mut self, f: bool) {
        self.focused = f;
        self.mouse_mods = 0;
    }
}

/// Run the inline shape with only the given threads writing, so each verdict
/// names one block.
fn run_inline(w: Which) {
    let m = Box::into_raw(Box::new(Mutex::new()));
    let (io, io_thread, renderer_thread, state, search) = new_blocks(m);
    let s = P(Box::into_raw(Box::new(SurfaceInline {
        size: (0, 0),
        focused: true,
        io,
        io_thread,
        renderer_thread,
        renderer_state: state,
        search: Some(SearchInline { state: search }),
        mouse_mods: 0,
    })));
    let roots = unsafe {
        // `Surface.init`: `&self.renderer_state`, `&self.io.terminal`,
        // `&self.io`, `&self.io_thread`, `&self.renderer_thread`, and
        // `&self.search.?.state` handed to the threads.
        (*s.0).renderer_state.terminal = &raw mut (*s.0).io.terminal;
        (*s.0).io.renderer_state = &raw mut (*s.0).renderer_state;
        Roots {
            io: P(&raw mut (*s.0).io),
            io_thread: P(&raw mut (*s.0).io_thread),
            renderer_thread: P(&raw mut (*s.0).renderer_thread),
            state: P(&raw mut (*s.0).renderer_state),
            search: P(&raw mut (*s.0).search.as_mut().unwrap().state),
        }
    };
    let ts = spawn_threads(roots, w);
    for i in 0..ROUNDS as u32 {
        unsafe { &mut *s.0 }.size_callback(i, i);
        unsafe { &mut *s.0 }.focus_callback(i % 2 == 0);
        std::thread::yield_now();
    }
    for t in ts {
        t.join().unwrap();
    }
    let s = unsafe { Box::from_raw(s.0) };
    drop(s);
    drop(unsafe { Box::from_raw(m) });
}

// ===========================================================================
// FIXED: each block in its own allocation (`ThreadSharedBox`)
// ===========================================================================

/// `rustty_core::thread_shared::ThreadSharedBox<T>`: an owned raw pointer
/// with no `Drop`; freed explicitly, without drop glue, after the block's
/// own teardown (or while still uninitialized on an init-error path).
struct ThreadSharedBox<T>(*mut T);

impl<T> ThreadSharedBox<T> {
    fn new(v: T) -> Self {
        ThreadSharedBox(Box::into_raw(Box::new(v)))
    }
    fn new_uninit() -> Self {
        ThreadSharedBox(Box::into_raw(Box::<T>::new_uninit()).cast())
    }
    fn as_ptr(&self) -> *mut T {
        self.0
    }
    /// Free the allocation without running `T`'s drop glue.
    unsafe fn dealloc(&self) {
        drop(unsafe { Box::from_raw(self.0.cast::<MaybeUninit<T>>()) });
    }
}

struct SurfaceBoxed {
    size: (u32, u32),
    focused: bool,
    io: ThreadSharedBox<Termio>,
    io_thread: ThreadSharedBox<IoThread>,
    renderer_thread: ThreadSharedBox<RenderThread>,
    renderer_state: ThreadSharedBox<State>,
    search: Option<Search>,
    mouse_mods: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum QueueIo {
    /// The fix: `Termio::queueMessage(self_: *mut Termio, ..)`, and the
    /// mailbox's `&self` send.
    Narrow,
    /// The shipped `self.io.queueMessage(..)`: a whole `&mut Termio`.
    TermioMut,
    /// Raw-self `queueMessage`, but still `Mailbox::send(&mut self)`.
    SendMut,
    /// No queueIo (the N449 arms isolate the io threads).
    Skip,
}

impl SurfaceBoxed {
    fn size_callback(&mut self, w: u32, h: u32) {
        self.size = (w, h);
        std::thread::yield_now();
    }

    fn focus_callback(&mut self, f: bool) {
        self.focused = f;
        self.mouse_mods = 0;
    }

    /// `queueIo(&mut self, ..)`.
    fn queue_io(&mut self, v: u32, q: QueueIo) {
        let io = self.io.as_ptr();
        match q {
            QueueIo::Narrow => unsafe { Termio::queue_message(io, v) },
            QueueIo::TermioMut => unsafe { &mut *io }.queue_message_mut(v),
            QueueIo::SendMut => unsafe { Termio::queue_message_send_mut(io, v) },
            QueueIo::Skip => {}
        }
    }

    /// A main-thread terminal entry (selection, scroll, clipboard, ...): the
    /// reference locks `renderer_state.mutex`, then works on
    /// `self.io.terminal` — by place, through the `io` root.
    fn locked_terminal_write(&mut self) {
        unsafe {
            let m = (*self.renderer_state.as_ptr()).mutex;
            (*m).lock();
            let t = &mut (*self.io.as_ptr()).terminal;
            t.rows[1] += 1;
            (*m).unlock();
        }
    }

    /// `preeditCallback`: `renderer_state.preedit` written under the mutex,
    /// by place.
    fn preedit_callback(&mut self, v: u32) {
        unsafe {
            let rs = self.renderer_state.as_ptr();
            (*(*rs).mutex).lock();
            (*rs).preedit = Some(vec![v]);
            (*(*rs).mutex).unlock();
        }
    }

    /// `queueRender`: `renderer_thread.wakeup.notify()` (`&self`), by place.
    fn queue_render(&mut self) {
        unsafe { (*self.renderer_thread.as_ptr()).wakeup.store(true, Ordering::Release) };
    }
}

fn run_boxed(w: Which, q: QueueIo) {
    let m = Box::into_raw(Box::new(Mutex::new()));
    let (io, io_thread, renderer_thread, state, search) = new_blocks(m);
    let io_box = ThreadSharedBox::<Termio>::new_uninit();
    let state_box = ThreadSharedBox::new(state);
    unsafe {
        // `renderer_state.terminal = &self.io.terminal` (address only, before
        // the io block is written), then `Termio.init(&self.io, ..)` in place.
        (*state_box.as_ptr()).terminal = &raw mut (*io_box.as_ptr()).terminal;
        io_box.as_ptr().write(Termio {
            renderer_state: state_box.as_ptr(),
            ..io
        });
    }
    let s = P(Box::into_raw(Box::new(SurfaceBoxed {
        size: (0, 0),
        focused: true,
        io: io_box,
        io_thread: ThreadSharedBox::new(io_thread),
        renderer_thread: ThreadSharedBox::new(renderer_thread),
        renderer_state: state_box,
        search: Some(Search {
            state: ThreadSharedBox::new(search).as_ptr(),
        }),
        mouse_mods: 0,
    })));
    let roots = unsafe {
        Roots {
            io: P((*s.0).io.as_ptr()),
            io_thread: P((*s.0).io_thread.as_ptr()),
            renderer_thread: P((*s.0).renderer_thread.as_ptr()),
            state: P((*s.0).renderer_state.as_ptr()),
            search: P((*s.0).search.as_ref().unwrap().state),
        }
    };
    let ts = spawn_threads(roots, w);
    for i in 0..ROUNDS as u32 {
        unsafe { &mut *s.0 }.size_callback(i, i);
        unsafe { &mut *s.0 }.focus_callback(i % 2 == 0);
        unsafe { &mut *s.0 }.queue_io(i, q);
        unsafe { &mut *s.0 }.locked_terminal_write();
        unsafe { &mut *s.0 }.preedit_callback(i);
        unsafe { &mut *s.0 }.queue_render();
        std::thread::yield_now();
    }
    // `Surface.deinit`: stop (narrow `&self` notifies), join, then tear down.
    unsafe {
        (*(*s.0).renderer_thread.as_ptr()).stop.store(true, Ordering::Release);
        (*(*s.0).io_thread.as_ptr()).stop.store(true, Ordering::Release);
        (*(*s.0).search.as_ref().unwrap().state).stop.store(true, Ordering::Release);
    }
    for t in ts {
        t.join().unwrap();
    }
    unsafe { teardown_boxed(s.0, Owner::ManuallyDrop) };
    drop(unsafe { Box::from_raw(m) });
}

// ===========================================================================
// Teardown: every path frees each new allocation exactly once
// ===========================================================================

/// How the owner frees the core `Surface` allocation (always without glue).
#[derive(Clone, Copy)]
enum Owner {
    /// embedded `closeSurface`: `Box<ManuallyDrop<Surface>>`.
    ManuallyDrop,
    /// GTK `finalize_core_surface`: `dealloc` of the raw layout.
    Dealloc,
    /// the init-error callers: the `Box<MaybeUninit<Surface>>` slot.
    MaybeUninit,
}

unsafe fn free_surface(s: *mut SurfaceBoxed, owner: Owner) {
    unsafe {
        match owner {
            Owner::ManuallyDrop => drop(Box::from_raw(s.cast::<ManuallyDrop<SurfaceBoxed>>())),
            Owner::Dealloc => {
                std::alloc::dealloc(s.cast(), std::alloc::Layout::new::<SurfaceBoxed>())
            }
            Owner::MaybeUninit => drop(Box::from_raw(s.cast::<MaybeUninit<SurfaceBoxed>>())),
        }
    }
}

/// `Surface::deinit` after the joins: each block's own teardown (read out by
/// value, or `&mut` now that its thread is gone), then its allocation freed
/// without glue; then the owner frees the surface.
unsafe fn teardown_boxed(s: *mut SurfaceBoxed, owner: Owner) {
    unsafe {
        // search: `s.deinit()` → `state.deinit()`, then the state's box.
        if let Some(search) = (*s).search.take() {
            std::ptr::read(search.state).deinit_model();
            ThreadSharedBox(search.state).dealloc();
        }
        // self.renderer_thread.deinit();
        (*(*s).renderer_thread.as_ptr()).flags = 0;
        (*s).renderer_thread.dealloc();
        // self.io_thread.deinit(); (consuming)
        drop(std::ptr::read((*s).io_thread.as_ptr()));
        (*s).io_thread.dealloc();
        // self.io.deinit(); (consuming)
        drop(std::ptr::read((*s).io.as_ptr()));
        (*s).io.dealloc();
        // renderer_state.preedit freed, then the mutex (by the caller here).
        drop((*(*s).renderer_state.as_ptr()).preedit.take());
        (*s).renderer_state.dealloc();
        free_surface(s, owner);
    }
}

impl SearchThread {
    fn deinit_model(self) {}
}

/// The init-error stages, in `Surface.init`'s order.
#[derive(Clone, Copy)]
enum Stage {
    /// `Renderer::init` failed: the `renderer_thread` / `renderer_state`
    /// slots were allocated (their addresses are handed out) but never written.
    RendererInit,
    /// `renderer::Thread::init` failed: same, still unwritten.
    RenderThreadInit,
    /// `termio::Thread::init` failed: the render thread value is a local.
    IoThreadInit,
    /// `initErrdefers(io_live = false)`: after the big write, io unwritten.
    ErrdefersIoDead,
    /// `initErrdefers(io_live = true)`: io initialized.
    ErrdefersIoLive,
}

/// The pre-publication and `initErrdefers` paths: free every block that was
/// allocated, exactly once, whatever its init state; the caller then frees
/// the surface slot without glue.
fn run_init_error(stage: Stage, forget_io_slot: bool) {
    let m = Box::into_raw(Box::new(Mutex::new()));
    let (io, io_thread, renderer_thread, state, _search) = new_blocks(m);
    let rt_slot = ThreadSharedBox::<RenderThread>::new_uninit();
    let rs_slot = ThreadSharedBox::<State>::new_uninit();
    unsafe {
        match stage {
            Stage::RendererInit | Stage::RenderThreadInit => {
                rt_slot.dealloc();
                rs_slot.dealloc();
                drop(Box::from_raw(m));
                drop((io, io_thread, renderer_thread, state));
                return;
            }
            Stage::IoThreadInit => {
                drop(renderer_thread); // render_thread.deinit() on the local
                rt_slot.dealloc();
                rs_slot.dealloc();
                drop(Box::from_raw(m));
                drop((io, io_thread, state));
                return;
            }
            Stage::ErrdefersIoDead | Stage::ErrdefersIoLive => {}
        }
        // The big write: the slot allocated by `MaybeUninit`, fields written
        // by place.
        let slot = Box::into_raw(Box::<MaybeUninit<SurfaceBoxed>>::new_uninit()).cast::<SurfaceBoxed>();
        rt_slot.as_ptr().write(renderer_thread);
        rs_slot.as_ptr().write(state);
        let io_box = ThreadSharedBox::<Termio>::new_uninit();
        (*rs_slot.as_ptr()).terminal = &raw mut (*io_box.as_ptr()).terminal;
        slot.write(SurfaceBoxed {
            size: (0, 0),
            focused: true,
            io: io_box,
            io_thread: ThreadSharedBox::new(io_thread),
            renderer_thread: rt_slot,
            renderer_state: rs_slot,
            search: None,
            mouse_mods: 0,
        });
        let io_live = matches!(stage, Stage::ErrdefersIoLive);
        if io_live {
            (*slot).io.as_ptr().write(Termio {
                renderer_state: (*slot).renderer_state.as_ptr(),
                ..io
            });
        } else {
            drop(io);
        }
        // initErrdefers:
        if io_live {
            drop(std::ptr::read((*slot).io.as_ptr()));
        }
        if !forget_io_slot {
            (*slot).io.dealloc();
        }
        drop(std::ptr::read((*slot).io_thread.as_ptr()));
        (*slot).io_thread.dealloc();
        drop(std::ptr::read((*slot).renderer_thread.as_ptr()));
        (*slot).renderer_thread.dealloc();
        drop(Box::from_raw((*(*slot).renderer_state.as_ptr()).mutex));
        (*slot).renderer_state.dealloc();
        free_surface(slot, Owner::MaybeUninit);
    }
}

/// NEGATIVE (leak arm): a `Drop`-owning box. Every owner frees the surface
/// without glue, so the `Drop` never runs and each block leaks.
struct DropBox<T>(*mut T);
impl<T> Drop for DropBox<T> {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.0) });
    }
}

#[allow(dead_code)]
struct SurfaceDropBox {
    size: (u32, u32),
    io_thread: DropBox<IoThread>,
}

fn run_drop_box_leak() {
    let (_, io_thread, ..) = new_blocks(std::ptr::null_mut());
    let s = Box::into_raw(Box::new(SurfaceDropBox {
        size: (1, 1),
        io_thread: DropBox(Box::into_raw(Box::new(io_thread))),
    }));
    unsafe {
        (*s).size = (2, 2);
        drop(Box::from_raw(s.cast::<ManuallyDrop<SurfaceDropBox>>()));
    }
}

/// Every teardown path, with explicit frees.
fn run_teardown_all_paths() {
    // close → deinit, under each owner.
    for owner in [Owner::ManuallyDrop, Owner::Dealloc, Owner::MaybeUninit] {
        let m = Box::into_raw(Box::new(Mutex::new()));
        let (io, io_thread, renderer_thread, state, search) = new_blocks(m);
        let io_box = ThreadSharedBox::<Termio>::new_uninit();
        let state_box = ThreadSharedBox::new(state);
        unsafe {
            (*state_box.as_ptr()).terminal = &raw mut (*io_box.as_ptr()).terminal;
            io_box.as_ptr().write(Termio {
                renderer_state: state_box.as_ptr(),
                ..io
            });
            (*state_box.as_ptr()).preedit = Some(vec![1, 2]);
        }
        let s = Box::into_raw(Box::new(SurfaceBoxed {
            size: (0, 0),
            focused: true,
            io: io_box,
            io_thread: ThreadSharedBox::new(io_thread),
            renderer_thread: ThreadSharedBox::new(renderer_thread),
            renderer_state: state_box,
            search: Some(Search {
                state: ThreadSharedBox::new(search).as_ptr(),
            }),
            mouse_mods: 0,
        }));
        unsafe {
            teardown_boxed(s, owner);
        }
        drop(unsafe { Box::from_raw(m) });
    }
    // every init-error stage.
    for stage in [
        Stage::RendererInit,
        Stage::RenderThreadInit,
        Stage::IoThreadInit,
        Stage::ErrdefersIoDead,
        Stage::ErrdefersIoLive,
    ] {
        run_init_error(stage, false);
    }
}


// ===========================================================================
// Fix round 1 (ruling 48): the search thread's callback (`searchCallback_`)
// ===========================================================================
//
// `searchCallback_` runs on the SEARCH thread with the raw `event_userdata`.
// Round 0 read the surface's own bytes through it: the `renderer_thread` box
// pointer (for the renderer mailbox / wakeup) and `rt_app` / `app` (through
// `surface_mailbox_raw`). Those are non-atomic reads of `Surface` bytes while
// the main thread forms `&mut Surface`. The fix: a small boxed context captured
// at spawn (the renderer mailbox and wakeup pointers, a copy of the surface
// mailbox) is the userdata, so the callback never reads the surface.

/// The core `App`: its mailbox is its own queue (outside the surface).
struct AppModel {
    mailbox: StdMutex<VecDeque<u32>>,
}

/// The core `Surface` with the fields the callback used to read.
struct SurfaceCb {
    size: (u32, u32),
    focused: bool,
    renderer_thread: ThreadSharedBox<RenderThread>,
    rt_app: *mut AppModel,
    app: *mut AppModel,
    mouse_mods: u8,
}

impl SurfaceCb {
    fn size_callback(&mut self, w: u32, h: u32) {
        self.size = (w, h);
        std::thread::yield_now();
    }
    fn focus_callback(&mut self, f: bool) {
        self.focused = f;
        self.mouse_mods = 0;
    }
}

/// The fix's userdata: `SearchCallbackCtx { renderer_mailbox, renderer_wakeup,
/// surface_mailbox }`, all raw pointers captured on the main thread at spawn.
struct SearchCallbackCtx {
    renderer_wakeup: *const AtomicBool,
    surface_mailbox_app: *mut AppModel,
}

/// Round 0: `searchCallback_(self_: *mut Surface, ..)`.
unsafe fn search_callback_surface(self_: *mut SurfaceCb) {
    unsafe {
        // `(*(*self_).renderer_thread.as_ptr()).wakeup.notify()`.
        let rt = (*self_).renderer_thread.as_ptr();
        (*rt).wakeup.store(true, Ordering::Release);
        // `surface_mailbox_raw(self_)`: reads `rt_app` and `app`.
        let _rt_app = (*self_).rt_app;
        (*(*self_).app).mailbox.lock().unwrap().push_back(1);
    }
}

/// The fix: `searchCallback_(ctx: *const SearchCallbackCtx, ..)`.
unsafe fn search_callback_ctx(ctx: *const SearchCallbackCtx) {
    unsafe {
        (*(*ctx).renderer_wakeup).store(true, Ordering::Release);
        (*(*ctx).surface_mailbox_app).mailbox.lock().unwrap().push_back(1);
    }
}

fn run_search_callback(fixed: bool) {
    let app = Box::into_raw(Box::new(AppModel {
        mailbox: StdMutex::new(VecDeque::new()),
    }));
    let (_, _, renderer_thread, ..) = new_blocks(std::ptr::null_mut());
    let s = P(Box::into_raw(Box::new(SurfaceCb {
        size: (0, 0),
        focused: true,
        renderer_thread: ThreadSharedBox::new(renderer_thread),
        rt_app: app,
        app,
        mouse_mods: 0,
    })));
    // The spawn (main thread): the context is captured here, before the
    // search thread starts.
    let ctx = P(Box::into_raw(Box::new(SearchCallbackCtx {
        renderer_wakeup: unsafe { &raw const (*(*s.0).renderer_thread.as_ptr()).wakeup },
        surface_mailbox_app: unsafe { (*s.0).app },
    })));
    let t = std::thread::spawn(move || {
        let (s, ctx) = (s, ctx);
        for _ in 0..ROUNDS {
            unsafe {
                if fixed {
                    search_callback_ctx(ctx.0);
                } else {
                    search_callback_surface(s.0);
                }
            }
            std::thread::yield_now();
        }
    });
    for i in 0..ROUNDS as u32 {
        unsafe { &mut *s.0 }.size_callback(i, i);
        unsafe { &mut *s.0 }.focus_callback(i % 2 == 0);
        std::thread::yield_now();
    }
    t.join().unwrap();
    unsafe {
        // `Search::deinit`: after the join, the context is freed once.
        drop(Box::from_raw(ctx.0));
        (*s.0).renderer_thread.dealloc();
        drop(Box::from_raw(s.0));
        drop(Box::from_raw(app));
    }
}

// ===========================================================================
// N449 (P7-W7 T5d): the io writer thread's `Termio` methods, the backend
// chain, and the main thread's backend reads
// ===========================================================================
//
// The io writer thread runs `Termio` methods (`focusGained`, `resize`,
// `changeConfig`, `queueWrite`, ... and `threadExit` after its loop ends).
// Shipped, each is `&mut self`: a protected Unique over the whole `Termio`,
// terminal included, while the io read thread writes the terminal under the
// renderer mutex. They reach the backend through `&mut Backend` -> `&mut Exec`
// -> `&mut Subprocess` -> `&mut Pty` (`resize` -> `setSize`; `threadExit` ->
// `stop`). The main thread meanwhile reads the backend: `getForegroundPid`,
// `getTtyName` (which WRITES the pty's tty-name cache), and the close-confirm
// `childExitedAbnormally` read of `subprocess.args`.
//
// The fix modelled here, as shipped in the port:
// - every writer-thread `Termio` method is raw-self and reaches only the
//   fields it touches, by place (the `processOutput` shape);
// - the `Backend` / `Exec` / `Subprocess` cross-thread methods are raw-self
//   (`Backend` is `#[repr(transparent)]`, so `*mut Backend` casts to
//   `*mut Exec` without a retag);
// - the pty's `setSize` / `getForegroundPid` / `getTtyName` take `&self`, and
//   the tty-name cache is interior-mutable (`UnsafeCell` buffer, `Cell`
//   length), so a shared pty reference covers none of the cache bytes;
// - the close-confirm read borrows `subprocess.args` only.

use std::cell::{Cell, UnsafeCell};

/// `rustty_os::pty::PosixPty` (the fields the model's threads touch).
struct N449Pty {
    master: i32,
    tty_name_buf: UnsafeCell<[u8; 8]>,
    tty_name: Cell<Option<usize>>,
}

impl N449Pty {
    /// SHIPPED `setSize(&mut self, ..)`: a protected Unique over the whole pty.
    fn set_size_mut(&mut self, rows: u16) -> i32 {
        std::hint::black_box(self.master) + i32::from(rows)
    }
    /// FIXED `setSize(&self, ..)`: reads `master` only.
    fn set_size(&self, rows: u16) -> i32 {
        std::hint::black_box(self.master) + i32::from(rows)
    }
    /// SHIPPED `getForegroundPid(&mut self)`.
    fn get_foreground_pid_mut(&mut self) -> Option<u64> {
        Some(self.master as u64)
    }
    /// FIXED `getForegroundPid(&self)`.
    fn get_foreground_pid(&self) -> Option<u64> {
        Some(self.master as u64)
    }
    /// SHIPPED `getTtyName(&mut self)`: fills the cache on first use.
    fn get_tty_name_mut(&mut self) -> Option<usize> {
        if let Some(len) = self.tty_name.get() {
            return Some(len);
        }
        self.tty_name_buf.get_mut()[0] = b'p';
        self.tty_name.set(Some(1));
        Some(1)
    }
    /// FIXED `getTtyName(&self)`: the cache is interior-mutable.
    fn get_tty_name(&self) -> Option<usize> {
        if let Some(len) = self.tty_name.get() {
            return Some(len);
        }
        unsafe { (*self.tty_name_buf.get())[0] = b'p' };
        self.tty_name.set(Some(1));
        Some(1)
    }
}

/// `termio::exec::Subprocess`.
struct N449Subprocess {
    args: Vec<u32>,
    grid_size: (u16, u16),
    pty: Option<N449Pty>,
    process: Option<u32>,
}

impl N449Subprocess {
    fn resize_mut(&mut self, g: (u16, u16)) {
        self.grid_size = g;
        if let Some(pty) = &mut self.pty {
            pty.set_size_mut(g.0);
        }
    }
    /// FIXED raw-self; `pty_shared == false` keeps the shipped `&mut Pty`
    /// `setSize` (a partial arm).
    unsafe fn resize(self_: *mut Self, g: (u16, u16), pty_shared: bool) {
        unsafe {
            (*self_).grid_size = g;
            if pty_shared {
                if let Some(pty) = &(*self_).pty {
                    pty.set_size(g.0);
                }
            } else if let Some(pty) = &mut (*self_).pty {
                pty.set_size_mut(g.0);
            }
        }
    }
    fn get_foreground_pid_mut(&mut self) -> Option<u64> {
        self.pty.as_mut()?.get_foreground_pid_mut()
    }
    unsafe fn get_foreground_pid(self_: *mut Self) -> Option<u64> {
        unsafe { (*self_).pty.as_ref()?.get_foreground_pid() }
    }
    fn get_tty_name_mut(&mut self) -> Option<usize> {
        self.pty.as_mut()?.get_tty_name_mut()
    }
    unsafe fn get_tty_name(self_: *mut Self) -> Option<usize> {
        unsafe { (*self_).pty.as_ref()?.get_tty_name() }
    }
    fn stop_mut(&mut self) {
        self.process = None;
    }
    unsafe fn stop(self_: *mut Self) {
        unsafe { (*self_).process = None };
    }
}

/// `termio::exec::Exec`.
struct N449Exec {
    subprocess: N449Subprocess,
}

/// `termio::backend::Backend` (`#[repr(transparent)]` in the fix).
#[repr(transparent)]
enum N449Backend {
    Exec(N449Exec),
}

impl N449Backend {
    /// FIXED: `Backend::exec_ptr`, the single-variant projection without a
    /// retag (sound because of `#[repr(transparent)]`).
    fn exec_ptr(self_: *mut Self) -> *mut N449Exec {
        self_.cast()
    }
    // SHIPPED `&mut self` delegation (`match self { Backend::Exec(exec) => .. }`).
    fn resize_mut(&mut self, g: (u16, u16)) {
        match self {
            N449Backend::Exec(exec) => exec.subprocess.resize_mut(g),
        }
    }
    fn focus_gained_mut(&mut self) {
        match self {
            N449Backend::Exec(exec) => {
                let _ = exec;
            }
        }
    }
    fn queue_write_mut(&mut self) {
        match self {
            N449Backend::Exec(exec) => {
                let _ = exec;
            }
        }
    }
    fn thread_exit_mut(&mut self) {
        match self {
            N449Backend::Exec(exec) => exec.subprocess.stop_mut(),
        }
    }
    fn get_foreground_pid_mut(&mut self) -> Option<u64> {
        match self {
            N449Backend::Exec(exec) => exec.subprocess.get_foreground_pid_mut(),
        }
    }
    fn get_tty_name_mut(&mut self) -> Option<usize> {
        match self {
            N449Backend::Exec(exec) => exec.subprocess.get_tty_name_mut(),
        }
    }
    // FIXED raw-self delegation.
    unsafe fn resize(self_: *mut Self, g: (u16, u16), pty_shared: bool) {
        let exec = Self::exec_ptr(self_);
        unsafe { N449Subprocess::resize(&raw mut (*exec).subprocess, g, pty_shared) }
    }
    unsafe fn thread_exit(self_: *mut Self) {
        let exec = Self::exec_ptr(self_);
        unsafe { N449Subprocess::stop(&raw mut (*exec).subprocess) }
    }
    unsafe fn get_foreground_pid(self_: *mut Self) -> Option<u64> {
        let exec = Self::exec_ptr(self_);
        unsafe { N449Subprocess::get_foreground_pid(&raw mut (*exec).subprocess) }
    }
    unsafe fn get_tty_name(self_: *mut Self) -> Option<usize> {
        let exec = Self::exec_ptr(self_);
        unsafe { N449Subprocess::get_tty_name(&raw mut (*exec).subprocess) }
    }
}

/// `StreamHandler`: holds the terminal pointer the read thread writes through.
struct N449Handler {
    terminal: *mut Terminal,
    enquiry: u32,
}

/// `terminal_stream: Stream(StreamHandler)`.
struct N449Stream {
    handler: N449Handler,
    parser: u32,
}

/// `termio::Termio` with the fields the writer, read and main threads touch.
struct N449Termio {
    terminal: Terminal,
    backend: N449Backend,
    config: u32,
    terminal_stream: N449Stream,
    mailbox: IoMailbox,
    size: (u32, u32),
    last_cursor_reset: u32,
    renderer_state: *mut State,
    renderer_wakeup: AtomicBool,
}

impl N449Termio {
    /// `processOutput(io: *mut Termio, ..)`: the read thread (raw-self, as
    /// shipped). The stream parses and its handler writes the terminal through
    /// its own pointer, under the renderer mutex.
    unsafe fn process_output(io: *mut Self) {
        unsafe {
            let rs = (*io).renderer_state;
            (*(*rs).mutex).lock();
            (*io).terminal_stream.parser += 1;
            let t = (*io).terminal_stream.handler.terminal;
            (*t).cursor += 1;
            (&mut (*t).rows)[0] += 1;
            (*io).last_cursor_reset += 1;
            (*(*rs).mutex).unlock();
        }
    }

    // ---- SHIPPED: `&mut self` writer-thread methods ----

    fn focus_gained_mut(&mut self) {
        unsafe {
            let rs = self.renderer_state;
            (*(*rs).mutex).lock();
            let _mode = (*(*rs).terminal).cursor;
            (*(*rs).mutex).unlock();
        }
        self.queue_write_mut();
        self.backend.focus_gained_mut();
    }
    fn resize_mut(&mut self, g: (u16, u16)) {
        self.size = (u32::from(g.0), u32::from(g.1));
        self.backend.resize_mut(g);
        unsafe { (*(*self.renderer_state).mutex).lock() };
        self.terminal.rows[1] += 1;
        unsafe { (*(*self.renderer_state).mutex).unlock() };
        self.renderer_wakeup.store(true, Ordering::Release);
    }
    fn change_config_mut(&mut self, c: u32) {
        unsafe { (*(*self.renderer_state).mutex).lock() };
        self.config = c;
        self.terminal_stream.handler.enquiry = self.config;
        unsafe { (*self.terminal_stream.handler.terminal).cursor += 1 };
        self.terminal.cursor = 0;
        unsafe { (*(*self.renderer_state).mutex).unlock() };
    }
    fn queue_write_mut(&mut self) {
        self.backend.queue_write_mut();
    }
    fn thread_exit_mut(&mut self) {
        self.backend.thread_exit_mut();
    }

    // ---- FIXED: raw-self, narrow ----

    unsafe fn focus_gained(self_: *mut Self) {
        unsafe {
            let rs = (*self_).renderer_state;
            (*(*rs).mutex).lock();
            let _mode = (*(*rs).terminal).cursor;
            (*(*rs).mutex).unlock();
            Self::queue_write(self_);
            // `Backend::focusGained(&raw mut (*self_).backend, td, ..)`: the
            // exec half touches only `td`.
            let _ = N449Backend::exec_ptr(&raw mut (*self_).backend);
        }
    }
    unsafe fn resize(self_: *mut Self, g: (u16, u16), pty_shared: bool) {
        unsafe {
            (*self_).size = (u32::from(g.0), u32::from(g.1));
            N449Backend::resize(&raw mut (*self_).backend, g, pty_shared);
            let rs = (*self_).renderer_state;
            (*(*rs).mutex).lock();
            (&mut (*self_).terminal.rows)[1] += 1;
            (*(*rs).mutex).unlock();
            (*self_).renderer_wakeup.store(true, Ordering::Release);
        }
    }
    /// `terminal_early` borrows the terminal place BEFORE the handler writes
    /// the terminal through its own pointer (a negative arm).
    unsafe fn change_config(self_: *mut Self, c: u32, terminal_early: bool) {
        unsafe {
            let rs = (*self_).renderer_state;
            (*(*rs).mutex).lock();
            if terminal_early {
                let t = &mut (*self_).terminal;
                t.cursor = 1;
                (*(*self_).terminal_stream.handler.terminal).cursor += 1;
                t.cursor = 0;
                (*(*rs).mutex).unlock();
                return;
            }
            // As shipped: `config` and `terminal_stream` borrowed by place; the
            // handler's `changeConfig` writes the terminal through its own
            // pointer (`set_default_cursor_style`); the terminal place is
            // borrowed only after it.
            let this_config = &mut (*self_).config;
            let terminal_stream = &mut (*self_).terminal_stream;
            *this_config = c;
            terminal_stream.handler.enquiry = *this_config;
            (*terminal_stream.handler.terminal).cursor += 1;
            let t = &mut (*self_).terminal;
            t.cursor = 0;
            (*(*rs).mutex).unlock();
        }
    }
    unsafe fn queue_write(self_: *mut Self) {
        // `Backend::queueWrite(&raw mut (*self_).backend, ..)`: the exec half
        // touches only `td`.
        let _ = N449Backend::exec_ptr(unsafe { &raw mut (*self_).backend });
    }
    unsafe fn thread_exit(self_: *mut Self) {
        unsafe { N449Backend::thread_exit(&raw mut (*self_).backend) };
    }
}

/// Which pieces of the fix are applied (all `true` is the shipped fix).
#[derive(Clone, Copy)]
struct N449Shape {
    /// Writer-thread `Termio` methods raw-self (else `&mut self`).
    termio_raw: bool,
    /// The main thread's `getForegroundPid` / `getTtyName` reach the backend
    /// raw-self (else the T5c `&mut Backend` -> `&mut Pty` chain). The raw
    /// writer methods always use the raw backend chain.
    backend_raw: bool,
    /// The pty's `setSize` takes `&self` (else `&mut self`).
    pty_shared: bool,
    /// The close-confirm read borrows `subprocess.args` only (else a whole
    /// `&Backend`).
    close_confirm_narrow: bool,
    /// `changeConfig` borrows the terminal before the stream handler writes
    /// it through its own pointer (else after, as shipped).
    terminal_early: bool,
}

fn run_n449(shape: N449Shape) {
    let m = Box::into_raw(Box::new(Mutex::new()));
    let (_, _, _, state, _) = new_blocks(m);
    let state_box = ThreadSharedBox::new(state);
    let io_box = ThreadSharedBox::<N449Termio>::new_uninit();
    unsafe {
        let io = io_box.as_ptr();
        // `renderer_state.terminal = &self.io.terminal`, then `Termio.init`
        // in place: the stream handler's terminal pointer is the same place.
        (*state_box.as_ptr()).terminal = &raw mut (*io).terminal;
        io.write(N449Termio {
            terminal: Terminal {
                rows: vec![0; 4],
                cursor: 0,
            },
            backend: N449Backend::Exec(N449Exec {
                subprocess: N449Subprocess {
                    args: vec![1, 2, 3],
                    grid_size: (80, 24),
                    pty: Some(N449Pty {
                        master: 7,
                        tty_name_buf: UnsafeCell::new([0; 8]),
                        tty_name: Cell::new(None),
                    }),
                    process: Some(42),
                },
            }),
            config: 0,
            terminal_stream: N449Stream {
                handler: N449Handler {
                    terminal: &raw mut (*io).terminal,
                    enquiry: 0,
                },
                parser: 0,
            },
            mailbox: IoMailbox {
                queue: Box::new(StdMutex::new(VecDeque::new())),
                wakeup: AtomicBool::new(false),
            },
            size: (0, 0),
            last_cursor_reset: 0,
            renderer_state: state_box.as_ptr(),
            renderer_wakeup: AtomicBool::new(false),
        });
    }
    let io = P(io_box.as_ptr());

    // The io read thread.
    let reader = std::thread::spawn(move || {
        let io = io;
        for _ in 0..ROUNDS {
            unsafe { N449Termio::process_output(io.0) };
            std::thread::yield_now();
        }
    });
    // The io writer thread: `drainMailbox` and the coalesce / sync-reset
    // timers, then `threadExit` once its loop ends (the read thread still runs
    // until `threadExit` joins it).
    let writer = std::thread::spawn(move || {
        let io = io;
        for i in 0..ROUNDS as u16 {
            unsafe {
                if shape.termio_raw {
                    N449Termio::focus_gained(io.0);
                    N449Termio::resize(io.0, (80 + i, 24), shape.pty_shared);
                    N449Termio::change_config(io.0, u32::from(i), shape.terminal_early);
                    N449Termio::queue_write(io.0);
                } else {
                    (*io.0).focus_gained_mut();
                    (*io.0).resize_mut((80 + i, 24));
                    (*io.0).change_config_mut(u32::from(i));
                    (*io.0).queue_write_mut();
                }
                while (*io.0).mailbox.pop().is_some() {}
            }
            std::thread::yield_now();
        }
        unsafe {
            if shape.termio_raw {
                N449Termio::thread_exit(io.0);
            } else {
                (*io.0).thread_exit_mut();
            }
        }
    });
    // The main thread: `queueIo`, `getForegroundPid` / `getTtyName` (the T5c
    // raw-self `Termio` entries), the close-confirm command read, and a locked
    // terminal write by place.
    for i in 0..ROUNDS as u32 {
        unsafe {
            let io = io.0;
            let rs = (*io).renderer_state;
            (*io).mailbox.send(i);
            if shape.backend_raw {
                let _ = N449Backend::get_foreground_pid(&raw mut (*io).backend);
                let _ = N449Backend::get_tty_name(&raw mut (*io).backend);
            } else {
                let _ = (*io).backend.get_foreground_pid_mut();
                let _ = (*io).backend.get_tty_name_mut();
            }
            let n = if shape.close_confirm_narrow {
                let exec = N449Backend::exec_ptr(&raw mut (*io).backend);
                let args = &(*exec).subprocess.args;
                args.len()
            } else {
                match &(*io).backend {
                    N449Backend::Exec(exec) => exec.subprocess.args.len(),
                }
            };
            std::hint::black_box(n);
            (*(*rs).mutex).lock();
            let t = &mut (*io).terminal;
            t.rows[2] += 1;
            (*(*rs).mutex).unlock();
        }
        std::thread::yield_now();
    }
    writer.join().unwrap();
    reader.join().unwrap();
    unsafe {
        drop(std::ptr::read(io_box.as_ptr()));
        io_box.dealloc();
        state_box.dealloc();
        drop(Box::from_raw(m));
    }
}

// ===========================================================================
// UD367 (P7-W7 T5d fix round 1): `Subprocess.start` on the io thread
// publishes the pty while a main-thread `getProcessInfo` reads it
// ===========================================================================
//
// `Exec::threadEnter` (io thread) runs `subprocess.start(&mut Subprocess)`,
// which writes `pty` (and `process`). The embedded
// `rustty_surface_foreground_pid` / `rustty_surface_tty_name` (AppleScript
// `pid` / `tty`, App Intents) may run `getProcessInfo` on the main thread at
// the same time: a read of `pty` racing its write, in the reference too.
//
// The fix: a `pty_ready: AtomicBool` the io thread stores `Release` after
// `start` returns; the main thread's accessors load it `Acquire` and return
// `None` (as before `start`) until it is set. It lives in `Exec`, beside
// `subprocess`, not inside `Subprocess`: `start`'s `&mut Subprocess` retag
// covers every byte of the subprocess, so a flag inside it races with the
// main thread's load (the `ud367_ready_in_subprocess` arm).

/// `termio::exec::Subprocess` (the startup fields).
struct Ud367Subprocess {
    pty: Option<N449Pty>,
    process: Option<u32>,
    /// Only the `ud367_ready_in_subprocess` arm uses this placement.
    pty_ready_inner: AtomicBool,
}

impl Ud367Subprocess {
    /// `start(&mut self, ..)`: opens the pty and spawns the child.
    fn start(&mut self) {
        self.pty = Some(N449Pty {
            master: 9,
            tty_name_buf: UnsafeCell::new([0; 8]),
            tty_name: Cell::new(None),
        });
        std::thread::yield_now();
        self.process = Some(7);
    }
}

/// `termio::exec::Exec` with the fix's flag.
struct Ud367Exec {
    subprocess: Ud367Subprocess,
    pty_ready: AtomicBool,
}

#[derive(Clone, Copy, PartialEq)]
enum Ud367Shape {
    /// SHIPPED (and the reference): no ordering.
    Unordered,
    /// The fix: `Exec.pty_ready`.
    ReadyInExec,
    /// A flag inside `Subprocess` (covered by `start`'s `&mut`).
    ReadyInSubprocess,
}

fn run_ud367(shape: Ud367Shape) {
    let exec = P(Box::into_raw(Box::new(Ud367Exec {
        subprocess: Ud367Subprocess {
            pty: None,
            process: None,
            pty_ready_inner: AtomicBool::new(false),
        },
        pty_ready: AtomicBool::new(false),
    })));
    // The io thread: `Exec::threadEnter(self_: *mut Exec, ..)`.
    let io = std::thread::spawn(move || {
        let exec = exec;
        unsafe {
            (*exec.0).subprocess.start();
            match shape {
                Ud367Shape::Unordered => {}
                Ud367Shape::ReadyInExec => (*exec.0).pty_ready.store(true, Ordering::Release),
                Ud367Shape::ReadyInSubprocess => {
                    (*exec.0).subprocess.pty_ready_inner.store(true, Ordering::Release)
                }
            }
        }
    });
    // The main thread: `getTtyName` / `getForegroundPid` (raw-self, through
    // `&Pty`), as a script's `tty of terminal 1` right after `make new window`.
    for _ in 0..ROUNDS {
        unsafe {
            let ready = match shape {
                Ud367Shape::Unordered => true,
                Ud367Shape::ReadyInExec => (*exec.0).pty_ready.load(Ordering::Acquire),
                Ud367Shape::ReadyInSubprocess => {
                    (*exec.0).subprocess.pty_ready_inner.load(Ordering::Acquire)
                }
            };
            if ready {
                if let Some(pty) = (*exec.0).subprocess.pty.as_ref() {
                    let _ = pty.get_tty_name();
                    let _ = pty.get_foreground_pid();
                }
            }
        }
        std::thread::yield_now();
    }
    io.join().unwrap();
    drop(unsafe { Box::from_raw(exec.0) });
}

// ===========================================================================
// UD368 (P7-W7 T5d fix round 1): `Termio.resize` writes `size` before it
// takes the renderer mutex, while the read thread reads it under the mutex
// ===========================================================================
//
// `resize` (io writer thread) does `self.size = size` before
// `renderer_state.mutex.lock()`; the stream handler's `.enable_mode_3` (io
// read thread, under the mutex) reads `self.size.*` through its `size`
// pointer. Program output that sets mode 3 during a window resize races the
// write, in the reference too. The fix: store `size` inside the mutex section.

struct Ud368Size {
    cols: u32,
    rows: u32,
}

struct Ud368Termio {
    size: Ud368Size,
    /// `StreamHandler.size: *mut Size` (points at `Termio.size`).
    handler_size: *mut Ud368Size,
    renderer_state: *mut State,
    terminal_cols: u32,
}

fn run_ud368(under_lock: bool) {
    let m = Box::into_raw(Box::new(Mutex::new()));
    let (_, _, _, state, _) = new_blocks(m);
    let state_box = ThreadSharedBox::new(state);
    let io_box = ThreadSharedBox::<Ud368Termio>::new_uninit();
    unsafe {
        let io = io_box.as_ptr();
        io.write(Ud368Termio {
            size: Ud368Size { cols: 80, rows: 24 },
            handler_size: &raw mut (*io).size,
            renderer_state: state_box.as_ptr(),
            terminal_cols: 80,
        });
    }
    let io = P(io_box.as_ptr());
    // The read thread: `processOutput` -> `.enable_mode_3`, under the mutex.
    let reader = std::thread::spawn(move || {
        let io = io;
        for _ in 0..ROUNDS {
            unsafe {
                let rs = (*io.0).renderer_state;
                (*(*rs).mutex).lock();
                let s = (*io.0).handler_size;
                let cols = (*s).cols;
                let _rows = (*s).rows;
                (*io.0).terminal_cols = cols;
                (*(*rs).mutex).unlock();
            }
            std::thread::yield_now();
        }
    });
    // The writer thread: `Termio::resize(self_: *mut Termio, ..)`.
    let writer = std::thread::spawn(move || {
        let io = io;
        for i in 0..ROUNDS as u32 {
            unsafe {
                let rs = (*io.0).renderer_state;
                if !under_lock {
                    (*io.0).size = Ud368Size {
                        cols: 100 + i,
                        rows: 30,
                    };
                }
                // `Backend::resize(..)` with the size as arguments.
                std::thread::yield_now();
                (*(*rs).mutex).lock();
                if under_lock {
                    (*io.0).size = Ud368Size {
                        cols: 100 + i,
                        rows: 30,
                    };
                }
                (*io.0).terminal_cols = (*io.0).size.cols;
                (*(*rs).mutex).unlock();
            }
            std::thread::yield_now();
        }
    });
    writer.join().unwrap();
    reader.join().unwrap();
    unsafe {
        io_box.dealloc();
        state_box.dealloc();
        drop(Box::from_raw(m));
    }
}

fn main() {
    let which = std::env::args().nth(1).expect("candidate name");
    let all = Which {
        io_read: true,
        io_writer: true,
        render: true,
        search: true,
        state_atomics: true,
        ..Which::default()
    };
    match which.as_str() {
        // ---- SHIPPED: one writer at a time against the inline surface ----
        "n444_inline_io_terminal" => run_inline(Which {
            io_read: true,
            ..Which::default()
        }),
        "n444_inline_io_thread" => run_inline(Which {
            io_writer: true,
            ..Which::default()
        }),
        "n444_inline_renderer_thread_state" => run_inline(Which {
            render: true,
            ..Which::default()
        }),
        "n444_inline_renderer_state_atomics" => run_inline(Which {
            state_atomics: true,
            ..Which::default()
        }),
        "n444_inline_search_state" => run_inline(Which {
            search: true,
            ..Which::default()
        }),
        // ---- the fix: every block boxed, every writer running ----
        "n444_boxed_all" => run_boxed(all, QueueIo::Narrow),
        // The fix's boxes, but the shipped `self.io.queueMessage` (&mut Termio).
        "n444_boxed_queue_io_termio_mut" => run_boxed(
            Which {
                io_read: true,
                ..Which::default()
            },
            QueueIo::TermioMut,
        ),
        // Raw-self `queueMessage`, but `Mailbox::send(&mut self)`.
        "n444_boxed_queue_io_send_mut" => run_boxed(
            Which {
                io_writer: true,
                ..Which::default()
            },
            QueueIo::SendMut,
        ),
        // ---- teardown ----
        "n444_drop_box_leak" => run_drop_box_leak(),
        "n444_init_error_io_slot_leak" => run_init_error(Stage::ErrdefersIoDead, true),
        "n444_teardown_all_paths" => run_teardown_all_paths(),
        // ---- N449 (not the Surface): the io writer thread's `&mut Termio` ----
        // ---- fix round 1: the search callback ----
        "n444_search_cb_reads_surface" => run_search_callback(false),
        "n444_search_cb_ctx" => run_search_callback(true),
        "n449_writer_termio_mut" => run_boxed(
            Which {
                io_read: true,
                io_writer_termio_mut: true,
                ..Which::default()
            },
            QueueIo::Skip,
        ),
        "n449_writer_termio_raw" => run_boxed(
            Which {
                io_read: true,
                io_writer_termio_raw: true,
                ..Which::default()
            },
            QueueIo::Skip,
        ),
        // ---- N449 (P7-W7 T5d): the shipped writer-thread methods, the
        // backend chain and the close-confirm read ----
        "n449_shipped" => run_n449(N449Shape {
            termio_raw: false,
            backend_raw: false,
            pty_shared: false,
            close_confirm_narrow: false,
            terminal_early: false,
        }),
        "n449_main_backend_mut" => run_n449(N449Shape {
            termio_raw: true,
            backend_raw: false,
            pty_shared: true,
            close_confirm_narrow: true,
            terminal_early: false,
        }),
        "n449_pty_set_size_mut" => run_n449(N449Shape {
            termio_raw: true,
            backend_raw: true,
            pty_shared: false,
            close_confirm_narrow: true,
            terminal_early: false,
        }),
        "n449_close_confirm_backend_ref" => run_n449(N449Shape {
            termio_raw: true,
            backend_raw: true,
            pty_shared: true,
            close_confirm_narrow: false,
            terminal_early: false,
        }),
        "n449_fixed" => run_n449(N449Shape {
            termio_raw: true,
            backend_raw: true,
            pty_shared: true,
            close_confirm_narrow: true,
            terminal_early: false,
        }),
        "n449_change_config_terminal_early" => run_n449(N449Shape {
            termio_raw: true,
            backend_raw: true,
            pty_shared: true,
            close_confirm_narrow: true,
            terminal_early: true,
        }),
        // ---- UD367 / UD368 (T5d fix round 1) ----
        "ud367_start_unordered" => run_ud367(Ud367Shape::Unordered),
        "ud367_ready_in_subprocess" => run_ud367(Ud367Shape::ReadyInSubprocess),
        "ud367_ready_in_exec" => run_ud367(Ud367Shape::ReadyInExec),
        "ud368_size_before_lock" => run_ud368(false),
        "ud368_size_under_lock" => run_ud368(true),
        other => panic!("unknown candidate {other}"),
    }
}
