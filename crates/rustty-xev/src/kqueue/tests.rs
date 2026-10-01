//! Tests for the kqueue backend.
//!
//! PORTS: libxev src/backend/kqueue.zig — the `test{}` blocks.

use super::*;
use core::ffi::c_void;

// Serialize the two socket tests: both bind the fixed loopback port 3131
// (faithful to the reference), which cargo's parallel test runner would race
// (EADDRINUSE) — the reference's Zig test runner is single-threaded.
static SOCKET_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// -----------------------------------------------------------------------
// W2D/F-004: the per-target Kevent ABI adaptation, proved on BOTH targets.
//
// These are rustty-added (the reference needs no such test: Zig's
// `kevent_init` freebsd arm is a literal identity, and `posix.Kevent.udata`
// is a `usize` on every platform, so there is nothing to get wrong). In the
// port the two arms are hand-written per-target field copies, which IS
// something to get wrong — a dropped field or a truncating cast would be
// silent. They RUN on the host for the macOS arm; on FreeBSD they compile
// today (`cargo check --target x86_64-unknown-freebsd --all-targets`) and run
// with the rest of the suite on a real FreeBSD host (DEFERRALS.md row).
// -----------------------------------------------------------------------

/// `kevent_init` must carry EVERY `KeventInit` field through to the target's
/// `Kevent`, with no truncation. The sentinels are deliberately extreme:
/// `ident`/`udata` use the full 64-bit width (on FreeBSD they land in
/// `uintptr_t`/`*mut c_void`), `data` uses `i64::MIN` (the field is `i64` on
/// both ABIs — it must NOT be routed through the `udata` accessors), and
/// `filter`/`flags`/`fflags` use their type minima/maxima.
#[test]
fn kevent_init_carries_every_field() {
    let k = kevent_init(KeventInit {
        ident: 0xDEAD_BEEF_CAFE_1234,
        filter: i16::MIN,
        flags: u16::MAX,
        fflags: 0xF0F0_0F0F,
        data: i64::MIN,
        udata: 0x1234_5678_9ABC_DEF0,
    });
    assert_eq!(k.ident as u64, 0xDEAD_BEEF_CAFE_1234);
    assert_eq!(k.filter, i16::MIN);
    assert_eq!(k.flags, u16::MAX);
    assert_eq!(k.fflags, 0xF0F0_0F0F);
    assert_eq!(k.data, i64::MIN);
    assert_eq!(kev_udata(&k), 0x1234_5678_9ABC_DEF0);
}

/// The `udata` accessor pair must round-trip a full 64-bit value and must
/// zero exactly the way the two disarm sites need (`events[changes].udata = 0`
/// in the reference — the sentinel `tick`/`tick_inner` test with `== 0`).
#[test]
fn kev_udata_accessors_round_trip() {
    let mut k = kevent_init(KeventInit {
        ident: 1,
        filter: libc::EVFILT_READ,
        flags: libc::EV_ADD,
        fflags: 0,
        data: 0,
        udata: 0,
    });
    assert_eq!(kev_udata(&k), 0, "a fresh kevent's udata is the 0 sentinel");

    // A realistic completion pointer must survive both directions.
    let mut c = Completion::default();
    let p = ptr::addr_of_mut!(c) as u64;
    kev_set_udata(&mut k, p);
    assert_eq!(kev_udata(&k), p);
    assert_eq!(kev_udata(&k) as *mut Completion, ptr::addr_of_mut!(c));

    // The disarm sites zero it back; the tick loops then skip the event.
    kev_set_udata(&mut k, 0);
    assert_eq!(kev_udata(&k), 0);
}

/// S-13 (UD275, N255): the non-Darwin `Wakeup` is the reference's own
/// `@panic("wakeup not implemented on this platform")` stub
/// (`kqueue.zig:1488-1510`), kept faithful because no caller reaches it (see
/// the PORT NOTE on the FreeBSD `Wakeup`). This pins the ported shape: `init`
/// and `setup` succeed and register nothing, and `wakeup` is the panic, so a
/// future caller that starts reaching it fails here on a FreeBSD host
/// instead of in the field. rustty-added (the reference has no such test).
#[cfg(target_os = "freebsd")]
#[test]
#[should_panic(expected = "wakeup not implemented on this platform")]
fn freebsd_wakeup_stub_is_the_reference_panic() {
    let mut w = Wakeup::init().expect("the FreeBSD Wakeup::init is infallible");
    let kq = unsafe { libc::kqueue() };
    assert!(kq >= 0, "kqueue() failed");
    w.setup(kq)
        .expect("the FreeBSD Wakeup::setup registers nothing");
    unsafe { libc::close(kq) };
    let _ = w.wakeup();
}

fn read_slice(buf: &mut [u8]) -> ReadBuffer {
    ReadBuffer::Slice(ptr::slice_from_raw_parts_mut(buf.as_mut_ptr(), buf.len()))
}
fn write_slice(buf: &[u8]) -> WriteBuffer {
    WriteBuffer::Slice(ptr::slice_from_raw_parts(buf.as_ptr(), buf.len()))
}

// PORT NOTE: `posix.socket(family, ty, 0)` — macOS emulates SOCK.CLOEXEC/NONBLOCK
// via fcntl (the Zig std.posix.socket behavior).
unsafe fn new_socket(family: libc::c_int, nonblock: bool) -> socket_t {
    let fd = unsafe { libc::socket(family, libc::SOCK_STREAM, 0) };
    assert!(fd >= 0);
    // SOCK.CLOEXEC
    unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    if nonblock {
        let fl = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        unsafe { libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK) };
    }
    fd
}

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_loop_time() {
    let mut loop_ = Loop::init(Options::default()).unwrap();
    // defer loop.deinit();

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
// Real-code twin of mirihar-n431 `raw_local_root` (the backend's syscalls
// keep it out of Miri; the epoll twin runs under Miri on the Linux target).
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
#[test]
fn kqueue_callback_reenters_through_stored_loop_pointer() {
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

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_stop() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Add the timer
    let mut called = false;
    let mut c1 = Completion::default();
    fn callback(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, _r: Result) -> CallbackAction {
        let b = ud as *mut bool;
        unsafe { *b = true };
        CallbackAction::Disarm
    }
    unsafe {
        loop_.timer(
            ptr::addr_of_mut!(c1),
            1_000_000,
            ptr::addr_of_mut!(called).cast::<c_void>(),
            callback,
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

// `called` is mutated only through the raw userdata pointer inside the timer
// callback (the intrusive-completion contract), which clippy's dataflow can't
// see — the reference is `while (!called) try loop.run(.no_wait)`.
#[allow(clippy::while_immutable_condition)]
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_timer() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Add the timer
    let mut called = false;
    let mut c1 = Completion::default();
    fn callback(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, _r: Result) -> CallbackAction {
        let b = ud as *mut bool;
        unsafe { *b = true };
        CallbackAction::Disarm
    }
    unsafe {
        loop_.timer(
            ptr::addr_of_mut!(c1),
            1,
            ptr::addr_of_mut!(called).cast::<c_void>(),
            callback,
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
            callback,
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

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_timer_reset() {
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

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_timer_reset_before_tick() {
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

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_timer_reset_after_trigger() {
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

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_timer_cancellation() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Add the timer
    let mut trigger: Option<TimerTrigger> = None;
    let mut c1 = Completion::default();
    fn timer_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        let ptr = ud as *mut Option<TimerTrigger>;
        match r {
            Result::Timer(Ok(t)) => unsafe { *ptr = Some(t) },
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    unsafe {
        loop_.timer(
            ptr::addr_of_mut!(c1),
            100_000,
            ptr::addr_of_mut!(trigger).cast::<c_void>(),
            timer_cb,
        );
    }

    // Tick and verify we're not called.
    loop_.run(RunMode::NoWait).unwrap();
    assert!(trigger.is_none());

    // Cancel the timer
    let mut called = false;
    fn cancel_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        match r {
            Result::Cancel(Ok(())) => {}
            _ => unreachable!(),
        }
        let ptr = ud as *mut bool;
        unsafe { *ptr = true };
        CallbackAction::Disarm
    }
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

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_canceling_a_completed_operation() {
    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Add the timer
    let mut trigger: Option<TimerTrigger> = None;
    let mut c1 = Completion::default();
    fn timer_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        let ptr = ud as *mut Option<TimerTrigger>;
        match r {
            Result::Timer(Ok(t)) => unsafe { *ptr = Some(t) },
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    unsafe {
        loop_.timer(
            ptr::addr_of_mut!(c1),
            1,
            ptr::addr_of_mut!(trigger).cast::<c_void>(),
            timer_cb,
        );
    }

    // Tick and verify we're not called.
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(trigger.unwrap() == TimerTrigger::Expiration);

    // Cancel the timer
    let mut called = false;
    fn cancel_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        match r {
            Result::Cancel(Ok(())) => {}
            _ => unreachable!(),
        }
        let ptr = ud as *mut bool;
        unsafe { *ptr = true };
        CallbackAction::Disarm
    }
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

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_socket_accept_connect_send_recv_close() {
    let _guard = SOCKET_TEST_LOCK.lock().unwrap();
    let mut loop_ = Loop::init(Options::default()).unwrap();

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
            size_of::<libc::c_int>() as socklen_t,
        );
        // PORT NOTE (cargo-parallel + rapid-rerun stability adaptation):
        // add SO_REUSEPORT alongside the reference's SO_REUSEADDR. The
        // reference binds the fixed port 3131 once per single-threaded
        // `zig test` run; under cargo's rapid sequential re-runs a prior
        // process can leave a 3131 socket mid-teardown (macOS TIME_WAIT /
        // leaked accepted-fd lifecycle) that SO_REUSEADDR alone does not
        // fully cover on Darwin, yielding intermittent EADDRINUSE at bind.
        // SO_REUSEPORT closes that residual rebind race. Port 3131 and all
        // observable behavior are unchanged — same family of adaptation as
        // the SOCKET_TEST_LOCK here and the onig_initialize/OnceLock guard.
        libc::setsockopt(
            ln,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            ptr::addr_of!(one).cast(),
            size_of::<libc::c_int>() as socklen_t,
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
    fn accept_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
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
            addr_size: size_of::<net::sockaddr::Any>() as socklen_t,
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
        let b = ud as *mut bool;
        unsafe { *b = true };
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
    fn send_cb(_ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
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
    fn recv_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
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
        let ptr = ud as *mut bool;
        unsafe { *ptr = true };
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
    fn eof_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
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
    fn close_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        match r {
            Result::Close(Ok(())) => {}
            _ => unreachable!(),
        }
        let ptr = ud as *mut socket_t;
        unsafe { *ptr = 0 };
        CallbackAction::Disarm
    }
    let mut c_client_close = Completion {
        op: Operation::Close { fd: client_conn },
        userdata: ptr::addr_of_mut!(client_conn).cast::<c_void>(),
        callback: close_cb,
        ..Completion::default()
    };
    // NOTE: client_conn is captured by the op above; the close cb zeroes the
    // userdata-pointed var. We reuse a mutable binding, matching the reference.
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

    loop_.deinit();
}

#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
fn kqueue_socket_accept_cancel_decrements_active() {
    let _guard = SOCKET_TEST_LOCK.lock().unwrap();

    //if (true) return error.SkipZigTest;

    let mut loop_ = Loop::init(Options::default()).unwrap();

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
            size_of::<libc::c_int>() as socklen_t,
        );
        // PORT NOTE (cargo-parallel + rapid-rerun stability adaptation):
        // add SO_REUSEPORT alongside the reference's SO_REUSEADDR. The
        // reference binds the fixed port 3131 once per single-threaded
        // `zig test` run; under cargo's rapid sequential re-runs a prior
        // process can leave a 3131 socket mid-teardown (macOS TIME_WAIT /
        // leaked accepted-fd lifecycle) that SO_REUSEADDR alone does not
        // fully cover on Darwin, yielding intermittent EADDRINUSE at bind.
        // SO_REUSEPORT closes that residual rebind race. Port 3131 and all
        // observable behavior are unchanged — same family of adaptation as
        // the SOCKET_TEST_LOCK here and the onig_initialize/OnceLock guard.
        libc::setsockopt(
            ln,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            ptr::addr_of!(one).cast(),
            size_of::<libc::c_int>() as socklen_t,
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

    // Accept
    let mut server_conn: socket_t = 0;
    fn accept_cb(
        _ud: *mut c_void,
        _l: *mut Loop,
        _c: *mut Completion,
        r: Result,
    ) -> CallbackAction {
        match r {
            Result::Accept(Err(AcceptError::Canceled)) => {}
            Result::Accept(_) => panic!("wrong"),
            _ => unreachable!(),
        }
        CallbackAction::Disarm
    }
    let mut c_accept = Completion {
        op: Operation::Accept {
            socket: ln,
            addr: unsafe { core::mem::zeroed() },
            addr_size: size_of::<net::sockaddr::Any>() as socklen_t,
            flags: SOCK_CLOEXEC,
        },
        userdata: ptr::addr_of_mut!(server_conn).cast::<c_void>(),
        callback: accept_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_accept)) };
    loop_.run(RunMode::NoWait).unwrap();
    assert_eq!(1usize, loop_.active);

    let mut cancel_called = false;
    fn cancel_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        match r {
            Result::Cancel(Ok(())) => {}
            _ => unreachable!(),
        }
        let ptr = ud as *mut bool;
        unsafe { *ptr = true };
        CallbackAction::Disarm
    }
    let mut c_cancel = Completion {
        op: Operation::Cancel {
            c: ptr::addr_of_mut!(c_accept),
        },
        userdata: ptr::addr_of_mut!(cancel_called).cast::<c_void>(),
        callback: cancel_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_cancel)) };

    assert_eq!(2usize, loop_.active);
    loop_.run(RunMode::Once).unwrap();
    assert!(cancel_called);

    // Both callbacks are called active count should be 0
    assert_eq!(0usize, loop_.active);

    fn close_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        match r {
            Result::Close(Ok(())) => {}
            _ => unreachable!(),
        }
        let ptr = ud as *mut socket_t;
        unsafe { *ptr = 0 };
        CallbackAction::Disarm
    }
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

    loop_.deinit();
}

// PORT NOTE (deferred skip-stub, XEV_BLUEPRINT §7): "file IO on thread pool" builds
// a real ThreadPool and asserts thread-pool file IO round-trips. The ThreadPool
// work-stealing BODY is deferred (§1/§6, DEFERRALS.md), so this is a
// SkipZigTest-style skip-stub — the `#[test]` exists so the R1 count holds and
// the reference comments are preserved; a later chunk activates the body. The
// reference (`if (builtin.os.tag != .macos) return error.SkipZigTest;` then):
//   // Create our file
//   // Perform a write and then a read
//   // Wait for the write
//   // Make sure the data is on disk
//   // Read
//   // Wait for the send/receive
//
// HONEST-SKIP CONTRACT (VALIDATION_REPORT §3e; R5 forbids an #[ignore]): the
// marker const below is flipped to `true` by the chunk that lands the real
// blocker (the `TMUX_DCS_WIRED` precedent), and while it is `false` the test
// body FAILS LOUDLY unless the deferral is still tracked in the DEFERRALS
// ledger — the skip can never silently outlive its ledger entry.
//
// MARKER CORRECTED (deferral re-audit, 2026-07-31): this const was named
// `THREAD_POOL_BODY_PORTED` and its prose blamed "the Zap pool + `xev.File`
// watcher". The Zap work-stealing pool HAS since landed in full
// (`crates/rustty-xev/src/thread_pool.rs`, 0 `unimplemented!()`), so the old
// name asserted a blocker that no longer exists and would have read as
// "pool unported" to anyone grepping. The SOLE remaining blocker is the
// macOS `xev.File` watcher, which kqueue never instantiates (Linux ports
// `File`/`TCP`/`UDP`; macOS does not) — this test cannot open a file
// watcher to drive IO through the pool.
/// Flipped to `true` by the chunk that instantiates the macOS `xev.File`
/// watcher, which must then replace the ledger-tracking arm with the live
/// reference body above.
const MACOS_FILE_WATCHER_PORTED: bool = false;

// NO MIRI EXCEPTION. This test carried the C3 live-syscall annotation until
// the 2026-07-31 adversarial re-audit, which found the reason string false
// for THIS test: while `MACOS_FILE_WATCHER_PORTED == false` the body makes no
// syscall at all -- it is a deferral-ledger skip-stub (`include_str!` +
// `contains`). Miri runs it clean in ~24s. The annotation had been applied by
// class/file, not by body. If the watcher is ever ported and the flag flipped,
// the real body WILL make live kqueue/kevent64 calls and the annotation must
// come back (see the sibling tests in this module for the exact string).
//
// R14 (G-SKIP, task S-15): this test used to RETURN `ok` while
// `MACOS_FILE_WATCHER_PORTED == false`, having asserted only that the
// deferral was ledgered — a reference test reported as passing with none of
// its body run. It is now `#[ignore]`d and FAILS when asked for, naming the
// missing port; the ledger tripwire it carried is its own always-run test,
// `kqueue_file_watcher_deferral_is_ledgered` below.
#[test]
#[ignore = "the macOS xev.File watcher is not ported (.port/phase-2/DEFERRALS.md); \
            this reference test cannot run until it is (R14)"]
fn kqueue_file_io_on_thread_pool() {
    // Deferred: the macOS `xev.File` watcher. The Zap work-stealing pool
    // itself IS ported (`thread_pool.rs`), and so are
    // `ThreadPool.init/deinit/shutdown/schedule` +
    // `Loop.init(.{.thread_pool})`; what is missing is the macOS
    // `File`/`TCP`/`UDP` watcher instantiation this test needs to drive
    // IO through the pool.
    assert!(
        MACOS_FILE_WATCHER_PORTED,
        "`kqueue: file IO on thread pool` cannot run: the macOS `xev.File` \
         watcher is not ported (MACOS_FILE_WATCHER_PORTED == false)"
    );
    unreachable!(
        "MACOS_FILE_WATCHER_PORTED flipped without porting the reference \
         `kqueue: file IO on thread pool` body into this test"
    );
}

/// rustty-only tripwire (not a reference `test {}`), split out of
/// `kqueue_file_io_on_thread_pool` by task S-15: while the watcher is
/// unported, its deferral must stay ledgered, so the ignored test above
/// stays visible (compile-time probe: a removed ledger entry fails this
/// test on the next build).
///
/// PORT NOTE (parity S63-F8): this was `include_str!` of
/// `.port/phase-2/DEFERRALS.md`, which HARD-FAILS to compile where that
/// file does not exist — i.e. in the source tarball, which deliberately
/// excludes `.port/`. The shipped tarball therefore could not build its own
/// tests, and `xtask distcheck` could not see it because it only BUILT the
/// extracted tree. build.rs now probes the ledger and reports both "is there
/// a ledger here at all" and "does it still carry the entry", so the
/// tripwire is unchanged in the repo and simply has nothing to check in a
/// tarball.
#[test]
fn kqueue_file_watcher_deferral_is_ledgered() {
    let ledger_present = env!("RUSTTY_XEV_DEFERRALS_PRESENT") == "1";
    let ledgered = env!("RUSTTY_XEV_DEFERRAL_LEDGERED") == "1";
    assert!(
        MACOS_FILE_WATCHER_PORTED || !ledger_present || ledgered,
        "the macOS `xev.File` watcher deferral vanished from DEFERRALS.md \
         but `kqueue_file_io_on_thread_pool` is still unported: port the \
         watcher (and flip MACOS_FILE_WATCHER_PORTED) or restore the ledger \
         entry"
    );
}

// `libc::mach_task_self()` is `#[deprecated]` in libc (see async.rs PORT NOTE).
#[allow(deprecated)]
// PORT NOTE (W2D): `if (builtin.os.tag != .macos) return error.SkipZigTest;`
// (kqueue.zig:2662) — the reference's own per-test OS guard, ported as the cfg
// now that this module also compiles for FreeBSD. Zig's runtime skip has no
// Rust equivalent that also stops the mach symbols being NAMED, so it is a cfg.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "visionos"))]
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-xev C3, .port/phase-2/MIRI_EXCEPTIONS.md): live kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions; runs natively (ci.sh HARD `cargo test --workspace`) + the G6 ASan companion arm (check_leaks.sh `live_ffi_unit_filters`, re-verified 2026-07-31: 31 tests, 0 findings)."
)]
#[cfg_attr(
    not(target_os = "macos"),
    ignore = "SkipZigTest: upstream runs `kqueue: mach port` on .macos only (R14)"
)]
fn kqueue_mach_port() {
    // if (builtin.os.tag != .macos) return error.SkipZigTest;

    let mut loop_ = Loop::init(Options::default()).unwrap();

    // Allocate the port
    let mach_self = unsafe { libc::mach_task_self() };
    let mut mach_port: darwin::mach_port_name_t = 0;
    assert_eq!(
        darwin::KernE::SUCCESS,
        darwin::getKernError(unsafe {
            darwin::mach_port_allocate(
                mach_self,
                darwin::MACH_PORT_RIGHT::RECEIVE.0,
                &mut mach_port,
            )
        }),
    );
    // defer _ = posix.system.mach_port_deallocate(mach_self, mach_port);

    // Add the waiter
    let mut called = false;
    fn wait_cb(ud: *mut c_void, _l: *mut Loop, _c: *mut Completion, r: Result) -> CallbackAction {
        match r {
            Result::Machport(Ok(())) => {}
            _ => unreachable!(),
        }
        let b = ud as *mut bool;
        unsafe { *b = true };
        CallbackAction::Disarm
    }
    let mut c_wait = Completion {
        op: Operation::Machport {
            port: mach_port,
            buffer: ReadBuffer::Array(unsafe { core::mem::zeroed() }),
        },
        userdata: ptr::addr_of_mut!(called).cast::<c_void>(),
        callback: wait_cb,
        ..Completion::default()
    };
    unsafe { loop_.add(ptr::addr_of_mut!(c_wait)) };

    // Tick so we submit... should not call since we never sent.
    loop_.run(RunMode::NoWait).unwrap();
    assert!(!called);

    // Send a message to the port
    let mut msg = darwin::mach_msg_header_t {
        msgh_bits: darwin::MACH_MSG_TYPE::MAKE_SEND_ONCE.0,
        msgh_size: size_of::<darwin::mach_msg_header_t>() as darwin::mach_msg_size_t,
        msgh_remote_port: mach_port,
        msgh_local_port: darwin::MACH_PORT_NULL,
        // msgh_voucher_port / msgh_id are `undefined` in the reference.
        msgh_voucher_port: 0,
        msgh_id: 0,
    };
    assert_eq!(
        darwin::MachMsgE::SUCCESS,
        darwin::getMachMsgError(unsafe {
            darwin::mach_msg(
                &mut msg,
                darwin::MACH_SEND_MSG,
                msg.msgh_size,
                0,
                darwin::MACH_PORT_NULL,
                darwin::MACH_MSG_TIMEOUT_NONE,
                darwin::MACH_PORT_NULL,
            )
        }),
    );

    // We should receive now!
    loop_.run(RunMode::UntilDone).unwrap();
    assert!(called);

    // We should not receive again
    called = false;
    unsafe { loop_.add(ptr::addr_of_mut!(c_wait)) };

    // Tick so we submit... should not call since we never sent.
    loop_.run(RunMode::NoWait).unwrap();
    assert!(!called);

    unsafe { darwin::mach_port_deallocate(mach_self, mach_port) };
    loop_.deinit();
}
