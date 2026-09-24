//! timer.rs — the full port of libxev `watcher/timer.zig` (D6, XEV_BLUEPRINT
//! §4/§7).
//!
//! `Timer` (the `xev.Timer` high-level watcher) wraps the loop's low-level
//! `timer`/`timer_reset` methods. termio names `xev::Timer` (scroll/coalesce/
//! sync_reset/termios timers) and constructs it via `Timer::init()`, and KAT 2
//! (Timer fires) drives `Timer::run`. `init`/`deinit`/`run` landed in C3; C7 (the
//! finale) un-defers `reset`/`cancel` + `watcher/timer.zig`'s 5 `test{}`s
//! (`timer`, `timer reset`, `timer reset before tick`, `timer reset after
//! trigger`, `timer cancel`) — the loop-level `timer`/`timer_reset`/`add`/`.cancel`
//! machinery they ride was already ported + exercised by the kqueue timer tests in
//! C3, so these watcher-level methods are thin faithful wrappers over it.
//
// PORT NOTE: the reference is `fn Timer(comptime xev: type) type` → `if
// (xev.dynamic) TimerDynamic else TimerLoop`. On macOS `xev.dynamic == false`
// (§0), so this is the `TimerLoop` instantiation (`xev = kqueue`).
// (module is `#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos", target_os = "freebsd"))]`-gated in lib.rs)

use crate::kqueue::{CancelError, Completion, Loop, Operation, Result, TimerTrigger};
use crate::looppkg::CallbackAction;

/// A timer fires a callback after a specified amount of time. A timer can
/// repeat by returning "rearm" in the callback or by rescheduling the
/// start within the callback.
//
// PORT NOTE: this is the `TimerLoop` instantiation (on macOS `xev.dynamic == false`,
// §0, so `Timer(xev)` is `TimerLoop(xev)`, never `TimerDynamic`).
// An implementation that uses the loop timer methods.
//
// PORT NOTE: the `TimerLoop` struct is empty (only methods); a zero-sized handle.
// `#[derive(Clone, Copy)]` — the reference `run`/`reset` take `self: Self` by
// value (trivially copyable), and termio holds `xev::Timer` fields by value.
#[derive(Clone, Copy)]
pub struct Timer;

impl Timer {
    /// Create a new timer.
    pub fn init() -> core::result::Result<Timer, crate::Error> {
        Ok(Timer)
    }

    pub fn deinit(&self) {
        // Nothing for now.
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
    /// meaning of intervals are. For example, if you want a timer to repeat every
    /// 10 seconds, is it every 10th second of a wall clock? every 10th second
    /// after an invocation? every 10th second after the work time from the
    /// invocation? You have the power to answer these questions, manually.
    //
    // PORT NOTE (§2.2): the reference `comptime Userdata: type` + `userdata:
    // ?*Userdata` + `comptime cb: *const fn(...)` monomorphize to a generic
    // `run::<UD, CB>` where `CB` is the callback's ZST fn-item/non-capturing-
    // closure TYPE (Zig's comptime-known `cb` value ⇒ a Rust zero-sized type,
    // NOT a runtime fn pointer). Each call-site emits exactly one untyped loop
    // trampoline `trampoline::<UD, CB>` with zero allocation and one indirection
    // — matching Zig's comptime `struct{fn callback}` (no `dyn`, no boxing). The
    // trampoline coerces `?*anyopaque` → `*mut UD` (via `common::userdata_value`)
    // and maps `xev.Result.timer` → `RunError!void`. `loop`/`c` stay RAW pointers.
    ///
    /// # Safety
    /// `c` must be a valid, stable `Completion` pointer that outlives the timer.
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
                core::result::Result<(), RunError>,
            ) -> CallbackAction
            + 'static,
    {
        // The typed `cb` must be a zero-sized comptime-known value (a fn-item or
        // non-capturing closure) so the trampoline can reconstruct it with no
        // state — exactly Zig's comptime capture. `cb` itself is dropped here.
        const {
            assert!(core::mem::size_of::<CB>() == 0, "Timer::run cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb")
        };
        let _ = cb;

        unsafe {
            (*loop_).timer(
                c,
                next_ms,
                userdata.cast::<core::ffi::c_void>(),
                Self::trampoline::<UD, CB>,
            );
        }
    }

    /// The monomorphized untyped loop trampoline for a given (`UD`, `CB`).
    fn trampoline<UD, CB>(
        ud: *mut core::ffi::c_void,
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
                core::result::Result<(), RunError>,
            ) -> CallbackAction
            + 'static,
    {
        // Reconstruct the zero-sized `CB` value — the Rust equivalent of Zig
        // baking `comptime cb` into the monomorphized trampoline.
        //
        // SAFETY: `CB` is a ZST (asserted in `run`), so reading it through an
        // aligned dangling pointer accesses zero bytes and is sound (ZST reads
        // never dereference). Preferred over `MaybeUninit::assume_init` (which
        // clippy flags for non-ZST misuse).
        let cb: CB = unsafe { core::ptr::NonNull::<CB>::dangling().as_ptr().read() };

        let ud_typed = crate::common::userdata_value::<UD>(ud);
        let mapped: core::result::Result<(), RunError> = match r {
            // if (r.timer) |trigger| switch(trigger){ .request,.expiration => {}, .cancel => error.Canceled }
            Result::Timer(Ok(trigger)) => match trigger {
                TimerTrigger::Request | TimerTrigger::Expiration => Ok(()),
                TimerTrigger::Cancel => Err(RunError::Canceled),
            },
            // else |err| err  — the loop's TimerError → RunError.
            Result::Timer(Err(_err)) => Err(RunError::Unexpected),
            _ => unreachable!(),
        };
        cb(ud_typed, l_inner, c_inner, mapped)
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
    // PORT NOTE: delegates to `loop.timer_reset` with the SAME untyped
    // trampoline as `run` (both map `xev.Result.timer` → `RunError!void`), so it
    // reuses `Self::trampoline::<UD, CB>`. `loop`/`c`/`c_cancel` stay RAW pointers.
    ///
    /// # Safety
    /// `c`/`c_cancel` must be valid, stable `Completion` pointers that outlive
    /// the timer.
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
                core::result::Result<(), RunError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(core::mem::size_of::<CB>() == 0, "Timer::reset cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb")
        };
        let _ = cb;

        unsafe {
            (*loop_).timer_reset(
                c,
                c_cancel,
                next_ms,
                userdata.cast::<core::ffi::c_void>(),
                Self::trampoline::<UD, CB>,
            );
        }
    }

    /// Cancel a previously started timer. The timer to cancel used the completion
    /// "c_cancel". A new completion "c" must be specified which will be called
    /// with the callback once cancellation is complete.
    ///
    /// The original timer will still have its callback fired but with the
    /// error "error.Canceled".
    //
    // PORT NOTE: the reference switches on `xev.backend`; the kqueue arm builds a
    // `.cancel = .{ .c = c_timer }` completion (the io_uring `.timer_remove` arm
    // is comptime-elided on macOS, §0) and `loop.add`s it. Its trampoline maps
    // `xev.Result.cancel` → `CancelError!void`.
    ///
    /// # Safety
    /// `c_timer`/`c_cancel` must be valid, stable `Completion` pointers.
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
                core::result::Result<(), CancelError>,
            ) -> CallbackAction
            + 'static,
    {
        const {
            assert!(core::mem::size_of::<CB>() == 0, "Timer::cancel cb must be zero-sized (a fn item / non-capturing closure), matching Zig comptime cb")
        };
        let _ = cb;

        unsafe {
            // Reference: `c_cancel.* = .{ .op = .{ .cancel = .{ .c = c_timer } },
            // .userdata = userdata, .callback = ... }`. Built via the `pub(crate)`
            // `Completion::new_op` seam (the faithful full-reset helper C4/C6 use)
            // since the internal fields are private to `kqueue` (module boundary).
            *c_cancel = Completion::new_op(
                Operation::Cancel { c: c_timer },
                userdata.cast::<core::ffi::c_void>(),
                Self::cancel_trampoline::<UD, CB>,
            );

            (*loop_).add(c_cancel);
        }
    }

    /// The monomorphized untyped loop trampoline for `cancel` (maps the loop's
    /// `Result::Cancel` → `CancelError!void`, distinct from `run`/`reset`).
    fn cancel_trampoline<UD, CB>(
        ud: *mut core::ffi::c_void,
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
        // SAFETY: `CB` is a ZST (asserted in `cancel`), so reading it through an
        // aligned dangling pointer accesses zero bytes and is sound.
        let cb: CB = unsafe { core::ptr::NonNull::<CB>::dangling().as_ptr().read() };

        let ud_typed = crate::common::userdata_value::<UD>(ud);
        // if (r.cancel) |_| {} else |err| err
        let mapped: core::result::Result<(), CancelError> = match r {
            Result::Cancel(res) => res,
            _ => unreachable!(),
        };
        cb(ud_typed, l_inner, c_inner, mapped)
    }
}

/// Error that could happen while running a timer.
//
// PORT NOTE: `pub const RunError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    /// The timer was canceled before it could expire
    Canceled,
    /// Some unexpected error.
    Unexpected,
}

// PORT NOTE: `pub const CancelError = xev.CancelError;` — the timer watcher forwards
// the loop's cancel error set unchanged (`crate::kqueue::CancelError`, imported
// at the top of this module and used by `cancel`/`cancel_trampoline`).

// ---------------------------------------------------------------------------
// PORT NOTE: `watcher/timer.zig`'s `TimerTests(xev, Impl)` — the 5 R1 `test{}`s of the
// timer watcher (`timer`, `timer reset`, `timer reset before tick`, `timer reset
// after trigger`, `timer cancel`). These drive the real kqueue loop (Miri can't —
// real syscalls); `#[cfg(target_os = "macos")]` via the module gate. `undefined`
// completions in the reference become `Completion::default()` (Rust has no
// `undefined`; `run`/`timer` overwrite them). Callbacks are ZST non-capturing
// closures (Copy) — the faithful monomorphized-comptime-cb form (§2.2).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::kqueue::{Completion, Loop};
    use crate::looppkg::{CompletionState, RunMode};
    use core::ptr::addr_of_mut;

    // test "timer"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C3/C7, .port/phase-2/MIRI_EXCEPTIONS.md): drives the real kqueue loop (kqueue/kevent64/clock_gettime) -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
    )]
    fn timer() {
        let mut loop_ = Loop::init(crate::Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        // Add the timer
        let mut called = false;
        let mut c1 = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), RunError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            timer.run(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c1),
                1,
                addr_of_mut!(called),
                cb,
            );
        }

        // Wait
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(called);

        timer.deinit();
        loop_.deinit();
    }

    // test "timer reset"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C3/C7, .port/phase-2/MIRI_EXCEPTIONS.md): drives the real kqueue loop (kqueue/kevent64/clock_gettime) -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
    )]
    fn timer_reset() {
        let mut loop_ = Loop::init(crate::Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        let mut c_timer = Completion::default();
        let mut c_cancel = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), RunError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };

        // Add the timer
        let mut canceled = false;
        unsafe {
            timer.run(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c_timer),
                100_000,
                addr_of_mut!(canceled),
                cb,
            );
        }

        // Wait
        loop_.run(RunMode::NoWait).unwrap();
        assert!(!canceled);

        // Reset it
        unsafe {
            timer.reset(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c_timer),
                addr_of_mut!(c_cancel),
                1,
                addr_of_mut!(canceled),
                cb,
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        assert!(c_timer.state_of() == CompletionState::Dead);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        timer.deinit();
        loop_.deinit();
    }

    // test "timer reset before tick"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C3/C7, .port/phase-2/MIRI_EXCEPTIONS.md): drives the real kqueue loop (kqueue/kevent64/clock_gettime) -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
    )]
    fn timer_reset_before_tick() {
        let mut loop_ = Loop::init(crate::Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        let mut c_timer = Completion::default();
        let mut c_cancel = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), RunError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };

        // Add the timer
        let mut canceled = false;
        unsafe {
            timer.run(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c_timer),
                100_000,
                addr_of_mut!(canceled),
                cb,
            );
        }

        // Reset it
        unsafe {
            timer.reset(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c_timer),
                addr_of_mut!(c_cancel),
                1,
                addr_of_mut!(canceled),
                cb,
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        assert!(c_timer.state_of() == CompletionState::Dead);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        timer.deinit();
        loop_.deinit();
    }

    // test "timer reset after trigger"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C3/C7, .port/phase-2/MIRI_EXCEPTIONS.md): drives the real kqueue loop (kqueue/kevent64/clock_gettime) -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
    )]
    fn timer_reset_after_trigger() {
        let mut loop_ = Loop::init(crate::Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        let mut c_timer = Completion::default();
        let mut c_cancel = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), RunError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };

        // Add the timer
        let mut canceled = false;
        unsafe {
            timer.run(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c_timer),
                1,
                addr_of_mut!(canceled),
                cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        canceled = false;

        // Reset it
        unsafe {
            timer.reset(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c_timer),
                addr_of_mut!(c_cancel),
                1,
                addr_of_mut!(canceled),
                cb,
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        assert!(c_timer.state_of() == CompletionState::Dead);
        assert!(c_cancel.state_of() == CompletionState::Dead);

        timer.deinit();
        loop_.deinit();
    }

    // test "timer cancel"
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C3/C7, .port/phase-2/MIRI_EXCEPTIONS.md): drives the real kqueue loop (kqueue/kevent64/clock_gettime) -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
    )]
    fn timer_cancel() {
        let mut loop_ = Loop::init(crate::Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        // Add the timer
        let mut canceled = false;
        let mut c1 = Completion::default();
        let cb_timer = |ud: *mut bool,
                        _l: *mut Loop,
                        _c: *mut Completion,
                        r: core::result::Result<(), RunError>| {
            // ud.?.* = if (r) false else |err| err == error.Canceled;
            unsafe {
                *ud = match r {
                    Ok(()) => false,
                    Err(e) => e == RunError::Canceled,
                }
            };
            CallbackAction::Disarm
        };
        unsafe {
            timer.run(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c1),
                100_000,
                addr_of_mut!(canceled),
                cb_timer,
            );
        }

        // Cancel
        let mut cancel_confirm = false;
        let mut c2 = Completion::default();
        let cb_cancel = |ud: *mut bool,
                         _l: *mut Loop,
                         _c: *mut Completion,
                         r: core::result::Result<(), CancelError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            timer.cancel(
                &mut loop_ as *mut Loop,
                addr_of_mut!(c1),
                addr_of_mut!(c2),
                addr_of_mut!(cancel_confirm),
                cb_cancel,
            );
        }

        // Wait
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled);
        assert!(cancel_confirm);

        timer.deinit();
        loop_.deinit();
    }
}

// ---------------------------------------------------------------------------
// rustty-only live KAT (XEV_BLUEPRINT §7 KAT 2). Proves the loop actually runs
// on kqueue (Miri can't — real syscalls). `#[cfg(target_os = "macos")]` via the
// module gate.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod kat {
    use super::*;
    use crate::kqueue::{Completion, Loop};
    use crate::looppkg::RunMode;

    // KAT 2 — Timer fires: `Timer.run(1ms)` -> `run(.until_done)` -> assert the
    // callback fired AND `now()` advanced.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-xev C3/C7, .port/phase-2/MIRI_EXCEPTIONS.md): drives the real kqueue loop (kqueue/kevent64/clock_gettime) -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
    )]
    fn kat2_timer_fires() {
        let mut loop_ = Loop::init(crate::Options::default()).unwrap();

        let start_now = loop_.now();

        let mut fired = false;
        let mut c = Completion::default();
        let timer = Timer::init().unwrap();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), RunError>| {
            // The timer expired (not canceled).
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            timer.run(
                &mut loop_ as *mut Loop,
                core::ptr::addr_of_mut!(c),
                1,
                core::ptr::addr_of_mut!(fired),
                cb,
            );
        }

        loop_.run(RunMode::UntilDone).unwrap();

        assert!(fired, "timer callback fired");
        assert!(loop_.now() >= start_now, "loop now() advanced");

        timer.deinit();
        loop_.deinit();
    }
}
