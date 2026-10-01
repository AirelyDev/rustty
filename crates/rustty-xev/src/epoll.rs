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
    // PORT NOTE (N431 in NEW-FINDINGS): the reference's `run(self: *Loop, ..)`
    // body lives in `run_raw`, which takes the loop as a raw pointer, as Zig's
    // unprotected `*Loop` does. Callbacks re-enter the loop while it runs,
    // through their `l` argument or through a pointer the caller stored (termio's
    // `td.loop_`), so no `&mut Loop` may be live across a callback: a
    // function-entry `&mut self` is protected for the whole call, and any other
    // access to the loop pops it (UB under Stacked and Tree Borrows,
    // .port/phase-2/mirihar-n431). This entry converts to raw at once and never
    // uses `self` again, so it is sound when callbacks reach the loop only
    // through `l`. A caller whose callbacks re-enter through a stored pointer
    // must enter through `run_raw` with that same pointer instead: this
    // `&mut self` is itself protected for the call (mirihar-n431
    // `wrapper_*_root`).
    pub fn run(&mut self, mode: RunMode) -> core::result::Result<(), Error> {
        // SAFETY: `self` is a live, exclusive `Loop` for the whole call.
        unsafe { Loop::run_raw(self, mode) }
    }

    /// `run(self: *Loop, mode)`: the raw-pointer entry (see `run`'s PORT NOTE).
    /// Callers whose callbacks re-enter the loop through a stored pointer
    /// enter here with that pointer.
    ///
    /// # Safety
    /// `this` must point at a live, initialized `Loop` that stays at the same
    /// address and is not otherwise borrowed for the whole call, except by
    /// the loop's own callbacks through raw pointers.
    pub unsafe fn run_raw(this: *mut Loop, mode: RunMode) -> core::result::Result<(), Error> {
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
                // N434: through `timer_ptr`, never `&mut (*c).op`.
                let next = self.timer_next(next_ms);
                (*timer_ptr(c)).next = next;
                (*c).userdata = userdata;
                (*c).callback = cb;
            },

            State::Active => unsafe {
                // Update the reset time for the timer to the desired time
                // along with all the callbacks.
                let reset = self.timer_next(next_ms);
                // N434: the timer is linked in the heap; `timer_ptr`, never
                // `&mut (*c).op`.
                (*timer_ptr(c)).reset = Some(reset);
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
        // SAFETY: as `run`.
        unsafe { Loop::tick_raw(self, wait) }
    }

    /// `tick(self: *Loop, wait)`: the raw-pointer twin of `tick` (see `run`).
    ///
    /// # Safety
    /// As `run_raw`.
    pub unsafe fn tick_raw(this: *mut Loop, wait: u32) -> core::result::Result<(), Error> {
        unsafe {
            // If we're stopped then the loop is fully over.
            if (*this).flags.stopped {
                return Ok(());
            }

            // We can't nest runs.
            if (*this).flags.in_run {
                return Err(Error); // error.NestedRunsNotAllowed
            }
            (*this).flags.in_run = true;
            // defer self.flags.in_run = false; — via the guard below.
            let result = Loop::tick_inner(this, wait);
            (*this).flags.in_run = false;
            result
        }
    }

    unsafe fn tick_inner(this: *mut Loop, wait: u32) -> core::result::Result<(), Error> {
        unsafe {
            // Initialize
            if !(*this).flags.init {
                (*this).flags.init = true;

                if (*this).thread_pool.is_some() {
                    // Establish the stable heap-rooted raw handle ONCE, then init the
                    // self-referential queue THROUGH it, so the stub self-pointers it
                    // writes carry the queue's own (Box) allocation provenance — not a
                    // per-tick `&mut self` tag that the next tick's reborrow would pop.
                    // (mirrors kqueue.rs's LIFETIMES-AUDIT M2 fix; mirihar `boxq`.)
                    {
                        let qp: *mut TaskCompletionQueue =
                            &raw mut *(*this).thread_pool_completions;
                        ptr::addr_of_mut!((*this).thread_pool_completions_ptr).write(qp);
                        TaskCompletionQueue::init(qp);
                    }
                }

                let mut ev = libc::epoll_event {
                    events: (libc::EPOLLIN | libc::EPOLLRDHUP) as u32,
                    u64: (*this).eventfd.fd as u64,
                };
                // SAFETY: register the always-on wakeup eventfd with our epoll fd.
                let rc =
                    libc::epoll_ctl((*this).fd, libc::EPOLL_CTL_ADD, (*this).eventfd.fd, &mut ev);
                if rc < 0 {
                    // We reset initialization because we can't do anything
                    // safely unless we get this eventfd registered!
                    (*this).flags.init = false;
                    return Err(Error);
                }
            }

            // Submit all the submissions. We copy the submission queue so that
            // any resubmits don't cause an infinite loop.
            let mut wait_rem: usize = wait as usize;
            let mut queued = core::mem::take(&mut (*this).submissions);
            // self.submissions = .{};  (done by mem::take)
            loop {
                let c = queued.pop();
                if c.is_null() {
                    break;
                }
                // We ignore any completions that aren't in the adding state.
                // This usually means that we switched them to be deleted or
                // something.
                if (*c).flags.state != State::Adding {
                    continue;
                }

                // These operations happen synchronously. Ensure they are
                // decremented from wait_rem.
                match &(*c).op {
                    Operation::Cancel { .. } | Operation::Shutdown { .. } | Operation::Timer(_) => {
                        wait_rem = wait_rem.saturating_sub(1);
                    }
                    _ => {}
                }

                Loop::start(this, c);
            }

            // Handle all deletions so we don't wait for them.
            loop {
                let c = (*this).deletions.pop();
                if c.is_null() {
                    break;
                }
                if (*c).flags.state != State::Deleting {
                    continue;
                }
                Loop::stop_completion(this, c);
            }

            // If we have no active handles then we return no matter what.
            if (*this).active == 0 {
                // We still have to update our concept of "now".
                (*this).update_now();
                return Ok(());
            }

            // Wait and process events. We only do this if we have any active.
            let mut events: [libc::epoll_event; 1024] = core::mem::zeroed();
            while (*this).active > 0 && (wait == 0 || wait_rem > 0) {
                (*this).update_now();
                let mut now_timer = Timer {
                    next: (*this).cached_now,
                    reset: None,
                    heap: heap::IntrusiveField::default(),
                    c: ptr::null_mut(),
                };

                // Run our expired timers
                loop {
                    let t = (*this).timers.peek();
                    if t.is_null() {
                        break;
                    }
                    if !Timer::less((), t, &mut now_timer) {
                        break;
                    }

                    // Remove the timer
                    assert!((*this).timers.delete_min() == t);

                    // Mark completion as done
                    let c = (*t).c;
                    (*c).flags.state = State::Dead;
                    (*this).active -= 1;

                    // Lower our remaining count
                    wait_rem = wait_rem.saturating_sub(1);

                    // Invoke
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    let action = cb(ud, this, c, Result::Timer(Ok(TimerTrigger::Expiration)));
                    match action {
                        CallbackAction::Disarm => {}
                        CallbackAction::Rearm => Loop::start(this, c),
                    }
                }

                // Run our completed thread pool work
                if (*this).thread_pool.is_some() {
                    loop {
                        // Drain through the cached stable raw handle (never a fresh
                        // `&mut self.thread_pool_completions`, which would pop the
                        // queue's stub-pointer lineage). Mirrors kqueue.rs; mirihar
                        // `boxq`.
                        let c = TaskCompletionQueue::pop((*this).thread_pool_completions_ptr);
                        if c.is_null() {
                            break;
                        }

                        // Mark completion as done
                        (*c).flags.state = State::Dead;
                        (*this).active -= 1;

                        // Lower our remaining count
                        wait_rem = wait_rem.saturating_sub(1);

                        // Invoke
                        let cb = (*c).callback;
                        let ud = (*c).userdata;
                        let tr = (*c).task_result;
                        let action = cb(ud, this, c, tr);
                        match action {
                            CallbackAction::Disarm => {}
                            CallbackAction::Rearm => Loop::start(this, c),
                        }
                    }
                }

                // Determine our next timeout based on the timers
                let timeout: i32 = if wait_rem == 0 {
                    0
                } else {
                    // If we have a timer, we want to set the timeout to our next
                    // timer value. If we have no timer, we wait forever.
                    let t = (*this).timers.peek();
                    if t.is_null() {
                        -1
                    } else {
                        // Determine the time in milliseconds.
                        let ms_now = ((*this).cached_now.tv_sec as u64) * ms_per_s
                            + ((*this).cached_now.tv_nsec as u64) / ns_per_ms;
                        let ms_next = ((*t).next.tv_sec as u64) * ms_per_s
                            + ((*t).next.tv_nsec as u64) / ns_per_ms;
                        // CARRY-FORWARD FIX (T3 review): the reference `@intCast(ms_next
                        // -| ms_now)` truncates; a plain `as i32` on a >24.8-day wait
                        // (u64 diff > i32::MAX ms) would WRAP NEGATIVE, turning the
                        // epoll_wait timeout into a spurious immediate/forever value.
                        // Clamp to i32::MAX (epoll_wait caps at ~24.8 days anyway).
                        i32::try_from(ms_next.saturating_sub(ms_now)).unwrap_or(i32::MAX)
                    }
                };

                // SAFETY: epoll_wait fills `events[0..n]`; on -1 we inspect errno.
                let n = libc::epoll_wait(
                    (*this).fd,
                    events.as_mut_ptr(),
                    events.len() as libc::c_int,
                    timeout,
                );
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
                    if data == (*this).eventfd.fd as u64 {
                        // Drain the coalesced counter; ignore errors (WouldBlock).
                        let _ = (*this).eventfd.drain();
                        continue;
                    }

                    let c: *mut Completion = data as *mut Completion;

                    // We get the fd and mark this as in progress we can properly
                    // clean this up later.
                    let fd = if (*c).flags.dup {
                        Some((*c).flags.dup_fd)
                    } else {
                        Completion::fd(c)
                    };
                    let close_dup = (*c).flags.dup;
                    (*c).flags.state = State::Dead;

                    let res = Completion::perform(c, Some(ev));
                    let cb = (*c).callback;
                    let ud = (*c).userdata;
                    let action = cb(ud, this, c, res);
                    match action {
                        CallbackAction::Disarm => {
                            // We can't use self.stop because we can't trust
                            // that c is still a valid pointer.
                            if let Some(v) = fd {
                                // SAFETY: DEL on a fd we previously ADDed; never fails.
                                libc::epoll_ctl(
                                    (*this).fd,
                                    libc::EPOLL_CTL_DEL,
                                    v,
                                    ptr::null_mut(),
                                );

                                if close_dup {
                                    libc::close(v);
                                }
                            }

                            (*this).active -= 1;
                        }

                        // For epoll, epoll remains armed by default. We have to
                        // reset the state, that is all.
                        CallbackAction::Rearm => (*c).flags.state = State::Active,
                    }
                }

                if wait == 0 {
                    break;
                }
                wait_rem = wait_rem.saturating_sub(n);
            }

            Ok(())
        }
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
    unsafe fn start(this: *mut Loop, completion: *mut Completion) {
        unsafe {
            // res_ is Some(result) if the op completed synchronously (an immediate
            // result to hand to the callback), or None if it registered with epoll
            // (or the timer heap) and we should mark it active.
            let res_: Option<Result> = match &(*completion).op {
                Operation::Noop => {
                    (*completion).flags.state = State::Dead;
                    return;
                }

                Operation::Cancel { c } => {
                    let target = *c;
                    // We stop immediately. We only stop if we are in the
                    // "adding" state because cancellation or any other action
                    // means we're complete already.
                    if (*completion).flags.state == State::Adding {
                        if matches!(&(*target).op, Operation::Cancel { .. }) {
                            panic!("cannot cancel a cancellation");
                        }
                        Loop::stop_completion(this, target);
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
                    if (*completion).flags.threadpool {
                        match (*this).thread_schedule(completion) {
                            Ok(()) => return,
                            Err(()) => Some(Result::Read(Err(ReadError::ThreadPoolRequired))),
                        }
                    } else {
                        match Completion::fd_maybe_dup(completion) {
                            // DupFailed — surfaced as an immediate read error.
                            Err(()) => Some(Result::Read(Err(ReadError::Unexpected))),
                            Ok(reg_fd) => {
                                let mut ev = libc::epoll_event {
                                    events: (libc::EPOLLIN | libc::EPOLLRDHUP) as u32,
                                    u64: completion as u64,
                                };
                                // SAFETY: register `reg_fd` for read readiness with our epoll fd.
                                let rc = libc::epoll_ctl(
                                    (*this).fd,
                                    libc::EPOLL_CTL_ADD,
                                    reg_fd,
                                    &mut ev,
                                );
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
                    if (*completion).flags.threadpool {
                        match (*this).thread_schedule(completion) {
                            Ok(()) => return,
                            Err(()) => Some(Result::PRead(Err(ReadError::ThreadPoolRequired))),
                        }
                    } else {
                        match Completion::fd_maybe_dup(completion) {
                            Err(()) => Some(Result::PRead(Err(ReadError::Unexpected))),
                            Ok(reg_fd) => {
                                let mut ev = libc::epoll_event {
                                    events: (libc::EPOLLIN | libc::EPOLLRDHUP) as u32,
                                    u64: completion as u64,
                                };
                                // SAFETY: register `reg_fd` for read readiness with our epoll fd.
                                let rc = libc::epoll_ctl(
                                    (*this).fd,
                                    libc::EPOLL_CTL_ADD,
                                    reg_fd,
                                    &mut ev,
                                );
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
                    // N434: the heap keeps this pointer; it carries the
                    // completion's own tag (`timer_ptr`).
                    let v_ptr: *mut Timer = timer_ptr(completion);
                    (*v_ptr).c = completion;

                    // Insert the timer into our heap.
                    (*this).timers.insert(v_ptr);

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
                    match (*this).epoll_register(completion, libc::EPOLLIN as u32) {
                        Ok(()) => None,
                        Err(()) => Some(Result::Accept(Err(AcceptError::Unexpected))),
                    }
                }

                Operation::Connect { socket: _, addr } => {
                    let addr_copy = *addr;
                    match Completion::fd_maybe_dup(completion) {
                        Err(()) => Some(Result::Connect(Err(ConnectError::Unexpected))),
                        Ok(fd) => {
                            // SAFETY: connect on the (possibly dup'd) socket fd; `addr`
                            // is a valid `Address` held by value in the op.
                            let rc = libc::connect(
                                fd,
                                ptr::addr_of!(addr_copy) as *const libc::sockaddr,
                                addr_copy.get_os_sock_len(),
                            );
                            if rc == 0 {
                                Some(Result::Connect(Ok(())))
                            } else {
                                match errno() {
                                    // If we would block then we register for OUT events
                                    // and are notified of connection completion that way.
                                    libc::EINPROGRESS | libc::EAGAIN => {
                                        match (*this)
                                            .epoll_register(completion, libc::EPOLLOUT as u32)
                                        {
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
                    if (*completion).flags.threadpool {
                        match (*this).thread_schedule(completion) {
                            Ok(()) => return,
                            Err(()) => Some(Result::Write(Err(WriteError::ThreadPoolRequired))),
                        }
                    } else {
                        match (*this).epoll_register(completion, libc::EPOLLOUT as u32) {
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
                    if (*completion).flags.threadpool {
                        match (*this).thread_schedule(completion) {
                            Ok(()) => return,
                            Err(()) => Some(Result::PWrite(Err(WriteError::ThreadPoolRequired))),
                        }
                    } else {
                        match (*this).epoll_register(completion, libc::EPOLLOUT as u32) {
                            Ok(()) => None,
                            Err(()) => Some(Result::PWrite(Err(WriteError::Unexpected))),
                        }
                    }
                }

                Operation::Send { .. } => {
                    match (*this).epoll_register(completion, libc::EPOLLOUT as u32) {
                        Ok(()) => None,
                        Err(()) => Some(Result::Send(Err(WriteError::Unexpected))),
                    }
                }

                Operation::Recv { .. } => {
                    match (*this)
                        .epoll_register(completion, (libc::EPOLLIN | libc::EPOLLRDHUP) as u32)
                    {
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
                    match (*this).epoll_register(completion, libc::EPOLLOUT as u32) {
                        Ok(()) => None,
                        Err(()) => Some(Result::SendMsg(Err(WriteError::Unexpected))),
                    }
                }

                Operation::RecvMsg { .. } => {
                    match (*this)
                        .epoll_register(completion, (libc::EPOLLIN | libc::EPOLLRDHUP) as u32)
                    {
                        Ok(()) => None,
                        Err(()) => Some(Result::RecvMsg(Err(ReadError::Unexpected))),
                    }
                }

                Operation::Poll { events, .. } => {
                    let ev = *events;
                    match (*this).epoll_register(completion, ev) {
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
                    if (*completion).flags.threadpool {
                        match (*this).thread_schedule(completion) {
                            Ok(()) => return,
                            Err(()) => Some(Result::Close(Err(CloseError::ThreadPoolRequired))),
                        }
                    } else {
                        // SAFETY: closing a caller-owned fd.
                        libc::close(fd);
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
                    let rc = libc::shutdown(socket, how_i);
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
                (*completion).flags.state = State::Dead;

                let cb = (*completion).callback;
                let ud = (*completion).userdata;
                match cb(ud, this, completion, res) {
                    CallbackAction::Disarm => {}

                    // If we rearm then we requeue this. Due to the way that tick
                    // works, this won't try to re-add immediately, it won't happen
                    // until the next tick.
                    CallbackAction::Rearm => (*this).add(completion),
                }

                return;
            }

            // Mark the completion as active if we reached this point.
            (*completion).flags.state = State::Active;

            // Increase our active count.
            (*this).active += 1;
        }
    }

    /// # Safety
    /// `completion` must be a valid completion pointer owned by this loop.
    unsafe fn stop_completion(this: *mut Loop, completion: *mut Completion) {
        unsafe {
            // Delete. This should never fail.
            let maybe_fd = if (*completion).flags.dup {
                Some((*completion).flags.dup_fd)
            } else {
                Completion::fd(completion)
            };

            if let Some(fd) = maybe_fd {
                // SAFETY: DEL on a fd we previously ADDed; never fails.
                libc::epoll_ctl((*this).fd, libc::EPOLL_CTL_DEL, fd, ptr::null_mut());
            } else {
                // PORT NOTE: `switch (completion.op) { .timer => |*v| {...}, else => unreachable }`
                // — the sole fd-less stoppable op is `.timer`.
                // N434: a shared match; the Timer is reached through
                // `timer_ptr`, never `&mut (*completion).op`.
                match &(*completion).op {
                    Operation::Timer(_) => {
                        let v: *mut Timer = timer_ptr(completion);
                        let c = (*v).c;

                        if (*c).flags.state == State::Active {
                            // Timers need to be removed from the timer heap.
                            (*this).timers.remove(v);
                        }

                        // If the timer was never fired, we need to fire it with
                        // the cancellation notice.
                        if (*c).flags.state != State::Dead {
                            // If we have reset set AND we got a cancellation result,
                            // that means that we were canceled so that we can update
                            // our expiration time.
                            if let Some(r) = (*v).reset {
                                (*v).next = r;
                                (*v).reset = None;
                                (*this).active -= 1;
                                Loop::start(this, c);
                                return;
                            }

                            let cb = (*c).callback;
                            let ud = (*c).userdata;
                            let action = cb(ud, this, c, Result::Timer(Ok(TimerTrigger::Cancel)));
                            match action {
                                CallbackAction::Disarm => {}
                                CallbackAction::Rearm => {
                                    (*this).active -= 1;
                                    Loop::start(this, c);
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
            if (*completion).flags.state == State::Active {
                (*this).active -= 1;
            }

            // Mark the completion as done.
            (*completion).flags.state = State::Dead;
        }
    }
}

mod completion;
pub use self::completion::*;

mod async_timer_process;
pub use self::async_timer_process::*;

mod stream;
pub use self::stream::*;

mod tcp;
pub use self::tcp::*;

mod udp;
pub use self::udp::*;

mod file;
pub use self::file::*;

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
//
// UPSTREAM DRIFT (unabsorbed): the libxev bundled by the ghostty pin 6ad1fe7d
// (libxev 9ce8e8e6) asserts 184 (epoll.zig:1458), down from the 208 this port
// tracks (libxev 34fa5087). The bump is a documented non-port
// (`.port/phase-3/NON_PORTS.md` G3; rustty-xev is a D6 Zig-port-defer and does
// not track libxev revisions) — a future bump absorption re-measures against
// 184, not 208.
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
mod tests;
