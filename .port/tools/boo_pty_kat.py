#!/usr/bin/env python3
"""WS12 T5 — end-to-end pty harness for `rustty +boo`.

WHAT THIS PROVES, and why the obvious harness does not prove it
--------------------------------------------------------------
`+boo` is a full-screen vaxis TUI animation. "It printed some bytes" is NOT evidence it
works, and two specific traps make a naive harness assert nothing:

1. **A pty with no winsize renders NOTHING.** Under `script -q /dev/null` (stdin a pipe,
   so winsize is 0x0) `+boo` emits only the per-frame synchronized-update wrapper —
   `ESC[?2026h ESC[?25l ESC[H ESC[m ESC[?2026l` — 2,640 bytes with zero cell content. A
   harness that asserts "bytes > 0" passes on that. So this harness `ioctl(TIOCSWINSZ)`es
   a real 205x50 window (also above `+boo`'s own 100w x 41h floor, boo.rs:465) and asserts
   on CONTENT: frame count, colour runs, and a cursor address in a far-right column.

2. **ESC quits, but the process then blocks in `Loop::stop()` unless the terminal
   answers DSR.** This looks exactly like "+boo ignores the quit key" and is not: `stop()`
   (`rustty-vaxis/src/loop_.rs:215`) wakes the reader thread — blocked in `read(/dev/tty)`
   — by writing a DSR query `ESC[5n`, then `join()`s it. A dumb pty never answers, the
   read never returns, and `join()` blocks forever. This is UPSTREAM-FAITHFUL libvaxis
   behaviour, not a rustty defect. So this harness acts as a minimal *terminal* and
   answers the queries `+boo` sends, `ESC[5n` -> `ESC[0n` above all.

`--no-dsr` suppresses ONLY the `ESC[5n` reply. That is the negative control: it must NOT
exit, which is what proves the clean exit in the normal arm is caused by the ESC key
being received and honoured, rather than by a timeout, a crash, or a closed pty.

Usage:
    python3 boo_pty_kat.py <path-to-rustty-binary> [--no-dsr] [--quit-after SEC]
                           [--exit-wait SEC] [--cols N] [--rows N]

Machine-readable output (one per line, always printed):
    EXIT_STATUS=<n>      child exit status, or -1 if it never exited
    EXITED=<0|1>
    FRAMES=<n>           count of ESC[?2026h (synchronized-update begins)
    COLOR_RUNS=<n>       count of ESC[34m (the ghost outline's blue SGR run)
    MAX_COL=<n>          largest column seen in any ESC[<row>;<col>H
    BYTES=<n>            total bytes read from the master
    ELAPSED_AFTER_QUIT=<float seconds>

Exit code 0 iff the child exited cleanly (status 0). With --no-dsr the expected and
correct result is a NON-zero exit code from this script, because the child is expected to
still be alive; the caller asserts that.

std-lib only (pty, fcntl, termios, struct, select) — no third-party dependency, so it
runs anywhere the repo does.
"""

import os
import pty
import re
import select
import signal
import struct
import sys
import termios
import fcntl
import time

ESC = b"\x1b"

# The startup query block `+boo` emits, captured verbatim during WS12 planning:
#   ESC[?1049h ESC[?1016$p ESC[?2027$p ESC[?2031$p ESC[?2048h ESC[H ESC]66;w=1; ESC\
#   ESC[6n ESC[H ESC]66;s=2; ESC\ ESC[> q ESC[6n ESC[>0q ESC[?u ESC_Gi=1,a=q ESC\
#   ESC[c ESC[?2004h ESC[?996n ESC[?2031h ESC[?1002;1003;1004;1006h ESC[H ESC[m ESC[J
# Answering the five below is sufficient to get through it and to shut down cleanly.
REPLIES = [
    # (query bytes, reply bytes, is_dsr_wake)
    (b"\x1b[6n", b"\x1b[1;1R", False),        # CPR — cursor position report
    (b"\x1b[c", b"\x1b[?62;22c", False),      # DA1 — primary device attributes
    (b"\x1b[?u", b"\x1b[?0u", False),         # kitty keyboard flags query
    (b"\x1b[?996n", b"\x1b[?997;1n", False),  # colour-scheme query
    (b"\x1b[5n", b"\x1b[0n", True),           # DSR — THE shutdown wake (loop_.rs:215)
]

CURSOR_ADDR = re.compile(rb"\x1b\[(\d+);(\d+)H")


def set_winsize(fd, rows, cols):
    """ioctl(TIOCSWINSZ). Without this the child renders no cell content at all."""
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def main() -> int:
    argv = sys.argv[1:]
    if not argv:
        print("usage: boo_pty_kat.py <rustty-binary> [--no-dsr] ...", file=sys.stderr)
        return 2

    binary = argv[0]
    no_dsr = "--no-dsr" in argv
    quit_after = 3.0
    exit_wait = 2.0
    cols, rows = 205, 50
    for i, a in enumerate(argv):
        if a == "--quit-after":
            quit_after = float(argv[i + 1])
        elif a == "--exit-wait":
            exit_wait = float(argv[i + 1])
        elif a == "--cols":
            cols = int(argv[i + 1])
        elif a == "--rows":
            rows = int(argv[i + 1])

    if not os.path.isfile(binary):
        print(f"FATAL: binary not found: {binary}", file=sys.stderr)
        return 2

    pid, master = pty.fork()
    if pid == 0:
        # Child: a clean, capability-plausible environment. TERM=xterm-256color so the
        # capability probe takes the normal path.
        env = dict(os.environ)
        env["TERM"] = "xterm-256color"
        env.pop("NO_COLOR", None)
        try:
            os.execve(binary, [binary, "+boo"], env)
        finally:
            os._exit(127)

    # Parent: act as the terminal.
    set_winsize(master, rows, cols)

    captured = bytearray()
    pending_scan_from = 0
    sent_quit = False
    quit_at = None
    exited = False
    status = -1
    start = time.monotonic()
    deadline_slack = 1.0

    try:
        while True:
            now = time.monotonic()

            if not sent_quit and now - start >= quit_after:
                try:
                    os.write(master, ESC)
                except OSError:
                    pass
                sent_quit = True
                quit_at = time.monotonic()

            if sent_quit and time.monotonic() - quit_at > exit_wait:
                break

            if not sent_quit and now - start > quit_after + deadline_slack:
                break

            try:
                r, _, _ = select.select([master], [], [], 0.05)
            except (OSError, ValueError):
                break

            if r:
                try:
                    chunk = os.read(master, 65536)
                except OSError:
                    # EIO on macOS == the child closed the slave side: it exited.
                    break
                if not chunk:
                    break
                captured.extend(chunk)

                # Answer any queries in the newly arrived bytes. Scan with a small
                # overlap so a query split across two reads is still matched.
                window = bytes(captured[max(0, pending_scan_from - 8):])
                for query, reply, is_dsr in REPLIES:
                    if query in window:
                        if is_dsr and no_dsr:
                            continue  # the negative-control arm
                        try:
                            os.write(master, reply)
                        except OSError:
                            pass
                pending_scan_from = len(captured)

            # Non-blocking reap.
            try:
                wpid, wstatus = os.waitpid(pid, os.WNOHANG)
            except ChildProcessError:
                exited = True
                break
            if wpid == pid:
                exited = True
                status = os.waitstatus_to_exitcode(wstatus) if hasattr(
                    os, "waitstatus_to_exitcode"
                ) else (wstatus >> 8)
                break

        # Drain whatever is still buffered so the frame counts reflect the whole run.
        drain_until = time.monotonic() + 0.3
        while time.monotonic() < drain_until:
            try:
                r, _, _ = select.select([master], [], [], 0.05)
            except (OSError, ValueError):
                break
            if not r:
                continue
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            captured.extend(chunk)

        # BOUNDED BLOCKING REAP. The normal shutdown signature is master EOF/EIO — the
        # child closed the slave side — which arrives BEFORE the child has been reaped.
        # A single WNOHANG here loses that race and reports EXITED=0 for a process that
        # exited perfectly (observed while writing this harness), so poll for up to
        # `exit_wait` instead of sampling once.
        if not exited:
            reap_deadline = time.monotonic() + exit_wait
            while time.monotonic() < reap_deadline:
                try:
                    wpid, wstatus = os.waitpid(pid, os.WNOHANG)
                except ChildProcessError:
                    break
                if wpid == pid:
                    exited = True
                    status = (
                        os.waitstatus_to_exitcode(wstatus)
                        if hasattr(os, "waitstatus_to_exitcode")
                        else (wstatus >> 8)
                    )
                    break
                time.sleep(0.02)
    finally:
        if not exited:
            # Do NOT leave a full-screen TUI running; SIGKILL is required because the
            # whole point of the negative control is that it is wedged in join().
            try:
                os.kill(pid, signal.SIGKILL)
                os.waitpid(pid, 0)
            except OSError:
                pass
        try:
            os.close(master)
        except OSError:
            pass

    elapsed_after_quit = (time.monotonic() - quit_at) if quit_at else 0.0
    data = bytes(captured)
    frames = data.count(b"\x1b[?2026h")
    color_runs = data.count(b"\x1b[34m")
    max_col = 0
    for m in CURSOR_ADDR.finditer(data):
        col = int(m.group(2))
        if col > max_col:
            max_col = col

    print(f"EXIT_STATUS={status}")
    print(f"EXITED={1 if exited else 0}")
    print(f"FRAMES={frames}")
    print(f"COLOR_RUNS={color_runs}")
    print(f"MAX_COL={max_col}")
    print(f"BYTES={len(data)}")
    print(f"ELAPSED_AFTER_QUIT={elapsed_after_quit:.3f}")

    return 0 if (exited and status == 0) else 1


if __name__ == "__main__":
    sys.exit(main())
