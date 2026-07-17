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
use crate::linux::{CallbackAction, CompletionState, RunMode};
use crate::queue;
use crate::Error;
use core::any::TypeId;
use core::ffi::c_void;
use core::ptr::NonNull;

// PORT NOTE: the reference `init(options: Options)` carries `Options` (entries +
// thread_pool). ghostty only calls `Loop.init(.{})` and the thread-pool path is
// deferred (see the Loop PORT NOTE), so `linux::Options` is not consumed yet; it
// lands with Task 4's blocking-op path.

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

            Operation::Write { .. } => {
                match unsafe { self.epoll_register(completion, libc::EPOLLOUT as u32) } {
                    Ok(()) => None,
                    Err(()) => Some(Result::Write(Err(WriteError::Unexpected))),
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
                // `flags.threadpool` is set (Stream.close does, epoll arm). The pool
                // body is DEFERRED (type-surface only, per the brief) — no pool is
                // ever attached on Linux — so a threadpool-flagged close yields
                // ThreadPoolRequired exactly as the reference's `thread_schedule`
                // would with no pool. Un-flagged close (the raw socket-test path)
                // runs the `posix.close` synchronously here.
                if unsafe { (*completion).flags.threadpool } {
                    Some(Result::Close(Err(CloseError::ThreadPoolRequired)))
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

    /// Schedule this onto the threadpool rather than epoll. Not all
    /// operations support this.
    //
    // PORT NOTE: the thread-pool work-stealing body is DEFERRED (type-surface
    // only, per the brief — the shell path attaches no pool). The bit is honored
    // by the `.close` start arm (Stream.close sets it, epoll arm): with no pool
    // attached it yields `ThreadPoolRequired`, exactly as the reference's
    // `thread_schedule` does when `Loop.thread_pool == null`.
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

            Operation::Write { fd, buffer } => Result::Write(match buffer {
                WriteBuffer::Slice(v) => unsafe { posix_write(*fd, (*v).cast::<u8>(), (*v).len()) },
                WriteBuffer::Array { array, len } => unsafe {
                    posix_write(*fd, array.as_ptr(), *len)
                },
            }),

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

            // PORT NOTE: `close` completes synchronously in `start` (or via the
            // deferred threadpool). It only reaches `perform` on the threadpool
            // path, which yields ThreadPoolRequired in `start` — so this arm is
            // unreachable in the ported (pool-less) surface, but the reference's
            // `posix.close` body is preserved for fidelity.
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
            Operation::Recv { fd, .. } => Some(*fd),
            Operation::Write { fd, .. } => Some(*fd),
            Operation::Send { fd, .. } => Some(*fd),
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
// shutdown); the reference's pread/pwrite/sendmsg/recvmsg tags stay DEFERRED (no
// in-scope test/shell-path use — the brief scopes the socket subset).
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

    Write {
        fd: fd_t,
        buffer: WriteBuffer,
    },

    Send {
        fd: fd_t,
        buffer: WriteBuffer,
    },

    Recv {
        fd: fd_t,
        buffer: ReadBuffer,
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
    Write(core::result::Result<usize, WriteError>),
    Send(core::result::Result<usize, WriteError>),
    Recv(core::result::Result<usize, ReadError>),
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

/// PORT NOTE: `pub const WriteError = ... || posix.WriteError || posix.SendError
/// || error{DupFailed, Unknown}`. Collapsed to `Unexpected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
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
// RE-MEASURED (Task 4): 104 → 152. The Task-4 socket/proc/stream ops widened the
// `Operation` union: `Connect` embeds `net::Address` (a `sockaddr` union whose
// largest member is `sockaddr_un`, ~112 bytes), which now dominates the tag's
// payload. The `threadpool` flag bit was also added. 152 is the new measured
// Rust size on aarch64-unknown-linux-gnu.
const COMPLETION_SIZE: usize = 152;
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
        let mut loop_ = Loop::init().unwrap();

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

        let mut loop_ = Loop::init().unwrap();
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
        let mut loop_ = Loop::init().unwrap();

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
        let mut loop_ = Loop::init().unwrap();
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
}
