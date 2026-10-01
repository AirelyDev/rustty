//! Tests for the epoll backend.
//!
//! PORTS: libxev src/backend/epoll.zig — the `test{}` blocks. The socket,
//! process/pty and async/file tests live in the `tests/` children.

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

/// PORT-ADDED (N431): a callback re-enters the loop through a pointer the
/// caller stored (termio's `td.loop_` -> `stream.rs` `(*loop_).add`), not
/// through its `l`, while the loop runs through `run_raw` with that same
/// pointer. The second timer is armed from inside the first callback and
/// stops the loop, so both callbacks must have run for `until_done` to
/// return.
// Under Miri (`cargo +nightly miri test -p rustty-xev --target
// aarch64-unknown-linux-gnu`, the epoll/eventfd shims) this is the real-code
// twin of mirihar-n431 `raw_local_root`: clean under SB and TB; entering
// through `(*root).run(..)` instead is UB (`wrapper_local_root`).
#[test]
fn epoll_callback_reenters_through_stored_loop_pointer() {
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
fn trigger_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
    let v = ud as *mut Option<TimerTrigger>;
    // v.* = r.timer catch unreachable;
    match r {
        Result::Timer(Ok(t)) => unsafe { *v = Some(t) },
        _ => unreachable!(),
    }
    CallbackAction::Disarm
}

// Sets `*ud = true` on any result. Used by the timer/stop tests.
fn set_true_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, _r: Result) -> CallbackAction {
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

fn read_ok_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
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

pub(super) fn write_slice(buf: &[u8]) -> WriteBuffer {
    WriteBuffer::Slice(ptr::slice_from_raw_parts(buf.as_ptr(), buf.len()))
}
pub(super) fn read_slice(buf: &mut [u8]) -> ReadBuffer {
    ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(buf.as_mut_ptr(), buf.len()))
}

mod net;

mod process_pty;

mod async_file;
