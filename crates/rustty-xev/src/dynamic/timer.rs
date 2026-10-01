//! The Dynamic `Timer` watcher.
//!
//! PORTS: libxev src/watcher/timer.zig `TimerDynamic` — the dynamic-dispatch arm
//! over the io_uring and epoll backends.

use super::*;

// ---------------------------------------------------------------------------
// Timer (watcher/timer.zig `TimerDynamic`, timer.zig:229-430).
// ---------------------------------------------------------------------------

/// Error that could happen while running a timer.
//
// PORT NOTE: `pub const RunError = xev.ErrorSet(&.{"Timer", "RunError"})`
// (timer.zig:236). Each backend's `Timer.RunError` is the SAME literal set
// (timer.zig:213-219 — the watcher declares it, not the backend), so the merge is
// that set. Named `TimerRunError` here because everything shares one module (the
// macOS surface exports the same type as `xev::TimerRunError`, lib.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerRunError {
    /// The timer was canceled before it could expire
    Canceled,
    /// Some unexpected error.
    Unexpected,
}

/// PORT NOTE: `pub const CancelError = xev.ErrorSet(&.{"Timer", "CancelError"})`
/// (timer.zig:237); each backend's `Timer.CancelError` is `xev.CancelError`
/// (timer.zig:221), so the merge is this module's `CancelError`.
pub type TimerCancelError = CancelError;

/// A timer fires a callback after a specified amount of time. A timer can
/// repeat by returning "rearm" in the callback or by rescheduling the
/// start within the callback.
//
// PORT NOTE: `TimerDynamic` is `struct { backend: xev.Union(&.{"Timer"}) }`
// (timer.zig:233-235). Each candidate's `api.Timer` is `TimerLoop(api)`, an EMPTY
// struct (timer.zig:15-17) that only forwards to `loop.timer`/`loop.timer_reset`
// — which is why the reference has to bolt a `_zig_bug_padding: u8` arm onto the
// union to dodge a Zig 0.13 zero-sized-union crash (dynamic.zig:483-491). Rust
// has no such bug, and a union of two ZSTs carries no information, so this is the
// unit struct it always was. `Copy`: the reference `run`/`reset`/`cancel` take
// `self: Self` by value.
//
// PORT NOTE: neither `epoll.rs` nor `io_uring.rs` carries a `Timer` WATCHER
// (their `Timer` type is the loop-internal heap node / kernel timespec), because
// `TimerLoop` is a pure forwarder with no backend-specific state. So this type is
// `TimerLoop(Dynamic)`-shaped — it reaches the backend loop's `timer`/
// `timer_reset` directly, which is precisely what `TimerDynamic` → `TimerLoop` →
// `loop.timer` collapses to.
#[derive(Clone, Copy)]
pub struct Timer;

impl Timer {
    /// Create a new timer.
    //
    // PORT NOTE: `pub fn init() !Self` (timer.zig:239 / timer.zig:20).
    pub fn init() -> core::result::Result<Timer, Error> {
        Ok(Timer)
    }

    /// PORT NOTE: `pub fn deinit(self: *Self) void` (timer.zig:252 / timer.zig:24)
    /// — "Nothing for now."
    pub fn deinit(&self) {
        let _ = self;
    }

    /// Start the timer. The timer will execute in next_ms milliseconds from
    /// now.
    ///
    /// This will use the monotonic clock on your system if available so
    /// this is immune to system clock changes or drift. The callback is
    /// guaranteed to fire NO EARLIER THAN "next_ms" milliseconds. We can't
    /// make any guarantees about exactness or time bounds because its possible
    /// for your OS to just... pause.. the process for an indefinite period of
    /// time.
    ///
    /// Like everything else in libxev, if you want something to repeat, you
    /// must then requeue the completion manually. This punts off one of the
    /// "hard" aspects of timers: it is up to you to determine what the semantic
    /// meaning of intervals are.
    //
    // PORT NOTE: `pub fn run(...)` (timer.zig:261) — `c.ensureTag(tag)` then the
    // backend's `Timer.run` (= `loop.timer(c, next_ms, userdata, cb)`). Unlike
    // the other watchers, the backend entry point here takes a PLAIN fn-pointer
    // (`epoll::Callback` / `io_uring::Callback`), not a generic ZST `CB`, so the
    // per-arm bridge must be a monomorphized `fn` item (`*_run_trampoline`)
    // rather than a closure — exactly Zig's `struct { fn callback }` idiom.
    ///
    /// # Safety
    /// `c`/`loop_` must be valid, stable pointers that outlive the timer.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn run<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        next_ms: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerRunError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Timer::run cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        match backend() {
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                (*Loop::as_io_uring(loop_)).timer(
                    Completion::as_io_uring(c),
                    next_ms,
                    userdata.cast::<c_void>(),
                    Self::io_uring_run_trampoline::<UD, CB>,
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                (*Loop::as_epoll(loop_)).timer(
                    Completion::as_epoll(c),
                    next_ms,
                    userdata.cast::<c_void>(),
                    Self::epoll_run_trampoline::<UD, CB>,
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// Reset a timer to execute in next_ms milliseconds. If the timer
    /// is already started, this will stop it and restart it. If the
    /// timer has never been started, this is equivalent to running "run".
    /// In every case, the timer callback is updated to the given userdata
    /// and callback.
    ///
    /// This requires an additional completion c_cancel to represent
    /// the need to possibly cancel the previous timer. You can check
    /// if c_cancel was used by checking the state() after the call.
    ///
    /// VERY IMPORTANT: both c and c_cancel MUST NOT be undefined. They
    /// must be initialized to ".{}" if being used for the first time.
    //
    // PORT NOTE: `pub fn reset(...)` (timer.zig:306) — `c.ensureTag(tag)`,
    // `c_cancel.ensureTag(tag)`, then the backend's `Timer.reset` (=
    // `loop.timer_reset`). Reuses the SAME trampoline as `run` (both map the
    // loop's `Result::Timer` → `TimerRunError`), exactly as `TimerLoop` does.
    ///
    /// # Safety
    /// `c`/`c_cancel`/`loop_` must be valid, stable pointers that outlive the timer.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn reset<UD, CB>(
        self,
        loop_: *mut Loop,
        c: *mut Completion,
        c_cancel: *mut Completion,
        next_ms: u64,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerRunError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Timer::reset cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        match backend() {
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c, Backend::IoUring);
                Completion::ensure_tag_raw(c_cancel, Backend::IoUring);
                (*Loop::as_io_uring(loop_)).timer_reset(
                    Completion::as_io_uring(c),
                    Completion::as_io_uring(c_cancel),
                    next_ms,
                    userdata.cast::<c_void>(),
                    Self::io_uring_run_trampoline::<UD, CB>,
                );
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c, Backend::Epoll);
                Completion::ensure_tag_raw(c_cancel, Backend::Epoll);
                (*Loop::as_epoll(loop_)).timer_reset(
                    Completion::as_epoll(c),
                    Completion::as_epoll(c_cancel),
                    next_ms,
                    userdata.cast::<c_void>(),
                    Self::epoll_run_trampoline::<UD, CB>,
                );
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The monomorphized io_uring trampoline for `run`/`reset` — maps
    /// `io_uring::Result::Timer` → `TimerRunError!void` and `@fieldParentPtr`s the
    /// inner pointers back to the outer ones.
    fn io_uring_run_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut io_uring::Loop,
        c_inner: *mut io_uring::Completion,
        r: io_uring::Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerRunError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted in `run`/`reset`), so reading it through
        // an aligned dangling pointer accesses zero bytes and is sound.
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        // if (r.timer) |trigger| switch (trigger) { .request, .expiration => {},
        //     .cancel => error.Canceled } else |err| err
        let mapped: core::result::Result<(), TimerRunError> = match r {
            io_uring::Result::Timer(Ok(trigger)) => match trigger {
                io_uring::TimerTrigger::Request | io_uring::TimerTrigger::Expiration => Ok(()),
                io_uring::TimerTrigger::Cancel => Err(TimerRunError::Canceled),
            },
            io_uring::Result::Timer(Err(io_uring::TimerError::Unexpected)) => {
                Err(TimerRunError::Unexpected)
            }
            _ => unreachable!("timer completion yielded a non-timer result"),
        };
        cb(
            userdata_value::<UD>(ud),
            Loop::from_io_uring(l_inner),
            Completion::from_io_uring(c_inner),
            mapped,
        )
    }

    /// See `io_uring_run_trampoline`.
    fn epoll_run_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut epoll::Loop,
        c_inner: *mut epoll::Completion,
        r: epoll::Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerRunError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: see `io_uring_run_trampoline`.
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let mapped: core::result::Result<(), TimerRunError> = match r {
            epoll::Result::Timer(Ok(trigger)) => match trigger {
                epoll::TimerTrigger::Request | epoll::TimerTrigger::Expiration => Ok(()),
                epoll::TimerTrigger::Cancel => Err(TimerRunError::Canceled),
            },
            epoll::Result::Timer(Err(epoll::TimerError::Unexpected)) => {
                Err(TimerRunError::Unexpected)
            }
            _ => unreachable!("timer completion yielded a non-timer result"),
        };
        cb(
            userdata_value::<UD>(ud),
            Loop::from_epoll(l_inner),
            Completion::from_epoll(c_inner),
            mapped,
        )
    }

    /// Cancel a previously started timer. The timer to cancel used the completion
    /// "c_cancel". A new completion "c" must be specified which will be called
    /// with the callback once cancellation is complete.
    ///
    /// The original timer will still have its callback fired but with the
    /// error "error.Canceled".
    //
    // PORT NOTE: `TimerDynamic.cancel` (timer.zig:376-430) does
    // `c_timer.ensureTag(tag)` + `c_cancel.ensureTag(tag)` then forwards to
    // `api.Timer.cancel` = `TimerLoop.cancel` (timer.zig:137-210), whose body is a
    // `switch (xev.backend)` writing a backend op into `c_cancel` and then
    // `loop.add(c_cancel)`:
    //   * io_uring — `.op = .{ .timer_remove = .{ .timer = c_timer } }` (timer.zig:160-164)
    //   * epoll    — `.op = .{ .cancel = .{ .c = c_timer } }` (timer.zig:181-187)
    // Both arms are inlined here for the same reason `run`/`reset` are: this type
    // is `TimerLoop(Dynamic)`-shaped and reaches the backend loop directly.
    // (This un-defers the C7 gap the earlier PORT NOTE described, via exactly the
    // route it prescribed: `Completion::new_op` is now `pub(crate)` in both
    // backends, matching kqueue.rs:1589.)
    ///
    /// # Safety
    /// `c_timer`/`c_cancel`/`loop_` must be valid, stable pointers that outlive
    /// the timer.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub unsafe fn cancel<UD, CB>(
        self,
        loop_: *mut Loop,
        c_timer: *mut Completion,
        c_cancel: *mut Completion,
        userdata: *mut UD,
        cb: CB,
    ) where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerCancelError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(
                size_of::<CB>() == 0,
                "Timer::cancel cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb"
            )
        };
        let _ = cb;

        match backend() {
            Backend::IoUring => unsafe {
                Completion::ensure_tag_raw(c_timer, Backend::IoUring);
                Completion::ensure_tag_raw(c_cancel, Backend::IoUring);
                // `c_cancel.* = .{ .op = .{ .timer_remove = .{ .timer = c_timer } },
                // .userdata = userdata, .callback = ... }` — written through the
                // backend arm, leaving the Dynamic `tag` set by `ensure_tag_raw`.
                *Completion::as_io_uring(c_cancel) = io_uring::Completion::new_op(
                    io_uring::Operation::TimerRemove {
                        timer: Completion::as_io_uring(c_timer),
                    },
                    userdata.cast::<c_void>(),
                    Self::io_uring_cancel_trampoline::<UD, CB>,
                );
                (*Loop::as_io_uring(loop_)).add(Completion::as_io_uring(c_cancel));
            },
            Backend::Epoll => unsafe {
                Completion::ensure_tag_raw(c_timer, Backend::Epoll);
                Completion::ensure_tag_raw(c_cancel, Backend::Epoll);
                // `c_cancel.* = .{ .op = .{ .cancel = .{ .c = c_timer } }, ... }`.
                *Completion::as_epoll(c_cancel) = epoll::Completion::new_op(
                    epoll::Operation::Cancel {
                        c: Completion::as_epoll(c_timer),
                    },
                    userdata.cast::<c_void>(),
                    Self::epoll_cancel_trampoline::<UD, CB>,
                );
                (*Loop::as_epoll(loop_)).add(Completion::as_epoll(c_cancel));
            },
            be => unreachable!("non-candidate backend {be:?}"),
        }
    }

    /// The monomorphized io_uring trampoline for `cancel` — maps
    /// `io_uring::Result::TimerRemove` → `TimerCancelError!void`.
    //
    // PORT NOTE: `if (r.timer_remove) |_| {} else |err| err` (timer.zig:176). Zig's
    // structural error sets let `TimerRemoveError` flow straight into the
    // `CancelError!void` slot because io_uring's two sets are the SAME literal set
    // ({NotFound, ExpirationInProgress, Unexpected} — io_uring.rs's
    // `TimerRemoveError`/`CancelError`); Rust's nominal enums need the explicit
    // 1:1 widening into the merged `ErrorSet` that `TimerCancelError` is.
    fn io_uring_cancel_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut io_uring::Loop,
        c_inner: *mut io_uring::Completion,
        r: io_uring::Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerCancelError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: `CB` is a ZST (asserted in `cancel`), so reading it through an
        // aligned dangling pointer accesses zero bytes and is sound.
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let mapped: core::result::Result<(), TimerCancelError> = match r {
            io_uring::Result::TimerRemove(Ok(())) => Ok(()),
            io_uring::Result::TimerRemove(Err(e)) => Err(match e {
                io_uring::TimerRemoveError::NotFound => TimerCancelError::NotFound,
                io_uring::TimerRemoveError::ExpirationInProgress => {
                    TimerCancelError::ExpirationInProgress
                }
                io_uring::TimerRemoveError::Unexpected => TimerCancelError::Unexpected,
            }),
            _ => unreachable!("timer cancel completion yielded a non-timer_remove result"),
        };
        cb(
            userdata_value::<UD>(ud),
            Loop::from_io_uring(l_inner),
            Completion::from_io_uring(c_inner),
            mapped,
        )
    }

    /// See `io_uring_cancel_trampoline`. Maps `epoll::Result::Cancel` →
    /// `TimerCancelError!void` (`if (r.cancel) |_| {} else |err| err`,
    /// timer.zig:202).
    fn epoll_cancel_trampoline<UD, CB>(
        ud: *mut c_void,
        l_inner: *mut epoll::Loop,
        c_inner: *mut epoll::Completion,
        r: epoll::Result,
    ) -> CallbackAction
    where
        UD: 'static,
        CB: Fn(
                *mut UD,
                *mut Loop,
                *mut Completion,
                core::result::Result<(), TimerCancelError>,
            ) -> CallbackAction
            + 'static,
    {
        // SAFETY: see `io_uring_cancel_trampoline`.
        let cb: CB = unsafe { NonNull::<CB>::dangling().as_ptr().read() };
        let mapped: core::result::Result<(), TimerCancelError> = match r {
            epoll::Result::Cancel(Ok(())) => Ok(()),
            // epoll's `CancelError` is `ThreadPoolError || error{NotFound}`; only
            // `NotFound` is ported (epoll.rs's `CancelError`).
            epoll::Result::Cancel(Err(epoll::CancelError::NotFound)) => {
                Err(TimerCancelError::NotFound)
            }
            _ => unreachable!("timer cancel completion yielded a non-cancel result"),
        };
        cb(
            userdata_value::<UD>(ud),
            Loop::from_epoll(l_inner),
            Completion::from_epoll(c_inner),
            mapped,
        )
    }
}
