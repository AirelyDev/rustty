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
use crate::linux::{CallbackAction, CompletionState, RunMode};
use crate::queue;
use crate::Error;

// PORT NOTE: the reference `init(options: Options)` carries `Options` (entries +
// thread_pool). ghostty only calls `Loop.init(.{})` and the thread-pool path is
// deferred (see the Loop PORT NOTE), so `linux::Options` is not consumed yet; it
// lands with Task 4's blocking-op path.

// PORT NOTE: `posix.fd_t` — the C file descriptor int.
pub type fd_t = libc::c_int;
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

/// PORT NOTE: `pub const Loop = struct { ... }`.
//
// PORT NOTE (deferred fields): the reference also carries `thread_pool:
// ?*ThreadPool` + `thread_pool_completions: TaskCompletionQueue` for blocking
// ops that epoll can't do. The Linux `ThreadPool` body is a whole follow-up port
// (R5 forbids importing the macOS one; `Options.thread_pool` is an opaque
// `*mut c_void` on Linux), and no L1 LT3 test uses it, so the thread-pool
// machinery is omitted here and lands with Task 4's blocking-op path.
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

// PORT NOTE: `Loop` holds raw-ptr intrusive queues/heap + an fd → auto-`!Send`;
// ghostty MOVES a `Loop` onto the spawned termio thread. SAFETY / single-thread
// affinity: after the move a `Loop` (and every `Completion` it links) is touched
// by exactly one thread at a time — the owning event-loop thread — so
// transferring ownership is sound. The cross-thread wakeup rides the `Eventfd`
// (a plain `Copy` fd, auto-`Send`), never a shared `&mut Loop`.
unsafe impl Send for Loop {}

impl Loop {
    /// Initialize a new epoll-backed event loop.
    //
    // PORT NOTE: reference `init(options: Options)`; ghostty only calls
    // `Loop.init(.{})`, so this specializes to default `Options` (the
    // thread-pool option is deferred with the pool body). `Async.init()` →
    // `Eventfd::init()`.
    pub fn init() -> core::result::Result<Loop, Error> {
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
                Operation::Cancel { .. } | Operation::Timer(_) => {
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
                    ms_next.saturating_sub(ms_now) as i32
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

    /// Sends an empty message to this loop's eventfd so that it wakes up.
    fn wakeup(&mut self) -> core::result::Result<(), Error> {
        self.eventfd.notify().map_err(|_| Error)
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

            Operation::Read { .. } => match unsafe { Completion::fd_maybe_dup(completion) } {
                // DupFailed — surfaced as an immediate read error.
                Err(()) => Some(Result::Read(Err(ReadError::Unexpected))),
                Ok(reg_fd) => {
                    let mut ev = libc::epoll_event {
                        events: (libc::EPOLLIN | libc::EPOLLRDHUP) as u32,
                        u64: completion as u64,
                    };
                    // SAFETY: register `reg_fd` for read readiness with our epoll fd.
                    let rc =
                        unsafe { libc::epoll_ctl(self.fd, libc::EPOLL_CTL_ADD, reg_fd, &mut ev) };
                    if rc < 0 {
                        Some(Result::Read(Err(ReadError::Unexpected)))
                    } else {
                        None
                    }
                }
            },

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
}

/// PORT NOTE: the `flags: packed struct { state, threadpool, dup, dup_fd }`
/// sub-struct. The `threadpool` bit is deferred with the thread-pool path
/// (Task 4); `dup`/`dup_fd` back the epoll single-op-per-fd dup hack.
#[derive(Clone, Copy)]
struct CompletionFlags {
    /// Watch state of this completion. We use this to determine whether
    /// we're active, adding, deleting, etc. This lets us add and delete
    /// multiple times before a loop tick and handle the state properly.
    state: State,

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
                dup: false,
                dup_fd: 0,
            },
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

    /// Perform the operation associated with this completion. This performs
    /// the full blocking operation for the completion.
    ///
    /// # Safety
    /// `self_` valid+stable; `ev_` (if Some) a valid epoll_event for this op.
    unsafe fn perform(self_: *mut Completion, _ev: Option<*const libc::epoll_event>) -> Result {
        match unsafe { &mut (*self_).op } {
            // These never reach perform: they complete synchronously in `start`
            // or the timer heap.
            Operation::Cancel { .. } | Operation::Noop | Operation::Timer(_) => {
                unreachable!()
            }

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
            Operation::Read { fd, .. } => Some(*fd),

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
// `op: Operation = .{ .noop = {} }` — the `#[default]` `Noop` variant. The full
// reference tag set (accept/connect/poll/pread/write/pwrite/send/recv/sendmsg/
// recvmsg/close/shutdown) lands with Task 4's socket/process/stream scope; this
// task carries the loop-core tags (noop/cancel/timer) + the single fd op the L1
// LT3 KATs exercise (read, via `epoll: timerfd`).
#[derive(Default)]
pub enum Operation {
    #[default]
    Noop,

    Cancel {
        c: *mut Completion,
    },

    Read {
        fd: fd_t,
        buffer: ReadBuffer,
    },

    Timer(Timer),
}

/// PORT NOTE: `pub const Result = union(OperationType) { ... }`. Mirrors the
/// `Operation` tag subset ported for Task 3.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Result {
    Noop,
    Cancel(core::result::Result<(), CancelError>),
    Read(core::result::Result<usize, ReadError>),
    Timer(core::result::Result<TimerTrigger, TimerError>),
}

/// PORT NOTE: `pub const CancelError = ThreadPoolError || error{NotFound}`. Only
/// the `Ok(())` result is produced by the ported paths; the variant is kept so
/// the result type is nameable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelError {
    NotFound,
}

/// PORT NOTE: `pub const ReadError = ... || error{DupFailed, EOF, Unknown}`. The
/// posix syscall-error variants collapse to `Unexpected` (no in-scope test
/// exercises a raw read-error path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
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

// PORT NOTE: `posix.read` wrapper the `perform` read path calls. Raw `libc`.
unsafe fn posix_read(fd: fd_t, ptr: *mut u8, len: usize) -> core::result::Result<usize, ReadError> {
    let n = unsafe { libc::read(fd, ptr.cast(), len) };
    if n < 0 {
        Err(ReadError::Unexpected)
    } else {
        Ok(n as usize)
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
const COMPLETION_SIZE: usize = 104;
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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();

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
}
