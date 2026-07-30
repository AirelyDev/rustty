#!/usr/bin/env bash
# ws19_wayland_deco_live_gate.sh — WS19 T9 live gate (LINUX / VM ONLY)
#
# Proves `winproto/wayland.rs`'s `Window` honours a decoration-mode change the
# compositor makes AFTER the window exists (audit row O7) — the reference's live
# `globals: *Globals` read (`wayland.zig:222`/`:252`/`:303`), which the port used to
# snapshot at init.
#
# WHY KWIN, AND WHY NESTED.
#   * mutter advertises NO `org_kde_kwin_server_decoration_manager`, so on the primary
#     GNOME session `default_deco_mode` is `None` and the mode can never change — the
#     check would be vacuous there.
#   * sway/wlroots DOES implement the protocol but calls
#     `wlr_server_decoration_manager_set_default_mode` exactly once at startup, so it
#     cannot produce a LIVE change either.
#   * KWin re-evaluates its decoration plugin on `org.kde.KWin.reconfigure` and
#     broadcasts a fresh `default_mode` to already-bound clients. That is the only live
#     source available on this box.
#   * It runs NESTED on its own socket (`wayland-kwinkat`) with a throwaway
#     `XDG_CONFIG_HOME`, so GDM, the default systemd target and the primary tty2 GNOME
#     session are untouched. Two historical hard host crashes correlate with primary-
#     session KAT activity (VERIFICATION_DEBT.md V4); nothing here touches it.
#
# ROOTLESS KWIN. `sudo` on this VM requires a password, so `dnf install kwin` is not
# available to a non-interactive runner. The gate therefore uses a USER-PREFIX kwin
# extracted from downloaded RPMs (no root at any point):
#
#   cd /tmp/kwinrpm && dnf download --disablerepo=cursor --resolve kwin
#   mkdir root && cd root && for f in ../*.rpm; do rpm2cpio "$f" | cpio -idmu --quiet; done
#
# Set KWIN_PREFIX to that `root` directory (default below). The gate FAILS with a clear
# message if it is absent — it never silently degrades to "compositor not available".
#
# Exit 0 = GREEN.
set -uo pipefail

RUSTTY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
KWIN_PREFIX="${KWIN_PREFIX:-/tmp/kwinrpm/root}"
WORK="${TMPDIR:-/tmp}/ws19-t9-gate"
SOCKET="wayland-kwinkat"
KWIN_LOG="$WORK/kwin.log"
TEST_LOG="$WORK/test.log"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }

[ -n "${XDG_RUNTIME_DIR:-}" ] || fail "XDG_RUNTIME_DIR unset"
# The PARENT compositor kwin nests inside. An ssh shell is not a session shell, so this
# is normally unset — and without it kwin silently falls through to the DRM backend and
# exits 1 with an EMPTY log, which reads exactly like "kwin is broken". Named
# explicitly, and asserted, so that failure mode cannot recur.
PARENT_WAYLAND_DISPLAY="${PARENT_WAYLAND_DISPLAY:-wayland-0}"
[ -S "$XDG_RUNTIME_DIR/$PARENT_WAYLAND_DISPLAY" ] \
  || fail "no parent compositor socket at $XDG_RUNTIME_DIR/$PARENT_WAYLAND_DISPLAY (set PARENT_WAYLAND_DISPLAY); kwin cannot nest without one"
[ "$PARENT_WAYLAND_DISPLAY" != "$SOCKET" ] || fail "the parent and nested sockets must differ"
[ -x "$KWIN_PREFIX/usr/bin/kwin_wayland" ] \
  || fail "no kwin_wayland at $KWIN_PREFIX/usr/bin (see the rootless extraction recipe in this file's header)"
command -v dbus-send >/dev/null || fail "dbus-send is required to call org.kde.KWin.reconfigure"

# --- leftover-detection pre-flight -------------------------------------------
# A crashed previous run leaves a stale socket and/or a live kwin. Refuse rather than
# attach to unknown state. NOTE: match the BINARY, never `pgrep -f kwin_wayland`, which
# also matches this script's own argv.
if pgrep -x kwin_wayland >/dev/null 2>&1; then
  fail "a kwin_wayland is already running; stop it first (pkill -x kwin_wayland) — this gate will not attach to state it did not create"
fi
if [ -e "$XDG_RUNTIME_DIR/$SOCKET" ]; then
  fail "stale wayland socket $XDG_RUNTIME_DIR/$SOCKET from a previous run; remove it first"
fi

rm -rf "$WORK"; mkdir -p "$WORK/config"

# A throwaway kwinrc WITH a decoration plugin, so KWin starts in `default_mode=Server`
# and the test's "before" assertion is reachable.
cat > "$WORK/config/kwinrc" <<'KWINRC'
[org.kde.kdecoration2]
library=org.kde.breeze
theme=Breeze
KWINRC

echo "uptime before: $(uptime)"

KWIN_PID=""
cleanup() {
  if [ -n "$KWIN_PID" ]; then
    kill "$KWIN_PID" 2>/dev/null
    for _ in $(seq 1 20); do kill -0 "$KWIN_PID" 2>/dev/null || break; sleep 0.2; done
    kill -9 "$KWIN_PID" 2>/dev/null
  fi
  # Belt and braces: the binary, matched exactly so this script's own argv is immune.
  pkill -x kwin_wayland 2>/dev/null
  rm -f "$XDG_RUNTIME_DIR/$SOCKET" "$XDG_RUNTIME_DIR/$SOCKET.lock"
  return 0
}
# Installed BEFORE the compositor is spawned, and armed for signals as well as exit, so
# no run can leave a compositor behind. (`git worktree remove` does NOT reap spawned
# processes — they reparent to init and run forever.)
trap cleanup EXIT INT TERM HUP

export LD_LIBRARY_PATH="$KWIN_PREFIX/usr/lib64${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export QT_PLUGIN_PATH="$KWIN_PREFIX/usr/lib64/qt6/plugins"
export QML2_IMPORT_PATH="$KWIN_PREFIX/usr/lib64/qt6/qml"
export XDG_DATA_DIRS="$KWIN_PREFIX/usr/share:${XDG_DATA_DIRS:-/usr/share}"

XDG_CONFIG_HOME="$WORK/config" WAYLAND_DISPLAY="$PARENT_WAYLAND_DISPLAY" \
  setsid nohup "$KWIN_PREFIX/usr/bin/kwin_wayland" --socket "$SOCKET" \
  > "$KWIN_LOG" 2>&1 < /dev/null &
KWIN_PID=$!

for _ in $(seq 1 100); do
  [ -S "$XDG_RUNTIME_DIR/$SOCKET" ] && break
  sleep 0.2
done
[ -S "$XDG_RUNTIME_DIR/$SOCKET" ] || { tail -20 "$KWIN_LOG"; fail "nested kwin never created $SOCKET"; }

# READINESS, not merely existence. The socket file appears well BEFORE kwin will accept
# clients, and connecting too early makes `gtk4::init()` fail with a bare
# "Failed to initialize GTK" that looks like a toolkit problem rather than a race.
# Owning `org.kde.KWin` on the session bus happens after kwin's workspace is up, and it
# is also the interface the KAT's reconfigure call needs, so waiting on it covers both.
ready=0
for _ in $(seq 1 150); do
  if ! kill -0 "$KWIN_PID" 2>/dev/null; then
    tail -20 "$KWIN_LOG"
    fail "nested kwin exited during startup (an empty log usually means it could not reach the parent compositor)"
  fi
  if gdbus call --session --dest org.kde.KWin --object-path /KWin \
       --method org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1; then
    ready=1; break
  fi
  sleep 0.2
done
[ "$ready" -eq 1 ] || { tail -20 "$KWIN_LOG"; fail "nested kwin never took org.kde.KWin on the session bus"; }
echo "nested kwin up and ready on $SOCKET (pid $KWIN_PID)"

# --- the KAT ------------------------------------------------------------------
# The test itself edits the throwaway kwinrc and calls org.kde.KWin.reconfigure, so it
# needs the same XDG_CONFIG_HOME. GDK_BACKEND is left unset on purpose (forcing x11
# here would silently select the wrong winproto arm); WAYLAND_DISPLAY selects the
# nested compositor.
#
# The KWin user prefix MUST NOT leak into the test's environment: its `usr/lib64` holds
# a full Qt/KDE stack that shadows system libraries, and inheriting `LD_LIBRARY_PATH`
# made `gtk4::init()` fail outright with "Failed to initialize GTK". Strip the four
# variables the compositor needed.
cd "$RUSTTY_ROOT" || fail "cannot cd to $RUSTTY_ROOT"
env -u LD_LIBRARY_PATH -u QT_PLUGIN_PATH -u QML2_IMPORT_PATH -u XDG_DATA_DIRS \
    WAYLAND_DISPLAY="$SOCKET" \
    XDG_CONFIG_HOME="$WORK/config" \
    LIBGL_ALWAYS_SOFTWARE=1 \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_deco_live \
    -- --ignored --nocapture > "$TEST_LOG" 2>&1
rc=$?

grep -qE "^test result: ok\. 1 passed" "$TEST_LOG" \
  || { tail -30 "$TEST_LOG"; fail "the live decoration-mode KAT did not run exactly one passing test (rc=$rc)"; }
grep -q "KAT PASS" "$TEST_LOG" \
  || { tail -30 "$TEST_LOG"; fail "the KAT did not report PASS"; }

grep "KAT PASS" "$TEST_LOG" | sed 's/^/  /'
echo "uptime after: $(uptime)"
echo
echo "GATE PASS: a live compositor decoration-mode change is honoured by winproto::wayland::Window"
