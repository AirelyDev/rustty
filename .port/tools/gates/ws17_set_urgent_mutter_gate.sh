#!/usr/bin/env bash
# ws17_set_urgent_mutter_gate.sh — WS17 V4 (LINUX / VM ONLY)
#
# Discharges `.port/phase-2/VERIFICATION_DEBT.md` V4: the `set_urgent` activation-token
# dispatch seam had been runtime-verified only on headless sway/wlroots, never on mutter,
# because two hard host crashes correlated with live-compositor KAT activity on the
# PRIMARY GNOME session and the run was deliberately not repeated.
#
# HOW THE CRASH RISK IS REMOVED: this runs against a **real mutter**, but a SECOND,
# HEADLESS instance of it — `mutter --headless --virtual-monitor 1280x720` on its own
# socket (`wayland-mutterkat`). The primary tty2 GNOME session, GDM and the default
# systemd target are never touched, so "the mutter-session run" is achieved without
# touching the session that crashed. It is real mutter, not a stand-in.
#
# WHAT IS ASSERTED. The KAT `wayland_set_urgent_dispatches_activation_token` realizes a
# toplevel, calls `Window::set_urgent(true)`, and requires the compositor's real
# `xdg_activation_token_v1.done` to have dispatched through `WlQueueSource` (observed via
# `App::activation_done_count`). Bounded at ~10 s, so a broken seam FAILS, never hangs.
#
# NEGATIVE CONTROL (run here, and required to fail): the SAME test pointed at a
# WAYLAND_DISPLAY that no compositor is serving. It must fail — without a compositor there
# is nothing to issue a token, so a pass would mean the counter is not actually driven by
# the compositor.
#
# RECOVERY, if the host ever does wedge (the reason this gate exists):
#   On the macOS host, from Terminal or over SSH:
#     osascript -e 'tell application "UTM" to stop virtual machine "Fedora Linux"'
#     osascript -e 'tell application "UTM" to start virtual machine "Fedora Linux"'
#   UTM's AppleScript interface survives an unresponsive guest; a force-stop plus start is
#   preferable to power-cycling the host.
#
# Exit 0 = GREEN.
set -uo pipefail

RUSTTY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORK="${TMPDIR:-/tmp}/ws17-v4-gate"
SOCKET="wayland-mutterkat"
EVIDENCE="$RUSTTY_ROOT/.port/phase-2/evidence/ws17"
DEAD_SOCKET="wayland-ws17-nocompositor"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }

[ -n "${XDG_RUNTIME_DIR:-}" ] || fail "XDG_RUNTIME_DIR unset"
command -v mutter >/dev/null || fail "mutter not installed"

# --- leftover-detection pre-flight -------------------------------------------
# Match the BINARY, never `pgrep -f mutter` — that would match this script's own argv AND
# the primary session's mutter. We must never touch the latter, so the check is scoped to
# our own socket instead of to any running mutter.
if [ -e "$XDG_RUNTIME_DIR/$SOCKET" ]; then
  fail "stale wayland socket $XDG_RUNTIME_DIR/$SOCKET from a previous run; remove it first"
fi

rm -rf "$WORK"; mkdir -p "$WORK" "$EVIDENCE"

UPTIME_BEFORE="$(uptime)"
echo "uptime before: $UPTIME_BEFORE"
echo "mutter: $(mutter --version 2>&1 | head -1)"
echo "primary session left alone: WAYLAND_DISPLAY=wayland-0 is NOT used by this gate"

MUTTER_PID=""
cleanup() {
  if [ -n "$MUTTER_PID" ]; then
    kill "$MUTTER_PID" 2>/dev/null
    for _ in $(seq 1 25); do kill -0 "$MUTTER_PID" 2>/dev/null || break; sleep 0.2; done
    kill -9 "$MUTTER_PID" 2>/dev/null
    wait "$MUTTER_PID" 2>/dev/null
  fi
  rm -f "$XDG_RUNTIME_DIR/$SOCKET" "$XDG_RUNTIME_DIR/$SOCKET.lock"
  return 0
}
# Armed BEFORE anything is spawned and for signals too: `git worktree remove` does NOT
# reap spawned processes — they reparent to init and run forever.
trap cleanup EXIT INT TERM HUP

# A headless mutter with a virtual monitor. `--headless` needs no DRM device and no seat,
# so it cannot contend with the primary session for the display.
setsid nohup mutter --headless --virtual-monitor 1280x720 --wayland-display "$SOCKET" \
  > "$WORK/mutter.log" 2>&1 < /dev/null &
MUTTER_PID=$!

ready=0
for _ in $(seq 1 150); do
  kill -0 "$MUTTER_PID" 2>/dev/null || { tail -20 "$WORK/mutter.log"; fail "headless mutter exited during startup"; }
  [ -S "$XDG_RUNTIME_DIR/$SOCKET" ] && { ready=1; break; }
  sleep 0.2
done
[ "$ready" -eq 1 ] || { tail -20 "$WORK/mutter.log"; fail "headless mutter never created $SOCKET"; }
# The socket appears slightly before clients are served; settle briefly rather than race
# (the KWin gate learned this the hard way — an early connect surfaces as a bare
# "Failed to initialize GTK" that looks like a toolkit fault).
sleep 2
echo "headless mutter up on $SOCKET (pid $MUTTER_PID)"

cd "$RUSTTY_ROOT" || fail "cannot cd to $RUSTTY_ROOT"

# --- arm 1: the KAT against REAL mutter --------------------------------------
echo "== arm 1: set_urgent activation-token dispatch on REAL (headless) mutter =="
env WAYLAND_DISPLAY="$SOCKET" LIBGL_ALWAYS_SOFTWARE=1 \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_set_urgent \
    -- --ignored --exact wayland_set_urgent_dispatches_activation_token --nocapture \
    > "$WORK/mutter-arm.log" 2>&1
arm1=$?
grep -qE "^test result: ok\. 1 passed" "$WORK/mutter-arm.log" \
  || { tail -25 "$WORK/mutter-arm.log"; fail "the set_urgent KAT did not pass on real mutter (rc=$arm1)"; }
echo "  OK"

# --- arm 2: NEGATIVE CONTROL, no compositor ----------------------------------
# Point at a socket nobody serves. GTK cannot even open the display, so the test must
# fail. If it passed, `activation_done_count` would not be compositor-driven.
echo "== arm 2 (negative control): same KAT with NO compositor must FAIL =="
rm -f "$XDG_RUNTIME_DIR/$DEAD_SOCKET"
env WAYLAND_DISPLAY="$DEAD_SOCKET" LIBGL_ALWAYS_SOFTWARE=1 \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_set_urgent \
    -- --ignored --exact wayland_set_urgent_dispatches_activation_token --nocapture \
    > "$WORK/nocompositor-arm.log" 2>&1
arm2=$?
if [ "$arm2" -eq 0 ]; then
  tail -25 "$WORK/nocompositor-arm.log"
  fail "the set_urgent KAT PASSED with no compositor at all — it is not measuring a real activation token"
fi
echo "  OK — it failed as required:"
grep -E "panicked|never dispatched|gtk init|test result:" "$WORK/nocompositor-arm.log" | sed 's/^/    /' | head -4

UPTIME_AFTER="$(uptime)"

{
  echo "WS17 V4 — set_urgent activation-token dispatch against REAL mutter"
  echo "captured: $(date -Is)"
  echo "mutter:   $(mutter --version 2>&1 | head -1)"
  echo "compositor: a SECOND, HEADLESS mutter on socket $SOCKET"
  echo "            (--headless --virtual-monitor 1280x720). The primary tty2 GNOME"
  echo "            session, GDM and the default systemd target were NOT touched."
  echo "uptime before: $UPTIME_BEFORE"
  echo "uptime after : $UPTIME_AFTER"
  echo
  echo "--- arm 1: real headless mutter (must pass) ---"
  grep -E "test result:|activation" "$WORK/mutter-arm.log" | head -5
  echo
  echo "--- arm 2: no compositor, NEGATIVE CONTROL (must fail) ---"
  grep -E "panicked|never dispatched|gtk init|test result:" "$WORK/nocompositor-arm.log" | head -4
  echo
  echo "RESIDUAL, deliberately not attempted: re-running this KAT on the PRIMARY GNOME"
  echo "session. Two hard host crashes correlated with primary-session live-compositor KAT"
  echo "activity (abrupt journal end, no guest OOM/panic — hypervisor-style death, root"
  echo "cause never attributed). Since this run exercises the SAME mutter codebase and the"
  echo "SAME xdg_activation_v1 implementation, the marginal information from repeating it on"
  echo "the crash-correlated session does not justify the risk to a shared dev host."
} > "$EVIDENCE/v4-set-urgent-real-mutter.txt"

echo "uptime after: $UPTIME_AFTER"
echo "evidence: $EVIDENCE/v4-set-urgent-real-mutter.txt"
echo
echo "GATE PASS: set_urgent dispatches a real mutter activation token; no-compositor control fails"
