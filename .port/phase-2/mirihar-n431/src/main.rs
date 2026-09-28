//! N431 / N430: rustty-xev `Loop::run(&mut self)` held a protected `&mut Loop`
//! across every callback; callbacks re-enter the loop through the stored root
//! (termio `td.loop_`) or through their `l` argument. Isolated, non-FFI model of
//! the real borrow shapes (see README.md). One candidate per argv[1].
//!
//! Real-code map (crates/rustty-xev/src):
//!   Loop::run / tick / tick_inner      kqueue.rs, epoll.rs, io_uring.rs, iocp.rs, wasi_poll.rs
//!   Loop::start (sync callback arm)    epoll.rs `start`, wasi_poll.rs `start`, iocp.rs `start_completion`
//!   Loop::async_notify(&self)          wasi_poll.rs `async_notify` (writes Loop.wakeup)
//!   Async::wait / notify               wasi_watcher.rs `wait` / `notify`
//!   termio re-entry                    thread.rs `(*self_).loop_.run`, termio_impl.rs `loop_: &raw mut (*thread).loop_`,
//!                                      exec.rs `td.loop_` -> stream.rs `(*loop_).add`, thread.rs `(*self_).loop_.stop()`
//!   exec.rs tests                      `let mut loop_; loop_ptr = addr_of_mut!(loop_); ...; loop_.run(NoWait)`
#![allow(clippy::missing_safety_doc, static_mut_refs, dead_code)]

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

// ---------------------------------------------------------------------------
// Candidate configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Shape {
    /// The shipped code: run/tick/tick_inner/start all `&mut self`.
    Shipped,
    /// The fix: `run(&mut self)` is a thin wrapper over `run_raw(this: *mut Loop)`;
    /// tick/tick_inner/start are raw-self with narrow `(*this).field` access.
    Raw,
    /// Raw tick path, but the callback-invoking `start` kept as `&mut self`
    /// (shows why `start`/`stop_completion` must be raw-self too).
    RawMutStart,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Entry {
    /// termio Thread: `(*self_).loop_.run(..)` (shipped / wrapper) or
    /// `Loop::run_raw(&raw mut (*self_).loop_, ..)` (fix).
    ThreadMethod,
    ThreadRaw,
    /// exec.rs tests: a stack `loop_` + `loop_ptr = addr_of_mut!(loop_)`, then
    /// `loop_.run(..)` (shipped / wrapper) or `Loop::run_raw(loop_ptr, ..)` (fix).
    LocalMethod,
    LocalRaw,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Reentry {
    /// Through the stored root (`td.loop_`): `(*loop_).add(c)`, `(*loop_).stop()`.
    Root,
    /// Through the callback's `l`: `(&mut *l).add(c)`, `(*l).stop()`.
    L,
    /// WASI: a timer callback notifies an Async whose waiter holds the root;
    /// `(*w.loop_).async_notify(w.c)` writes `Loop.wakeup`.
    Wasi,
    /// A timer callback rearms into an op whose `start` invokes the callback
    /// synchronously (epoll/wasi `start`, iocp `start_completion`); that callback
    /// re-enters through the root.
    Sync,
}

static mut SHAPE: Shape = Shape::Shipped;
static mut REENTRY: Reentry = Reentry::Root;

// ---------------------------------------------------------------------------
// Completions and the intrusive queue (queue.rs: next via addr_of_mut!)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Dead,
    Adding,
    Active,
}

enum Operation {
    Noop,
    Timer { next: u64 },
    AsyncWait { wakeup: AtomicBool },
    Write,
    /// An op whose `start` produces its result at once and invokes the callback
    /// synchronously (epoll.rs `start`: `match cb(ud, self_ptr, completion, res)`).
    Immediate,
}

#[derive(Clone, Copy, Debug)]
enum Res {
    Timer,
    AsyncWait,
    Write,
    Immediate,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Action {
    Disarm,
    Rearm,
}

type Callback = fn(*mut c_void, *mut Loop, *mut Completion, Res) -> Action;

fn noop_cb(_: *mut c_void, _: *mut Loop, _: *mut Completion, _: Res) -> Action {
    Action::Disarm
}

struct Completion {
    op: Operation,
    next: *mut Completion,
    state: State,
    result: Option<Res>,
    callback: Callback,
    userdata: *mut c_void,
}

impl Completion {
    fn new() -> Completion {
        Completion {
            op: Operation::Noop,
            next: ptr::null_mut(),
            state: State::Dead,
            result: None,
            callback: noop_cb,
            userdata: ptr::null_mut(),
        }
    }
}

#[derive(Default)]
struct Queue {
    head: *mut Completion,
    tail: *mut Completion,
}

impl Default for Completion {
    fn default() -> Self {
        Completion::new()
    }
}

impl Queue {
    unsafe fn push(&mut self, v: *mut Completion) {
        unsafe {
            ptr::addr_of_mut!((*v).next).write(ptr::null_mut());
            if !self.tail.is_null() {
                ptr::addr_of_mut!((*self.tail).next).write(v);
                self.tail = v;
            } else {
                self.head = v;
                self.tail = v;
            }
        }
    }
    unsafe fn pop(&mut self) -> *mut Completion {
        let next = self.head;
        if next.is_null() {
            return ptr::null_mut();
        }
        unsafe {
            self.head = ptr::addr_of!((*next).next).read();
            if self.head.is_null() {
                self.tail = ptr::null_mut();
            }
            ptr::addr_of_mut!((*next).next).write(ptr::null_mut());
        }
        next
    }
    fn empty(&self) -> bool {
        self.head.is_null()
    }
}

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

#[derive(Default)]
struct LoopFlags {
    init: bool,
    in_run: bool,
    stopped: bool,
}

#[derive(Clone, Copy)]
enum RunMode {
    NoWait,
    Once,
    UntilDone,
}

struct Loop {
    active: usize,
    submissions: Queue,
    completions: Queue,
    asyncs: Queue,
    /// TimerHeap stand-in: due timers in insertion order.
    timers: Queue,
    /// wasi_poll.rs `wakeup: WakeupType` (AtomicBool).
    wakeup: AtomicBool,
    cached_now: u64,
    flags: LoopFlags,
}

impl Loop {
    fn init() -> Loop {
        Loop {
            active: 0,
            submissions: Queue::default(),
            completions: Queue::default(),
            asyncs: Queue::default(),
            timers: Queue::default(),
            wakeup: AtomicBool::new(false),
            cached_now: 0,
            flags: LoopFlags::default(),
        }
    }

    fn stop(&mut self) {
        self.flags.stopped = true;
    }

    fn stopped(&self) -> bool {
        self.flags.stopped
    }

    fn done(&self) -> bool {
        self.flags.stopped || (self.active == 0 && self.submissions.empty())
    }

    unsafe fn add(&mut self, c: *mut Completion) {
        unsafe { (*c).state = State::Adding };
        unsafe { self.submissions.push(c) };
    }

    fn update_now(&mut self) {
        self.cached_now += 1;
    }

    /// wasi_poll.rs `async_notify(&self, c)`.
    unsafe fn async_notify(&self, c: *mut Completion) {
        debug_assert!(matches!(unsafe { &(*c).op }, Operation::AsyncWait { .. }));
        self.wakeup.store(true, Ordering::SeqCst);
        match unsafe { &(*c).op } {
            Operation::AsyncWait { wakeup } => wakeup.store(true, Ordering::SeqCst),
            _ => unreachable!(),
        }
    }

    // ----- the public entry --------------------------------------------------

    fn run(&mut self, mode: RunMode) -> Result<(), ()> {
        if unsafe { SHAPE } == Shape::Shipped {
            // Shipped: the whole run under this protected `&mut self`.
            match mode {
                RunMode::NoWait => self.tick(0),
                RunMode::Once => self.tick(1),
                RunMode::UntilDone => {
                    while !self.done() {
                        self.tick(1)?;
                    }
                    Ok(())
                }
            }
        } else {
            // Fix: thin entry, converts to raw at once and never uses `self` again.
            unsafe { Loop::run_raw(self, mode) }
        }
    }

    // ----- shipped shape (&mut self) ------------------------------------------

    fn tick(&mut self, wait: u32) -> Result<(), ()> {
        if self.flags.stopped {
            return Ok(());
        }
        if self.flags.in_run {
            return Err(());
        }
        self.flags.in_run = true;
        let result = self.tick_inner(wait);
        self.flags.in_run = false;
        result
    }

    fn tick_inner(&mut self, wait: u32) -> Result<(), ()> {
        let self_ptr: *mut Loop = self;
        if !self.flags.init {
            self.flags.init = true;
        }
        let mut wait_rem: usize = wait as usize;

        // Submit (wasi_poll/epoll: start each Adding completion).
        let mut queued = core::mem::take(&mut self.submissions);
        loop {
            let c = unsafe { queued.pop() };
            if c.is_null() {
                break;
            }
            if unsafe { (*c).state } != State::Adding {
                continue;
            }
            unsafe { self.start(c) };
        }

        loop {
            if self.flags.stopped {
                return Ok(());
            }
            self.update_now();
            if !((self.active > 0 && (wait == 0 || wait_rem > 0)) || !self.completions.empty()) {
                break;
            }

            // Run our expired timers
            loop {
                let t = self.timers.head;
                if t.is_null() {
                    break;
                }
                let due = match unsafe { &(*t).op } {
                    Operation::Timer { next } => *next <= self.cached_now,
                    _ => true,
                };
                if !due {
                    break;
                }
                assert!(unsafe { self.timers.pop() } == t);
                let c = t;
                unsafe { (*c).state = State::Dead };
                self.active -= 1;
                wait_rem = wait_rem.saturating_sub(1);
                let cb = unsafe { (*c).callback };
                let ud = unsafe { (*c).userdata };
                let action = cb(ud, self_ptr, c, Res::Timer);
                match action {
                    Action::Disarm => {}
                    Action::Rearm => unsafe { self.start(c) },
                }
            }

            // Run our async waiters (wasi_poll.rs)
            if !self.asyncs.empty() {
                let wakeup = self.wakeup.load(Ordering::SeqCst);
                if wakeup {
                    self.wakeup.store(false, Ordering::SeqCst);
                    let mut asyncs = core::mem::take(&mut self.asyncs);
                    loop {
                        let c = unsafe { asyncs.pop() };
                        if c.is_null() {
                            break;
                        }
                        let c_wakeup = match unsafe { &(*c).op } {
                            Operation::AsyncWait { wakeup } => wakeup.load(Ordering::SeqCst),
                            _ => unreachable!(),
                        };
                        if !c_wakeup {
                            unsafe { self.asyncs.push(c) };
                            continue;
                        }
                        unsafe { (*c).state = State::Dead };
                        self.active -= 1;
                        wait_rem = wait_rem.saturating_sub(1);
                        let cb = unsafe { (*c).callback };
                        let ud = unsafe { (*c).userdata };
                        let action = cb(ud, self_ptr, c, Res::AsyncWait);
                        match action {
                            Action::Disarm => {}
                            Action::Rearm => unsafe { self.start(c) },
                        }
                    }
                }
            }

            // Process the completions we already have completed (kqueue.rs).
            loop {
                let c = unsafe { self.completions.pop() };
                if c.is_null() {
                    break;
                }
                let c_active = unsafe { (*c).state } == State::Active;
                unsafe { (*c).state = State::Dead };
                wait_rem = wait_rem.saturating_sub(1);
                let cb = unsafe { (*c).callback };
                let ud = unsafe { (*c).userdata };
                let res = unsafe { (*c).result.unwrap() };
                let action = cb(ud, self_ptr, c, res);
                match action {
                    Action::Disarm => {
                        if c_active {
                            self.active -= 1;
                        }
                    }
                    Action::Rearm => {
                        if !c_active {
                            unsafe { self.submissions.push(c) };
                        }
                    }
                }
            }

            if wait == 0 {
                break;
            }
        }
        Ok(())
    }

    /// Shipped `start(&mut self, c)` (epoll.rs / wasi_poll.rs): its own
    /// `self_ptr`, and the synchronous callback arm.
    unsafe fn start(&mut self, c: *mut Completion) {
        let self_ptr: *mut Loop = self;
        unsafe {
            match &(*c).op {
                Operation::Timer { .. } => {
                    (*c).state = State::Active;
                    self.active += 1;
                    self.timers.push(c);
                }
                Operation::AsyncWait { .. } => {
                    (*c).state = State::Active;
                    self.active += 1;
                    self.asyncs.push(c);
                }
                Operation::Write => {
                    (*c).result = Some(Res::Write);
                    (*c).state = State::Active;
                    self.active += 1;
                    self.completions.push(c);
                }
                Operation::Immediate => {
                    (*c).state = State::Dead;
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    match cb(ud, self_ptr, c, Res::Immediate) {
                        Action::Disarm => {}
                        Action::Rearm => self.add(c),
                    }
                }
                Operation::Noop => (*c).state = State::Dead,
            }
        }
    }

    // ----- the fix: raw-self (Zig `self: *Loop`) --------------------------------

    unsafe fn run_raw(this: *mut Loop, mode: RunMode) -> Result<(), ()> {
        unsafe {
            match mode {
                RunMode::NoWait => Loop::tick_raw(this, 0),
                RunMode::Once => Loop::tick_raw(this, 1),
                RunMode::UntilDone => {
                    while !(*this).done() {
                        Loop::tick_raw(this, 1)?;
                    }
                    Ok(())
                }
            }
        }
    }

    unsafe fn tick_raw(this: *mut Loop, wait: u32) -> Result<(), ()> {
        unsafe {
            if (*this).flags.stopped {
                return Ok(());
            }
            if (*this).flags.in_run {
                return Err(());
            }
            (*this).flags.in_run = true;
            let result = Loop::tick_inner_raw(this, wait);
            (*this).flags.in_run = false;
            result
        }
    }

    unsafe fn start_dispatch(this: *mut Loop, c: *mut Completion) {
        unsafe {
            if SHAPE == Shape::RawMutStart {
                (*this).start(c)
            } else {
                Loop::start_raw(this, c)
            }
        }
    }

    unsafe fn tick_inner_raw(this: *mut Loop, wait: u32) -> Result<(), ()> {
        unsafe {
            if !(*this).flags.init {
                (*this).flags.init = true;
            }
            let mut wait_rem: usize = wait as usize;

            let mut queued = core::mem::take(&mut (*this).submissions);
            loop {
                let c = queued.pop();
                if c.is_null() {
                    break;
                }
                if (*c).state != State::Adding {
                    continue;
                }
                Loop::start_dispatch(this, c);
            }

            loop {
                if (*this).flags.stopped {
                    return Ok(());
                }
                (*this).update_now();
                if !(((*this).active > 0 && (wait == 0 || wait_rem > 0))
                    || !(*this).completions.empty())
                {
                    break;
                }

                loop {
                    let t = (*this).timers.head;
                    if t.is_null() {
                        break;
                    }
                    let due = match &(*t).op {
                        Operation::Timer { next } => *next <= (*this).cached_now,
                        _ => true,
                    };
                    if !due {
                        break;
                    }
                    assert!((*this).timers.pop() == t);
                    let c = t;
                    (*c).state = State::Dead;
                    (*this).active -= 1;
                    wait_rem = wait_rem.saturating_sub(1);
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    let action = cb(ud, this, c, Res::Timer);
                    match action {
                        Action::Disarm => {}
                        Action::Rearm => Loop::start_dispatch(this, c),
                    }
                }

                if !(*this).asyncs.empty() {
                    let wakeup = (*this).wakeup.load(Ordering::SeqCst);
                    if wakeup {
                        (*this).wakeup.store(false, Ordering::SeqCst);
                        let mut asyncs = core::mem::take(&mut (*this).asyncs);
                        loop {
                            let c = asyncs.pop();
                            if c.is_null() {
                                break;
                            }
                            let c_wakeup = match &(*c).op {
                                Operation::AsyncWait { wakeup } => wakeup.load(Ordering::SeqCst),
                                _ => unreachable!(),
                            };
                            if !c_wakeup {
                                (*this).asyncs.push(c);
                                continue;
                            }
                            (*c).state = State::Dead;
                            (*this).active -= 1;
                            wait_rem = wait_rem.saturating_sub(1);
                            let cb = (*c).callback;
                            let ud = (*c).userdata;
                            let action = cb(ud, this, c, Res::AsyncWait);
                            match action {
                                Action::Disarm => {}
                                Action::Rearm => Loop::start_dispatch(this, c),
                            }
                        }
                    }
                }

                loop {
                    let c = (*this).completions.pop();
                    if c.is_null() {
                        break;
                    }
                    let c_active = (*c).state == State::Active;
                    (*c).state = State::Dead;
                    wait_rem = wait_rem.saturating_sub(1);
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    let res = (*c).result.unwrap();
                    let action = cb(ud, this, c, res);
                    match action {
                        Action::Disarm => {
                            if c_active {
                                (*this).active -= 1;
                            }
                        }
                        Action::Rearm => {
                            if !c_active {
                                (*this).submissions.push(c);
                            }
                        }
                    }
                }

                if wait == 0 {
                    break;
                }
            }
            Ok(())
        }
    }

    unsafe fn start_raw(this: *mut Loop, c: *mut Completion) {
        unsafe {
            match &(*c).op {
                Operation::Timer { .. } => {
                    (*c).state = State::Active;
                    (*this).active += 1;
                    (*this).timers.push(c);
                }
                Operation::AsyncWait { .. } => {
                    (*c).state = State::Active;
                    (*this).active += 1;
                    (*this).asyncs.push(c);
                }
                Operation::Write => {
                    (*c).result = Some(Res::Write);
                    (*c).state = State::Active;
                    (*this).active += 1;
                    (*this).completions.push(c);
                }
                Operation::Immediate => {
                    (*c).state = State::Dead;
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    match cb(ud, this, c, Res::Immediate) {
                        Action::Disarm => {}
                        Action::Rearm => (*this).add(c),
                    }
                }
                Operation::Noop => (*c).state = State::Dead,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// WASI Async watcher (wasi_watcher.rs)
// ---------------------------------------------------------------------------

struct AsyncWaiter {
    loop_: *mut Loop,
    c: *mut Completion,
}

struct Async {
    wakeup: bool,
    waiter: Option<AsyncWaiter>,
}

impl Async {
    unsafe fn wait(&mut self, l: *mut Loop, c: *mut Completion, ud: *mut c_void, cb: Callback) {
        unsafe {
            *c = Completion {
                op: Operation::AsyncWait { wakeup: AtomicBool::new(false) },
                next: ptr::null_mut(),
                state: State::Dead,
                result: None,
                callback: cb,
                userdata: ud,
            };
            if self.wakeup {
                (*l).async_notify(c);
            }
            (*l).add(c);
            self.waiter = Some(AsyncWaiter { loop_: l, c });
        }
    }

    fn notify(&mut self) {
        match &self.waiter {
            Some(w) => unsafe { (*w.loop_).async_notify(w.c) },
            None => self.wakeup = true,
        }
    }
}

// ---------------------------------------------------------------------------
// termio shapes: ThreadData (cb.data) with the stored loop root
// ---------------------------------------------------------------------------

struct ThreadData {
    /// termio_impl.rs `loop_: &raw mut (*thread).loop_` / exec tests `loop_ptr`.
    loop_: *mut Loop,
    write_c: *mut Completion,
    async_: *mut Async,
    wrote: bool,
    async_fired: bool,
    immediate_fired: bool,
}

/// termio Thread (thread.rs): the loop is an inline field.
struct Thread {
    loop_: Loop,
    timer_c: Completion,
    write_c: Completion,
    async_c: Completion,
    stop: Async,
    td: ThreadData,
}

/// The "mailbox wakeup" callback: queues a write the way exec.rs:1644/1766 ->
/// stream.rs:461 does (`(*loop_).add`), or notifies (WASI), or rearms into a
/// synchronously-completing op.
fn timer_cb(ud: *mut c_void, l: *mut Loop, c: *mut Completion, _r: Res) -> Action {
    let td = ud as *mut ThreadData;
    unsafe {
        match REENTRY {
            Reentry::Root => {
                let loop_: *mut Loop = (*td).loop_;
                (*loop_).add((*td).write_c);
                Action::Disarm
            }
            Reentry::L => {
                let lr: &mut Loop = &mut *l;
                lr.add((*td).write_c);
                Action::Disarm
            }
            Reentry::Wasi => {
                (*(*td).async_).notify();
                Action::Disarm
            }
            Reentry::Sync => {
                (*c).op = Operation::Immediate;
                (*c).callback = immediate_cb;
                Action::Rearm
            }
        }
    }
}

fn immediate_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, _r: Res) -> Action {
    let td = ud as *mut ThreadData;
    unsafe {
        (*td).immediate_fired = true;
        let loop_: *mut Loop = (*td).loop_;
        (*loop_).add((*td).write_c);
    }
    Action::Disarm
}

/// The write callback; then stop (thread.rs `(*self_).loop_.stop()` through the root).
fn write_cb(ud: *mut c_void, l: *mut Loop, _c: *mut Completion, _r: Res) -> Action {
    let td = ud as *mut ThreadData;
    unsafe {
        (*td).wrote = true;
        match REENTRY {
            Reentry::L => (*l).stop(),
            _ => (*(*td).loop_).stop(),
        }
    }
    Action::Disarm
}

fn async_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, _r: Res) -> Action {
    let td = ud as *mut ThreadData;
    unsafe { (*td).async_fired = true };
    Action::Disarm
}

unsafe fn arm(root: *mut Loop, td: *mut ThreadData, timer_c: *mut Completion, write_c: *mut Completion, async_c: *mut Completion) {
    unsafe {
        *timer_c = Completion {
            op: Operation::Timer { next: 0 },
            next: ptr::null_mut(),
            state: State::Dead,
            result: None,
            callback: timer_cb,
            userdata: td.cast(),
        };
        *write_c = Completion {
            op: Operation::Write,
            next: ptr::null_mut(),
            state: State::Dead,
            result: None,
            callback: write_cb,
            userdata: td.cast(),
        };
        (*root).add(timer_c);
        if REENTRY == Reentry::Wasi {
            // mailbox.wakeup.wait(&self.loop, ..) before run, with the stable root.
            (*(*td).async_).wait(root, async_c, td.cast(), async_cb);
        }
    }
}

unsafe fn check(td: *const ThreadData, l: *const Loop) {
    unsafe {
        match REENTRY {
            Reentry::Wasi => {
                assert!((*td).async_fired);
                assert_eq!((*l).active, 0);
            }
            Reentry::Sync => {
                assert!((*td).immediate_fired);
                assert!((*td).wrote);
                assert!((*l).stopped());
            }
            _ => {
                assert!((*td).wrote);
                assert!((*l).stopped());
            }
        }
    }
}

fn run_thread(entry: Entry) {
    let self_: *mut Thread = Box::into_raw(Box::new(Thread {
        loop_: Loop::init(),
        timer_c: Completion::new(),
        write_c: Completion::new(),
        async_c: Completion::new(),
        stop: Async { wakeup: false, waiter: None },
        td: ThreadData {
            loop_: ptr::null_mut(),
            write_c: ptr::null_mut(),
            async_: ptr::null_mut(),
            wrote: false,
            async_fired: false,
            immediate_fired: false,
        },
    }));
    unsafe {
        let root: *mut Loop = &raw mut (*self_).loop_;
        let td: *mut ThreadData = &raw mut (*self_).td;
        (*td).loop_ = root;
        (*td).write_c = &raw mut (*self_).write_c;
        (*td).async_ = &raw mut (*self_).stop;
        arm(root, td, &raw mut (*self_).timer_c, &raw mut (*self_).write_c, &raw mut (*self_).async_c);
        let r = match entry {
            Entry::ThreadMethod => (*self_).loop_.run(RunMode::UntilDone),
            Entry::ThreadRaw => Loop::run_raw(&raw mut (*self_).loop_, RunMode::UntilDone),
            _ => unreachable!(),
        };
        r.unwrap();
        check(td, root);
        drop(Box::from_raw(self_));
    }
}

fn run_local(entry: Entry) {
    let mut loop_ = Loop::init();
    let loop_ptr: *mut Loop = ptr::addr_of_mut!(loop_);
    let timer_c: *mut Completion = Box::into_raw(Box::new(Completion::new()));
    let write_c: *mut Completion = Box::into_raw(Box::new(Completion::new()));
    let async_c: *mut Completion = Box::into_raw(Box::new(Completion::new()));
    let a: *mut Async = Box::into_raw(Box::new(Async { wakeup: false, waiter: None }));
    let mut td = ThreadData {
        loop_: loop_ptr,
        write_c,
        async_: a,
        wrote: false,
        async_fired: false,
        immediate_fired: false,
    };
    let tdp: *mut ThreadData = &raw mut td;
    unsafe {
        arm(loop_ptr, tdp, timer_c, write_c, async_c);
        // bounded pump, as the exec.rs tests do with NoWait
        for _ in 0..8 {
            let r = match entry {
                Entry::LocalMethod => loop_.run(RunMode::NoWait),
                Entry::LocalRaw => Loop::run_raw(loop_ptr, RunMode::NoWait),
                _ => unreachable!(),
            };
            r.unwrap();
            if (*loop_ptr).done() {
                break;
            }
        }
        check(tdp, loop_ptr);
        drop(Box::from_raw(timer_c));
        drop(Box::from_raw(write_c));
        drop(Box::from_raw(async_c));
        drop(Box::from_raw(a));
    }
    let _ = &mut loop_;
}

fn parse(name: &str) -> (Shape, Entry, Reentry) {
    let parts: Vec<&str> = name.split('_').collect();
    assert_eq!(parts.len(), 3, "candidate <shape>_<entry>_<reentry>");
    let (shape, method) = match parts[0] {
        "shipped" => (Shape::Shipped, true),
        "raw" => (Shape::Raw, false),
        "wrapper" => (Shape::Raw, true),
        "rawmutstart" => (Shape::RawMutStart, false),
        s => panic!("shape {s}"),
    };
    let entry = match (parts[1], method) {
        ("thread", true) => Entry::ThreadMethod,
        ("thread", false) => Entry::ThreadRaw,
        ("local", true) => Entry::LocalMethod,
        ("local", false) => Entry::LocalRaw,
        (e, _) => panic!("entry {e}"),
    };
    let re = match parts[2] {
        "root" => Reentry::Root,
        "l" => Reentry::L,
        "wasi" => Reentry::Wasi,
        "sync" => Reentry::Sync,
        r => panic!("reentry {r}"),
    };
    (shape, entry, re)
}

fn run_candidate(name: &str) {
    let (shape, entry, re) = parse(name);
    unsafe {
        SHAPE = shape;
        REENTRY = re;
    }
    match entry {
        Entry::ThreadMethod | Entry::ThreadRaw => run_thread(entry),
        Entry::LocalMethod | Entry::LocalRaw => run_local(entry),
    }
}

fn main() {
    let name = std::env::args().nth(1).expect("candidate");
    run_candidate(&name);
    println!("{name}: clean");
}

#[cfg(test)]
mod tests {
    use super::run_candidate;

    /// The applied shape: every re-entry kind, both entries.
    #[test]
    fn applied_shape_is_clean() {
        for c in [
            "raw_thread_root",
            "raw_thread_l",
            "raw_thread_wasi",
            "raw_thread_sync",
            "raw_local_root",
            "raw_local_l",
            "raw_local_wasi",
            "raw_local_sync",
            "wrapper_thread_l",
            "wrapper_local_l",
        ] {
            run_candidate(c);
        }
    }
}
