//! io_uring.rs — faithful port of libxev `backend/io_uring.zig` (XEV_BLUEPRINT §1
//! Linux port, tasks L1 LT5 (ring + `Loop` core) and L1 LT6 (the remaining ops +
//! the watcher surfaces)).
//!
//! Backend to use io_uring. This is a SELF-CONTAINED whole backend, parallel to
//! `epoll.rs` (and the macOS `kqueue.rs`): it defines its own `Loop`/`Completion`/
//! `Operation`/`Result`/`Callback` locally and builds ONLY on the shared
//! cross-platform data structure (`queue`) and the Linux primitives/ABI enums
//! (`linux::{RunMode, CallbackAction, CompletionState, Options}`,
//! `linux::eventfd::Eventfd`, `linux::net`). It never imports from any
//! macOS-gated file (R5).
//!
//! Like `epoll.rs`, this backend also carries its OWN watcher surfaces —
//! `Async` (watcher/async.zig `AsyncEventFd`), `Process` (watcher/process.zig
//! `ProcessPidFd`) and `Stream`/`WriteQueue`/`WriteRequest` (watcher/stream.zig
//! `GenericStream`) — rather than importing the crate's macOS-gated `async.rs`/
//! `process.rs`/`stream.rs`. In the reference those files are `comptime`-generic
//! over `xev`; each backend gets its own monomorphization, and these are
//! io_uring's.
//
// PORT NOTE (THE MODEL — why this file looks nothing like epoll.rs): io_uring is
// SUBMISSION-based, not readiness-based. There is no "register an fd, get told
// when it's ready" step: you fill a Submission Queue Entry (SQE) describing the
// whole operation, hand the ring to the kernel with one `io_uring_enter`, and the
// kernel posts a Completion Queue Entry (CQE) carrying the operation's RESULT
// (`res`, a negative errno on failure) and your `user_data`. Consequences that
// shape this port vs. epoll.rs:
//  - There is NO `perform` step. epoll's `start` registers and `perform` runs the
//    syscall when the fd fires; here `add_` fills the SQE and `invoke` merely
//    DECODES `cqe.res` into a `Result`. All the syscall work happened in-kernel.
//  - There is NO timer heap and NO wakeup eventfd. epoll derives timeouts from a
//    pairing heap and needs an eventfd to break `epoll_wait`; io_uring expresses a
//    timer AS AN OPERATION (`IORING_OP_TIMEOUT` with `IORING_TIMEOUT_ABS`), so the
//    kernel owns the timer and `io_uring_enter` returns when it fires. Hence this
//    `Loop` carries neither `heap::Intrusive` nor `linux::eventfd::Eventfd`
//    (the reference carries neither either).
//  - `submissions` means something DIFFERENT here. In epoll it is the queue of
//    work to register on the next tick; in io_uring it is the OVERFLOW queue of
//    completions that could not get an SQE because the ring was full.
//
// PORT NOTE (R3 — the ring is hand-rolled over raw libc syscalls): the reference
// rides Zig's `std.os.linux.IoUring`, itself a thin wrapper over
// `io_uring_setup`/`io_uring_enter` + three `mmap`s. `libc` 0.2 exposes ONLY the
// syscall numbers (`SYS_io_uring_setup`/`SYS_io_uring_enter`) — it carries NO
// `io_uring_params`/`io_uring_sqe`/`io_uring_cqe` struct and NO `IORING_*`
// constant (verified against libc 0.2.186) — so the whole ABI is hand-declared
// below, per the brief's "hand-declare only what libc lacks". No `io-uring`/`nix`/
// `mio` crate is added; the crate's dep set stays `{rustty-global, libc}`.
//
// PORT NOTE (the load-bearing disciplines, mirrored from epoll.rs / kqueue.rs §2):
//  - RAW POINTERS end-to-end: the `Loop` holds `*mut Completion` into caller
//    memory; callbacks get `*mut Loop` + `*mut Completion`; links are NEVER
//    `&mut`/lifetimes (Stacked-Borrows UB). A callback may legally re-add / rearm
//    its completion — exactly libxev's contract.
//  - `unsafe impl Send` for `Loop`/`Completion` (documented at each impl).
//  - hand-written `Default` for `Completion`: fn-ptr + raw-ptr fields.
//  - the `const _: () = assert!(size_of::<Completion>() == N)` layout tripwire.
//
// MIRI_EXCEPTIONS: this whole module is an isolated-Miri EXCEPTION and cannot be
// run under Miri at all. The ring's SQ/CQ live in `mmap`'d, KERNEL-SHARED memory
// obtained from `io_uring_setup` + `mmap(..., MAP_SHARED, ring_fd, IORING_OFF_*)`;
// Miri implements neither `mmap` of a device fd nor the `io_uring_setup`/
// `io_uring_enter` syscalls, and it cannot model the kernel as a concurrent writer
// of the CQ tail / SQ head. RAW ALIASING NOTE: every head/tail index is touched
// ONLY through a raw `*mut u32` reborrowed per-access via `AtomicU32::from_ptr`
// (never a long-lived `&mut`), so the kernel's concurrent stores never race a Rust
// reference; the `sqes`/`cqes`/`array` regions are likewise reached by raw-pointer
// arithmetic off the mmap base and are never covered by a Rust reference. The
// pure data structures this backend rides (`queue`) remain Miri-clean on their own
// tests. See also the `Sq`/`Cq` SAFETY notes.
#![allow(dead_code)]
// PORT NOTE: `ms_per_s`/`ns_per_ms`/`ns_per_s`/`fd_t` keep their Zig/`std.posix`
// snake_case identifiers (R2 identifier-fidelity), the `io_uring_*`/`IORING_*` ABI
// decls keep their KERNEL spelling, and `ReadError::EOF` keeps the reference's
// acronym spelling — mirroring epoll.rs/kqueue.rs's module-level allows.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(clippy::upper_case_acronyms)]

use core::any::TypeId;
use core::ffi::c_void;
use core::mem::size_of;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::linux::eventfd::Eventfd;
use crate::linux::net;
use crate::linux::{CallbackAction, CompletionState, Options, RunMode};
use crate::queue;
use crate::Error;

// PORT NOTE: `posix.fd_t` — the C file descriptor int.
pub type fd_t = libc::c_int;
/// PORT NOTE: `posix.socket_t` — the socket handle (a C int on Linux).
pub type socket_t = libc::c_int;
/// PORT NOTE: `posix.pid_t` — the process id.
pub type pid_t = libc::pid_t;

// PORT NOTE: `watcher/common.zig`'s `userdataValue` — the typed→untyped userdata
// bridge the io_uring `Async`/`Process`/`Stream` trampolines use. Kept local so
// this backend stays self-contained (the crate's `common.rs` is macOS-gated, R5),
// exactly as epoll.rs keeps its own copy. Zig `void` userdata ⇒ a null pointer;
// Rust's stand-in is the zero-sized unit `()`.
#[inline]
fn userdata_value<UD: 'static>(v: *mut c_void) -> *mut UD {
    if TypeId::of::<UD>() == TypeId::of::<()>() {
        return ptr::null_mut();
    }
    v as *mut UD
}

/// PORT NOTE: `posix.SOCK.CLOEXEC` — the accept-op `flags` default.
pub const SOCK_CLOEXEC: u32 = libc::SOCK_CLOEXEC as u32;

/// PORT NOTE: `posix.timespec` — the monotonic-clock instant the loop caches.
pub type Timespec = libc::timespec;

// PORT NOTE: `std.time.*` constants used by `now`/`update_now`/timer math.
const ms_per_s: u64 = 1_000;
const ns_per_ms: u64 = 1_000_000;
const ns_per_s: u64 = 1_000_000_000;

// PORT NOTE: `const errno` reader — `posix.errno(rc)` reads the C `errno` global.
#[inline]
fn errno() -> i32 {
    // SAFETY: `__errno_location()` returns the thread-local errno location.
    unsafe { *libc::__errno_location() }
}

mod ring;
pub use self::ring::*;

// ---------------------------------------------------------------------------
// available() (io_uring.zig:14).
// ---------------------------------------------------------------------------

/// True if this backend is available on this platform.
//
// PORT NOTE (verbatim from the reference): Perform a harmless syscall to determine
// if we have io_uring support. We should really only be checking for a subset of
// possible errors to determine if io_uring is available but there isn't really a
// strong course of action for any other error so we just mark it as unavailable.
//
// Also, this is a bit expensive -- we could get away with a single
// io_uring_register syscall to check for availability but that involves manually
// handling errors vs. the tried and true Zig stdlib here.
//
// PORT NOTE: `if (comptime builtin.os.tag != .linux) return false;` — this module
// compiles only on Linux (`#[cfg(target_os = "linux")]` in lib.rs), so that arm is
// comptime-elided. Every `IoUring::init` failure (ENOSYS on a kernel without
// io_uring, EPERM under `kernel.io_uring_disabled=2`, ...) degrades to `false`
// gracefully — no panic, exactly as the reference's `catch return false`.
pub fn available() -> bool {
    // var ring = linux.IoUring.init(256, 0) catch return false;
    let mut ring = match IoUring::init(256, 0) {
        Ok(v) => v,
        Err(_) => return false,
    };
    // ring.deinit();
    ring.deinit();
    true
}

/// The callback of the main Loop operations.
//
// PORT NOTE: `loop.zig`'s `Callback(@This())` monomorphized for the io_uring
// backend — a concrete fn-pointer referencing THIS module's `Loop`/`Completion`/
// `Result` (self-contained, per the brief). `loop_`/`completion` are RAW pointers.
pub type Callback = fn(
    userdata: *mut core::ffi::c_void,
    loop_: *mut Loop,
    completion: *mut Completion,
    result: Result,
) -> CallbackAction;

/// A callback that does nothing and immediately disarms. This implements the
/// io_uring `Callback` and is the default value for completions.
//
// PORT NOTE: `loop.zig`'s `NoopCallback(@This())` monomorphized for io_uring.
pub fn noop_callback(
    _: *mut core::ffi::c_void,
    _: *mut Loop,
    _: *mut Completion,
    _: Result,
) -> CallbackAction {
    CallbackAction::Disarm
}

// ---------------------------------------------------------------------------
// The Loop (io_uring.zig:31).
// ---------------------------------------------------------------------------

/// PORT NOTE: the error set of `Loop.init` — `error{TooManyEntries}` (the `u13`
/// entries cast) unioned with `IoUring.init`'s errors.
//
// PORT NOTE: this is a LOCAL error enum rather than the crate's unified `Error`
// because the `io_uring: overflow entries count` test (io_uring.zig:1128)
// discriminates `error.TooManyEntries` specifically — a unit-struct `Error` could
// not express that assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitError {
    /// `options.entries` did not fit the reference's `u13` cast (> 8191).
    TooManyEntries,
    /// The ring itself could not be created.
    Ring(RingInitError),
}

/// PORT NOTE: `Loop.tick_`'s error set — `error{NestedRunsNotAllowed}` unioned
/// with `SubmitError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    NestedRunsNotAllowed,
    Submit(SubmitError),
}

/// PORT NOTE: `pub const Loop = struct { ... }`.
//
// PORT NOTE (vs. epoll's Loop): NO `timers` heap and NO `eventfd` — io_uring
// expresses timers as `IORING_OP_TIMEOUT` operations owned by the kernel, and
// `io_uring_enter` needs no eventfd to be interruptible. See the module MODEL note.
pub struct Loop {
    ring: IoUring,

    /// The number of active completions. This DOES NOT include completions that
    /// are queued in the submissions queue.
    active: usize,

    /// Our queue of submissions that failed to enqueue.
    //
    // PORT NOTE: unlike epoll's `submissions` (work to register next tick), this is
    // the OVERFLOW queue: completions that could not get an SQE because the ring
    // was full, retried by `submit`.
    submissions: queue::Intrusive<Completion>,

    /// Cached time
    cached_now: Timespec,

    flags: LoopFlags,
}

/// PORT NOTE: the `flags: packed struct { now_outdated, stopped, in_run }` sub-struct.
#[derive(Clone, Copy)]
struct LoopFlags {
    /// True if the "now" field is outdated and should be updated when it is used.
    now_outdated: bool,
    /// Whether the loop is stopped or not.
    stopped: bool,
    /// Whether we're in a run or not (to prevent nested runs).
    in_run: bool,
}

// PORT NOTE: the Zig field defaults are `now_outdated = true`, `stopped = false`,
// `in_run = false` — `#[derive(Default)]` would render `now_outdated` as `false`,
// so `Default` is hand-written (the `true` default is load-bearing: a fresh
// `Loop.now()` must read the clock).
impl Default for LoopFlags {
    fn default() -> Self {
        LoopFlags {
            now_outdated: true,
            stopped: false,
            in_run: false,
        }
    }
}

// PORT NOTE: `Loop` holds the ring's raw mmap pointers + a raw-ptr intrusive queue
// + an fd → auto-`!Send`; ghostty MOVES a `Loop` onto the spawned termio thread.
// SAFETY / single-thread affinity: after the move a `Loop` (and every `Completion`
// it links) is touched by exactly one thread at a time — the owning event-loop
// thread — so transferring ownership is sound. The ring's SQ/CQ are shared with the
// KERNEL, not with another Rust thread; that sharing is mediated entirely by the
// head/tail atomics (see the ring-atomics note above), which is exactly what makes
// the single-owner-thread invariant sufficient here.
unsafe impl Send for Loop {}

impl Loop {
    /// Initialize the event loop. "entries" is the maximum number of
    /// submissions that can be queued at one time. The number of completions
    /// always matches the number of entries so the memory allocated will be
    /// 2x entries (plus the basic loop overhead).
    pub fn init(options: Options) -> core::result::Result<Loop, InitError> {
        // const entries = std.math.cast(u13, options.entries) orelse
        //     return error.TooManyEntries;
        //
        // PORT NOTE: Zig's `@as(u13, ...)` cast fails for any value that doesn't fit
        // 13 bits, i.e. > 8191. Rust has no u13, so the bound is spelled explicitly;
        // `2^13 = 8192` (the reference test's value) is the first rejected entry count.
        const MAX_ENTRIES: u32 = (1 << 13) - 1;
        if options.entries > MAX_ENTRIES {
            return Err(InitError::TooManyEntries);
        }
        let entries = options.entries as u16;

        let mut result: Loop = Loop {
            // TODO(mitchellh): add an init_advanced function or something
            // for people using the io_uring API directly to be able to set
            // the flags for this.
            ring: IoUring::init(entries, 0).map_err(InitError::Ring)?,
            active: 0,
            submissions: queue::Intrusive::default(),
            // .cached_now = undefined — populated by update_now() below.
            cached_now: unsafe { core::mem::zeroed() },
            flags: LoopFlags::default(),
        };
        result.update_now();

        Ok(result)
    }

    pub fn deinit(&mut self) {
        self.ring.deinit();
    }

    /// Run the event loop. See RunMode documentation for details on modes.
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
    pub fn run(&mut self, mode: RunMode) -> core::result::Result<(), RunError> {
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
    pub unsafe fn run_raw(this: *mut Loop, mode: RunMode) -> core::result::Result<(), RunError> {
        // PORT NOTE: the reference's `tick_` takes the mode COMPTIME ("so we can do
        // some tricks to avoid function calls and runtime branching"). Rust has no
        // comptime-parameter monomorphization over a value, so the mode is a runtime
        // parameter (exactly as epoll.rs's `tick` takes a runtime `wait`); the
        // branch arms are identical.
        unsafe { Loop::tick_(this, mode) }
    }

    /// Stop the loop. This can only be called from the main thread.
    /// This will stop the loop forever. Future ticks will do nothing.
    /// All completions are safe to read/write once any outstanding
    /// `run` or `tick` calls are returned.
    pub fn stop(&mut self) {
        self.flags.stopped = true;
    }

    /// Returns true if the loop is stopped. This may mean there
    /// are still pending completions to be processed.
    pub fn stopped(&self) -> bool {
        self.flags.stopped
    }

    /// Returns the "loop" time in milliseconds. The loop time is updated
    /// once per loop tick, before IO polling occurs. It remains constant
    /// throughout callback execution.
    ///
    /// You can force an update of the "now" value by calling update_now()
    /// at any time from the main thread.
    ///
    /// The clock that is used is not guaranteed. In general, a monotonic
    /// clock source is always used if available. This value should typically
    /// just be used for relative time calculations within the loop, such as
    /// answering the question "did this happen <x> ms ago?".
    //
    // PORT NOTE: takes `&mut self` (not epoll's `&self`) because the reference
    // `now(self: *Loop)` lazily refreshes the cache via the `now_outdated` flag.
    pub fn now(&mut self) -> i64 {
        if self.flags.now_outdated {
            self.update_now();
        }

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
            self.flags.now_outdated = false;
        } else {
            // Errors are ignored.
        }
    }

    /// Tick the loop.
    unsafe fn tick_(this: *mut Loop, mode: RunMode) -> core::result::Result<(), RunError> {
        unsafe {
            // We can't nest runs.
            if (*this).flags.in_run {
                return Err(RunError::NestedRunsNotAllowed);
            }
            (*this).flags.in_run = true;
            // defer self.flags.in_run = false; — via the explicit reset below.
            let result = Loop::tick_inner(this, mode);
            (*this).flags.in_run = false;
            result
        }
    }

    unsafe fn tick_inner(this: *mut Loop, mode: RunMode) -> core::result::Result<(), RunError> {
        unsafe {
            // The total number of events we're waiting for.
            let wait: u32 = match mode {
                // until_done is one because we need to wait for at least one
                // (if we have any).
                RunMode::UntilDone => 1,
                RunMode::Once => 1,
                RunMode::NoWait => 0,
            };

            // var cqes: [128]linux.io_uring_cqe = undefined;
            let mut cqes: [io_uring_cqe; 128] = core::mem::zeroed();
            loop {
                // If we're stopped then the loop is fully over.
                if (*this).flags.stopped {
                    break;
                }

                // We always note that our "now" value is outdated even if we have
                // no completions to execute.
                (*this).flags.now_outdated = true;

                if (*this).active == 0 && (*this).submissions.empty() {
                    break;
                }

                // If we have no queued submissions then we do the wait as part
                // of the submit call, because then we can do exactly once syscall
                // to get all our events.
                if (*this).submissions.empty() {
                    match (*this).ring.submit_and_wait(wait) {
                        Ok(_) => {}
                        Err(SubmitError::SignalInterrupt) => continue,
                        Err(e) => return Err(RunError::Submit(e)),
                    }
                } else {
                    // We have submissions, meaning we have to do multiple submissions
                    // anyways so we always just do non-waiting ones.
                    match (*this).submit() {
                        Ok(()) => {}
                        Err(SubmitError::SignalInterrupt) => continue,
                        Err(e) => return Err(RunError::Submit(e)),
                    }
                }

                // Wait for completions...
                let count = match (*this).ring.copy_cqes(&mut cqes, wait) {
                    Ok(v) => v,
                    // EINTR means our blocking syscall was interrupted by some
                    // process signal. We just retry when we can. See signal(7).
                    Err(SubmitError::SignalInterrupt) => continue,
                    Err(e) => return Err(RunError::Submit(e)),
                };

                for cqe in &cqes[0..count as usize] {
                    // const c = @as(?*Completion, @ptrFromInt(cqe.user_data)) orelse continue;
                    //
                    // PORT NOTE: a NULL `user_data` is the `.noop` completion's stray SQE
                    // (see `add_`'s noop arm) — the kernel posts a CQE for it, and we
                    // skip it WITHOUT touching `active` (the noop arm never counted it).
                    let c: *mut Completion = cqe.user_data as *mut Completion;
                    if c.is_null() {
                        continue;
                    }

                    (*this).active -= 1;
                    (*c).flags.state = State::Dead;
                    match Completion::invoke(c, this, cqe.res) {
                        CallbackAction::Disarm => {}
                        CallbackAction::Rearm => (*this).add(c),
                    }
                }

                // Subtract our waiters. If we reached zero then we're done.
                match mode {
                    RunMode::NoWait => break,
                    RunMode::Once => {
                        if count > 0 {
                            break;
                        }
                    }
                    RunMode::UntilDone => {}
                }
            }

            Ok(())
        }
    }

    /// Submit all queued operations. This never does an io_uring submit
    /// and wait operation.
    pub fn submit(&mut self) -> core::result::Result<(), SubmitError> {
        self.ring.submit()?;

        // If we have any submissions that failed to submit, we try to
        // send those now. We have to make a copy so that any failures are
        // resubmitted without an infinite loop.
        let mut queued = core::mem::take(&mut self.submissions);
        // self.submissions = .{};  (done by mem::take)
        loop {
            let c = unsafe { queued.pop() };
            if c.is_null() {
                break;
            }
            unsafe { self.add_(c, true) };
        }

        Ok(())
    }

    /// Add a timer to the loop. The timer will initially execute in "next_ms"
    /// from now and will repeat every "repeat_ms" thereafter. If "next_ms" is
    /// zero then the timer will invoke on the next loop tick.
    ///
    /// If next_ms is too large of a value that it doesn't fit into the
    /// kernel time structure, the maximum sleep value will be used for your
    /// system (this is at least 45 days on a 32-bit system, and thousands of
    /// years on a 64-bit system).
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
                }),
                userdata,
                callback: cb,
                ..Completion::default()
            };

            self.add(c);
        }
    }

    /// Reset a timer to trigger in next_ms.
    ///
    /// Important: c and c_cancel must NEVER be undefined. If you are using
    /// timer_reset you should always initial c to the zero value ".{}".
    ///
    /// If the timer completion "c" is dead, this is equivalent to calling
    /// "timer" and "c_cancel" is not used. You can verify this scenario by
    /// checking "c_cancel.state()" after this call.
    ///
    /// If the timer completion "c" is active, and "c_cancel" is also active,
    /// it is assumed that c_cancel is already resetting the timer. The
    /// "next_ms" time is updated but otherwise the cancellation continues
    /// and reset works.
    ///
    /// If the timer completion "c" is active, and "c_cancel" is dead,
    /// "c" is cancelled and restarted to trigger in next_ms.
    ///
    /// There is no guarantee that c_cancel will ever be used. Check the
    /// resulting state to know if you can reclaim its memory or not.
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
        if unsafe { Completion::state(c) } == CompletionState::Dead {
            unsafe { self.timer(c, next_ms, userdata, cb) };
            return;
        }

        // Update the reset time for the timer to the desired time
        // along with all the callbacks.
        let reset = self.timer_next(next_ms);
        unsafe {
            match &mut (*c).op {
                Operation::Timer(t) => t.reset = Some(reset),
                _ => unreachable!(),
            }
            (*c).userdata = userdata;
            (*c).callback = cb;
        }

        // If the cancellation is active, we assume its for this timer
        // and do nothing.
        if unsafe { Completion::state(c_cancel) } == CompletionState::Active {
            return;
        }

        debug_assert!(
            unsafe { Completion::state(c_cancel) } == CompletionState::Dead
                && unsafe { Completion::state(c) } == CompletionState::Active
        );

        // PORT NOTE (verbatim): we COULD use IORING_TIMEOUT_UPDATE here with the
        // IORING_TIMEOUT_REMOVE op. We don't right now because it complicates
        // our logic and ups our required kernel version to 5.11 while this
        // works good enough. If there is a compelling reason to use
        // IORING_TIMEOUT_UPDATE, I'm open to hearing it.
        unsafe {
            *c_cancel = Completion {
                op: Operation::TimerRemove { timer: c },
                ..Completion::default()
            };
            self.add(c_cancel);
        }
    }

    fn timer_next(&mut self, next_ms: u64) -> KernelTimespec {
        // Get the timestamp of the absolute time that we'll execute this timer.
        // There are lots of failure scenarios here in math. If we see any
        // of them we just use the maximum value.
        let max: KernelTimespec = KernelTimespec {
            sec: isize::MAX as i64,
            nsec: isize::MAX as i64,
        };

        let next_s: isize = match isize::try_from(next_ms / ms_per_s) {
            Ok(v) => v,
            Err(_) => return max,
        };
        let next_ns: isize = match isize::try_from((next_ms % ms_per_s) * ns_per_ms) {
            Ok(v) => v,
            Err(_) => return max,
        };

        if self.flags.now_outdated {
            self.update_now();
        }

        // PORT NOTE: the resulting `nsec` may exceed 1e9 (it is `cached_now.nsec +
        // next_ns`, un-normalized). This is FAITHFUL and correct: the kernel's
        // `io_timeout_prep` only rejects NEGATIVE fields, and `timespec64_to_ktime`
        // computes `sec * 1e9 + nsec`, so a carry-less timespec is handled exactly.
        KernelTimespec {
            sec: match (self.cached_now.tv_sec as isize).checked_add(next_s) {
                Some(v) => v as i64,
                None => return max,
            },
            nsec: match (self.cached_now.tv_nsec as isize).checked_add(next_ns) {
                Some(v) => v as i64,
                None => return max,
            },
        }
    }

    /// Add a completion to the loop. This does NOT start the operation!
    /// You must call "submit" at some point to submit all of the queued
    /// work.
    ///
    /// # Safety
    /// `completion` must be a valid, stable pointer to a `Completion` that lives
    /// until its callback fires (libxev's intrusive-ownership contract).
    pub unsafe fn add(&mut self, completion: *mut Completion) {
        unsafe { self.add_(completion, false) }
    }

    /// Internal add function. The only difference is try_submit. If try_submit
    /// is true, then this function will attempt to submit the queue to the
    /// ring if the submission queue is full rather than filling up our FIFO.
    ///
    /// # Safety
    /// `completion` must be a valid, stable `Completion` pointer.
    unsafe fn add_(&mut self, completion: *mut Completion, try_submit: bool) {
        // The completion at this point is active no matter what because
        // it is going to be queued.
        unsafe { (*completion).flags.state = State::Active };

        let sqe: *mut io_uring_sqe = match unsafe { self.get_sqe_or_queue(completion, try_submit) }
        {
            Some(v) => v,
            None => return,
        };

        // Increase active to the amount in the ring.
        self.active += 1;

        // Setup the submission depending on the operation
        match unsafe { &mut (*completion).op } {
            // Do nothing with noop completions.
            //
            // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): this arm returns
            // WITHOUT filling the SQE it just took from `get_sqe`, yet `get_sqe`
            // already advanced `sqe_tail` — so `flush_sq` WILL hand that stale slot
            // to the kernel. The reference does exactly this (io_uring.zig:394); it
            // is what the reference test means by "Due to implementation details, we
            // need to have an ongoing completion to test that the noop operation
            // works properly".
            //
            // Why it is benign for a FRESH slot: the kernel allocates the SQE mapping
            // with `__GFP_ZERO`, so a never-before-used slot reads as `opcode =
            // IORING_OP_NOP` (0) with `user_data = 0` — the kernel posts a harmless
            // NOP CQE and `tick_` skips any CQE with a null `user_data` (which is
            // also why the noop arm must NOT leave `active` incremented: it is
            // decremented here, and the null-user_data CQE never decrements it).
            //
            // CAVEAT (latent, inherited): if the slot was previously used by a REAL
            // op, it still holds that op's opcode AND `user_data`, so a `.noop` add
            // could re-submit a stale operation pointing at an old completion. This
            // is a reference bug, not a port artifact. It is unreachable in practice:
            // nothing in ghostty (nor any ported watcher) ever adds a `.noop`
            // completion — only libxev's own test does, and it does so on a fresh
            // ring. Flagged here so Task 6+ doesn't "discover" it as a port defect.
            // Fixing it would mean rewinding `sqe_tail`, which would DIVERGE from the
            // reference; left faithful per R3.
            Operation::Noop => {
                unsafe { (*completion).flags.state = State::Dead };
                self.active -= 1;
                return;
            }

            Operation::Accept {
                socket,
                addr,
                addr_size,
                flags,
            } => unsafe {
                prep_accept(
                    sqe,
                    *socket,
                    ptr::addr_of_mut!(*addr),
                    ptr::addr_of_mut!(*addr_size),
                    *flags,
                )
            },

            Operation::Close { fd } => unsafe { prep_close(sqe, *fd) },

            Operation::Connect { socket, addr } => unsafe {
                let len = addr.get_os_sock_len();
                prep_connect(sqe, *socket, ptr::addr_of!(addr.any), len)
            },

            Operation::Poll { fd, events } => unsafe { prep_poll_add(sqe, *fd, *events) },

            Operation::Read { fd, buffer } => unsafe {
                match buffer {
                    // offset is a u64 but if the value is -1 then it uses
                    // the offset in the fd.
                    ReadBuffer::Array(buf) => {
                        prep_read(sqe, *fd, buf.as_mut_ptr(), buf.len(), -1i64 as u64)
                    }
                    ReadBuffer::Slice(buf) => {
                        prep_read(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), -1i64 as u64)
                    }
                }
            },

            // PORT NOTE: `.pread => |*v| switch (v.buffer) { ... sqe.prep_read(v.fd,
            // buf, v.offset) }` (io_uring.zig:436) — the `.read` arm with the op's
            // actual `offset` instead of the `-1` "use the fd's offset" sentinel.
            Operation::PRead { fd, buffer, offset } => unsafe {
                match buffer {
                    ReadBuffer::Array(buf) => {
                        prep_read(sqe, *fd, buf.as_mut_ptr(), buf.len(), *offset)
                    }
                    ReadBuffer::Slice(buf) => {
                        prep_read(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), *offset)
                    }
                }
            },

            Operation::Recv { fd, buffer } => unsafe {
                match buffer {
                    ReadBuffer::Array(buf) => prep_recv(sqe, *fd, buf.as_mut_ptr(), buf.len(), 0),
                    ReadBuffer::Slice(buf) => {
                        prep_recv(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), 0)
                    }
                }
            },

            Operation::Send { fd, buffer } => unsafe {
                match buffer {
                    WriteBuffer::Array { array, len } => {
                        prep_send(sqe, *fd, array.as_ptr(), *len, 0)
                    }
                    WriteBuffer::Slice(buf) => {
                        prep_send(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), 0)
                    }
                }
            },

            Operation::SendMsg {
                fd, msghdr, buffer, ..
            } => unsafe {
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

                prep_sendmsg(sqe, *fd, *msghdr, 0)
            },

            Operation::RecvMsg { fd, msghdr } => unsafe { prep_recvmsg(sqe, *fd, *msghdr, 0) },

            Operation::Shutdown { socket, how } => unsafe {
                prep_shutdown(
                    sqe,
                    *socket,
                    match how {
                        ShutdownHow::Both => libc::SHUT_RDWR as u32,
                        ShutdownHow::Send => libc::SHUT_WR as u32,
                        ShutdownHow::Recv => libc::SHUT_RD as u32,
                    },
                )
            },

            Operation::Timer(v) => unsafe {
                // PORT NOTE: `&v.next` — the timespec pointer handed to the kernel
                // points INTO the completion, which the caller guarantees is stable
                // until the callback fires. That is precisely why the SQE may
                // reference it asynchronously.
                prep_timeout(sqe, ptr::addr_of!(v.next), 0, IORING_TIMEOUT_ABS)
            },

            Operation::TimerRemove { timer } => unsafe {
                prep_timeout_remove(sqe, *timer as u64, 0)
            },

            Operation::Write { fd, buffer } => unsafe {
                match buffer {
                    // offset is a u64 but if the value is -1 then it uses
                    // the offset in the fd.
                    WriteBuffer::Array { array, len } => {
                        prep_write(sqe, *fd, array.as_ptr(), *len, -1i64 as u64)
                    }
                    WriteBuffer::Slice(buf) => {
                        prep_write(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), -1i64 as u64)
                    }
                }
            },

            // PORT NOTE: `.pwrite => |*v| switch (v.buffer) { .array => sqe.prep_write(
            // v.fd, buf.array[0..buf.len], v.offset), .slice => sqe.prep_write(v.fd,
            // buf, v.offset) }` (io_uring.zig:539) — the `.write` arm with the op's
            // actual `offset`.
            Operation::PWrite { fd, buffer, offset } => unsafe {
                match buffer {
                    WriteBuffer::Array { array, len } => {
                        prep_write(sqe, *fd, array.as_ptr(), *len, *offset)
                    }
                    WriteBuffer::Slice(buf) => {
                        prep_write(sqe, *fd, (*buf).cast::<u8>(), (*buf).len(), *offset)
                    }
                }
            },

            Operation::Cancel { c } => unsafe { prep_cancel(sqe, *c as u64, 0) },
        }

        // Our sqe user data always points back to the completion.
        // The prep functions above reset the user data so we have to do this
        // here.
        // SAFETY: `sqe` is the live slot we just prepped.
        unsafe { (*sqe).user_data = completion as u64 };
    }

    /// PORT NOTE: the `catch |err| switch (err) { error.SubmissionQueueFull =>
    /// retry: {...} }` block of `add_`, lifted to a helper so the (recursive)
    /// `self.submit()` retry doesn't nest inside the `match` on `self.ring`.
    /// Returns `None` when the completion was pushed to the overflow queue.
    ///
    /// # Safety
    /// `completion` must be a valid, stable `Completion` pointer.
    unsafe fn get_sqe_or_queue(
        &mut self,
        completion: *mut Completion,
        try_submit: bool,
    ) -> Option<*mut io_uring_sqe> {
        match self.ring.get_sqe() {
            Ok(sqe) => Some(sqe),
            Err(SubmissionQueueFull) => {
                // If the queue is full and we're in try_submit mode then we
                // attempt to submit. This is used during submission flushing.
                if try_submit && self.submit().is_ok() {
                    // Submission succeeded but we may still fail (unlikely)
                    // to get an SQE...
                    if let Ok(sqe) = self.ring.get_sqe() {
                        return Some(sqe);
                    }
                }

                // Add the completion to our submissions to try to flush later.
                unsafe { self.submissions.push(completion) };
                None
            }
        }
    }

    /// Submits a completion cancelation request.
    /// Results in error.NotFound if the completion couldn't be located because
    /// it was already completed or an invalid Completion was provided.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata` + `comptime cb`
    // monomorphize to `cancel::<UD, CB>` where `CB` is the callback's ZST
    // fn-item/non-capturing-closure TYPE (Zig comptime `cb` ⇒ a Rust zero-sized
    // type). One untyped loop trampoline per call-site; `loop`/`c` stay RAW. Same
    // shape as epoll.rs's `Process::wait` / `Stream::*`.
    //
    // PORT NOTE: `Loop.cancel` (io_uring.zig:565) is io_uring-ONLY — `epoll.zig`
    // has no such method (its cancellation test builds the `.cancel` op by hand,
    // which is why epoll.rs has none either). It is ported because the reference's
    // "socket read cancellation" test (io_uring.zig:1725) calls it. NOTE it ADDS
    // `c_cancel` itself (`self.add(c_cancel)`) — see that test's PORT NOTE about
    // the reference then adding it a second time.
    ///
    /// # Safety
    /// `c`/`c_cancel` must be valid, stable `Completion` pointers held raw by the
    /// loop until their callbacks fire.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn cancel<UD, CB>(
        &mut self,
        c: *mut Completion,
        c_cancel: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), CancelError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Loop::cancel cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        unsafe {
            *c_cancel = Completion::new_op(
                Operation::Cancel { c },
                userdata.cast::<c_void>(),
                Self::cancel_trampoline::<UD, CB>,
            );
            self.add(c_cancel);
        }
    }

    /// The monomorphized untyped loop trampoline for `cancel`.
    fn cancel_trampoline<UD, CB>(
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
                core::result::Result<(), CancelError>,
            ) -> CallbackAction
            + 'static,
    {
        // if (r.cancel) |_| {} else |err| err
        let mapped: core::result::Result<(), CancelError> = match r {
            Result::Cancel(res) => res,
            _ => unreachable!(),
        };
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        cb(userdata_value::<UD>(ud), l_inner, c_inner, mapped)
    }
}

// ---------------------------------------------------------------------------
// Completion (io_uring.zig:613).
// ---------------------------------------------------------------------------

/// A completion represents a single queued request in the ring.
/// Completions must have stable pointers.
///
/// For the lowest overhead, these can be created manually and queued
/// directly. The API over the individual fields isn't the most user-friendly
/// since it is tune for performance. For user-friendly operations,
/// use the higher-level functions on this structure or the even
/// higher-level abstractions like the Timer struct.
#[repr(C)]
pub struct Completion {
    /// Operation to execute. This is only safe to read BEFORE the completion
    /// is queued. After being queued (with "add"), the operation may change.
    pub op: Operation,

    /// Userdata and callback for when the completion is finished.
    pub userdata: *mut core::ffi::c_void,
    pub callback: Callback,

    /// Internally set
    next: *mut Completion,

    flags: CompletionFlags,
}

/// PORT NOTE: the `flags: packed struct { state: State = .dead }` sub-struct.
#[derive(Clone, Copy)]
struct CompletionFlags {
    state: State,
}

/// PORT NOTE: `const State = enum(u1) { dead, active }`. NOTE this is a TWO-state
/// enum, unlike epoll's four-state one: io_uring has no "adding"/"deleting"
/// intermediate (a completion is either in the ring or it isn't).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// completion is not part of any loop
    Dead = 0,
    /// completion is registered with the ring
    Active = 1,
}

// PORT NOTE: hand-written `Default` (NOT `#[derive]`) — `Completion` has an
// fn-ptr `callback` + raw-ptr fields with no `Default`. Mirrors the Zig field
// defaults `op = .noop`, `userdata = null`, `callback = noopCallback`,
// `next = null`, `flags = { state: dead }`.
impl Default for Completion {
    fn default() -> Self {
        Completion {
            op: Operation::default(),
            userdata: ptr::null_mut(),
            callback: noop_callback,
            next: ptr::null_mut(),
            flags: CompletionFlags { state: State::Dead },
        }
    }
}

// PORT NOTE: `Completion` holds a raw-ptr `next` link + an fn-ptr → auto-`!Send`;
// it rides the `Loop` moved onto the termio thread. SAFETY / single-thread
// affinity: after the move a completion is touched by exactly the owning
// event-loop thread, so transferring ownership is sound.
unsafe impl Send for Completion {}

// PORT NOTE: intrusive `next` link seam for `queue::Intrusive<Completion>`
// (the overflow `submissions` queue).
impl queue::Node for Completion {
    unsafe fn next_field(this: *mut Self) -> *mut *mut Self {
        unsafe { ptr::addr_of_mut!((*this).next) }
    }
}

impl Completion {
    /// Returns the state of this completion. There are some things to
    /// be caution about when calling this function.
    ///
    /// First, this is only safe to call from the main thread. This cannot
    /// be called from any other thread.
    ///
    /// Second, if you are using default "undefined" completions, this will
    /// NOT return a valid value if you access it. You must zero your
    /// completion using ".{}". You only need to zero the completion once.
    /// Once the completion is in use, it will always be valid.
    ///
    /// Third, if you stop the loop (loop.stop()), the completions registered
    /// with the loop will NOT be reset to a dead state.
    //
    // PORT NOTE: reference `state(self: Completion)` (by value); ported as an
    // associated fn taking `*mut Completion` (raw self) to stay on the raw-ptr
    // discipline. Test call sites (`c1.state()`) route through `state_of`.
    //
    // # Safety
    // `c` must be a valid, stable `Completion` pointer.
    unsafe fn state(c: *mut Completion) -> CompletionState {
        match unsafe { (*c).flags.state } {
            State::Dead => CompletionState::Dead,
            State::Active => CompletionState::Active,
        }
    }

    /// Convenience wrapper for `state` callable on a `&Completion` (tests do
    /// `c1.state_of()`).
    pub fn state_of(&self) -> CompletionState {
        // SAFETY: `self` is a live `Completion`.
        unsafe { Completion::state(self as *const Completion as *mut Completion) }
    }

    /// PORT NOTE: the `c.* = .{ .op, .userdata, .callback }` initialization the
    /// watcher `wait`/`read`/`write`/`close`/`poll` methods perform — a FULL reset
    /// with the internal fields (`next`/`flags`) taking their `Default` values.
    /// (Mirrors epoll.rs's `Completion::new_op`.)
    //
    // PORT NOTE (`pub(crate)`): `watcher/timer.zig`'s `TimerLoop.cancel`
    // (timer.zig:137-210) builds its `c_cancel` completion as `.{ .op = .{
    // .timer_remove = .{ .timer = c_timer } }, ... }` from OUTSIDE this backend
    // module (the io_uring arm of that `switch (xev.backend)`), so the watcher
    // needs this full-reset seam — exactly the visibility `kqueue.rs`'s twin
    // (kqueue.rs:1589) already carries for the macOS `timer.rs` watcher.
    pub(crate) fn new_op(op: Operation, userdata: *mut c_void, callback: Callback) -> Completion {
        Completion {
            op,
            userdata,
            callback,
            ..Completion::default()
        }
    }

    /// Invokes the callback for this completion after properly constructing
    /// the Result based on the res code.
    ///
    /// # Safety
    /// `self_` must be a valid, stable `Completion` pointer and `loop_` the owning
    /// loop; `res` is the raw `cqe.res` for this completion.
    unsafe fn invoke(self_: *mut Completion, loop_: *mut Loop, res: i32) -> CallbackAction {
        let result: Result = match unsafe { &mut (*self_).op } {
            Operation::Noop => unreachable!(),

            Operation::Accept { .. } => Result::Accept(if res >= 0 {
                Ok(res as socket_t)
            } else {
                Err(match -res {
                    libc::ECANCELED => AcceptError::Canceled,
                    libc::EAGAIN => AcceptError::Again,
                    _ => AcceptError::Unexpected,
                })
            }),

            Operation::Close { .. } => Result::Close(if res >= 0 {
                Ok(())
            } else {
                Err(CloseError::Unexpected)
            }),

            Operation::Connect { .. } => Result::Connect(if res >= 0 {
                Ok(())
            } else {
                Err(match -res {
                    libc::ECANCELED => ConnectError::Canceled,
                    libc::ECONNREFUSED => ConnectError::ConnectionRefused,
                    libc::ETIMEDOUT => ConnectError::TimedOut,
                    libc::EHOSTUNREACH => ConnectError::HostUnreachable,
                    _ => ConnectError::Unexpected,
                })
            }),

            Operation::Poll { .. } => Result::Poll(if res >= 0 {
                Ok(())
            } else {
                Err(PollError::Unexpected)
            }),

            Operation::Read { buffer, .. } => Result::Read(read_result(Some(buffer), res)),

            // PORT NOTE: `.pread => .{ .pread = self.readResult(.pread, res) }`
            // (io_uring.zig:698) — the same `readResult` decoder as `.read`.
            Operation::PRead { buffer, .. } => Result::PRead(read_result(Some(buffer), res)),

            Operation::Recv { buffer, .. } => Result::Recv(read_result(Some(buffer), res)),

            Operation::Send { .. } => Result::Send(if res >= 0 {
                Ok(res as usize)
            } else {
                Err(match -res {
                    libc::ECANCELED => WriteError::Canceled,
                    libc::EPIPE => WriteError::BrokenPipe,
                    libc::ECONNRESET => WriteError::ConnectionResetByPeer,
                    _ => WriteError::Unexpected,
                })
            }),

            Operation::SendMsg { .. } => Result::SendMsg(if res >= 0 {
                Ok(res as usize)
            } else {
                Err(match -res {
                    libc::ECANCELED => WriteError::Canceled,
                    libc::EPIPE => WriteError::BrokenPipe,
                    libc::ECONNRESET => WriteError::ConnectionResetByPeer,
                    _ => WriteError::Unexpected,
                })
            }),

            // PORT NOTE: `self.readResult(.recvmsg, res)` — the `recvmsg` op has NO
            // `buffer` field, so the Zig `@hasField(@TypeOf(active), "buffer")`
            // reflection is FALSE and a zero-byte result is an unconditional
            // `error.EOF`. That is exactly the `None` arm of `read_result`.
            Operation::RecvMsg { .. } => Result::RecvMsg(read_result(None, res)),

            Operation::Shutdown { .. } => Result::Shutdown(if res >= 0 {
                Ok(())
            } else {
                Err(match -res {
                    libc::ECANCELED => ShutdownError::Canceled,
                    _ => ShutdownError::Unexpected,
                })
            }),

            Operation::Timer(op) => {
                // If we have reset set, that means that we were canceled so
                // that we can update our expiration time.
                //
                // PORT NOTE: this arm returns EARLY — the callback is deliberately
                // NOT called on the cancellation that a `timer_reset` induced.
                if let Some(r) = op.reset {
                    op.next = r;
                    op.reset = None;
                    return CallbackAction::Rearm;
                }

                Result::Timer(if res >= 0 {
                    Ok(TimerTrigger::Request)
                } else {
                    match -res {
                        libc::ETIME => Ok(TimerTrigger::Expiration),
                        libc::ECANCELED => Ok(TimerTrigger::Cancel),
                        _ => Err(TimerError::Unexpected),
                    }
                })
            }

            Operation::TimerRemove { .. } => Result::TimerRemove(if res >= 0 {
                Ok(())
            } else {
                match -res {
                    libc::ENOENT => Err(TimerRemoveError::NotFound),
                    libc::EBUSY => Err(TimerRemoveError::ExpirationInProgress),

                    // PORT NOTE (verbatim): Okay, I don't know why this might
                    // happen. It appears to happen when the timer you're attempting
                    // to cancel has already expired, but I can't reproduce this in a
                    // unit test. If this isn't safe to ignore or someone can find
                    // the meaning of this, I'd be curious.
                    libc::ETIME => Ok(()),

                    _ => Err(TimerRemoveError::Unexpected),
                }
            }),

            Operation::Write { .. } => {
                if res >= 0 {
                    Result::Write(Ok(res as usize))
                } else {
                    match -res {
                        libc::ECANCELED => Result::Write(Err(WriteError::Canceled)),
                        // If a write is interrupted, we retry it automatically.
                        libc::EINTR => return CallbackAction::Rearm,
                        _ => Result::Write(Err(WriteError::Unexpected)),
                    }
                }
            }

            // PORT NOTE: `.pwrite => .{ .pwrite = if (res >= 0) @intCast(res) else
            // switch { .CANCELED => error.Canceled, else => unexpectedErrno } }`
            // (io_uring.zig:786). NOTE: unlike `.write`, the reference's `.pwrite`
            // decode has NO `EINTR => .rearm` auto-retry — reproduced faithfully.
            Operation::PWrite { .. } => Result::PWrite(if res >= 0 {
                Ok(res as usize)
            } else {
                Err(match -res {
                    libc::ECANCELED => WriteError::Canceled,
                    _ => WriteError::Unexpected,
                })
            }),

            Operation::Cancel { .. } => Result::Cancel(if res >= 0 {
                Ok(())
            } else {
                Err(match -res {
                    libc::ENOENT => CancelError::NotFound,
                    libc::EALREADY => CancelError::ExpirationInProgress,
                    _ => CancelError::Unexpected,
                })
            }),
        };

        // return self.callback(self.userdata, loop, self, result);
        let cb = unsafe { (*self_).callback };
        let ud = unsafe { (*self_).userdata };
        cb(ud, loop_, self_, result)
    }
}

/// PORT NOTE: `Completion.readResult(op, res)` — the shared read/pread/recv/recvmsg
/// result decoder. The Zig comptime `@field(self.op, @tagName(op))` +
/// `@hasField(..., "buffer")` reflection collapses to an `Option<&ReadBuffer>`
/// parameter (`None` = the buffer-less ops, i.e. the deferred `recvmsg`).
fn read_result(buffer: Option<&ReadBuffer>, res: i32) -> core::result::Result<usize, ReadError> {
    if res > 0 {
        return Ok(res as usize);
    }

    if res == 0 {
        // if (!@hasField(@TypeOf(active), "buffer")) return ReadError.EOF;
        let Some(buffer) = buffer else {
            return Err(ReadError::EOF);
        };

        // If we receieve a zero byte read, it is an EOF _unless_
        // the requestesd buffer size was zero (weird).
        return match buffer {
            ReadBuffer::Slice(b) => {
                if (*b).len() == 0 {
                    Ok(0)
                } else {
                    Err(ReadError::EOF)
                }
            }
            ReadBuffer::Array(_) => Err(ReadError::EOF),
        };
    }

    Err(match -res {
        libc::ECANCELED => ReadError::Canceled,
        libc::ECONNRESET => ReadError::ConnectionResetByPeer,
        _ => ReadError::Unexpected,
    })
}

// ---------------------------------------------------------------------------
// Operation / Result unions (io_uring.zig:890/914) + errors + buffers.
// ---------------------------------------------------------------------------

/// All the supported operations of this event loop. These are always
/// backend-specific and therefore the structure and types change depending
/// on the underlying system in use. The high level operations are
/// done by initializing the request handles.
//
// PORT NOTE: `op: Operation = .{ .noop = {} }` — the `#[default]` `Noop` variant.
// The ported tag set mirrors epoll.rs's PLUS io_uring's own `timer_remove` (which
// `timer_reset` needs — epoll cancels via `Cancel`) and `sendmsg`/`recvmsg` (which
// io_uring's own reference tests exercise, unlike epoll's; `libc::msghdr`/`iovec`
// supply the surface epoll.rs lacked a reason to reach for).
//
// PORT NOTE: Task 5 adds `pread`/`pwrite` (the File watcher's offset-carrying
// read/write) — pure `offset`-carrying variants of `Read`/`Write`; on io_uring
// they are native SQEs (`prep_read`/`prep_write` with the op's `offset` instead of
// `-1`), NOT thread-pool ops (that split is epoll-only).
#[derive(Default)]
pub enum Operation {
    /// Do nothing. This operation will not be queued and will never
    /// have its callback fired. This is NOT equivalent to the io_uring
    /// "nop" operation.
    #[default]
    Noop,

    /// Accept a connection on a socket.
    Accept {
        socket: socket_t,
        addr: net::sockaddr::Any,
        addr_size: net::socklen_t,
        flags: u32,
    },

    /// Close a file descriptor.
    Close { fd: fd_t },

    /// Initiate a connection on a socket.
    Connect {
        socket: socket_t,
        addr: net::Address,
    },

    /// Poll a fd. Only oneshot mode is supported today.
    Poll { fd: fd_t, events: u32 },

    /// Read
    Read { fd: fd_t, buffer: ReadBuffer },

    /// Read at an offset (the File watcher's offset-carrying read).
    PRead {
        fd: fd_t,
        buffer: ReadBuffer,
        offset: u64,
    },

    /// Receive a message from a socket.
    Recv { fd: fd_t, buffer: ReadBuffer },

    /// Send a message on a socket.
    Send { fd: fd_t, buffer: WriteBuffer },

    /// Send a message on a socket using sendmsg (i.e. UDP).
    //
    // PORT NOTE: the reference field is `msghdr: *posix.msghdr_const`. `libc` has no
    // `msghdr_const` twin (Zig splits the type by constness; C/Rust do not), so the
    // constness rides the POINTER: `*const libc::msghdr`. `libc::msghdr` is the
    // glibc/kernel `struct user_msghdr` layout — the exact ABI the kernel's
    // `io_sendmsg_prep` reads through the SQE's `addr`.
    SendMsg {
        fd: fd_t,
        msghdr: *const libc::msghdr,

        /// Optionally, a write buffer can be specified and the given
        /// msghdr will be populated with information about this buffer.
        //
        // PORT NOTE (a REFERENCE QUIRK, preserved deliberately): this field is
        // DEAD in the reference — `add_`'s `sendmsg` arm does
        // `if (v.buffer) |_| @panic("TODO: sendmsg with buffer");`, so the only
        // reachable value is `null`. The field (and `iov` below) are kept so the
        // op's shape matches the reference and so the panic arm is faithful; see
        // `add_`'s `SendMsg` arm.
        buffer: Option<WriteBuffer>,

        /// Do not use this, it is only used internally.
        //
        // PORT NOTE: `iov: [1]posix.iovec_const = undefined` — never written nor
        // read by the reference (it is the scratch the unimplemented `buffer` path
        // would populate). Kept for shape fidelity; `libc::iovec` is the same
        // layout as Zig's `iovec_const` (constness rides the pointer in C/Rust).
        iov: [libc::iovec; 1],
    },

    /// Recieve a message on a socket using recvmsg (i.e. UDP).
    RecvMsg { fd: fd_t, msghdr: *mut libc::msghdr },

    /// Shutdown all or part of a full-duplex connection.
    Shutdown { socket: socket_t, how: ShutdownHow },

    /// Write
    Write { fd: fd_t, buffer: WriteBuffer },

    /// Write at an offset (the File watcher's offset-carrying write).
    PWrite {
        fd: fd_t,
        buffer: WriteBuffer,
        offset: u64,
    },

    /// A oneshot or repeating timer. For io_uring, this is implemented
    /// using the timeout mechanism.
    Timer(Timer),

    /// Cancel an existing timer.
    TimerRemove { timer: *mut Completion },

    /// Cancel an existing operation.
    Cancel { c: *mut Completion },
}

/// PORT NOTE: the `timer: struct { next, reset }` payload of `Operation`.
//
// PORT NOTE (vs. epoll's `Timer`): NO `heap`/`c` fields — io_uring's timer lives in
// the KERNEL (an `IORING_OP_TIMEOUT` SQE), not in a userspace pairing heap, so it
// needs neither a heap link nor a back-pointer to its completion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timer {
    pub next: KernelTimespec,

    /// Only used internally. If this is non-null and timer is
    /// CANCELLED, then the timer is rearmed automatically with this
    /// as the next time. The callback will not be called on the
    /// cancellation.
    pub reset: Option<KernelTimespec>,
}

/// PORT NOTE: `posix.ShutdownHow` — the `how` selector of the `shutdown` op
/// (`.both` default).
#[derive(Clone, Copy)]
pub enum ShutdownHow {
    Recv,
    Send,
    Both,
}

/// The result type based on the operation type. For a callback, the
/// result tag will ALWAYS match the operation tag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Result {
    Noop,
    Accept(core::result::Result<socket_t, AcceptError>),
    Close(core::result::Result<(), CloseError>),
    Connect(core::result::Result<(), ConnectError>),
    Poll(core::result::Result<(), PollError>),
    Read(core::result::Result<usize, ReadError>),
    PRead(core::result::Result<usize, ReadError>),
    Recv(core::result::Result<usize, ReadError>),
    Send(core::result::Result<usize, WriteError>),
    SendMsg(core::result::Result<usize, WriteError>),
    RecvMsg(core::result::Result<usize, ReadError>),
    Shutdown(core::result::Result<(), ShutdownError>),
    Write(core::result::Result<usize, WriteError>),
    PWrite(core::result::Result<usize, WriteError>),
    Timer(core::result::Result<TimerTrigger, TimerError>),
    TimerRemove(core::result::Result<(), TimerRemoveError>),
    Cancel(core::result::Result<(), CancelError>),
}

/// PORT NOTE: `pub const AcceptError = error{Canceled, Again, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptError {
    Canceled,
    Again,
    Unexpected,
}

/// PORT NOTE: `pub const CancelError = error{NotFound, ExpirationInProgress, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelError {
    NotFound,
    ExpirationInProgress,
    Unexpected,
}

/// PORT NOTE: `pub const CloseError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const ConnectError = error{Canceled, Unexpected,
/// ConnectionRefused, HostUnreachable, TimedOut}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    Canceled,
    Unexpected,
    ConnectionRefused,
    HostUnreachable,
    TimedOut,
}

/// PORT NOTE: `pub const PollError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const ReadError = error{EOF, Canceled, Unexpected,
/// ConnectionResetByPeer}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    EOF,
    Canceled,
    Unexpected,
    ConnectionResetByPeer,
}

/// PORT NOTE: `pub const ShutdownError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownError {
    Canceled,
    Unexpected,
}

/// PORT NOTE: `pub const WriteError = error{Canceled, BrokenPipe,
/// ConnectionResetByPeer, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteError {
    Canceled,
    BrokenPipe,
    ConnectionResetByPeer,
    Unexpected,
}

/// PORT NOTE: `pub const TimerError = error{Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    Unexpected,
}

/// PORT NOTE: `pub const TimerRemoveError = error{NotFound, ExpirationInProgress,
/// Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerRemoveError {
    NotFound,
    ExpirationInProgress,
    Unexpected,
}

/// PORT NOTE: `pub const TimerTrigger = enum { request, expiration, cancel }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerTrigger {
    /// Timer completed due to linked request completing in time.
    Request,
    /// Timer expired.
    Expiration,
    /// Timer was canceled.
    Cancel,
}

/// ReadBuffer are the various options for reading.
//
// PORT NOTE (R3): the Zig `slice: []u8` ports as a raw fat pointer `*mut [u8]`
// (never a lifetime — the buffer lives in caller memory and the KERNEL writes it
// asynchronously), NEVER a `String`.
#[derive(Clone, Copy)]
pub enum ReadBuffer {
    /// Read into this slice.
    Slice(*mut [u8]),

    /// Read into this array, just set this to undefined and it will
    /// be populated up to the size of the array. This is an option because
    /// the other union members force a specific size anyways so this lets us
    /// use the other size in the union to support small reads without worrying
    /// about buffer allocation.
    ///
    /// To know the size read you have to use the return value of the
    /// read operations (i.e. recv).
    ///
    /// Note that the union at the time of this writing could accomodate a
    /// much larger fixed size array here but we want to retain flexiblity
    /// for future fields.
    Array([u8; 32]),
    // TODO: future will have vectors
}

/// WriteBuffer are the various options for writing.
//
// PORT NOTE (R3): the Zig `slice: []const u8` ports as a raw fat pointer
// `*const [u8]` (never a lifetime — the buffer lives in caller memory and the
// KERNEL reads it asynchronously).
#[derive(Clone, Copy)]
pub enum WriteBuffer {
    /// Write from this buffer.
    Slice(*const [u8]),

    /// Write from this array. See ReadBuffer.array for why we support this.
    Array { array: [u8; 32], len: usize },
    // TODO: future will have vectors
}

mod async_process;
pub use self::async_process::*;

mod stream;
pub use self::stream::*;

mod tcp;
pub use self::tcp::*;

mod udp;
pub use self::udp::*;

mod file;
pub use self::file::*;

// PORT NOTE: `test "Completion size"` (io_uring.zig:1117) — the layout tripwire.
//
// PORT NOTE: the reference asserts the Zig `@sizeOf(Completion) == 152`. Rust's
// tagged-enum + struct layout differs FUNDAMENTALLY from Zig's tagged-union
// layout, so the concrete byte size is not 152. The tripwire's PURPOSE — trip on
// any accidental `Completion` bloat — is preserved by pinning the MEASURED Rust
// size; drift in the op union / buffers / error sets will fail this `const`
// assert exactly as the Zig `@compileLog` guard does. (Same divergence as
// epoll.rs's / the kqueue port's `COMPLETION_SIZE`.)
//
// Measured Rust size on aarch64-unknown-linux-gnu. As in epoll.rs, the
// `Operation` union is dominated by `Connect`, which embeds `net::Address` (a
// `sockaddr` union whose largest member is `sockaddr_un`, ~110 bytes).
//
// RE-MEASURED (Task 6, after adding the `SendMsg`/`RecvMsg` ops): UNCHANGED at
// 152. `Connect` still dominates the `Operation` union and nothing was added to
// `Completion` itself. Measured on aarch64-unknown-linux-gnu:
//   Completion = 152, Operation = 120, net::Address = 112,
//   Option<WriteBuffer> = 48, libc::iovec = 16
// so `Connect` (4 + 112, +tag → 120) still outweighs the widest new arm,
// `SendMsg` (msghdr 8 + buffer 48 + iov 16 + fd 4, +tag/padding → 88).
//
// UPSTREAM DRIFT (unabsorbed): the libxev bundled by the ghostty pin 6ad1fe7d
// (libxev 9ce8e8e6) asserts 128 (io_uring.zig:1130), down from the 152 this
// port tracks (libxev 34fa5087) — against its own pin the Rust size is
// byte-identical. The bump is a documented non-port (`.port/phase-3/NON_PORTS.md`
// G3; rustty-xev is a D6 Zig-port-defer and does not track libxev revisions) —
// a future bump absorption re-measures against 128, not 152.
const COMPLETION_SIZE: usize = 152;
const _: () = assert!(size_of::<Completion>() == COMPLETION_SIZE);

// ---------------------------------------------------------------------------
// Tests (io_uring.zig `test{}` blocks). The FULL reference set: available /
// overflow entries count / Completion size / default completion / loop time /
// stop (L1 LT5) + timerfd / timer / timer reset / timer remove / socket
// accept-connect-send-recv-close / sendmsg-recvmsg / socket read cancellation
// (L1 LT6), plus the two Linux live KATs (process spawn+exit, pty write
// round-trip) that mirror epoll.rs's. Callbacks are plain fn items matching the
// loop `Callback` ABI; completions/userdata are stack locals reached through raw
// pointers (the intrusive-ownership contract).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests;
