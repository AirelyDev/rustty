//! PORTS: ghostty src/termio/Exec.zig — the `threadEnter`/`processExit` KATs.

// (Every test here is gated off Windows, so the glob is unused there.)
#[cfg_attr(windows, allow(unused_imports))]
use super::*;

/// PORT NOTE: reference defect (R13), UD367. The process-info accessors
/// answer `None` while the io thread's `start` has written the pty but not
/// yet published it (`Exec::pty_ready`), and answer from the pty once it
/// is published. On the reference's shape (no flag, `getProcessInfo`
/// reads `self.pty` directly) the first assertions fail: the main thread
/// sees the pty mid-publication.
#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[cfg_attr(miri, ignore)] // opens a real pty
fn ud367_process_info_waits_for_thread_enter_to_publish_the_pty() {
    use allocator_api2::alloc::Global;

    let cfg = Config {
        command: Some(configpkg::command::Command::Shell(b"exit 0".to_vec())),
        env: EnvMap::new(),
        env_override: configpkg::RepeatableStringMap::default(),
        shell_integration: configpkg::value_types::ShellIntegration::None,
        shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
        cursor_blink: None,
        working_directory: None,
        resources_dir: None,
        term: b"xterm-256color".to_vec(),
        rt_pre_exec_info: RtPreExecInfo::default(),
        rt_post_fork_info: RtPostForkInfo::default(),
    };
    let mut exec = Exec::init(Global, cfg).expect("Exec::init");
    let exec_ptr: *mut Exec = &mut exec;

    // Before `start`: nothing to report (the reference agrees).
    assert!(unsafe { Exec::getTtyName(exec_ptr) }.is_none());
    assert!(unsafe { Exec::getForegroundPid(exec_ptr) }.is_none());

    // `start` has written the pty (the part of `Subprocess.start` the
    // main thread races), but `threadEnter` has not published it yet.
    let ws = ptypkg::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 800,
        ws_ypixel: 480,
    };
    exec.subprocess.pty = Some(ptypkg::Pty::open(ws).expect("Pty::open"));
    assert!(
        unsafe { Exec::getTtyName(exec_ptr) }.is_none(),
        "the tty name must wait for threadEnter to publish the pty"
    );
    assert!(
        unsafe { Exec::getForegroundPid(exec_ptr) }.is_none(),
        "the foreground pid must wait for threadEnter to publish the pty"
    );

    // Published (`threadEnter`'s `Release` store after `start`): the
    // accessors answer from the pty, as the reference does after `start`.
    exec.pty_ready
        .store(true, core::sync::atomic::Ordering::Release);
    let name = unsafe { Exec::getTtyName(exec_ptr) }
        .expect("the tty name once the pty is published")
        .to_bytes()
        .to_vec();
    assert!(name.starts_with(b"/dev/"), "tty name {name:?}");

    exec.deinit();
    assert!(!exec.pty_ready.load(core::sync::atomic::Ordering::Relaxed));
}

// KAT (rustty-only margin — blueprint §7 B2; NOT a mapped Zig `test{}`, so the
// test-count contract is unaffected). Drives the xev write path end-to-end and
// (now that B3 wires the REAL read loop) the full round trip:
//   Exec::init(/bin/cat) -> threadEnter (real fork/exec on a pty + xev
//   Process::wait / Stream / Timer wiring + a REAL read-thread spawn) ->
//   queueWrite a marker -> bounded loop.run(NoWait) pump -> the read thread
//   reads cat's echo off the pty master and `Termio::processOutput` lands it in
//   the terminal `Screen` -> assert the Screen shows the marker -> threadExit
//   reaps + joins.
//
// Real fork/exec + openpty + kqueue syscalls -> Miri-excluded
// (MIRI_EXCEPTIONS.md, "rustty-termio B2"). Bounded (NoWait passes + a
// wall-clock deadline + a tick cap + explicit teardown), so it never blocks on
// the child's life. The `io: *mut Termio` handed to threadEnter is now a REAL,
// heap-pinned `Termio` (B3's read thread dereferences it for `processOutput`;
// the B2 read-thread stub that ignored it is gone). We assert on the Screen
// (the sink the read thread commits to) rather than reading the master directly
// — the real read thread is the master's sole reader now.
#[test]
#[cfg(target_os = "macos")]
fn threadenter_queuewrite_writes_to_pty() {
    use allocator_api2::alloc::Global;
    use std::time::{Duration, Instant};

    // Serialize with the other fork/pty KATs (see FORK_KAT_SERIAL).
    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    // Config: run `/bin/cat` directly (a deterministic pty passthrough), no
    // resources dir (shell integration disabled), xterm-256color.
    let cfg = Config {
        command: Some(configpkg::command::Command::Direct(vec![
            b"/bin/cat".to_vec()
        ])),
        env: EnvMap::new(),
        env_override: configpkg::RepeatableStringMap::default(),
        shell_integration: configpkg::value_types::ShellIntegration::Detect,
        shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
        cursor_blink: None,
        working_directory: None,
        resources_dir: None,
        term: b"xterm-256color".to_vec(),
        rt_pre_exec_info: RtPreExecInfo::default(),
        rt_post_fork_info: RtPostForkInfo::default(),
    };
    let mut exec = Exec::init(Global, cfg).expect("Exec::init");

    // The event loop (created here, driven here — the KAT stands in for the
    // io thread's `threadMain_` frame).
    let mut loop_ = xev::Loop::init(xev::Options::default()).expect("Loop::init");
    let loop_ptr: *mut xev::Loop = std::ptr::addr_of_mut!(loop_);

    // A REAL, heap-pinned `Termio<TestMailbox>` for the `io` arg: the read
    // thread now dereferences it via `processOutput` and deposits cat's echo
    // into its terminal `Screen`.
    let t = TestTermio::build();
    let io_ptr = t.io;

    // The frame-pinned ThreadData. `backend` is a placeholder that
    // `threadEnter` overwrites (the reference's `backend = undefined`
    // out-parameter protocol); it holds a trivial already-finished thread +
    // inert fds so its drop (on the overwrite) is a no-op. `renderer_state`
    // points at the harness State (unused by cat's write path, but valid).
    let mut td: crate::termio_impl::ThreadData<TestMailbox> = crate::termio_impl::ThreadData {
        alloc: Global,
        loop_: loop_ptr,
        renderer_state: t.state,
        surface_mailbox: TestMailbox,
        backend: crate::backend::ThreadData::Exec(ThreadData {
            start: Instant::now(),
            exited: false,
            write_stream: xev::Stream::init_fd(-1),
            process: None,
            write_pool: MemoryPool::init(Global),
            write_queue: xev::WriteQueue::default(),
            process_wait_c: xev::Completion::default(),
            flatpak_wait_c: FlatpakHostCommandCompletion::default(),
            read_thread: std::thread::spawn(|| {}),
            read_thread_pipe: -1,
            read_thread_fd: -1,
            termios_timer: xev::Timer::init().expect("placeholder Timer::init"),
            termios_timer_c: xev::Completion::default(),
            termios_timer_running: false,
            termios_mode: ptypkg::Mode::default(),
        }),
        mailbox: std::ptr::null_mut(),
    };

    // Start the subprocess + wire the xev handles (+ spawn the REAL read
    // thread). SAFETY: `io_ptr` outlives the read thread (joined in threadExit);
    // `td` is frame-pinned for the loop's life.
    unsafe { Exec::threadEnter(&mut exec, Global, io_ptr, &mut td) }.expect("threadEnter");

    // The pty master (parent side; both read + write ends of PtyFds). The read
    // thread is its sole reader now; we only keep it to close at teardown.
    let master: std::os::fd::RawFd = {
        let crate::backend::ThreadData::Exec(e) = &td.backend;
        e.read_thread_fd
    };

    // Queue a marker write (goes: direct queueWrite -> chunk into the write
    // pools -> Stream::queue_write -> loop). cat echoes it back on the pty; the
    // read thread reads it and `processOutput` commits it to the Screen.
    const MARK: &[u8] = b"RUSTTY_B2_MARK\n";
    Exec::queueWrite(&mut exec, Global, &mut td, MARK, false).expect("queueWrite");

    // Bounded pump: NoWait ticks (to flush the write) + poll the Screen + a
    // wall-clock deadline.
    let needle = b"RUSTTY_B2_MARK";
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut found = false;
    for _ in 0..2000 {
        // N431: through `loop_ptr`, the pointer `td.loop_` re-enters through.
        unsafe { xev::Loop::run_raw(loop_ptr, xev::RunMode::NoWait) }.expect("loop.run(NoWait)");

        let dump = t.screen_dump();
        if dump.windows(needle.len()).any(|w| w == needle) {
            found = true;
            break;
        }
        if Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(
        found,
        "queued write did not round-trip the pty into the Screen within the deadline; screen saw: {:?}",
        String::from_utf8_lossy(&t.screen_dump())
    );

    // Teardown: reap cat + write the quit pipe + join the read thread (so
    // nothing dereferences `io` after this).
    unsafe { Exec::threadExit(&mut exec, &mut td) };
    // Close the pty master (never closed by deinit) and free the ThreadData.
    unsafe {
        libc::close(master);
    }
    td.deinit();
    loop_.deinit();
    exec.deinit();
    // The read thread is joined; deinit the harness Termio last.
    t.teardown();
}

/// S211-F5 / UD102: drive one of `threadEnter`'s error arms for real, the
/// read-thread spawn failure (a thread that cannot get its stack), and
/// check that it drains the pty master before `subprocess.stop()` and
/// fails `threadEnter`. The reference's arm is the bare `errdefer
/// self.subprocess.stop()`; the drain is the port's R13 handling. The child
/// is a real `/bin/sh` that writes to the pty and keeps running.
#[test]
#[cfg(target_os = "macos")]
fn threadenter_read_thread_spawn_failure_drains_before_stop() {
    use allocator_api2::alloc::Global;
    use std::time::Instant;

    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let cfg = Config {
        command: Some(configpkg::command::Command::Direct(vec![
            b"/bin/sh".to_vec(),
            b"-c".to_vec(),
            b"printf PENDING-OUTPUT; exec /bin/sleep 5".to_vec(),
        ])),
        env: EnvMap::new(),
        env_override: configpkg::RepeatableStringMap::default(),
        shell_integration: configpkg::value_types::ShellIntegration::Detect,
        shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
        cursor_blink: None,
        working_directory: None,
        resources_dir: None,
        term: b"xterm-256color".to_vec(),
        rt_pre_exec_info: RtPreExecInfo::default(),
        rt_post_fork_info: RtPostForkInfo::default(),
    };
    let mut exec = Exec::init(Global, cfg).expect("Exec::init");
    let mut loop_ = xev::Loop::init(xev::Options::default()).expect("Loop::init");
    let loop_ptr: *mut xev::Loop = std::ptr::addr_of_mut!(loop_);
    let t = TestTermio::build();
    let mut td: crate::termio_impl::ThreadData<TestMailbox> = crate::termio_impl::ThreadData {
        alloc: Global,
        loop_: loop_ptr,
        renderer_state: t.state,
        surface_mailbox: TestMailbox,
        backend: crate::backend::ThreadData::Exec(ThreadData {
            start: Instant::now(),
            exited: false,
            write_stream: xev::Stream::init_fd(-1),
            process: None,
            write_pool: MemoryPool::init(Global),
            write_queue: xev::WriteQueue::default(),
            process_wait_c: xev::Completion::default(),
            flatpak_wait_c: FlatpakHostCommandCompletion::default(),
            read_thread: std::thread::spawn(|| {}),
            read_thread_pipe: -1,
            read_thread_fd: -1,
            termios_timer: xev::Timer::init().expect("placeholder Timer::init"),
            termios_timer_c: xev::Completion::default(),
            termios_timer_running: false,
            termios_mode: ptypkg::Mode::default(),
        }),
        mailbox: std::ptr::null_mut(),
    };

    DRAIN_CALLS.with(|c| c.set(0));
    STOP_CALLS.with(|c| c.set(0));
    STOPS_BEFORE_DRAIN.with(|c| c.set(None));
    THREAD_ENTER_FAILURE.with(|f| f.borrow_mut().clear());
    FAIL_READ_THREAD_SPAWN.with(|f| f.set(true));
    let result = unsafe { Exec::threadEnter(&mut exec, Global, t.io, &mut td) };
    FAIL_READ_THREAD_SPAWN.with(|f| f.set(false));

    assert!(
        result.is_err(),
        "a failed read-thread spawn fails threadEnter"
    );
    // N354: the test-only diagnostic names the step that failed.
    let steps = THREAD_ENTER_FAILURE.with(|f| f.borrow().clone());
    assert_eq!(steps.len(), 1, "one failed step is recorded: {steps:?}");
    assert!(
        steps[0].starts_with("threadEnter: std.Thread.spawn: "),
        "the failed step is the read-thread spawn: {steps:?}"
    );
    assert_eq!(
        DRAIN_CALLS.with(std::cell::Cell::get),
        1,
        "the error arm drains the pty master once"
    );
    // The order is the fix: draining after `stop()` does nothing, because
    // `stop()` is what wedges on an undrained child.
    assert_eq!(
        STOP_CALLS.with(std::cell::Cell::get),
        1,
        "the error arm calls subprocess.stop() once"
    );
    assert_eq!(
        STOPS_BEFORE_DRAIN.with(std::cell::Cell::get),
        Some(0),
        "the drain runs before subprocess.stop(), not after it"
    );
    // `stop()` ran: the subprocess has been torn down.
    assert!(
        exec.subprocess.process.is_none(),
        "the error arm stops the subprocess"
    );

    td.deinit();
    loop_.deinit();
    exec.deinit();
    t.teardown();
}

/// UD67 harness: `threadEnter` a real `/bin/sh -c <script>` child on a real
/// pty and kqueue loop, run `before_first_tick` (which sees the child's pid)
/// BEFORE the loop has submitted the `EVFILT_PROC` registration, then pump the
/// loop until `processExit` has run. Returns the `child_exited` codes the
/// mailbox received and the backend's `exited` flag.
#[cfg(target_os = "macos")]
fn run_child_to_process_exit(
    script: &[u8],
    before_first_tick: impl FnOnce(libc::pid_t, &mut xev::Loop),
) -> (Vec<u32>, bool) {
    use allocator_api2::alloc::Global;
    use std::time::{Duration, Instant};

    CHILD_EXITED_CODES.with(|v| v.borrow_mut().clear());
    let cfg = Config {
        command: Some(configpkg::command::Command::Direct(vec![
            b"/bin/sh".to_vec(),
            b"-c".to_vec(),
            script.to_vec(),
        ])),
        env: EnvMap::new(),
        env_override: configpkg::RepeatableStringMap::default(),
        shell_integration: configpkg::value_types::ShellIntegration::Detect,
        shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
        cursor_blink: None,
        working_directory: None,
        resources_dir: None,
        term: b"xterm-256color".to_vec(),
        rt_pre_exec_info: RtPreExecInfo::default(),
        rt_post_fork_info: RtPostForkInfo::default(),
    };
    let mut exec = Exec::init(Global, cfg).expect("Exec::init");
    let mut loop_ = xev::Loop::init(xev::Options::default()).expect("Loop::init");
    let loop_ptr: *mut xev::Loop = std::ptr::addr_of_mut!(loop_);
    let t = TestTermio::build();
    let mut td: crate::termio_impl::ThreadData<TestMailbox> = crate::termio_impl::ThreadData {
        alloc: Global,
        loop_: loop_ptr,
        renderer_state: t.state,
        surface_mailbox: TestMailbox,
        backend: crate::backend::ThreadData::Exec(ThreadData {
            start: Instant::now(),
            exited: false,
            write_stream: xev::Stream::init_fd(-1),
            process: None,
            write_pool: MemoryPool::init(Global),
            write_queue: xev::WriteQueue::default(),
            process_wait_c: xev::Completion::default(),
            flatpak_wait_c: FlatpakHostCommandCompletion::default(),
            read_thread: std::thread::spawn(|| {}),
            read_thread_pipe: -1,
            read_thread_fd: -1,
            termios_timer: xev::Timer::init().expect("placeholder Timer::init"),
            termios_timer_c: xev::Completion::default(),
            termios_timer_running: false,
            termios_mode: ptypkg::Mode::default(),
        }),
        mailbox: std::ptr::null_mut(),
    };
    unsafe { Exec::threadEnter(&mut exec, Global, t.io, &mut td) }.expect("threadEnter");
    let pid = match &exec.subprocess.process {
        Some(Process::ForkExec(cmd)) => cmd.pid.expect("spawned child has a pid"),
        _ => panic!("expected a fork/exec child"),
    };
    let master: std::os::fd::RawFd = {
        let crate::backend::ThreadData::Exec(e) = &td.backend;
        e.read_thread_fd
    };

    before_first_tick(pid, unsafe { &mut *loop_ptr });

    let exited = |td: &crate::termio_impl::ThreadData<TestMailbox>| {
        let crate::backend::ThreadData::Exec(e) = &td.backend;
        e.exited
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !exited(&td) && Instant::now() < deadline {
        // N431: through `loop_ptr`, the pointer `td.loop_` re-enters through.
        unsafe { xev::Loop::run_raw(loop_ptr, xev::RunMode::NoWait) }.expect("loop.run(NoWait)");
        std::thread::sleep(Duration::from_millis(1));
    }
    let was_exited = exited(&td);
    let codes = CHILD_EXITED_CODES.with(|v| v.borrow().clone());

    unsafe { Exec::threadExit(&mut exec, &mut td) };
    unsafe {
        libc::close(master);
    }
    td.deinit();
    loop_.deinit();
    exec.deinit();
    t.teardown();
    (codes, was_exited)
}

/// Whether `pid` has exited and is waiting to be reaped. `WNOWAIT` leaves it
/// a zombie, so the loop's `EVFILT_PROC` registration still sees it.
#[cfg(target_os = "macos")]
fn child_is_zombie(pid: libc::pid_t) -> bool {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    rc == 0 && info.si_pid == pid
}

/// Registers `EVFILT_PROC` for `pid` on a private kqueue, exactly as the xev
/// kqueue backend does (`EV_ADD | EV_ENABLE`, `NOTE_EXIT | NOTE_EXITSTATUS`),
/// and returns the errno the kernel answers the registration with (0 if it
/// was accepted).
#[cfg(target_os = "macos")]
fn evfilt_proc_registration_errno(pid: libc::pid_t) -> i64 {
    unsafe {
        let kq = libc::kqueue();
        assert!(kq >= 0);
        let change = libc::kevent {
            ident: pid as libc::uintptr_t,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ENABLE | libc::EV_RECEIPT,
            fflags: libc::NOTE_EXIT | libc::NOTE_EXITSTATUS,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let mut out: libc::kevent = std::mem::zeroed();
        let n = libc::kevent(kq, &change, 1, &mut out, 1, std::ptr::null());
        libc::close(kq);
        assert_eq!(n, 1, "EV_RECEIPT always answers the change");
        if out.flags & libc::EV_ERROR != 0 {
            out.data as i64
        } else {
            0
        }
    }
}

// UD67 (R13): a child that has already exited when the kqueue loop submits
// its `EVFILT_PROC` registration. XNU answers that registration with ESRCH
// (`NoSuchProcess`), where the reference's `processExit` does `catch
// unreachable`. Driven through the REAL path, no seam: the registration is
// only submitted on the loop's first tick after `threadEnter`, so the test
// waits (with `WNOWAIT`, not a guessed sleep) until the child is a zombie
// before that first tick, and first checks on a private kqueue that the
// kernel really refuses a registration for it with ESRCH. (The exit code
// cannot tell the arms apart on macOS: the child is `login(1)`, which exits
// 0 whatever its command does.)
#[test]
#[cfg(target_os = "macos")]
fn process_exit_reports_a_child_that_exited_before_the_watch_was_registered() {
    use std::time::{Duration, Instant};
    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let mut saw_zombie = false;
    let mut registration_errno = 0;
    let (codes, exited) = run_child_to_process_exit(b"exit 3", |pid, _loop| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !child_is_zombie(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        saw_zombie = child_is_zombie(pid);
        registration_errno = evfilt_proc_registration_errno(pid);
    });
    assert!(
        saw_zombie,
        "precondition: the child must have exited before the first tick"
    );
    assert_eq!(
        registration_errno,
        libc::ESRCH as i64,
        "precondition: the kernel refuses EVFILT_PROC for the zombie with ESRCH"
    );
    assert!(
        exited,
        "processExitCommon must run for the ESRCH wait result"
    );
    assert_eq!(
        codes,
        vec![0],
        "exactly one child_exited, with the no-status code 0"
    );
}
