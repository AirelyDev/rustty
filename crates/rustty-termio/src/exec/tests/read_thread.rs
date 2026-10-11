//! PORTS: ghostty src/termio/Exec.zig — the `ReadThread` KATs (posix gather/read pipeline and Windows).

use super::*;

/// UD19: the outer-poll decision on its own. A hangup without data ends
/// the thread at once (the reference's behaviour); a hangup with data
/// sends it back to read once, and the next hangup ends it.
#[cfg(not(windows))]
#[test]
fn outer_poll_hangup_with_data_reads_once_more() {
    let mut drained = false;
    assert!(!ReadThread::outer_poll_hup_exits(
        libc::POLLIN,
        &mut drained
    ));
    assert!(!drained, "no hangup, nothing to drain");
    assert!(ReadThread::outer_poll_hup_exits(
        libc::POLLHUP,
        &mut drained
    ));

    let mut drained = false;
    let both = libc::POLLIN | libc::POLLHUP;
    assert!(!ReadThread::outer_poll_hup_exits(both, &mut drained));
    assert!(drained);
    assert!(ReadThread::outer_poll_hup_exits(both, &mut drained));
}

/// UD19 (PORT NOTE: reference defect (R13), at `outer_poll_hup_exits`):
/// output that arrives together with the hangup, while the gather stage is
/// parked in its outer poll, is still read. The hook writes `second` and
/// shuts down the writing side right before that poll, so the poll reports
/// `POLLIN | POLLHUP` with `second` unread (measured on macOS for a Unix
/// socket pair after `shutdown(SHUT_WR)` or `shutdown(SHUT_RDWR)`, and for a
/// pipe after `close`: `revents == 0x11`; on Linux a pty master reports the
/// same word when the child writes and exits, UD19). The reference returns
/// on `POLLHUP` alone and drops `second`.
#[cfg(not(windows))]
#[test]
fn gather_stage_reads_output_that_arrives_with_the_hangup() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // The data fd is a socket pair, not a pipe: the hangup is made with
    // `shutdown(SHUT_RDWR)`, which acts on the socket itself. Closing a pipe's
    // write end is not enough in this binary, where other tests fork
    // children that can hold a copy of it for a moment, and then the poll
    // can see the data before the hangup (measured: an extra outer poll).
    //
    // FreeBSD (N563) reports `POLLHUP` on a Unix stream socket only once the
    // peer is closed: after a peer `shutdown(SHUT_RDWR)` it reports `POLLIN`
    // alone, even with the reader's own `SHUT_WR` (measured on 15.1; a real
    // pty reports `POLLIN | POLLHUP` when its slave closes, as on Linux and
    // macOS). There the hangup closes the peer instead, and the pair is
    // close-on-exec so a forked child drops its copy when it execs.
    let mut fds = [0 as libc::c_int; 2];
    #[cfg(target_os = "freebsd")]
    let sock_type = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
    #[cfg(not(target_os = "freebsd"))]
    let sock_type = libc::SOCK_STREAM;
    assert_eq!(
        unsafe { libc::socketpair(libc::AF_UNIX, sock_type, 0, fds.as_mut_ptr()) },
        0
    );
    let (data_r, data_w) = (fds[0], fds[1]);
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    let (quit_r, quit_w) = (fds[0], fds[1]);
    // The gather stage expects a non-blocking fd (`threadMainPosix` sets it).
    unsafe {
        let fl = libc::fcntl(data_r, libc::F_GETFL);
        assert!(fl >= 0);
        assert_eq!(libc::fcntl(data_r, libc::F_SETFL, fl | libc::O_NONBLOCK), 0);
    }
    let first = b"first";
    assert_eq!(
        unsafe { libc::write(data_w, first.as_ptr().cast(), first.len()) },
        first.len() as isize
    );

    let polls = Arc::new(AtomicUsize::new(0));
    {
        let polls = Arc::clone(&polls);
        *BEFORE_OUTER_POLL.lock().unwrap() = Some((
            data_r,
            Box::new(move || {
                if polls.fetch_add(1, Ordering::SeqCst) == 0 {
                    let second = b"second";
                    assert_eq!(
                        unsafe { libc::write(data_w, second.as_ptr().cast(), second.len()) },
                        second.len() as isize
                    );
                    // SHUT_RDWR, not SHUT_WR: Linux reports `POLLHUP` on a
                    // Unix stream socket only once both directions are shut
                    // (a peer's SHUT_WR alone gives `POLLIN`, and read then
                    // returns 0 forever, so the gather stage never ends).
                    // macOS reports `POLLIN | POLLHUP` for either.
                    #[cfg(not(target_os = "freebsd"))]
                    assert_eq!(unsafe { libc::shutdown(data_w, libc::SHUT_RDWR) }, 0);
                    #[cfg(target_os = "freebsd")]
                    assert_eq!(unsafe { libc::close(data_w) }, 0);
                }
            }),
        ));
    }

    // `'static` so a hung gather stage cannot hang the test: the test waits
    // with a timeout instead of joining. The box is freed once the stage has
    // ended (below); it stays leaked only when the stage hangs, the case the
    // `finished` assertion fails on (it was always leaked: 262,240 bytes
    // under memcheck, NEW-FINDINGS N410).
    let pipeline_box: *mut Pipeline = Box::into_raw(Box::new(Pipeline::new()));
    // SAFETY: `pipeline_box` is live until the `Box::from_raw` below, which
    // runs only after the gather thread has returned from its last use.
    let pipeline: &'static Pipeline = unsafe { &*pipeline_box };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        ReadThread::gatherMainPosix(data_r, quit_r, pipeline);
        let _ = tx.send(());
    });
    let finished = rx.recv_timeout(std::time::Duration::from_secs(10));
    *BEFORE_OUTER_POLL.lock().unwrap() = None;
    assert!(
        finished.is_ok(),
        "the gather stage must end after the hangup"
    );

    let mut got = Vec::new();
    {
        let mut m = pipeline.mutex.lock().unwrap();
        assert!(m.done, "the gather stage marks the stream done");
        while m.count > 0 {
            let slot = m.tail;
            let len = m.lens[slot];
            got.extend_from_slice(unsafe {
                std::slice::from_raw_parts(pipeline.slot_ptr(slot), len)
            });
            m.tail = (m.tail + 1) % ReadThread::BUFFER_COUNT;
            m.count -= 1;
        }
    }
    unsafe {
        libc::close(data_r);
        #[cfg(not(target_os = "freebsd"))]
        libc::close(data_w);
        libc::close(quit_r);
        libc::close(quit_w);
    }
    // SAFETY: the gather stage returned (`finished`), so the thread holds no
    // further use of `pipeline`, and `got` is an owned copy.
    drop(unsafe { Box::from_raw(pipeline_box) });
    assert_eq!(
        got, b"firstsecond",
        "the output that came with the hangup must be delivered"
    );
    assert_eq!(
        polls.load(Ordering::SeqCst),
        2,
        "one outer poll saw POLLIN|POLLHUP and drained; the next saw the hangup again and ended"
    );
}

/// S211-F1: the read pipeline's ring is the pinned reference's on every
/// target (`Exec.zig:1305` `buffer_count = 4`, `:1318` `buffer_capacity =
/// 64 * 1024`). It was 2 x 8 KiB off Darwin, which only a non-Darwin run
/// of this test observes.
#[test]
fn read_thread_ring_matches_the_reference_on_every_target() {
    assert_eq!(ReadThread::BUFFER_COUNT, 4);
    assert_eq!(ReadThread::BUFFER_CAPACITY, 64 * 1024);
}

// KAT (rustty-only margin — blueprint §7 B3; NOT a mapped Zig `test{}`, so the
// test-count contract is unaffected). Drives the B3 read path end-to-end,
// WITHOUT a shell: a real `ReadThread::threadMainPosix` poll/read loop over a
// real pty delivers a byte string fed to the slave into the terminal `Screen`
// via the [ADV-FIX F2] raw-`*mut` `Termio::processOutput`/`processOutputLocked`.
//
//   build a heap-pinned Termio<TestMailbox> (its `StreamHandler` self-pointers
//   re-seated at the final address, per F1) -> openpty -> spawn the read thread
//   on the master -> write "RUSTTY_B3_READPATH" to the slave -> bounded poll of
//   the active Screen under the renderer mutex -> quit-pipe teardown + join.
//
// Real openpty + poll/read syscalls + a spawned thread -> Miri-excluded
// (MIRI_EXCEPTIONS.md, "rustty-termio B3"). Bounded (tick cap + a wall-clock
// deadline + explicit teardown), so it never blocks. No fork here, but it holds
// open pty fds while other in-process KATs may `fork()`, so it takes
// FORK_KAT_SERIAL (a concurrent fork would snapshot these fds).
#[test]
#[cfg(target_os = "macos")]
fn readthread_pty_bytes_land_in_screen() {
    use std::time::{Duration, Instant};

    // Serialize with the other fork/pty KATs (see FORK_KAT_SERIAL).
    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    // A real, heap-pinned Termio<TestMailbox> whose read path the read thread
    // drives via `processOutput`.
    let t = TestTermio::build();
    let io = t.io;

    // --- A real pty: the read thread reads the master; the test writes the
    //     slave (which appears on the master). ---
    let pty = Pty::open(ptypkg::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 800,
        ws_ypixel: 480,
    })
    .expect("Pty::open");
    let master = pty.master;
    let slave = pty.slave;

    // --- The quit pipe (read end -> read thread; write end -> teardown). ---
    let mut pipefds = [0 as std::os::fd::RawFd; 2];
    assert_eq!(unsafe { libc::pipe(pipefds.as_mut_ptr()) }, 0, "pipe()");
    let quit_read = pipefds[0];
    let quit_write = pipefds[1];

    // --- Spawn the read thread (the raw `io` ptr crosses via ReadThreadArgs). ---
    let read_args = ReadThreadArgs::<TestMailbox>(master, io, quit_read);
    let handle = std::thread::spawn(move || {
        let args = read_args;
        ReadThread::threadMainPosix::<TestMailbox>(args.0, args.1, args.2);
    });

    // --- Feed the pty: write the needle to the slave (no newline -> no OPOST
    //     translation). It appears on the master, which the read thread reads. ---
    const NEEDLE: &[u8] = b"RUSTTY_B3_READPATH";
    let w = unsafe {
        libc::write(
            slave,
            NEEDLE.as_ptr().cast::<std::ffi::c_void>(),
            NEEDLE.len(),
        )
    };
    assert_eq!(
        w,
        NEEDLE.len() as isize,
        "wrote the full needle to the pty slave"
    );

    // --- Bounded poll of the active Screen (under the renderer mutex, via the
    //     harness `screen_dump`). ---
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut found = false;
    for _ in 0..3000 {
        let dump = t.screen_dump();
        if dump.windows(NEEDLE.len()).any(|c| c == NEEDLE) {
            found = true;
            break;
        }
        if Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    // --- Teardown: signal quit, join the read thread (which closes quit_read
    //     via its Drop guard), then close the remaining fds + deinit the Termio
    //     (safe now that the read thread — the sole `io` toucher — is joined). ---
    let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
    handle.join().expect("read thread join");
    unsafe {
        libc::close(quit_write);
        libc::close(master);
        libc::close(slave);
    }
    t.teardown();

    assert!(
        found,
        "pty bytes did not land in the terminal Screen within the deadline"
    );
}

/// S211-F2: the io-reader thread tags its crash metadata as an `io` thread
/// of the mailbox's surface (`Exec.zig:1411-1417`). A BEL written to the
/// pty makes the read thread push `RingBell`, and `TestMailbox` records
/// that thread's `thread_state`.
#[test]
#[cfg(target_os = "macos")]
fn readthread_sets_io_crash_thread_state() {
    use std::time::{Duration, Instant};

    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    BELL_THREAD_STATES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    // The test thread itself has no crash state.
    assert!(rustty_crash::sentry::threadState().is_none());

    let (handle, master, slave, quit_write, t) = spawn_readthread_over_pty();
    let w = unsafe { libc::write(slave, b"\x07".as_ptr().cast::<std::ffi::c_void>(), 1) };
    assert_eq!(w, 1, "wrote BEL to the pty slave");

    let deadline = Instant::now() + Duration::from_secs(3);
    let states = loop {
        let states = BELL_THREAD_STATES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if !states.is_empty() || Instant::now() > deadline {
            break states;
        }
        std::thread::sleep(Duration::from_millis(1));
    };

    let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
    handle.join().expect("read thread join");
    unsafe {
        libc::close(quit_write);
        libc::close(master);
        libc::close(slave);
    }
    t.teardown();

    assert_eq!(
        states,
        vec![Some((rustty_crash::sentry::Type::Io, true))],
        "the read thread's thread_state is .io with the mailbox's surface"
    );
}

/// Serializes the Windows read-thread tests, which share
/// `BELL_THREAD_STATES` (Windows has no `FORK_KAT_SERIAL`: nothing forks).
#[cfg(windows)]
static WINDOWS_READTHREAD_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Windows: the anonymous pipe pair the reference reads a ConPTY through
/// (`CreatePipe`, `pty.zig`'s `out_pipe`), as `[read, write]`.
#[cfg(windows)]
fn windows_pipe() -> [rustty_os::windows::HANDLE; 2] {
    rustty_os::pipe::pipe().expect("pipe()")
}

/// Windows: write all of `bytes` to a pipe handle.
#[cfg(windows)]
fn windows_write(h: rustty_os::windows::HANDLE, bytes: &[u8]) {
    use rustty_os::windows as win;
    let mut written: win::DWORD = 0;
    let ok = unsafe {
        win::exp::kernel32::WriteFile(
            h,
            bytes.as_ptr().cast::<std::ffi::c_void>(),
            bytes.len() as win::DWORD,
            &mut written,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, win::FALSE, "WriteFile");
    assert_eq!(written as usize, bytes.len(), "short WriteFile");
}

/// S211-F2, Windows arm: `threadMainWindows` tags its crash metadata as an
/// `io` thread of the mailbox's surface (`Exec.zig:1776-1781`). A BEL
/// written to the pipe it reads makes it push `RingBell`, and `TestMailbox`
/// records that thread's `thread_state`. The thread is stopped as
/// `threadExit` stops it: a byte on the quit pipe, then `CancelIoEx` on the
/// read handle.
#[test]
#[cfg(windows)]
fn windows_readthread_sets_io_crash_thread_state() {
    use std::time::{Duration, Instant};

    let _serial = WINDOWS_READTHREAD_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    BELL_THREAD_STATES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
    // The test thread itself has no crash state.
    assert!(rustty_crash::sentry::threadState().is_none());

    let t = TestTermio::build();
    let [data_read, data_write] = windows_pipe();
    let [quit_read, quit_write] = windows_pipe();
    let read_args = ReadThreadArgs::<TestMailbox>(data_read, t.io, quit_read);
    let handle = std::thread::spawn(move || {
        let args = read_args;
        ReadThread::threadMainWindows::<TestMailbox>(args.0, args.1, args.2);
    });

    windows_write(data_write, b"\x07");
    let deadline = Instant::now() + Duration::from_secs(5);
    let states = loop {
        let states = BELL_THREAD_STATES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if !states.is_empty() || Instant::now() > deadline {
            break states;
        }
        std::thread::sleep(Duration::from_millis(1));
    };

    // Stop it as `threadExit` does: the quit byte, then the interrupt.
    windows_write(quit_write, b"x");
    ReadThread::interruptWindows(data_read, || handle.is_finished());
    handle.join().expect("read thread join");
    // (the read thread closed `quit_read` on exit)
    for h in [quit_write, data_read, data_write] {
        let _ = rustty_os::windows::CloseHandle(h);
    }
    t.teardown();

    assert_eq!(
        states,
        vec![Some((rustty_crash::sentry::Type::Io, true))],
        "the read thread's thread_state is .io with the mailbox's surface"
    );
}

/// UD326 (N341): `threadExit`'s interrupt stops the Windows read thread
/// even when its cancel lands between two reads. The test holds the
/// renderer mutex, so after the thread reads a byte it blocks in
/// `processOutput`, not in `ReadFile`. The quit byte and the interrupt run
/// then, on another thread; the mutex is released after the first cancel
/// has found nothing to cancel. The reference's single cancel is lost, the
/// thread re-enters `ReadFile`, and nothing more is written: it would
/// block forever. With the port's repeated cancel it returns.
#[test]
#[cfg(windows)]
fn windows_readthread_quit_survives_a_cancel_between_reads() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    let _serial = WINDOWS_READTHREAD_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let t = TestTermio::build();
    let [data_read, data_write] = windows_pipe();
    let [quit_read, quit_write] = windows_pipe();
    let finished = Arc::new(AtomicBool::new(false));
    let read_args = ReadThreadArgs::<TestMailbox>(data_read, t.io, quit_read);
    let done = Arc::clone(&finished);
    let reader = std::thread::spawn(move || {
        let args = read_args;
        ReadThread::threadMainWindows::<TestMailbox>(args.0, args.1, args.2);
        done.store(true, Ordering::SeqCst);
    });

    // The thread reads this byte and blocks on the renderer mutex.
    unsafe { (*t.mutex).lock() };
    windows_write(data_write, b"a");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut avail: rustty_os::windows::DWORD = 0;
        let ok = unsafe {
            rustty_os::windows::exp::kernel32::PeekNamedPipe(
                data_read,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut avail,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(ok, 0, "PeekNamedPipe");
        if avail == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the read thread never read the byte"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    // It is now in (or on its way into) `processOutput`, blocked on the
    // mutex. Positive check that the cancel really lands between reads:
    // with no read pending, `CancelIoEx` reports ERROR_NOT_FOUND.
    std::thread::sleep(Duration::from_millis(50));
    let cancelled =
        unsafe { rustty_os::windows::exp::kernel32::CancelIoEx(data_read, std::ptr::null_mut()) };
    let err = unsafe { rustty_os::windows::kernel32::GetLastError() };
    assert_eq!(
        (cancelled, err),
        (0, 1168),
        "no read may be pending while the thread is in processOutput"
    );

    // `threadExit`'s sequence, on its own thread: the quit byte, then the
    // interrupt.
    let flag = Arc::clone(&finished);
    let data_read_addr = data_read as usize;
    let quit_write_addr = quit_write as usize;
    let stopper = std::thread::spawn(move || {
        windows_write(quit_write_addr as rustty_os::windows::HANDLE, b"x");
        ReadThread::interruptWindows(data_read_addr as rustty_os::windows::HANDLE, || {
            flag.load(Ordering::SeqCst)
        });
    });
    // Let the interrupt's first cancel run while the thread is still
    // between reads, then let the thread finish processing.
    std::thread::sleep(Duration::from_millis(100));
    unsafe { (*t.mutex).unlock() };

    let deadline = Instant::now() + Duration::from_secs(5);
    while !finished.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let stopped = finished.load(Ordering::SeqCst);
    if !stopped {
        // Unblock it so the test can clean up, then fail.
        while !finished.load(Ordering::SeqCst) {
            let _ = unsafe {
                rustty_os::windows::exp::kernel32::CancelIoEx(data_read, std::ptr::null_mut())
            };
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    stopper.join().expect("stopper join");
    reader.join().expect("read thread join");
    for h in [quit_write, data_read, data_write] {
        let _ = rustty_os::windows::CloseHandle(h);
    }
    t.teardown();
    assert!(
        stopped,
        "the read thread was still blocked in ReadFile 5 s after threadExit's interrupt"
    );
}

/// S211-F3: on Windows `threadEnter` spawns `threadMainWindows`
/// (`Exec.zig:135-140`). A real child on a real ConPTY (`cmd.exe /c echo`)
/// writes a marker; the only reader of the pseudo console's output pipe is
/// the read thread `threadEnter` spawned, so the marker reaching the
/// terminal's Screen proves that thread ran the Windows read loop.
#[test]
#[cfg(windows)]
fn windows_threadenter_reads_the_pty_through_the_windows_read_thread() {
    use allocator_api2::alloc::Global;
    use std::time::{Duration, Instant};

    let _serial = WINDOWS_READTHREAD_SERIAL
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let cfg = Config {
        command: Some(configpkg::command::Command::Direct(vec![
            b"cmd.exe".to_vec(),
            b"/c".to_vec(),
            // `%RUSTTY_W1_VAR%` expands in the child from the environment
            // given below, so the marker appears only in its output, never
            // in its command line.
            b"echo RUSTTY_W1_%RUSTTY_W1_VAR%".to_vec(),
        ])),
        env: EnvMap::from([(b"RUSTTY_W1_VAR".to_vec(), b"MARK".to_vec())]),
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
    // `initTerminalIo`'s initial resize from the terminal (80x24), which
    // runs before `threadEnter`; ConPTY rejects the default 0x0 size.
    unsafe {
        Exec::resize(
            &mut exec,
            GridSize {
                columns: 80,
                rows: 24,
            },
            ScreenSize {
                width: 800,
                height: 480,
            },
        )
    }
    .expect("resize");
    let mut loop_ = xev::Loop::init(xev::Options::default()).expect("Loop::init");
    let loop_ptr: *mut xev::Loop = std::ptr::addr_of_mut!(loop_);
    let t = TestTermio::build();
    let io_ptr = t.io;
    let mut td: crate::termio_impl::ThreadData<TestMailbox> = crate::termio_impl::ThreadData {
        alloc: Global,
        loop_: loop_ptr,
        renderer_state: t.state,
        surface_mailbox: TestMailbox,
        backend: crate::backend::ThreadData::placeholder(),
        mailbox: std::ptr::null_mut(),
    };

    // N354: on failure, name the step that failed and its error.
    THREAD_ENTER_FAILURE.with(|f| f.borrow_mut().clear());
    if let Err(e) = unsafe { Exec::threadEnter(&mut exec, Global, io_ptr, &mut td) } {
        let steps = THREAD_ENTER_FAILURE.with(|f| f.borrow().clone());
        panic!("threadEnter: {e:?}; failed steps (innermost first): {steps:?}");
    }

    let needle = b"RUSTTY_W1_MARK";
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut found = false;
    while Instant::now() < deadline {
        // N431: through `loop_ptr`, the pointer `td.loop_` re-enters through.
        unsafe { xev::Loop::run_raw(loop_ptr, xev::RunMode::NoWait) }.expect("loop.run(NoWait)");
        let dump = t.screen_dump();
        if dump.windows(needle.len()).any(|w| w == needle) {
            found = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let screen = String::from_utf8_lossy(&t.screen_dump()).into_owned();
    eprintln!("screen: {screen:?}");

    unsafe { Exec::threadExit(&mut exec, &mut td) };
    td.deinit();
    loop_.deinit();
    exec.deinit();
    t.teardown();

    assert!(
        found,
        "the child's output did not reach the Screen through the read thread; screen: {screen:?}"
    );
}

/// S211-F6: when the gather thread cannot be spawned, `threadMainPosix`
/// logs the reference's error and returns (`Exec.zig:1470-1479`); it does
/// not panic. The spawn fails for real (a test-only switch asks for a stack
/// no thread can get). `threadMainPosix` runs on the test thread itself, so
/// the thread-local switch and the thread-owned log capture both see it.
#[test]
#[cfg(target_os = "macos")]
fn readthread_gather_spawn_failure_logs_and_returns() {
    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let t = TestTermio::build();
    let pty = Pty::open(ptypkg::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 800,
        ws_ypixel: 480,
    })
    .expect("Pty::open");
    let mut pipefds = [0 as std::os::fd::RawFd; 2];
    assert_eq!(unsafe { libc::pipe(pipefds.as_mut_ptr()) }, 0, "pipe()");
    let (quit_read, quit_write) = (pipefds[0], pipefds[1]);

    let cap = crate::log_capture::start();
    FAIL_GATHER_SPAWN.with(|f| f.set(true));
    let io = t.io;
    let master = pty.master;
    let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ReadThread::threadMainPosix::<TestMailbox>(master, io, quit_read);
    }));
    FAIL_GATHER_SPAWN.with(|f| f.set(false));
    let errors: Vec<String> = cap
        .messages()
        .into_iter()
        .filter(|(level, target, _)| *level == log::Level::Error && target == LOG_SCOPE)
        .map(|(_, _, message)| message)
        .collect();
    drop(cap);

    // `quit_read` was closed by the read thread's guard on return.
    unsafe {
        libc::close(quit_write);
        libc::close(pty.master);
        libc::close(pty.slave);
    }
    t.teardown();

    assert!(ran.is_ok(), "threadMainPosix must return, not panic");
    // S211-F2: its crash `thread_state` guard cleared on the way out.
    assert!(rustty_crash::sentry::threadState().is_none());
    assert_eq!(errors.len(), 1, "one error logged: {errors:?}");
    assert!(
        errors[0].starts_with("read thread exiting, failed to spawn gather thread err="),
        "the reference's message: {errors:?}"
    );
}

// KAT (rustty-only margin; NOT a mapped Zig `test{}`) for 2f0e6659d's
// separable `setQosClass` hunk — WS10 T8. `E-os-termio-platform.md` row E4
// measures it at 15% throughput on an M4 Max, and the reference ships it
// with no test at all.
//
// WHY THIS IS NOT A UNIT TEST OF `setQosClass`. Calling the helper on any
// old thread and reading the class back would pass whether or not
// `threadMainPosix` ever calls it — the vacuity trap. So this drives the
// REAL read thread over a REAL pty, exactly as
// `readthread_pty_bytes_land_in_screen` does, and interrogates THAT
// thread's QoS class from outside via its `pthread_t`
// (`JoinHandleExt::as_pthread_t`). `pthread_get_qos_class_np` is the
// thread-addressed reader (`pthread_set_qos_class_self_np`, which the
// helper wraps, is self-only), so no assertion has to be smuggled into the
// thread body.
//
// The default for a thread spawned from a default-QoS thread is
// QOS_CLASS_DEFAULT (0x15); the assertion is QOS_CLASS_USER_INITIATED
// (0x19). Those differ, so the test cannot pass by accident on an
// unmodified thread — verified by deleting the `Self::setQosClass()` call,
// which turns it RED with `qos=0x15`.
//
// Real openpty + a spawned thread -> Miri-excluded, same as the KAT above,
// and it takes FORK_KAT_SERIAL for the same reason (it holds pty fds open
// while other in-process KATs may `fork()`).
#[test]
#[cfg(target_os = "macos")]
fn readthread_sets_user_initiated_qos() {
    use std::os::unix::thread::JoinHandleExt;
    use std::time::{Duration, Instant};

    // int pthread_get_qos_class_np(pthread_t, qos_class_t *, int *);
    //
    // Hand-rolled per the `mem.rs`/`page.rs` convention: `libc` 0.2 exposes
    // `pthread_set_qos_class_self_np` but NOT the thread-addressed getter,
    // and the constraint is "no NEW `libc` surface elsewhere", not "no
    // extern blocks". `qos_class_t` is an unsigned enum.
    unsafe extern "C" {
        fn pthread_get_qos_class_np(
            thread: libc::pthread_t,
            qos_class: *mut libc::c_uint,
            relative_priority: *mut libc::c_int,
        ) -> libc::c_int;
    }
    const QOS_CLASS_USER_INITIATED: libc::c_uint = 0x19;
    const QOS_CLASS_DEFAULT: libc::c_uint = 0x15;

    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let t = TestTermio::build();
    let io = t.io;

    let pty = Pty::open(ptypkg::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 800,
        ws_ypixel: 480,
    })
    .expect("Pty::open");
    let master = pty.master;
    let slave = pty.slave;

    let mut pipefds = [0 as std::os::fd::RawFd; 2];
    assert_eq!(unsafe { libc::pipe(pipefds.as_mut_ptr()) }, 0, "pipe()");
    let quit_read = pipefds[0];
    let quit_write = pipefds[1];

    let read_args = ReadThreadArgs::<TestMailbox>(master, io, quit_read);
    let handle = std::thread::spawn(move || {
        let args = read_args;
        ReadThread::threadMainPosix::<TestMailbox>(args.0, args.1, args.2);
    });
    let tid = handle.as_pthread_t();

    // The helper runs at the very top of the thread body, but "the very top"
    // is still after the spawn returns, so poll rather than race.
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut qos: libc::c_uint = 0;
    let mut rel: libc::c_int = 0;
    let mut observed = false;
    loop {
        let rc = unsafe { pthread_get_qos_class_np(tid, &mut qos, &mut rel) };
        assert_eq!(rc, 0, "pthread_get_qos_class_np failed");
        if qos == QOS_CLASS_USER_INITIATED {
            observed = true;
            break;
        }
        if Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    // Teardown BEFORE asserting, so a failure does not leak the thread/fds.
    let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
    handle.join().expect("read thread join");
    unsafe {
        libc::close(quit_write);
        libc::close(master);
        libc::close(slave);
    }
    t.teardown();

    assert!(
        observed,
        "the io-reader thread must run at QOS_CLASS_USER_INITIATED (0x19); \
         observed 0x{qos:x} (0x{QOS_CLASS_DEFAULT:x} is the un-set default)"
    );
}

// ---------------------------------------------------------------------
// WS10 T20 — the two properties `E-os-termio-platform.md` row E1 says do
// not exist, plus the shutdown property for the new early returns.
//
// `readthread_pty_bytes_land_in_screen` above passes with EITHER the
// pre-pipeline single loop or the two-stage pipeline (it writes 18 bytes and
// looks for them), so it is NOT a guard for this commit. These three are.
//
// Real openpty + poll/read syscalls + spawned threads -> Miri-excluded
// (MIRI_EXCEPTIONS.md, "rustty-termio B3"); the pure ring logic is proven
// under Miri by `.port/phase-2/mirihar-ws10-pipeline` instead.
// ---------------------------------------------------------------------

/// Drive a real pty through a real `ReadThread::threadMainPosix` and return
/// `(handle, master, slave, quit_write, TestTermio)` wired for teardown.
#[cfg(target_os = "macos")]
fn spawn_readthread_over_pty() -> (
    std::thread::JoinHandle<()>,
    std::os::fd::RawFd,
    std::os::fd::RawFd,
    std::os::fd::RawFd,
    TestTermio,
) {
    let t = TestTermio::build();
    let io = t.io;

    let pty = Pty::open(ptypkg::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 800,
        ws_ypixel: 480,
    })
    .expect("Pty::open");
    let master = pty.master;
    let slave = pty.slave;

    let mut pipefds = [0 as std::os::fd::RawFd; 2];
    assert_eq!(unsafe { libc::pipe(pipefds.as_mut_ptr()) }, 0, "pipe()");
    let quit_read = pipefds[0];
    let quit_write = pipefds[1];

    let read_args = ReadThreadArgs::<TestMailbox>(master, io, quit_read);
    let handle = std::thread::spawn(move || {
        let args = read_args;
        ReadThread::threadMainPosix::<TestMailbox>(args.0, args.1, args.2);
    });

    (handle, master, slave, quit_write, t)
}

/// A saturated producer must be fully drained into the terminal with no
/// lost or duplicated bytes, and the pipeline must be measurably FASTER at
/// it than the pre-pipeline shape.
///
/// This is the THROUGHPUT property `2f0e6659d` exists for. Two assertions,
/// deliberately different in kind:
///
/// 1. COMPLETENESS (both profiles). Every byte the producer wrote reaches
///    `processOutput` exactly once. This catches a dropped, duplicated or
///    stalled batch — the ways the ring can be wrong. It does NOT
///    distinguish the pipeline from the old loop: measured, the old shape
///    is equally correct. Stating that plainly matters, because a test that
///    passes either way is not a guard, and this half is not one.
///
/// 2. BATCHING (both profiles). The MEAN BATCH SIZE floor below. THIS is
///    the guard, and it is deliberately NOT a throughput number.
///
/// WHY NOT THROUGHPUT. The obvious assertion is bytes/second, and the
/// pipeline does win on it: measured on this host (M4 Pro, 2 MiB through a
/// real pty, `--release`, N=9 each, both shapes built from this same tree)
/// the old loop ran at 87.5 MiB/s median and this pipeline at 123.8 MiB/s,
/// +41%. But a wall-clock floor is not a sound ASSERTION here: run inside
/// the full crate suite on a box under load ~9 the same pipeline measured
/// 71.7 MiB/s — below the old shape's median. A floor that a healthy build
/// fails under contention is a flaky gate, and a flaky gate gets ignored,
/// which is the failure mode this whole workstream exists to avoid. The
/// throughput number is therefore PRINTED, not asserted.
///
/// The mean batch size measures the same mechanism without the clock. The
/// pre-pipeline loop read into a `[u8; 1024]`, so its batches cannot exceed
/// 1024 bytes — a HARD ceiling, not a measurement. The pipeline gathers into
/// 64 KiB slots. Measured, saturated: old shape exactly **1024.0** bytes,
/// this shape exactly **65536** bytes. The floor is 4096 — 4x above the old
/// shape's structural ceiling and 16x below this shape's observed value, and
/// it does not move when the machine is busy.
///
/// PROVEN TO FAIL AGAINST THE PRE-PIPELINE SHAPE: the old
/// `read`-into-`[u8; 1024]`/`processOutput` loop was spliced back into this
/// same tree and this test re-run — mean batch 1024, RED.
#[test]
#[cfg(target_os = "macos")]
fn readthread_pipeline_sustains_saturated_producer() {
    use std::time::{Duration, Instant};

    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    // A saturating write: many multiples of one 64 KiB gather buffer, so the
    // ring rotates repeatedly and the gather stage hits its backpressure
    // wait. Printable, non-control bytes only, so every byte lands in the
    // screen as itself (no VT interpretation, no OPOST translation).
    //
    // 2 MiB through an 80x24 screen scrolls ~1000 times and the SCREEN
    // cannot retain it, so the byte-exact assertion is made on what the
    // parse stage was handed (`processed_bytes`), and the screen is used
    // only as the liveness proof that the bytes really went through
    // `processOutput` rather than into a counter.
    const TOTAL: usize = 2 * 1024 * 1024;

    // The mean-batch-size floor, and how many samples we take. See the doc
    // comment for the measurements these come from.
    const MEAN_BATCH_FLOOR: f64 = 4096.0;
    #[cfg(not(debug_assertions))]
    const REPS: usize = 5;
    // One rep in dev: the same work takes ~22s there and the throughput
    // assertion is disabled anyway.
    #[cfg(debug_assertions)]
    const REPS: usize = 1;

    let mut best_mib_s = 0.0f64;
    let mut best_mean_batch = 0.0f64;
    for _ in 0..REPS {
        let (handle, master, slave, quit_write, t) = spawn_readthread_over_pty();
        let chunk: Vec<u8> = (0..4096u32).map(|i| b'a' + (i % 26) as u8).collect();

        let t0 = Instant::now();
        let writer = std::thread::spawn(move || {
            let mut written = 0usize;
            while written < TOTAL {
                let want = core::cmp::min(chunk.len(), TOTAL - written);
                let rc =
                    unsafe { libc::write(slave, chunk.as_ptr().cast::<std::ffi::c_void>(), want) };
                if rc <= 0 {
                    let e = std::io::Error::last_os_error();
                    if e.raw_os_error() == Some(libc::EINTR)
                        || e.raw_os_error() == Some(libc::EAGAIN)
                    {
                        continue;
                    }
                    break;
                }
                written += rc as usize;
            }
            written
        });

        // Bounded wait for the parse stage to consume everything the writer
        // produced. `processed_bytes` is the harness's tally of every byte
        // `processOutput` was handed.
        let written = writer.join().expect("writer join");
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut processed = 0usize;
        while Instant::now() < deadline {
            processed = t.processed_bytes();
            if processed >= written {
                break;
            }
            std::thread::sleep(Duration::from_micros(200));
        }
        let elapsed = t0.elapsed();
        let mib_s = processed as f64 / (1024.0 * 1024.0) / elapsed.as_secs_f64();
        if mib_s > best_mib_s {
            best_mib_s = mib_s;
        }
        let calls = t.processed_calls();
        let mean_batch = processed as f64 / calls.max(1) as f64;
        if mean_batch > best_mean_batch {
            best_mean_batch = mean_batch;
        }

        // Teardown BEFORE asserting, so a failure never leaks threads/fds.
        let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
        handle.join().expect("read thread join");
        unsafe {
            libc::close(quit_write);
            libc::close(master);
            libc::close(slave);
        }
        // The bytes must actually reach the Screen, not just a counter.
        let dump = t.screen_dump();
        let inked = dump.iter().any(|&b| b.is_ascii_lowercase());
        t.teardown();

        assert!(
            written >= TOTAL / 2,
            "the writer stalled: only {written} of {TOTAL} bytes reached the pty"
        );
        assert_eq!(
            processed, written,
            "every byte the producer wrote must reach processOutput exactly once \
             (lost/duplicated batch, or a stalled ring)"
        );
        assert!(inked, "the drained bytes must actually reach the Screen");
    }

    eprintln!(
        "PIPELINE: best of {REPS} = {best_mib_s:.2} MiB/s, \
         best mean batch = {best_mean_batch:.0} bytes"
    );

    assert!(
        best_mean_batch >= MEAN_BATCH_FLOOR,
        "mean batch size {best_mean_batch:.0} bytes is below the \
         {MEAN_BATCH_FLOOR} byte floor. The pre-pipeline loop read into a \
         [u8; 1024], so it CANNOT exceed 1024 here (measured: 1024.0); a \
         number in that range means the gather stage's batching is gone."
    );
}

/// A single sub-1 KiB write must reach the screen with interactive latency.
///
/// This is the LATENCY property `2f0e6659d` + `bb0ac4c72` exist for, and the
/// throughput suite cannot see it: a pipeline that batched aggressively
/// would ace throughput and ruin interactivity. Anything under
/// `bridge_threshold` (1 KiB) is by construction an interactive trickle that
/// must be delivered on the FIRST EAGAIN with no bridging at all, so the
/// only added latency is one condvar hop.
///
/// TWO BOUNDS, and the reason there are two:
///
/// * LIVENESS, on the worst of 8 writes: `gather_budget_ns` (3 ms) x 50 =
///   150 ms. Deliberately loose. This one only says "a small write is not
///   lost or wedged", and it is the bound a single scheduler hiccup on a
///   contended box must not trip.
/// * BATCHING, on the MEDIAN of 8 writes: `gather_budget_ns` / 10 = 300 us.
///   THIS is the guard. Median, not worst, because the regression it catches
///   moves EVERY write while contention moves one — so the median separates
///   the two causes that a worst-case bound conflates.
///
/// The 300 us figure is measured, not chosen (M4 Pro, real pty, `--release`,
/// box under load ~9, N=9 runs of 8 writes each):
///
///     healthy (this shape):        medians 22..46 us
///     bridging-always (mutated):   medians 1176..1212 us
///
/// 300 us sits ~6x above the healthy tail and ~4x below the degraded floor.
///
/// PROVEN TO FAIL: disabling BOTH early-delivery paths — the
/// `total < bridge_threshold` break (2f0e6659d) and the `count == 0` idle
/// break (bb0ac4c72) — moves the median to ~1.18 ms, over the bound.
/// Disabling only the first is NOT caught, and that is a real finding
/// recorded here rather than papered over: with an idle parser,
/// `bb0ac4c72`'s guard delivers immediately anyway, so the two paths are
/// genuinely redundant for THIS scenario and only their conjunction is
/// observable from outside.
///
/// RELEASE-ONLY for the batching bound: in a `dev` build the VT parse
/// dominates (see `readthread_pipeline_sustains_saturated_producer`) and the
/// microsecond-scale signal is buried. The liveness bound runs in both.
#[test]
#[cfg(target_os = "macos")]
fn readthread_pipeline_keeps_interactive_latency() {
    use std::time::{Duration, Instant};

    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (handle, master, slave, quit_write, t) = spawn_readthread_over_pty();

    // The two bounds. See the doc comment for the measurements behind them.
    const LIVENESS_FACTOR: u32 = 50;
    const BATCHING_DIVISOR: u32 = 10;
    let bound = Duration::from_nanos(ReadThread::GATHER_BUDGET_NS) * LIVENESS_FACTOR;
    let batching_bound = Duration::from_nanos(ReadThread::GATHER_BUDGET_NS) / BATCHING_DIVISOR;

    // Several separate small writes, each well under bridge_threshold, so
    // one lucky delivery cannot carry the test.
    let mut worst = Duration::ZERO;
    let mut lats: Vec<Duration> = Vec::new();
    for i in 0..8u8 {
        let needle: Vec<u8> = format!("LAT{i}").into_bytes();
        let start = Instant::now();
        let w = unsafe {
            libc::write(
                slave,
                needle.as_ptr().cast::<std::ffi::c_void>(),
                needle.len(),
            )
        };
        assert_eq!(w, needle.len() as isize, "wrote the needle");

        let deadline = start + bound;
        let mut seen = false;
        while Instant::now() < deadline {
            let dump = t.screen_dump();
            if dump.windows(needle.len()).any(|c| c == needle.as_slice()) {
                seen = true;
                break;
            }
            std::thread::yield_now();
        }
        let elapsed = start.elapsed();
        lats.push(elapsed);
        if elapsed > worst {
            worst = elapsed;
        }
        if !seen {
            // Teardown before failing.
            let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
            handle.join().expect("read thread join");
            unsafe {
                libc::close(quit_write);
                libc::close(master);
                libc::close(slave);
            }
            t.teardown();
            panic!(
                "interactive write {i} ({} bytes, under the {} byte bridge threshold) \
                 did not reach the screen within {bound:?}",
                needle.len(),
                1024,
            );
        }
    }

    let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };
    handle.join().expect("read thread join");
    unsafe {
        libc::close(quit_write);
        libc::close(master);
        libc::close(slave);
    }
    t.teardown();

    lats.sort();
    let median = lats[lats.len() / 2];
    eprintln!("PIPELINE-LATENCY: median = {median:?} worst = {worst:?}");
    assert!(
        worst < bound,
        "LIVENESS: worst interactive latency {worst:?} exceeded the bound \
         {bound:?} (gather_budget_ns x {LIVENESS_FACTOR})"
    );
    #[cfg(not(debug_assertions))]
    assert!(
        median < batching_bound,
        "BATCHING: median interactive latency {median:?} exceeded \
         {batching_bound:?} (gather_budget_ns / {BATCHING_DIVISOR}). Healthy \
         is 22-46us on this host; a bridging-always pipeline measures \
         ~1.18ms. Interactive writes are being batched."
    );
    #[cfg(debug_assertions)]
    let _ = batching_bound;
}

/// Both pipeline stages must shut down cleanly, from either side.
///
/// `E-os-termio-platform.md` row E1 warns that the parse stage gained two
/// new early returns. The hazards are (a) the `QuitFdCloser` Drop guard not
/// firing on one of them, and (b) the gather stage exiting without setting
/// `done`, which would leave the parse stage blocked on `batch_ready`
/// forever and hang the join. Both directions are exercised:
///
///   quit-during-gather: quit with the pty idle, so the gather stage is
///     asleep in its outer `poll` and the parse stage is asleep on
///     `batch_ready`. The gather stage's `DoneSignal` guard is the ONLY
///     thing that wakes the parse stage.
///   quit-during-parse: quit while a bulk stream is in flight, so the ring
///     is non-empty and the parse stage is mid-`processOutput`.
///
/// A regression in either direction hangs, so the join is bounded by a
/// watchdog channel rather than blocking forever.
///
/// THE BOUND IS A STALL BOUND, NOT A WALL-CLOCK BOUND (S-15, N293). On a
/// quit the parse stage drains every published batch before it exits, and
/// the gather stage keeps filling slots without polling `quit` while the
/// pty still yields full 64 KiB buffers. Both are the reference's design
/// (`Exec.zig` parse loop: `if (pipeline.done) return;` only once
/// `count == 0`; gather: `if (total == buffer_capacity) continue;`), so a
/// correct shutdown from quit-during-parse first parses what is still in
/// the ring and the pty queue. Measured on an M4 Pro, idle host, `dev`
/// profile, three runs: 145-170 KiB were still in flight at the quit and
/// the drain took 2.3-2.6 s, i.e. ~65 KiB/s. (The ~3 s before the quit is
/// the eight blocking 64 KiB writes waiting on the same parse.) The `dev`
/// parse is that slow because every page mutation runs
/// `Page::verify_integrity` (sampled: `page::mask`, `page::get_bits` and
/// `verify_integrity` are the top frames of the parse stage), which is the
/// reference's `assertIntegrity` under `slow_runtime_safety`
/// (`page.zig:366-376`, Debug builds only): faithful, not a port defect.
/// A fixed 10 s bound on the WHOLE drain therefore failed under host load
/// (load average 20-29, task S-6: 1 run in 8) while the pipeline was
/// healthy and still making progress.
///
/// The regression looks different: a stage that exits without waking the
/// other leaves the parse stage asleep on `batch_ready`, so no batch ever
/// reaches `processOutput` again. The watchdog therefore fails only when
/// the thread has not joined AND no new batch has reached `processOutput`
/// for `STALL`. A live drain hands over a batch every ~0.7-0.8 s here
/// idle (one 64 KiB slot at ~65 KiB/s; the longest gap is printed as
/// `SHUTDOWN-DRAIN`). Under a synthetic load of 42 spinning processes on
/// 14 cores (load average 46) the longest gap measured 4.1-4.7 s and the
/// whole drain up to 9.96 s, at the old fixed 10 s bound. STALL is
/// 30 s: over 6x the loaded gap, and a hang is still caught 30 s after its
/// last batch instead of wedging the suite. The drain is finite (at most the
/// 512 KiB written), so a livelock cannot hide behind the progress rule:
/// once every byte is processed, progress stops and the stall rule applies.
#[test]
#[cfg(target_os = "macos")]
fn readthread_pipeline_shuts_down_cleanly_from_both_stages() {
    use std::time::{Duration, Instant};

    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    for scenario in ["quit-during-gather", "quit-during-parse"] {
        let (handle, master, slave, quit_write, t) = spawn_readthread_over_pty();

        const CHUNK: usize = 64 * 1024;
        const CHUNKS: usize = 8;
        if scenario == "quit-during-parse" {
            // Put a bulk stream in flight so the ring is non-empty and the
            // parse stage is busy when the quit arrives.
            let chunk = vec![b'z'; CHUNK];
            for _ in 0..CHUNKS {
                let w = unsafe {
                    libc::write(
                        slave,
                        chunk.as_ptr().cast::<std::ffi::c_void>(),
                        chunk.len(),
                    )
                };
                assert_eq!(w, CHUNK as isize, "{scenario}: wrote a bulk chunk");
            }
            // The scenario's premise, checked rather than assumed: bytes
            // are still in flight (ring or pty queue) when the quit lands.
            let at_quit = t.processed_bytes();
            assert!(
                at_quit < CHUNK * CHUNKS,
                "{scenario}: all {at_quit} bytes were parsed before the quit, \
                 so this is not a quit during parse"
            );
        } else {
            // Idle pty: let both stages settle into their waits.
            std::thread::sleep(Duration::from_millis(20));
        }

        let _ = unsafe { libc::write(quit_write, b"x".as_ptr().cast::<std::ffi::c_void>(), 1) };

        // Bounded join: a hang here IS the regression, so surface it as a
        // failure rather than wedging the suite. The bound is STALL since
        // the last batch reached processOutput (see the doc comment).
        const STALL: Duration = Duration::from_secs(30);
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let joiner = std::thread::spawn(move || {
            handle.join().expect("read thread join");
            let _ = tx.send(());
        });
        let quit_at = Instant::now();
        let mut last_calls = t.processed_calls();
        let mut last_progress = quit_at;
        let mut longest_gap = Duration::ZERO;
        let joined = loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(()) => break true,
                // The joiner panicked (the read thread panicked): not joined.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break false,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
            let calls = t.processed_calls();
            if calls != last_calls {
                longest_gap = longest_gap.max(last_progress.elapsed());
                last_calls = calls;
                last_progress = Instant::now();
            } else if last_progress.elapsed() >= STALL {
                break false;
            }
        };
        eprintln!(
            "SHUTDOWN-DRAIN {scenario}: joined={joined} after {:?}, longest gap \
             between batches {longest_gap:?}",
            quit_at.elapsed()
        );

        if joined {
            joiner.join().expect("joiner join");
        }
        unsafe {
            libc::close(quit_write);
            libc::close(master);
            libc::close(slave);
        }
        if joined {
            t.teardown();
        } else {
            // The read thread is still alive and still owns `io`, so
            // `teardown()` (which moves the Termio out of its box and
            // invalidates its self-pointers) would be UB. Leaking the
            // harness is the only safe option on this path.
            //
            // `TestTermio` is not `Drop` — its `teardown` is an explicit
            // consuming method — so clippy is right that `mem::forget` is a
            // no-op here. The no-op IS the point: NOT calling `teardown` is
            // the leak, and the explicit forget documents that at the site.
            #[allow(clippy::forget_non_drop)]
            std::mem::forget(t);
        }

        assert!(
            joined,
            "{scenario}: the read pipeline did not shut down, and no batch \
             reached processOutput for {STALL:?} (a stage exited without \
             waking the other)"
        );
    }
}
