//! Thread pool copied almost directly from Zap[1]. In @kprotty's own words:
//! lock-free, allocation-free* (excluding spawning threads), supports batch
//! scheduling, and dynamically spawns threads while handling thread spawn
//! failure. I highly recommend reading @kprotty's incredible blog post[2] on
//! this topic.
//!
//! The original file in Zap is licensed under the MIT license, and the
//! license and copyright is reproduced below. The libxev project is also
//! MIT licensed so the entire project (including this file) are equally
//! licensed. This is just a convenience note for any OSS users, contributors,
//! etc.
//!
//! MIT License
//!
//! Copyright (c) 2021 kprotty
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//!
//! [1]: https://github.com/kprotty/zap
//! [2]: https://zig.news/kprotty/resource-efficient-thread-pools-with-zig-3291
//
// PORT NOTE (Task 1, `.superpowers/sdd/task-1-brief.md`): this is the FULL faithful
// R1 port of libxev `ThreadPool.zig` (kprotty's Zap work-stealing pool) — no longer
// the C3 type-surface stub. The two lock-free intrusive containers (`Node::Queue`
// swap-based MPSC + `Node::Buffer` work-stealing ring) are SB-class raw-pointer
// code; they were proven Stacked-Borrows AND Tree-Borrows clean on the isolated
// non-FFI harness `.port/phase-2/mirihar-threadpool/` (mirihar doctrine,
// `.port/PORT_RULES.md:209`; memory `shaper-hook-sb-clean-interleave`) BEFORE
// shipping here. The module is compiled on macOS (its original home) AND Linux
// (the epoll File watcher routes blocking pread/pwrite/close through this pool).
// It currently has ZERO callers on Linux — library-completeness parity work.
//
// PORT NOTE (interior mutability / receivers): Zig passes `*ThreadPool` /
// `*Queue` / `*Buffer` (shared mutable pointers) to worker threads that run
// concurrently with the owner. The sound Rust translation is `&self` methods over
// interior-mutable state (atomics + one `UnsafeCell` for the queue cache): a
// concurrently-running worker holding a raw `*mut ThreadPool` would make any
// `&mut self` uniqueness claim UB.
// `schedule`/`notify`/`wait`/`register`/`join`/`shutdown`/`deinit` therefore take
// `&self`. `shutdown` is `&self` rather than the brief's nominal `&mut` for exactly
// this reason (it is designed to be called while workers run); a `&mut` receiver
// would be unsound. No caller is affected — a caller holding `&mut ThreadPool` can
// still call a `&self` method.
//
// UPDATE (WS-XEV-UB): `deinit` USED to take `&mut self` on the theory that
// "join() means every worker has exited"; Miri proved that false and it was real
// Stacked-Borrows UB — see the PORT NOTE on `deinit`. It is now `&self`.
// `unregister` goes further and takes a RAW `*const ThreadPool`, because its frame
// deliberately outlives the pool — see the PORT NOTE on `unregister`. Both are
// instances of the same rule: a reference argument is strongly protected for its
// whole call frame, so any pool method whose frame can overlap another thread's
// use of the pool (or the pool's own invalidation) must not take one.

#![allow(dead_code)]

use core::cell::UnsafeCell;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize};

// ThreadPool = @This();
//
// stack_size: u32,
// max_threads: u32,
// sync: Atomic(u32) = ...,
// idle_event: Event = .{},
// join_event: Event = .{},
// run_queue: Node.Queue = .{},
// threads: Atomic(?*Thread) = ...,
pub struct ThreadPool {
    stack_size: u32,
    max_threads: u32,
    sync: AtomicU32,
    idle_event: Event,
    join_event: Event,
    run_queue: NodeQueue,
    threads: AtomicPtr<Thread>,
}

// PORT NOTE: worker threads receive `*mut ThreadPool` via `std::thread::spawn`
// (the faithful port of Zig's `std.Thread.spawn(..., .{self})` + `thread.detach()`).
// A raw pointer is not `Send`, so it rides in this newtype; the pool is guaranteed
// to outlive every worker because `deinit`→`join` waits for all of them to exit.
struct PoolRef(*mut ThreadPool);
unsafe impl Send for PoolRef {}

/// Tracks the number of threads not searching for Tasks / spawned, plus the
/// pool state. Zig `Sync` is a `packed struct` bit-cast to/from an `Atomic(u32)`;
/// the packed layout is LSB-first: idle:u14 | spawned:u14 | unused:1 | notified:1
/// | state:u2.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Sync {
    /// Tracks the number of threads not searching for Tasks
    idle: u32,
    /// Tracks the number of threads spawned
    spawned: u32,
    /// What you see is what you get
    unused: bool,
    /// Used to not miss notifications while state = waking
    notified: bool,
    /// The current state of the thread pool
    state: State,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum State {
    /// A notification can be issued to wake up a sleeping as the "waking thread".
    #[default]
    Pending = 0,
    /// The state was notifiied with a signal. A thread is woken up.
    /// The first thread to transition to `waking` becomes the "waking thread".
    Signaled = 1,
    /// There is a "waking thread" among us.
    /// No other thread should be woken up until the waking thread transitions the state.
    Waking = 2,
    /// The thread pool was terminated. Start decremented `spawned` so that it can be joined.
    Shutdown = 3,
}

impl Sync {
    // @bitCast between the packed struct and u32 (LSB-first field layout).
    const IDLE_MASK: u32 = (1 << 14) - 1;
    const SPAWNED_MASK: u32 = (1 << 14) - 1;

    #[inline]
    fn from_u32(v: u32) -> Sync {
        Sync {
            idle: v & Self::IDLE_MASK,
            spawned: (v >> 14) & Self::SPAWNED_MASK,
            unused: (v >> 28) & 1 != 0,
            notified: (v >> 29) & 1 != 0,
            state: match (v >> 30) & 0b11 {
                0 => State::Pending,
                1 => State::Signaled,
                2 => State::Waking,
                _ => State::Shutdown,
            },
        }
    }

    #[inline]
    fn to_u32(self) -> u32 {
        (self.idle & Self::IDLE_MASK)
            | ((self.spawned & Self::SPAWNED_MASK) << 14)
            | ((self.unused as u32) << 28)
            | ((self.notified as u32) << 29)
            | ((self.state as u32) << 30)
    }
}

/// Configuration options for the thread pool.
/// TODO: add CPU core affinity?
pub struct Config {
    pub stack_size: u32,
    pub max_threads: u32,
}

impl Default for Config {
    fn default() -> Self {
        // PORT NOTE: Zig `stack_size = (std.Thread.SpawnConfig{}).stack_size`
        // (16 MiB default) and `max_threads = 0`. A derived `Default` of 0 for
        // `stack_size` would make `init`'s `@max(1, stack_size)` produce a 1-byte
        // stack, so the reference's default is reproduced as the literal here.
        Config {
            stack_size: 16 * 1024 * 1024,
            max_threads: 0,
        }
    }
}

impl ThreadPool {
    /// Statically initialize the thread pool using the configuration.
    pub fn init(config: Config) -> ThreadPool {
        ThreadPool {
            stack_size: core::cmp::max(1, config.stack_size),
            max_threads: if config.max_threads > 0 {
                config.max_threads
            } else {
                std::thread::available_parallelism()
                    .map(|n| n.get() as u32)
                    .unwrap_or(1)
            },
            sync: AtomicU32::new(Sync::default().to_u32()),
            idle_event: Event::default(),
            join_event: Event::default(),
            run_queue: NodeQueue::default(),
            threads: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// Wait for a thread to call shutdown() on the thread pool and kill the worker threads.
    //
    // PORT NOTE (`&self`, WS-XEV-UB — same reason as `shutdown` above, see the
    // module header): this was `&mut self`, and that was genuine Stacked-Borrows
    // UB, caught by Miri on `multi_worker_quiesce_then_join_50x`:
    //
    //   error: Undefined Behavior: not granting access to tag <N> because that
    //   would remove [SharedReadOnly for <M>] which is strongly protected
    //     --> thread_pool.rs:199  pub fn deinit(&mut self)
    //   help: <M> is this argument --> fn unregister(&self, ...)
    //
    // `deinit`'s whole job is to `join()` — i.e. it is BY CONSTRUCTION called
    // while worker threads are still running and still touching the pool (the
    // last worker's `unregister` is what wakes the joiner). A `&mut self`
    // retag at `deinit`'s frame entry claims unique access to the pool at
    // exactly the moment that is false: it invalidates every worker's
    // `*mut ThreadPool` alias, and it cannot even be performed while a worker
    // sits in a `&self` frame (which is strongly protected). `&self` +
    // interior mutability is the only sound receiver, exactly as the module
    // header argues for `shutdown`. Zig's `deinit(self: *ThreadPool)` is an
    // unprotected shared pointer, so the oracle has no equivalent constraint.
    // Behaviourally identical (the body is just `join()`; the oracle's
    // `self.* = undefined` has no Rust counterpart — the pool is dropped by its
    // owner), and source-compatible for every caller, which holds the pool by
    // value or by `&mut`.
    pub fn deinit(&self) {
        self.join();
        // self.* = undefined; — Rust has no `undefined`; the pool is dropped by
        // its owner. `join()` guarantees all workers have exited by this point.
    }

    /// Schedule a batch of tasks to be executed by some thread on the thread pool.
    pub fn schedule(&self, batch: Batch) {
        // Sanity check
        if batch.len == 0 {
            return;
        }

        // Extract out the Node's from the Tasks
        let mut list = NodeList {
            head: unsafe { core::ptr::addr_of_mut!((*batch.head.unwrap()).node) },
            tail: unsafe { core::ptr::addr_of_mut!((*batch.tail.unwrap()).node) },
        };

        // Push the task Nodes to the most approriate queue
        let thread = Thread::current();
        if !thread.is_null() {
            // thread.run_buffer.push(&list) catch thread.run_queue.push(list)
            unsafe {
                if (*thread).run_buffer.push(&mut list).is_err() {
                    (*thread).run_queue.push(list);
                }
            }
        } else {
            self.run_queue.push(list);
        }

        // Try to notify a thread
        let is_waking = false;
        self.notify(is_waking);
    }

    #[inline]
    fn notify(&self, is_waking: bool) {
        // Fast path to check the Sync state to avoid calling into notifySlow().
        // If we're waking, then we need to update the state regardless
        if !is_waking {
            let sync = Sync::from_u32(self.sync.load(Relaxed));
            if sync.notified {
                return;
            }
        }

        self.notify_slow(is_waking);
    }

    fn notify_slow(&self, is_waking: bool) {
        let mut sync = Sync::from_u32(self.sync.load(Relaxed));
        while sync.state != State::Shutdown {
            let can_wake = is_waking || (sync.state == State::Pending);
            if is_waking {
                debug_assert!(sync.state == State::Waking);
            }

            let mut new_sync = sync;
            new_sync.notified = true;
            if can_wake && sync.idle > 0 {
                // wake up an idle thread
                new_sync.state = State::Signaled;
            } else if can_wake && sync.spawned < self.max_threads {
                // spawn a new thread
                new_sync.state = State::Signaled;
                new_sync.spawned += 1;
            } else if is_waking {
                // no other thread to pass on "waking" status
                new_sync.state = State::Pending;
            } else if sync.notified {
                // nothing to update
                return;
            }

            // Release barrier synchronizes with Acquire in wait()
            // to ensure pushes to run queues happen before observing a posted notification.
            match self.sync.compare_exchange_weak(
                sync.to_u32(),
                new_sync.to_u32(),
                Release,
                Relaxed,
            ) {
                Ok(_) => {
                    // We signaled to notify an idle thread
                    if can_wake && sync.idle > 0 {
                        self.idle_event.notify();
                        return;
                    }

                    // We signaled to spawn a new thread
                    if can_wake && sync.spawned < self.max_threads {
                        let pool = PoolRef(self as *const ThreadPool as *mut ThreadPool);
                        let builder =
                            std::thread::Builder::new().stack_size(self.stack_size as usize);
                        // `let pool = pool` forces the closure to capture the
                        // whole `Send` `PoolRef`, not the `!Send` raw field.
                        match builder.spawn(move || {
                            let pool = pool;
                            unsafe { Thread::run(pool.0) }
                        }) {
                            // thread.detach()
                            Ok(handle) => {
                                drop(handle);
                            }
                            Err(_) => unsafe { Self::unregister(self as *const ThreadPool, None) },
                        }
                        return;
                    }

                    return;
                }
                Err(e) => sync = Sync::from_u32(e),
            }
        }
    }

    fn wait(&self, is_waking_in: bool) -> Result<bool, Shutdown> {
        let mut is_idle = false;
        let mut is_waking = is_waking_in;
        let mut sync = Sync::from_u32(self.sync.load(Relaxed));

        loop {
            if sync.state == State::Shutdown {
                return Err(Shutdown);
            }
            if is_waking {
                debug_assert!(sync.state == State::Waking);
            }

            // Consume a notification made by notify().
            if sync.notified {
                let mut new_sync = sync;
                new_sync.notified = false;
                if is_idle {
                    new_sync.idle -= 1;
                }
                if sync.state == State::Signaled {
                    new_sync.state = State::Waking;
                }

                // Acquire barrier synchronizes with notify()
                // to ensure that pushes to run queue are observed after wait() returns.
                match self.sync.compare_exchange_weak(
                    sync.to_u32(),
                    new_sync.to_u32(),
                    Acquire,
                    Relaxed,
                ) {
                    Ok(_) => return Ok(is_waking || (sync.state == State::Signaled)),
                    Err(e) => sync = Sync::from_u32(e),
                }

            // No notification to consume.
            // Mark this thread as idle before sleeping on the idle_event.
            } else if !is_idle {
                let mut new_sync = sync;
                new_sync.idle += 1;
                if is_waking {
                    new_sync.state = State::Pending;
                }

                match self.sync.compare_exchange_weak(
                    sync.to_u32(),
                    new_sync.to_u32(),
                    Relaxed,
                    Relaxed,
                ) {
                    Ok(_) => {
                        is_waking = false;
                        is_idle = true;
                    }
                    Err(e) => sync = Sync::from_u32(e),
                }

            // Wait for a signal by either notify() or shutdown() without wasting cpu cycles.
            // TODO: Add I/O polling here.
            } else {
                self.idle_event.wait();
                sync = Sync::from_u32(self.sync.load(Relaxed));
            }
        }
    }

    /// Marks the thread pool as shutdown
    pub fn shutdown(&self) {
        let mut sync = Sync::from_u32(self.sync.load(Relaxed));
        while sync.state != State::Shutdown {
            let mut new_sync = sync;
            new_sync.notified = true;
            new_sync.state = State::Shutdown;
            new_sync.idle = 0;

            // Full barrier to synchronize with both wait() and notify()
            match self
                .sync
                .compare_exchange_weak(sync.to_u32(), new_sync.to_u32(), AcqRel, Relaxed)
            {
                Ok(_) => {
                    // Wake up any threads sleeping on the idle_event.
                    // TODO: I/O polling notification here.
                    if sync.idle > 0 {
                        self.idle_event.shutdown();
                    }
                    if sync.spawned == 0 {
                        self.join_event.notify();
                    }
                    return;
                }
                Err(e) => sync = Sync::from_u32(e),
            }
        }
    }

    fn register(&self, thread: *mut Thread) {
        // Push the thread onto the threads stack in a lock-free manner.
        let mut threads = self.threads.load(Relaxed);
        loop {
            unsafe { (*thread).next = opt_ptr(threads) };
            match self
                .threads
                .compare_exchange_weak(threads, thread, Release, Relaxed)
            {
                Ok(_) => break,
                Err(e) => threads = e,
            }
        }
    }

    // PORT NOTE (raw `pool` receiver, WS-XEV-UB): the oracle takes
    // `self: *ThreadPool` and states the constraint explicitly a few lines down —
    // "We have to do that without touching the thread pool itself since its
    // memory is invalidated by now". A Rust `&self` receiver CANNOT express that:
    // a reference argument is STRONGLY PROTECTED for the WHOLE call frame, and
    // this frame deliberately outlives the pool — after `join_event.notify()`
    // wakes the joiner, the thread parks in `(*thread).join_event.wait()` for the
    // shutdown chain while the joiner is free to invalidate/free the pool. A live
    // protected `&ThreadPool` over that window is UB (Miri: "not granting access
    // ... would remove [SharedReadOnly] which is strongly protected", raised
    // against `deinit`; and a deallocation would trip the protector too), even
    // though the code never *touches* the pool there. So the pool travels as a
    // raw `*const ThreadPool` — no protector, no aliasing claim — and every pool
    // access below is a NARROW autoref of one field (`sync`, `join_event`) whose
    // protector lives only for that single atomic op / call, all of which
    // strictly precede the invalidation point. `maybe_thread`/`thread` stay raw
    // for the same reason (memory `rust-mut-args-strongly-protected`).
    //
    /// # Safety
    /// `pool` must point to a live `ThreadPool` at entry and remain live until
    /// the `join_event.notify()` below; after that point the pool may be
    /// invalidated by the joiner and is never touched again. `maybe_thread`, if
    /// `Some`, must point to this worker's live `Thread`.
    unsafe fn unregister(pool: *const ThreadPool, maybe_thread: Option<*mut Thread>) {
        // Un-spawn one thread, either due to a failed OS thread spawning or the thread is exitting.
        let one_spawned: u32 = Sync {
            spawned: 1,
            ..Sync::default()
        }
        .to_u32();
        let sync = Sync::from_u32(unsafe { (*pool).sync.fetch_sub(one_spawned, Release) });
        debug_assert!(sync.spawned > 0);

        // The last thread to exit must wake up the thread pool join()er
        // who will start the chain to shutdown all the threads.
        if sync.state == State::Shutdown && sync.spawned == 1 {
            unsafe { (*pool).join_event.notify() };
        }
        // ---- the pool's memory may be invalidated from HERE on (oracle note
        // below); nothing past this line may name it. ----

        // If this is a thread pool thread, wait for a shutdown signal by the thread pool join()er.
        let thread = match maybe_thread {
            Some(t) => t,
            None => return,
        };
        unsafe { (*thread).join_event.wait() };

        // After receiving the shutdown signal, shutdown the next thread in the pool.
        // We have to do that without touching the thread pool itself since it's memory is invalidated by now.
        // So just follow our .next link.
        let next_thread = match unsafe { (*thread).next } {
            Some(t) => t,
            None => return,
        };
        unsafe { (*next_thread).join_event.notify() };
    }

    fn join(&self) {
        // Wait for the thread pool to be shutdown() then for all threads to enter a joinable state
        self.join_event.wait();
        // DELIBERATE DEVIATION (WS-XEV-UB): the oracle loads `.monotonic`
        // (Relaxed) here — the value is only sanity-checked. It is ACQUIRE in
        // this port, which is the ONLY happens-before edge that makes the
        // shutdown chain below data-race-free, as Miri proved:
        //
        //   error: Undefined Behavior: Data race detected between (1) non-atomic
        //   read on thread `unnamed-5` [`(*target).next` in `Thread::pop`'s
        //   work-stealing traversal, i.e. one worker reading ANOTHER worker's
        //   stack-allocated `Thread`] and (2) deallocation on thread `unnamed-7`
        //   [that other worker's frame dying at the end of `Thread::run`]
        //
        // The chain is: every exiting worker publishes its accesses with the
        // `Release` RMW in `unregister` (`sync.fetch_sub`); the LAST one then
        // `join_event.notify()`s the joiner; the joiner walks `self.threads` and
        // notifies the head worker, which notifies the next, and each notified
        // worker then RETURNS — destroying the very `Thread` its peers were
        // reading during work stealing. Without an acquire on `sync` the joiner
        // inherits only the LAST worker's release, so the peers' earlier steal
        // reads are unordered w.r.t. the frame teardowns the joiner sets in
        // motion: a formal data race (benign on real hardware, real UB in the
        // model). All writes to `sync` are RMWs, so one acquire load here
        // synchronizes with EVERY worker's release via the release sequence, and
        // that edge propagates down the notify chain. Strictly more ordering than
        // the oracle: no behaviour, state or API changes, and it cannot introduce
        // a hang (it is a load, not a CAS).
        let sync = Sync::from_u32(self.sync.load(Acquire));

        debug_assert!(sync.state == State::Shutdown);
        debug_assert!(sync.spawned == 0);

        // If there are threads, start off the chain sending it the shutdown signal.
        // The thread receives the shutdown signal and sends it to the next thread, and the next..
        let thread = match opt_ptr(self.threads.load(Acquire)) {
            Some(t) => t,
            None => return,
        };
        unsafe { (*thread).join_event.notify() };
    }
}

/// error{Shutdown}
struct Shutdown;

/// A Task represents the unit of Work / Job / Execution that the ThreadPool schedules.
/// The user provides a `callback` which is invoked when the *Task can run on a thread.
pub struct Task {
    pub node: Node,
    pub callback: fn(*mut Task),
}

// PORT NOTE: Rust glue — `Completion`'s `task: ThreadPool.Task` field is Zig
// `undefined` (set only when scheduled on the pool). Rust has no `undefined`, so
// `Completion::default` needs a `Task::default`: a null-ish node + a noop callback.
fn task_noop(_: *mut Task) {}

impl Default for Task {
    fn default() -> Self {
        Task {
            node: Node::default(),
            callback: task_noop,
        }
    }
}

/// An unordered collection of Tasks which can be submitted for scheduling as a group.
#[derive(Default)]
pub struct Batch {
    pub len: usize,
    pub head: Option<*mut Task>,
    pub tail: Option<*mut Task>,
}

impl Batch {
    /// Create a batch from a single task.
    pub fn from(task: *mut Task) -> Batch {
        Batch {
            len: 1,
            head: Some(task),
            tail: Some(task),
        }
    }

    /// Another batch into this one, taking ownership of its tasks.
    pub fn push(&mut self, batch: Batch) {
        if batch.len == 0 {
            return;
        }
        if self.len == 0 {
            *self = batch;
        } else {
            unsafe {
                (*self.tail.unwrap()).node.next =
                    batch.head.map(|h| core::ptr::addr_of_mut!((*h).node));
            }
            self.tail = batch.tail;
            self.len += batch.len;
        }
    }
}

#[derive(Default)]
struct Thread {
    next: Option<*mut Thread>,
    target: Option<*mut Thread>,
    join_event: Event,
    run_queue: NodeQueue,
    run_buffer: NodeBuffer,
}

thread_local! {
    // threadlocal var current: ?*Thread = null;
    static CURRENT: core::cell::Cell<*mut Thread> = const { core::cell::Cell::new(core::ptr::null_mut()) };
}

impl Thread {
    #[inline]
    fn current() -> *mut Thread {
        CURRENT.with(|c| c.get())
    }

    /// Thread entry point which runs a worker for the ThreadPool
    ///
    /// # Safety
    /// `thread_pool` must point to a live `ThreadPool` that outlives this worker
    /// (guaranteed by `deinit`→`join`).
    unsafe fn run(thread_pool: *mut ThreadPool) {
        let mut self_storage = Thread::default();
        let self_ptr: *mut Thread = &mut self_storage;
        CURRENT.with(|c| c.set(self_ptr));

        unsafe { (*thread_pool).register(self_ptr) };
        // defer thread_pool.unregister(&self);

        let mut is_waking = false;
        // The explicit `loop { match wait {...Err => break} }` mirrors the Zig
        // `is_waking = thread_pool.wait(is_waking) catch return;` (+ the deferred
        // unregister below) more faithfully than a `while let Ok(..)` collapse.
        #[allow(clippy::while_let_loop)]
        loop {
            // is_waking = thread_pool.wait(is_waking) catch return;
            match unsafe { (*thread_pool).wait(is_waking) } {
                Ok(w) => is_waking = w,
                Err(Shutdown) => break,
            }

            while let Some(result) = unsafe { Thread::pop(self_ptr, thread_pool) } {
                if result.pushed || is_waking {
                    unsafe { (*thread_pool).notify(is_waking) };
                }
                is_waking = false;

                // const task: *Task = @fieldParentPtr("node", result.node);
                let task = unsafe {
                    (result.node as *mut u8).sub(core::mem::offset_of!(Task, node)) as *mut Task
                };
                let cb = unsafe { (*task).callback };
                cb(task);
            }
        }

        // the deferred unregister
        unsafe { ThreadPool::unregister(thread_pool, Some(self_ptr)) };
    }

    /// Try to dequeue a Node/Task from the ThreadPool.
    /// Spurious reports of dequeue() returning empty are allowed.
    ///
    /// # Safety
    /// `self_` and `thread_pool` must be valid pointers to a live `Thread` /
    /// `ThreadPool`.
    unsafe fn pop(self_: *mut Thread, thread_pool: *mut ThreadPool) -> Option<Stole> {
        // Check our local buffer first
        if let Some(node) = unsafe { (*self_).run_buffer.pop() } {
            return Some(Stole {
                node,
                pushed: false,
            });
        }

        // Then check our local queue
        if let Some(stole) = unsafe { (*self_).run_buffer.consume(&(*self_).run_queue) } {
            return Some(stole);
        }

        // Then the global queue
        if let Some(stole) = unsafe { (*self_).run_buffer.consume(&(*thread_pool).run_queue) } {
            return Some(stole);
        }

        // TODO: add optimistic I/O polling here

        // Then try work stealing from other threads
        let mut num_threads: u32 =
            Sync::from_u32(unsafe { (*thread_pool).sync.load(Relaxed) }).spawned;
        while num_threads > 0 {
            // Traverse the stack of registered threads on the thread pool
            let target = match unsafe { (*self_).target } {
                Some(t) => t,
                None => match opt_ptr(unsafe { (*thread_pool).threads.load(Acquire) }) {
                    Some(t) => t,
                    None => unreachable!(),
                },
            };
            unsafe { (*self_).target = (*target).next };

            // Try to steal from their queue first to avoid contention (the target steal's from queue last).
            if let Some(stole) = unsafe { (*self_).run_buffer.consume(&(*target).run_queue) } {
                return Some(stole);
            }

            // Skip stealing from the buffer if we're the target.
            // We still steal from our own queue above given it may have just been locked the first time we tried.
            if target == self_ {
                num_threads -= 1;
                continue;
            }

            // Steal from the buffer of a remote thread as a last resort
            if let Some(stole) = unsafe { (*self_).run_buffer.steal(&(*target).run_buffer) } {
                return Some(stole);
            }

            num_threads -= 1;
        }

        None
    }
}

/// An event which stores 1 semaphore token and is multi-threaded safe.
/// The event can be shutdown(), waking up all wait()ing threads and
/// making subsequent wait()'s return immediately.
struct Event {
    state: AtomicU32,
}

impl Default for Event {
    fn default() -> Self {
        Event {
            state: AtomicU32::new(Event::EMPTY),
        }
    }
}

impl Event {
    const EMPTY: u32 = 0;
    const WAITING: u32 = 1;
    const NOTIFIED: u32 = 2;
    const SHUTDOWN: u32 = 3;

    /// Wait for and consume a notification
    /// or wait for the event to be shutdown entirely
    fn wait(&self) {
        let mut acquire_with: u32 = Event::EMPTY;
        let mut state = self.state.load(Relaxed);

        loop {
            // If we're shutdown then exit early.
            // Acquire barrier to ensure operations before the shutdown() are seen after the wait().
            // Shutdown is rare so it's better to have an Acquire barrier here instead of on CAS failure + load which are common.
            if state == Event::SHUTDOWN {
                self.state.load(Acquire);
                return;
            }

            // Consume a notification when it pops up.
            // Acquire barrier to ensure operations before the notify() appear after the wait().
            if state == Event::NOTIFIED {
                match self
                    .state
                    .compare_exchange_weak(state, acquire_with, Acquire, Relaxed)
                {
                    Ok(_) => return,
                    Err(e) => {
                        state = e;
                        continue;
                    }
                }
            }

            // There is no notification to consume, we should wait on the event by ensuring its WAITING.
            if state != Event::WAITING {
                match self
                    .state
                    .compare_exchange_weak(state, Event::WAITING, Relaxed, Relaxed)
                {
                    Ok(_) => {}
                    Err(e) => {
                        state = e;
                        continue;
                    }
                }
            }

            // Wait on the event until a notify() or shutdown().
            // If we wake up to a notification, we must acquire it with WAITING instead of EMPTY
            // since there may be other threads sleeping on the Futex who haven't been woken up yet.
            //
            // Acquiring to WAITING will make the next notify() or shutdown() wake a sleeping futex thread
            // who will either exit on SHUTDOWN or acquire with WAITING again, ensuring all threads are awoken.
            // This unfortunately results in the last notify() or shutdown() doing an extra futex wake but that's fine.
            futex_wait(&self.state, Event::WAITING);
            state = self.state.load(Relaxed);
            acquire_with = Event::WAITING;
        }
    }

    /// Post a notification to the event if it doesn't have one already
    /// then wake up a waiting thread if there is one as well.
    fn notify(&self) {
        self.wake(Event::NOTIFIED, 1);
    }

    /// Marks the event as shutdown, making all future wait()'s return immediately.
    /// Then wakes up any threads currently waiting on the Event.
    fn shutdown(&self) {
        self.wake(Event::SHUTDOWN, u32::MAX);
    }

    // DELIBERATE DEVIATION (Task 1, controller-approved 2026-07-19; ledgered in
    // .port/phase-2/DEFERRALS.md "xev ThreadPool `Event` shutdown wake").
    //
    // FIRST, the corrected attribution (task-1-report.md addendum 2): the
    // originally-reported "~100% multi-worker teardown deadlock" was ROOT-CAUSED
    // to a port bug in this file's `futex_wake` shim (u32::MAX passed unclamped
    // through FUTEX_WAKE's C-int slot = -1 = wake AT MOST ONE), not to this
    // Event. With that shim fixed (see `futex_wake`), the VERBATIM oracle
    // `wake` passed 400/400 correct-usage multi-worker lifecycles. The oracle's
    // teardown holes below are therefore ANALYTIC (real interleavings, empirically
    // rare), and this deviation closes them defensively for Task-5's File
    // watcher, which multi-worker-joins under trailing notify traffic.
    //
    // The oracle (ThreadPool.zig:508-518) has two strand windows at shutdown,
    // both stemming from `notifySlow` issuing `idle_event.notify()` OUTSIDE its
    // sync CAS while `wake` swaps the event word unconditionally and wakes only
    // `if (state == WAITING)`:
    //
    // (1) skip-wake: a trailing notify swaps idle_event WAITING->NOTIFIED (waking
    // ONE of >= 2 parked sleepers); `shutdown()`'s `swap(->SHUTDOWN)` then
    // observes NOTIFIED != WAITING and SKIPS its futex_wake — the remaining
    // parked sleepers are never woken, `spawned` never reaches 0, `join()` hangs.
    // Fix: the SHUTDOWN wake is UNCONDITIONAL. Sound because SHUTDOWN is the
    // terminal, unrepeatable Event state — an extra `futex_wake(MAX)` cannot lose
    // a token or wake anything unsoundly: (a) woken sleepers re-check
    // `self.state` (observe SHUTDOWN, exit); (b) later arrivals fail the futex
    // value-check (word != WAITING) and return without sleeping. Cost: one extra
    // syscall, once, at pool shutdown.
    //
    // (2) resurrection: a notifier that won its sync CAS before pool-shutdown's
    // CAS can execute its `swap(->NOTIFIED)` AFTER `idle_event.shutdown()`'s
    // `swap(->SHUTDOWN)` + wake — overwriting the terminal state. A worker woken
    // by the shutdown wake that loads the event AFTER the overwrite sees
    // NOTIFIED; one consumes the bogus token (CAS re-arming the word to WAITING)
    // and any other re-parks on WAITING forever — the shutdown wake has already
    // fired and nothing ever wakes the event again. Fix: SHUTDOWN is STICKY —
    // the NOTIFIED transition is a CAS loop that no-ops (no write, no wake) if it
    // observes SHUTDOWN. Semantically "notifying a shutdown event is a no-op",
    // which is `Event::wait`'s own contract (wait() on a SHUTDOWN event returns
    // immediately; the notification would be meaningless). For all non-SHUTDOWN
    // prior states the CAS loop is observably identical to the oracle's swap
    // (WAITING->NOTIFIED wakes 1, EMPTY->NOTIFIED no wake, NOTIFIED->NOTIFIED
    // no wake).
    //
    // Upstream context: ghostty itself NEVER instantiates the pool (all 4 of its
    // xev loops are `xev.Loop.init(.{})` with no thread_pool — zero pool workers
    // ever spawn), so these holes are latent-dead-code in the mirrored artifact.
    // Upstream libxev's Dec 2024 `1c80766` "remove race from threadpool" fixed a
    // DIFFERENT race (the join_event join-before-notify race) and that fix is
    // ALREADY in our pinned oracle (`if (sync.spawned == 0) join_event.notify()`
    // in shutdown() + the unconditional join_event.wait() in join()); upstream
    // HEAD still carries the two windows above.
    fn wake(&self, release_with: u32, wake_threads: u32) {
        // Update the Event to notifty it with the new `release_with` state (either NOTIFIED or SHUTDOWN).
        // Release barrier to ensure any operations before this are this to happen before the wait() in the other threads.
        // DEVIATION prong (2) (see note above): NOTIFIED must not overwrite the
        // terminal SHUTDOWN state, so the notify transition is a CAS loop that
        // no-ops on a shutdown event; the shutdown transition keeps the swap.
        let state = if release_with == Event::SHUTDOWN {
            self.state.swap(release_with, Release)
        } else {
            let mut state = self.state.load(Relaxed);
            loop {
                if state == Event::SHUTDOWN {
                    return; // notifying a shutdown event is a no-op
                }
                match self
                    .state
                    .compare_exchange_weak(state, release_with, Release, Relaxed)
                {
                    Ok(_) => break state,
                    Err(e) => state = e,
                }
            }
        };

        // Only wake threads sleeping in futex if the state is WAITING.
        // Avoids unnecessary wake ups.
        // DEVIATION prong (1) (see note above): the terminal SHUTDOWN wake is
        // unconditional.
        if state == Event::WAITING || release_with == Event::SHUTDOWN {
            futex_wake(&self.state, wake_threads);
        }
    }
}

// PORT NOTE: `std.Thread.Futex.wait/wake` — Rust's std futex is not public, so
// this binds the same OS primitives std itself uses: Linux `futex(2)`
// (FUTEX_WAIT/WAKE_PRIVATE), Darwin `__ulock_wait/wake` (UL_COMPARE_AND_WAIT),
// FreeBSD `_umtx_op` (UMTX_OP.WAIT_UINT_PRIVATE/WAKE_PRIVATE, W2D),
// Windows `ntdll.RtlWaitOnAddress`/`RtlWakeAddress{Single,All}` (WS20), and — for
// every remaining target, all of which are `builtin.single_threaded` in the
// reference — Zig's `SingleThreadedImpl` (WS20).
//
// Each arm below is the direct map of the correspondingly-named `*Impl` struct in
// `std/Thread/Futex.zig`, which is exactly what `std.Thread.Futex.wait/wake`
// dispatches to. The pool always waits WITHOUT a timeout (`Event::wait` calls
// `Futex.wait(&self.state, WAITING)`), so the `timeout: ?u64` parameter is
// monomorphized to `null` in every arm.
#[cfg(all(target_os = "linux", not(miri)))]
fn futex_wait(atomic: &AtomicU32, expect: u32) {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            atomic.as_ptr(),
            libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG,
            expect,
            core::ptr::null::<libc::timespec>(),
        );
    }
}

#[cfg(all(target_os = "linux", not(miri)))]
fn futex_wake(atomic: &AtomicU32, n: u32) {
    // PORT-FIDELITY FIX (root cause of the original 100%-reproducible teardown
    // wedge — see task-1-report.md): `std.Thread.Futex.wake` clamps the waiter
    // count to `maxInt(i32)` before the syscall, because FUTEX_WAKE's count
    // argument is a C `int`. Passing `u32::MAX` ("wake all") unclamped through
    // the variadic int slot becomes `-1`, and the kernel's wake loop
    // (`if (++ret >= nr_wake) break`) then wakes AT MOST ONE waiter — so
    // `Event::shutdown`'s wake-all stranded every sleeper but one. The clamp IS
    // the oracle's semantic (i32::MAX >= any possible waiter count = wake all).
    let n = n.min(i32::MAX as u32) as libc::c_int;
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            atomic.as_ptr(),
            libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG,
            n,
        );
    }
}

// ---------------------------------------------------------------------------
// `FreebsdImpl` (std/Thread/Futex.zig:303) — W2D. `Impl` selects it for
// `builtin.os.tag == .freebsd` (Futex.zig:77-78), between the Linux and OpenBSD
// arms. `_umtx_op` is the FreeBSD userland-mutex syscall:
// https://www.freebsd.org/cgi/man.cgi?query=_umtx_op&sektion=2&n=1
// ---------------------------------------------------------------------------

#[cfg(all(target_os = "freebsd", not(miri)))]
fn futex_wait(atomic: &AtomicU32, expect: u32) {
    // `const rc = c._umtx_op(@intFromPtr(&ptr.raw), @intFromEnum(c.UMTX_OP.
    //  WAIT_UINT_PRIVATE), @as(c_ulong, expect), tm_size, @intFromPtr(tm_ptr));`
    // (Futex.zig:319-325). The pool never passes a timeout, so `tm_size == 0` and
    // `tm_ptr == null` — both tail slots are the null pointer here.
    //
    // rc DISCARDED, same discipline as this file's linux arm: the reference's
    // switch maps `.SUCCESS` and `.INTR` (a spurious wake) to a plain return, and
    // `.TIMEDOUT` is unreachable without a timeout; every caller re-checks state
    // in a loop (Futex.zig:327-337).
    //
    // SAFETY: `atomic` is a live, aligned `u32` for the duration of the call; the
    // two tail arguments are null, which is what the no-timeout path passes.
    unsafe {
        libc::_umtx_op(
            atomic.as_ptr().cast::<libc::c_void>(),
            libc::UMTX_OP_WAIT_UINT_PRIVATE,
            expect as libc::c_ulong,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
    }
}

#[cfg(all(target_os = "freebsd", not(miri)))]
fn futex_wake(atomic: &AtomicU32, n: u32) {
    // `c._umtx_op(@intFromPtr(&ptr.raw), @intFromEnum(c.UMTX_OP.WAKE_PRIVATE),
    //  @as(c_ulong, max_waiters), 0, 0)` (Futex.zig:340-347).
    //
    // NO CLAMP, deliberately (R1): unlike `LinuxImpl.wake`, `FreebsdImpl.wake`
    // passes `max_waiters` straight through. The linux arm's `min(i32::MAX)` is an
    // artifact of FUTEX_WAKE's C `int` count slot; `_umtx_op`'s `val` slot is a
    // `c_ulong`, so `u32::MAX` ("wake all") arrives intact and must NOT be clamped
    // here.
    //
    // SAFETY: as `futex_wait` above.
    unsafe {
        libc::_umtx_op(
            atomic.as_ptr().cast::<libc::c_void>(),
            libc::UMTX_OP_WAKE_PRIVATE,
            n as libc::c_ulong,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
extern "C" {
    fn __ulock_wait(
        operation: u32,
        addr: *mut core::ffi::c_void,
        value: u64,
        timeout_us: u32,
    ) -> i32;
    fn __ulock_wake(operation: u32, addr: *mut core::ffi::c_void, wake_value: u64) -> i32;
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
const UL_COMPARE_AND_WAIT: u32 = 1;
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
const ULF_WAKE_ALL: u32 = 0x0000_0100;

#[cfg(all(
    any(target_os = "macos", target_os = "ios", target_os = "visionos"),
    not(miri)
))]
fn futex_wait(atomic: &AtomicU32, expect: u32) {
    unsafe {
        __ulock_wait(
            UL_COMPARE_AND_WAIT,
            atomic.as_ptr() as *mut core::ffi::c_void,
            expect as u64,
            0,
        );
    }
}

#[cfg(all(
    any(target_os = "macos", target_os = "ios", target_os = "visionos"),
    not(miri)
))]
fn futex_wake(atomic: &AtomicU32, n: u32) {
    let op = if n > 1 {
        UL_COMPARE_AND_WAIT | ULF_WAKE_ALL
    } else {
        UL_COMPARE_AND_WAIT
    };
    unsafe {
        __ulock_wake(op, atomic.as_ptr() as *mut core::ffi::c_void, 0);
    }
}

// ---------------------------------------------------------------------------
// `WindowsImpl` (std/Thread/Futex.zig:132) — WS20.
//
// Verbatim reference comment (R3):
//   // We use WaitOnAddress through NtDll instead of API-MS-Win-Core-Synch-l1-2-0.dll
//   // as it's generally already a linked target and is autoloaded into all processes anyway.
//
// Hand-rolled `extern "system"` per repo convention (no `windows-sys` dep, no
// `libc` reliance for Win32).
// ---------------------------------------------------------------------------

/// `windows.NTSTATUS` — only `.SUCCESS` (0) and `.TIMEOUT` (0x102) are reachable
/// here; the reference's `else => unreachable`.
#[cfg(windows)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
type NTSTATUS = i32;
#[cfg(windows)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
type LARGE_INTEGER = i64;

#[cfg(windows)]
#[link(name = "ntdll")]
extern "system" {
    /// `windows.ntdll.RtlWaitOnAddress(ptr, &expect, @sizeOf(u32), timeout_ptr)`.
    fn RtlWaitOnAddress(
        address: *const core::ffi::c_void,
        compare_address: *const core::ffi::c_void,
        address_size: usize,
        timeout: *const LARGE_INTEGER,
    ) -> NTSTATUS;

    /// `windows.ntdll.RtlWakeAddressSingle(address)`.
    fn RtlWakeAddressSingle(address: *const core::ffi::c_void);

    /// `windows.ntdll.RtlWakeAddressAll(address)`.
    fn RtlWakeAddressAll(address: *const core::ffi::c_void);
}

#[cfg(all(windows, not(miri)))]
fn futex_wait(atomic: &AtomicU32, expect: u32) {
    // NTDLL functions work with time in units of 100 nanoseconds.
    // Positive values are absolute deadlines while negative values are relative
    // durations. `timeout` is always null here (`Event::wait` never times out),
    // so `timeout_ptr` stays null and `.TIMEOUT` is unreachable.
    let expect_local = expect;
    // SAFETY: `atomic` and `expect_local` are live for the duration of the call;
    // `address_size` is the size of the compared word.
    let rc = unsafe {
        RtlWaitOnAddress(
            atomic.as_ptr().cast::<core::ffi::c_void>(),
            core::ptr::addr_of!(expect_local).cast::<core::ffi::c_void>(),
            core::mem::size_of::<u32>(),
            core::ptr::null(),
        )
    };
    // switch (rc) { .SUCCESS => {}, .TIMEOUT => { assert(timeout != null); ... },
    //               else => unreachable }
    debug_assert!(rc == 0, "RtlWaitOnAddress: unexpected NTSTATUS {rc:#x}");
}

#[cfg(all(windows, not(miri)))]
fn futex_wake(atomic: &AtomicU32, n: u32) {
    // assert(max_waiters != 0);
    debug_assert!(n != 0);
    let address = atomic.as_ptr().cast::<core::ffi::c_void>();
    // switch (max_waiters) { 1 => RtlWakeAddressSingle, else => RtlWakeAddressAll }
    // SAFETY: `address` is a live, aligned `u32` the waiters compare against.
    unsafe {
        if n == 1 {
            RtlWakeAddressSingle(address);
        } else {
            RtlWakeAddressAll(address);
        }
    }
}

// ---------------------------------------------------------------------------
// `SingleThreadedImpl` (std/Thread/Futex.zig:105) — WS20.
//
// `Impl = if (builtin.single_threaded) SingleThreadedImpl else ...` is the FIRST
// arm of the reference's dispatch chain, and every target left over here (WASI
// without the threads proposal, freestanding wasm) is single-threaded. Verbatim
// reference comments preserved (R3).
//
// W2D: `freebsd` is EXCLUDED from this arm — it is a multithreaded target with
// its own `FreebsdImpl` above. Landing it here would have made the first pool
// sleep hit `unreachable!("deadlock detected")`.
// ---------------------------------------------------------------------------

#[cfg(all(
    not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "visionos",
        target_os = "freebsd",
        windows
    )),
    not(miri)
))]
fn futex_wait(atomic: &AtomicU32, expect: u32) {
    // if (ptr.raw != expect) return;
    if atomic.load(Relaxed) != expect {
        return;
    }

    // There are no threads to wake us up.
    // So if we wait without a timeout we would never wake up.
    //
    // (`const delay = timeout orelse { unreachable; // deadlock detected };` —
    // the pool never passes a timeout, so this arm is the reference's
    // `unreachable`. Reaching it IS the deadlock the reference declares
    // impossible: with no other threads, nothing can ever have parked here.)
    unreachable!("deadlock detected");
}

#[cfg(all(
    not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "ios",
        target_os = "visionos",
        target_os = "freebsd",
        windows
    )),
    not(miri)
))]
fn futex_wake(atomic: &AtomicU32, n: u32) {
    // There are no other threads to possibly wake up
    let _ = atomic;
    let _ = n;
}

// ---------------------------------------------------------------------------
// MIRI-ONLY futex arm (WS-XEV-UB).
//
// Every real arm above bottoms out in a foreign function Miri cannot execute
// (`__ulock_wait`/`__ulock_wake` on Darwin, the `futex(2)` syscall on Linux,
// `ntdll.RtlWaitOnAddress` on Windows) — so under Miri the whole pool lifecycle
// aborts with "unsupported operation: can't call foreign function" the first
// time a worker parks or is woken, which is exactly what kept
// `multi_worker_quiesce_then_join_50x` (the crate's only real multi-worker
// Stacked-Borrows exercise) from ever completing a run under Miri.
//
// A futex is allowed to return spuriously — `std.Thread.Futex.wait`'s own
// contract ("this function can spuriously wake up"), and every caller here is a
// re-checking loop (`Event::wait` reloads `self.state` and re-CASes after every
// `futex_wait`). So the sound Miri stand-in is a spin/yield: `futex_wait`
// value-checks and yields (an immediate spurious wakeup), `futex_wake` is a
// no-op. Correctness of the pool is unchanged — only its blocking behaviour
// (busy-wait instead of park), which no test observes. This arm is
// `cfg(miri)`-only and cannot affect ANY shipped build; the real arms are gated
// `not(miri)`. It is a Miri accommodation, NOT a port deviation: the oracle's
// semantics are preserved, spurious wakeups included.
// ---------------------------------------------------------------------------

#[cfg(miri)]
fn futex_wait(atomic: &AtomicU32, expect: u32) {
    // The value check the real futex does atomically at park time; the yield
    // then models an immediate spurious wakeup.
    if atomic.load(Relaxed) != expect {
        return;
    }
    std::thread::yield_now();
}

#[cfg(miri)]
fn futex_wake(atomic: &AtomicU32, n: u32) {
    // Nothing is ever parked (waiters spin), so there is nothing to wake.
    let _ = atomic;
    let _ = n;
}

// ---------------------------------------------------------------------------
// Linked list intrusive memory node and lock-free data structures to operate with it
//
// PORT NOTE: the Zig `Node` is a struct namespace that IS a node (`next: ?*Node`)
// AND hosts `Node.List` / `Node.Queue` / `Node.Buffer`. Rust cannot nest a struct
// inside another struct/impl, so the four are flat siblings: `Node` (the intrusive
// node), `NodeList`, `NodeQueue` (the swap-based MPSC queue), and `NodeBuffer` (the
// bounded work-stealing ring). The SB/TB-cleanliness of the raw-pointer `NodeQueue`
// and `NodeBuffer` primitives was proven on `.port/phase-2/mirihar-threadpool/`
// BEFORE this port (Task 1 Step 2; the ONLY rule that matters is that `Node.next`
// is touched exclusively through raw `*mut Node`, never a `&Node`/`&mut Node`).
// ---------------------------------------------------------------------------

/// `?*Node`/`?*Thread` reconstruction from a possibly-null raw pointer.
#[inline]
fn opt_ptr<T>(p: *mut T) -> Option<*mut T> {
    if p.is_null() {
        None
    } else {
        Some(p)
    }
}

/// `@ptrFromInt(addr)` into a `?*Node`: address 0 is Zig's null pointer.
#[inline]
fn node_from_addr(addr: usize) -> Option<*mut Node> {
    if addr == 0 {
        None
    } else {
        Some(core::ptr::with_exposed_provenance_mut::<Node>(addr))
    }
}

/// The intrusive queue node embedded in every `Task` (Zap `Node`).
#[derive(Default)]
pub struct Node {
    /// `next: ?*Node = null`
    pub next: Option<*mut Node>,
}

/// A linked list of Nodes (Zig `Node.List`).
#[derive(Clone, Copy)]
struct NodeList {
    head: *mut Node,
    tail: *mut Node,
}

/// An unbounded multi-producer-(non blocking)-multi-consumer queue of Node pointers.
/// (Zig `Node.Queue`.)
struct NodeQueue {
    stack: AtomicUsize,
    // Zig `cache: ?*Node = null` — mutated only by the single consumer holding
    // IS_CONSUMING; `UnsafeCell` so the `&self` methods can write it (no data
    // race: the consumer bit serialises all cache access).
    cache: UnsafeCell<Option<*mut Node>>,
}

impl Default for NodeQueue {
    fn default() -> Self {
        NodeQueue {
            stack: AtomicUsize::new(0),
            cache: UnsafeCell::new(None),
        }
    }
}

const HAS_CACHE: usize = 0b01;
const IS_CONSUMING: usize = 0b10;
const PTR_MASK: usize = !(HAS_CACHE | IS_CONSUMING);

// comptime { assert(@alignOf(Node) >= ((IS_CONSUMING | HAS_CACHE) + 1)); }
// (faithful port of the Zig expression; clippy's `> x` rewrite would drop the
// `+ 1` the oracle wrote, so the expression is preserved verbatim.)
#[allow(clippy::int_plus_one)]
const _: () = assert!(core::mem::align_of::<Node>() >= (IS_CONSUMING | HAS_CACHE) + 1);

/// error{ Empty, Contended }
enum AcquireError {
    Empty,
    Contended,
}

impl NodeQueue {
    fn push(&self, list: NodeList) {
        let mut stack = self.stack.load(Relaxed);
        loop {
            // Attach the list to the stack (pt. 1)
            unsafe { (*list.tail).next = node_from_addr(stack & PTR_MASK) };

            // Update the stack with the list (pt. 2).
            // Don't change the HAS_CACHE and IS_CONSUMING bits of the consumer.
            let mut new_stack = list.head.expose_provenance();
            debug_assert_eq!(new_stack & !PTR_MASK, 0);
            new_stack |= stack & !PTR_MASK;

            // Push to the stack with a release barrier for the consumer to see the proper list links.
            match self
                .stack
                .compare_exchange_weak(stack, new_stack, Release, Relaxed)
            {
                Ok(_) => break,
                Err(e) => stack = e,
            }
        }
    }

    fn try_acquire_consumer(&self) -> Result<Option<*mut Node>, AcquireError> {
        let mut stack = self.stack.load(Relaxed);
        loop {
            if stack & IS_CONSUMING != 0 {
                return Err(AcquireError::Contended); // The queue already has a consumer.
            }
            if stack & (HAS_CACHE | PTR_MASK) == 0 {
                return Err(AcquireError::Empty); // The queue is empty when there's nothing cached and nothing in the stack.
            }

            // When we acquire the consumer, also consume the pushed stack if the cache is empty.
            let mut new_stack = stack | HAS_CACHE | IS_CONSUMING;
            if stack & HAS_CACHE == 0 {
                debug_assert!(stack & PTR_MASK != 0);
                new_stack &= !PTR_MASK;
            }

            // Acquire barrier on getting the consumer to see cache/Node updates done by previous consumers
            // and to ensure our cache/Node updates in pop() happen after that of previous consumers.
            match self
                .stack
                .compare_exchange_weak(stack, new_stack, Acquire, Relaxed)
            {
                Ok(_) => {
                    // return self.cache orelse @ptrFromInt(stack & PTR_MASK);
                    let cache = unsafe { *self.cache.get() };
                    return Ok(cache.or_else(|| node_from_addr(stack & PTR_MASK)));
                }
                Err(e) => stack = e,
            }
        }
    }

    fn release_consumer(&self, consumer: Option<*mut Node>) {
        // Stop consuming and remove the HAS_CACHE bit as well if the consumer's cache is empty.
        // When HAS_CACHE bit is zeroed, the next consumer will acquire the pushed stack nodes.
        let mut remove = IS_CONSUMING;
        if consumer.is_none() {
            remove |= HAS_CACHE;
        }

        // Release the consumer with a release barrier to ensure cache/node accesses
        // happen before the consumer was released and before the next consumer starts using the cache.
        unsafe { *self.cache.get() = consumer };
        let stack = self.stack.fetch_sub(remove, Release);
        debug_assert!(stack & remove != 0);
    }

    fn pop(&self, consumer_ref: &mut Option<*mut Node>) -> Option<*mut Node> {
        // Check the consumer cache (fast path)
        if let Some(node) = *consumer_ref {
            *consumer_ref = unsafe { (*node).next };
            return Some(node);
        }

        // Load the stack to see if there was anything pushed that we could grab.
        let mut stack = self.stack.load(Relaxed);
        debug_assert!(stack & IS_CONSUMING != 0);
        if stack & PTR_MASK == 0 {
            return None;
        }

        // Nodes have been pushed to the stack, grab then with an Acquire barrier to see the Node links.
        stack = self.stack.swap(HAS_CACHE | IS_CONSUMING, Acquire);
        debug_assert!(stack & IS_CONSUMING != 0);
        debug_assert!(stack & PTR_MASK != 0);

        let node: *mut Node = core::ptr::with_exposed_provenance_mut(stack & PTR_MASK);
        *consumer_ref = unsafe { (*node).next };
        Some(node)
    }
}

/// A bounded single-producer, multi-consumer ring buffer for node pointers.
/// (Zig `Node.Buffer`.)
struct NodeBuffer {
    head: AtomicU32,
    tail: AtomicU32,
    array: [AtomicPtr<Node>; CAPACITY],
}

type Index = u32;
const CAPACITY: usize = 256; // Appears to be a pretty good trade-off in space vs contended throughput
                             // comptime { assert(std.math.maxInt(Index) >= capacity); assert(std.math.isPowerOfTwo(capacity)); }
const _: () = assert!(Index::MAX as usize >= CAPACITY);
const _: () = assert!(CAPACITY.is_power_of_two());

impl Default for NodeBuffer {
    fn default() -> Self {
        NodeBuffer {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            // Zig leaves `array` `undefined`; the algorithm never reads a slot
            // before writing it (size accounting), so null-init is safe.
            array: [const { AtomicPtr::new(core::ptr::null_mut()) }; CAPACITY],
        }
    }
}

/// error{Overflow}
struct Overflow;

struct Stole {
    node: *mut Node,
    pushed: bool,
}

impl NodeBuffer {
    fn push(&self, list: &mut NodeList) -> Result<(), Overflow> {
        let mut head = self.head.load(Relaxed);
        let mut tail = self.tail.load(Relaxed); // we're the only thread that can change this

        loop {
            let mut size = tail.wrapping_sub(head);
            debug_assert!(size as usize <= CAPACITY);

            // Push nodes from the list to the buffer if it's not empty..
            if (size as usize) < CAPACITY {
                let mut nodes: Option<*mut Node> = Some(list.head);
                while (size as usize) < CAPACITY {
                    let node = match nodes {
                        Some(n) => n,
                        None => break,
                    };
                    nodes = unsafe { (*node).next };

                    // Array written atomically with weakest ordering since it could be getting atomically read by steal().
                    self.array[(tail as usize) % CAPACITY].store(node, Relaxed);
                    tail = tail.wrapping_add(1);
                    size = size.wrapping_add(1);
                }

                // Release barrier synchronizes with Acquire loads for steal()ers to see the array writes.
                self.tail.store(tail, Release);

                // Update the list with the nodes we pushed to the buffer and try again if there's more.
                match nodes {
                    None => return Ok(()),
                    Some(h) => list.head = h,
                }
                std::hint::spin_loop();
                head = self.head.load(Relaxed);
                continue;
            }

            // Try to steal/overflow half of the tasks in the buffer to make room for future push()es.
            // Migrating half amortizes the cost of stealing while requiring future pops to still use the buffer.
            // Acquire barrier to ensure the linked list creation after the steal only happens after we succesfully steal.
            let mut migrate = size / 2;
            match self.head.compare_exchange_weak(
                head,
                head.wrapping_add(migrate),
                Acquire,
                Relaxed,
            ) {
                Ok(_) => {
                    // Link the migrated Nodes together
                    let first = self.array[(head as usize) % CAPACITY].load(Relaxed);
                    while migrate > 0 {
                        let prev = self.array[(head as usize) % CAPACITY].load(Relaxed);
                        head = head.wrapping_add(1);
                        unsafe {
                            (*prev).next =
                                Some(self.array[(head as usize) % CAPACITY].load(Relaxed))
                        };
                        migrate -= 1;
                    }

                    // Append the list that was supposed to be pushed to the end of the migrated Nodes
                    let last = self.array[(head.wrapping_sub(1) as usize) % CAPACITY].load(Relaxed);
                    unsafe { (*last).next = Some(list.head) };
                    unsafe { (*list.tail).next = None };

                    // Return the migrated nodes + the original list as overflowed
                    list.head = first;
                    return Err(Overflow);
                }
                Err(e) => head = e,
            }
        }
    }

    fn pop(&self) -> Option<*mut Node> {
        let mut head = self.head.load(Relaxed);
        let tail = self.tail.load(Relaxed); // we're the only thread that can change this

        loop {
            // Quick sanity check and return null when not empty
            let size = tail.wrapping_sub(head);
            debug_assert!(size as usize <= CAPACITY);
            if size == 0 {
                return None;
            }

            // Dequeue with an acquire barrier to ensure any writes done to the Node
            // only happen after we succesfully claim it from the array.
            match self
                .head
                .compare_exchange_weak(head, head.wrapping_add(1), Acquire, Relaxed)
            {
                Ok(_) => return Some(self.array[(head as usize) % CAPACITY].load(Relaxed)),
                Err(e) => head = e,
            }
        }
    }

    fn consume(&self, queue: &NodeQueue) -> Option<Stole> {
        let mut consumer = match queue.try_acquire_consumer() {
            Ok(c) => c,
            Err(_) => return None,
        };
        // Zig `defer queue.releaseConsumer(consumer)` runs with the FINAL `consumer`
        // value at each return; we release explicitly before each return below.

        let head = self.head.load(Relaxed);
        let tail = self.tail.load(Relaxed); // we're the only thread that can change this

        let size = tail.wrapping_sub(head);
        debug_assert!(size as usize <= CAPACITY);
        debug_assert!(size == 0); // we should only be consuming if our array is empty

        // Pop nodes from the queue and push them to our array.
        // Atomic stores to the array as steal() threads may be atomically reading from it.
        let mut pushed: Index = 0;
        while (pushed as usize) < CAPACITY {
            let node = match queue.pop(&mut consumer) {
                Some(n) => n,
                None => break,
            };
            self.array[(tail.wrapping_add(pushed) as usize) % CAPACITY].store(node, Relaxed);
            pushed += 1;
        }

        // We will be returning one node that we stole from the queue.
        // Get an extra, and if that's not possible, take one from our array.
        let node = match queue.pop(&mut consumer) {
            Some(n) => n,
            None => {
                if pushed == 0 {
                    queue.release_consumer(consumer);
                    return None;
                }
                pushed -= 1;
                self.array[(tail.wrapping_add(pushed) as usize) % CAPACITY].load(Relaxed)
            }
        };

        // Update the array tail with the nodes we pushed to it.
        // Release barrier to synchronize with Acquire barrier in steal()'s to see the written array Nodes.
        if pushed > 0 {
            self.tail.store(tail.wrapping_add(pushed), Release);
        }
        queue.release_consumer(consumer);
        Some(Stole {
            node,
            pushed: pushed > 0,
        })
    }

    fn steal(&self, buffer: &NodeBuffer) -> Option<Stole> {
        let head = self.head.load(Relaxed);
        let tail = self.tail.load(Relaxed); // we're the only thread that can change this

        let size = tail.wrapping_sub(head);
        debug_assert!(size as usize <= CAPACITY);
        debug_assert!(size == 0); // we should only be stealing if our array is empty

        loop {
            let buffer_head = buffer.head.load(Acquire);
            let buffer_tail = buffer.tail.load(Acquire);

            // Overly large size indicates the the tail was updated a lot after the head was loaded.
            // Reload both and try again.
            let buffer_size = buffer_tail.wrapping_sub(buffer_head);
            if buffer_size as usize > CAPACITY {
                std::hint::spin_loop();
                continue;
            }

            // Try to steal half (divCeil) to amortize the cost of stealing from other threads.
            let steal_size = buffer_size - (buffer_size / 2);
            if steal_size == 0 {
                return None;
            }

            // Copy the nodes we will steal from the target's array to our own.
            // Atomically load from the target buffer array as it may be pushing and atomically storing to it.
            // Atomic store to our array as other steal() threads may be atomically loading from it as above.
            let mut i: Index = 0;
            while i < steal_size {
                let node =
                    buffer.array[(buffer_head.wrapping_add(i) as usize) % CAPACITY].load(Relaxed);
                self.array[(tail.wrapping_add(i) as usize) % CAPACITY].store(node, Relaxed);
                i += 1;
            }

            // Try to commit the steal from the target buffer using:
            // - an Acquire barrier to ensure that we only interact with the stolen Nodes after the steal was committed.
            // - a Release barrier to ensure that the Nodes are copied above prior to the committing of the steal
            //   because if they're copied after the steal, the could be getting rewritten by the target's push().
            match buffer.head.compare_exchange(
                buffer_head,
                buffer_head.wrapping_add(steal_size),
                AcqRel,
                Relaxed,
            ) {
                Ok(_) => {
                    // Pop one from the nodes we stole as we'll be returning it
                    let pushed = steal_size - 1;
                    let node =
                        self.array[(tail.wrapping_add(pushed) as usize) % CAPACITY].load(Relaxed);

                    // Update the array tail with the nodes we pushed to it.
                    // Release barrier to synchronize with Acquire barrier in steal()'s to see the written array Nodes.
                    if pushed > 0 {
                        self.tail.store(tail.wrapping_add(pushed), Release);
                    }
                    return Some(Stole {
                        node,
                        pushed: pushed > 0,
                    });
                }
                Err(_) => {
                    std::hint::spin_loop();
                    continue;
                }
            }
        }
    }
}

// The lock-free containers are shared across worker threads via raw pointers; the
// interior state is atomics + the single-consumer `UnsafeCell` cache, so sharing
// is race-free (proven on the mirihar harness). These `unsafe impl`s let the pool
// / thread carry them across `std::thread::spawn`.
unsafe impl std::marker::Sync for NodeQueue {}
unsafe impl std::marker::Send for NodeQueue {}
unsafe impl std::marker::Sync for NodeBuffer {}
unsafe impl std::marker::Send for NodeBuffer {}
unsafe impl std::marker::Sync for Thread {}
unsafe impl std::marker::Send for Thread {}
unsafe impl std::marker::Sync for ThreadPool {}
unsafe impl std::marker::Send for ThreadPool {}

// ThreadPool.zig ships no `test{}` of its own; this is the schedule/execute smoke
// test the brief (Task 1 Step 4) asks for: spawn N tasks across the pool, assert
// every callback ran, and `deinit` joins cleanly.
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::time::Duration;

    struct TestCtx {
        ran: Mutex<usize>,
        cv: Condvar,
        expected: usize,
    }

    static CTX: OnceLock<TestCtx> = OnceLock::new();

    fn on_task(_t: *mut Task) {
        let ctx = CTX.get().unwrap();
        let mut n = ctx.ran.lock().unwrap();
        *n += 1;
        if *n == ctx.expected {
            ctx.cv.notify_all();
        }
    }

    fn run_schedule_all(max_threads: u32, n: usize) {
        let _ = CTX.set(TestCtx {
            ran: Mutex::new(0),
            cv: Condvar::new(),
            expected: n,
        });
        // Fresh counter each invocation (OnceLock may already be set by a prior test).
        *CTX.get().unwrap().ran.lock().unwrap() = 0;

        // `..Config::default()`, NOT an explicit `stack_size`. The reference
        // gives the field a DEFAULT — `stack_size: u32 = (std.Thread.SpawnConfig{})
        // .stack_size` (16 MiB, ThreadPool.zig:84) — so a Zig test written as
        // `.{ .max_threads = n }` silently receives 16 MiB. Rust struct literals
        // require every field, and these two tests originally wrote `stack_size:
        // 0`, which `init`'s `max(1, ..)` turns into a ONE-BYTE request; macOS
        // then clamps to PTHREAD_STACK_MIN = 16 KiB. That is a dropped Zig field
        // default, and it is what made the ASan companion arm die with
        // `BUS ... ___chkstk_darwin` in `pthread_create` — a crash previously
        // attributed to ASan frames not fitting "16 KB pool-thread stacks",
        // i.e. to the tool rather than to us.
        let pool = ThreadPool::init(Config {
            max_threads,
            ..Config::default()
        });

        // Tasks must outlive the pool run; box them so their `node`s have stable
        // addresses while the pool links them through raw `*mut Node`.
        let mut tasks: Vec<Box<Task>> = (0..n)
            .map(|_| {
                Box::new(Task {
                    node: Node::default(),
                    callback: on_task,
                })
            })
            .collect();

        for t in tasks.iter_mut() {
            let ptr: *mut Task = &mut **t;
            pool.schedule(Batch::from(ptr));
        }

        // Wait until every task has run BEFORE shutting down.
        let ctx = CTX.get().unwrap();
        let mut ran = ctx.ran.lock().unwrap();
        while *ran < n {
            let (g, timeout) = ctx.cv.wait_timeout(ran, Duration::from_secs(30)).unwrap();
            ran = g;
            assert!(
                !timeout.timed_out(),
                "only {}/{} tasks ran within the timeout",
                *ran,
                n
            );
        }
        drop(ran);

        pool.shutdown();
        pool.deinit();

        assert_eq!(*ctx.ran.lock().unwrap(), n, "not all task callbacks ran");
        drop(tasks);
    }

    // The full pool lifecycle end-to-end: spawn a worker, schedule N tasks onto
    // the global run queue, run every callback, then `shutdown` + `deinit` join
    // cleanly. Single worker (max_threads = 1) as the deterministic baseline;
    // multi-worker teardown (formerly wedged by the `futex_wake` clamp bug, now
    // fixed, with the oracle's analytic strand windows closed by the ledgered
    // `Event::wake` deviation) is covered by `multi_worker_quiesce_then_join_50x`
    // below, and multi-threaded correctness of the lock-free containers is proven
    // on `.port/phase-2/mirihar-threadpool/`.
    #[test]
    fn schedule_runs_all_tasks_and_deinit_joins() {
        run_schedule_all(1, 200);
    }

    #[test]
    fn init_shutdown_deinit_without_tasks_joins_cleanly() {
        // With no scheduled work no worker is ever spawned; shutdown() must still
        // wake join() (spawned == 0 path) so deinit() returns without hanging.
        let pool = ThreadPool::init(Config {
            stack_size: 1024 * 1024,
            max_threads: 4,
        });
        pool.shutdown();
        pool.deinit();
    }

    #[test]
    fn batch_push_links_tasks() {
        // Exercise Batch::from + Batch::push (the group-scheduling surface).
        let mut a = Task::default();
        let mut b = Task::default();
        let mut batch = Batch::from(&mut a);
        batch.push(Batch::from(&mut b));
        assert_eq!(batch.len, 2);
        assert_eq!(batch.head, Some(&mut a as *mut Task));
        assert_eq!(batch.tail, Some(&mut b as *mut Task));
        // a.node.next should now point at b.node.
        let expect = core::ptr::addr_of_mut!(b.node);
        assert_eq!(a.node.next, Some(expect));
    }

    // -----------------------------------------------------------------------
    // Multi-worker quiesce-then-join (the REAL usage Task 5's File watcher will
    // drive). Each callback blocks briefly BEFORE recording completion, so work
    // backs up and the pool actually spawns >= 2 workers that run concurrently.
    // Main then waits (condvar) for every task to finish — with NO concurrent
    // schedule() after that point — and only then shutdown()+deinit()+joins.
    // This distinguishes "the race needs a concurrent schedule() during
    // shutdown" from "correct quiesce-then-join hangs too". A watchdog aborts
    // loudly if any lifecycle deadlocks, so a hang FAILS the suite instead of
    // blocking it forever.
    // -----------------------------------------------------------------------
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    struct LifeCtx {
        ran: Mutex<usize>,
        cv: Condvar,
        expected: usize,
        active: AtomicUsize, // currently-executing callbacks (observed concurrency)
        peak: AtomicUsize,   // max observed concurrency
    }

    // The task carrier: `task` MUST be recoverable from the callback's `*mut Task`
    // via offset_of, and it carries a raw handle to the shared lifecycle context.
    //
    // PORT NOTE (provenance contract of `schedule`, WS-XEV-UB): the pool routes
    // `*mut Node` addresses through `usize` (Zap packs the HAS_CACHE/IS_CONSUMING
    // bits into the stack word, so `NodeQueue::push` does `expose_provenance()`
    // and `pop` does `with_exposed_provenance_mut`). The pointer the callback
    // receives is therefore a WILDCARD pointer, checked by Stacked Borrows
    // against the set of EXPOSED tags. A callback that recovers its enclosing
    // carrier via `container_of` (`Task` -> `LifeJob` here; `Task` ->
    // `Completion` in `kqueue.rs`/`epoll.rs`) reads bytes OUTSIDE the `Task`
    // sub-range, so the tag that was exposed must span the WHOLE carrier
    // allocation. Consequently the `*mut Task` handed to `Batch::from` must be
    // derived from a raw pointer to the whole carrier
    // (`addr_of_mut!((*job).task)`), NEVER from a `&mut job.task` field
    // reference — a field reborrow retags a tag covering only the `Task` bytes,
    // and the callback's `container_of` walk then has no suitable exposed tag.
    // The two real callers already do this correctly (`epoll.rs:837`,
    // `kqueue.rs:1375`); this test used to do the `&mut j.task` form and was
    // genuine Stacked-Borrows UB ("attempting a read access using <wildcard> ...
    // but no exposed tags have suitable permission"). Hence the carriers below
    // are leaked with `Box::into_raw` and only ever touched through that raw
    // pointer's own provenance; no `&mut LifeJob` is ever created.
    struct LifeJob {
        task: Task,
        ctx: *const LifeCtx,
    }

    fn life_task(t: *mut Task) {
        // Recover the enclosing LifeJob and its ctx (the pool never moves the
        // boxed job; deinit joins all workers before ctx is dropped).
        let job =
            unsafe { (t as *mut u8).sub(core::mem::offset_of!(LifeJob, task)) as *mut LifeJob };
        let ctx = unsafe { &*(*job).ctx };

        let cur = ctx.active.fetch_add(1, Ordering::SeqCst) + 1;
        ctx.peak.fetch_max(cur, Ordering::SeqCst);
        // Block briefly so work backs up and forces >= 2 workers to spin up.
        std::thread::sleep(Duration::from_millis(2));
        ctx.active.fetch_sub(1, Ordering::SeqCst);

        let mut n = ctx.ran.lock().unwrap();
        *n += 1;
        if *n == ctx.expected {
            ctx.cv.notify_all();
        }
    }

    /// One full correct lifecycle: schedule -> wait-all-ran (quiesce) -> shutdown
    /// -> deinit (join). Returns the peak observed concurrency. NO schedule()
    /// happens after the quiesce point.
    fn run_one_multi_lifecycle(max_threads: u32, n: usize) -> usize {
        // The context is leaked to a raw pointer as well: the workers reach it
        // through `(*job).ctx`, so no `Box`-owned (`Unique`) reborrow may exist
        // on the main thread while they run. It is reclaimed at the end, after
        // `deinit()` has joined every worker.
        let ctx_ptr: *const LifeCtx = Box::into_raw(Box::new(LifeCtx {
            ran: Mutex::new(0),
            cv: Condvar::new(),
            expected: n,
            active: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }));
        let ctx: &LifeCtx = unsafe { &*ctx_ptr };

        // `..Config::default()`, NOT an explicit `stack_size`. The reference
        // gives the field a DEFAULT — `stack_size: u32 = (std.Thread.SpawnConfig{})
        // .stack_size` (16 MiB, ThreadPool.zig:84) — so a Zig test written as
        // `.{ .max_threads = n }` silently receives 16 MiB. Rust struct literals
        // require every field, and these two tests originally wrote `stack_size:
        // 0`, which `init`'s `max(1, ..)` turns into a ONE-BYTE request; macOS
        // then clamps to PTHREAD_STACK_MIN = 16 KiB. That is a dropped Zig field
        // default, and it is what made the ASan companion arm die with
        // `BUS ... ___chkstk_darwin` in `pthread_create` — a crash previously
        // attributed to ASan frames not fitting "16 KB pool-thread stacks",
        // i.e. to the tool rather than to us.
        let pool = ThreadPool::init(Config {
            max_threads,
            ..Config::default()
        });

        // `Box::into_raw` -> the raw pointer's provenance spans the WHOLE
        // `LifeJob`, which is what `life_task`'s `container_of` walk needs (see
        // the PORT NOTE on `LifeJob`).
        let jobs: Vec<*mut LifeJob> = (0..n)
            .map(|_| {
                Box::into_raw(Box::new(LifeJob {
                    task: Task {
                        node: Node::default(),
                        callback: life_task,
                    },
                    ctx: ctx_ptr,
                }))
            })
            .collect();

        for &jp in jobs.iter() {
            // Field projection off the RAW pointer: keeps the whole-allocation
            // provenance (a `&mut (*jp).task` reborrow would narrow it).
            let tptr: *mut Task = unsafe { core::ptr::addr_of_mut!((*jp).task) };
            pool.schedule(Batch::from(tptr));
        }

        // Quiesce: wait for every callback to finish. After this loop there is NO
        // further schedule() — the ONLY thing left is shutdown()+deinit().
        {
            let mut ran = ctx.ran.lock().unwrap();
            while *ran < n {
                let (g, to) = ctx.cv.wait_timeout(ran, Duration::from_secs(30)).unwrap();
                ran = g;
                assert!(!to.timed_out(), "only {}/{} tasks ran within 30s", *ran, n);
            }
        }

        pool.shutdown();
        pool.deinit(); // <- the multi-worker join under test

        assert_eq!(*ctx.ran.lock().unwrap(), n, "not all callbacks ran");
        let peak = ctx.peak.load(Ordering::SeqCst);
        // jobs + ctx reclaimed now — safe, deinit joined every worker.
        for jp in jobs {
            drop(unsafe { Box::from_raw(jp) });
        }
        drop(unsafe { Box::from_raw(ctx_ptr.cast_mut()) });
        peak
    }

    // REGRESSION GATE for multi-worker teardown (the `futex_wake` clamp
    // port-fidelity fix + the ledgered `Event::wake` deviation — see both notes
    // above + task-1-report.md addendum 2). RED evidence: with the unclamped
    // `futex_wake` (u32::MAX -> C int -1 -> kernel wakes at most ONE), this exact
    // correct-usage lifecycle DEADLOCKED ~100% of runs (watchdog abort; gdb:
    // >= 3 workers stranded parked on idle_event, `spawned` never reached 0,
    // `join()` hung). GREEN: with the clamp it passes deterministically (incl.
    // 400 lifecycles against the verbatim oracle Event, and with the deviation
    // applied). The watchdog stays so any regression FAILS loudly instead of
    // wedging the suite.
    #[test]
    fn multi_worker_quiesce_then_join_50x() {
        // Under Miri the whole pool runs interpreted (and the futex is the
        // spin/yield stand-in), so 50x64 would take hours. The shape under test
        // — real >= 2-worker concurrency, quiesce, then shutdown+join — is fully
        // exercised at the reduced counts; the NATIVE run keeps the full 50x64
        // regression gate unchanged.
        const LIFECYCLES: usize = if cfg!(miri) { 3 } else { 50 };
        const MAX_THREADS: u32 = 4;
        const TASKS: usize = if cfg!(miri) { 8 } else { 64 };

        // Watchdog: if the whole run wedges, abort LOUDLY so a deadlock fails the
        // suite instead of blocking it forever.
        let done = Arc::new(AtomicBool::new(false));
        let wd = {
            let done = done.clone();
            std::thread::spawn(move || {
                let start = Instant::now();
                while !done.load(Ordering::SeqCst) {
                    if start.elapsed() > Duration::from_secs(60) {
                        eprintln!(
                            "WATCHDOG: multi_worker_quiesce_then_join exceeded 60s -> \
                             DEADLOCK in correct quiesce-then-join teardown; aborting"
                        );
                        std::process::abort();
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            })
        };

        let mut max_peak = 0usize;
        for i in 0..LIFECYCLES {
            let peak = run_one_multi_lifecycle(MAX_THREADS, TASKS);
            if peak > max_peak {
                max_peak = peak;
            }
            let _ = i;
        }

        done.store(true, Ordering::SeqCst);
        wd.join().unwrap();

        // Prove we actually exercised real multi-worker parallelism (not a
        // degenerate single-worker run that would sidestep the strand race).
        assert!(
            max_peak >= 2,
            "expected >= 2 concurrent workers across {LIFECYCLES} lifecycles, peak was {max_peak}"
        );
    }
}
