//! epoll.rs — faithful port of libxev `backend/epoll.zig` (XEV_BLUEPRINT §1 Linux
//! port, task L1 LT3).
//!
//! Backend to use epoll. This is a SELF-CONTAINED whole backend, parallel to the
//! macOS `kqueue.rs`: it defines its own `Loop`/`Completion`/`Operation`/`Result`/
//! `Callback` locally and builds ONLY on the shared cross-platform data structures
//! (`heap`/`queue`) and the Linux primitives/enums (`linux::eventfd::Eventfd`,
//! `linux::{RunMode, CallbackAction, CompletionState, Options}`). It never imports
//! from any macOS-gated file (R5).
//
// PORT NOTE: the reference carries this warning verbatim (epoll.zig:27):
//   WARNING: this backend is a bit of a mess. It is missing features and in
//   general is in much poorer quality than all of the other backends. It
//   isn't meant to really be used yet. We should remodel this in the style of
//   the kqueue backend.
//
// PORT NOTE (the load-bearing disciplines, mirrored from kqueue.rs §2):
//  - RAW POINTERS end-to-end: the `Loop` holds `*mut Completion` into caller
//    memory; callbacks get `*mut Loop` + `*mut Completion`; loop/completion links
//    are NEVER `&mut`/lifetimes (Stacked-Borrows UB — memory
//    `rust-mut-args-strongly-protected`). Public `Loop` methods take `&mut self`
//    but every completion is touched through a raw `*mut Completion` popped from a
//    raw-ptr queue, and callbacks receive `self as *mut Loop` (raw), so a callback
//    may legally re-add / rearm a completion — exactly libxev's contract.
//  - `unsafe impl Send` for `Loop`/`Completion`: ghostty MOVES a `Loop` onto the
//    termio thread; single-thread affinity after the move makes this sound
//    (documented at each impl).
//  - hand-written `Default` for `Completion`: fn-ptr + raw-ptr fields.
//
// PORT NOTE (SCOPE, Task 3 vs Task 4): this task ports the loop CORE — `Loop`/
// `Completion`/`Operation`/`Result`, `submit`/`tick`/`run`, the timer heap, the
// internal eventfd wakeup, cancellation, and the single fd-registered op the L1
// LT3 tests exercise (`Operation::Read`, driven by the `epoll: timerfd` KAT). The
// remaining reference operations (accept/connect/poll/pread/write/pwrite/send/
// recv/sendmsg/recvmsg/close/shutdown) + the thread-pool path (`thread_schedule`/
// `thread_perform`, which needs the not-yet-ported Linux `ThreadPool` type) are
// Task 4's socket/process/stream scope; the `start`/`perform`/`fd` match arms are
// structured so Task 4 extends the `Operation` enum without reshaping the loop.
#![allow(dead_code)]
// PORT NOTE: `ms_per_s`/`ns_per_ms`/`ns_per_s`/`fd_t` keep their Zig/`std.posix`
// snake_case identifiers (R2 identifier-fidelity), and `ReadError::EOF` keeps the
// reference's acronym spelling — mirroring kqueue.rs's module-level allows.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(clippy::upper_case_acronyms)]

use core::mem::size_of;
use core::ptr;

use crate::heap;
use crate::linux::eventfd::Eventfd;
use crate::linux::net;
use crate::linux::{CallbackAction, CompletionState, Options, RunMode};
use crate::queue;
use crate::queue_mpsc;
use crate::thread_pool::{self, ThreadPool};
use crate::Error;
use core::any::TypeId;
use core::ffi::c_void;
use core::ptr::NonNull;

// PORT NOTE (Task 2): the reference `init(options: Options)` carries `Options`
// (entries + thread_pool). The epoll backend consumes exactly ONE field —
// `options.thread_pool` — for the blocking-op (`.close`, and File's pread/pwrite)
// thread-pool path (`thread_schedule`/`thread_perform`); `entries` is io_uring-only
// upstream (loop.zig:15 "Backends: io_uring"). With the Linux `ThreadPool` body now
// ported (Task 1) and un-gated for Linux, `Loop::init` takes `Options` (matching
// `io_uring::Loop::init` + the oracle) and stores the pool.

// PORT NOTE: `posix.fd_t` — the C file descriptor int.
pub type fd_t = libc::c_int;
/// PORT NOTE: `posix.socket_t` — the socket handle (a C int on Linux).
pub type socket_t = libc::c_int;
/// PORT NOTE: `posix.pid_t` — the process id.
pub type pid_t = libc::pid_t;

/// PORT NOTE: `posix.SOCK.CLOEXEC` — the accept-op `flags` default (close-on-exec
/// on the accepted socket, via `accept4`).
pub const SOCK_CLOEXEC: u32 = libc::SOCK_CLOEXEC as u32;

// PORT NOTE: `watcher/common.zig`'s `userdataValue` — the typed→untyped userdata
// bridge the epoll `Process`/`Stream` trampolines use. Kept local so this backend
// stays self-contained (the crate's `common.rs` is macOS-gated, R5). Zig `void`
// userdata ⇒ a null pointer; Rust's stand-in is the zero-sized unit `()`.
#[inline]
fn userdata_value<UD: 'static>(v: *mut c_void) -> *mut UD {
    if TypeId::of::<UD>() == TypeId::of::<()>() {
        return ptr::null_mut();
    }
    v as *mut UD
}
/// PORT NOTE: `posix.timespec`/`std.os.linux.timespec` — the monotonic-clock
/// instant the loop caches + the timer-heap key. Linux `timespec`
/// (`{ tv_sec: i64, tv_nsec: i64 }` on aarch64), from `libc`; Zig `.sec`/`.nsec`
/// map to `.tv_sec`/`.tv_nsec`.
pub type Timespec = libc::timespec;

// PORT NOTE: `std.time.*` constants used by `now`/`update_now`/timer math.
const ms_per_s: u64 = 1_000;
const ns_per_ms: u64 = 1_000_000;
const ns_per_s: u64 = 1_000_000_000;

/// True if epoll is available on this platform.
//
// PORT NOTE: the reference `builtin.os.tag == .linux`; this module compiles only
// on Linux (`#[cfg(target_os = "linux")]` in lib.rs), so it is a constant `true`.
pub fn available() -> bool {
    true
}

/// The callback of the main Loop operations.
//
// PORT NOTE: `loop.zig`'s `Callback(@This())` monomorphized for the epoll backend
// — a concrete fn-pointer referencing THIS module's `Loop`/`Completion`/`Result`
// (self-contained, per the brief). `loop_`/`completion` are RAW pointers (`*mut`),
// never `&mut`/lifetimes (the loop re-enters them across ticks). A plain Rust `fn`
// pointer preserves the Zig zero-cost fn-pointer dispatch.
pub type Callback = fn(
    userdata: *mut core::ffi::c_void,
    loop_: *mut Loop,
    completion: *mut Completion,
    result: Result,
) -> CallbackAction;

/// A callback that does nothing and immediately disarms. This implements the
/// epoll `Callback` and is the default value for completions.
//
// PORT NOTE: `loop.zig`'s `NoopCallback(@This())` monomorphized for epoll.
pub fn noop_callback(
    _: *mut core::ffi::c_void,
    _: *mut Loop,
    _: *mut Completion,
    _: Result,
) -> CallbackAction {
    CallbackAction::Disarm
}

// PORT NOTE: `const errno` reader — `posix.errno(rc)` reads the C `errno` global.
#[inline]
fn errno() -> i32 {
    // SAFETY: `__errno_location()` returns the thread-local errno location.
    unsafe { *libc::__errno_location() }
}

// ---------------------------------------------------------------------------
// The Loop (epoll.zig:31).
// ---------------------------------------------------------------------------

// PORT NOTE: `const TimerHeap = heap.Intrusive(Operation.Timer, void, Operation.Timer.less);`
type TimerHeap = heap::Intrusive<Timer, ()>;
// PORT NOTE: `const TaskCompletionQueue = queue_mpsc.Intrusive(Completion);`
type TaskCompletionQueue = queue_mpsc::Intrusive<Completion>;

/// PORT NOTE: `pub const Loop = struct { ... }`.
pub struct Loop {
    /// The epoll fd.
    fd: fd_t,

    /// The eventfd that this epoll queue always has a filter for. Writing
    /// an empty message to this eventfd can be used to wake up the loop
    /// at any time. Waking up the loop via this eventfd won't trigger any
    /// particular completion, it just forces tick to cycle.
    //
    // PORT NOTE: the reference field type is the `Async`(eventfd) watcher; the
    // Task-2 `linux::eventfd::Eventfd` primitive IS that wakeup handle.
    eventfd: Eventfd,

    /// The number of active completions. This DOES NOT include completions that
    /// are queued in the submissions queue.
    active: usize,

    /// Our queue of submissions that we want to enqueue on the next tick.
    submissions: queue::Intrusive<Completion>,

    /// The queue for completions to delete from the epoll fd.
    deletions: queue::Intrusive<Completion>,

    /// Heap of timers.
    timers: TimerHeap,

    /// The thread pool to use for blocking operations that epoll can't do.
    thread_pool: Option<*mut ThreadPool>,

    /// The MPSC queue for completed completions from the thread pool.
    //
    // PORT NOTE [mirrors kqueue.rs's LIFETIMES-AUDIT M1+M2 fix, proven on
    // `.port/phase-2/mirihar-xev-mpsc`]: the Zig field is inline
    // (`thread_pool_completions: TaskCompletionQueue`), but that queue is
    // SELF-REFERENTIAL — its `head`/`tail` point at the embedded `stub`, an address
    // INSIDE this queue. `init` writes those stub self-pointers ONCE and `pop`/`push`
    // re-deref them on every later tick. If the queue lived inline in `Loop`, each
    // `tick(&mut self)`/`run(&mut self)` whole-`Loop` `&mut` reborrow would
    // INVALIDATE the provenance the stub self-pointers were minted with, making the
    // later deref Stacked/Tree-Borrows UB. Giving the queue its OWN heap allocation
    // (a `Box`) launders it: `tick`'s `&mut self` reborrows only the `Loop`
    // allocation, never the queue's separate heap allocation, so the stub
    // self-pointers survive across ticks. The Zig `*Self` method contract on
    // `TaskCompletionQueue` is preserved verbatim; only the storage is heap-rooted.
    // `thread_pool_completions_ptr` caches the stable raw handle established at
    // `init` (from the `Box`), and every post-init access (`pop` in tick_inner,
    // `push` in thread_perform) routes through THAT cache — the `Box` itself is never
    // reborrowed again — so no `&mut`/`Box`-deref retag ever pops the queue's
    // internal-pointer lineage (mirihar `boxq` + `xthread_boxq`: CLEAN under BOTH SB
    // and TB, single- and cross-thread). Symmetric with kqueue.rs (DEFERRALS
    // 4396-4398).
    thread_pool_completions: Box<TaskCompletionQueue>,

    /// Stable raw handle to `*thread_pool_completions`, cached at first-tick `init`
    /// from the `Box` (null until then). The ONLY handle used post-init by
    /// `pop`/`push` so the self-referential queue's stub pointers are never
    /// invalidated by a `&mut self` reborrow. See the field PORT NOTE above.
    thread_pool_completions_ptr: *mut TaskCompletionQueue,

    /// Cached time.
    cached_now: Timespec,

    /// Some internal fields we can pack for better space.
    flags: LoopFlags,
}

/// PORT NOTE: the `flags: packed struct { init, in_run, stopped }` sub-struct.
#[derive(Clone, Copy, Default)]
struct LoopFlags {
    /// True once it is initialized.
    init: bool,
    /// Whether we're in a run or not (to prevent nested runs).
    in_run: bool,
    /// Whether our loop is in a stopped state or not.
    stopped: bool,
}

// PORT NOTE: `Loop` holds raw-ptr intrusive queues/heap + an fd + a `*mut
// ThreadPool` → auto-`!Send`; ghostty MOVES a `Loop` onto the spawned termio
// thread. SAFETY / single-thread affinity: after the move a `Loop` (and every
// `Completion` it links) is touched by exactly one thread at a time — the owning
// event-loop thread — so transferring ownership is sound. The `thread_pool` raw
// pointer targets a `ThreadPool` the caller keeps alive across the loop's life;
// the pool itself is `Sync + Send` (its worker threads run concurrently by design),
// so the only cross-thread channel — a worker running `thread_perform` and pushing
// onto `thread_pool_completions` — rides the lock-free MPSC queue (through the
// stable raw handle) + the `Eventfd` wakeup (a plain `Copy` fd), BOTH read out of
// the caller-owned `Completion` (`task_completions`/`task_wakeup`), so the worker
// never references — nor even ACCESSES — the `Loop` allocation (WS19 T10).
unsafe impl Send for Loop {}

impl Loop {
    /// Initialize a new epoll-backed event loop.
    //
    // PORT NOTE: reference `init(options: Options)`. The epoll backend reads the
    // one field it consumes — `options.thread_pool` — and stores it for the
    // blocking-op path; `entries` is io_uring-only upstream. `Async.init()` →
    // `Eventfd::init()`.
    pub fn init(options: Options) -> core::result::Result<Loop, Error> {
        let eventfd = Eventfd::init().map_err(|_| Error)?;
        // errdefer eventfd.deinit();
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 {
            eventfd.deinit();
            return Err(Error);
        }

        let mut res: Loop = Loop {
            fd,
            eventfd,
            active: 0,
            submissions: queue::Intrusive::default(),
            deletions: queue::Intrusive::default(),
            timers: TimerHeap::new((), Timer::less),
            thread_pool: options.thread_pool,
            // .thread_pool_completions = undefined — Rust has no undefined; place a
            // valid stub queue (never `init`'d/used while thread_pool is None).
            // Heap-rooted (Box) so its self-referential stub pointers survive the
            // per-tick `&mut self` reborrows (see the field PORT NOTE).
            thread_pool_completions: Box::new(TaskCompletionQueue::new(Completion::default())),
            // Cached at first-tick `init` from the `Box`; null until then. Never
            // dereferenced while `thread_pool` is None.
            thread_pool_completions_ptr: ptr::null_mut(),
            // .cached_now = undefined — populated by update_now() below.
            cached_now: unsafe { core::mem::zeroed() },
            flags: LoopFlags::default(),
        };
        res.update_now();
        Ok(res)
    }

    /// Deinitialize the loop, this closes the epoll fd and the eventfd.
    pub fn deinit(&mut self) {
        // SAFETY: closing our own epoll fd; eventfd owns its own fd.
        unsafe { libc::close(self.fd) };
        self.eventfd.deinit();
    }

    /// Run the event loop. See RunMode documentation for details on modes.
    /// Once the loop is run, the pointer MUST remain stable.
    pub fn run(&mut self, mode: RunMode) -> core::result::Result<(), Error> {
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
    }

    fn done(&self) -> bool {
        self.flags.stopped || (self.active == 0 && self.submissions.empty())
    }

    /// Stop the loop. This can only be called from the main thread.
    /// This will stop the loop forever. Future ticks will do nothing.
    pub fn stop(&mut self) {
        self.flags.stopped = true;
    }

    /// Returns true if the loop is stopped. This may mean there
    /// are still pending completions to be processed.
    pub fn stopped(&self) -> bool {
        self.flags.stopped
    }

    /// Add a completion to the loop.
    ///
    /// # Safety
    /// `completion` must be a valid, stable pointer to a `Completion` that lives
    /// until its callback fires (libxev's intrusive-ownership contract).
    pub unsafe fn add(&mut self, completion: *mut Completion) {
        match unsafe { (*completion).flags.state } {
            // Already adding, forget about it.
            State::Adding => return,

            // If it is dead we're good. If we're deleting we'll ignore it
            // while we're processing.
            State::Dead | State::Deleting => {}

            State::Active => unreachable!(),
        }
        unsafe { (*completion).flags.state = State::Adding };

        // We just add the completion to the queue. Failures can happen
        // at tick time...
        unsafe { self.submissions.push(completion) };
    }

    /// Delete a completion from the loop.
    ///
    /// # Safety
    /// `completion` must be a valid, stable `Completion` pointer.
    pub unsafe fn delete(&mut self, completion: *mut Completion) {
        match unsafe { (*completion).flags.state } {
            // Already deleted
            State::Deleting => return,

            // If we're active then we will stop it and remove from epoll.
            // If we're adding then we'll ignore it when adding.
            State::Dead | State::Active | State::Adding => {}
        }
        unsafe { (*completion).flags.state = State::Deleting };

        unsafe { self.deletions.push(completion) };
    }

    /// Returns the "loop" time in milliseconds. The loop time is updated
    /// once per loop tick, before IO polling occurs. It remains constant
    /// throughout callback execution.
    ///
    /// You can force an update of the "now" value by calling update_now()
    /// at any time from the main thread.
    pub fn now(&self) -> i64 {
        // If anything overflows we just return the max value.
        let max = i64::MAX;

        // Calculate all the values, being careful about overflows in order
        // to just return the maximum value.
        let sec = match (self.cached_now.tv_sec as isize).checked_mul(ms_per_s as isize) {
            Some(v) => v,
            None => return max,
        };
        let nsec = (self.cached_now.tv_nsec as isize).div_euclid(ns_per_ms as isize);
        // std.math.lossyCast(i64, sec +| nsec)
        (sec.saturating_add(nsec)) as i64
    }

    /// Update the cached time.
    pub fn update_now(&mut self) {
        // if (posix.clock_gettime(posix.CLOCK.MONOTONIC)) |new_time| ... else |_| {}
        let mut new_time: Timespec = unsafe { core::mem::zeroed() };
        // SAFETY: clock_gettime writes a `timespec` into `new_time`.
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut new_time) } == 0 {
            self.cached_now = new_time;
        } else {
            // Errors are ignored.
        }
    }

    /// Add a timer to the loop. The timer will execute in "next_ms". This
    /// is oneshot: the timer will not repeat.
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer that outlives the timer.
    pub unsafe fn timer(
        &mut self,
        c: *mut Completion,
        next_ms: u64,
        userdata: *mut core::ffi::c_void,
        cb: Callback,
    ) {
        unsafe {
            *c = Completion {
                op: Operation::Timer(Timer {
                    next: self.timer_next(next_ms),
                    reset: None,
                    heap: heap::IntrusiveField::default(),
                    c: ptr::null_mut(),
                }),
                userdata,
                callback: cb,
                ..Completion::default()
            };

            self.add(c);
        }
    }

    /// See io_uring.timer_reset for docs.
    ///
    /// # Safety
    /// `c`/`c_cancel` must be valid, stable `Completion` pointers.
    pub unsafe fn timer_reset(
        &mut self,
        c: *mut Completion,
        c_cancel: *mut Completion,
        next_ms: u64,
        userdata: *mut core::ffi::c_void,
        cb: Callback,
    ) {
        match unsafe { (*c).flags.state } {
            State::Dead | State::Deleting => {
                unsafe { self.timer(c, next_ms, userdata, cb) };
            }

            // Adding state we can just modify the metadata and return
            // since the timer isn't in the heap yet.
            State::Adding => unsafe {
                match &mut (*c).op {
                    Operation::Timer(t) => t.next = self.timer_next(next_ms),
                    _ => unreachable!(),
                }
                (*c).userdata = userdata;
                (*c).callback = cb;
            },

            State::Active => unsafe {
                // Update the reset time for the timer to the desired time
                // along with all the callbacks.
                let reset = self.timer_next(next_ms);
                match &mut (*c).op {
                    Operation::Timer(t) => t.reset = Some(reset),
                    _ => unreachable!(),
                }
                (*c).userdata = userdata;
                (*c).callback = cb;

                // If the cancellation is active, we assume its for this timer
                // and do nothing.
                if Completion::state(c_cancel) == CompletionState::Active {
                    return;
                }
                assert!(
                    Completion::state(c_cancel) == CompletionState::Dead
                        && Completion::state(c) == CompletionState::Active
                );
                *c_cancel = Completion {
                    op: Operation::Cancel { c },
                    ..Completion::default()
                };
                self.add(c_cancel);
            },
        }
    }

    fn timer_next(&self, next_ms: u64) -> Timespec {
        // Get the timestamp of the absolute time that we'll execute this timer.
        // There are lots of failure scenarios here in math. If we see any
        // of them we just use the maximum value.
        let max: Timespec = {
            let mut ts: Timespec = unsafe { core::mem::zeroed() };
            ts.tv_sec = isize::MAX as libc::time_t;
            ts.tv_nsec = isize::MAX as _;
            ts
        };

        let next_s: isize = match isize::try_from(next_ms / ms_per_s) {
            Ok(v) => v,
            Err(_) => return max,
        };
        let next_ns: isize = match isize::try_from((next_ms % ms_per_s) * ns_per_ms) {
            Ok(v) => v,
            Err(_) => return max,
        };

        let mut ts: Timespec = unsafe { core::mem::zeroed() };
        ts.tv_sec = match (self.cached_now.tv_sec as isize).checked_add(next_s) {
            Some(v) => v as libc::time_t,
            None => return max,
        };
        ts.tv_nsec = match (self.cached_now.tv_nsec as isize).checked_add(next_ns) {
            Some(v) => v as _,
            None => return max,
        };
        ts
    }

    /// Tick through the event loop once, waiting for at least "wait" completions
    /// to be processed by the loop itself.
    pub fn tick(&mut self, wait: u32) -> core::result::Result<(), Error> {
        // If we're stopped then the loop is fully over.
        if self.flags.stopped {
            return Ok(());
        }

        // We can't nest runs.
        if self.flags.in_run {
            return Err(Error); // error.NestedRunsNotAllowed
        }
        self.flags.in_run = true;
        // defer self.flags.in_run = false; — via the guard below.
        let result = self.tick_inner(wait);
        self.flags.in_run = false;
        result
    }

    fn tick_inner(&mut self, wait: u32) -> core::result::Result<(), Error> {
        let self_ptr: *mut Loop = self;

        // Initialize
        if !self.flags.init {
            self.flags.init = true;

            if self.thread_pool.is_some() {
                // Establish the stable heap-rooted raw handle ONCE, then init the
                // self-referential queue THROUGH it, so the stub self-pointers it
                // writes carry the queue's own (Box) allocation provenance — not a
                // per-tick `&mut self` tag that the next tick's reborrow would pop.
                // (mirrors kqueue.rs's LIFETIMES-AUDIT M2 fix; mirihar `boxq`.)
                unsafe {
                    let qp: *mut TaskCompletionQueue =
                        &raw mut *(*self_ptr).thread_pool_completions;
                    ptr::addr_of_mut!((*self_ptr).thread_pool_completions_ptr).write(qp);
                    TaskCompletionQueue::init(qp);
                }
            }

            let mut ev = libc::epoll_event {
                events: (libc::EPOLLIN | libc::EPOLLRDHUP) as u32,
                u64: self.eventfd.fd as u64,
            };
            // SAFETY: register the always-on wakeup eventfd with our epoll fd.
            let rc =
                unsafe { libc::epoll_ctl(self.fd, libc::EPOLL_CTL_ADD, self.eventfd.fd, &mut ev) };
            if rc < 0 {
                // We reset initialization because we can't do anything
                // safely unless we get this eventfd registered!
                self.flags.init = false;
                return Err(Error);
            }
        }

        // Submit all the submissions. We copy the submission queue so that
        // any resubmits don't cause an infinite loop.
        let mut wait_rem: usize = wait as usize;
        let mut queued = core::mem::take(&mut self.submissions);
        // self.submissions = .{};  (done by mem::take)
        loop {
            let c = unsafe { queued.pop() };
            if c.is_null() {
                break;
            }
            // We ignore any completions that aren't in the adding state.
            // This usually means that we switched them to be deleted or
            // something.
            if unsafe { (*c).flags.state } != State::Adding {
                continue;
            }

            // These operations happen synchronously. Ensure they are
            // decremented from wait_rem.
            match unsafe { &(*c).op } {
                Operation::Cancel { .. } | Operation::Shutdown { .. } | Operation::Timer(_) => {
                    wait_rem = wait_rem.saturating_sub(1);
                }
                _ => {}
            }

            unsafe { self.start(c) };
        }

        // Handle all deletions so we don't wait for them.
        loop {
            let c = unsafe { self.deletions.pop() };
            if c.is_null() {
                break;
            }
            if unsafe { (*c).flags.state } != State::Deleting {
                continue;
            }
            unsafe { self.stop_completion(c) };
        }

        // If we have no active handles then we return no matter what.
        if self.active == 0 {
            // We still have to update our concept of "now".
            self.update_now();
            return Ok(());
        }

        // Wait and process events. We only do this if we have any active.
        let mut events: [libc::epoll_event; 1024] = unsafe { core::mem::zeroed() };
        while self.active > 0 && (wait == 0 || wait_rem > 0) {
            self.update_now();
            let mut now_timer = Timer {
                next: self.cached_now,
                reset: None,
                heap: heap::IntrusiveField::default(),
                c: ptr::null_mut(),
            };

            // Run our expired timers
            loop {
                let t = self.timers.peek();
                if t.is_null() {
                    break;
                }
                if !Timer::less((), t, &mut now_timer) {
                    break;
                }

                // Remove the timer
                assert!(unsafe { self.timers.delete_min() } == t);

                // Mark completion as done
                let c = unsafe { (*t).c };
                unsafe { (*c).flags.state = State::Dead };
                self.active -= 1;

                // Lower our remaining count
                wait_rem = wait_rem.saturating_sub(1);

                // Invoke
                let cb = unsafe { (*c).callback };
                let ud = unsafe { (*c).userdata };
                let action = cb(ud, self_ptr, c, Result::Timer(Ok(TimerTrigger::Expiration)));
                match action {
                    CallbackAction::Disarm => {}
                    CallbackAction::Rearm => unsafe { self.start(c) },
                }
            }

            // Run our completed thread pool work
            if self.thread_pool.is_some() {
                loop {
                    // Drain through the cached stable raw handle (never a fresh
                    // `&mut self.thread_pool_completions`, which would pop the
                    // queue's stub-pointer lineage). Mirrors kqueue.rs; mirihar
                    // `boxq`.
                    let c = unsafe {
                        TaskCompletionQueue::pop((*self_ptr).thread_pool_completions_ptr)
                    };
                    if c.is_null() {
                        break;
                    }

                    // Mark completion as done
                    unsafe { (*c).flags.state = State::Dead };
                    self.active -= 1;

                    // Lower our remaining count
                    wait_rem = wait_rem.saturating_sub(1);

                    // Invoke
                    let cb = unsafe { (*c).callback };
                    let ud = unsafe { (*c).userdata };
                    let tr = unsafe { (*c).task_result };
                    let action = cb(ud, self_ptr, c, tr);
                    match action {
                        CallbackAction::Disarm => {}
                        CallbackAction::Rearm => unsafe { self.start(c) },
                    }
                }
            }

            // Determine our next timeout based on the timers
            let timeout: i32 = if wait_rem == 0 {
                0
            } else {
                // If we have a timer, we want to set the timeout to our next
                // timer value. If we have no timer, we wait forever.
                let t = self.timers.peek();
                if t.is_null() {
                    -1
                } else {
                    // Determine the time in milliseconds.
                    let ms_now = (self.cached_now.tv_sec as u64) * ms_per_s
                        + (self.cached_now.tv_nsec as u64) / ns_per_ms;
                    let ms_next = (unsafe { (*t).next.tv_sec } as u64) * ms_per_s
                        + (unsafe { (*t).next.tv_nsec } as u64) / ns_per_ms;
                    // CARRY-FORWARD FIX (T3 review): the reference `@intCast(ms_next
                    // -| ms_now)` truncates; a plain `as i32` on a >24.8-day wait
                    // (u64 diff > i32::MAX ms) would WRAP NEGATIVE, turning the
                    // epoll_wait timeout into a spurious immediate/forever value.
                    // Clamp to i32::MAX (epoll_wait caps at ~24.8 days anyway).
                    i32::try_from(ms_next.saturating_sub(ms_now)).unwrap_or(i32::MAX)
                }
            };

            // SAFETY: epoll_wait fills `events[0..n]`; on -1 we inspect errno.
            let n = unsafe {
                libc::epoll_wait(
                    self.fd,
                    events.as_mut_ptr(),
                    events.len() as libc::c_int,
                    timeout,
                )
            };
            if n < 0 {
                match errno() {
                    libc::EINTR => continue,
                    _ => return Err(Error),
                }
            }
            let n = n as usize;

            // Process all our events and invoke their completion handlers
            for ev in &events[0..n] {
                // Copy out of the (potentially packed) epoll_event field.
                let data = ev.u64;

                // Handle wakeup eventfd
                if data == self.eventfd.fd as u64 {
                    // Drain the coalesced counter; ignore errors (WouldBlock).
                    let _ = self.eventfd.drain();
                    continue;
                }

                let c: *mut Completion = data as *mut Completion;

                // We get the fd and mark this as in progress we can properly
                // clean this up later.
                let fd = if unsafe { (*c).flags.dup } {
                    Some(unsafe { (*c).flags.dup_fd })
                } else {
                    unsafe { Completion::fd(c) }
                };
                let close_dup = unsafe { (*c).flags.dup };
                unsafe { (*c).flags.state = State::Dead };

                let res = unsafe { Completion::perform(c, Some(ev)) };
                let cb = unsafe { (*c).callback };
                let ud = unsafe { (*c).userdata };
                let action = cb(ud, self_ptr, c, res);
                match action {
                    CallbackAction::Disarm => {
                        // We can't use self.stop because we can't trust
                        // that c is still a valid pointer.
                        if let Some(v) = fd {
                            // SAFETY: DEL on a fd we previously ADDed; never fails.
                            unsafe {
                                libc::epoll_ctl(self.fd, libc::EPOLL_CTL_DEL, v, ptr::null_mut())
                            };

                            if close_dup {
                                unsafe { libc::close(v) };
                            }
                        }

                        self.active -= 1;
                    }

                    // For epoll, epoll remains armed by default. We have to
                    // reset the state, that is all.
                    CallbackAction::Rearm => unsafe { (*c).flags.state = State::Active },
                }
            }

            if wait == 0 {
                break;
            }
            wait_rem = wait_rem.saturating_sub(n);
        }

        Ok(())
    }

    /// Schedule a completion to run on a thread.
    ///
    /// PORT NOTE: `fn thread_schedule(self: *Loop, c: *Completion) !void`
    /// (epoll.zig:483). Returns `Err(())` for the reference's
    /// `error.ThreadPoolRequired` when no pool is attached; the caller maps that to
    /// the op's `ThreadPoolRequired` result. Mirrors kqueue.rs's threadpool
    /// `StartAction` idioms, including the per-completion `task_completions` field
    /// the oracle sets (restored in WS19 T10) plus the port-added `task_wakeup`, so
    /// that `thread_perform` never accesses the `Loop` allocation and the
    /// self-referential MPSC queue's stub pointers are never invalidated.
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer owned by this loop.
    unsafe fn thread_schedule(&mut self, c: *mut Completion) -> core::result::Result<(), ()> {
        // const pool = self.thread_pool orelse return error.ThreadPoolRequired;
        let pool = match self.thread_pool {
            Some(p) => p,
            None => return Err(()),
        };

        unsafe {
            // Setup our completion state so that thread_perform can do stuff.
            (*c).task_loop = self;
            // WS19 T10: copy the two handles `thread_perform` needs INTO the
            // completion, so the pool worker never touches the `Loop` allocation
            // (which the loop thread holds a protected `&mut` to for the whole
            // tick). Both reads happen here, on the loop thread;
            // `thread_pool_completions_ptr` was established in the `tick_inner`
            // init arm. Symmetric with kqueue.rs.
            (*c).task_completions = self.thread_pool_completions_ptr;
            (*c).task_wakeup = self.eventfd;
            (*c).task = thread_pool::Task {
                callback: Loop::thread_perform,
                ..thread_pool::Task::default()
            };

            // We need to mark this completion as active before we schedule.
            (*c).flags.state = State::Active;
            self.active += 1;

            // Schedule it, from this point forward its not safe to touch c.
            (*pool).schedule(thread_pool::Batch::from(ptr::addr_of_mut!((*c).task)));
        }

        Ok(())
    }

    /// This is the main callback for the threadpool to perform work
    /// on completions for the loop.
    //
    // PORT NOTE: `fn thread_perform(t: *ThreadPool.Task) void` (epoll.zig:501).
    // `@fieldParentPtr("task", t)` recovers the owning Completion via `container_of`
    // (offset-of). The result is stored on `task_result`, then the completion is
    // pushed onto the loop's MPSC queue through the stable raw handle the loop
    // thread copied into THIS COMPLETION (`task_completions`, the reference's own
    // field), never a fresh `&mut …thread_pool_completions`: this runs ON A POOL
    // THREAD, so a unique `&mut` reborrow of the queue (or of the owning `Loop`)
    // formed here would be UB regardless of the atomics inside, AND would pop the
    // queue's stub-pointer lineage (mirrors kqueue.rs; mirihar `xthread_boxq`:
    // CLEAN under SB + TB).
    //
    // The final wakeup applies the SAME invariant, through `task_wakeup`. REVISED
    // WS19 T10: the previous shape here read `(*(*c).task_loop).eventfd` out by
    // value, on the reasoning that a `Copy` read forms no reference. The mirihar
    // `wakeup_copied` candidate DISPROVED that as sufficient — the loop thread
    // holds a PROTECTED `&mut Loop` for the whole tick, and a by-value read is
    // still a foreign ACCESS on the `Loop` allocation, hence SB UB. Only touching
    // the caller-owned `Completion` allocation is clean under both models
    // (`wakeup_completion_cached`). Both handles are therefore copied in at
    // schedule time and `task_loop` is no longer dereferenced here at all — which
    // ALSO discharges the `task_loop` lineage hazard (`(*c).task_loop = self` is
    // minted from one tick's `&mut self` and popped by the next tick's entry
    // retag; mirihar `taskloop_lineage_naive`: UB under SB).
    fn thread_perform(t: *mut thread_pool::Task) {
        // const c: *Completion = @fieldParentPtr("task", t);
        let c: *mut Completion = unsafe {
            (t as *mut u8).sub(core::mem::offset_of!(Completion, task)) as *mut Completion
        };

        // Do our task
        unsafe { (*c).task_result = Completion::perform(c, None) };

        // Add to our completion queue
        unsafe { TaskCompletionQueue::push((*c).task_completions, c) };

        // Wake up our main loop.
        unsafe {
            let _ = (*c).task_wakeup.notify();
        }
    }

    /// Sends an empty message to this loop's eventfd so that it wakes up.
    fn wakeup(&mut self) -> core::result::Result<(), Error> {
        self.eventfd.notify().map_err(|_| Error)
    }

    /// Register `completion`'s fd with epoll for the given `events`, dup-ing the
    /// fd first if the completion requests it. Shared by the socket/poll `start`
    /// arms (the reference inlines `fd_maybe_dup` + `epoll_ctl(ADD)` in each).
    ///
    /// # Safety
    /// `completion` must be a valid, stable `Completion` pointer whose op has an fd.
    unsafe fn epoll_register(
        &mut self,
        completion: *mut Completion,
        events: u32,
    ) -> core::result::Result<(), ()> {
        let fd = unsafe { Completion::fd_maybe_dup(completion) }?;
        let mut ev = libc::epoll_event {
            events,
            u64: completion as u64,
        };
        // SAFETY: register `fd` with our epoll fd, keyed on the completion pointer.
        let rc = unsafe { libc::epoll_ctl(self.fd, libc::EPOLL_CTL_ADD, fd, &mut ev) };
        if rc < 0 {
            Err(())
        } else {
            Ok(())
        }
    }

    /// Start the completion.
    ///
    /// # Safety
    /// `completion` must be a valid, stable `Completion` pointer owned by this
    /// loop and in the `Adding` state.
    unsafe fn start(&mut self, completion: *mut Completion) {
        let self_ptr: *mut Loop = self;

        // res_ is Some(result) if the op completed synchronously (an immediate
        // result to hand to the callback), or None if it registered with epoll
        // (or the timer heap) and we should mark it active.
        let res_: Option<Result> = match unsafe { &(*completion).op } {
            Operation::Noop => {
                unsafe { (*completion).flags.state = State::Dead };
                return;
            }

            Operation::Cancel { c } => {
                let target = *c;
                // We stop immediately. We only stop if we are in the
                // "adding" state because cancellation or any other action
                // means we're complete already.
                if unsafe { (*completion).flags.state } == State::Adding {
                    if matches!(unsafe { &(*target).op }, Operation::Cancel { .. }) {
                        panic!("cannot cancel a cancellation");
                    }
                    unsafe { self.stop_completion(target) };
                }

                // We always run cancellations.
                Some(Result::Cancel(Ok(())))
            }

            // PORT NOTE: the reference `.read` arm opens with `if
            // (completion.flags.threadpool) { if (self.thread_schedule(completion))
            // |_| return else |err| break :res .{ .read = err }; }` (epoll.zig:588)
            // — a threadpool-flagged read (File's, epoll arm) is a BLOCKING op that
            // rides the pool, exactly like `.close`/`.pread`/`.pwrite`. With no pool
            // attached `thread_schedule` yields `ThreadPoolRequired`. An un-flagged
            // read (GenericStream/TCP, which set `dup` not `threadpool`) registers
            // with epoll below.
            Operation::Read { .. } => {
                if unsafe { (*completion).flags.threadpool } {
                    match unsafe { self.thread_schedule(completion) } {
                        Ok(()) => return,
                        Err(()) => Some(Result::Read(Err(ReadError::ThreadPoolRequired))),
                    }
                } else {
                    match unsafe { Completion::fd_maybe_dup(completion) } {
                        // DupFailed — surfaced as an immediate read error.
                        Err(()) => Some(Result::Read(Err(ReadError::Unexpected))),
                        Ok(reg_fd) => {
                            let mut ev = libc::epoll_event {
                                events: (libc::EPOLLIN | libc::EPOLLRDHUP) as u32,
                                u64: completion as u64,
                            };
                            // SAFETY: register `reg_fd` for read readiness with our epoll fd.
                            let rc = unsafe {
                                libc::epoll_ctl(self.fd, libc::EPOLL_CTL_ADD, reg_fd, &mut ev)
                            };
                            if rc < 0 {
                                Some(Result::Read(Err(ReadError::Unexpected)))
                            } else {
                                None
                            }
                        }
                    }
                }
            }

            // PORT NOTE: the `.pread` arm (epoll.zig:608) is byte-identical to
            // `.read` — the same threadpool gate then the same IN|RDHUP epoll
            // registration; only the perform-side syscall differs (`posix.pread`
            // with the op's `offset`). File sets `threadpool = true` on pread
            // (file.zig:131), so in practice the pool path is always taken here.
            // REFERENCE-QUIRK PORT NOTE: the oracle tags THIS start-arm's errors
            // `.read` even though the op is `.pread` (epoll.zig:608 — an upstream
            // tagged-union slip; the PERFORM side at epoll.zig:992 tags `.pread`
            // correctly). We deliberately tag `Result::PRead` here: the strict
            // pread trampoline matches on `Result::PRead` and would hit
            // `unreachable!` on a `Result::Read`.
            Operation::PRead { .. } => {
                if unsafe { (*completion).flags.threadpool } {
                    match unsafe { self.thread_schedule(completion) } {
                        Ok(()) => return,
                        Err(()) => Some(Result::PRead(Err(ReadError::ThreadPoolRequired))),
                    }
                } else {
                    match unsafe { Completion::fd_maybe_dup(completion) } {
                        Err(()) => Some(Result::PRead(Err(ReadError::Unexpected))),
                        Ok(reg_fd) => {
                            let mut ev = libc::epoll_event {
                                events: (libc::EPOLLIN | libc::EPOLLRDHUP) as u32,
                                u64: completion as u64,
                            };
                            // SAFETY: register `reg_fd` for read readiness with our epoll fd.
                            let rc = unsafe {
                                libc::epoll_ctl(self.fd, libc::EPOLL_CTL_ADD, reg_fd, &mut ev)
                            };
                            if rc < 0 {
                                Some(Result::PRead(Err(ReadError::Unexpected)))
                            } else {
                                None
                            }
                        }
                    }
                }
            }

            Operation::Timer(_) => {
                // Point back to completion since we need this.
                let v_ptr: *mut Timer = match unsafe { &mut (*completion).op } {
                    Operation::Timer(t) => {
                        t.c = completion;
                        t
                    }
                    _ => unreachable!(),
                };

                // Insert the timer into our heap.
                unsafe { self.timers.insert(v_ptr) };

                // We always run timers.
                None
            }

            // PORT NOTE: the socket-op `start` arms register the fd with epoll for
            // the readiness the op needs (IN for read-family, OUT for write-family)
            // and defer the actual syscall to `perform` when the fd fires. The
            // reference `fd_maybe_dup catch DupFailed` maps to the op's `Unexpected`
            // (the T3 read arm's established mapping — accept/connect/poll/send/recv
            // never dup, and the dup-setting Stream read/write map dup-fail the same
            // way).
            Operation::Accept { .. } => {
                match unsafe { self.epoll_register(completion, libc::EPOLLIN as u32) } {
                    Ok(()) => None,
                    Err(()) => Some(Result::Accept(Err(AcceptError::Unexpected))),
                }
            }

            Operation::Connect { socket: _, addr } => {
                let addr_copy = *addr;
                match unsafe { Completion::fd_maybe_dup(completion) } {
                    Err(()) => Some(Result::Connect(Err(ConnectError::Unexpected))),
                    Ok(fd) => {
                        // SAFETY: connect on the (possibly dup'd) socket fd; `addr`
                        // is a valid `Address` held by value in the op.
                        let rc = unsafe {
                            libc::connect(
                                fd,
                                ptr::addr_of!(addr_copy) as *const libc::sockaddr,
                                addr_copy.get_os_sock_len(),
                            )
                        };
                        if rc == 0 {
                            Some(Result::Connect(Ok(())))
                        } else {
                            match errno() {
                                // If we would block then we register for OUT events
                                // and are notified of connection completion that way.
                                libc::EINPROGRESS | libc::EAGAIN => {
                                    match unsafe {
                                        self.epoll_register(completion, libc::EPOLLOUT as u32)
                                    } {
                                        Ok(()) => None,
                                        Err(()) => {
                                            Some(Result::Connect(Err(ConnectError::Unexpected)))
                                        }
                                    }
                                }
                                _ => Some(Result::Connect(Err(ConnectError::Unexpected))),
                            }
                        }
                    }
                }
            }

            // PORT NOTE: the reference `.write` arm opens with `if
            // (completion.flags.threadpool) { if (self.thread_schedule(completion))
            // |_| return else |err| break :res .{ .write = err }; }`
            // (epoll.zig:632) — the write twin of the `.read` gate above. File sets
            // `threadpool = true` on plain write (file.zig:142), so its writes ride
            // the pool; an un-flagged write (GenericStream/TCP, which set `dup`)
            // registers for OUT below.
            Operation::Write { .. } => {
                if unsafe { (*completion).flags.threadpool } {
                    match unsafe { self.thread_schedule(completion) } {
                        Ok(()) => return,
                        Err(()) => Some(Result::Write(Err(WriteError::ThreadPoolRequired))),
                    }
                } else {
                    match unsafe { self.epoll_register(completion, libc::EPOLLOUT as u32) } {
                        Ok(()) => None,
                        Err(()) => Some(Result::Write(Err(WriteError::Unexpected))),
                    }
                }
            }

            // PORT NOTE: the `.pwrite` arm (epoll.zig:652) is the pwrite twin of
            // `.write` — the same threadpool gate then the same OUT registration;
            // only the perform-side syscall differs (`posix.pwrite` with `offset`).
            // File sets `threadpool = true` on pwrite (file.zig:302).
            // REFERENCE-QUIRK PORT NOTE: the oracle tags THIS start-arm's errors
            // `.write` even though the op is `.pwrite` (epoll.zig:652 — an upstream
            // tagged-union slip; the PERFORM side tags `.pwrite` correctly). We
            // deliberately tag `Result::PWrite` here: the strict pwrite trampoline
            // matches on `Result::PWrite` and would hit `unreachable!` on a
            // `Result::Write`.
            Operation::PWrite { .. } => {
                if unsafe { (*completion).flags.threadpool } {
                    match unsafe { self.thread_schedule(completion) } {
                        Ok(()) => return,
                        Err(()) => Some(Result::PWrite(Err(WriteError::ThreadPoolRequired))),
                    }
                } else {
                    match unsafe { self.epoll_register(completion, libc::EPOLLOUT as u32) } {
                        Ok(()) => None,
                        Err(()) => Some(Result::PWrite(Err(WriteError::Unexpected))),
                    }
                }
            }

            Operation::Send { .. } => {
                match unsafe { self.epoll_register(completion, libc::EPOLLOUT as u32) } {
                    Ok(()) => None,
                    Err(()) => Some(Result::Send(Err(WriteError::Unexpected))),
                }
            }

            Operation::Recv { .. } => {
                match unsafe {
                    self.epoll_register(completion, (libc::EPOLLIN | libc::EPOLLRDHUP) as u32)
                } {
                    Ok(()) => None,
                    Err(()) => Some(Result::Recv(Err(ReadError::Unexpected))),
                }
            }

            Operation::SendMsg { buffer, .. } => {
                // if (v.buffer) |_| { @panic("TODO: sendmsg with buffer"); }
                //
                // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): the
                // buffer-populating path is UNIMPLEMENTED upstream and panics. Kept
                // as a panic rather than "fixed" (that would diverge, R1); no ported
                // watcher constructs a `SendMsg` with a buffer, so it is unreachable
                // in practice.
                if buffer.is_some() {
                    panic!("TODO: sendmsg with buffer");
                }
                match unsafe { self.epoll_register(completion, libc::EPOLLOUT as u32) } {
                    Ok(()) => None,
                    Err(()) => Some(Result::SendMsg(Err(WriteError::Unexpected))),
                }
            }

            Operation::RecvMsg { .. } => {
                match unsafe {
                    self.epoll_register(completion, (libc::EPOLLIN | libc::EPOLLRDHUP) as u32)
                } {
                    Ok(()) => None,
                    Err(()) => Some(Result::RecvMsg(Err(ReadError::Unexpected))),
                }
            }

            Operation::Poll { events, .. } => {
                let ev = *events;
                match unsafe { self.epoll_register(completion, ev) } {
                    Ok(()) => None,
                    Err(()) => Some(Result::Poll(Err(PollError::Unexpected))),
                }
            }

            Operation::Close { fd, .. } => {
                let fd = *fd;
                // PORT NOTE: the reference routes close through the thread pool when
                // `flags.threadpool` is set (Stream.close does, epoll arm). When a
                // pool is attached, `thread_schedule` enqueues the blocking
                // `posix.close` onto it and returns (the completion fires from the
                // drain in `tick`); with no pool, `thread_schedule` yields
                // `error.ThreadPoolRequired`, mapped to the op result. An un-flagged
                // close (the raw socket-test path) runs `posix.close` synchronously
                // here.
                if unsafe { (*completion).flags.threadpool } {
                    match unsafe { self.thread_schedule(completion) } {
                        Ok(()) => return,
                        Err(()) => Some(Result::Close(Err(CloseError::ThreadPoolRequired))),
                    }
                } else {
                    // SAFETY: closing a caller-owned fd.
                    unsafe { libc::close(fd) };
                    Some(Result::Close(Ok(())))
                }
            }

            Operation::Shutdown { socket, how } => {
                let socket = *socket;
                let how_i = match how {
                    ShutdownHow::Recv => libc::SHUT_RD,
                    ShutdownHow::Send => libc::SHUT_WR,
                    ShutdownHow::Both => libc::SHUT_RDWR,
                };
                // SAFETY: shutdown on a caller-owned socket.
                let rc = unsafe { libc::shutdown(socket, how_i) };
                if rc == 0 {
                    Some(Result::Shutdown(Ok(())))
                } else {
                    Some(Result::Shutdown(Err(ShutdownError::Unexpected)))
                }
            }
        };

        // If we have a synchronous result then we call the callback
        // immediately and mark the error/completion.
        if let Some(res) = res_ {
            unsafe { (*completion).flags.state = State::Dead };

            let cb = unsafe { (*completion).callback };
            let ud = unsafe { (*completion).userdata };
            match cb(ud, self_ptr, completion, res) {
                CallbackAction::Disarm => {}

                // If we rearm then we requeue this. Due to the way that tick
                // works, this won't try to re-add immediately, it won't happen
                // until the next tick.
                CallbackAction::Rearm => unsafe { self.add(completion) },
            }

            return;
        }

        // Mark the completion as active if we reached this point.
        unsafe { (*completion).flags.state = State::Active };

        // Increase our active count.
        self.active += 1;
    }

    /// # Safety
    /// `completion` must be a valid completion pointer owned by this loop.
    unsafe fn stop_completion(&mut self, completion: *mut Completion) {
        let self_ptr: *mut Loop = self;

        // Delete. This should never fail.
        let maybe_fd = if unsafe { (*completion).flags.dup } {
            Some(unsafe { (*completion).flags.dup_fd })
        } else {
            unsafe { Completion::fd(completion) }
        };

        if let Some(fd) = maybe_fd {
            // SAFETY: DEL on a fd we previously ADDed; never fails.
            unsafe { libc::epoll_ctl(self.fd, libc::EPOLL_CTL_DEL, fd, ptr::null_mut()) };
        } else {
            // PORT NOTE: `switch (completion.op) { .timer => |*v| {...}, else => unreachable }`
            // — the sole fd-less stoppable op is `.timer`.
            match unsafe { &mut (*completion).op } {
                Operation::Timer(v) => {
                    let c = v.c;

                    if unsafe { (*c).flags.state } == State::Active {
                        // Timers need to be removed from the timer heap.
                        let v_ptr: *mut Timer = v;
                        unsafe { self.timers.remove(v_ptr) };
                    }

                    // If the timer was never fired, we need to fire it with
                    // the cancellation notice.
                    if unsafe { (*c).flags.state } != State::Dead {
                        // If we have reset set AND we got a cancellation result,
                        // that means that we were canceled so that we can update
                        // our expiration time.
                        if let Some(r) = v.reset {
                            v.next = r;
                            v.reset = None;
                            self.active -= 1;
                            unsafe { self.start(c) };
                            return;
                        }

                        let cb = unsafe { (*c).callback };
                        let ud = unsafe { (*c).userdata };
                        let action = cb(ud, self_ptr, c, Result::Timer(Ok(TimerTrigger::Cancel)));
                        match action {
                            CallbackAction::Disarm => {}
                            CallbackAction::Rearm => {
                                self.active -= 1;
                                unsafe { self.start(c) };
                                return;
                            }
                        }
                    }
                }

                _ => unreachable!(),
            }
        }

        // Decrement the active count so we know how many are running for
        // .until_done run semantics.
        if unsafe { (*completion).flags.state } == State::Active {
            self.active -= 1;
        }

        // Mark the completion as done.
        unsafe { (*completion).flags.state = State::Dead };
    }
}

// ---------------------------------------------------------------------------
// Completion (epoll.zig:872).
// ---------------------------------------------------------------------------

/// A completion is a request to perform some work with the loop.
#[repr(C)]
pub struct Completion {
    /// Operation to execute. This is only safe to read BEFORE the completion
    /// is queued. After being queued (with "add"), the operation may change.
    pub op: Operation,

    /// Userdata and callback for when the completion is finished.
    pub userdata: *mut core::ffi::c_void,
    pub callback: Callback,

    //---------------------------------------------------------------
    // Internal fields
    /// Intrusive queue field
    next: *mut Completion,

    flags: CompletionFlags,

    /// If scheduled on a thread pool, these will be set. This is NOT a
    /// reliable way to get access to the loop and shouldn't be used
    /// except internally.
    //
    // PORT NOTE (Task 2, REVISED WS19 T10): the oracle's `task_completions:
    // *TaskCompletionQueue` per-completion field had been elided — `thread_perform`
    // reached the loop's cached `thread_pool_completions_ptr` via `task_loop`
    // instead. WS19 T10 RESTORED it (and added `task_wakeup`): the mirihar
    // `wakeup_copied` candidate proves that ANY worker access to the `Loop`
    // allocation — including a by-value read of a `Copy` handle — is a foreign
    // access against the protected `&mut Loop` the loop thread holds for the whole
    // tick, hence SB UB. `task_result` holds the result computed on the pool thread
    // until the loop drains it in `tick`. See kqueue.rs's field docs (the two
    // backends are symmetric) and `.port/phase-2/DEFERRALS.md`.
    task: thread_pool::Task,
    task_loop: *mut Loop,
    task_result: Result,
    /// The reference's own field, restored (fidelity gain).
    task_completions: *mut TaskCompletionQueue,
    /// PORT-ADDED [R1-DIVERGENCE, WS19 T10] — the reference wakes the loop with
    /// `c.task_loop.wakeup()`; `Eventfd` is a `Copy` fd wrapper, copied in here at
    /// schedule time so the worker touches only this caller-owned allocation.
    task_wakeup: Eventfd,
}

/// PORT NOTE: the `flags: packed struct { state, threadpool, dup, dup_fd }`
/// sub-struct. The `threadpool` bit routes the op through the thread pool (Task 2);
/// `dup`/`dup_fd` back the epoll single-op-per-fd dup hack.
#[derive(Clone, Copy)]
struct CompletionFlags {
    /// Watch state of this completion. We use this to determine whether
    /// we're active, adding, deleting, etc. This lets us add and delete
    /// multiple times before a loop tick and handle the state properly.
    state: State,

    /// Schedule this onto the threadpool rather than epoll. Not all
    /// operations support this.
    //
    // PORT NOTE (Task 2): honored by the `.close` start arm (Stream.close sets it,
    // epoll arm). With a pool attached, `thread_schedule` enqueues the blocking op
    // onto it; with no pool it yields `ThreadPoolRequired`, exactly as the
    // reference's `thread_schedule` does when `Loop.thread_pool == null`.
    threadpool: bool,

    /// Set to true to dup the file descriptor for the operation prior
    /// to setting it up with epoll. This is a hack to make it so that
    /// a completion can represent a single op per fd, since epoll requires
    /// a single fd for multiple ops.
    dup: bool,
    dup_fd: fd_t,
}

/// PORT NOTE: `const State = enum(u3) { dead, adding, deleting, active }`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// completion is not part of any loop
    Dead = 0,
    /// completion is in the submission queue
    Adding = 1,
    /// completion is in the deletion queue
    Deleting = 2,
    /// completion is registered with epoll
    Active = 3,
}

// PORT NOTE: hand-written `Default` (NOT `#[derive]`) — `Completion` has an
// fn-ptr `callback` + raw-ptr fields with no `Default`. Mirrors the Zig field
// defaults `op = .noop`, `userdata = null`, `callback = noopCallback`,
// `next = null`, `flags = { state: dead, dup: false, dup_fd: 0 }`.
impl Default for Completion {
    fn default() -> Self {
        Completion {
            op: Operation::default(),
            userdata: ptr::null_mut(),
            callback: noop_callback,
            next: ptr::null_mut(),
            flags: CompletionFlags {
                state: State::Dead,
                threadpool: false,
                dup: false,
                dup_fd: 0,
            },
            // `task`/`task_loop`/`task_result`/`task_completions`/`task_wakeup` are
            // Zig `undefined` (only set when scheduled on the pool); zeroed/
            // placeholder here (Rust has no undefined).
            task: thread_pool::Task::default(),
            task_loop: ptr::null_mut(),
            task_result: Result::Noop,
            task_completions: ptr::null_mut(),
            task_wakeup: Eventfd { fd: -1 },
        }
    }
}

// PORT NOTE: `Completion` holds a raw-ptr `next` link + an fn-ptr → auto-`!Send`;
// it rides the `Loop` moved onto the termio thread. SAFETY / single-thread
// affinity: after the move a completion is touched by exactly the owning
// event-loop thread, so transferring ownership is sound.
unsafe impl Send for Completion {}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<Completion>`
// (submissions/deletions).
impl queue::Node for Completion {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

// PORT NOTE: intrusive `next` link seam for `queue_mpsc::Intrusive<Completion>`
// (thread_pool_completions).
impl queue_mpsc::Node for Completion {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

impl Completion {
    /// Returns the state of this completion.
    //
    // PORT NOTE: reference `state(self: Completion)` (by value); ported as an
    // associated fn taking `*mut Completion` (raw self) to stay on the raw-ptr
    // discipline. Test call sites (`c1.state()`) route through `state_of`.
    fn state(c: *mut Completion) -> CompletionState {
        match unsafe { (*c).flags.state } {
            State::Dead => CompletionState::Dead,
            State::Adding | State::Deleting | State::Active => CompletionState::Active,
        }
    }

    /// Convenience wrapper for `state` callable on a `&Completion` (tests do
    /// `c1.state_of()`).
    pub fn state_of(&self) -> CompletionState {
        Completion::state(self as *const Completion as *mut Completion)
    }

    /// PORT NOTE: the `c.* = .{ .op, .userdata, .callback }` initialization the
    /// watcher `wait`/`read`/`write`/`close`/`poll` methods perform — a FULL reset
    /// with the internal fields (`next`/`flags`) taking their `Default` values.
    //
    // PORT NOTE (`pub(crate)`): `watcher/timer.zig`'s `TimerLoop.cancel`
    // (timer.zig:137-210) builds its `c_cancel` completion as `.{ .op = .{ .cancel
    // = .{ .c = c_timer } }, ... }` from OUTSIDE this backend module, so the
    // watcher needs this full-reset seam — exactly the visibility `kqueue.rs`'s
    // twin (kqueue.rs:1589) already carries for the macOS `timer.rs` watcher.
    pub(crate) fn new_op(op: Operation, userdata: *mut c_void, callback: Callback) -> Completion {
        Completion {
            op,
            userdata,
            callback,
            ..Completion::default()
        }
    }

    /// PORT NOTE: the `c.flags.dup = true` assignment the epoll stream `Readable`/
    /// `Writeable` arms perform (`stream.zig:598/993`).
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer.
    unsafe fn set_dup(c: *mut Completion, v: bool) {
        unsafe { (*c).flags.dup = v };
    }

    /// PORT NOTE: the `c.flags.threadpool = true` assignment the epoll stream
    /// `Closeable` arm performs (`stream.zig:430`).
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer.
    unsafe fn set_threadpool(c: *mut Completion, v: bool) {
        unsafe { (*c).flags.threadpool = v };
    }

    /// Perform the operation associated with this completion. This performs
    /// the full blocking operation for the completion.
    ///
    /// # Safety
    /// `self_` valid+stable; `ev_` (if Some) a valid epoll_event for this op.
    unsafe fn perform(self_: *mut Completion, _ev: Option<*const libc::epoll_event>) -> Result {
        match unsafe { &mut (*self_).op } {
            // These never reach perform: they complete synchronously in `start`
            // or the timer heap.
            Operation::Cancel { .. }
            | Operation::Noop
            | Operation::Shutdown { .. }
            | Operation::Timer(_) => {
                unreachable!()
            }

            Operation::Accept {
                socket,
                addr,
                addr_size,
                flags,
            } => {
                // SAFETY: `accept4` fills `addr`/`addr_size` for the accepted conn.
                let r = unsafe {
                    libc::accept4(
                        *socket,
                        ptr::addr_of_mut!(*addr) as *mut libc::sockaddr,
                        addr_size,
                        *flags as libc::c_int,
                    )
                };
                if r < 0 {
                    Result::Accept(Err(AcceptError::Unknown))
                } else {
                    Result::Accept(Ok(r as socket_t))
                }
            }

            Operation::Connect { socket, .. } => {
                // if (posix.getsockoptError(op.socket)) {} else |err| err
                Result::Connect(unsafe { getsockopt_error(*socket) })
            }

            Operation::Poll { .. } => Result::Poll(Ok(())),

            Operation::Read { fd, buffer } => {
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => unsafe {
                        posix_read(*fd, (*v).cast::<u8>(), (*v).len())
                    },
                    ReadBuffer::Array(v) => unsafe { posix_read(*fd, v.as_mut_ptr(), v.len()) },
                };

                Result::Read(match n_ {
                    Ok(n) => {
                        if n == 0 {
                            Err(ReadError::EOF)
                        } else {
                            Ok(n)
                        }
                    }
                    Err(err) => Err(err),
                })
            }

            // PORT NOTE: `.pread => |*op| { const n_ = switch (op.buffer) { .slice
            // => posix.pread(op.fd, v, op.offset), .array => posix.pread(...) };
            // ... if (n == 0) error.EOF else n }` (epoll.zig:992) — the `.read`
            // body with `posix.pread(fd, buf, offset)` in place of `posix.read`.
            Operation::PRead { fd, buffer, offset } => {
                let off = *offset;
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => unsafe {
                        posix_pread(*fd, (*v).cast::<u8>(), (*v).len(), off)
                    },
                    ReadBuffer::Array(v) => unsafe {
                        posix_pread(*fd, v.as_mut_ptr(), v.len(), off)
                    },
                };

                Result::PRead(match n_ {
                    Ok(n) => {
                        if n == 0 {
                            Err(ReadError::EOF)
                        } else {
                            Ok(n)
                        }
                    }
                    Err(err) => Err(err),
                })
            }

            Operation::Write { fd, buffer } => Result::Write(match buffer {
                WriteBuffer::Slice(v) => unsafe { posix_write(*fd, (*v).cast::<u8>(), (*v).len()) },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_write(*fd, array.as_ptr(), *len)
                },
            }),

            // PORT NOTE: `.pwrite => |*op| .{ .pwrite = switch (op.buffer) { .slice
            // => posix.pwrite(op.fd, v, op.offset), .array => posix.pwrite(op.fd,
            // v.array[0..v.len], op.offset) } }` (epoll.zig:1013) — the `.write`
            // body with `posix.pwrite(fd, buf, offset)`.
            Operation::PWrite { fd, buffer, offset } => {
                let off = *offset;
                Result::PWrite(match buffer {
                    WriteBuffer::Slice(v) => unsafe {
                        posix_pwrite(*fd, (*v).cast::<u8>(), (*v).len(), off)
                    },
                    WriteBuffer::Array { array, len } => unsafe {
                        posix_pwrite(*fd, array.as_ptr(), *len, off)
                    },
                })
            }

            Operation::Send { fd, buffer } => Result::Send(match buffer {
                WriteBuffer::Slice(v) => unsafe { posix_send(*fd, (*v).cast::<u8>(), (*v).len()) },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_send(*fd, array.as_ptr(), *len)
                },
            }),

            Operation::Recv { fd, buffer } => {
                let n_: core::result::Result<usize, ReadError> = match buffer {
                    ReadBuffer::Slice(v) => unsafe {
                        posix_recv(*fd, (*v).cast::<u8>(), (*v).len())
                    },
                    ReadBuffer::Array(v) => unsafe { posix_recv(*fd, v.as_mut_ptr(), v.len()) },
                };

                Result::Recv(match n_ {
                    Ok(n) => {
                        if n == 0 {
                            Err(ReadError::EOF)
                        } else {
                            Ok(n)
                        }
                    }
                    Err(err) => Err(err),
                })
            }

            // PORT NOTE: `.sendmsg => |*op| .{ .sendmsg = posix.sendmsg(op.fd,
            // op.msghdr, 0) }`. The `buffer` panic guard lives in `start`, not here.
            Operation::SendMsg { fd, msghdr, .. } => {
                Result::SendMsg(unsafe { posix_sendmsg(*fd, *msghdr) })
            }

            // PORT NOTE: `.recvmsg => |*op| { const res = linux.recvmsg(op.fd,
            // op.msghdr, 0); ... if (res == 0) error.EOF else if (res > 0) res ...
            // }`. A zero-byte datagram result is an unconditional `error.EOF`.
            Operation::RecvMsg { fd, msghdr } => {
                Result::RecvMsg(unsafe { posix_recvmsg(*fd, *msghdr) })
            }

            // PORT NOTE: an un-flagged `close` completes synchronously in `start`; a
            // threadpool-flagged `close` (Stream.close) reaches `perform` HERE, on a
            // pool worker thread via `thread_perform`, when a pool is attached (Task
            // 2). The `posix.close` body matches the reference's `perform` `.close`
            // arm.
            Operation::Close { fd } => {
                unsafe { libc::close(*fd) };
                Result::Close(Ok(()))
            }
        }
    }

    /// Return the fd for the completion. This will perform a dup(2) if
    /// requested.
    ///
    /// # Safety
    /// `self_` must be a valid, stable `Completion` pointer whose op has an fd.
    unsafe fn fd_maybe_dup(self_: *mut Completion) -> core::result::Result<fd_t, ()> {
        let old_fd = unsafe { Completion::fd(self_) }.unwrap();
        if !unsafe { (*self_).flags.dup } {
            return Ok(old_fd);
        }
        if unsafe { (*self_).flags.dup_fd } > 0 {
            return Ok(unsafe { (*self_).flags.dup_fd });
        }

        let dup_fd = unsafe { libc::dup(old_fd) };
        if dup_fd < 0 {
            return Err(());
        }
        unsafe { (*self_).flags.dup_fd = dup_fd };
        Ok(dup_fd)
    }

    /// Returns the fd associated with the completion (if any).
    ///
    /// # Safety
    /// `self_` must be a valid, stable `Completion` pointer.
    unsafe fn fd(self_: *mut Completion) -> Option<fd_t> {
        match unsafe { &(*self_).op } {
            Operation::Accept { socket, .. } => Some(*socket),
            Operation::Connect { socket, .. } => Some(*socket),
            Operation::Poll { fd, .. } => Some(*fd),
            Operation::Read { fd, .. } => Some(*fd),
            Operation::PRead { fd, .. } => Some(*fd),
            Operation::Recv { fd, .. } => Some(*fd),
            Operation::Write { fd, .. } => Some(*fd),
            Operation::PWrite { fd, .. } => Some(*fd),
            Operation::Send { fd, .. } => Some(*fd),
            Operation::SendMsg { fd, .. } => Some(*fd),
            Operation::RecvMsg { fd, .. } => Some(*fd),
            Operation::Close { fd, .. } => Some(*fd),
            Operation::Shutdown { socket, .. } => Some(*socket),

            Operation::Cancel { .. } | Operation::Timer(_) => None,

            Operation::Noop => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------------
// Operation / Result unions (epoll.zig:1105/1126/1149) + errors + buffers.
// ---------------------------------------------------------------------------

/// All the supported operations of this event loop.
//
// PORT NOTE: `pub const Operation = union(OperationType) { ... }`.
// `op: Operation = .{ .noop = {} }` — the `#[default]` `Noop` variant. Task 4 adds
// the socket/process/stream tags (accept/connect/poll/write/send/recv/close/
// shutdown); Task 5 adds `pread`/`pwrite` (the File watcher's offset-carrying
// read/write, which ride the thread pool on epoll).
#[derive(Default)]
pub enum Operation {
    #[default]
    Noop,

    Cancel {
        c: *mut Completion,
    },

    Accept {
        socket: socket_t,
        addr: net::sockaddr::Any,
        addr_size: net::socklen_t,
        flags: u32,
    },

    Connect {
        socket: socket_t,
        addr: net::Address,
    },

    /// Poll for events but do not perform any operations on them being
    /// ready. The "events" field are a OR-ed list of EPOLL events.
    Poll {
        fd: fd_t,
        events: u32,
    },

    Read {
        fd: fd_t,
        buffer: ReadBuffer,
    },

    /// PORT NOTE: `pread: struct { fd, buffer, offset: u64 }` — the offset-carrying
    /// read used by the File watcher.
    PRead {
        fd: fd_t,
        buffer: ReadBuffer,
        offset: u64,
    },

    Write {
        fd: fd_t,
        buffer: WriteBuffer,
    },

    /// PORT NOTE: `pwrite: struct { fd, buffer, offset: u64 }` — the offset-carrying
    /// write used by the File watcher.
    PWrite {
        fd: fd_t,
        buffer: WriteBuffer,
        offset: u64,
    },

    Send {
        fd: fd_t,
        buffer: WriteBuffer,
    },

    Recv {
        fd: fd_t,
        buffer: ReadBuffer,
    },

    /// Send a message on a socket using sendmsg (i.e. UDP).
    //
    // PORT NOTE: the reference field is `msghdr: *posix.msghdr_const`. `libc` has no
    // `msghdr_const` twin (Zig splits the type by constness; C/Rust do not), so the
    // constness rides the POINTER: `*const libc::msghdr`. `libc::msghdr` is the
    // glibc/kernel `struct msghdr` layout — exactly what `libc::sendmsg` reads.
    SendMsg {
        fd: fd_t,
        msghdr: *const libc::msghdr,

        /// Optionally, a write buffer can be specified and the given
        /// msghdr will be populated with information about this buffer.
        //
        // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): this field is
        // DEAD in the reference — the `start` `.sendmsg` arm does
        // `if (v.buffer) |_| @panic("TODO: sendmsg with buffer");`, so the only
        // reachable value is `null`. The field (and `iov` below) are kept so the
        // op's shape matches the reference and so the panic arm is faithful (mirrors
        // io_uring.rs's `SendMsg`).
        buffer: Option<WriteBuffer>,

        /// Do not use this, it is only used internally.
        //
        // PORT NOTE: `iov: [1]posix.iovec_const = undefined` — never written nor
        // read by the reference (it is the scratch the unimplemented `buffer` path
        // would populate). Kept for shape fidelity; `libc::iovec` is the same layout
        // as Zig's `iovec_const` (constness rides the pointer in C/Rust).
        iov: [libc::iovec; 1],
    },

    /// Recieve a message on a socket using recvmsg (i.e. UDP).
    RecvMsg {
        fd: fd_t,
        msghdr: *mut libc::msghdr,
    },

    Close {
        fd: fd_t,
    },

    Shutdown {
        socket: socket_t,
        how: ShutdownHow,
    },

    Timer(Timer),
}

/// PORT NOTE: `posix.ShutdownHow` — the `how` selector of the `shutdown` op
/// (`.both` default).
#[derive(Clone, Copy)]
pub enum ShutdownHow {
    Recv,
    Send,
    Both,
}

/// PORT NOTE: `pub const Result = union(OperationType) { ... }`. Mirrors the
/// `Operation` tag subset ported so far (Task 3 core + Task 4 socket/stream).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Result {
    Noop,
    Cancel(core::result::Result<(), CancelError>),
    Accept(core::result::Result<socket_t, AcceptError>),
    Connect(core::result::Result<(), ConnectError>),
    Poll(core::result::Result<(), PollError>),
    Read(core::result::Result<usize, ReadError>),
    PRead(core::result::Result<usize, ReadError>),
    Write(core::result::Result<usize, WriteError>),
    PWrite(core::result::Result<usize, WriteError>),
    Send(core::result::Result<usize, WriteError>),
    Recv(core::result::Result<usize, ReadError>),
    SendMsg(core::result::Result<usize, WriteError>),
    RecvMsg(core::result::Result<usize, ReadError>),
    Close(core::result::Result<(), CloseError>),
    Shutdown(core::result::Result<(), ShutdownError>),
    Timer(core::result::Result<TimerTrigger, TimerError>),
}

/// PORT NOTE: `pub const CancelError = ThreadPoolError || error{NotFound}`. Only
/// the `Ok(())` result is produced by the ported paths; the variant is kept so
/// the result type is nameable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelError {
    NotFound,
}

/// PORT NOTE: `pub const AcceptError = posix.EpollCtlError || error{DupFailed,
/// Unknown}`. `Unknown` is the reference's `posix.accept catch → error.Unknown`;
/// `Unexpected` covers the `start`-arm epoll_ctl/dup failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptError {
    Unknown,
    Unexpected,
}

/// PORT NOTE: `pub const ConnectError = posix.EpollCtlError || posix.ConnectError
/// || error{DupFailed, Unknown}`. Collapsed to `Unexpected` (no in-scope test
/// exercises a specific connect-error path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    Unexpected,
}

/// PORT NOTE: `pub const PollError = posix.EpollCtlError || error{DupFailed,
/// Unknown}` (the epoll `Sys.PollError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollError {
    Unexpected,
}

/// PORT NOTE: `pub const WriteError = ThreadPoolError || posix.EpollCtlError ||
/// posix.WriteError || posix.PWriteError || posix.SendError || error{DupFailed,
/// Unknown}`. The `ThreadPoolRequired` arm is reached when a threadpool-flagged
/// write/pwrite (File's, epoll arm) runs with no pool attached; the rest of the
/// posix syscall-error variants collapse to `Unexpected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    ThreadPoolRequired,
    Unexpected,
}

/// PORT NOTE: `pub const CloseError = posix.EpollCtlError || ThreadPoolError ||
/// error{Unknown}`. The `ThreadPoolRequired` arm is reached when a threadpool-
/// flagged close (Stream.close) runs with no pool attached (deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    ThreadPoolRequired,
    Unexpected,
}

/// PORT NOTE: `pub const ShutdownError = posix.EpollCtlError ||
/// posix.ShutdownError || error{Unknown}`. Collapsed to `Unexpected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownError {
    Unexpected,
}

/// PORT NOTE: `pub const ReadError = ThreadPoolError || posix.EpollCtlError ||
/// posix.ReadError || posix.PReadError || ... || error{DupFailed, EOF, Unknown}`.
/// The `ThreadPoolRequired` arm is reached when a threadpool-flagged read/pread
/// (File's, epoll arm) runs with no pool attached; the rest of the posix
/// syscall-error variants collapse to `Unexpected` (no in-scope test exercises a
/// raw read-error path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    ThreadPoolRequired,
    EOF,
    Unexpected,
}

/// PORT NOTE: `pub const TimerError = error{Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    Unexpected,
}

/// PORT NOTE: `pub const TimerTrigger = enum { request, expiration, cancel }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerTrigger {
    /// Unused with epoll
    Request,
    /// Timer expired.
    Expiration,
    /// Timer was canceled.
    Cancel,
}

/// ReadBuffer are the various options for reading.
//
// PORT NOTE (R3): the Zig `slice: []u8` ports as a raw fat pointer `*mut [u8]`
// (never a lifetime — the buffer lives in caller memory and the loop re-enters it
// across ticks), NEVER a `String`.
#[derive(Clone, Copy)]
pub enum ReadBuffer {
    /// Read into this slice.
    Slice(*mut [u8]),

    /// Read into this array, just set this to undefined and it will
    /// be populated up to the size of the array.
    Array([u8; 32]),
    // TODO: future will have vectors
}

/// WriteBuffer are the various options for writing.
//
// PORT NOTE (R3): the Zig `slice: []const u8` ports as a raw fat pointer
// `*const [u8]` (never a lifetime — the buffer lives in caller memory and the
// loop re-enters it across ticks). The `array` arm mirrors the Zig
// `struct { array: [32]u8, len: usize }`.
#[derive(Clone, Copy)]
pub enum WriteBuffer {
    /// Write from this buffer.
    Slice(*const [u8]),

    /// Write from this array. See ReadBuffer.array for why we support this.
    Array { array: [u8; 32], len: usize },
    // TODO: future will have vectors
}

// PORT NOTE: `posix.read` wrapper the `perform` read path calls. Raw `libc`.
unsafe fn posix_read(fd: fd_t, ptr: *mut u8, len: usize) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::read(fd, ptr.cast(), len) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.pread(fd, buf, offset)` wrapper (the `pread` perform path).
unsafe fn posix_pread(
    fd: fd_t,
    ptr: *mut u8,
    len: usize,
    offset: u64,
) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::pread(fd, ptr.cast(), len, offset as libc::off_t) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.recv(fd, buf, 0)` wrapper (the `recv` perform path).
unsafe fn posix_recv(fd: fd_t, ptr: *mut u8, len: usize) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::recv(fd, ptr.cast(), len, 0) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.write` wrapper (the `write` perform path).
unsafe fn posix_write(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::write(fd, ptr.cast(), len) };
    if n < 0 {
        Err(WriteError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.pwrite(fd, buf, offset)` wrapper (the `pwrite` perform path).
unsafe fn posix_pwrite(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
    offset: u64,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::pwrite(fd, ptr.cast(), len, offset as libc::off_t) };
    if n < 0 {
        Err(WriteError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.send(fd, buf, 0)` wrapper (the `send` perform path).
unsafe fn posix_send(
    fd: fd_t,
    ptr: *const u8,
    len: usize,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::send(fd, ptr.cast(), len, 0) };
    if n < 0 {
        Err(WriteError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `posix.sendmsg(fd, msghdr, 0)` wrapper (the `sendmsg` perform path).
//
// # Safety
// `fd` a live socket; `msghdr` (and everything it points at) valid for the call.
unsafe fn posix_sendmsg(
    fd: fd_t,
    msghdr: *const libc::msghdr,
) -> core::result::Result<usize, WriteError> {
    let n = unsafe { libc::sendmsg(fd, msghdr, 0) };
    if n < 0 {
        Err(WriteError::Unexpected)
    } else {
        Ok(n as usize)
    }
}

// PORT NOTE: `linux.recvmsg(fd, msghdr, 0)` wrapper (the `recvmsg` perform path):
// a zero-byte datagram is `error.EOF`; a negative return is `Unexpected`.
//
// # Safety
// `fd` a live socket; `msghdr` (and everything it points at) valid+writable for
// the call — the kernel writes `msg_name`/the iovec buffers.
unsafe fn posix_recvmsg(
    fd: fd_t,
    msghdr: *mut libc::msghdr,
) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::recvmsg(fd, msghdr, 0) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else if n == 0 {
        Err(ReadError::EOF)
    } else {
        Ok(n as usize)
    }
}

/// PORT NOTE: `posix.getsockoptError(socket)` — reads `SO_ERROR` and maps to a
/// `ConnectError` (0 → success). The `connect` perform arm calls this once the
/// socket signals writable.
///
/// # Safety
/// `socket` must be a valid socket fd.
unsafe fn getsockopt_error(socket: socket_t) -> core::result::Result<(), ConnectError> {
    let mut err_code: libc::c_int = 0;
    let mut size = size_of::<libc::c_int>() as net::socklen_t;
    unsafe {
        libc::getsockopt(
            socket,
            libc::SOL_SOCKET,
            libc::SO_ERROR,
            ptr::addr_of_mut!(err_code).cast(),
            &mut size,
        )
    };
    if err_code == 0 {
        Ok(())
    } else {
        Err(ConnectError::Unexpected)
    }
}

// ---------------------------------------------------------------------------
// Async (watcher/async.zig `AsyncEventFd`) — the cross-thread loop wakeup,
// INSIDE this backend (mirroring io_uring.rs). `Async(xev)` selects
// `AsyncEventFd` for the io_uring AND epoll backends alike (async.zig:14-16),
// and its `wait` is the `waitPoll` arm for both (async.zig:86) — this watcher is
// byte-identical between the two Linux backends; only the module-local
// `Loop`/`Completion`/`Operation`/`Result` types it names differ.
//
// PORT NOTE (SCOPE, task L1 LT7): Tasks 3/4 did not port this watcher (the epoll
// `Loop` embeds a bare `linux::eventfd::Eventfd` for its own internal wakeup
// instead), but `dynamic.rs` must union `epoll::Async` with `io_uring::Async` —
// `Async(Dynamic)` is `AsyncDynamic` over BOTH candidates (async.zig:9-10 +
// dynamic.zig:82). It lands here, additively, so both Linux backends expose the
// same watcher surface.
// ---------------------------------------------------------------------------

/// "Wake up" an event loop from any thread using an async completion.
//
// PORT NOTE: `AsyncEventFd(xev)` monomorphized for epoll. `#[derive(Clone,
// Copy)]`: the reference `wait`/`notify` take `self: Self` by value (a trivially
// copyable fd handle), exactly like `linux::eventfd::Eventfd`.
//
// PORT NOTE: the reference's `fd: posix.fd_t` + inline `posix.eventfd`/`read`/
// `write` calls are expressed through the crate's already-ported
// `linux::eventfd::Eventfd` primitive (Task 2), which IS that fd plus the very
// same `eventfd(0, EFD_CLOEXEC|EFD_NONBLOCK)` / `write(1u64)` /
// `EAGAIN => success` surface. The FreeBSD `extern "c" fn eventfd` arm is
// comptime-dead here (this module is Linux-only).
#[derive(Clone, Copy)]
pub struct Async {
    /// eventfd file descriptor
    ev: Eventfd,
}

impl Async {
    /// Create a new async. An async can be assigned to exactly one loop
    /// to be woken up. The completion must be allocated in advance.
    pub fn init() -> core::result::Result<Async, Error> {
        match Eventfd::init() {
            Ok(ev) => Ok(Async { ev }),
            Err(_) => Err(Error),
        }
    }

    /// Clean up the async. This will forcibly deinitialize any resources
    /// and may result in erroneous wait callbacks to be fired.
    pub fn deinit(&self) {
        self.ev.deinit();
    }

    /// The underlying eventfd. Exposed so a caller can poll/register it directly.
    pub fn fd(&self) -> fd_t {
        self.ev.fd
    }

    /// Wait for a message on this async. Note that async messages may be
    /// coalesced (or they may not be) so you should not expect a 1:1 mapping
    /// between send and wait.
    ///
    /// Just like the rest of libxev, the wait must be re-queued if you want
    /// to continue to be notified of async events.
    ///
    /// You should NOT register an async with multiple loops (the same loop
    /// is fine -- but unnecessary). The behavior when waiting on multiple
    /// loops is undefined.
    //
    // PORT NOTE: `pub const wait = switch (xev.backend) { .io_uring, .epoll =>
    // waitPoll, ... }` — this IS `waitPoll`; the `waitRead` (kqueue) arm is
    // comptime-dead for epoll.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `wait::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE. `WaitError = xev.ReadError`.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn wait<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Async::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // PORT NOTE (verbatim from the reference): We use a poll operation
            // instead of a read operation because in Kernel 6.15.4, read was
            // regressed for io_uring on eventfd/timerfd and would block forever.
            // However, poll works fine.
            //
            // PORT NOTE: `waitPoll` spells the mask `posix.POLL.IN` for BOTH the
            // io_uring and epoll arms (async.zig:145) even though this backend's
            // `Operation::Poll.events` is documented as EPOLL events. That is
            // benign upstream and here: `POLLIN == EPOLLIN == 0x1`. Kept as the
            // reference spells it rather than "fixed" to `EPOLLIN`.
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.ev.fd,
                    events: libc::POLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::wait_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn wait_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let mapped: core::result::Result<(), ReadError> = match r {
            Result::Poll(res) => res.map_err(|e| match e {
                // PORT NOTE: `WaitError = xev.ReadError`, but the epoll `poll` op
                // yields a `PollError`. The reference's `if (r.poll) |_| {} else
                // |err| err` relies on Zig's error-set COERCION: this backend's
                // `PollError` (`{Unexpected}`) is a subset of its `ReadError`
                // (`{EOF, Unexpected}`), so the coercion is total. Rust has no
                // error-set subtyping, so the same total mapping is spelled out.
                PollError::Unexpected => ReadError::Unexpected,
            }),
            _ => unreachable!(),
        };

        if mapped.is_ok() {
            // PORT NOTE (verbatim): We need to read so that we can consume the
            // eventfd value. We only read 8 bytes because we only write up to 8
            // bytes and we own the fd. We ignore errors here because we expect the
            // read to succeed given we just polled it.
            let fd = match unsafe { &(*c_inner).op } {
                Operation::Poll { fd, .. } => *fd,
                _ => unreachable!(),
            };
            let mut buf = [0u8; 8];
            // SAFETY: an 8-byte read into a live local buffer off our own eventfd.
            // The result is discarded exactly as the reference's `_ = posix.read(
            // c_inner.op.poll.fd, &buf) catch {}` discards it.
            let _ = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<c_void>(), buf.len()) };
        }

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
    }

    /// Notify a loop to wake up synchronously. This should never block forever
    /// (it will always EVENTUALLY succeed regardless of if the loop is currently
    /// ticking or not).
    ///
    /// The "c" value is the completion associated with the "wait".
    ///
    /// Internal details subject to change but if you're relying on these
    /// details then you may want to consider using a lower level interface
    /// using the loop directly:
    ///
    ///   - linux+io_uring: eventfd is used. If the eventfd write would block
    ///     (EAGAIN) then we assume success because the eventfd is full.
    pub fn notify(&self) -> core::result::Result<(), Error> {
        // `Eventfd::notify` IS the reference's `write(fd, &@bitCast(@as(u64, 1)))`
        // plus its `error.WouldBlock => return` arm.
        self.ev.notify().map_err(|_| Error)
    }
}

// ---------------------------------------------------------------------------
// Timer (heap node) (epoll.zig:1235).
// ---------------------------------------------------------------------------

/// Timer that is inserted into the heap.
pub struct Timer {
    /// The absolute time to fire this timer next.
    next: Timespec,

    /// Only used internally. If this is non-null and timer is CANCELLED, then
    /// the timer is rearmed automatically with this as the next time. The
    /// callback will not be called on the cancellation.
    reset: Option<Timespec>,

    /// Internal heap fields.
    heap: heap::IntrusiveField<Timer>,

    /// We point back to completion for now.
    /// [1]: https://github.com/ziglang/zig/issues/6611
    c: *mut Completion,
}

// PORT NOTE: the intrusive `heap` field seam for `heap::Intrusive<Timer, ()>`.
impl heap::Node for Timer {
    unsafe fn heap_field(this: *mut Self) -> *mut heap::IntrusiveField<Self> {
        unsafe { ptr::addr_of_mut!((*this).heap) }
    }
}

impl Timer {
    /// PORT NOTE: `fn less(_: void, a: *const Timer, b: *const Timer) bool`.
    fn less(_: (), a: *mut Timer, b: *mut Timer) -> bool {
        unsafe { Timer::ns(a) < Timer::ns(b) }
    }

    /// Returns the nanoseconds of this timer. maxInt(u64) ns is 584 years, so
    /// any overflow just uses maxInt(u64).
    ///
    /// # Safety
    /// `this` must be a valid `Timer` pointer.
    unsafe fn ns(this: *mut Timer) -> u64 {
        let next = unsafe { (*this).next };
        debug_assert!(next.tv_sec >= 0);
        debug_assert!(next.tv_nsec >= 0);

        let max = u64::MAX;
        let s_ns = match (next.tv_sec as u64).checked_mul(ns_per_s) {
            Some(v) => v,
            None => return max,
        };
        s_ns.checked_add(next.tv_nsec as u64).unwrap_or(max)
    }
}

// ---------------------------------------------------------------------------
// Process (watcher/process.zig `ProcessPidFd`) — the Linux subprocess-exit
// watcher. Lives INSIDE this backend (per the brief): a `pidfd_open`'d fd,
// registered readable via the loop's `poll` op, reaped with `waitid(P_PIDFD)`.
// ---------------------------------------------------------------------------

/// PORT NOTE: `pub const WaitError = xev.Sys.PollError || error{InvalidChild}`.
/// The error that can come in the wait callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessWaitError {
    InvalidChild,
    Unexpected,
}

/// Process implementation using pidfd (Linux).
//
// PORT NOTE: `ProcessPidFd` monomorphized. `#[derive(Clone, Copy)]`: the reference
// `wait` takes `self: Self` by value (a trivially-copyable `fd` handle).
#[derive(Clone, Copy)]
pub struct Process {
    /// pidfd file descriptor
    fd: fd_t,
}

impl Process {
    /// Create a new process watcher for the given pid.
    //
    // PORT NOTE: `linux.pidfd_open(pid, posix.SOCK.NONBLOCK)` (== PIDFD_NONBLOCK).
    // `pidfd_open` has no direct `libc` binding, so it goes through the raw
    // `SYS_pidfd_open` syscall. The reference maps each errno to a specific error;
    // the D6 seam collapses those to the unified `crate::Error`.
    //
    // PORT NOTE (SIGCHLD fallback): the brief notes an old-kernel `pidfd_open`
    // ENOSYS fallback to a SIGCHLD/`waitpid` path. libxev's `ProcessPidFd` has NO
    // such fallback (it returns `unexpectedErrno`), and `pidfd_open` has existed
    // since Linux 5.3 — the target here is 6.x — so ENOSYS cannot occur on any
    // supported kernel and no KAT exercises it. Rather than invent an untested,
    // reference-less signalfd/SIGCHLD watcher, ENOSYS (like any other error)
    // surfaces as `Err` from `init`; a real fallback is deferred if an old-kernel
    // target ever materializes.
    pub fn init(pid: pid_t) -> core::result::Result<Process, Error> {
        // SAFETY: raw pidfd_open(2) syscall; returns a new fd or -1/errno.
        let res = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, libc::PIDFD_NONBLOCK) };
        if res < 0 {
            return Err(Error);
        }
        Ok(Process { fd: res as fd_t })
    }

    /// Clean up the process watcher.
    pub fn deinit(&mut self) {
        // SAFETY: closing our own pidfd.
        unsafe { libc::close(self.fd) };
    }

    /// Wait for the process to exit. This will automatically call
    /// `waitpid` or equivalent and report the exit status.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `wait::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE (Zig comptime `cb` ⇒ a Rust zero-sized
    // type). One untyped loop trampoline per call-site; `loop`/`c` stay RAW.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers held raw by the loop.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn wait<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<u32, ProcessWaitError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Process::wait cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        // c.* = .{ .op = .{ .poll = { .fd, .events = EPOLL.IN } }, ... };
        unsafe {
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    events: libc::EPOLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    /// The monomorphized untyped loop trampoline: on the pidfd signalling
    /// readable it reaps the child via `waitid(P_PIDFD)` then calls the typed `cb`.
    fn trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<u32, ProcessWaitError>,
            ) -> CallbackAction
            + 'static,
    {
        // const arg: WaitError!u32 = arg: { ... };
        let arg: core::result::Result<u32, ProcessWaitError> = 'arg: {
            // If our poll failed, report that error.
            // _ = r.poll catch |err| break :arg err;
            match r {
                Result::Poll(Ok(())) => {}
                Result::Poll(Err(_)) => break 'arg Err(ProcessWaitError::Unexpected),
                _ => unreachable!(),
            }

            // We need to wait on the pidfd because it is noted as ready.
            // const fd = c_inner.op.poll.fd;
            let fd = match unsafe { &(*c_inner).op } {
                Operation::Poll { fd, .. } => *fd,
                _ => unreachable!(),
            };
            // var info: siginfo_t; res = waitid(.PIDFD, fd, &info, W.EXITED);
            let mut info: libc::siginfo_t = unsafe { core::mem::zeroed() };
            let res =
                unsafe { libc::waitid(libc::P_PIDFD, fd as libc::id_t, &mut info, libc::WEXITED) };
            if res == 0 {
                // .SUCCESS => info.fields.common.second.sigchld.status
                break 'arg Ok(unsafe { info.si_status() } as u32);
            }
            match errno() {
                libc::ECHILD => Err(ProcessWaitError::InvalidChild),
                // The fd isn't ready to read, I guess?
                libc::EAGAIN => return CallbackAction::Rearm,
                _ => Err(ProcessWaitError::Unexpected),
            }
        };

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, arg)
    }
}

// ---------------------------------------------------------------------------
// Stream / WriteQueue / WriteRequest (watcher/stream.zig `GenericStream`) — the
// pty read/write engine, INSIDE this backend. Monomorphized for the epoll
// `GenericStream` config: `{ .close = true, .poll = true, .read = .read,
// .write = .write, .threadpool = false }`.
// ---------------------------------------------------------------------------

/// WriteQueue is the queue of write requests for ordered writes.
//
// PORT NOTE: `pub const WriteQueue = queue.Intrusive(WriteRequest);` — the
// hand-written `Default` (head/tail = null) comes from `queue::Intrusive`.
pub type WriteQueue = queue::Intrusive<WriteRequest>;

/// WriteRequest is a single request for a write. It wraps a
/// completion so that it can be inserted into the WriteQueue.
pub struct WriteRequest {
    pub completion: Completion,
    pub userdata: *mut c_void,

    /// This is the original buffer passed to queueWrite. We have
    /// to keep track of this because we may be forced to split
    /// the write or rearm the write due to partial writes, but when
    /// we call the final callback we want to pass the original
    /// complete buffer.
    pub full_write_buffer: WriteBuffer,

    next: *mut WriteRequest,
}

// PORT NOTE: hand-written `Default` (NOT `#[derive]`) — `full_write_buffer` has no
// Zig default (it is `undefined` in the pool element, fully overwritten by
// `queueWrite`). The placeholder empty `Array` is never observed.
impl Default for WriteRequest {
    fn default() -> Self {
        WriteRequest {
            completion: Completion::default(),
            userdata: ptr::null_mut(),
            full_write_buffer: WriteBuffer::Array {
                array: [0u8; 32],
                len: 0,
            },
            next: ptr::null_mut(),
        }
    }
}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<WriteRequest>`.
impl queue::Node for WriteRequest {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

// PORT NOTE: `WriteRequest` holds raw-ptr fields + embeds `Completion` (fn-ptr) →
// auto-`!Send`; it rides the `Loop` moved onto the termio thread. Single-thread
// affinity after the move makes transferring ownership sound.
unsafe impl Send for WriteRequest {}

impl WriteRequest {
    /// This can be used to convert a completion pointer back to
    /// a WriteRequest.
    //
    // PORT NOTE: `return @fieldParentPtr("completion", c);` — `container_of`
    // (offset-of) recovery of the enclosing `WriteRequest`.
    pub fn from(c: *mut Completion) -> *mut WriteRequest {
        unsafe {
            (c as *mut u8).sub(core::mem::offset_of!(WriteRequest, completion)) as *mut WriteRequest
        }
    }
}

/// Events that can be polled for using the high level streams (epoll arm:
/// `read = EPOLL.IN`).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollEvent {
    Read,
}

/// PORT NOTE: `GenericStream(epoll)`. `#[derive(Clone, Copy)]`: the reference
/// `read`/`write`/`close`/`poll`/`queueWrite` take `self: Self` by value.
#[derive(Clone, Copy)]
pub struct Stream {
    /// The underlying file
    fd: fd_t,
}

impl Stream {
    /// Initialize a generic stream from a file descriptor.
    pub fn init_fd(fd: fd_t) -> Stream {
        Stream { fd }
    }

    /// Clean up any watcher resources. This does NOT close the file.
    pub fn deinit(&self) {
        let _ = self;
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:443). `options.read == .read` for GenericStream.
    // -------------------------------------------------------------------

    /// Read from the socket. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the read.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn read<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: ReadBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::read cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Read {
                    fd: self.fd,
                    buffer: buf,
                },
                userdata.cast::<c_void>(),
                Self::read_trampoline::<UD, CB>,
            );
            // epoll arm: `if (options.threadpool) threadpool=true else dup=true`.
            // GenericStream `options.threadpool == false` ⇒ dup.
            Completion::set_dup(c, true);
            (*loop_).add(c);
        }
    }

    fn read_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (fd, buffer) = match unsafe { &(*c_inner).op } {
            Operation::Read { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::Read(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            Stream::init_fd(fd),
            buffer,
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:620). `options.write == .write` for GenericStream.
    // -------------------------------------------------------------------

    /// Write to the stream. This queues the writes to ensure they remain in
    /// order.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks.
    pub unsafe fn queue_write<UD, CB>(
        self,
        loop_: *mut Loop,
        q: *mut WriteQueue,
        req: *mut WriteRequest,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::queue_write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // req.* = .{ .full_write_buffer = buf };
            *req = WriteRequest {
                full_write_buffer: buf,
                ..WriteRequest::default()
            };
            // Must be kept in sync with partial write logic inside the callback
            self.write_init(ptr::addr_of_mut!((*req).completion), buf);
            (*req).completion.userdata = q as *mut c_void;
            (*req).completion.callback = Self::queue_write_trampoline::<UD, CB>;

            // The userdata as to go on the WriteRequest because we need
            // our actual completion userdata to be the WriteQueue.
            (*req).userdata = userdata.cast::<c_void>();

            // If the queue is empty, then we add our completion.
            if (*q).empty() {
                (*loop_).add(ptr::addr_of_mut!((*req).completion));
            }

            // We always add this item to our queue no matter what
            (*q).push(req);
        }
    }

    /// The ordered-write engine trampoline — chains completions through the
    /// `WriteQueue` and handles partial writes by re-`write_init`-ing the remainder.
    fn queue_write_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let q_inner = ud as *mut WriteQueue;
        debug_assert!(!q_inner.is_null());

        // We only peek the request here (not pop) because we may need to rearm
        // this write if the write was partial.
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).head };
        debug_assert!(!req_inner.is_null());

        let cb_res = Stream::write_result(c_inner, r);
        let mut result: core::result::Result<usize, WriteError> = cb_res.result;

        // Checks whether the entire buffer was written; if the write was partial,
        // it re-submits the remainder of the buffer.
        let queued_len = Stream::write_buffer_length(cb_res.buf);
        if let Ok(written_len) = cb_res.result {
            if written_len < queued_len {
                // Write remainder of the buffer, reusing the same completion.
                let rem_buf = Stream::write_buffer_remainder(cb_res.buf, written_len);
                unsafe {
                    cb_res
                        .writer
                        .write_init(ptr::addr_of_mut!((*req_inner).completion), rem_buf);
                    (*req_inner).completion.userdata = q_inner as *mut c_void;
                    (*req_inner).completion.callback = Self::queue_write_trampoline::<UD, CB>;
                    (*l_inner).add(ptr::addr_of_mut!((*req_inner).completion));
                }
                return CallbackAction::Disarm;
            }

            // We wrote the entire buffer; report the full length to the caller.
            result = Ok(Stream::write_buffer_length(unsafe {
                (*req_inner).full_write_buffer
            }));
        }

        // We can pop previously peeked request.
        let popped = unsafe { (*q_inner).pop() };
        debug_assert!(!popped.is_null());

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let action = cb(
            userdata_value::<UD>(unsafe { (*req_inner).userdata }),
            l_inner,
            c_inner,
            cb_res.writer,
            unsafe { (*req_inner).full_write_buffer },
            result,
        );

        // Rearm requeues this request at the end of the queue.
        if action == CallbackAction::Rearm {
            unsafe { (*q_inner).push(req_inner) };
        }

        // If we have another request, add that completion next.
        let req_next = unsafe { (*q_inner).head };
        if !req_next.is_null() {
            unsafe { (*l_inner).add(ptr::addr_of_mut!((*req_next).completion)) };
        }

        // We always disarm because the completion in the next request will be used.
        CallbackAction::Disarm
    }

    /// Write to the stream. This performs a single write (NOT queued).
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the write.
    pub unsafe fn write<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            self.write_init(c, buf);
            (*c).userdata = userdata.cast::<c_void>();
            (*c).callback = Self::write_trampoline::<UD, CB>;
            (*loop_).add(c);
        }
    }

    fn write_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let cb_res = Stream::write_result(c_inner, r);
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            cb_res.writer,
            cb_res.buf,
            cb_res.result,
        )
    }

    /// Extracts the result from a completion for a write callback (`.write` arm).
    fn write_result(c: *mut Completion, r: Result) -> WriteResult {
        let (fd, buffer) = match unsafe { &(*c).op } {
            Operation::Write { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let result: core::result::Result<usize, WriteError> = match r {
            Result::Write(res) => res,
            _ => unreachable!(),
        };
        WriteResult {
            writer: Stream::init_fd(fd),
            buf: buffer,
            result,
        }
    }

    /// Initialize the completion c for a write. This does NOT set userdata or a
    /// callback.
    //
    // PORT NOTE: epoll arm sets `c.flags.dup = true` (options.threadpool==false).
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn write_init(self, c: *mut Completion, buf: WriteBuffer) {
        unsafe {
            *c = Completion::new_op(
                Operation::Write {
                    fd: self.fd,
                    buffer: buf,
                },
                ptr::null_mut(),
                noop_callback,
            );
            Completion::set_dup(c, true);
        }
    }

    /// Returns the length of the write buffer.
    fn write_buffer_length(buf: WriteBuffer) -> usize {
        match buf {
            WriteBuffer::Slice(slice) => slice.len(),
            WriteBuffer::Array { len, .. } => len,
        }
    }

    /// Given a `WriteBuffer` and number of bytes written during the previous
    /// write operation, returns a new `WriteBuffer` with remaining data.
    fn write_buffer_remainder(buf: WriteBuffer, offset: usize) -> WriteBuffer {
        match buf {
            WriteBuffer::Slice(slice) => {
                assert!(offset <= slice.len());
                let new_ptr = unsafe { slice.cast::<u8>().add(offset) };
                WriteBuffer::Slice(ptr::slice_from_raw_parts(new_ptr, slice.len() - offset))
            }
            WriteBuffer::Array { array, len } => {
                assert!(offset <= len);
                let rem_len = len - offset;
                let mut wb_array = [0u8; 32];
                wb_array[0..rem_len].copy_from_slice(&array[offset..offset + rem_len]);
                WriteBuffer::Array {
                    array: wb_array,
                    len: rem_len,
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // Closeable (stream.zig:319). `options.close == true` for GenericStream.
    // -------------------------------------------------------------------

    /// Close the socket.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn close<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::close cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Close { fd: self.fd },
                userdata.cast::<c_void>(),
                Self::close_trampoline::<UD, CB>,
            );
            // epoll arm: `c.flags.threadpool = true`.
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

    fn close_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Close { fd } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), CloseError> = match r {
            Result::Close(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            Stream::init_fd(fd),
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:139). `options.poll == true` for GenericStream.
    // -------------------------------------------------------------------

    /// Poll the file descriptor for the given event.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn poll<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        _event: PollEvent,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Stream::poll cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // epoll arm: `.poll = { .fd, .events = EPOLL.IN }` (the `.read` event).
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    events: libc::EPOLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::poll_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn poll_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                Stream,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Poll { fd, .. } => *fd,
            _ => unreachable!(),
        };
        // epoll arm: `if (result.poll) |_| @enumFromInt(c.op.poll.events) else |err| err`.
        // GenericStream always polls EPOLL.IN, which maps to `PollEvent::Read`.
        let mapped: core::result::Result<PollEvent, PollError> = match r {
            Result::Poll(Ok(())) => Ok(PollEvent::Read),
            Result::Poll(Err(e)) => Err(e),
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            Stream::init_fd(fd),
            mapped,
        )
    }
}

/// PORT NOTE: the anonymous return struct of `write_result`.
struct WriteResult {
    writer: Stream,
    buf: WriteBuffer,
    result: core::result::Result<usize, WriteError>,
}

// ---------------------------------------------------------------------------
// TCP (watcher/tcp.zig `TCPStream`, tcp.zig:22-244).
//
// PORT NOTE: `xev.TCP` = a socket fd over `stream.Stream(xev, Self, .{ .close =
// true, .poll = true, .read = .recv, .write = .send })` PLUS the
// Accept/Connect/Shutdown ops. This is the per-backend (`epoll`) instantiation of
// the comptime `TCP(xev)` — the Rust shape of libxev's `if (xev.dynamic) return
// TCPDynamic(xev)` else `TCPStream(xev)` split (tcp.zig:17-20); the Dynamic arm
// lives in `dynamic.rs`.
//
// PORT NOTE (the Stream monomorphization): the crate's `Stream` (above) is the
// GenericStream instantiation (`.read = .read, .write = .write`); TCP is the
// SAME stream generic instantiated with `.read = .recv, .write = .send`, so its
// `read`/`write`/`queue_write` reproduce the `Stream` bodies verbatim but emit
// `Operation::Recv`/`Operation::Send` (and map `Result::Recv`/`Result::Send`).
// `close`/`poll` are identical to `Stream`'s (the op is fd-only) except the
// callback hands back a `TCP` instead of a `Stream`. The buffer-remainder helpers
// (`Stream::write_buffer_length`/`write_buffer_remainder`) are backend-shared and
// reused directly (same module).
// ---------------------------------------------------------------------------

/// TCP client and server.
///
/// This is a "higher-level abstraction" in libxev. The goal of higher-level
/// abstractions in libxev are to make it easier to use specific functionality
/// with the event loop, but does not promise perfect flexibility or optimal
/// performance. In almost all cases, the abstraction is good enough. But,
/// if you have specific needs or want to push for the most optimal performance,
/// use the platform-specific Loop directly.
//
// PORT NOTE: `#[derive(Clone, Copy)]` — the reference `TCP` is a bare `fd`
// struct passed BY VALUE everywhere (`accept(self: Self)`, `?Self` in the tests).
#[derive(Clone, Copy)]
pub struct TCP {
    fd: socket_t,
}

impl TCP {
    /// Initialize a new TCP with the family from the given address. Only
    /// the family is used, the actual address has no impact on the created
    /// resource.
    //
    // PORT NOTE: the `xev.backend == .io_uring` arm OMITS `SOCK.NONBLOCK` ("On
    // io_uring we don't use non-blocking sockets because we may just get EAGAIN
    // over and over from completions"); this is the epoll arm, which keeps it.
    pub fn init(addr: net::Address) -> core::result::Result<TCP, Error> {
        // SAFETY: reading `any.family` is valid for any active `Address` member
        // (all overlay `family` as their first field).
        let family = unsafe { addr.any }.family as libc::c_int;
        let flags = libc::SOCK_STREAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK;
        let fd = unsafe { libc::socket(family, flags, 0) };
        if fd < 0 {
            return Err(Error);
        }
        Ok(TCP { fd })
    }

    /// Initialize a TCP socket from a file descriptor.
    pub fn init_fd(fd: socket_t) -> TCP {
        TCP { fd }
    }

    /// The underlying socket file descriptor.
    //
    // PORT NOTE: the reference exposes `fd` as a public FIELD (`tcp.zig:27`); the
    // Rust field stays private (mirroring `Stream`) and this accessor stands in —
    // it is what the Dynamic wrapper's `fd()` and `getsockname` in the tests reach.
    pub fn fd(self) -> socket_t {
        self.fd
    }

    /// Bind the address to the socket.
    pub fn bind(self, addr: net::Address) -> core::result::Result<(), Error> {
        let one: libc::c_int = 1;
        // SAFETY: `self.fd` is a live socket; `addr.any` overlays a valid sockaddr
        // for the bound length.
        unsafe {
            if libc::setsockopt(
                self.fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                ptr::addr_of!(one).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            ) != 0
            {
                return Err(Error);
            }
            if libc::bind(
                self.fd,
                ptr::addr_of!(addr.any) as *const libc::sockaddr,
                addr.get_os_sock_len(),
            ) != 0
            {
                return Err(Error);
            }
        }
        Ok(())
    }

    /// Listen for connections on the socket. This puts the socket into passive
    /// listening mode. Connections must still be accepted one at a time.
    //
    // PORT NOTE: the reference `backlog` is `u31` (Zig); Rust has no `u31`, so it
    // is the `c_int` `libc::listen` wants (a `u31` always fits `c_int`).
    pub fn listen(self, backlog: libc::c_int) -> core::result::Result<(), Error> {
        // SAFETY: `self.fd` is a live socket.
        if unsafe { libc::listen(self.fd, backlog) } != 0 {
            return Err(Error);
        }
        Ok(())
    }

    /// Accept a single connection.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn accept<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<TCP, AcceptError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::accept cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // The reference relies on the Zig `.accept` field defaults
            // (`addr = undefined`, `addr_size = @sizeOf(posix.sockaddr)`,
            // `flags = posix.SOCK.CLOEXEC`); spelled at the call site here.
            *c = Completion::new_op(
                Operation::Accept {
                    socket: self.fd,
                    addr: core::mem::zeroed(),
                    addr_size: size_of::<net::sockaddr::Any>() as net::socklen_t,
                    flags: SOCK_CLOEXEC,
                },
                userdata.cast::<c_void>(),
                Self::accept_trampoline::<UD, CB>,
            );
            // If we're dup-ing, then we ask the backend to manage the fd.
            // (epoll arm: `c.flags.dup = true`.)
            Completion::set_dup(c, true);
            (*loop_).add(c);
        }
    }

    fn accept_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<TCP, AcceptError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // if (r.accept) |fd| initFd(fd) else |err| err
        let mapped: core::result::Result<TCP, AcceptError> = match r {
            Result::Accept(Ok(fd)) => Ok(TCP::init_fd(fd)),
            Result::Accept(Err(e)) => Err(e),
            _ => unreachable!(),
        };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
    }

    /// Establish a connection as a client.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn connect<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        addr: net::Address,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<(), ConnectError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::connect cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Connect {
                    socket: self.fd,
                    addr,
                },
                userdata.cast::<c_void>(),
                Self::connect_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn connect_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<(), ConnectError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // initFd(c_inner.op.connect.socket), if (r.connect) |_| {} else |err| err
        let socket = match unsafe { &(*c_inner).op } {
            Operation::Connect { socket, .. } => *socket,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), ConnectError> = match r {
            Result::Connect(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            TCP::init_fd(socket),
            mapped,
        )
    }

    /// Shutdown the socket. This always only shuts down the writer side. You
    /// can use the lower level interface directly to control this if the
    /// platform supports it.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn shutdown<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<(), ShutdownError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::shutdown cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Shutdown {
                    socket: self.fd,
                    how: ShutdownHow::Send,
                },
                userdata.cast::<c_void>(),
                Self::shutdown_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn shutdown_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<(), ShutdownError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // initFd(c_inner.op.shutdown.socket), if (r.shutdown) |_| {} else |err| err
        let socket = match unsafe { &(*c_inner).op } {
            Operation::Shutdown { socket, .. } => *socket,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), ShutdownError> = match r {
            Result::Shutdown(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            TCP::init_fd(socket),
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:443). `options.read == .recv` for TCP.
    // -------------------------------------------------------------------

    /// Read from the socket. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the read.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn read<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: ReadBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::read cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // `options.read == .recv` ⇒ the `.recv` op.
            *c = Completion::new_op(
                Operation::Recv {
                    fd: self.fd,
                    buffer: buf,
                },
                userdata.cast::<c_void>(),
                Self::read_trampoline::<UD, CB>,
            );
            // epoll arm: `if (options.threadpool) threadpool=true else dup=true`.
            // TCP `options.threadpool == false` ⇒ dup.
            Completion::set_dup(c, true);
            (*loop_).add(c);
        }
    }

    fn read_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (fd, buffer) = match unsafe { &(*c_inner).op } {
            Operation::Recv { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::Recv(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            TCP::init_fd(fd),
            buffer,
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:620). `options.write == .send` for TCP.
    // -------------------------------------------------------------------

    /// Write to the stream. This queues the writes to ensure they remain in
    /// order.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks.
    pub unsafe fn queue_write<UD, CB>(
        self,
        loop_: *mut Loop,
        q: *mut WriteQueue,
        req: *mut WriteRequest,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::queue_write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // req.* = .{ .full_write_buffer = buf };
            *req = WriteRequest {
                full_write_buffer: buf,
                ..WriteRequest::default()
            };
            // Must be kept in sync with partial write logic inside the callback
            self.write_init(ptr::addr_of_mut!((*req).completion), buf);
            (*req).completion.userdata = q as *mut c_void;
            (*req).completion.callback = Self::queue_write_trampoline::<UD, CB>;

            // The userdata as to go on the WriteRequest because we need
            // our actual completion userdata to be the WriteQueue.
            (*req).userdata = userdata.cast::<c_void>();

            // If the queue is empty, then we add our completion.
            if (*q).empty() {
                (*loop_).add(ptr::addr_of_mut!((*req).completion));
            }

            // We always add this item to our queue no matter what
            (*q).push(req);
        }
    }

    /// The ordered-write engine trampoline — chains completions through the
    /// `WriteQueue` and handles partial writes by re-`write_init`-ing the remainder.
    fn queue_write_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let q_inner = ud as *mut WriteQueue;
        debug_assert!(!q_inner.is_null());

        // We only peek the request here (not pop) because we may need to rearm
        // this write if the write was partial.
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).head };
        debug_assert!(!req_inner.is_null());

        let (writer, cb_buf, cb_result) = TCP::write_result(c_inner, r);
        let mut result: core::result::Result<usize, WriteError> = cb_result;

        // Checks whether the entire buffer was written; if the write was partial,
        // it re-submits the remainder of the buffer. (`write_buffer_length`/
        // `write_buffer_remainder` are backend-shared with `Stream`.)
        let queued_len = Stream::write_buffer_length(cb_buf);
        if let Ok(written_len) = cb_result {
            if written_len < queued_len {
                // Write remainder of the buffer, reusing the same completion.
                let rem_buf = Stream::write_buffer_remainder(cb_buf, written_len);
                unsafe {
                    writer.write_init(ptr::addr_of_mut!((*req_inner).completion), rem_buf);
                    (*req_inner).completion.userdata = q_inner as *mut c_void;
                    (*req_inner).completion.callback = Self::queue_write_trampoline::<UD, CB>;
                    (*l_inner).add(ptr::addr_of_mut!((*req_inner).completion));
                }
                return CallbackAction::Disarm;
            }

            // We wrote the entire buffer; report the full length to the caller.
            result = Ok(Stream::write_buffer_length(unsafe {
                (*req_inner).full_write_buffer
            }));
        }

        // We can pop previously peeked request.
        let popped = unsafe { (*q_inner).pop() };
        debug_assert!(!popped.is_null());

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let action = cb(
            userdata_value::<UD>(unsafe { (*req_inner).userdata }),
            l_inner,
            c_inner,
            writer,
            unsafe { (*req_inner).full_write_buffer },
            result,
        );

        // Rearm requeues this request at the end of the queue.
        if action == CallbackAction::Rearm {
            unsafe { (*q_inner).push(req_inner) };
        }

        // If we have another request, add that completion next.
        let req_next = unsafe { (*q_inner).head };
        if !req_next.is_null() {
            unsafe { (*l_inner).add(ptr::addr_of_mut!((*req_next).completion)) };
        }

        // We always disarm because the completion in the next request will be used.
        CallbackAction::Disarm
    }

    /// Write to the stream. This performs a single write (NOT queued).
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the write.
    pub unsafe fn write<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            self.write_init(c, buf);
            (*c).userdata = userdata.cast::<c_void>();
            (*c).callback = Self::write_trampoline::<UD, CB>;
            (*loop_).add(c);
        }
    }

    fn write_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (writer, buf, result) = TCP::write_result(c_inner, r);
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            writer,
            buf,
            result,
        )
    }

    /// Extracts the result from a completion for a write callback (`.send` arm).
    fn write_result(
        c: *mut Completion,
        r: Result,
    ) -> (TCP, WriteBuffer, core::result::Result<usize, WriteError>) {
        let (fd, buffer) = match unsafe { &(*c).op } {
            Operation::Send { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let result: core::result::Result<usize, WriteError> = match r {
            Result::Send(res) => res,
            _ => unreachable!(),
        };
        (TCP::init_fd(fd), buffer, result)
    }

    /// Initialize the completion c for a write. This does NOT set userdata or a
    /// callback.
    //
    // PORT NOTE: `options.write == .send` ⇒ the `.send` op. epoll arm sets
    // `c.flags.dup = true` (`options.threadpool == false`).
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn write_init(self, c: *mut Completion, buf: WriteBuffer) {
        unsafe {
            *c = Completion::new_op(
                Operation::Send {
                    fd: self.fd,
                    buffer: buf,
                },
                ptr::null_mut(),
                noop_callback,
            );
            Completion::set_dup(c, true);
        }
    }

    // -------------------------------------------------------------------
    // Closeable (stream.zig:319). `options.close == true` for TCP.
    // -------------------------------------------------------------------

    /// Close the socket.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn close<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::close cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Close { fd: self.fd },
                userdata.cast::<c_void>(),
                Self::close_trampoline::<UD, CB>,
            );
            // epoll arm: `c.flags.threadpool = true`.
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

    fn close_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Close { fd } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), CloseError> = match r {
            Result::Close(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            TCP::init_fd(fd),
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:139). `options.poll == true` for TCP.
    // -------------------------------------------------------------------

    /// Poll the file descriptor for the given event.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn poll<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        _event: PollEvent,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "TCP::poll cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    events: libc::EPOLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::poll_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn poll_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                TCP,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Poll { fd, .. } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<PollEvent, PollError> = match r {
            Result::Poll(Ok(())) => Ok(PollEvent::Read),
            Result::Poll(Err(e)) => Err(e),
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            TCP::init_fd(fd),
            mapped,
        )
    }
}

// ---------------------------------------------------------------------------
// UDP (watcher/udp.zig `UDPSendMsg`) — the per-backend (`epoll`) instantiation of
// the datagram socket. On both Linux backends `UDP(xev)` selects `UDPSendMsg`
// (udp.zig:20-24), which uses `sendmsg`/`recvmsg` exclusively; this is the epoll
// arm (the io_uring arm is byte-identical apart from `init`'s NONBLOCK and the
// dup/threadpool flag switches). The Dynamic arm lives in `dynamic.rs`.
//
// PORT NOTE (ownership shape): the msghdr + its iovec + the address storage are
// embedded in the caller-owned `UdpState` (udp.zig:405-422) and the msghdr's
// `msg_name`/`msg_iov` point back INTO that same `UdpState` — a self-referential
// struct. `read`/`write` therefore write `*s` FIRST (establishing stable field
// addresses) and only then fix up those interior pointers; the caller must keep
// `s` pinned for the whole in-flight op (exactly the Zig `s: *State` contract).
// `close`/`poll` reproduce `Stream`'s bodies (fd-only ops) but hand back a `UDP`.
// ---------------------------------------------------------------------------

/// UDP requires some extra state to perform operations. The state is
/// opaque. This isn't part of `Completion` because it is relatively
/// large and would force ALL operations (not just UDP) to have a relatively
/// large structure size and we didn't want to pay that cost.
//
// PORT NOTE: the reference `State.op` is an UNTAGGED `union { recv, send }` whose
// active arm is tracked by the in-flight operation; Rust models it as a safe
// tagged `enum` (`UdpStateOp`) — the discriminant is the same information Zig's
// `s.op = .{ .recv = undefined }` tag-set line encodes. `userdata` is the
// reference's `?*anyopaque = null` (set by `read`/`write`).
pub struct UdpState {
    userdata: *mut c_void,
    op: UdpStateOp,
}

// PORT NOTE: `msghdr`/`iov` for the recv arm are the writable `posix.msghdr`/
// `iovec`; the send arm's are `posix.msghdr_const`/`iovec_const`. `libc` has a
// single `msghdr`/`iovec` (constness rides the pointer in C/Rust), so both arms
// name `libc::msghdr`/`libc::iovec` and the send path only ever READS through them.
enum UdpStateOp {
    Recv {
        buf: ReadBuffer,
        addr_buffer: libc::sockaddr_storage,
        msghdr: libc::msghdr,
        iov: [libc::iovec; 1],
    },
    Send {
        buf: WriteBuffer,
        addr: net::Address,
        msghdr: libc::msghdr,
        iov: [libc::iovec; 1],
    },
}

impl UdpState {
    /// A throwaway `UdpState` used only to establish a valid Rust value before
    /// `read`/`write` overwrite it wholesale. Mirrors the reference's
    /// `var s: State = undefined;` (the content is never observed).
    pub fn undefined() -> UdpState {
        UdpState {
            userdata: ptr::null_mut(),
            // SAFETY: `sockaddr_storage`/`msghdr`/`iovec` are POD; all-zero is a
            // valid bit pattern. `ReadBuffer::Slice(empty)` is a valid enum value.
            op: UdpStateOp::Recv {
                buf: ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(ptr::null_mut(), 0)),
                addr_buffer: unsafe { core::mem::zeroed() },
                msghdr: unsafe { core::mem::zeroed() },
                iov: [libc::iovec {
                    iov_base: ptr::null_mut(),
                    iov_len: 0,
                }],
            },
        }
    }
}

/// UDP client and server.
///
/// This is a "higher-level abstraction" in libxev. The goal of higher-level
/// abstractions in libxev are to make it easier to use specific functionality
/// with the event loop, but does not promise perfect flexibility or optimal
/// performance. In almost all cases, the abstraction is good enough. But,
/// if you have specific needs or want to push for the most optimal performance,
/// use the platform-specific Loop directly.
//
// PORT NOTE: `#[derive(Clone, Copy)]` — the reference `UDP` is a bare `fd` struct
// passed BY VALUE everywhere (`bind(self)`, `Impl` in the tests).
#[derive(Clone, Copy)]
pub struct UDP {
    fd: socket_t,
}

impl UDP {
    /// Initialize a new UDP with the family from the given address. Only
    /// the family is used, the actual address has no impact on the created
    /// resource.
    //
    // PORT NOTE: `if (xev.backend != .io_uring) flags |= posix.SOCK.NONBLOCK` —
    // this is the epoll arm, which KEEPS NONBLOCK (io_uring omits it).
    pub fn init(addr: net::Address) -> core::result::Result<UDP, Error> {
        // SAFETY: reading `any.family` is valid for any active `Address` member
        // (all overlay `family` as their first field).
        let family = unsafe { addr.any }.family as libc::c_int;
        let flags = libc::SOCK_DGRAM | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK;
        let fd = unsafe { libc::socket(family, flags, 0) };
        if fd < 0 {
            return Err(Error);
        }
        Ok(UDP { fd })
    }

    /// Initialize a UDP socket from a file descriptor.
    pub fn init_fd(fd: socket_t) -> UDP {
        UDP { fd }
    }

    /// The underlying socket file descriptor.
    //
    // PORT NOTE: the reference exposes `fd` as a public FIELD; the Rust field stays
    // private (mirroring `TCP`) and this accessor stands in — it is what the Dynamic
    // wrapper's `fd()` and `getsockname`/`setsockopt` in the tests reach.
    pub fn fd(self) -> socket_t {
        self.fd
    }

    /// Bind the address to the socket.
    pub fn bind(self, addr: net::Address) -> core::result::Result<(), Error> {
        let one: libc::c_int = 1;
        // SAFETY: `self.fd` is a live socket; `addr.any` overlays a valid sockaddr
        // for the bound length.
        unsafe {
            if libc::setsockopt(
                self.fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                ptr::addr_of!(one).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            ) != 0
            {
                return Err(Error);
            }
            if libc::setsockopt(
                self.fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                ptr::addr_of!(one).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            ) != 0
            {
                return Err(Error);
            }
            if libc::bind(
                self.fd,
                ptr::addr_of!(addr.any) as *const libc::sockaddr,
                addr.get_os_sock_len(),
            ) != 0
            {
                return Err(Error);
            }
        }
        Ok(())
    }

    /// Read from the socket. This performs a single read. The callback must
    /// requeue the read if additional reads want to be performed. Additional
    /// reads simultaneously can be queued by calling this multiple times. Note
    /// that depending on the backend, the reads can happen out of order.
    ///
    /// # Safety
    /// `c`/`loop_`/`s` valid+stable (held raw by the loop); `s` must outlive the
    /// op (the msghdr embedded in it points back into it); `buf` (if a slice)
    /// outlives the read — the KERNEL writes it asynchronously.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn read<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        s: *mut UdpState,
        buf: ReadBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                *mut UdpState,
                net::Address,
                UDP,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "UDP::read cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // s.* = .{ .userdata, .op = .{ .recv = .{ ... } } }; — write the State
            // FIRST so its interior addresses are stable, then patch the self-refs.
            *s = UdpState {
                userdata: userdata.cast::<c_void>(),
                op: UdpStateOp::Recv {
                    buf,
                    addr_buffer: core::mem::zeroed(),
                    msghdr: core::mem::zeroed(),
                    iov: [libc::iovec {
                        iov_base: ptr::null_mut(),
                        iov_len: 0,
                    }],
                },
            };

            let UdpStateOp::Recv {
                buf,
                addr_buffer,
                msghdr,
                iov,
            } = &mut (*s).op
            else {
                unreachable!()
            };

            match buf {
                ReadBuffer::Slice(v) => {
                    iov[0] = libc::iovec {
                        iov_base: (*v).cast::<u8>() as *mut c_void,
                        iov_len: (*v).len(),
                    };
                }
                ReadBuffer::Array(arr) => {
                    iov[0] = libc::iovec {
                        iov_base: arr.as_mut_ptr().cast::<c_void>(),
                        iov_len: arr.len(),
                    };
                }
            }

            *msghdr = core::mem::zeroed::<libc::msghdr>();
            msghdr.msg_name = ptr::addr_of_mut!(*addr_buffer).cast::<c_void>();
            msghdr.msg_namelen = size_of::<libc::sockaddr_storage>() as net::socklen_t;
            msghdr.msg_iov = iov.as_mut_ptr();
            msghdr.msg_iovlen = 1;

            let msghdr_ptr: *mut libc::msghdr = ptr::addr_of_mut!(*msghdr);

            *c = Completion::new_op(
                Operation::RecvMsg {
                    fd: self.fd,
                    msghdr: msghdr_ptr,
                },
                s as *mut c_void,
                Self::read_trampoline::<UD, CB>,
            );
            // epoll arm of the dup switch: `c.flags.dup = true`.
            Completion::set_dup(c, true);
            (*loop_).add(c);
        }
    }

    fn read_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                *mut UdpState,
                net::Address,
                UDP,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // ud is the *State (the Completion's userdata).
        let s_inner = ud as *mut UdpState;
        let fd = match unsafe { &(*c_inner).op } {
            Operation::RecvMsg { fd, .. } => *fd,
            _ => unreachable!(),
        };
        let (addr, buf) = match unsafe { &(*s_inner).op } {
            UdpStateOp::Recv {
                buf, addr_buffer, ..
            } => (
                // std.net.Address.initPosix(@ptrCast(&s_inner.op.recv.addr_buffer))
                unsafe {
                    net::Address::init_posix(
                        ptr::addr_of!(*addr_buffer).cast::<net::sockaddr::Any>(),
                    )
                },
                *buf,
            ),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::RecvMsg(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(unsafe { (*s_inner).userdata }),
            l_inner,
            c_inner,
            s_inner,
            addr,
            UDP::init_fd(fd),
            buf,
            mapped,
        )
    }

    /// Write to the socket. This performs a single write. Additional writes
    /// can be queued by calling this multiple times. Note that depending on the
    /// backend, writes can happen out of order.
    ///
    /// # Safety
    /// `c`/`loop_`/`s` valid+stable (held raw by the loop); `s` must outlive the
    /// op (the msghdr embedded in it points back into it); `buf` (if a slice)
    /// outlives the write — the KERNEL reads it asynchronously.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn write<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        s: *mut UdpState,
        addr: net::Address,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                *mut UdpState,
                UDP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "UDP::write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // s.* = .{ .userdata, .op = .{ .send = .{ ... } } }; — write FIRST, then
            // patch the msghdr's self-references (msg_name → s.op.send.addr, msg_iov
            // → s.op.send.iov).
            *s = UdpState {
                userdata: userdata.cast::<c_void>(),
                op: UdpStateOp::Send {
                    buf,
                    addr,
                    msghdr: core::mem::zeroed(),
                    iov: [libc::iovec {
                        iov_base: ptr::null_mut(),
                        iov_len: 0,
                    }],
                },
            };

            let UdpStateOp::Send {
                buf,
                addr,
                msghdr,
                iov,
            } = &mut (*s).op
            else {
                unreachable!()
            };

            match buf {
                WriteBuffer::Slice(v) => {
                    iov[0] = libc::iovec {
                        iov_base: (*v).cast::<u8>() as *mut c_void,
                        iov_len: (*v).len(),
                    };
                }
                WriteBuffer::Array { array, len } => {
                    iov[0] = libc::iovec {
                        iov_base: array.as_mut_ptr().cast::<c_void>(),
                        iov_len: *len,
                    };
                }
            }

            let namelen = addr.get_os_sock_len();
            *msghdr = core::mem::zeroed::<libc::msghdr>();
            msghdr.msg_name = ptr::addr_of_mut!(addr.any).cast::<c_void>();
            msghdr.msg_namelen = namelen;
            msghdr.msg_iov = iov.as_mut_ptr();
            msghdr.msg_iovlen = 1;

            let msghdr_ptr: *const libc::msghdr = ptr::addr_of!(*msghdr);

            *c = Completion::new_op(
                Operation::SendMsg {
                    fd: self.fd,
                    msghdr: msghdr_ptr,
                    // Zig field defaults: `buffer = null`, `iov = undefined`.
                    buffer: None,
                    iov: [libc::iovec {
                        iov_base: ptr::null_mut(),
                        iov_len: 0,
                    }],
                },
                s as *mut c_void,
                Self::write_trampoline::<UD, CB>,
            );
            // epoll arm of the dup switch: `c.flags.dup = true`.
            Completion::set_dup(c, true);
            (*loop_).add(c);
        }
    }

    fn write_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                *mut UdpState,
                UDP,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let s_inner = ud as *mut UdpState;
        let fd = match unsafe { &(*c_inner).op } {
            Operation::SendMsg { fd, .. } => *fd,
            _ => unreachable!(),
        };
        let buf = match unsafe { &(*s_inner).op } {
            UdpStateOp::Send { buf, .. } => *buf,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, WriteError> = match r {
            Result::SendMsg(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(unsafe { (*s_inner).userdata }),
            l_inner,
            c_inner,
            s_inner,
            UDP::init_fd(fd),
            buf,
            mapped,
        )
    }

    /// Close the socket.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn close<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                UDP,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "UDP::close cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Close { fd: self.fd },
                userdata.cast::<c_void>(),
                Self::close_trampoline::<UD, CB>,
            );
            // epoll arm: `c.flags.threadpool = true`.
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

    fn close_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                UDP,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Close { fd } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), CloseError> = match r {
            Result::Close(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            UDP::init_fd(fd),
            mapped,
        )
    }

    /// Poll the file descriptor for the given event.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn poll<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        _event: PollEvent,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                UDP,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "UDP::poll cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    events: libc::EPOLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::poll_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn poll_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                UDP,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Poll { fd, .. } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<PollEvent, PollError> = match r {
            Result::Poll(Ok(())) => Ok(PollEvent::Read),
            Result::Poll(Err(e)) => Err(e),
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            UDP::init_fd(fd),
            mapped,
        )
    }
}

// ---------------------------------------------------------------------------
// File (watcher/file.zig `FileStream`, file.zig:33-317) — the per-backend
// (`epoll`) instantiation of the regular-file watcher.
//
// PORT NOTE: `xev.File` = a regular-fd handle over `stream.Stream(xev, Self, .{
// .close = true, .poll = true, .read = .read, .write = .write, .threadpool = true
// })` PLUS the pread/pwrite ops. This is the epoll arm of the comptime `File(xev)`
// split (`if (xev.dynamic) return FileDynamic(xev) else FileStream(xev)`,
// file.zig:27-30); the Dynamic arm lives in `dynamic.rs`.
//
// PORT NOTE (the Stream monomorphization): File is the SAME stream generic as the
// crate's `Stream`/`TCP`, instantiated with `.read = .read, .write = .write` (so
// the ops are `Operation::Read`/`Operation::Write`, like `Stream`) but with
// `.threadpool = true`. On epoll that ONE option flips the Readable/Writeable
// flag switch (`if (options.threadpool) c.flags.threadpool = true else c.flags.dup
// = true`): File's read/write set `threadpool` (routing them through the pool as
// BLOCKING ops), where `Stream`/`TCP` set `dup`. `close` is byte-identical to
// `Stream`'s (already `threadpool`); `poll` is identical (fd-only, no flag) except
// the callback hands back a `File`. The pread/pwrite ops are File-only additions —
// `.read`/`.write` with an `offset`, always threadpool-flagged (file.zig:131/302).
// The buffer helpers (`Stream::write_buffer_length`/`write_buffer_remainder`) are
// backend-shared and reused directly.
// ---------------------------------------------------------------------------

/// File operations.
///
/// These operations typically run on the event loop thread pool, rather
/// than the core async OS APIs, because most core async OS APIs don't support
/// async operations on regular files (with many caveats attached to that
/// statement). This high-level abstraction will attempt to make the right
/// decision about what to do but this should generally be used by
/// operations that need to run on a thread pool. For operations that you're
/// sure are better supported by core async OS APIs (such as sockets, pipes,
/// TTYs, etc.), use a specific high-level abstraction like xev.TCP or
/// the generic xev.Stream.
//
// PORT NOTE: `#[derive(Clone, Copy)]` — the reference `File` is a bare `fd` struct
// passed BY VALUE everywhere (`read(self: Self)`, `?Self` in the tests).
#[derive(Clone, Copy)]
pub struct File {
    /// The underlying file
    fd: fd_t,
}

impl File {
    /// Initialize a File from a file descriptor.
    //
    // PORT NOTE: `pub fn init(file: std.fs.File) !Self { return .{ .fd =
    // file.handle }; }`. Rust has no `std.fs.File` handle-wrapper worth threading
    // through this crate; the caller passes the raw fd (its `std.fs.File.handle`).
    // The reference's `!Self` is vestigial (the body cannot error), so this is
    // infallible.
    pub fn init(fd: fd_t) -> File {
        File { fd }
    }

    /// Initialize a File from a file descriptor.
    pub fn init_fd(fd: fd_t) -> File {
        File { fd }
    }

    /// Clean up any watcher resources. This does NOT close the file.
    /// If you want to close the file you must call close or do so
    /// synchronously.
    pub fn deinit(&self) {
        let _ = self;
    }

    // -------------------------------------------------------------------
    // Readable (stream.zig:443). `options.read == .read` for File.
    // -------------------------------------------------------------------

    /// Read from the file. This performs a single read.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the read.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn read<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: ReadBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::read cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Read {
                    fd: self.fd,
                    buffer: buf,
                },
                userdata.cast::<c_void>(),
                Self::read_trampoline::<UD, CB>,
            );
            // epoll arm: `if (options.threadpool) threadpool=true else dup=true`.
            // File `options.threadpool == true` ⇒ threadpool.
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

    fn read_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (fd, buffer) = match unsafe { &(*c_inner).op } {
            Operation::Read { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::Read(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            File::init_fd(fd),
            buffer,
            mapped,
        )
    }

    /// Read from the file at the given offset.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the read.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn pread<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: ReadBuffer,
        offset: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::pread cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::PRead {
                    fd: self.fd,
                    buffer: buf,
                    offset,
                },
                userdata.cast::<c_void>(),
                Self::pread_trampoline::<UD, CB>,
            );
            // epoll arm: `c.flags.threadpool = true` (file.zig:131).
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

    fn pread_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                ReadBuffer,
                core::result::Result<usize, ReadError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (fd, buffer) = match unsafe { &(*c_inner).op } {
            Operation::PRead { fd, buffer, .. } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let mapped: core::result::Result<usize, ReadError> = match r {
            Result::PRead(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            File::init_fd(fd),
            buffer,
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Writeable (stream.zig:620). `options.write == .write` for File.
    // -------------------------------------------------------------------

    /// Write to the file. This queues the writes to ensure they remain in
    /// order.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks.
    pub unsafe fn queue_write<UD, CB>(
        self,
        loop_: *mut Loop,
        q: *mut WriteQueue,
        req: *mut WriteRequest,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::queue_write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // req.* = .{ .full_write_buffer = buf };
            *req = WriteRequest {
                full_write_buffer: buf,
                ..WriteRequest::default()
            };
            // Must be kept in sync with partial write logic inside the callback
            self.write_init(ptr::addr_of_mut!((*req).completion), buf);
            (*req).completion.userdata = q as *mut c_void;
            (*req).completion.callback = Self::queue_write_trampoline::<UD, CB>;

            // The userdata as to go on the WriteRequest because we need
            // our actual completion userdata to be the WriteQueue.
            (*req).userdata = userdata.cast::<c_void>();

            // If the queue is empty, then we add our completion.
            if (*q).empty() {
                (*loop_).add(ptr::addr_of_mut!((*req).completion));
            }

            // We always add this item to our queue no matter what
            (*q).push(req);
        }
    }

    /// The ordered-write engine trampoline — chains completions through the
    /// `WriteQueue` and handles partial writes by re-`write_init`-ing the remainder.
    fn queue_write_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let q_inner = ud as *mut WriteQueue;
        debug_assert!(!q_inner.is_null());

        // We only peek the request here (not pop) because we may need to rearm
        // this write if the write was partial.
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).head };
        debug_assert!(!req_inner.is_null());

        let (writer, cb_buf, cb_result) = File::write_result(c_inner, r);
        let mut result: core::result::Result<usize, WriteError> = cb_result;

        // Checks whether the entire buffer was written; if the write was partial,
        // it re-submits the remainder of the buffer. (`write_buffer_length`/
        // `write_buffer_remainder` are backend-shared with `Stream`.)
        let queued_len = Stream::write_buffer_length(cb_buf);
        if let Ok(written_len) = cb_result {
            if written_len < queued_len {
                // Write remainder of the buffer, reusing the same completion.
                let rem_buf = Stream::write_buffer_remainder(cb_buf, written_len);
                unsafe {
                    writer.write_init(ptr::addr_of_mut!((*req_inner).completion), rem_buf);
                    (*req_inner).completion.userdata = q_inner as *mut c_void;
                    (*req_inner).completion.callback = Self::queue_write_trampoline::<UD, CB>;
                    (*l_inner).add(ptr::addr_of_mut!((*req_inner).completion));
                }
                return CallbackAction::Disarm;
            }

            // We wrote the entire buffer; report the full length to the caller.
            result = Ok(Stream::write_buffer_length(unsafe {
                (*req_inner).full_write_buffer
            }));
        }

        // We can pop previously peeked request.
        let popped = unsafe { (*q_inner).pop() };
        debug_assert!(!popped.is_null());

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let action = cb(
            userdata_value::<UD>(unsafe { (*req_inner).userdata }),
            l_inner,
            c_inner,
            writer,
            unsafe { (*req_inner).full_write_buffer },
            result,
        );

        // Rearm requeues this request at the end of the queue.
        if action == CallbackAction::Rearm {
            unsafe { (*q_inner).push(req_inner) };
        }

        // If we have another request, add that completion next.
        let req_next = unsafe { (*q_inner).head };
        if !req_next.is_null() {
            unsafe { (*l_inner).add(ptr::addr_of_mut!((*req_next).completion)) };
        }

        // We always disarm because the completion in the next request will be used.
        CallbackAction::Disarm
    }

    /// Write to the file. This performs a single write (NOT queued).
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the write.
    pub unsafe fn write<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: WriteBuffer,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::write cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            self.write_init(c, buf);
            (*c).userdata = userdata.cast::<c_void>();
            (*c).callback = Self::write_trampoline::<UD, CB>;
            (*loop_).add(c);
        }
    }

    fn write_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (writer, buf, result) = File::write_result(c_inner, r);
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            writer,
            buf,
            result,
        )
    }

    /// Extracts the result from a completion for a write callback (`.write` arm).
    fn write_result(
        c: *mut Completion,
        r: Result,
    ) -> (File, WriteBuffer, core::result::Result<usize, WriteError>) {
        let (fd, buffer) = match unsafe { &(*c).op } {
            Operation::Write { fd, buffer } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let result: core::result::Result<usize, WriteError> = match r {
            Result::Write(res) => res,
            _ => unreachable!(),
        };
        (File::init_fd(fd), buffer, result)
    }

    /// Initialize the completion c for a write. This does NOT set userdata or a
    /// callback.
    //
    // PORT NOTE: epoll arm sets `c.flags.threadpool = true`
    // (`options.threadpool == true`).
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn write_init(self, c: *mut Completion, buf: WriteBuffer) {
        unsafe {
            *c = Completion::new_op(
                Operation::Write {
                    fd: self.fd,
                    buffer: buf,
                },
                ptr::null_mut(),
                noop_callback,
            );
            Completion::set_threadpool(c, true);
        }
    }

    /// Write to the file at the given offset. This queues the writes to ensure
    /// they remain in order.
    ///
    /// # Safety
    /// `loop_`/`q`/`req` valid+stable, outliving the queued write; the loop holds
    /// `&req.completion` raw across ticks.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn queue_pwrite<UD, CB>(
        self,
        loop_: *mut Loop,
        q: *mut WriteQueue,
        req: *mut WriteRequest,
        buf: WriteBuffer,
        offset: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::queue_pwrite cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            // req.* = .{};
            //
            // Matches the oracle (file.zig:170) exactly: `queuePWrite` default-inits
            // the whole `WriteRequest` and does NOT set `full_write_buffer` (unlike
            // `queueWrite`, which needs it for the remainder-chaining trampoline).
            // Behavior-neutral: `queue_pwrite_trampoline` reads only the op's buffer
            // via `pwrite_result`, never `full_write_buffer` — so `default()` here.
            *req = WriteRequest::default();
            self.pwrite_init(ptr::addr_of_mut!((*req).completion), buf, offset);
            (*req).completion.userdata = q as *mut c_void;
            (*req).completion.callback = Self::queue_pwrite_trampoline::<UD, CB>;

            // The userdata as to go on the WriteRequest because we need
            // our actual completion userdata to be the WriteQueue.
            (*req).userdata = userdata.cast::<c_void>();

            // If the queue is empty, then we add our completion.
            if (*q).empty() {
                (*loop_).add(ptr::addr_of_mut!((*req).completion));
            }

            // We always add this item to our queue no matter what
            (*q).push(req);
        }
    }

    /// The ordered-pwrite engine trampoline. Mirrors `queuePWrite`'s callback
    /// (file.zig:172-208): pops the request, chains the next, but has NO
    /// partial-write re-arm (pwrite requests are one-shot at their offset).
    fn queue_pwrite_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let q_inner = ud as *mut WriteQueue;
        debug_assert!(!q_inner.is_null());

        // The queue MUST have a request because a completion can only be added if
        // the queue is not empty, and nothing else should be popping!
        let req_inner: *mut WriteRequest = unsafe { (*q_inner).pop() };
        debug_assert!(!req_inner.is_null());

        let (writer, buf, result) = File::pwrite_result(c_inner, r);

        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let action = cb(
            userdata_value::<UD>(unsafe { (*req_inner).userdata }),
            l_inner,
            c_inner,
            writer,
            buf,
            result,
        );

        // Rearm requeues this request, it doesn't return rearm on the actual
        // callback here...
        if action == CallbackAction::Rearm {
            unsafe { (*q_inner).push(req_inner) };
        }

        // If we have another request, add that completion next.
        let req_next = unsafe { (*q_inner).head };
        if !req_next.is_null() {
            unsafe { (*l_inner).add(ptr::addr_of_mut!((*req_next).completion)) };
        }

        // We always disarm because the completion in the next request will be used.
        CallbackAction::Disarm
    }

    /// Write to the file at the given offset. This performs a single write.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop); `buf` (if a slice) outlives
    /// the write.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn pwrite<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        buf: WriteBuffer,
        offset: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::pwrite cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            self.pwrite_init(c, buf, offset);
            (*c).userdata = userdata.cast::<c_void>();
            (*c).callback = Self::pwrite_trampoline::<UD, CB>;
            (*loop_).add(c);
        }
    }

    fn pwrite_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                WriteBuffer,
                core::result::Result<usize, WriteError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let (writer, buf, result) = File::pwrite_result(c_inner, r);
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            writer,
            buf,
            result,
        )
    }

    /// Extracts the result from a completion for a pwrite callback (`.pwrite` arm).
    //
    // PORT NOTE: `pwrite_result` (file.zig:264) — the `.pwrite` twin of
    // `write_result`.
    fn pwrite_result(
        c: *mut Completion,
        r: Result,
    ) -> (File, WriteBuffer, core::result::Result<usize, WriteError>) {
        let (fd, buffer) = match unsafe { &(*c).op } {
            Operation::PWrite { fd, buffer, .. } => (*fd, *buffer),
            _ => unreachable!(),
        };
        let result: core::result::Result<usize, WriteError> = match r {
            Result::PWrite(res) => res,
            _ => unreachable!(),
        };
        (File::init_fd(fd), buffer, result)
    }

    /// Initialize the completion c for a pwrite. This does NOT set userdata or a
    /// callback.
    //
    // PORT NOTE: `pwriteInit` (file.zig:276). epoll arm sets `c.flags.threadpool =
    // true` (file.zig:302).
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn pwrite_init(self, c: *mut Completion, buf: WriteBuffer, offset: u64) {
        unsafe {
            *c = Completion::new_op(
                Operation::PWrite {
                    fd: self.fd,
                    buffer: buf,
                    offset,
                },
                ptr::null_mut(),
                noop_callback,
            );
            Completion::set_threadpool(c, true);
        }
    }

    // -------------------------------------------------------------------
    // Closeable (stream.zig:319). `options.close == true` for File.
    // -------------------------------------------------------------------

    /// Close the file.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn close<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::close cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Close { fd: self.fd },
                userdata.cast::<c_void>(),
                Self::close_trampoline::<UD, CB>,
            );
            // epoll arm: `c.flags.threadpool = true`.
            Completion::set_threadpool(c, true);
            (*loop_).add(c);
        }
    }

    fn close_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                core::result::Result<(), CloseError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Close { fd } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<(), CloseError> = match r {
            Result::Close(res) => res,
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            File::init_fd(fd),
            mapped,
        )
    }

    // -------------------------------------------------------------------
    // Pollable (stream.zig:139). `options.poll == true` for File.
    // -------------------------------------------------------------------

    /// Poll the file descriptor for the given event.
    ///
    /// # Safety
    /// `c`/`loop_` valid+stable (held raw by the loop).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn poll<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        _event: PollEvent,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "File::poll cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c = Completion::new_op(
                Operation::Poll {
                    fd: self.fd,
                    events: libc::EPOLLIN as u32,
                },
                userdata.cast::<c_void>(),
                Self::poll_trampoline::<UD, CB>,
            );
            (*loop_).add(c);
        }
    }

    fn poll_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut Loop,
        c_inner: *mut Completion,
        r: Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                File,
                core::result::Result<PollEvent, PollError>,
            ) -> CallbackAction
            + 'static,
    {
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let fd = match unsafe { &(*c_inner).op } {
            Operation::Poll { fd, .. } => *fd,
            _ => unreachable!(),
        };
        let mapped: core::result::Result<PollEvent, PollError> = match r {
            Result::Poll(Ok(())) => Ok(PollEvent::Read),
            Result::Poll(Err(e)) => Err(e),
            _ => unreachable!(),
        };
        cb(
            userdata_value::<UD>(ud),
            l_inner,
            c_inner,
            File::init_fd(fd),
            mapped,
        )
    }
}

// PORT NOTE: `test "Completion size"` (epoll.zig:1380) — the layout tripwire.
//
// PORT NOTE: the reference asserts the Zig `@sizeOf(Completion) == 208`. Rust's
// tagged-enum + struct layout differs FUNDAMENTALLY from Zig's tagged-union
// layout, so the concrete byte size is not 208. The tripwire's PURPOSE — trip on
// any accidental `Completion` bloat — is preserved by pinning the MEASURED Rust
// size; drift in the op union / buffers / error sets will fail this `const`
// assert exactly as the Zig `@compileLog` guard does. (Same divergence as the
// kqueue port's `COMPLETION_SIZE`.)
// Measured Rust size (Zig reference is 208; Rust tagged-enum/struct layout
// differs — see the PORT NOTE above). Pinned so any op-union/buffer/error-set
// drift trips this compile-time guard.
//
// RE-MEASURED (UDP task, after adding the `SendMsg`/`RecvMsg` ops): UNCHANGED at
// 208. `SendMsg` (msghdr ptr 8 + `Option<WriteBuffer>` 48 + `[iovec;1]` 16 + fd 4
// → ~76) stays smaller than `Connect`'s embedded `net::Address` (~112), which
// still dominates the `Operation` union — so the socket-datagram ops add no width.
// (Same result as io_uring.rs's `SendMsg`/`RecvMsg` re-measure.)
//
// RE-MEASURED (Task 4): 104 → 152. The Task-4 socket/proc/stream ops widened the
// `Operation` union: `Connect` embeds `net::Address` (a `sockaddr` union whose
// largest member is `sockaddr_un`, ~112 bytes), which now dominates the tag's
// payload. The `threadpool` flag bit was also added.
//
// RE-MEASURED (Task 2): 152 → 208. Wiring the thread-pool path added the
// per-completion `task: thread_pool::Task` (16), `task_loop: *mut Loop` (8), and
// `task_result: Result` (32 incl. alignment) fields the blocking-op path needs.
// 208 is the new measured Rust size on aarch64-unknown-linux-gnu.
// RE-MEASURED (WS19 T10): 208 → 224. The pool-worker aliasing fix puts the two
// loop-side handles `thread_perform` needs in the completion itself:
// `task_completions: *mut TaskCompletionQueue` (8 — the reference's OWN field,
// restored after the Task-2 elision, so this half moves the Rust size TOWARD
// Zig's) + `task_wakeup: Eventfd` (4, port-added, +4 tail padding). Symmetric with
// kqueue.rs (248 → 264).
const COMPLETION_SIZE: usize = 224;
const _: () = assert!(size_of::<Completion>() == COMPLETION_SIZE);

// ---------------------------------------------------------------------------
// Tests (epoll.zig `test{}` blocks). The L1 LT3 subset: Completion size /
// default completion / loop time / stop / timer x-family / timerfd / cancel x2.
// (socket/process/stream tests are Task 4.) Callbacks are plain fn items
// matching the loop `Callback` ABI; completions/userdata are stack locals
// reached through raw pointers (the intrusive-ownership contract).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::linux::timerfd::Timerfd;
    use core::ffi::c_void;
    use core::ptr;

    // test "Completion size"
    #[test]
    fn epoll_completion_size() {
        // Just so we are aware when we change the size (Rust layout differs from
        // Zig's — the pinned constant is the measured Rust size, see COMPLETION_SIZE).
        assert_eq!(core::mem::size_of::<Completion>(), COMPLETION_SIZE);
    }

    // test "epoll: default completion"
    #[test]
    fn epoll_default_completion() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        let mut c = Completion::default();
        unsafe { loop_.add(ptr::addr_of_mut!(c)) };

        // Tick
        loop_.run(RunMode::UntilDone).unwrap();

        // Completion should be dead.
        assert!(c.state_of() == CompletionState::Dead);

        loop_.deinit();
    }

    // test "epoll: loop time"
    #[test]
    fn epoll_loop_time() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // should never init zero
        let now = loop_.now();
        assert!(now > 0);

        // should update on a loop tick
        while now == loop_.now() {
            loop_.run(RunMode::NoWait).unwrap();
        }

        loop_.deinit();
    }

    // test "epoll: stop"
    #[test]
    fn epoll_stop() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut called = false;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                1_000_000,
                ptr::addr_of_mut!(called).cast::<c_void>(),
                set_true_cb,
            );
        }

        // Tick
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!called);

        // Stop
        loop_.stop();
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(!called);

        loop_.deinit();
    }

    // test "epoll: timer"
    // `called` is mutated only through the raw userdata pointer inside the timer
    // callback (the intrusive-completion contract), which clippy's dataflow can't
    // see — the reference is `while (!called) try loop.run(.no_wait)`.
    #[allow(clippy::while_immutable_condition)]
    #[test]
    fn epoll_timer() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut called = false;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                1,
                ptr::addr_of_mut!(called).cast::<c_void>(),
                set_true_cb,
            );
        }

        // Add another timer
        let mut called2 = false;
        let mut c2 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c2),
                100_000,
                ptr::addr_of_mut!(called2).cast::<c_void>(),
                set_true_cb,
            );
        }

        // State checking
        assert!(c1.state_of() == CompletionState::Active);
        assert!(c2.state_of() == CompletionState::Active);

        // Tick
        while !called {
            loop_.run(RunMode::NoWait).unwrap();
        }
        assert!(called);
        assert!(!called2);

        // State checking
        assert!(c1.state_of() == CompletionState::Dead);
        assert!(c2.state_of() == CompletionState::Active);

        loop_.deinit();
    }

    // Shared callback for the timer-reset family: writes the TimerTrigger.
    fn trigger_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        let v = ud as *mut Option<TimerTrigger>;
        // v.* = r.timer catch unreachable;
        match r {
            Result::Timer(Ok(t)) => unsafe { *v = Some(t) },
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }

    // Sets `*ud = true` on any result. Used by the timer/stop tests.
    fn set_true_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        _r: Result,
    ) -> CallbackAction {
        unsafe { *(ud as *mut bool) = true };
        CallbackAction::Disarm
    }

    // test "epoll: timer reset"
    #[test]
    fn epoll_timer_reset() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut trigger: Option<TimerTrigger> = None;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                100_000,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }

        // We know timer won't be called from the timer test previously.
        loop_.run(RunMode::NoWait).unwrap();
        assert!(trigger.is_none());

        // Reset the timer
        let mut c_cancel = Completion::default();
        unsafe {
            loop_.timer_reset(
                ptr::addr_of_mut!(c1),
                ptr::addr_of_mut!(c_cancel),
                1,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }
        assert!(c1.state_of() == CompletionState::Active);
        assert!(c_cancel.state_of() == CompletionState::Active);

        // Run
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(trigger.unwrap() == TimerTrigger::Expiration);
        assert!(c1.state_of() == CompletionState::Dead);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        loop_.deinit();
    }

    // test "epoll: timer reset before tick"
    #[test]
    fn epoll_timer_reset_before_tick() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut trigger: Option<TimerTrigger> = None;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                100_000,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }

        // Reset the timer
        let mut c_cancel = Completion::default();
        unsafe {
            loop_.timer_reset(
                ptr::addr_of_mut!(c1),
                ptr::addr_of_mut!(c_cancel),
                1,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }
        assert!(c1.state_of() == CompletionState::Active);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        // Run
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(trigger.unwrap() == TimerTrigger::Expiration);
        assert!(c1.state_of() == CompletionState::Dead);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        loop_.deinit();
    }

    // test "epoll: timer reset after trigger"
    #[test]
    fn epoll_timer_reset_after_trigger() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut trigger: Option<TimerTrigger> = None;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                1,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }

        // Run the timer
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(trigger.unwrap() == TimerTrigger::Expiration);
        assert!(c1.state_of() == CompletionState::Dead);
        trigger = None;

        // Reset the timer
        let mut c_cancel = Completion::default();
        unsafe {
            loop_.timer_reset(
                ptr::addr_of_mut!(c1),
                ptr::addr_of_mut!(c_cancel),
                1,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }
        assert!(c1.state_of() == CompletionState::Active);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        // Run
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(trigger.unwrap() == TimerTrigger::Expiration);
        assert!(c1.state_of() == CompletionState::Dead);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        loop_.deinit();
    }

    // test "epoll: timerfd"
    #[test]
    fn epoll_timerfd() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // We'll try with a simple timerfd
        let t = Timerfd::init().unwrap();
        // The reference arms a 1ns timer; our Task-2 `set` is ms-granularity, so
        // we arm the shortest one-shot it exposes (1ms). Observably identical: the
        // timerfd becomes readable near-instantly and `run(until_done)` waits.
        t.set(1).unwrap();

        // Add the timer
        let mut called = false;
        let mut c = Completion {
            op: Operation::Read {
                fd: t.fd,
                buffer: ReadBuffer::Array([0; 32]),
            },
            userdata: ptr::addr_of_mut!(called).cast::<c_void>(),
            callback: read_ok_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c)) };

        // Tick
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(called);

        t.deinit();
        loop_.deinit();
    }

    fn read_ok_cb(
        ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        // _ = r.read catch unreachable;
        match r {
            Result::Read(Ok(_)) => {}
            _ => unreachable!(),
        }
        unsafe { *(ud as *mut bool) = true };
        CallbackAction::Disarm
    }

    fn cancel_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        // _ = r.cancel catch unreachable;
        match r {
            Result::Cancel(Ok(())) => {}
            _ => unreachable!(),
        }
        unsafe { *(ud as *mut bool) = true };
        CallbackAction::Disarm
    }

    // test "epoll: timer cancellation"
    #[test]
    fn epoll_timer_cancellation() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut trigger: Option<TimerTrigger> = None;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                100_000,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }

        // Tick and verify we're not called.
        loop_.run(RunMode::NoWait).unwrap();
        assert!(trigger.is_none());

        // Cancel the timer
        let mut called = false;
        let mut c_cancel = Completion {
            op: Operation::Cancel {
                c: ptr::addr_of_mut!(c1),
            },
            userdata: ptr::addr_of_mut!(called).cast::<c_void>(),
            callback: cancel_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_cancel)) };

        // Tick
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(called);
        assert!(trigger.unwrap() == TimerTrigger::Cancel);

        loop_.deinit();
    }

    // test "epoll: canceling a completed operation"
    #[test]
    fn epoll_canceling_a_completed_operation() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Add the timer
        let mut trigger: Option<TimerTrigger> = None;
        let mut c1 = Completion::default();
        unsafe {
            loop_.timer(
                ptr::addr_of_mut!(c1),
                1,
                ptr::addr_of_mut!(trigger).cast::<c_void>(),
                trigger_cb,
            );
        }

        // Tick and verify we're not called.
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(trigger.unwrap() == TimerTrigger::Expiration);

        // Cancel the timer
        let mut called = false;
        let mut c_cancel = Completion {
            op: Operation::Cancel {
                c: ptr::addr_of_mut!(c1),
            },
            userdata: ptr::addr_of_mut!(called).cast::<c_void>(),
            callback: cancel_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_cancel)) };

        // Tick
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(called);
        assert!(trigger.unwrap() == TimerTrigger::Expiration);

        loop_.deinit();
    }

    // -----------------------------------------------------------------------
    // Task 4 live KATs (socket / process / stream). These run REAL syscalls on
    // the live epoll loop (Miri can't), mirroring the kqueue KATs 3/4.
    // -----------------------------------------------------------------------

    use crate::linux::net;
    use std::sync::Mutex;

    // Serializes the fixed-port socket test against rapid cargo re-runs.
    static SOCKET_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn write_slice(buf: &[u8]) -> WriteBuffer {
        WriteBuffer::Slice(ptr::slice_from_raw_parts(buf.as_ptr(), buf.len()))
    }
    fn read_slice(buf: &mut [u8]) -> ReadBuffer {
        ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(buf.as_mut_ptr(), buf.len()))
    }

    // PORT NOTE: `posix.socket(family, ty, flags)`; the socket test creates a
    // blocking listener + a NONBLOCK client (SOCK.CLOEXEC on both).
    unsafe fn new_socket(family: libc::c_int, nonblock: bool) -> socket_t {
        let mut ty = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
        if nonblock {
            ty |= libc::SOCK_NONBLOCK;
        }
        let fd = unsafe { libc::socket(family, ty, 0) };
        assert!(fd >= 0);
        fd
    }

    // test "epoll: socket accept/connect/send/recv/close" (epoll.zig:1672)
    #[test]
    fn epoll_socket_roundtrip() {
        let _guard = SOCKET_TEST_LOCK.lock().unwrap();
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Create a TCP server socket
        let address = net::Address::parse_ip4(b"127.0.0.1", 3131).unwrap();
        let kernel_backlog = 1;
        let family = unsafe { address.any.family } as libc::c_int;
        let ln = unsafe { new_socket(family, false) };
        let one: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                ln,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                ptr::addr_of!(one).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            );
            libc::setsockopt(
                ln,
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                ptr::addr_of!(one).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            );
            assert!(
                libc::bind(
                    ln,
                    ptr::addr_of!(address.any) as *const libc::sockaddr,
                    address.get_os_sock_len()
                ) == 0
            );
            assert!(libc::listen(ln, kernel_backlog) == 0);
        }

        // Create a TCP client socket
        let mut client_conn = unsafe { new_socket(family, true) };

        // Accept
        let mut server_conn: socket_t = 0;
        fn accept_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let conn = ud as *mut socket_t;
            match r {
                Result::Accept(Ok(v)) => unsafe { *conn = v },
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_accept = Completion {
            op: Operation::Accept {
                socket: ln,
                addr: unsafe { core::mem::zeroed() },
                addr_size: size_of::<net::sockaddr::Any>() as net::socklen_t,
                flags: SOCK_CLOEXEC,
            },
            userdata: ptr::addr_of_mut!(server_conn).cast::<c_void>(),
            callback: accept_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_accept)) };

        // Connect
        let mut connected = false;
        fn connect_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            match r {
                Result::Connect(Ok(())) => {}
                _ => unreachable!(),
            }
            unsafe { *(ud as *mut bool) = true };
            CallbackAction::Disarm
        }
        let mut c_connect = Completion {
            op: Operation::Connect {
                socket: client_conn,
                addr: address,
            },
            userdata: ptr::addr_of_mut!(connected).cast::<c_void>(),
            callback: connect_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_connect)) };

        // Wait for the connection to be established
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn > 0);
        assert!(connected);

        // Send
        let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
        fn send_cb(
            _ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            match r {
                Result::Send(Ok(_)) => {}
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_send = Completion {
            op: Operation::Send {
                fd: client_conn,
                buffer: write_slice(&send_buf),
            },
            callback: send_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_send)) };

        // Receive
        let mut recv_buf = [0u8; 128];
        let mut recv_len: usize = 0;
        fn recv_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let ptr = ud as *mut usize;
            match r {
                Result::Recv(Ok(n)) => unsafe { *ptr = n },
                _ => unreachable!(),
            }
            CallbackAction::Disarm
        }
        let mut c_recv = Completion {
            op: Operation::Recv {
                fd: server_conn,
                buffer: read_slice(&mut recv_buf),
            },
            userdata: ptr::addr_of_mut!(recv_len).cast::<c_void>(),
            callback: recv_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_recv)) };

        // Wait for the send/receive
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

        // Shutdown
        let mut shutdown = false;
        fn shutdown_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            match r {
                Result::Shutdown(Ok(())) => {}
                _ => unreachable!(),
            }
            unsafe { *(ud as *mut bool) = true };
            CallbackAction::Disarm
        }
        let mut c_client_shutdown = Completion {
            op: Operation::Shutdown {
                socket: client_conn,
                how: ShutdownHow::Both,
            },
            userdata: ptr::addr_of_mut!(shutdown).cast::<c_void>(),
            callback: shutdown_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_client_shutdown)) };
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(shutdown);

        // Read should be EOF
        let mut eof: Option<bool> = None;
        fn eof_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let ptr = ud as *mut Option<bool>;
            let v = match r {
                Result::Recv(Ok(_)) => false,
                Result::Recv(Err(ReadError::EOF)) => true,
                Result::Recv(Err(_)) => false,
                _ => unreachable!(),
            };
            unsafe { *ptr = Some(v) };
            CallbackAction::Disarm
        }
        c_recv = Completion {
            op: Operation::Recv {
                fd: server_conn,
                buffer: read_slice(&mut recv_buf),
            },
            userdata: ptr::addr_of_mut!(eof).cast::<c_void>(),
            callback: eof_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_recv)) };

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(eof.unwrap());

        // Close
        fn close_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            match r {
                Result::Close(Ok(())) => {}
                _ => unreachable!(),
            }
            unsafe { *(ud as *mut socket_t) = 0 };
            CallbackAction::Disarm
        }
        let mut c_client_close = Completion {
            op: Operation::Close { fd: client_conn },
            userdata: ptr::addr_of_mut!(client_conn).cast::<c_void>(),
            callback: close_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_client_close)) };

        let mut ln_var = ln;
        let mut c_server_close = Completion {
            op: Operation::Close { fd: ln },
            userdata: ptr::addr_of_mut!(ln_var).cast::<c_void>(),
            callback: close_cb,
            ..Completion::default()
        };
        unsafe { loop_.add(ptr::addr_of_mut!(c_server_close)) };

        // Wait for the sockets to close
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(ln_var == 0);
        assert!(client_conn == 0);

        loop_.deinit();
    }

    // -----------------------------------------------------------------------
    // TCP watcher tests (watcher/tcp.zig `TCPTests`). Ported for the epoll
    // backend instantiation. `TCPTests` runs against `TCPStream(xev)` +
    // `TCPDynamic(xev)`; the per-backend split gives that coverage here (epoll)
    // and in `io_uring.rs` (io_uring), with the Dynamic wrapper covered by the
    // crate-level `dynamic` tests. The `test "TCP: Stream decls"` reflection test
    // (tcp.zig:506) is a Zig comptime `@hasDecl` sweep with no Rust runtime
    // analog — it is enforced at COMPILE time by `TCP` carrying every
    // `read`/`write`/`queue_write`/`close`/`poll` method, so it is not a
    // separate runtime test here.
    // -----------------------------------------------------------------------

    // test "TCP: accept/connect/send/recv/close" (tcp.zig:518). The epoll TCP
    // `close` rides the thread pool (the `.threadpool` flag), so the loop is
    // built with one — exactly as the reference builds `Loop.init(.{
    // .thread_pool = &tpool })`.
    #[test]
    fn epoll_tcp_accept_connect_send_recv_close() {
        use crate::thread_pool::{Config, ThreadPool};

        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        // Choose random available port (Zig #14907)
        let mut address = net::Address::parse_ip4(b"127.0.0.1", 0).unwrap();
        let server = TCP::init(address).unwrap();

        // Bind and listen
        server.bind(address).unwrap();
        server.listen(1).unwrap();

        // Retrieve bound port and initialize client
        let mut sock_len = address.get_os_sock_len();
        unsafe {
            libc::getsockname(
                server.fd(),
                ptr::addr_of_mut!(address.any) as *mut libc::sockaddr,
                ptr::addr_of_mut!(sock_len),
            );
        }
        let client = TCP::init(address).unwrap();

        let mut c_accept = Completion::default();
        let mut c_connect = Completion::default();

        // Accept
        let mut server_conn: Option<TCP> = None;
        unsafe {
            server.accept(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_conn),
                |ud: *mut Option<TCP>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 r: core::result::Result<TCP, AcceptError>| {
                    *ud = Some(r.unwrap());
                    CallbackAction::Disarm
                },
            );
        }

        // Connect
        let mut connected = false;
        unsafe {
            client.connect(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                address,
                ptr::addr_of_mut!(connected),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), ConnectError>| {
                    r.unwrap();
                    *ud = true;
                    CallbackAction::Disarm
                },
            );
        }

        // Wait for the connection to be established
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn.is_some());
        assert!(connected);

        // Close the server
        let mut server_closed = false;
        unsafe {
            server.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_closed),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = true;
                    CallbackAction::Disarm
                },
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_closed);

        // Send
        let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
        unsafe {
            client.write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                write_slice(&send_buf),
                ptr::null_mut::<()>(),
                |_ud: *mut (),
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 _b: WriteBuffer,
                 r: core::result::Result<usize, WriteError>| {
                    r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Receive
        let mut recv_buf = [0u8; 128];
        let mut recv_len: usize = 0;
        let server_conn_tcp = server_conn.unwrap();
        unsafe {
            server_conn_tcp.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                read_slice(&mut recv_buf),
                ptr::addr_of_mut!(recv_len),
                |ud: *mut usize,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 _b: ReadBuffer,
                 r: core::result::Result<usize, ReadError>| {
                    *ud = r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Wait for the send/receive
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

        // Close
        unsafe {
            server_conn_tcp.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_conn),
                |ud: *mut Option<TCP>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = None;
                    CallbackAction::Disarm
                },
            );
            client.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                ptr::addr_of_mut!(connected),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = false;
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn.is_none());
        assert!(!connected);
        assert!(server_closed);

        loop_.deinit();
        pool.shutdown();
        pool.deinit();
    }

    // test "TCP: Queued writes" (tcp.zig:697). Limits SO_SNDBUF to force a
    // partial unqueued write, then drains the remainder through the ordered-
    // write engine (`queue_write`), shutting the writer down inside the queued
    // callback; a re-arming receiver reads until the full 1 MB is collected.
    //
    // Potentially flaky (per the reference): could hang if the sender can't
    // flush or the receiver over-reads.
    #[test]
    fn epoll_tcp_queued_writes() {
        use crate::thread_pool::{Config, ThreadPool};

        // send_buf = [_]u8{ 1..9, 0 } ** 100_000 (1 MB).
        const PATTERN: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 0];
        const SEND_LEN: usize = PATTERN.len() * 100_000;
        let mut send_buf: Vec<u8> = Vec::with_capacity(SEND_LEN);
        for _ in 0..100_000 {
            send_buf.extend_from_slice(&PATTERN);
        }

        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        let mut address = net::Address::parse_ip4(b"127.0.0.1", 0).unwrap();
        let server = TCP::init(address).unwrap();
        server.bind(address).unwrap();
        server.listen(1).unwrap();

        let mut sock_len = address.get_os_sock_len();
        unsafe {
            libc::getsockname(
                server.fd(),
                ptr::addr_of_mut!(address.any) as *mut libc::sockaddr,
                ptr::addr_of_mut!(sock_len),
            );
        }
        let client = TCP::init(address).unwrap();

        let mut c_accept = Completion::default();
        let mut c_connect = Completion::default();

        // Accept
        let mut server_conn: Option<TCP> = None;
        unsafe {
            server.accept(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_conn),
                |ud: *mut Option<TCP>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 r: core::result::Result<TCP, AcceptError>| {
                    *ud = Some(r.unwrap());
                    CallbackAction::Disarm
                },
            );
        }

        // Connect
        let mut connected = false;
        unsafe {
            client.connect(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                address,
                ptr::addr_of_mut!(connected),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), ConnectError>| {
                    r.unwrap();
                    *ud = true;
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn.is_some());
        assert!(connected);

        // Close the server (listener)
        let mut server_closed = false;
        unsafe {
            server.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_closed),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = true;
                    CallbackAction::Disarm
                },
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_closed);

        // Unqueued send - Limit send buffer to 8kB, this should force partial writes.
        let sndbuf: libc::c_int = 8192;
        unsafe {
            libc::setsockopt(
                client.fd(),
                libc::SOL_SOCKET,
                libc::SO_SNDBUF,
                ptr::addr_of!(sndbuf).cast(),
                size_of::<libc::c_int>() as net::socklen_t,
            );
        }

        // First we try to send the whole 1MB buffer in one write operation, this
        // _should_ result in a partial write.
        let mut sent_unqueued: usize = 0;
        unsafe {
            client.write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                write_slice(&send_buf),
                ptr::addr_of_mut!(sent_unqueued),
                |ud: *mut usize,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 _b: WriteBuffer,
                 r: core::result::Result<usize, WriteError>| {
                    *ud = r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Make sure that we sent a small fraction of the buffer.
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(sent_unqueued < (send_buf.len() / 10));

        // Set up queued write for the remainder; shutdown() the socket afterwards.
        let mut w_queue = WriteQueue::default();
        let mut wr_send = WriteRequest::default();
        let mut sent_queued: usize = 0;
        let queued_slice = write_slice(&send_buf[sent_unqueued..]);
        unsafe {
            client.queue_write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(w_queue),
                ptr::addr_of_mut!(wr_send),
                queued_slice,
                ptr::addr_of_mut!(sent_queued),
                |ud: *mut usize,
                 l: *mut Loop,
                 c: *mut Completion,
                 tcp: TCP,
                 _b: WriteBuffer,
                 r: core::result::Result<usize, WriteError>| {
                    *ud = r.unwrap();
                    tcp.shutdown(
                        l,
                        c,
                        ptr::null_mut::<()>(),
                        |_ud: *mut (),
                         _l: *mut Loop,
                         _c: *mut Completion,
                         _s: TCP,
                         _r: core::result::Result<(), ShutdownError>| {
                            CallbackAction::Disarm
                        },
                    );
                    CallbackAction::Disarm
                },
            );
        }

        // Set up receiver which keeps reading until it reads the full send buffer.
        struct Receiver {
            loop_: *mut Loop,
            conn: TCP,
            completion: Completion,
            buf: Vec<u8>,
            bytes_read: usize,
        }
        impl Receiver {
            fn read(&mut self) {
                if self.bytes_read == self.buf.len() {
                    return;
                }
                let start = self.bytes_read;
                let len = self.buf.len() - start;
                let read_buf = ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(
                    unsafe { self.buf.as_mut_ptr().add(start) },
                    len,
                ));
                let loop_ = self.loop_;
                let comp = ptr::addr_of_mut!(self.completion);
                let self_ptr = self as *mut Receiver;
                unsafe {
                    self.conn
                        .read(loop_, comp, read_buf, self_ptr, Self::read_cb);
                }
            }

            fn read_cb(
                ud: *mut Receiver,
                _l: *mut Loop,
                _c: *mut Completion,
                _s: TCP,
                _b: ReadBuffer,
                r: core::result::Result<usize, ReadError>,
            ) -> CallbackAction {
                let receiver = unsafe { &mut *ud };
                let n_bytes = r.unwrap();
                receiver.bytes_read += n_bytes;
                if receiver.bytes_read < receiver.buf.len() {
                    receiver.read();
                }
                CallbackAction::Disarm
            }
        }
        let mut receiver = Receiver {
            loop_: ptr::addr_of_mut!(loop_),
            conn: server_conn.unwrap(),
            completion: Completion::default(),
            buf: vec![0u8; SEND_LEN],
            bytes_read: 0,
        };
        receiver.read();

        // Wait for the send/receive.
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(&send_buf[..], &receiver.buf[0..receiver.bytes_read]);
        assert_eq!(send_buf.len(), sent_unqueued + sent_queued);

        // Close
        let server_conn_tcp = server_conn.unwrap();
        unsafe {
            server_conn_tcp.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_accept),
                ptr::addr_of_mut!(server_conn),
                |ud: *mut Option<TCP>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = None;
                    CallbackAction::Disarm
                },
            );
            client.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_connect),
                ptr::addr_of_mut!(connected),
                |ud: *mut bool,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    *ud = false;
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(server_conn.is_none());
        assert!(!connected);
        assert!(server_closed);

        loop_.deinit();
        pool.shutdown();
        pool.deinit();
    }

    // -----------------------------------------------------------------------
    // UDP watcher tests (watcher/udp.zig `UDPTests`). Ported for the epoll
    // backend instantiation. The `test "UDP: Stream decls"` reflection test
    // (udp.zig:902) is a Zig comptime `@hasDecl` sweep with no Rust runtime
    // analog — it is enforced at COMPILE time by `UDP` carrying every
    // `read`/`write`/`close`/`poll` method, so it is not a separate runtime test.
    // Like the epoll `TCP` tests, `UDP::close` rides the thread pool, so the loop
    // is built with one (the reference builds `Loop.init(.{ .thread_pool })`).
    // -----------------------------------------------------------------------

    // test "UDP: read/write" (udp.zig:914). Distinct port from the TCP/socket
    // tests so parallel cargo runs cannot cross-route datagrams (SO_REUSEPORT).
    //
    // Named in the const-registry style (mirrors io_uring.rs's `PORT_*` block)
    // so the loopback port is greppable and non-colliding: 3131 is this module's
    // `epoll_socket_roundtrip`; 3132-3134/3136 are io_uring.rs's; 3137/3138 are
    // dynamic.rs's. 3135 is THIS test — was a bare literal, now registered here.
    const PORT_UDP_READ_WRITE: u16 = 3135;

    #[test]
    fn epoll_udp_read_write() {
        use crate::thread_pool::{Config, ThreadPool};

        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        let address = net::Address::parse_ip4(b"127.0.0.1", PORT_UDP_READ_WRITE).unwrap();
        let server = UDP::init(address).unwrap();
        let client = UDP::init(address).unwrap();

        // Bind / Recv
        server.bind(address).unwrap();
        let mut c_read = Completion::default();
        let mut s_read = UdpState::undefined();
        let mut recv_buf = [0u8; 128];
        let mut recv_len: usize = 0;
        unsafe {
            server.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
                ptr::addr_of_mut!(s_read),
                read_slice(&mut recv_buf),
                ptr::addr_of_mut!(recv_len),
                |ud: *mut usize,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: *mut UdpState,
                 _addr: net::Address,
                 _sock: UDP,
                 _b: ReadBuffer,
                 r: core::result::Result<usize, ReadError>| {
                    *ud = r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Send
        let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
        let mut c_write = Completion::default();
        let mut s_write = UdpState::undefined();
        unsafe {
            client.write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                ptr::addr_of_mut!(s_write),
                address,
                write_slice(&send_buf),
                ptr::null_mut::<()>(),
                |_ud: *mut (),
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: *mut UdpState,
                 _sock: UDP,
                 _b: WriteBuffer,
                 r: core::result::Result<usize, WriteError>| {
                    r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        // Wait for the send/receive
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(recv_len > 0);
        assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

        // Close
        unsafe {
            server.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
                ptr::null_mut::<()>(),
                |_ud: *mut (),
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: UDP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    CallbackAction::Disarm
                },
            );
            client.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                ptr::null_mut::<()>(),
                |_ud: *mut (),
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: UDP,
                 r: core::result::Result<(), CloseError>| {
                    r.unwrap();
                    CallbackAction::Disarm
                },
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();

        loop_.deinit();
        pool.shutdown();
        pool.deinit();
    }

    // KAT — Process spawn+exit: posix_spawn `/bin/true`; Process.wait;
    // run(until_done); assert exit status 0. Proves the pidfd poll + waitid reap
    // on the live epoll loop.
    #[test]
    #[allow(clippy::zombie_processes)]
    fn epoll_process_spawn_exit() {
        // posix_spawn /bin/true (a real process that exits 0).
        let mut pid: libc::pid_t = 0;
        let path = b"/bin/true\0";
        let mut argv: [*mut libc::c_char; 2] =
            [path.as_ptr() as *mut libc::c_char, ptr::null_mut()];
        let mut envp: [*mut libc::c_char; 1] = [ptr::null_mut()];
        let rc = unsafe {
            libc::posix_spawn(
                &mut pid,
                path.as_ptr() as *const libc::c_char,
                ptr::null(),
                ptr::null(),
                argv.as_mut_ptr() as *const *mut libc::c_char,
                envp.as_mut_ptr() as *const *mut libc::c_char,
            )
        };
        assert_eq!(rc, 0, "posix_spawn /bin/true");

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut p = Process::init(pid).unwrap();

        // Wait
        let mut code: Option<u32> = None;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut Option<u32>,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<u32, ProcessWaitError>| {
            unsafe { *ud = Some(r.unwrap()) };
            CallbackAction::Disarm
        };
        unsafe {
            p.wait(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_wait),
                ptr::addr_of_mut!(code),
                cb,
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(code, Some(0), "exit status 0");

        p.deinit();
        loop_.deinit();
    }

    // KAT — Pty write round-trip: openpty; Stream.init_fd(write side = parent);
    // queue_write a payload; read it back on the other end (child); assert bytes +
    // WriteQueue drains. Proves the ordered-write engine on the live epoll loop.
    #[test]
    fn epoll_pty_write_roundtrip() {
        let mut pty = Pty::init().unwrap();
        let mut loop_ = Loop::init(Options::default()).unwrap();

        let parent = Stream::init_fd(pty.parent); // write side
        let child = Stream::init_fd(pty.child); // read side

        // Read back on the child end.
        let mut read_buf = [0u8; 128];
        let mut read_len: Option<usize> = None;
        let mut c_read = Completion::default();
        let read_cb = |ud: *mut Option<usize>,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: Stream,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = Some(r.unwrap()) };
            CallbackAction::Disarm
        };
        unsafe {
            child.read(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_read),
                read_slice(&mut read_buf),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

        // queue_write the payload on the parent (write) end.
        let mut write_queue: WriteQueue = WriteQueue::default();
        let mut write_req = WriteRequest::default();
        let payload = b"KAT payload\n";
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            assert_eq!(r.unwrap(), b"KAT payload\n".len());
            CallbackAction::Disarm
        };
        unsafe {
            parent.queue_write::<(), _>(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req),
                write_slice(payload),
                ptr::null_mut(),
                write_cb,
            );
        }

        // The queue is non-empty until the write drains it.
        assert!(!write_queue.empty());

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some());
        assert_eq!(payload, &read_buf[0..read_len.unwrap()]);

        // The WriteQueue drains (the single request was popped by the trampoline).
        assert!(write_queue.empty());

        pty.deinit();
        loop_.deinit();
    }

    // KAT — Async (eventfd) wakeup: wait on an `Async`, notify it from a spawned
    // thread, and confirm the loop's `run(until_done)` returns with the wait
    // callback fired. Proves the eventfd poll + drain on the live epoll loop.
    //
    // PORT NOTE: the reference's `AsyncTests` (`watcher/async.zig`) are generic over
    // the backend; this is the "async: notify" shape monomorphized for epoll (the
    // twin of `io_uring_async_notify`).
    #[test]
    fn epoll_async_notify() {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        let notifier = Async::init().unwrap();

        let mut wake = false;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), ReadError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_wait),
                ptr::addr_of_mut!(wake),
                cb,
            );
        }

        // Not woken up before the notify.
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!wake);

        // Notify from ANOTHER thread WHILE this one is BLOCKED inside the loop —
        // the whole point of `Async`. `run(until_done)` parks in `epoll_wait`
        // until the eventfd goes readable, so this asserts the real wakeup path.
        // Not racy: if the notify lands first the poll simply completes at once.
        let n = notifier; // `Async` is Copy (an fd handle) and Send (a plain i32).
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            n.notify().unwrap();
        });

        // Blocks here until the spawned thread notifies. A hang is a real bug.
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(wake);
        t.join().unwrap();

        // The eventfd counter was drained by the wait callback, so a second wait
        // does NOT fire spuriously.
        let mut wake2 = false;
        let mut c_wait2 = Completion::default();
        let cb2 = |ud: *mut bool,
                   _l: *mut Loop,
                   _c: *mut Completion,
                   r: core::result::Result<(), ReadError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_wait2),
                ptr::addr_of_mut!(wake2),
                cb2,
            );
        }
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!wake2);

        notifier.deinit();
        loop_.deinit();
    }

    // Task-2 KAT: a `.close` op flagged for the thread pool runs on the pool
    // (`thread_schedule` -> worker `thread_perform` closes the fd + pushes onto
    // `thread_pool_completions` + wakes the loop) and its callback fires when the
    // loop drains the MPSC queue in `tick`. This exercises the whole epoll
    // thread-pool path the File watcher (Task 5) rides. With NO pool attached the
    // same flagged close yields `ThreadPoolRequired` (asserted in the second half).
    #[test]
    fn epoll_close_on_thread_pool() {
        use crate::thread_pool::Config;

        struct CloseState {
            fired: bool,
            ok: bool,
        }

        fn close_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let st = unsafe { &mut *(ud as *mut CloseState) };
            st.fired = true;
            st.ok = matches!(r, Result::Close(Ok(())));
            CallbackAction::Disarm
        }

        // --- With a real pool: the flagged close runs on the pool. ---
        let mut pool = ThreadPool::init(Config {
            stack_size: 0,
            max_threads: 1,
        });
        let pool_ptr: *mut ThreadPool = &mut pool;

        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        // A pipe gives us a real fd to close.
        let mut fds = [0 as fd_t; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let (read_fd, write_fd) = (fds[0], fds[1]);

        let mut st = CloseState {
            fired: false,
            ok: false,
        };
        let mut c = Completion::new_op(
            Operation::Close { fd: read_fd },
            ptr::addr_of_mut!(st) as *mut c_void,
            close_cb,
        );
        // Stream.close sets this bit; set it directly here to drive the pool path.
        unsafe { Completion::set_threadpool(ptr::addr_of_mut!(c), true) };
        unsafe { loop_.add(ptr::addr_of_mut!(c)) };

        // Blocks in epoll_wait until the pool worker wakes us; a hang is a real bug.
        loop_.run(RunMode::UntilDone).unwrap();

        assert!(
            st.fired,
            "close callback never fired — the thread-pool completion was not drained"
        );
        // `st.ok` (the pool-thread `perform` returned `Result::Close(Ok(()))`) is
        // the meaningful proof the blocking close ran on the pool AND that its
        // `task_result` round-tripped through `thread_pool_completions` intact — a
        // stale/never-written `task_result` would surface here as the default
        // `Noop` (or an `Err`). We deliberately do NOT re-close `read_fd` to prove
        // EBADF: fd numbers are per-process, and a concurrently-running test may
        // have already recycled this integer — the re-close would then race (or,
        // worse, close an unrelated test's fd).
        assert!(
            st.ok,
            "close on the pool did not deliver Result::Close(Ok(())) — task_result \
             was stale or an error"
        );
        let _ = read_fd;

        unsafe { libc::close(write_fd) };
        loop_.deinit();
        pool.shutdown();
        pool.deinit();

        // --- With NO pool: the same flagged close yields ThreadPoolRequired. ---
        let mut loop2 = Loop::init(Options::default()).unwrap();
        let mut fds2 = [0 as fd_t; 2];
        assert_eq!(unsafe { libc::pipe(fds2.as_mut_ptr()) }, 0);

        let mut got_required = false;
        fn required_cb(
            ud: *mut c_void,
            _l: *mut Loop,
            _c: *mut Completion,
            r: Result,
        ) -> CallbackAction {
            let flag = unsafe { &mut *(ud as *mut bool) };
            *flag = matches!(r, Result::Close(Err(CloseError::ThreadPoolRequired)));
            CallbackAction::Disarm
        }
        let mut c2 = Completion::new_op(
            Operation::Close { fd: fds2[0] },
            ptr::addr_of_mut!(got_required) as *mut c_void,
            required_cb,
        );
        unsafe { Completion::set_threadpool(ptr::addr_of_mut!(c2), true) };
        unsafe { loop2.add(ptr::addr_of_mut!(c2)) };
        loop2.run(RunMode::UntilDone).unwrap();
        assert!(
            got_required,
            "flagged close with no pool must yield ThreadPoolRequired"
        );

        unsafe {
            libc::close(fds2[0]);
            libc::close(fds2[1]);
        }
        loop2.deinit();
    }

    // Helper to open a pty (test-only), mirroring watcher/stream.zig's `Pty`.
    struct Pty {
        parent: fd_t,
        child: fd_t,
    }

    #[repr(C)]
    struct Winsize {
        ws_row: u16,
        ws_col: u16,
        ws_xpixel: u16,
        ws_ypixel: u16,
    }

    extern "C" {
        fn openpty(
            parent: *mut fd_t,
            child: *mut fd_t,
            name: *mut u8,
            termios: *const c_void,
            winsize: *const Winsize,
        ) -> libc::c_int;
    }

    impl Pty {
        fn init() -> core::result::Result<Pty, ()> {
            let size = Winsize {
                ws_row: 80,
                ws_col: 80,
                ws_xpixel: 800,
                ws_ypixel: 600,
            };
            let mut parent_fd: fd_t = 0;
            let mut child_fd: fd_t = 0;
            if unsafe {
                openpty(
                    ptr::addr_of_mut!(parent_fd),
                    ptr::addr_of_mut!(child_fd),
                    ptr::null_mut(),
                    ptr::null(),
                    &size,
                )
            } < 0
            {
                return Err(());
            }
            Ok(Pty {
                parent: parent_fd,
                child: child_fd,
            })
        }

        fn deinit(&mut self) {
            unsafe {
                libc::close(self.parent);
                libc::close(self.child);
            }
        }
    }

    // -----------------------------------------------------------------------
    // File watcher tests (watcher/file.zig `FileTests`). On epoll every File
    // read/write/pread/pwrite is a BLOCKING op that rides the ThreadPool, so these
    // exercise the whole epoll thread-pool round-trip (schedule -> worker
    // `thread_perform` -> MPSC drain -> callback) AND the real multi-worker pool
    // teardown at the end of each test (the exact path Task 1's deviation hardened
    // — `Config::default()` resolves `max_threads = 0` to the host CPU count).
    // -----------------------------------------------------------------------

    // Creates a fresh empty temp file (read+write, truncated) and returns its raw
    // fd + path. The caller closes the fd and removes the path.
    fn temp_file() -> (fd_t, std::path::PathBuf) {
        use std::os::unix::io::IntoRawFd;
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rustty_xev_file_epoll_{}_{}",
            std::process::id(),
            n
        ));
        let f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        (f.into_raw_fd(), path)
    }

    // Opens an existing path read-only and returns its raw fd (a fresh fd resets the
    // file offset, exactly as the reference reopens `f2` before the read).
    fn open_read_fd(path: &std::path::Path) -> fd_t {
        use std::os::unix::io::IntoRawFd;
        std::fs::File::open(path).unwrap().into_raw_fd()
    }

    // test "read/write" (file.zig:596). write a payload via File.write (on the
    // pool), then read it back through a freshly-opened fd.
    #[test]
    fn epoll_file_read_write() {
        use crate::thread_pool::Config;

        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        let (wfd, path) = temp_file();
        let file = File::init(wfd);

        let write_buf = [1u8, 1, 2, 3, 5, 8, 13];
        let mut c_write = Completion::default();
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: File,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            assert_eq!(r.unwrap(), 7);
            CallbackAction::Disarm
        };
        unsafe {
            file.write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                write_slice(&write_buf),
                ptr::null_mut::<()>(),
                write_cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();

        // Make sure the data is on disk.
        assert_eq!(unsafe { libc::fsync(wfd) }, 0);

        let rfd = open_read_fd(&path);
        let file2 = File::init(rfd);
        let mut read_buf = [0u8; 128];
        let mut read_len: usize = 0;
        let read_cb = |ud: *mut usize,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: File,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = r.unwrap() };
            CallbackAction::Disarm
        };
        unsafe {
            file2.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                read_slice(&mut read_buf),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(read_len, write_buf.len());
        assert_eq!(&write_buf, &read_buf[0..read_len]);

        unsafe {
            libc::close(wfd);
            libc::close(rfd);
        }
        loop_.deinit();
        pool.shutdown();
        pool.deinit();
        std::fs::remove_file(&path).ok();
    }

    // test "pread/pwrite" (file.zig:671). Same as read/write but with explicit
    // offset 0 through File.pwrite / File.pread.
    #[test]
    fn epoll_file_pread_pwrite() {
        use crate::thread_pool::Config;

        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        let (wfd, path) = temp_file();
        let file = File::init(wfd);

        let write_buf = [1u8, 1, 2, 3, 5, 8, 13];
        let mut c_write = Completion::default();
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: File,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            assert_eq!(r.unwrap(), 7);
            CallbackAction::Disarm
        };
        unsafe {
            file.pwrite(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                write_slice(&write_buf),
                0,
                ptr::null_mut::<()>(),
                write_cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();

        assert_eq!(unsafe { libc::fsync(wfd) }, 0);

        let rfd = open_read_fd(&path);
        let file2 = File::init(rfd);
        let mut read_buf = [0u8; 128];
        let mut read_len: usize = 0;
        let read_cb = |ud: *mut usize,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: File,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = r.unwrap() };
            CallbackAction::Disarm
        };
        unsafe {
            file2.pread(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                read_slice(&mut read_buf),
                0,
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(&write_buf, &read_buf[0..read_len]);

        unsafe {
            libc::close(wfd);
            libc::close(rfd);
        }
        loop_.deinit();
        pool.shutdown();
        pool.deinit();
        std::fs::remove_file(&path).ok();
    }

    // test "queued writes" (file.zig:744). Two ordered File.queue_write requests
    // ("1234" then "5678"); read back "12345678".
    #[test]
    fn epoll_file_queued_writes() {
        use crate::thread_pool::Config;

        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        let (wfd, path) = temp_file();
        let file = File::init(wfd);

        let mut write_queue: WriteQueue = WriteQueue::default();
        let mut write_req: [WriteRequest; 2] = [WriteRequest::default(), WriteRequest::default()];
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: File,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap();
            CallbackAction::Disarm
        };
        unsafe {
            file.queue_write::<(), _>(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req[0]),
                write_slice(b"1234"),
                ptr::null_mut(),
                write_cb,
            );
            file.queue_write::<(), _>(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req[1]),
                write_slice(b"5678"),
                ptr::null_mut(),
                write_cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(write_queue.empty());

        assert_eq!(unsafe { libc::fsync(wfd) }, 0);

        let rfd = open_read_fd(&path);
        let file2 = File::init(rfd);
        let mut read_buf = [0u8; 128];
        let mut read_len: usize = 0;
        let mut c_read = Completion::default();
        let read_cb = |ud: *mut usize,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: File,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = r.unwrap() };
            CallbackAction::Disarm
        };
        unsafe {
            file2.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
                read_slice(&mut read_buf),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(b"12345678", &read_buf[0..read_len]);

        unsafe {
            libc::close(wfd);
            libc::close(rfd);
        }
        loop_.deinit();
        pool.shutdown();
        pool.deinit();
        std::fs::remove_file(&path).ok();
    }

    // test "poll" (file.zig:557). A pipe with a byte already written is poll-read
    // ready. File.poll does NOT ride the pool (it registers with epoll directly),
    // so no ThreadPool is attached here.
    #[test]
    fn epoll_file_poll() {
        let mut loop_ = Loop::init(Options::default()).unwrap();

        // Create our pipe and write to it so it's ready to be read.
        let mut fds = [0 as fd_t; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let (read_fd, write_fd) = (fds[0], fds[1]);
        assert_eq!(unsafe { libc::write(write_fd, b"x".as_ptr().cast(), 1) }, 1);

        let file = File::init_fd(read_fd);
        let mut c = Completion::default();
        let mut ready = false;
        let poll_cb = |ud: *mut bool,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: File,
                       r: core::result::Result<PollEvent, PollError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            file.poll(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c),
                PollEvent::Read,
                ptr::addr_of_mut!(ready),
                poll_cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(ready);

        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
        loop_.deinit();
    }
}
