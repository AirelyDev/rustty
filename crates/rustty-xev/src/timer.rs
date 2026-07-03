//! timer.rs — the C3 subset of libxev `watcher/timer.zig` (D6, XEV_BLUEPRINT
//! §4/§7).
//!
//! `Timer` (the `xev.Timer` high-level watcher) wraps the loop's low-level
//! `timer`/`timer_reset` methods. termio names `xev::Timer` (scroll/coalesce/
//! sync_reset/termios timers) and constructs it via `Timer::init()`, and KAT 2
//! (Timer fires) drives `Timer::run`. So `init`/`deinit`/`run` land in C3; the
//! `reset`/`cancel` methods + `watcher/timer.zig`'s 5 `test{}`s (`timer`, `reset`,
//! `reset before tick`, `reset after trigger`, `cancel`) are DEFERRED to a later
//! chunk (DEFERRALS.md) — the loop-level `timer`/`timer_reset` machinery they ride
//! is already ported + exercised by the kqueue timer tests in C3.
//
// PORT NOTE: the reference is `fn Timer(comptime xev: type) type` → `if
// (xev.dynamic) TimerDynamic else TimerLoop`. On macOS `xev.dynamic == false`
// (§0), so this is the `TimerLoop` instantiation (`xev = kqueue`).
// (module is `#[cfg(target_os = "macos")]`-gated in lib.rs)

use crate::kqueue::{Completion, Loop, Result, TimerTrigger};
use crate::looppkg::CallbackAction;

/// A timer fires a callback after a specified amount of time. A timer can
/// repeat by returning "rearm" in the callback or by rescheduling the
/// start within the callback.
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
}

/// PORT: `pub const RunError = error{Canceled, Unexpected}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunError {
    /// The timer was canceled before it could expire
    Canceled,
    /// Some unexpected error.
    Unexpected,
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
    fn kat2_timer_fires() {
        let mut loop_ = Loop::init().unwrap();

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
