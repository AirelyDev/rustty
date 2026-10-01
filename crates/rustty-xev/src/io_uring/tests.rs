//! Tests for the io_uring backend.
//!
//! PORTS: libxev src/backend/io_uring.zig — the `test{}` blocks. The socket,
//! process/pty and async/file tests live in the `tests/` children.

use super::*;
use crate::linux::timerfd::Timerfd;
use core::ffi::c_void;

// PORT NOTE: the shared `.{ b.* = true; return .disarm; }` timer callback the
// "default completion"/"stop" tests use.
fn set_true_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, _r: Result) -> CallbackAction {
    // const b = @as(*bool, @ptrCast(ud.?)); b.* = true;
    unsafe { *ud.cast::<bool>() = true };
    CallbackAction::Disarm
}

// test "io_uring: available"
#[test]
fn io_uring_available() {
    assert!(available());
}

// test "io_uring: overflow entries count"
#[test]
fn io_uring_overflow_entries_count() {
    {
        let mut loop_ = Loop::init(Options::default()).unwrap();
        loop_.deinit();
    }

    {
        // try testing.expectError(error.TooManyEntries, Loop.init(.{
        //     .entries = std.math.pow(u14, 2, 13),
        // }));
        let r = Loop::init(Options {
            entries: 2u32.pow(13),
            ..Options::default()
        });
        assert!(matches!(r, Err(InitError::TooManyEntries)));
    }
}

// test "Completion size"
#[test]
fn io_uring_completion_size() {
    // Just so we are aware when we change the size (Rust layout differs from
    // Zig's — the pinned constant is the measured Rust size, see COMPLETION_SIZE).
    assert_eq!(core::mem::size_of::<Completion>(), COMPLETION_SIZE);
}

// test "io_uring: default completion"
#[test]
fn io_uring_default_completion() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Due to implementation details, we need to have an ongoing completion
    // to test that the noop operation works properly.
    let mut timer_called = false;
    let mut c1 = Completion::default();
    unsafe {
        loop_.timer(
            ptr::addr_of_mut!(c1),
            1,
            ptr::addr_of_mut!(timer_called).cast::<c_void>(),
            set_true_cb,
        );
    }

    // var c: Completion = .{};
    let mut c = Completion::default();
    unsafe { loop_.add(ptr::addr_of_mut!(c)) };

    // Tick
    loop_.run(RunMode::UntilDone).unwrap();

    // Completion should have been called and be dead.
    assert!(c.state_of() == CompletionState::Dead);
    assert!(timer_called);

    loop_.deinit();
}

/// PORT-ADDED (N431): a callback re-enters the loop through a pointer the
/// caller stored (termio's `td.loop_` -> `stream.rs` `(*loop_).add`), not
/// through its `l`, while the loop runs through `run_raw` with that same
/// pointer. The second timer is armed from inside the first callback and
/// stops the loop, so both callbacks must have run for `until_done` to
/// return.
// Real-code twin of mirihar-n431 `raw_local_root` (the backend's syscalls
// keep it out of Miri; the epoll twin runs under Miri on the Linux target).
#[test]
fn io_uring_callback_reenters_through_stored_loop_pointer() {
    struct Ud {
        root: *mut Loop,
        c2: *mut Completion,
        first: bool,
        second: bool,
    }
    fn first_cb(
        ud: *mut core::ffi::c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        _r: Result,
    ) -> CallbackAction {
        let ud = ud as *mut Ud;
        unsafe {
            (*ud).first = true;
            // Through the stored pointer, not `_l`.
            (*(*ud).root).timer((*ud).c2, 0, ud.cast(), second_cb);
        }
        CallbackAction::Disarm
    }
    fn second_cb(
        ud: *mut core::ffi::c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        _r: Result,
    ) -> CallbackAction {
        let ud = ud as *mut Ud;
        unsafe {
            (*ud).second = true;
            (*(*ud).root).stop();
        }
        CallbackAction::Disarm
    }

    let mut loop_ = Loop::init(Options::default()).unwrap();
    let root: *mut Loop = core::ptr::addr_of_mut!(loop_);
    let mut c1 = Completion::default();
    let mut c2 = Completion::default();
    let mut ud = Ud {
        root,
        c2: core::ptr::addr_of_mut!(c2),
        first: false,
        second: false,
    };
    let udp: *mut Ud = core::ptr::addr_of_mut!(ud);
    unsafe {
        (*root).timer(core::ptr::addr_of_mut!(c1), 0, udp.cast(), first_cb);
        Loop::run_raw(root, RunMode::UntilDone).unwrap();
        assert!((*udp).first);
        assert!((*udp).second);
        assert!((*root).stopped());
        (*root).deinit();
    }
}

// test "io_uring: loop time"
#[test]
fn io_uring_loop_time() {
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

// test "io_uring: stop"
// `called` is mutated only through the raw userdata pointer inside the timer
// callback (the intrusive-completion contract), which clippy's dataflow can't
// see — the reference is `try testing.expect(!called)`.
#[allow(clippy::while_immutable_condition)]
#[test]
fn io_uring_stop() {
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

// -----------------------------------------------------------------------
// L1 LT6: the timer family (io_uring.zig:1173/1226/1272/1357).
// -----------------------------------------------------------------------

// test "io_uring: timerfd" (io_uring.zig:1173)
#[test]
fn io_uring_timerfd() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // We'll try with a simple timerfd
    let t = Timerfd::init().unwrap();
    // The reference arms a 1ns timer (`.{ .value = .{ .nanoseconds = 1 } }`);
    // our Task-2 `set` is ms-granularity, so we arm the shortest one-shot it
    // exposes (1ms). Observably identical: the timerfd becomes readable
    // near-instantly and `run(until_done)` waits. (Same divergence note as
    // epoll.rs's `epoll_timerfd`.)
    t.set(1).unwrap();

    // Add the timer.
    //
    // PORT NOTE (verbatim from the reference): we should be able to use `read`
    // here but on Kernel 6.15.4 there is a bug that prevents the read from ever
    // firing with io_uring. I don't know why. I changed this to a poll so tests
    // pass, which should also be fine!
    let mut called = false;
    let mut c = Completion {
        op: Operation::Poll {
            fd: t.fd,
            // The reference relies on the Zig field default `events =
            // posix.POLL.IN`; Rust enum variants carry no field defaults, so
            // the default value is spelled at the call site.
            events: libc::POLLIN as u32,
        },
        userdata: ptr::addr_of_mut!(called).cast::<c_void>(),
        callback: set_true_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c)) };

    // Verify states
    assert!(c.state_of() == CompletionState::Active);

    // Tick
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(called);
    assert!(c.state_of() == CompletionState::Dead);

    t.deinit();
    loop_.deinit();
}

// test "io_uring: timer" (io_uring.zig:1226)
#[test]
fn io_uring_timer() {
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
    // `called`/`called2` are mutated only through the raw userdata pointer
    // inside the timer callback (the intrusive-completion contract), which
    // clippy's dataflow can't see — the reference is `while (!called) try
    // loop.run(.no_wait);`.
    #[allow(clippy::while_immutable_condition)]
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

// PORT NOTE: the shared `v.* = r.timer catch unreachable; return .disarm;`
// callback the "timer reset"/"timer remove" tests use.
fn trigger_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
    let v = ud as *mut Option<TimerTrigger>;
    match r {
        Result::Timer(Ok(t)) => unsafe { *v = Some(t) },
        _ => unreachable!(),
    }
    CallbackAction::Disarm
}

// test "io_uring: timer reset" (io_uring.zig:1272)
#[test]
fn io_uring_timer_reset() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    let cb: Callback = trigger_cb;

    // Add the timer
    let mut trigger: Option<TimerTrigger> = None;
    let mut c1 = Completion::default();
    unsafe {
        loop_.timer(
            ptr::addr_of_mut!(c1),
            100_000,
            ptr::addr_of_mut!(trigger).cast::<c_void>(),
            cb,
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
            cb,
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

// test "io_uring: timer remove" (io_uring.zig:1357)
#[test]
fn io_uring_timer_remove() {
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

    // Remove it
    fn timer_remove_cb(
        _ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        // _ = r.timer_remove catch unreachable;
        match r {
            Result::TimerRemove(Ok(())) => {}
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    let mut c_remove = Completion {
        op: Operation::TimerRemove {
            timer: ptr::addr_of_mut!(c1),
        },
        userdata: ptr::null_mut(),
        callback: timer_remove_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_remove)) };

    // Tick
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(trigger.unwrap() == TimerTrigger::Cancel);

    loop_.deinit();
}

pub(super) fn write_slice(buf: &[u8]) -> WriteBuffer {
    WriteBuffer::Slice(ptr::slice_from_raw_parts(buf.as_ptr(), buf.len()))
}
pub(super) fn read_slice(buf: &mut [u8]) -> ReadBuffer {
    ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(buf.as_mut_ptr(), buf.len()))
}

mod net;

mod process_pty;

mod async_file;
