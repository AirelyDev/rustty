//! PORTS: ghostty src/termio/Exec.zig — the `Subprocess` spawn/stop and `killPid` KATs.

// (Every test here is gated off Windows, so the glob is unused there.)
#[cfg_attr(windows, allow(unused_imports))]
use super::*;

// WS19 T17a: `drain_pty_master_once` — the helper the `threadEnter` error arms
// call before `stop()`. Two properties, both load-bearing:
//
//   1. It ACTUALLY CONSUMES what the child already wrote (that is the whole
//      point — an undrained master is what wedges a SIGHUP'd session leader).
//   2. It is BOUNDED: on a QUIET master it returns promptly instead of parking
//      in `read` on a blocking fd. (The master is blocking, so a naive
//      implementation hangs here forever — this assertion is what forbids it.)
//
// Deliberately NOT a reproduction of the wedge itself: see the `killPid`
// divergence entry in DEFERRALS.md for why the previous such test was deleted
// (it asserted nothing that could fail, its only failure mode was hanging, and
// it stranded `?Es` children). Real openpty/poll/read syscalls -> Miri-excluded
// (MIRI_EXCEPTIONS.md, "rustty-termio B3"); holds pty fds open while other
// in-process KATs may `fork()`, so it takes FORK_KAT_SERIAL.
#[test]
#[cfg(target_os = "macos")]
fn drain_pty_master_once_consumes_pending_and_returns_on_a_quiet_master() {
    use std::time::Instant;

    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let pty = Pty::open(ptypkg::winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 800,
        ws_ypixel: 480,
    })
    .expect("Pty::open");
    let master = pty.master;
    let slave = pty.slave;

    // (2) A quiet master must return promptly and consume nothing. Do this
    //     FIRST, while nothing has been written.
    let t0 = Instant::now();
    let drained = drain_pty_master_once(master);
    let quiet_elapsed = t0.elapsed();
    assert_eq!(drained, 0, "nothing was written, so nothing may be drained");
    assert!(
        quiet_elapsed < DRAIN_BUDGET * 4,
        "drain on a QUIET master must return promptly (it would park in \
         `read` on the blocking master without the zero-timeout poll gate); \
         took {quiet_elapsed:?}"
    );

    // (1) Write to the slave (no newline -> no OPOST translation), so the bytes
    //     appear on the master exactly as written, then drain them.
    const PENDING: &[u8] = b"RUSTTY_T17A_UNDRAINED_OUTPUT";
    let w = unsafe { libc::write(slave, PENDING.as_ptr().cast(), PENDING.len()) };
    assert_eq!(w, PENDING.len() as isize, "write to the pty slave");

    // The kernel may deliver in pieces; drain until we have them all (each
    // call is itself bounded, and the loop has its own deadline).
    let mut total = 0usize;
    let deadline = Instant::now() + std::time::Duration::from_secs(2);
    while total < PENDING.len() && Instant::now() < deadline {
        total += drain_pty_master_once(master);
    }
    assert_eq!(
        total,
        PENDING.len(),
        "the drain must consume everything the child already wrote"
    );

    // And the master is quiet again.
    assert_eq!(drain_pty_master_once(master), 0);

    unsafe {
        libc::close(slave);
        libc::close(master);
    }
}

// WS19 T17b: `posix_waitpid_checked` must REPORT (`None`), not panic, when the
// wait fails — the `ECHILD` race a concurrent reaper can win. `pid 1` is never
// our child, so `waitpid` returns `ECHILD` deterministically with no fork and
// no process left behind. The verbatim `posix_waitpid` shim would
// `unreachable!()` on exactly this input, which is what made the race a
// reachable panic on the teardown path.
#[test]
#[cfg(not(windows))]
fn posix_waitpid_checked_reports_echild_instead_of_aborting() {
    // Not our child -> ECHILD.
    let got = crate::command::posix_waitpid_checked(1, libc::WNOHANG as u32);
    assert!(
        got.is_none(),
        "waiting on a pid that is not our child must report None (ECHILD), \
         not a WaitPidResult"
    );
}

// KAT (rustty-only margin — blueprint §7 B1; NOT a mapped Zig `test{}`, so
// the test-count contract is unaffected). Drives the real init -> start
// fork/exec spawn on a real pty and proves the pid-only handle model:
//   Subprocess::init -> start "sh -c 'exit 7'" -> assert PtyFds returned +
//   process is ForkExec(pid) with 'static-empty reference fields -> stop()
//   reaps (SIGHUP to the child pgroup + waitpid).
// Real fork/exec + openpty syscalls -> Miri-excluded (MIRI_EXCEPTIONS.md,
// "rustty-termio B1"); the parent-side scoped-borrow spawn is the
// Stacked-Borrows surface (no `&'static`-to-non-static at rest; the sole raw
// self-pointer derefs only in the forked child).
#[test]
#[cfg(target_os = "macos")]
fn subprocess_start_spawns_and_reaps() {
    use allocator_api2::alloc::Global;

    // Serialize with the other fork/pty KATs (see FORK_KAT_SERIAL).
    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    // Minimal Config: `sh -c 'exit 7'`, no resources dir (shell integration
    // disabled), no cwd, xterm-256color.
    let cfg = Config {
        command: Some(configpkg::command::Command::Shell(b"exit 7".to_vec())),
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

    let mut subprocess = Subprocess::init(Global, cfg).expect("Subprocess::init");

    // start() on a real pty: fork + exec the (login-wrapped) shell.
    let fds = subprocess.start(Global).expect("Subprocess::start");

    // PtyFds returned: read == write == the pty master (a valid fd).
    assert!(fds.read >= 0, "PtyFds carries a valid master fd");
    assert_eq!(fds.read, fds.write, "read/write are both the pty master");

    // [T8] Drain the pty master concurrently — the role the REAL termio
    // read thread plays during `threadExit` (which calls `stop()` while the
    // read thread is still draining; ghostty Exec.zig does the same).
    // Without a reader, a SIGHUP'd `login(1)` (raced mid-startup by
    // `stop()`) wedges UNINTERRUPTIBLY in kernel exit (state `E`,
    // SIGKILL-immune) draining its pending slave output to the master, and
    // `killPid`'s faithful `waitpid(WNOHANG)` loop then spins forever (an
    // E-state proc is not reapable) — the T8 CI hang. Proven on macOS
    // 26.5.1: reading ~37 pending bytes un-wedges the child instantly.
    // The drainer reads until EOF (all slave refs closed at reap), exactly
    // the upstream runtime condition; nothing this KAT asserts changed.
    //
    // [M2 / review follow-up] The drainer is itself BOUNDED. It polls with
    // a timeout and gives up after DRAIN_DEADLINE rather than blocking in
    // `read` until EOF. RISK IT REMOVES: if the child ever wedges anyway,
    // the bounded `killPid` abandons it WITHOUT reaping — so the slave fds
    // are never closed, the master never reaches EOF, and an unbounded
    // drainer would block forever in `read`. `drainer.join()` below would
    // then hang the test, reintroducing on the TEST side exactly the CI
    // hang the bounded `killPid` removes on the production side. The
    // deadline is far longer than any legitimate drain (which finishes in
    // microseconds) and longer than `KILL_ABANDON`, so it never masks a
    // real failure — it only guarantees `join()` returns.
    let master_fd = fds.read;
    let drainer = std::thread::spawn(move || {
        const DRAIN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
        let deadline = std::time::Instant::now() + DRAIN_DEADLINE;
        let mut buf = [0u8; 4096];
        while std::time::Instant::now() < deadline {
            // SAFETY: `master_fd` is the live pty master (closed only after
            // this thread is joined); `poll` writes only `revents`.
            let mut pfd = libc::pollfd {
                fd: master_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let pr = unsafe { libc::poll(&mut pfd, 1, 100) };
            if pr < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break;
            }
            if pr == 0 {
                // Timed out with nothing pending; re-check the deadline.
                continue;
            }
            // SAFETY: as above; `read` writes at most `buf.len()` bytes.
            let n = unsafe { libc::read(master_fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                // EIO on macOS == all slave refs closed, i.e. EOF.
                break;
            }
            if n == 0 {
                break;
            }
        }
    });

    // process is a pid-bearing ForkExec handle whose reference fields are
    // TRUE 'static empties (the pid-only handle, blueprint §2).
    match &subprocess.process {
        Some(Process::ForkExec(cmd)) => {
            assert!(cmd.pid.is_some(), "ForkExec handle carries a pid");
            assert!(cmd.args.is_empty(), "stored args are the 'static empty");
            assert!(cmd.env.is_none(), "stored env is None (no dangling borrow)");
            assert!(cmd.data.is_none(), "stored data self-pointer is cleared");
        }
        _ => panic!("expected process == ForkExec after start"),
    }

    // stop() sends SIGHUP to the child's process group and reaps it (must
    // NOT hang — the child setsid'd in childPreExec so getpgid resolves).
    unsafe { Subprocess::stop(&mut subprocess) };
    assert!(subprocess.process.is_none(), "stop() clears the process");

    // The child is reaped, so every slave fd is closed and the drainer has
    // hit EOF; join it BEFORE closing the master (its fd copy). Bounded by
    // DRAIN_DEADLINE above, so this join always returns.
    drainer.join().expect("drainer join");

    // Close the master (read/write) fd so the test leaks nothing.
    unsafe {
        libc::close(fds.read);
    }

    // deinit is safe post-stop.
    subprocess.deinit();
}

// ---- task R13-P: NUL bytes in the cwd and in the argv (N50, N51) --------
//
// `Subprocess::init` used to build the cwd with `cstring`, which `.expect`ed
// on an interior NUL. A program that prints `OSC 7` with `%00` in the path
// plants one in the terminal's pwd, the next new tab/split/window inherits
// it as `working-directory`, and the panic aborted the macOS app. The
// reference keeps the bytes, asks `access` about them, and takes its
// `cannot access cwd, ignoring` arm (Zig 0.16 `pathToPosix` answers
// `error.BadPathName`; measured with `r13p-reference-probe.zig`).

/// Spawns `exit 0` with `working_directory` set to `wd`, and reports the
/// `cannot access cwd` warnings `start` logged on this thread, the `PWD` it
/// put in the child environment, and the pwd `initTerminal` set on a fresh
/// terminal. Drives the real `Exec::init` -> `initTerminal` -> `start`
/// fork/exec path; only `new_config` itself is out of reach from here,
/// because it needs a core `App` with an apprt.
#[cfg(unix)]
fn spawn_with_working_directory(wd: Vec<u8>) -> (Vec<String>, Option<Vec<u8>>, Option<Vec<u8>>) {
    use allocator_api2::alloc::Global;

    let cfg = Config {
        command: Some(configpkg::command::Command::Shell(b"exit 0".to_vec())),
        env: EnvMap::new(),
        env_override: configpkg::RepeatableStringMap::default(),
        shell_integration: configpkg::value_types::ShellIntegration::Detect,
        shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
        cursor_blink: None,
        working_directory: Some(wd),
        resources_dir: None,
        term: b"xterm-256color".to_vec(),
        rt_pre_exec_info: RtPreExecInfo::default(),
        rt_post_fork_info: RtPostForkInfo::default(),
    };

    // Before the fix this panicked (`.expect` on the interior NUL).
    let mut exec = Exec::init(Global, cfg).expect("Exec::init");

    // `if (cwd) |pwd| try env.put("PWD", pwd)` — the reference stores the
    // bytes, and the child's envp is cut at the NUL by the kernel (measured).
    let env_pwd = exec
        .subprocess
        .env
        .as_ref()
        .and_then(|e| e.get(b"PWD".as_slice()).cloned());

    // `if (self.subprocess.cwd) |cwd| term.setPwd(cwd)`.
    let mut term = rustty_vt::terminal::Terminal::init(
        Global.into(),
        crate::stream_handler::tests::terminal_opts(None),
    )
    .expect("Terminal::init");
    exec.initTerminal(&mut term);
    let term_pwd = term.get_pwd().map(<[u8]>::to_vec);
    term.deinit(Global.into());

    let cap = crate::log_capture::start();
    let fds = exec.subprocess.start(Global).expect("Subprocess::start");
    let warnings: Vec<String> = cap
        .messages()
        .into_iter()
        .filter(|(level, target, message)| {
            *level == log::Level::Warn
                && target == LOG_SCOPE
                && message.starts_with("cannot access cwd")
        })
        .map(|(_, _, message)| message)
        .collect();
    drop(cap);

    // Drain the pty master while the child is reaped (the KAT's [T8] note).
    let master_fd = fds.read;
    let drainer = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut buf = [0u8; 4096];
        while std::time::Instant::now() < deadline {
            // SAFETY: `master_fd` is the live pty master; `poll` writes only
            // `revents`, and `read` writes at most `buf.len()` bytes.
            let mut pfd = libc::pollfd {
                fd: master_fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let pr = unsafe { libc::poll(&mut pfd, 1, 100) };
            if pr < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break;
            }
            if pr == 0 {
                continue;
            }
            let n = unsafe { libc::read(master_fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break;
            }
        }
    });

    unsafe { Subprocess::stop(&mut exec.subprocess) };
    drainer.join().expect("drainer join");
    // SAFETY: the master fd is ours and is closed exactly once.
    unsafe { libc::close(fds.read) };
    exec.subprocess.deinit();

    (warnings, env_pwd, term_pwd)
}

/// B1. `OSC 7` with `%00` -> the terminal pwd -> the new surface's
/// `working-directory` -> `Exec`. The port used to panic in `Exec::init`.
#[test]
#[cfg(unix)]
fn osc7_percent_00_pwd_is_ignored_as_a_cwd_instead_of_panicking() {
    use allocator_api2::alloc::Global;
    use std::os::unix::ffi::OsStrExt as _;

    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    // A REAL directory at the prefix before the NUL: a port that silently cut
    // the path there would start the child in it and log no warning.
    let root = std::env::temp_dir().join(format!("rustty-r13p-b1-{}", std::process::id()));
    let prefix = root.join("a");
    std::fs::create_dir_all(&prefix).expect("create the prefix directory");
    let prefix_bytes = prefix.as_os_str().as_bytes().to_vec();

    // The VT stream half: `ESC ] 7 ; file://localhost/<prefix>%00b BEL`.
    let mut term = rustty_vt::terminal::Terminal::init(
        Global.into(),
        crate::stream_handler::tests::terminal_opts(None),
    )
    .expect("Terminal::init");
    let h = crate::stream_handler::tests::handler_for(
        &raw mut term,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    let mut s = rustty_vt::stream::Stream::init(rustty_vt::stream::Options::with_handler(h));
    let mut seq = b"\x1b]7;file://localhost".to_vec();
    seq.extend_from_slice(&prefix_bytes);
    seq.extend_from_slice(b"%00b\x07");
    s.next_slice(&seq);
    s.handler.renderer_wakeup.deinit();

    let mut expected = prefix_bytes.clone();
    expected.extend_from_slice(b"\x00b");
    let pwd = term.get_pwd().map(<[u8]>::to_vec);
    assert_eq!(
        Some(expected.clone()),
        pwd,
        "OSC 7 percent-decodes %00 into the pwd, NUL and all"
    );
    term.deinit(Global.into());

    // `app.newConfig`: `copy.@"working-directory" = .{ .path = pwd }`, then
    // `Surface.init`: `if (config.@"working-directory") |wd| wd.value()`.
    let wd = configpkg::value_types::WorkingDirectory::Path(pwd.unwrap());
    let wd_bytes = wd.value().map(<[u8]>::to_vec).expect("a .path value");

    let (warnings, env_pwd, term_pwd) = spawn_with_working_directory(wd_bytes);
    assert_eq!(
        vec!["cannot access cwd, ignoring: error.BadPathName".to_string()],
        warnings,
        "start takes the reference's `cannot access cwd` arm and spawns with no cwd"
    );
    assert_eq!(Some(expected.clone()), env_pwd, "PWD keeps the raw bytes");
    assert_eq!(
        Some(expected),
        term_pwd,
        "initTerminal seeds the new terminal with the same bytes"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// B2. The same sink, reached from a `working-directory` config line whose
/// value holds a raw NUL byte. The config line reader keeps it.
#[test]
#[cfg(unix)]
fn config_working_directory_with_a_nul_is_ignored_as_a_cwd() {
    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let mut cfg = configpkg::Config::default_config();
    let mut iter = rustty_args::args::LineIterator::init(b"working-directory = /tmp\x00x\n");
    cfg.load_iter(&mut iter).expect("load_iter");
    let wd = cfg
        .working_directory
        .as_ref()
        .and_then(|w| w.value())
        .map(<[u8]>::to_vec);
    assert_eq!(
        Some(b"/tmp\x00x".to_vec()),
        wd,
        "the config parser keeps the NUL byte"
    );

    let (warnings, env_pwd, _) = spawn_with_working_directory(wd.unwrap());
    assert_eq!(
        vec!["cannot access cwd, ignoring: error.BadPathName".to_string()],
        warnings
    );
    assert_eq!(Some(b"/tmp\x00x".to_vec()), env_pwd);
}

/// The counter the KAT's stand-in `postFork` bumps. Global because the hook
/// is a plain `fn` pointer (the reference's `PostForkFn` shape); the KAT
/// holds `FORK_KAT_SERIAL` and asserts on a DELTA, so a sibling spawn KAT
/// that also trips the hook cannot perturb it.
#[cfg(target_os = "macos")]
static KAT_POST_FORK_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Stands in for `apprt.runtime.pre_exec.preExec` (`Exec.zig:1034`). Runs in
/// the FORKED CHILD, so it does nothing observable and does not exit.
#[cfg(target_os = "macos")]
fn kat_pre_exec(_cmd: &mut Command<'_>) -> Option<u8> {
    None
}

/// Stands in for `apprt.runtime.post_fork.postFork` (`Exec.zig:1036`). Runs
/// in the PARENT (`Command::start`, `Command.zig:197`), so its effect is
/// visible to the KAT.
#[cfg(target_os = "macos")]
fn kat_post_fork(cmd: &mut Command<'_>) -> Result<(), crate::command::PostForkError> {
    // The reference's `postFork` reads `cmd.pid` (`post_fork.zig:53`) — assert
    // the parent-side handle really is populated by the time it is called.
    assert!(cmd.pid.is_some(), "postFork runs in the parent, after fork");
    KAT_POST_FORK_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

// KAT (rustty-only margin; V13) — THE `rt_*` HOOK WIRING.
//
// Proves that the hooks an apprt installs with
// `command::install_rt_hooks` actually reach the `Command` that `Subprocess::
// start` forks, i.e. that this file's `rt_pre_exec: rt_hooks.pre_exec` /
// `rt_post_fork: rt_hooks.post_fork` really are the port's answer to
// `Exec.zig:1034-1037`'s `@hasDecl(apprt.runtime, ...)`. Before V13 both
// fields were the literal `None` and NOTHING could call the (fully ported,
// fully unit-tested) GTK `preExec`/`postFork`.
//
// Only the POST-fork hook is observable from the test: `preExec` runs in the
// forked child, whose side effects die with the `exec` (that is the whole
// point of the reference's split). The two are set from the same
// `rt_hooks()` read at the same construction site, so the post-fork
// observation covers both wires.
//
// Real fork/exec + openpty -> Miri-excluded with its siblings; gated to
// macOS exactly like `subprocess_start_spawns_and_reaps` above (the spawn
// KATs run on the dev host). The code under test is NOT cfg'd — the same
// `rt_hooks()` read serves every platform.
#[test]
#[cfg(target_os = "macos")]
fn rt_hooks_reach_the_spawned_command() {
    use allocator_api2::alloc::Global;
    use std::sync::atomic::Ordering;

    // Serialize with the other fork/pty KATs (see FORK_KAT_SERIAL).
    let _serial = FORK_KAT_SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    // The apprt's job (`gtk::App::init` does exactly this call). Idempotent
    // for the SAME pair, so re-running this KAT in a retry is fine.
    crate::command::install_rt_hooks(crate::command::RtHooks {
        pre_exec: Some(kat_pre_exec),
        post_fork: Some(kat_post_fork),
    });

    let base = KAT_POST_FORK_CALLS.load(Ordering::SeqCst);

    // `/bin/echo` — bounded output, self-terminating (no drainer needed for
    // ~14 bytes, unlike the SIGHUP'd `login(1)` of the sibling KAT).
    let cfg = Config {
        command: Some(configpkg::command::Command::Direct(vec![
            b"/bin/echo".to_vec(),
            b"rt-hooks-kat".to_vec(),
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

    let mut subprocess = Subprocess::init(Global, cfg).expect("Subprocess::init");
    let fds = subprocess.start(Global).expect("Subprocess::start");

    // THE ASSERTION: the installed `postFork` ran, exactly once, during this
    // `start`. (`None` here — the pre-V13 state — makes this 0.)
    assert_eq!(
        KAT_POST_FORK_CALLS.load(Ordering::SeqCst) - base,
        1,
        "the installed rt_post_fork hook must be called by Command::start"
    );

    unsafe { Subprocess::stop(&mut subprocess) };
    // SAFETY: `fds.read` is the live pty master this KAT owns; closing it
    // once, after `stop()` reaped the child, leaks nothing.
    unsafe {
        libc::close(fds.read);
    }
    subprocess.deinit();
}

/// UD93 (R13, task R13-D): a child that survives SIGHUP. The reference's
/// `killPid` spins until the child is reaped, forever for one that ignores
/// the signal. The port abandons it at the deadline. The child here ignores
/// SIGHUP from before its `exec` (set in `pre_exec`, so there is no race with
/// the first `killpg`) and is in its own session, as the real child is after
/// `setsid`, so `killpg` never reaches the test process. The deadline is
/// 300 ms (production: `KILL_ABANDON`); a stuck `killPid` is detected by a
/// 10 s watchdog, which SIGKILLs the child so the thread can finish.
#[cfg(not(windows))]
#[test]
fn kill_pid_abandons_a_child_that_ignores_sighup() {
    use std::os::unix::process::CommandExt as _;
    let mut cmd = std::process::Command::new("/bin/sleep");
    cmd.arg("60");
    // SAFETY: `setsid` and `signal` are async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            Ok(())
        });
    }
    let child = cmd.spawn().expect("spawn sleep");
    let pid = child.id() as libc::pid_t;
    // `spawn` returns after `exec`, so `setsid` has run: the group is the
    // child's own, never the test runner's.
    assert_eq!(pid, unsafe { libc::getpgid(pid) });

    const DEADLINE: std::time::Duration = std::time::Duration::from_millis(300);
    let (tx, rx) = std::sync::mpsc::channel();
    let started = std::time::Instant::now();
    let worker = std::thread::spawn(move || {
        let _ = tx.send(kill_pid_until(pid, DEADLINE));
    });
    let result = rx.recv_timeout(std::time::Duration::from_secs(10));
    let elapsed = started.elapsed();
    let alive = unsafe { libc::kill(pid, 0) } == 0;
    // Clean up whatever happened: SIGKILL ends the child, which lets a stuck
    // loop reap it and return; then reap it here (an error just means the
    // loop already did).
    let mut child = child;
    let _ = child.kill();
    worker.join().unwrap();
    let _ = child.wait();

    let result = result.unwrap_or_else(|_| {
        panic!("killPid did not return within 10 s for a child that ignores SIGHUP")
    });
    assert!(
        matches!(result, Err(KillError::Abandoned)),
        "the child is abandoned at the deadline, got {result:?}"
    );
    assert!(
        elapsed >= DEADLINE,
        "the loop kept signalling until the deadline ({elapsed:?})"
    );
    assert!(
        alive,
        "the child survived every SIGHUP, so the loop never reaped it"
    );
}

// [I2 / review follow-up] REMOVED: `kill_pid_bounded_when_child_is_unreapable`.
//
// An earlier revision carried an in-suite "bound regression guard" that
// deliberately reproduced the WEDGING condition (start a real pty child,
// then `stop()` with NO drainer) inside the DEFAULT `cargo test`. It was
// deleted, not weakened:
//
//   * It asserted nothing useful. Whether the child actually wedges is
//     timing-dependent, so it reaped in milliseconds on ordinary runs, and
//     its threshold (`KILL_ABANDON + 20s`) was loose enough that tripling
//     `KILL_ABANDON` would still have passed it.
//   * Its only failure mode was HANGING — which is precisely the CI failure
//     this whole work exists to eliminate.
//   * It reintroduced the process-stranding hazard (an abandoned `?Es`
//     `login(1)` on the developer's machine) on every test run.
//
// A deterministic replacement was considered and rejected as well: the only
// shape that would deterministically exercise "new logic" is a child that
// survives SIGHUP but not SIGKILL, and after the I1 decision there IS no
// SIGKILL escalation left to exercise — the sole divergence is a wall-clock
// deadline, which cannot be asserted without spending `KILL_ABANDON`
// seconds and stranding a child.
//
// The deterministic evidence lives out-of-suite, where it belongs:
// `.port/tools/repro_subprocess_hang.sh` (pre-fix 12/12 hangs, post-fix
// 0 hangs) plus the `killPid bounded wait` divergence entry in
// `.port/phase-2/DEFERRALS.md`.
//
// (Task R13-D, UD93: the two objections above are answered by
// `kill_pid_abandons_a_child_that_ignores_sighup`. The deadline is now a
// parameter of `kill_pid_until`, so the test waits 300 ms rather than
// `KILL_ABANDON`, and its child ignores SIGHUP from before `exec` rather
// than wedging, so it is SIGKILLable and is killed and reaped afterwards.
// It exercises the deadline itself; the `?Es` wedge stays out-of-suite.)

/// S212-F1: the pty-exhaustion failure has a real producer. With the
/// process's descriptor table full, `Subprocess::start`'s `Pty.open`
/// (`openpty`) fails, and `start` returns `Error::OpenptyFailed` (the
/// member `Thread.threadMain` turns into the pty-exhaustion screen), not
/// the general `Error::Other`. The table is filled in a re-executed child
/// copy of this test binary, so no other test runs out of descriptors.
#[test]
#[cfg(target_os = "macos")]
fn subprocess_start_with_no_free_descriptor_is_openpty_failed() {
    use allocator_api2::alloc::Global;

    const CHILD: &str = "RUSTTY_TERMIO_OPENPTY_EXHAUSTION_CHILD";
    const TEST: &str =
        "exec::tests::subprocess::subprocess_start_with_no_free_descriptor_is_openpty_failed";
    if std::env::var_os(CHILD).is_none() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .expect("re-exec the test binary");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "the child run failed ({:?})\nstdout:\n{stdout}\nstderr:\n{stderr}",
            out.status
        );
        assert!(
            stdout.contains("test result: ok. 1 passed; 0 failed"),
            "the child ran exactly this test\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        return;
    }

    // Child: a fresh process, one test thread.
    let cfg = Config {
        command: Some(configpkg::command::Command::Direct(vec![
            b"/usr/bin/true".to_vec()
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

    // Fill the descriptor table: a low soft limit, then dup until EMFILE.
    let mut old = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut old) }, 0);
    let low = libc::rlimit {
        rlim_cur: 256,
        rlim_max: old.rlim_max,
    };
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &low) }, 0);
    let mut filler: Vec<libc::c_int> = Vec::new();
    loop {
        let fd = unsafe { libc::dup(2) };
        if fd < 0 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::EMFILE),
                "the table fills up"
            );
            break;
        }
        filler.push(fd);
    }
    assert!(!filler.is_empty(), "descriptors were taken");

    let result = exec.subprocess.start(Global);

    for fd in filler {
        unsafe {
            libc::close(fd);
        }
    }
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &old) }, 0);

    assert!(
        matches!(result, Err(crate::Error::OpenptyFailed)),
        "a full descriptor table makes start fail with OpenptyFailed, got {:?}",
        result.as_ref().map(|_| ()).map_err(|e| *e)
    );
    assert!(exec.subprocess.pty.is_none(), "no pty was kept");
    exec.deinit();
}
