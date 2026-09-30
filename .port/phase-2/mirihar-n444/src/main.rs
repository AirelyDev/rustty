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
        other => panic!("unknown candidate {other}"),
    }
}
