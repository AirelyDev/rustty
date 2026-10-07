//! Tests for the Dynamic backend selector.
//!
//! PORTS: libxev src/dynamic.zig — the `test{}` blocks, plus the live KATs that
//! run both candidate backends.

use super::*;
use crate::backend::Backend;
use core::ptr;
use std::sync::Mutex;

// PORT NOTE: Zig runs a file's `test{}` blocks SERIALLY; Rust runs the whole
// crate's tests in parallel threads of ONE process. Every test here mutates
// the process-global `BACKEND` (`detect`/`prefer`), so they must not overlap.
// This lock restores the reference's serial execution for exactly the tests
// that touch the global.
static BACKEND_LOCK: Mutex<()> = Mutex::new(());

/// Take the serialization lock, tolerating poisoning. A panicking test would
/// otherwise poison the mutex and make EVERY other test here fail on
/// `unwrap()`, hiding which one actually broke. The guarded state is `()` —
/// there is nothing to be corrupted by a panic — so recovering the guard is
/// sound, and the real failure stays legible.
fn lock_backend() -> std::sync::MutexGuard<'static, ()> {
    BACKEND_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

// test { @import("std").testing.refAllDecls(@This()); }  (dynamic.zig:198)
#[test]
fn dynamic_ref_all_decls() {
    // The shared structures (dynamic.zig:60-69).
    let _ = core::mem::size_of::<Options>();
    let _ = core::mem::size_of::<RunMode>();
    let _ = core::mem::size_of::<CallbackAction>();
    let _ = core::mem::size_of::<CompletionState>();
    let _ = core::mem::size_of::<Completion>();
    let _ = core::mem::size_of::<PollEvent>();
    let _ = core::mem::size_of::<ReadBuffer>();
    let _ = core::mem::size_of::<WriteBuffer>();
    let _ = core::mem::size_of::<WriteQueue>();
    let _ = core::mem::size_of::<WriteRequest>();

    // Error types (dynamic.zig:72-79).
    let _ = core::mem::size_of::<AcceptError>();
    let _ = core::mem::size_of::<CancelError>();
    let _ = core::mem::size_of::<CloseError>();
    let _ = core::mem::size_of::<ConnectError>();
    let _ = core::mem::size_of::<PollError>();
    let _ = core::mem::size_of::<ShutdownError>();
    let _ = core::mem::size_of::<WriteError>();
    let _ = core::mem::size_of::<ReadError>();
    let _ = core::mem::size_of::<Error>();
    let _ = core::mem::size_of::<NotifyError>();

    // Core types (dynamic.zig:82-88) + the loop (dynamic.zig:122).
    let _ = core::mem::size_of::<Loop>();
    let _ = core::mem::size_of::<Async>();
    let _ = core::mem::size_of::<Process>();
    let _ = core::mem::size_of::<Stream>();
    let _ = core::mem::size_of::<Timer>();

    // The dynamic-API marker + candidate list (dynamic.zig:46-48).
    // `DYNAMIC` is a `const`, so the check is a compile-time one (this is
    // the `hasDecl`-style marker consumers branch on, dynamic.zig:44-46).
    const { assert!(DYNAMIC) };
    assert_eq!(CANDIDATES, [Backend::IoUring, Backend::Epoll]);
    // `pub const candidates = bes;` — this instantiation IS `Xev(&.{
    // .io_uring, .epoll })`, i.e. exactly `backend.zig`'s `.linux` arm. If
    // `Backend::candidates()` ever changes, this file must be re-authored.
    assert_eq!(CANDIDATES.as_slice(), Backend::candidates());
}

// test "completion is zero-able" (dynamic.zig:202)
#[test]
fn dynamic_completion_zero_able() {
    let c: Completion = Completion::default();

    // Strengthened beyond the reference's `_ = c;` so this is not vacuous:
    // dynamic.zig:251-255 zero-inits the LAST candidate's variant, and
    // dynamic.zig:244-248 says the tag exists precisely so a zero-initialized
    // completion has a valid, readable backend tag.
    assert_eq!(c.tag(), *CANDIDATES.last().unwrap());
    assert_eq!(c.tag(), Backend::Epoll);
    // And the zero-init really is the backend's own `.{}` (a dead completion).
    assert_eq!(c.state(), CompletionState::Dead);
}

// The `ensureTag` contract (dynamic.zig:283-290): a mismatched tag is replaced
// with a ZERO-INITIALIZED value of the requested backend; a matching tag is a
// no-op.
#[test]
fn dynamic_completion_ensure_tag() {
    let mut c = Completion::default();
    assert_eq!(c.tag(), Backend::Epoll);

    // Mismatch → re-tagged + zero-initialized.
    c.ensure_tag(Backend::IoUring);
    assert_eq!(c.tag(), Backend::IoUring);
    assert_eq!(c.state(), CompletionState::Dead);

    // Match → no-op (idempotent).
    c.ensure_tag(Backend::IoUring);
    assert_eq!(c.tag(), Backend::IoUring);

    // And back again.
    c.ensure_tag(Backend::Epoll);
    assert_eq!(c.tag(), Backend::Epoll);
    assert_eq!(c.state(), CompletionState::Dead);
}

// The global defaults to the LAST candidate (dynamic.zig:91) — NOT the first,
// despite the doc comment at dynamic.zig:28-30 claiming otherwise. The other
// tests mutate the global, so assert on the static's declared initializer.
#[test]
fn dynamic_backend_default_is_last_candidate() {
    assert_eq!(BACKEND_DEFAULT, *CANDIDATES.last().unwrap());
    assert_eq!(BACKEND_DEFAULT, Backend::Epoll);
}

// test "detect" (dynamic.zig:207)
#[test]
fn dynamic_detect() {
    let _guard = lock_backend();

    detect().unwrap();

    // inline for (bes) |be| { if (@intFromEnum(be) == @intFromEnum(backend)) {
    //     try testing.expect(be.Api().available()); break; }
    // } else try testing.expect(false);
    let be = backend();
    assert!(
        CANDIDATES.contains(&be),
        "detect() must land on a candidate backend, got {be:?}"
    );
    assert!(available(be));

    // Strengthened: detect picks the FIRST AVAILABLE candidate (dynamic.zig:96-105).
    let first_available = CANDIDATES.iter().copied().find(|b| available(*b)).unwrap();
    assert_eq!(be, first_available);
}

// test "prefer" (dynamic.zig:218)
#[test]
fn dynamic_prefer() {
    let _guard = lock_backend();

    // PORT NOTE (reference quirk, preserved): the reference asserts
    // `prefer(bes[0])` succeeds — i.e. it ASSUMES the first candidate
    // (io_uring) is available on the test host. On a kernel without io_uring
    // this test fails upstream too. This host has io_uring enabled.
    assert!(prefer(CANDIDATES[0]));

    let be = backend();
    assert!(CANDIDATES.contains(&be));
    assert!(available(be));

    // Strengthened: a successful `prefer` really SET the global (dynamic.zig:114).
    assert_eq!(be, CANDIDATES[0]);
    assert_eq!(be, Backend::IoUring);

    // A non-candidate is never preferred and never changes the global
    // (dynamic.zig:110-119: the `inline for` finds no match → `return false`).
    assert!(!prefer(Backend::Kqueue));
    assert_eq!(backend(), CANDIDATES[0]);

    // The other candidate is preferable too (epoll is always available on Linux).
    assert!(prefer(Backend::Epoll));
    assert_eq!(backend(), Backend::Epoll);
}

// test "loop basics" (dynamic.zig:229)
#[test]
fn dynamic_loop_basics() {
    let _guard = lock_backend();

    detect().unwrap();
    let mut l = Loop::init(Options::default()).unwrap();
    l.run(RunMode::UntilDone).unwrap();
    l.stop();
    assert!(l.stopped());
    l.deinit();
}

// -----------------------------------------------------------------------
// Live KATs — each runs under BOTH candidate backends, so the union really
// dispatches (a single-arm implementation would fail one half).
// -----------------------------------------------------------------------

// KAT — `Loop::init` really constructs the backend named by the GLOBAL
// (dynamic.zig:128's `switch (backend)`), not a hard-wired arm.
//
// The `Loop` union is untagged (dynamic.zig:454 — the tag lives in the
// global), so there is nothing on the handle to assert against; a
// `backend_tag()` accessor would just re-read the global and prove nothing.
// `Options::entries` discriminates the two arms for real instead: io_uring
// rejects > 8191 entries (io_uring.zig's `u13` cast → `TooManyEntries`),
// while epoll ignores `entries` entirely (it is documented io_uring-only,
// loop.zig:15). So the SAME options must fail under one backend and succeed
// under the other — which can only happen if `init` read the global.
#[test]
fn dynamic_loop_init_dispatches_on_the_global() {
    let _guard = lock_backend();

    let overflow = || Options {
        entries: 8192,
        ..Options::default()
    };

    assert!(prefer(Backend::IoUring));
    assert!(
        Loop::init(overflow()).is_err(),
        "io_uring must reject entries > 8191"
    );

    assert!(prefer(Backend::Epoll));
    let mut l = Loop::init(overflow()).expect("epoll ignores `entries`");
    l.deinit();
}

// KAT — `Loop::now`/`update_now` reach a live loop of each backend.
#[test]
fn dynamic_loop_now_advances() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(
            prefer(be),
            "candidate {be:?} must be available on this host"
        );
        let mut l = Loop::init(Options::default()).unwrap();
        let t0 = l.now();
        assert!(t0 > 0, "backend {be:?}: monotonic clock must be readable");
        std::thread::sleep(std::time::Duration::from_millis(5));
        l.update_now();
        assert!(l.now() > t0, "backend {be:?}: update_now must advance");
        l.deinit();
    }
}

// KAT — Async (eventfd) wakeup through the Dynamic union: wait, notify from
// another thread while the loop is BLOCKED, confirm the callback fired with
// the OUTER (`dynamic::Loop`/`dynamic::Completion`) pointers.
#[test]
fn dynamic_async_notify() {
    let _guard = lock_backend();

    struct Ud {
        wake: bool,
        l: *mut Loop,
        c: *mut Completion,
    }

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut notifier = Async::init().unwrap();

        let mut c_wait = Completion::default();
        let mut ud = Ud {
            wake: false,
            l: &mut loop_ as *mut Loop,
            c: ptr::addr_of_mut!(c_wait),
        };
        let cb = |ud: *mut Ud,
                  l: *mut Loop,
                  c: *mut Completion,
                  r: core::result::Result<(), WaitError>| {
            r.unwrap();
            unsafe {
                // The callback receives the OUTER pointers; assert they are the
                // very ones we passed in (what the `@fieldParentPtr` up-cast owes us).
                assert_eq!(l, (*ud).l, "callback must get the OUTER Loop pointer");
                assert_eq!(c, (*ud).c, "callback must get the OUTER Completion pointer");
                (*ud).wake = true;
            }
            CallbackAction::Disarm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_wait),
                ptr::addr_of_mut!(ud),
                cb,
            );
        }

        // `wait` re-tagged the zero-initialized (epoll-tagged) completion.
        assert_eq!(c_wait.tag(), be);

        loop_.run(RunMode::NoWait).unwrap();
        assert!(!ud.wake);

        let n = notifier; // `Async` is Copy (an fd handle) and Send (a plain i32).
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            n.notify().unwrap();
        });

        // Blocks until the spawned thread notifies. A hang is a real bug.
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(ud.wake, "backend {be:?} did not wake");
        t.join().unwrap();

        notifier.deinit();
        loop_.deinit();
    }
}

// test "async: notify first" (watcher/async.zig) through the Dynamic union,
// on BOTH candidate backends — a notification sent BEFORE the wait is
// registered must still wake the waiter (the eventfd counter retains it).
#[test]
fn dynamic_async_notify_first() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut notifier = Async::init().unwrap();

        // Send a notification
        notifier.notify().unwrap();

        // Wait
        let mut wake = false;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), WaitError>| {
            r.unwrap(); // _ = r catch unreachable
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

        // Wait for wake
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(wake, "backend {be:?}: pre-registered notify did not wake");

        notifier.deinit();
        loop_.deinit();
    }
}

// test "async batches multiple notifications" (watcher/async.zig) through
// the Dynamic union, on BOTH candidate backends — N notifications before
// (5) and after (1) the wait is registered must coalesce into EXACTLY ONE
// callback (the eventfd read drains the counter), even across 10 extra
// `no_wait` ticks with a rearming waiter.
#[test]
fn dynamic_async_batches_multiple_notifications() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut notifier = Async::init().unwrap();

        // Send a notification many times
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();
        notifier.notify().unwrap();

        // Wait
        let mut count: u32 = 0;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut u32,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), WaitError>| {
            r.unwrap(); // _ = r catch unreachable
            unsafe { *ud += 1 };
            CallbackAction::Rearm
        };
        unsafe {
            notifier.wait(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_wait),
                ptr::addr_of_mut!(count),
                cb,
            );
        }

        // Send a notification
        notifier.notify().unwrap();

        // Wait for wake
        loop_.run(RunMode::Once).unwrap();
        for _ in 0..10 {
            loop_.run(RunMode::NoWait).unwrap();
        }
        assert_eq!(count, 1, "backend {be:?}: notifications must coalesce");

        notifier.deinit();
        loop_.deinit();
    }
}

// KAT — Timer through the Dynamic union, driven from a ZERO-INITIALIZED
// completion. This is the exact scenario dynamic.zig:244-248 says the
// completion tag exists for: `Completion::default()` is epoll-tagged, so under
// io_uring `Timer::run` must `ensure_tag` it before use.
#[test]
fn dynamic_timer() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        let mut called = false;
        let mut c = Completion::default();
        assert_eq!(c.tag(), Backend::Epoll, "zero-init is always epoll-tagged");

        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), TimerRunError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            timer.run(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c),
                1,
                ptr::addr_of_mut!(called),
                cb,
            );
        }

        // The zero-initialized completion was re-tagged to the live backend.
        assert_eq!(c.tag(), be);
        assert_eq!(c.state(), CompletionState::Active);

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(called, "backend {be:?} timer did not fire");
        assert_eq!(c.state(), CompletionState::Dead);

        timer.deinit();
        loop_.deinit();
    }
}

// KAT — Timer::reset through the Dynamic union (exercises `Loop::timer_reset`
// dispatch + the second `ensure_tag` on `c_cancel`).
#[test]
fn dynamic_timer_reset() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        let mut called = false;
        let mut c = Completion::default();
        let mut c_cancel = Completion::default();

        let cb = |ud: *mut bool,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<(), TimerRunError>| {
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };

        // Never started → equivalent to `run`; c_cancel is NOT used.
        unsafe {
            timer.reset(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c),
                ptr::addr_of_mut!(c_cancel),
                1,
                ptr::addr_of_mut!(called),
                cb,
            );
        }
        assert_eq!(c.tag(), be);
        assert_eq!(c_cancel.tag(), be);
        assert_eq!(c_cancel.state(), CompletionState::Dead);

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(called, "backend {be:?} timer reset did not fire");

        timer.deinit();
        loop_.deinit();
    }
}

// KAT — `watcher/timer.zig`'s `test "timer cancel"` (timer.zig:595-639),
// through the Dynamic union on BOTH candidates: a long timer is started, then
// canceled; the ORIGINAL timer callback must still fire but with
// `error.Canceled`, and the cancel callback must confirm success. This pins
// both `Timer::cancel` arms (io_uring `timer_remove`, epoll `cancel`) end to
// end — if cancel did nothing, `loop.run(.until_done)` would block for the
// full 100s rather than return.
#[test]
fn dynamic_timer_cancel() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let timer = Timer::init().unwrap();

        // Add the timer. `100_000` ms — so long that the ONLY way the loop
        // finishes is a real cancellation.
        let mut canceled = false;
        let mut c1 = Completion::default();
        let run_cb = |ud: *mut bool,
                      _l: *mut Loop,
                      _c: *mut Completion,
                      r: core::result::Result<(), TimerRunError>| {
            // `ud.?.* = if (r) false else |err| err == error.Canceled;`
            unsafe { *ud = matches!(r, Err(TimerRunError::Canceled)) };
            CallbackAction::Disarm
        };
        unsafe {
            timer.run(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c1),
                100_000,
                ptr::addr_of_mut!(canceled),
                run_cb,
            );
        }
        assert_eq!(c1.state(), CompletionState::Active);

        // Cancel.
        let mut cancel_confirm = false;
        let mut c2 = Completion::default();
        let cancel_cb = |ud: *mut bool,
                         _l: *mut Loop,
                         _c: *mut Completion,
                         r: core::result::Result<(), TimerCancelError>| {
            // `_ = r catch unreachable; ud.?.* = true;`
            r.unwrap();
            unsafe { *ud = true };
            CallbackAction::Disarm
        };
        unsafe {
            timer.cancel(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c1),
                ptr::addr_of_mut!(c2),
                ptr::addr_of_mut!(cancel_confirm),
                cancel_cb,
            );
        }
        // Both completions live under the running backend's arm.
        assert_eq!(c1.tag(), be);
        assert_eq!(c2.tag(), be);

        // Wait.
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(canceled, "backend {be:?}: timer cb did not report Canceled");
        assert!(cancel_confirm, "backend {be:?}: cancel cb did not confirm");
        assert_eq!(c1.state(), CompletionState::Dead);

        timer.deinit();
        loop_.deinit();
    }
}

// KAT — Process spawn+exit through the Dynamic union.
#[test]
#[allow(clippy::zombie_processes)]
fn dynamic_process_spawn_exit() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

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
        assert_eq!(code, Some(0), "backend {be:?}: exit status 0");

        p.deinit();
        loop_.deinit();
    }
}

// test "process wait with non-zero exit code" (watcher/process.zig) through
// the Dynamic union, on BOTH candidate backends — the reference's
// `ProcessTests` instantiation for `ProcessDynamic`. Asserts the pidfd
// waitid reap surfaces the REAL `si_status`, not just zero.
#[test]
#[allow(clippy::zombie_processes)]
fn dynamic_process_wait_with_non_zero_exit_code() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        // posix_spawn `sh -c "exit 42"` (the reference's argv_42).
        let mut pid: libc::pid_t = 0;
        let path = b"/bin/sh\0";
        let arg_c = b"-c\0";
        let arg_script = b"exit 42\0";
        let mut argv: [*mut libc::c_char; 4] = [
            path.as_ptr() as *mut libc::c_char,
            arg_c.as_ptr() as *mut libc::c_char,
            arg_script.as_ptr() as *mut libc::c_char,
            ptr::null_mut(),
        ];
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
        assert_eq!(rc, 0, "posix_spawn sh -c 'exit 42'");

        let mut loop_ = Loop::init(Options::default()).unwrap();
        let mut p = Process::init(pid).unwrap();

        let mut code: Option<u32> = None;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut Option<u32>,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<u32, ProcessWaitError>| {
            unsafe { *ud = Some(r.unwrap()) }; // ud.?.* = r catch unreachable
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
        assert_eq!(code, Some(42), "backend {be:?}: exit status 42");

        p.deinit();
        loop_.deinit();
    }
}

// test "process wait on a process that already exited" (watcher/process.zig)
// through the Dynamic union, on BOTH candidate backends — Process::init
// FIRST (the pidfd must be opened while the child exists), then the child
// is reaped OUT FROM UNDER the watcher via waitpid; the subsequent loop
// wait exercises the ECHILD → `InvalidChild` path (`r catch 0`).
#[test]
fn dynamic_process_wait_on_a_process_that_already_exited() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        // posix_spawn /bin/true (the reference's argv_0 equivalent).
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

        // _ = try child.wait();  — reap the child before the watcher does.
        let mut status: libc::c_int = 0;
        let reaped = unsafe { libc::waitpid(pid, &mut status, 0) };
        assert_eq!(reaped, pid, "waitpid must reap the child");

        let mut code: Option<u32> = None;
        let mut c_wait = Completion::default();
        let cb = |ud: *mut Option<u32>,
                  _l: *mut Loop,
                  _c: *mut Completion,
                  r: core::result::Result<u32, ProcessWaitError>| {
            unsafe { *ud = Some(r.unwrap_or(0)) }; // ud.?.* = r catch 0
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
        assert_eq!(code, Some(0), "backend {be:?}: already-reaped wait");

        p.deinit();
        loop_.deinit();
    }
}

// KAT — Pty write round-trip through the Dynamic union: `Stream::read` on the
// child end + `Stream::queue_write` on the parent end. Exercises the dynamic
// `WriteQueue`/`WriteRequest` tagging + the `ReadBuffer`/`WriteBuffer`
// to/from-backend conversions.
#[test]
fn dynamic_pty_write_roundtrip() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut pty = Pty::init().unwrap();
        let mut loop_ = Loop::init(Options::default()).unwrap();

        let parent = Stream::init_fd(pty.parent); // write side
        let child = Stream::init_fd(pty.child); // read side

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
                ReadBuffer::Slice(&mut read_buf[..] as *mut [u8]),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }
        assert_eq!(c_read.tag(), be);
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

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
                WriteBuffer::Slice(payload.as_slice() as *const [u8]),
                ptr::null_mut(),
                write_cb,
            );
        }

        // `queue_write` re-tagged the zero-initialized WriteQueue.
        assert_eq!(write_queue.tag(), be);
        assert!(!write_queue.empty());

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some(), "backend {be:?} read did not complete");
        assert_eq!(payload, &read_buf[0..read_len.unwrap()]);
        assert!(write_queue.empty());

        pty.deinit();
        loop_.deinit();
    }
}

// test "pty: child to parent" (watcher/stream.zig) through the Dynamic
// union, on BOTH candidate backends — direct `Stream::write` on the child
// end, read on the parent end.
#[test]
fn dynamic_pty_child_to_parent() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut pty = Pty::init().unwrap();
        let mut loop_ = Loop::init(Options::default()).unwrap();

        let parent = Stream::init_fd(pty.parent);
        let child = Stream::init_fd(pty.child);

        // Read
        let mut read_buf = [0u8; 128];
        let mut read_len: Option<usize> = None;
        let mut c_read = Completion::default();
        let read_cb = |ud: *mut Option<usize>,
                       _l: *mut Loop,
                       _c: *mut Completion,
                       _s: Stream,
                       _b: ReadBuffer,
                       r: core::result::Result<usize, ReadError>| {
            unsafe { *ud = Some(r.unwrap()) }; // ud.?.* = r catch unreachable
            CallbackAction::Disarm
        };
        unsafe {
            parent.read(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_read),
                ReadBuffer::Slice(&mut read_buf[..] as *mut [u8]),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }

        // This should not block!
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

        // Send
        let send_buf = b"hello, world!";
        let mut c_write = Completion::default();
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap(); // _ = r catch unreachable
            CallbackAction::Disarm
        };
        unsafe {
            child.write::<(), _>(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_write),
                WriteBuffer::Slice(send_buf.as_slice() as *const [u8]),
                ptr::null_mut(),
                write_cb,
            );
        }

        // The write and read should trigger
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some(), "backend {be:?} read did not complete");
        assert_eq!(send_buf, &read_buf[0..read_len.unwrap()]);

        pty.deinit();
        loop_.deinit();
    }
}

// test "pty: parent to child" (watcher/stream.zig) through the Dynamic
// union, on BOTH candidate backends — direct `Stream::write` on the parent
// end, read on the child end.
#[test]
fn dynamic_pty_parent_to_child() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut pty = Pty::init().unwrap();
        let mut loop_ = Loop::init(Options::default()).unwrap();

        let parent = Stream::init_fd(pty.parent);
        let child = Stream::init_fd(pty.child);

        // Read
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
                ReadBuffer::Slice(&mut read_buf[..] as *mut [u8]),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }

        // This should not block!
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

        // Send (note the newline at the end of the buf is important
        // since we're in cooked mode)
        let send_buf = b"hello, world!\n";
        let mut c_write = Completion::default();
        let write_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap();
            CallbackAction::Disarm
        };
        unsafe {
            parent.write::<(), _>(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(c_write),
                WriteBuffer::Slice(send_buf.as_slice() as *const [u8]),
                ptr::null_mut(),
                write_cb,
            );
        }

        // The write and read should trigger
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some(), "backend {be:?} read did not complete");
        assert_eq!(send_buf, &read_buf[0..read_len.unwrap()]);

        pty.deinit();
        loop_.deinit();
    }
}

// test "pty: queued writes" (watcher/stream.zig) through the Dynamic union,
// on BOTH candidate backends — TWO `queue_write` requests must drain FIFO
// ("hello, " then "world!\n" concatenate). The `WriteRequest.from(c_result)`
// identity assert is OMITTED here: the reference guards it with
// `if (!xev.dynamic)` (stream.zig:1373), so it is NOT part of the dynamic
// contract (the outer Completion is embedded differently per arm).
#[test]
#[cfg_attr(
    target_arch = "x86_64",
    ignore = "SkipZigTest: upstream skips the dynamic instantiation on x86_64 (R14)"
)]
fn dynamic_pty_queued_writes() {
    // this test fails on x86_64 with a strange error but passes
    // on aarch64. for now, just let it go until we investigate.
    // (`if (xev.dynamic and builtin.cpu.arch == .x86_64) return
    // error.SkipZigTest;` — stream.zig:1281: upstream skips the DYNAMIC
    // instantiation of this test on x86_64.)

    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut pty = Pty::init().unwrap();
        let mut loop_ = Loop::init(Options::default()).unwrap();

        let parent = Stream::init_fd(pty.parent);
        let child = Stream::init_fd(pty.child);

        // Read
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
                ReadBuffer::Slice(&mut read_buf[..] as *mut [u8]),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }

        // This should not block!
        loop_.run(RunMode::NoWait).unwrap();
        assert!(read_len.is_none());

        let mut write_queue: WriteQueue = WriteQueue::default();
        let mut write_req: [WriteRequest; 2] = [WriteRequest::default(), WriteRequest::default()];

        // Send (note the newline at the end of the buf is important
        // since we're in cooked mode)
        let hello_cb = |_ud: *mut (),
                        _l: *mut Loop,
                        _c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap();
            CallbackAction::Disarm
        };
        unsafe {
            parent.queue_write::<(), _>(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req[0]),
                WriteBuffer::Slice(b"hello, ".as_slice() as *const [u8]),
                ptr::null_mut(),
                hello_cb,
            );
        }

        let mut c_result: Option<*mut Completion> = None;
        let world_cb = |ud: *mut Option<*mut Completion>,
                        _l: *mut Loop,
                        c: *mut Completion,
                        _s: Stream,
                        _b: WriteBuffer,
                        r: core::result::Result<usize, WriteError>| {
            r.unwrap();
            unsafe { *ud = Some(c) }; // ud.?.* = c
            CallbackAction::Disarm
        };
        unsafe {
            parent.queue_write(
                &mut loop_ as *mut Loop,
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req[1]),
                WriteBuffer::Slice(b"world!\n".as_slice() as *const [u8]),
                ptr::addr_of_mut!(c_result),
                world_cb,
            );
        }

        // The write and read should trigger
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(read_len.is_some(), "backend {be:?} read did not complete");
        assert_eq!(b"hello, world!\n", &read_buf[0..read_len.unwrap()]);
        assert!(c_result.is_some());

        pty.deinit();
        loop_.deinit();
    }
}

// PORT NOTE (test-harness divergence, ports): see io_uring.rs's identical NOTE
// on its `PORT_*` registry — Rust's whole-crate parallel test harness means
// every fixed-port socket test (unlike the reference's serial `test{}` runner)
// needs its OWN port so concurrent tests never race a bind/connect. This is
// the `dynamic.rs`-local twin of that registry (3131-3136 are claimed by
// kqueue.rs/epoll.rs/io_uring.rs — including epoll.rs's `PORT_UDP_READ_WRITE`
// = 3135; 3137 and 3139 by `dynamic_udp_read_write` below).
const PORT_DYNAMIC_TCP_ROUNDTRIP: u16 = 3138;
// `dynamic_udp_read_write`'s port PER BACKEND iteration (N515, P7-W24). One
// port for both iterations lost the epoll iteration's datagram in about 1-6% of
// whole-crate runs on Linux (rvm): the server binds with SO_REUSEPORT
// (`UDP::bind`, as the reference), and the io_uring iteration's server, closed
// by an io_uring close op, can stay in 3137's reuseport group until the kernel
// finishes releasing it, so the hash could route the datagram to that dying
// socket and the read waited forever. The reference shares 3132 across its UDP
// test instantiations, run one after another by Zig's serial test runner.
const PORT_DYNAMIC_UDP_IO_URING: u16 = 3137;
const PORT_DYNAMIC_UDP_EPOLL: u16 = 3139;

// KAT — TCP accept/connect/send/recv/close round-trip through the Dynamic
// union, on BOTH candidate backends. This is the reference's `TCPDynamic`
// instantiation of `test "TCP: accept/connect/send/recv/close"` (tcp.zig:518,
// run via `TCPDynamic` at tcp.zig:498-500) — it exercises the dynamic `TCP`
// enum tagging (the accept/connect/read/write/close callback re-wraps +
// up/down-casts) plus the `ReadBuffer`/`WriteBuffer` to/from-backend
// conversions on the stream path. A ThreadPool is attached for BOTH backends,
// mirroring `dynamic_udp_read_write`/`dynamic_file_read_write`: the epoll
// arm's `TCP::close` rides it (the `.threadpool` flag), io_uring's is a real
// SQE that ignores it.
#[test]
fn dynamic_tcp_accept_connect_send_recv_close() {
    use crate::thread_pool::{Config, ThreadPool};

    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        let address = net::Address::parse_ip4(b"127.0.0.1", PORT_DYNAMIC_TCP_ROUNDTRIP).unwrap();
        let server = TCP::init(address).unwrap();
        server.bind(address).unwrap();
        server.listen(1).unwrap();

        let client = TCP::init(address).unwrap();

        // Accept
        let mut c_accept = Completion::default();
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
        // `accept` re-tagged the zero-initialized completion.
        assert_eq!(c_accept.tag(), be);

        // Connect
        let mut c_connect = Completion::default();
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
        assert_eq!(c_connect.tag(), be);

        loop_.run(RunMode::UntilDone).unwrap();
        assert!(
            server_conn.is_some(),
            "backend {be:?} TCP accept did not complete"
        );
        assert!(connected, "backend {be:?} TCP connect did not complete");
        let server_conn_tcp = server_conn.unwrap();

        // Close the server (the LISTENER closes before the send/recv below —
        // the oracle's own sequence, tcp.zig:593-609: the accepted connection
        // must survive its parent listener closing while data is in flight).
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
        assert!(
            server_closed,
            "backend {be:?} TCP listener close did not complete"
        );

        // Send
        let send_buf = [1u8, 1, 2, 3, 5, 8, 13];
        let mut c_write = Completion::default();
        unsafe {
            client.write(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_write),
                WriteBuffer::Slice(send_buf.as_slice() as *const [u8]),
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
        assert_eq!(c_write.tag(), be);

        // Receive
        let mut c_read = Completion::default();
        let mut recv_buf = [0u8; 128];
        let mut recv_len: Option<usize> = None;
        unsafe {
            server_conn_tcp.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
                ReadBuffer::Slice(&mut recv_buf[..] as *mut [u8]),
                ptr::addr_of_mut!(recv_len),
                |ud: *mut Option<usize>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: TCP,
                 _b: ReadBuffer,
                 r: core::result::Result<usize, ReadError>| {
                    *ud = Some(r.unwrap());
                    CallbackAction::Disarm
                },
            );
        }
        assert_eq!(c_read.tag(), be);

        // Wait for the send/receive
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(
            recv_len.is_some(),
            "backend {be:?} TCP read did not complete"
        );
        let recv_len = recv_len.unwrap();
        assert!(recv_len > 0);
        assert_eq!(send_buf.as_slice(), &recv_buf[0..recv_len]);

        // Close (the accepted connection + the client — the listener already
        // closed above, before send/recv, per the oracle's sequence).
        unsafe {
            server_conn_tcp.close(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
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
}

// KAT — UDP datagram round-trip through the Dynamic union, on BOTH candidate
// backends. This is the reference's `UDPDynamic` instantiation of
// `test "UDP: read/write"` (udp.zig:914 via udp.zig:894) — it exercises the
// dynamic `UdpState` union tagging (the `@fieldParentPtr` up/down-casts) plus
// the `ReadBuffer`/`WriteBuffer` to/from-backend conversions on the msghdr path.
#[test]
fn dynamic_udp_read_write() {
    use crate::thread_pool::{Config, ThreadPool};

    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        // The epoll arm's `UDP::close` rides the thread pool (the `.threadpool`
        // flag); io_uring's is a real SQE that ignores the pool. Attach one for
        // BOTH so the single test body works under either backend.
        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        let port = match be {
            Backend::IoUring => PORT_DYNAMIC_UDP_IO_URING,
            _ => PORT_DYNAMIC_UDP_EPOLL,
        };
        let address = net::Address::parse_ip4(b"127.0.0.1", port).unwrap();
        let server = UDP::init(address).unwrap();
        let client = UDP::init(address).unwrap();

        // Bind / Recv
        server.bind(address).unwrap();
        let mut c_read = Completion::default();
        let mut s_read = UdpState::undefined();
        let mut recv_buf = [0u8; 128];
        let mut recv_len: Option<usize> = None;
        unsafe {
            server.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
                ptr::addr_of_mut!(s_read),
                ReadBuffer::Slice(&mut recv_buf[..] as *mut [u8]),
                ptr::addr_of_mut!(recv_len),
                |ud: *mut Option<usize>,
                 _l: *mut Loop,
                 _c: *mut Completion,
                 _s: *mut UdpState,
                 _addr: net::Address,
                 _sock: UDP,
                 _b: ReadBuffer,
                 r: core::result::Result<usize, ReadError>| {
                    *ud = Some(r.unwrap());
                    CallbackAction::Disarm
                },
            );
        }
        // `read` re-tagged the zero-initialized (epoll-tagged) completion.
        assert_eq!(c_read.tag(), be);

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
                WriteBuffer::Slice(send_buf.as_slice() as *const [u8]),
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
        assert_eq!(c_write.tag(), be);

        // Wait for the send/receive
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(
            recv_len.is_some(),
            "backend {be:?} UDP read did not complete"
        );
        let recv_len = recv_len.unwrap();
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
}

// Helper to open a pty (test-only), mirroring watcher/stream.zig's `Pty`.
struct Pty {
    parent: libc::c_int,
    child: libc::c_int,
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
        parent: *mut libc::c_int,
        child: *mut libc::c_int,
        name: *mut u8,
        termios: *const core::ffi::c_void,
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
        let mut parent_fd: libc::c_int = 0;
        let mut child_fd: libc::c_int = 0;
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
// File watcher tests through the Dynamic union, on BOTH candidate backends.
// The reference's `FileDynamic` instantiation of `FileTests` (file.zig:495).
// These exercise the dynamic File union tagging (the read/write/pread/pwrite
// callback re-wraps + the ReadBuffer/WriteBuffer to/from-backend conversions).
// A ThreadPool is attached for BOTH backends: the epoll arm's File ops ride it
// (blocking), io_uring's are native SQEs that ignore it.
// -----------------------------------------------------------------------

fn temp_file() -> (libc::c_int, std::path::PathBuf) {
    use std::os::unix::io::IntoRawFd;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("rustty_xev_file_dyn_{}_{}", std::process::id(), n));
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    (f.into_raw_fd(), path)
}

fn open_read_fd(path: &std::path::Path) -> libc::c_int {
    use std::os::unix::io::IntoRawFd;
    std::fs::File::open(path).unwrap().into_raw_fd()
}

// test "read/write" + "pread/pwrite" + "queued writes" (file.zig:596/671/744),
// all through the Dynamic `File`, on each candidate backend.
#[test]
fn dynamic_file_read_write() {
    use crate::thread_pool::{Config, ThreadPool};

    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut pool = ThreadPool::init(Config::default());
        let pool_ptr: *mut ThreadPool = &mut pool;
        let mut loop_ = Loop::init(Options {
            thread_pool: Some(pool_ptr),
            ..Options::default()
        })
        .unwrap();

        let (wfd, path) = temp_file();
        let file = File::init(wfd).unwrap();

        // --- plain write then read ---
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
                WriteBuffer::Slice(write_buf.as_slice() as *const [u8]),
                ptr::null_mut::<()>(),
                write_cb,
            );
        }
        assert_eq!(c_write.tag(), be);
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(unsafe { libc::fsync(wfd) }, 0);

        let rfd = open_read_fd(&path);
        let file2 = File::init(rfd).unwrap();
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
        let mut c_read = Completion::default();
        unsafe {
            file2.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_read),
                ReadBuffer::Slice(&mut read_buf[..] as *mut [u8]),
                ptr::addr_of_mut!(read_len),
                read_cb,
            );
        }
        assert_eq!(c_read.tag(), be);
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(read_len, write_buf.len());
        assert_eq!(&write_buf, &read_buf[0..read_len]);

        unsafe {
            libc::close(wfd);
            libc::close(rfd);
        }
        std::fs::remove_file(&path).ok();

        // --- pread/pwrite at offset 0 ---
        let (wfd2, path2) = temp_file();
        let pfile = File::init(wfd2).unwrap();
        let mut c_pwrite = Completion::default();
        unsafe {
            pfile.pwrite(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_pwrite),
                WriteBuffer::Slice(write_buf.as_slice() as *const [u8]),
                0,
                ptr::null_mut::<()>(),
                write_cb,
            );
        }
        assert_eq!(c_pwrite.tag(), be);
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(unsafe { libc::fsync(wfd2) }, 0);

        let rfd2 = open_read_fd(&path2);
        let pfile2 = File::init(rfd2).unwrap();
        let mut pread_buf = [0u8; 128];
        let mut pread_len: usize = 0;
        let mut c_pread = Completion::default();
        unsafe {
            pfile2.pread(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_pread),
                ReadBuffer::Slice(&mut pread_buf[..] as *mut [u8]),
                0,
                ptr::addr_of_mut!(pread_len),
                read_cb,
            );
        }
        assert_eq!(c_pread.tag(), be);
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(&write_buf, &pread_buf[0..pread_len]);

        unsafe {
            libc::close(wfd2);
            libc::close(rfd2);
        }
        std::fs::remove_file(&path2).ok();

        // --- queued writes ("1234" then "5678") ---
        let (wfd3, path3) = temp_file();
        let qfile = File::init(wfd3).unwrap();
        let mut write_queue: WriteQueue = WriteQueue::default();
        let mut write_req: [WriteRequest; 2] = [WriteRequest::default(), WriteRequest::default()];
        // The queued writes are 4 bytes each, so this callback only asserts
        // success (unlike `write_cb`, which pins the 7-byte payload above).
        let qwrite_cb = |_ud: *mut (),
                         _l: *mut Loop,
                         _c: *mut Completion,
                         _s: File,
                         _b: WriteBuffer,
                         r: core::result::Result<usize, WriteError>| {
            r.unwrap();
            CallbackAction::Disarm
        };
        unsafe {
            qfile.queue_write::<(), _>(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req[0]),
                WriteBuffer::Slice(b"1234".as_slice() as *const [u8]),
                ptr::null_mut(),
                qwrite_cb,
            );
            qfile.queue_write::<(), _>(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(write_queue),
                ptr::addr_of_mut!(write_req[1]),
                WriteBuffer::Slice(b"5678".as_slice() as *const [u8]),
                ptr::null_mut(),
                qwrite_cb,
            );
        }
        assert_eq!(write_queue.tag(), be);
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(write_queue.empty());
        assert_eq!(unsafe { libc::fsync(wfd3) }, 0);

        let rfd3 = open_read_fd(&path3);
        let qfile2 = File::init(rfd3).unwrap();
        let mut qread_buf = [0u8; 128];
        let mut qread_len: usize = 0;
        let mut c_qread = Completion::default();
        unsafe {
            qfile2.read(
                ptr::addr_of_mut!(loop_),
                ptr::addr_of_mut!(c_qread),
                ReadBuffer::Slice(&mut qread_buf[..] as *mut [u8]),
                ptr::addr_of_mut!(qread_len),
                read_cb,
            );
        }
        loop_.run(RunMode::UntilDone).unwrap();
        assert_eq!(b"12345678", &qread_buf[0..qread_len]);

        unsafe {
            libc::close(wfd3);
            libc::close(rfd3);
        }
        std::fs::remove_file(&path3).ok();

        loop_.deinit();
        pool.shutdown();
        pool.deinit();
    }
}

// test "poll" (file.zig:557) through the Dynamic `File`, on each candidate.
#[test]
fn dynamic_file_poll() {
    let _guard = lock_backend();

    for be in CANDIDATES {
        assert!(prefer(be));

        let mut loop_ = Loop::init(Options::default()).unwrap();

        let mut fds = [0 as libc::c_int; 2];
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
        assert_eq!(c.tag(), be);
        loop_.run(RunMode::UntilDone).unwrap();
        assert!(ready);

        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
        loop_.deinit();
    }
}
