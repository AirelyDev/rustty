//! PORTS: ghostty src/termio/Exec.zig — `killCommand`/`killPid` (with the port's bounded kill wait) and
//! `killCommandFlatpak`.

use super::*;

/// PORT NOTE (Rust-only glue): the error set of `killCommand`/`killPid` —
/// `error.KillFailed` + the windows `unexpectedError`.
#[derive(Debug)]
pub enum KillError {
    KillFailed,
    #[allow(dead_code)]
    Unexpected(i32),
    /// [R1-DIVERGENCE, liveness] `killPid` gave up waiting for a child that
    /// cannot be reaped. See `KILL_ABANDON` and the divergence note on
    /// `killPid`. Non-fatal: the caller (`stop`) proceeds with teardown.
    #[allow(dead_code)] // M4: never constructed on windows (killPid is posix-only).
    Abandoned,
}

// ── [R1-DIVERGENCE: bounded kill wait] ──────────────────────────────────────
//
// DELIBERATE DIVERGENCE FROM THE ORACLE. Recorded in
// `.port/phase-2/DEFERRALS.md` ("killPid bounded wait"). Rationale:
//
// The reference `killPid` loops FOREVER — SIGHUP the process group, then
// `waitpid(WNOHANG)`, sleep 10ms, repeat — until the child is reaped. On macOS
// a child can enter a state where it is NEVER reapable: a `login(1)` (a session
// leader) SIGHUP'd while it still has output queued on its pty slave wedges
// UNINTERRUPTIBLY in the kernel exit path, state `?Es`. It is immune to SIGKILL
// and `waitpid` will never return it. The reference loop then spins forever,
// and — because this runs on the teardown path — the app never finishes closing.
//
// Reachability in PRODUCTION (why fidelity does not win here): the normal close
// path is safe, because `Exec::threadExit` calls `stop()` while the io-reader
// thread is still draining the pty master (upstream orders it that way on
// purpose), which is exactly what un-wedges the child. BUT `threadEnter`'s
// error arms also call `stop()` — and they run BEFORE the read thread exists
// (`pipe()` / `xev::Timer::init` / `xev::Process::init` / read-thread `spawn`
// failure, i.e. fd- or thread-exhaustion). On those arms the child is a
// freshly-forked `login` with pending output and NO reader: precisely the
// wedging condition. That is a genuine, if narrow, "rustty hangs on teardown
// and can never be closed" bug — a liveness defect, which this port treats as
// outranking strict R1 fidelity. Upstream Ghostty has the same ordering and so
// presumably the same latent bug.
//
// SHAPE OF THE DIVERGENCE — exactly one deadline, no signal change:
//
//   * 0 .. KILL_ABANDON — VERBATIM reference behavior: `killpg(pgid, SIGHUP)`
//                         + `waitpid(WNOHANG)` + 10ms sleep, repeated. Not one
//                         signal, argument or ordering differs.
//   * at KILL_ABANDON   — stop waiting and return `KillError::Abandoned`
//                         instead of looping forever. The child is left alive
//                         (or wedged) untracked and — on macOS — reparented to
//                         launchd once we exit; the caller (`stop`) proceeds
//                         with teardown. On Linux/FreeBSD there is NO launchd
//                         and NO reparenting at abandon time: rustty remains
//                         the parent of the forgotten pid, and a child that
//                         exits later stays an unreaped zombie until the app
//                         itself exits (nothing ever waits on it again).
//                         [Codex W4-02]
//
// PRECISE GUARANTEE (do not over-claim this): rustty is observationally
// identical to the oracle for any teardown that COMPLETES WITHIN
// `KILL_ABANDON`. It is NOT identical for teardowns that take longer — for
// those the oracle keeps SIGHUPing forever and rustty stops waiting. Two
// distinguishable cases live past the deadline:
//
//   1. The child is `?Es`-wedged and therefore UNREAPABLE. Waiting longer
//      provably cannot succeed, so abandoning loses nothing. This is the case
//      the divergence exists for.
//   2. The child is alive and reapable but slow — a shell trapping/ignoring
//      SIGHUP, a `zshexit`/history flush over a stalled network filesystem, an
//      ssh multiplexer or DB client flushing state. ACCEPTED CONSEQUENCE: we
//      stop waiting for it and no longer collect its exit status; on macOS it
//      is reparented to launchd once we exit, on Linux/FreeBSD it remains our
//      unreaped child (see above). Note what we deliberately do NOT do: we do not
//      kill it. Its cleanup runs to completion, so there is no data loss, and
//      the signals it received are exactly the ones the oracle would have sent.
//
// WHY THERE IS NO SIGKILL ESCALATION (a rejected earlier design; see the
// DEFERRALS.md entry for the full reasoning). An intermediate version escalated
// the process-group signal to SIGKILL after a 5s grace period. It was removed
// because it is useless where it was needed and harmful where it was not:
//
//   * For the `?Es` case it does NOTHING. An `E`-state process has already
//     entered the kernel exit path; it is SIGKILL-immune (this is the very
//     property that makes the wedge unreapable). Liveness there comes entirely
//     from the deadline, never from the signal.
//   * For case 2 above it converts "slow cleanup that eventually finishes"
//     into "killed mid-cleanup", i.e. a truncated history file or a half-
//     flushed client — real user data loss, in a case that is NOT pathological
//     and where the oracle would have waited.
//   * Its only genuine beneficiary is a child that ignores SIGHUP but is
//     killable, which the escalation would kill rather than abandon. The
//     oracle never kills such a child either, so abandoning it is closer to
//     the reference, and the alternative (inventing a SIGKILL the user's shell
//     never asked for) is not worth the data-loss exposure in case 2 — the two
//     are indistinguishable from inside `killPid`.
//
// The result is the MINIMAL divergence that achieves liveness: a single
// deadline, added to an otherwise byte-for-byte faithful loop.
//
/// Hard deadline after which `killPid` stops blocking the teardown.
/// Everything before it is verbatim reference behavior.
#[cfg(not(windows))]
const KILL_ABANDON: std::time::Duration = std::time::Duration::from_secs(10);

/// Wall-clock budget for one synchronous pty-master drain
/// ([`drain_pty_master_once`]). Generous enough to clear a shell's startup
/// output, small enough that a `threadEnter` error arm cannot become a stall.
#[cfg(not(windows))]
pub(super) const DRAIN_BUDGET: std::time::Duration = std::time::Duration::from_millis(50);

/// Drain whatever the child has ALREADY written to the pty master, once,
/// synchronously, discarding the bytes. Returns how many were consumed.
///
/// **Why (WS19 T17a, `[R1-DIVERGENCE]`).** `Exec::threadEnter`'s error arms —
/// `ProcessNotStarted` / `ProcessNoPid` / `xev::Process::init` / `pipe()` /
/// `xev::Timer::init` / read-thread `spawn` — all call `self.subprocess.stop()`
/// BEFORE the io-reader thread exists. At that moment the child is a
/// freshly-forked `login(1)` with output queued on the pty slave and NOBODY
/// reading the master, which is exactly the condition under which a SIGHUP'd
/// session leader wedges uninterruptibly in the macOS kernel exit path (state
/// `?Es`) and becomes permanently unreapable. The normal close path gets the
/// invariant for free — `threadExit` calls `stop()` while the reader is still
/// draining, and upstream orders it that way deliberately — but these arms do
/// not. Upstream Ghostty has the SAME ordering and therefore the same latent
/// defect (re-verified against upstream head:
/// `.port/phase-3/assessments/E-os-termio-platform.md` §5.3 — those `errdefer`s
/// are unchanged context lines), so this is a deliberate divergence, and one we
/// intend to upstream.
///
/// It is the same trade the bounded `killPid` already made: liveness over R1. It
/// is also strictly weaker than that one — it changes no signal, no ordering
/// observable to the child, and nothing at all on any path that succeeds; it only
/// consumes bytes that were about to be discarded anyway (the arm is tearing the
/// subprocess down).
///
/// **Bounded, so it can never become the hang it prevents:** every iteration
/// re-checks a `DRAIN_BUDGET` deadline, and each read is gated by a
/// zero-timeout `poll` so nothing ever blocks on a quiet master.
///
/// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD102 (the
/// `threadEnter` errdefers that stop the child with no reader), with UD93 (the
/// unbounded `killPid` wait it would hang on). The wedge itself was measured on
/// the port side at an earlier date, not re-measured at the pin.
#[cfg(not(windows))]
pub(super) fn drain_pty_master_once(fd: libc::c_int) -> usize {
    #[cfg(test)]
    {
        tests::DRAIN_CALLS.with(|c| c.set(c.get() + 1));
        tests::STOPS_BEFORE_DRAIN
            .with(|c| c.set(Some(tests::STOP_CALLS.with(std::cell::Cell::get))));
    }
    let deadline = std::time::Instant::now() + DRAIN_BUDGET;
    let mut buf = [0u8; 4096];
    let mut total: usize = 0;

    loop {
        if std::time::Instant::now() >= deadline {
            break;
        }

        // Is anything readable RIGHT NOW? The master is blocking, so a bare
        // `read` on a quiet fd would park us here.
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut pfd, 1, 0) };
        if rc < 0 {
            if errno() == libc::EINTR {
                continue;
            }
            break;
        }
        if rc == 0 || (pfd.revents & libc::POLLIN) == 0 {
            // Nothing pending (or HUP/ERR only) — drained.
            break;
        }

        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            if errno() == libc::EINTR {
                continue;
            }
            // EAGAIN / EIO (the slave closed) — done.
            break;
        }
        if n == 0 {
            // EOF.
            break;
        }
        total += n as usize;
    }

    total
}

/// Kill the underlying subprocess. This sends a SIGHUP to the child
/// process. This also waits for the command to exit and will return the
/// exit code.
//
// fn killCommand(command: *Command) !void {
//
// PORT NOTE: reference file-private fn nested in Subprocess; hoisted to module
// level (Rust impl blocks cannot host the windows/posix cfg split as cleanly).
pub(super) fn killCommand(command: &mut Command<'static>) -> Result<(), KillError> {
    // if (command.pid) |pid| {
    if let Some(pid) = command.pid {
        // switch (builtin.os.tag) {
        //     .windows => {
        //         if (windows.kernel32.TerminateProcess(pid, 0) == 0) {
        //             return windows.unexpectedError(windows.kernel32.GetLastError());
        //         }
        //
        //         _ = try command.wait(false);
        //     },
        //
        //     else => try killPid(pid),
        // }
        #[cfg(windows)]
        {
            if unsafe { rustty_os::windows::exp::kernel32::TerminateProcess(pid, 0) } == 0 {
                return Err(KillError::Unexpected(unsafe {
                    rustty_os::windows::kernel32::GetLastError() as i32
                }));
            }

            let _ = command.wait(false);
        }
        #[cfg(not(windows))]
        killPid(pid)?;
    }
    Ok(())
}

// fn killPid(pid: c.pid_t) !void {
//
// PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD93. The reference
// loop (`Exec.zig:1186-1214`) has no exit but reaping the child, so a child that
// survives SIGHUP — one that ignores it, a slow cleanup, or a macOS `login(1)`
// wedged in the kernel exit path (`?Es`) — keeps `killPid` spinning forever and
// the surface never finishes closing. The port's `KILL_ABANDON` deadline (the
// `[R1-DIVERGENCE: bounded kill wait]` note above) is that safe handling; the
// deadline is a parameter of `kill_pid_until` so a test can drive it without
// waiting ten seconds. Production always passes `KILL_ABANDON`.
#[cfg(not(windows))]
fn killPid(pid: libc::pid_t) -> Result<(), KillError> {
    kill_pid_until(pid, KILL_ABANDON)
}

/// The body of [`killPid`], with the abandon deadline as a parameter (UD93).
#[cfg(not(windows))]
pub(super) fn kill_pid_until(
    pid: libc::pid_t,
    abandon: std::time::Duration,
) -> Result<(), KillError> {
    // const pgid = getpgid(pid) orelse return;
    let pgid = match getpgid(pid) {
        Some(pgid) => pgid,
        None => return Ok(()),
    };

    // It is possible to send a killpg between the time that
    // our child process calls setsid but before or simultaneous
    // to calling execve. In this case, the direct child dies
    // but grandchildren survive. To work around this, we loop
    // and repeatedly kill the process group until all
    // descendents are well and truly dead. We will not rest
    // until the entire family tree is obliterated.
    // while (true) {
    //
    // [R1-DIVERGENCE] the reference loop is unbounded; this one is bounded by
    // KILL_ABANDON. The signal sent is unchanged (always SIGHUP, as in the
    // reference). See the divergence note above.
    let started = std::time::Instant::now();
    loop {
        let elapsed = started.elapsed();

        // switch (posix.errno(c.killpg(pgid, c.SIGHUP))) {
        //     .SUCCESS => log.debug("process group killed pgid={}", .{pgid}),
        //     else => |err| killpg: { ... },
        // }
        let rc = unsafe { libc::killpg(pgid, libc::SIGHUP) };
        if rc == 0 {
            // log.debug("process group killed pgid={}", .{pgid});
            log::debug!(target: LOG_SCOPE, "process group killed pgid={pgid}");
        } else {
            'killpg: {
                let err = errno();
                // if ((comptime builtin.target.os.tag.isDarwin()) and
                //     err == .PERM)
                // {
                //     log.debug("killpg failed with EPERM, expected on Darwin and ignoring", .{});
                //     break :killpg;
                // }
                //
                // PORT NOTE: `comptime builtin.target.os.tag.isDarwin()` is a
                // `const` fold of the cfg rather than a `cfg` attribute on the
                // `if`, so the `break 'killpg` survives on every target — a
                // labelled block whose label is never named warns off Darwin.
                const IS_DARWIN: bool = cfg!(any(target_os = "macos", target_os = "ios"));
                if IS_DARWIN && err == libc::EPERM {
                    // log.debug("killpg failed with EPERM, expected on Darwin and ignoring", .{});
                    log::debug!(
                        target: LOG_SCOPE,
                        "killpg failed with EPERM, expected on Darwin and ignoring"
                    );
                    break 'killpg;
                }

                // log.warn("error killing process group pgid={} err={}", .{ pgid, err });
                log::warn!(
                    target: LOG_SCOPE,
                    "error killing process group pgid={pgid} err={}",
                    std::io::Error::from_raw_os_error(err)
                );
                return Err(KillError::KillFailed);
            }
        }

        // See Command.zig wait for why we specify WNOHANG.
        // The gist is that it lets us detect when children
        // are still alive without blocking so that we can
        // kill them again.
        // const res = posix.waitpid(pid, std.c.W.NOHANG);
        //
        // WS19 T17b: `posix_waitpid_CHECKED`, not the verbatim shim. rustty's
        // xev `.proc` completion is a SECOND reaper, so `ECHILD` is genuinely
        // reachable here and the verbatim shim's `unreachable!` would turn that
        // race into a panic on the teardown path. `None` means "gone" — which is
        // what this loop is waiting for — so break, exactly as upstream head's
        // raw-syscall `killPid` does (`rc == -1` -> `res_pid != 0` -> break).
        // Fidelity against upstream head, NOT a divergence; see the shim's docs
        // and `.port/phase-3/assessments/E-os-termio-platform.md` §5.2.
        let Some(res) = crate::command::posix_waitpid_checked(pid, libc::WNOHANG as u32) else {
            // log.debug("waitpid: child already reaped (ECHILD)", .{});
            break;
        };
        // log.debug("waitpid result={}", .{res.pid});
        log::debug!(target: LOG_SCOPE, "waitpid result={}", res.pid);
        // if (res.pid != 0) break;
        if res.pid != 0 {
            break;
        }

        // [R1-DIVERGENCE] hard deadline — the ONLY divergence in this loop.
        // In the case this exists for the child is unreapable (an `?Es` macOS
        // process is SIGKILL-immune and `waitpid` never returns it), so waiting
        // longer provably cannot succeed. Give up rather than wedge teardown
        // forever; the child is reparented to launchd when we exit. A merely
        // SLOW-but-reapable child is also abandoned here (accepted consequence,
        // see the note above) — but it is never killed, so its cleanup still
        // completes.
        if elapsed >= abandon {
            // log.warn("child pid={} unreapable after {}s, abandoning", ...);
            return Err(KillError::Abandoned);
        }

        // std.Thread.sleep(10 * std.time.ns_per_ms);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}

// fn getpgid(pid: c.pid_t) ?c.pid_t {
#[cfg(not(windows))]
fn getpgid(pid: libc::pid_t) -> Option<libc::pid_t> {
    // Get our process group ID. Before the child pid calls setsid
    // the pgid will be ours because we forked it. Its possible that
    // we may be calling this before setsid if we are killing a surface
    // VERY quickly after starting it.
    // const my_pgid = c.getpgid(0);
    let my_pgid = unsafe { libc::getpgid(0) };

    // We loop while pgid == my_pgid. The expectation if we have a valid
    // pid is that setsid will eventually be called because it is the
    // FIRST thing the child process does and as far as I can tell,
    // setsid cannot fail. I'm sure that's not true, but I'd rather
    // have a bug reported than defensively program against it now.
    // while (true) {
    loop {
        // const pgid = c.getpgid(pid);
        let pgid = unsafe { libc::getpgid(pid) };
        // if (pgid == my_pgid) {
        if pgid == my_pgid {
            // log.warn("pgid is our own, retrying", .{});
            log::warn!(target: LOG_SCOPE, "pgid is our own, retrying");
            std::thread::sleep(std::time::Duration::from_millis(10));
            continue;
        }

        // Don't know why it would be zero but its not a valid pid
        // if (pgid == 0) return null;
        if pgid == 0 {
            return None;
        }

        // If the pid doesn't exist then... we're done!
        // if (pgid == c.ESRCH) return null;
        if pgid == libc::ESRCH {
            return None;
        }

        // If we have an error we're done.
        // if (pgid < 0) {
        //     log.warn("error getting pgid for kill", .{});
        //     return null;
        // }
        if pgid < 0 {
            // log.warn("error getting pgid for kill", .{});
            log::warn!(target: LOG_SCOPE, "error getting pgid for kill");
            return None;
        }

        return Some(pgid);
    }
}

/// Kill the underlying process started via Flatpak host command.
/// This sends a signal via the Flatpak API.
//
// fn killCommandFlatpak(command: *FlatpakHostCommand) !void {
//     try command.signal(c.SIGHUP, true);
// }
//
// PORT NOTE (WS20): filled in. `internal_os.FlatpakHostCommand.signal` is fully
// ported (`rustty-os/src/flatpak.rs`), so this is the reference body verbatim.
// It is `#[cfg(feature = "flatpak")]` for exactly the reason the reference's
// callers are `comptime build_config.flatpak`-gated: with the option off, the
// `FlatpakHostCommand` type is the empty placeholder and has no `signal`.
#[cfg(feature = "flatpak")]
#[allow(dead_code)]
pub(super) fn killCommandFlatpak(command: &mut FlatpakHostCommand) -> Result<(), crate::Error> {
    // try command.signal(c.SIGHUP, true);
    command.signal(SIGHUP as u8, true).map_err(|_| crate::Error)
}

/// PORT NOTE: `c.SIGHUP` — the reference `@cImport`s `signal.h`. Hand-rolled per
/// repo convention (the `libc` crate is not a dependency of this crate).
#[cfg(feature = "flatpak")]
const SIGHUP: i32 = 1;
